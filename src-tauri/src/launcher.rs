//! 快捷启动：使用 Windows Shell 直接打开 .exe/.lnk；
//! 图标用 PowerShell System.Drawing 提取为 PNG 并缓存，返回 base64 data URL。
use base64::Engine;
use serde::Serialize;
use std::hash::{Hash, Hasher};
use std::os::windows::ffi::OsStrExt;
use std::os::windows::process::CommandExt;
use tauri::{AppHandle, Manager};
use windows::core::PCWSTR;
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[derive(Serialize, Clone)]
pub struct AppEntry {
    pub id: String,
    pub name: String,
    pub path: String,
    pub icon: String, // data:image/png;base64,... 可为空（前端显示首字符头像）
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DragPathInfo {
    pub is_file: bool,
    pub is_directory: bool,
    pub can_add_app: bool,
}

fn is_launcher_path(path: &std::path::Path) -> bool {
    path.extension()
        .and_then(|value| value.to_str())
        .is_some_and(|extension| {
            extension.eq_ignore_ascii_case("exe") || extension.eq_ignore_ascii_case("lnk")
        })
}

fn wide_null(value: &std::ffi::OsStr) -> Vec<u16> {
    value.encode_wide().chain(std::iter::once(0)).collect()
}

#[tauri::command]
pub fn inspect_drag_path(path: String) -> DragPathInfo {
    let path = std::path::Path::new(&path);
    let metadata = std::fs::metadata(path).ok();
    let is_file = metadata.as_ref().is_some_and(|value| value.is_file());
    let is_directory = metadata.as_ref().is_some_and(|value| value.is_dir());
    DragPathInfo {
        is_file,
        is_directory,
        can_add_app: is_file && is_launcher_path(path),
    }
}

#[tauri::command]
pub fn launch_app(path: String, file_path: Option<String>) -> Result<(), String> {
    let path = std::path::Path::new(&path);
    if !path.is_file() {
        return Err("应用路径不存在".into());
    }
    if !is_launcher_path(path) {
        return Err("只允许启动 .exe 或 .lnk 文件".into());
    }

    let wide = wide_null(path.as_os_str());
    let parameters = if let Some(file_path) = file_path {
        let file = std::path::Path::new(&file_path);
        if !file.is_file() {
            return Err("文件不存在或不可访问".into());
        }
        // Windows 文件名不能包含双引号；包裹为一个参数即可保留路径中的空格。
        Some(wide_null(std::ffi::OsStr::new(&format!("\"{file_path}\""))))
    } else {
        None
    };
    let parameter_ptr = parameters
        .as_ref()
        .map_or(PCWSTR::null(), |value| PCWSTR(value.as_ptr()));
    let result = unsafe {
        ShellExecuteW(
            HWND::default(),
            PCWSTR::null(),
            PCWSTR(wide.as_ptr()),
            parameter_ptr,
            PCWSTR::null(),
            SW_SHOWNORMAL,
        )
    };
    if result.0 as isize <= 32 {
        Err(format!(
            "Windows 无法启动该应用（错误码 {}）",
            result.0 as isize
        ))
    } else {
        Ok(())
    }
}

fn ps_escape(s: &str) -> String {
    s.replace('\'', "''")
}

fn hash_path(path: &str) -> String {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    path.hash(&mut h);
    format!("{:016x}", h.finish())
}

fn extract_icon_b64(app: &AppHandle, path: &str) -> Option<String> {
    let dir = app.path().app_data_dir().ok()?.join("icons");
    std::fs::create_dir_all(&dir).ok()?;
    let out = dir.join(format!("{}-j.png", hash_path(path)));

    if !out.exists() {
        // PrivateExtractIcons 优先取 256px（jumbo）图标，逐级回退；
        // ExtractAssociatedIcon 只有 32px，放大后模糊
        let script = format!(
            r#"$ErrorActionPreference='SilentlyContinue'
$p='{}'
if ([System.IO.Path]::GetExtension($p).ToLower() -eq '.lnk') {{
  $ws = New-Object -ComObject WScript.Shell
  $sc = $ws.CreateShortcut($p)
  if ($sc -and $sc.TargetPath) {{ $p = $sc.TargetPath }}
}}
Add-Type -AssemblyName System.Drawing
$def = @'
using System;
using System.Runtime.InteropServices;
public class DBanIcon {{
  [DllImport("user32.dll", CharSet=CharSet.Unicode)]
  public static extern uint PrivateExtractIcons(string lpszFile, int nIconIndex, int cxIcon, int cyIcon, IntPtr[] phicon, int[] piconid, uint nIcons, uint flags);
  [DllImport("user32.dll")] public static extern bool DestroyIcon(IntPtr hIcon);
}}
'@
Add-Type -TypeDefinition $def
foreach ($s in 256,96,48,32) {{
  $h = New-Object 'IntPtr[]' 1
  $id = New-Object 'int[]' 1
  $n = [DBanIcon]::PrivateExtractIcons($p, 0, $s, $s, $h, $id, 1, 0)
  if ($n -gt 0 -and $h[0] -ne [IntPtr]::Zero) {{
    $i = [System.Drawing.Icon]::FromHandle($h[0])
    $i.ToBitmap().Save('{}', [System.Drawing.Imaging.ImageFormat]::Png)
    [DBanIcon]::DestroyIcon($h[0]) | Out-Null
    break
  }}
}}"#,
            ps_escape(path),
            ps_escape(&out.to_string_lossy()),
        );
        let _ = std::process::Command::new("powershell")
            .args(["-NoProfile", "-NonInteractive", "-Command", &script])
            .creation_flags(CREATE_NO_WINDOW)
            .output();
    }

    let bytes = std::fs::read(&out).ok()?;
    Some(format!(
        "data:image/png;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes)
    ))
}

#[tauri::command]
pub fn add_apps(app: AppHandle, paths: Vec<String>) -> Vec<AppEntry> {
    paths
        .iter()
        .filter(|p| {
            let path = std::path::Path::new(p);
            path.is_file() && is_launcher_path(path)
        })
        .map(|p| AppEntry {
            id: format!("app-{}", hash_path(p)),
            name: std::path::Path::new(p)
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_else(|| "应用".into()),
            path: p.clone(),
            icon: extract_icon_b64(&app, p).unwrap_or_default(),
        })
        .collect()
}
