#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod hot_corner;
mod launcher;
mod media;
mod secrets;
mod volume;

use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    fs,
    sync::{
        atomic::{AtomicBool, AtomicU32, Ordering},
        Mutex,
    },
    time::Duration,
};
use tauri::{
    menu::{CheckMenuItem, MenuBuilder, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    window::{Color, Monitor},
    Emitter, Manager, PhysicalPosition, PhysicalSize, State,
};
use tauri_plugin_autostart::ManagerExt as AutostartManagerExt;
use windows::Win32::{
    Foundation::{HWND, POINT},
    UI::{
        Input::KeyboardAndMouse::{GetAsyncKeyState, VK_LBUTTON},
        WindowsAndMessaging::{
            GetCursorPos, SetForegroundWindow, SetWindowPos, ShowWindow, HWND_NOTOPMOST,
            HWND_TOPMOST, SWP_NOMOVE, SWP_NOSIZE, SWP_SHOWWINDOW, SW_RESTORE,
        },
    },
};

pub const PANEL: (f64, f64) = (320.0, 520.0);
pub const CAPSULE: (f64, f64) = (220.0, 40.0);
pub const EDGE: f64 = 12.0; // 距目标显示器工作区上/右边缘的逻辑像素
pub const SNAP_DISTANCE: f64 = 18.0;

#[derive(Clone, Debug, Serialize, Deserialize)]
struct MonitorPlacement {
    anchor: String, // "left" | "right" | "free"
    center_ratio: f64,
}

impl Default for MonitorPlacement {
    fn default() -> Self {
        Self {
            anchor: "right".into(),
            center_ratio: 1.0,
        }
    }
}

#[derive(Default)]
pub struct AppState {
    mode: Mutex<String>, // "panel" | "capsule" | "hidden"
    autostart_item: Mutex<Option<CheckMenuItem<tauri::Wry>>>,
    global_shortcut_enabled: Mutex<bool>,
    placements: Mutex<HashMap<String, MonitorPlacement>>,
    window_dragging: AtomicBool,
    pinned: AtomicBool,
    top_trigger_width: AtomicU32,
    top_trigger_dwell_ms: AtomicU32,
}

fn current_mode(state: &State<AppState>) -> String {
    state.mode.lock().unwrap().clone()
}

fn monitor_key(monitor: &Monitor) -> String {
    monitor.name().cloned().unwrap_or_else(|| {
        let position = monitor.position();
        let size = monitor.size();
        format!(
            "{}:{}:{}x{}",
            position.x, position.y, size.width, size.height
        )
    })
}

fn placement_file(app: &tauri::AppHandle) -> Option<std::path::PathBuf> {
    app.path()
        .app_data_dir()
        .ok()
        .map(|directory| directory.join("window-placement.json"))
}

fn load_placements(app: &tauri::AppHandle) -> HashMap<String, MonitorPlacement> {
    let Some(path) = placement_file(app) else {
        return HashMap::new();
    };
    fs::read_to_string(path)
        .ok()
        .and_then(|content| serde_json::from_str(&content).ok())
        .unwrap_or_default()
}

fn save_placements(
    app: &tauri::AppHandle,
    placements: &HashMap<String, MonitorPlacement>,
) -> Result<(), String> {
    let Some(path) = placement_file(app) else {
        return Err("无法确定窗口位置配置路径".into());
    };
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("创建配置目录失败：{e}"))?;
    }
    let content =
        serde_json::to_string_pretty(placements).map_err(|e| format!("序列化窗口位置失败：{e}"))?;
    fs::write(path, content).map_err(|e| format!("保存窗口位置失败：{e}"))
}

pub(crate) fn monitor_at_point(
    monitors: &[Monitor],
    point: PhysicalPosition<i32>,
) -> Option<Monitor> {
    monitors
        .iter()
        .find(|monitor| {
            let position = monitor.position();
            let size = monitor.size();
            point.x >= position.x
                && point.x < position.x + size.width as i32
                && point.y >= position.y
                && point.y < position.y + size.height as i32
        })
        .cloned()
}

fn placement_for_monitor(state: &State<AppState>, monitor: &Monitor) -> MonitorPlacement {
    state
        .placements
        .lock()
        .unwrap()
        .get(&monitor_key(monitor))
        .cloned()
        .unwrap_or_default()
}

fn geometry_for_placement(
    logical_size: (f64, f64),
    monitor: &Monitor,
    placement: &MonitorPlacement,
) -> (PhysicalSize<u32>, PhysicalPosition<i32>) {
    let work = monitor.work_area();
    placement_geometry(
        logical_size,
        monitor.scale_factor(),
        work.position,
        work.size,
        placement,
    )
}

fn placement_geometry(
    logical_size: (f64, f64),
    scale: f64,
    work_position: PhysicalPosition<i32>,
    work_size: PhysicalSize<u32>,
    placement: &MonitorPlacement,
) -> (PhysicalSize<u32>, PhysicalPosition<i32>) {
    let width = (logical_size.0 * scale).round().max(1.0) as u32;
    let height = (logical_size.1 * scale).round().max(1.0) as u32;
    let edge = (EDGE * scale).round() as i32;
    let left = work_position.x + edge;
    let right = (work_position.x + work_size.width as i32 - width as i32 - edge).max(left);
    let x = match placement.anchor.as_str() {
        "left" => left,
        "free" => {
            let center = work_position.x as f64
                + work_size.width as f64 * placement.center_ratio.clamp(0.0, 1.0);
            (center.round() as i32 - width as i32 / 2).clamp(left, right)
        }
        _ => right,
    };
    (
        PhysicalSize::new(width, height),
        PhysicalPosition::new(x, work_position.y + edge),
    )
}

pub(crate) fn remembered_geometry(
    logical_size: (f64, f64),
    state: &State<AppState>,
    monitor: &Monitor,
) -> (PhysicalSize<u32>, PhysicalPosition<i32>) {
    geometry_for_placement(
        logical_size,
        monitor,
        &placement_for_monitor(state, monitor),
    )
}

/// 按鼠标所在显示器的 DPI 设置物理尺寸，并恢复该显示器记忆的顶部位置。
fn resize_and_dock(
    app: &tauri::AppHandle,
    state: &State<AppState>,
    logical_size: (f64, f64),
) -> Result<(), String> {
    let Some(win) = app.get_webview_window("main") else {
        return Err("no main window".into());
    };
    let monitors = win.available_monitors().unwrap_or_default();
    let cur = app.cursor_position().ok();
    let monitor = cur
        .and_then(|p| monitor_at_point(&monitors, PhysicalPosition::new(p.x as i32, p.y as i32)))
        .or_else(|| win.primary_monitor().ok().flatten());
    let Some(m) = monitor else {
        return Err("未找到可用显示器".into());
    };
    let (size, position) = remembered_geometry(logical_size, state, &m);

    // 先移动到目标屏，让 Windows 完成跨屏 DPI 切换，再固定最终物理尺寸和位置。
    win.set_position(position).map_err(|e| e.to_string())?;
    win.set_size(size).map_err(|e| e.to_string())?;
    win.set_position(position).map_err(|e| e.to_string())?;
    Ok(())
}

fn show_on_top(win: &tauri::WebviewWindow, pinned: bool) -> Result<(), String> {
    win.set_always_on_top(pinned).map_err(|e| e.to_string())?;
    win.show().map_err(|e| e.to_string())?;
    let hwnd = win.hwnd().map_err(|e| e.to_string())?;
    unsafe {
        let hwnd = HWND(hwnd.0);
        let _ = ShowWindow(hwnd, SW_RESTORE);
        SetWindowPos(
            hwnd,
            if pinned { HWND_TOPMOST } else { HWND_NOTOPMOST },
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_SHOWWINDOW,
        )
        .map_err(|e| e.to_string())?;
        let _ = SetForegroundWindow(hwnd);
    }
    win.set_focus().map_err(|e| e.to_string())?;
    Ok(())
}

pub fn apply_mode(
    app: &tauri::AppHandle,
    state: &State<AppState>,
    mode: &str,
) -> Result<(), String> {
    let win = app.get_webview_window("main").ok_or("no main window")?;
    match mode {
        "panel" => {
            resize_and_dock(app, state, PANEL)?;
            show_on_top(&win, state.pinned.load(Ordering::Acquire))?;
        }
        "capsule" => {
            resize_and_dock(app, state, CAPSULE)?;
            show_on_top(&win, state.pinned.load(Ordering::Acquire))?;
        }
        "hidden" => {
            win.hide().map_err(|e| e.to_string())?;
        }
        _ => return Err(format!("unknown mode: {mode}")),
    }
    *state.mode.lock().unwrap() = mode.to_string();
    let _ = app.emit("dban://mode", mode);
    Ok(())
}

pub fn toggle_mode(app: &tauri::AppHandle, state: &State<AppState>) {
    let next = if current_mode(state) == "panel" {
        "hidden"
    } else {
        "panel"
    };
    let _ = apply_mode(app, state, next);
}

#[tauri::command]
fn set_mode_command(
    app: tauri::AppHandle,
    state: State<AppState>,
    mode: String,
) -> Result<(), String> {
    apply_mode(&app, &state, &mode)
}

#[tauri::command]
fn get_mode_command(state: State<AppState>) -> String {
    current_mode(&state)
}

#[tauri::command]
fn set_pinned_command(
    app: tauri::AppHandle,
    state: State<AppState>,
    pinned: bool,
) -> Result<(), String> {
    let w = app.get_webview_window("main").ok_or("no main window")?;
    w.set_always_on_top(pinned).map_err(|e| e.to_string())?;
    state.pinned.store(pinned, Ordering::Release);
    Ok(())
}

fn track_window_drag(app: tauri::AppHandle) {
    let Some(win) = app.get_webview_window("main") else {
        app.state::<AppState>()
            .window_dragging
            .store(false, Ordering::Release);
        return;
    };
    let mut cursor = POINT::default();
    unsafe {
        let _ = GetCursorPos(&mut cursor);
    }
    let window_position = win
        .outer_position()
        .unwrap_or(PhysicalPosition::new(cursor.x, cursor.y));
    let window_width = win
        .outer_size()
        .map(|size| size.width)
        .unwrap_or(PANEL.0 as u32)
        .max(1);
    let cursor_offset_ratio =
        ((cursor.x - window_position.x) as f64 / window_width as f64).clamp(0.0, 1.0);
    let mut last_placement: Option<(String, MonitorPlacement)> = None;

    while unsafe { GetAsyncKeyState(VK_LBUTTON.0 as i32) } < 0 {
        unsafe {
            let _ = GetCursorPos(&mut cursor);
        }
        let monitors = win.available_monitors().unwrap_or_default();
        let Some(monitor) = monitor_at_point(&monitors, PhysicalPosition::new(cursor.x, cursor.y))
        else {
            std::thread::sleep(Duration::from_millis(16));
            continue;
        };
        let work = monitor.work_area();
        let scale = monitor.scale_factor();
        let width = (PANEL.0 * scale).round().max(1.0) as u32;
        let height = (PANEL.1 * scale).round().max(1.0) as u32;
        let edge = (EDGE * scale).round() as i32;
        let snap = (SNAP_DISTANCE * scale).round() as i32;
        let left = work.position.x + edge;
        let right = (work.position.x + work.size.width as i32 - width as i32 - edge).max(left);
        let proposed_x = cursor.x - (width as f64 * cursor_offset_ratio).round() as i32;
        let screen_left = monitor.position().x;
        let screen_right = monitor.position().x + monitor.size().width as i32;
        let (x, anchor) = if cursor.x <= screen_left + snap || proposed_x <= left + snap {
            (left, "left")
        } else if cursor.x >= screen_right - snap || proposed_x >= right - snap {
            (right, "right")
        } else {
            (proposed_x.clamp(left, right), "free")
        };
        let center_ratio = ((x as f64 + width as f64 / 2.0 - work.position.x as f64)
            / work.size.width as f64)
            .clamp(0.0, 1.0);
        let position = PhysicalPosition::new(x, work.position.y + edge);
        let _ = win.set_position(position);
        let _ = win.set_size(PhysicalSize::new(width, height));
        let _ = win.set_position(position);
        last_placement = Some((
            monitor_key(&monitor),
            MonitorPlacement {
                anchor: anchor.into(),
                center_ratio,
            },
        ));
        std::thread::sleep(Duration::from_millis(16));
    }

    let state = app.state::<AppState>();
    if let Some((key, placement)) = last_placement {
        let mut placements = state.placements.lock().unwrap();
        placements.insert(key, placement);
        if let Err(e) = save_placements(&app, &placements) {
            eprintln!("{e}");
        }
    }
    state.window_dragging.store(false, Ordering::Release);
}

#[tauri::command]
fn start_window_drag_command(app: tauri::AppHandle, state: State<AppState>) -> Result<(), String> {
    if current_mode(&state) != "panel" {
        return Ok(());
    }
    if state.window_dragging.swap(true, Ordering::AcqRel) {
        return Ok(());
    }
    std::thread::spawn(move || track_window_drag(app));
    Ok(())
}

fn sync_autostart_state(app: &tauri::AppHandle, state: &State<AppState>, enabled: bool) {
    if let Some(item) = state.autostart_item.lock().unwrap().as_ref() {
        let _ = item.set_checked(enabled);
    }
    let _ = app.emit("dban://autostart", enabled);
}

#[tauri::command]
fn get_autostart_command(app: tauri::AppHandle, state: State<AppState>) -> bool {
    let enabled = app.autolaunch().is_enabled().unwrap_or(false);
    sync_autostart_state(&app, &state, enabled);
    enabled
}

#[tauri::command]
fn set_autostart_command(
    app: tauri::AppHandle,
    state: State<AppState>,
    enabled: bool,
) -> Result<bool, String> {
    let autolaunch = app.autolaunch();
    if enabled {
        autolaunch.enable().map_err(|e| e.to_string())?;
    } else {
        autolaunch.disable().map_err(|e| e.to_string())?;
    }
    let actual = autolaunch.is_enabled().map_err(|e| e.to_string())?;
    sync_autostart_state(&app, &state, actual);
    Ok(actual)
}

#[tauri::command]
fn set_global_shortcut_enabled_command(
    app: tauri::AppHandle,
    state: State<AppState>,
    enabled: bool,
) -> Result<bool, String> {
    use tauri_plugin_global_shortcut::GlobalShortcutExt;

    let current = *state.global_shortcut_enabled.lock().unwrap();
    if current == enabled {
        return Ok(current);
    }

    let shortcut: tauri_plugin_global_shortcut::Shortcut = "alt+d"
        .parse()
        .map_err(|e| format!("快捷键格式错误：{e}"))?;
    if enabled {
        app.global_shortcut()
            .register(shortcut)
            .map_err(|e| format!("Alt+D 注册失败（可能已被其他程序占用）：{e}"))?;
    } else {
        app.global_shortcut()
            .unregister(shortcut)
            .map_err(|e| format!("Alt+D 注销失败：{e}"))?;
    }
    *state.global_shortcut_enabled.lock().unwrap() = enabled;
    Ok(enabled)
}

#[tauri::command]
fn set_top_trigger_settings_command(
    state: State<AppState>,
    width: u32,
    dwell_ms: u32,
) -> (u32, u32) {
    let width = width.clamp(160, 800);
    let dwell_ms = dwell_ms.clamp(100, 1000);
    state.top_trigger_width.store(width, Ordering::Release);
    state
        .top_trigger_dwell_ms
        .store(dwell_ms, Ordering::Release);
    (width, dwell_ms)
}

/// 胶囊模式下点击箭头展开或收起待办浮窗。
/// 仅在 capsule 模式生效；展开高度由前端按待办数量计算后传入。
#[tauri::command]
fn set_capsule_expanded_command(
    app: tauri::AppHandle,
    state: State<AppState>,
    expanded: bool,
    height: f64,
) -> Result<(), String> {
    if current_mode(&state) != "capsule" {
        return Ok(());
    }
    if app.get_webview_window("main").is_none() {
        return Err("no main window".into());
    }
    let size = if expanded {
        (CAPSULE.0, height.clamp(86.0, 560.0))
    } else {
        CAPSULE
    };
    resize_and_dock(&app, &state, size)
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            let state = app.state::<AppState>();
            if let Err(e) = apply_mode(app, &state, "panel") {
                eprintln!("第二实例呼出窗口失败：{e}");
            }
        }))
        .plugin(tauri_plugin_store::Builder::new().build())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, _shortcut, event| {
                    if event.state == tauri_plugin_global_shortcut::ShortcutState::Pressed {
                        toggle_mode(app, &app.state::<AppState>());
                    }
                })
                .build(),
        )
        .manage(AppState {
            mode: Mutex::new("hidden".into()),
            autostart_item: Mutex::new(None),
            global_shortcut_enabled: Mutex::new(false),
            placements: Mutex::new(HashMap::new()),
            window_dragging: AtomicBool::new(false),
            pinned: AtomicBool::new(true),
            top_trigger_width: AtomicU32::new(360),
            top_trigger_dwell_ms: AtomicU32::new(250),
        })
        .invoke_handler(tauri::generate_handler![
            set_mode_command,
            get_mode_command,
            set_pinned_command,
            start_window_drag_command,
            get_autostart_command,
            set_autostart_command,
            set_global_shortcut_enabled_command,
            set_top_trigger_settings_command,
            set_capsule_expanded_command,
            media::media_toggle,
            media::media_next,
            media::media_prev,
            media::media_set_mode,
            secrets::save_secret,
            secrets::get_secret,
            secrets::delete_secret,
            secrets::copy_secret,
            secrets::create_vault_entry,
            secrets::remove_vault_entry,
            launcher::launch_app,
            launcher::add_apps,
            volume::get_system_volume,
            volume::set_system_volume,
        ])
        .setup(|app| {
            use tauri_plugin_global_shortcut::GlobalShortcutExt;

            *app.state::<AppState>().placements.lock().unwrap() = load_placements(app.handle());

            // ---- 托盘 ----
            let show_i = MenuItem::with_id(app, "show", "显示 / 隐藏", true, None::<&str>)?;
            let auto_i =
                CheckMenuItem::with_id(app, "autostart", "开机自启", true, false, None::<&str>)?;
            let quit_i = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
            let menu = MenuBuilder::new(app)
                .items(&[&show_i, &auto_i, &quit_i])
                .build()?;
            let auto_ref = auto_i.clone();
            *app.state::<AppState>().autostart_item.lock().unwrap() = Some(auto_i.clone());

            TrayIconBuilder::with_id("main")
                .icon(app.default_window_icon().expect("missing icon").clone())
                .tooltip("DBan")
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        let app = tray.app_handle();
                        toggle_mode(app, &app.state::<AppState>());
                    }
                })
                .build(app)?;

            let handle = app.handle().clone();
            app.on_menu_event(move |app, event| match event.id().as_ref() {
                "show" => toggle_mode(app, &app.state::<AppState>()),
                "quit" => app.exit(0),
                "autostart" => {
                    let state = app.state::<AppState>();
                    let enabled = app.autolaunch().is_enabled().unwrap_or(false);
                    if set_autostart_command(app.clone(), state, !enabled).is_err() {
                        let _ = auto_ref.set_checked(enabled);
                    }
                }
                _ => {}
            });
            let _ = handle; // handle 保留给闭包外的潜在用途

            // 同步开机自启的初始勾选状态
            let _ = auto_i.set_checked(app.autolaunch().is_enabled().unwrap_or(false));

            // ---- 全局快捷键 Alt+D：呼出 / 隐藏 ----
            let sc: tauri_plugin_global_shortcut::Shortcut = "alt+d".parse().expect("bad shortcut");
            if let Err(e) = app.global_shortcut().register(sc) {
                eprintln!("Alt+D 全局快捷键注册失败（可能被其他程序占用）: {e}");
            } else {
                *app.state::<AppState>()
                    .global_shortcut_enabled
                    .lock()
                    .unwrap() = true;
            }

            // ---- 窗口事件：点关闭 = 隐藏到托盘 ----
            let h = app.handle().clone();
            let win = app.get_webview_window("main").expect("no main window");
            let _ = win.set_background_color(Some(Color(0, 0, 0, 0)));
            win.on_window_event(move |event| {
                if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                    api.prevent_close();
                    let _ = apply_mode(&h, &h.state::<AppState>(), "hidden");
                }
            });

            // ---- 启动即恢复鼠标所在显示器顶部的记忆位置 ----
            apply_mode(app.handle(), &app.state::<AppState>(), "panel")?;

            // ---- 热角监听 + SMTC 媒体轮询 ----
            hot_corner::spawn(app.handle().clone());
            media::spawn_watcher(app.handle().clone());

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running DBan");
}

#[cfg(test)]
mod tests {
    use super::{placement_geometry, MonitorPlacement, CAPSULE, PANEL};
    use tauri::{PhysicalPosition, PhysicalSize};

    #[test]
    fn docks_to_negative_origin_monitor_at_100_percent() {
        let (size, position) = placement_geometry(
            CAPSULE,
            1.0,
            PhysicalPosition::new(-1920, 0),
            PhysicalSize::new(1920, 1040),
            &MonitorPlacement::default(),
        );

        assert_eq!(size, PhysicalSize::new(220, 40));
        assert_eq!(position, PhysicalPosition::new(-232, 12));
    }

    #[test]
    fn scales_size_and_edge_for_150_percent_monitor() {
        let (size, position) = placement_geometry(
            PANEL,
            1.5,
            PhysicalPosition::new(1920, -180),
            PhysicalSize::new(2560, 1400),
            &MonitorPlacement::default(),
        );

        assert_eq!(size, PhysicalSize::new(480, 780));
        assert_eq!(position, PhysicalPosition::new(3982, -162));
    }

    #[test]
    fn clamps_free_placement_inside_monitor_work_area() {
        let (size, position) = placement_geometry(
            PANEL,
            1.0,
            PhysicalPosition::new(0, 0),
            PhysicalSize::new(1920, 1040),
            &MonitorPlacement {
                anchor: "free".into(),
                center_ratio: 0.5,
            },
        );

        assert_eq!(size, PhysicalSize::new(320, 520));
        assert_eq!(position.x, 800);
    }
}
