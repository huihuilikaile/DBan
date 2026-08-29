//! 账号密码：密码本体存 Windows 凭据管理器（DPAPI，按用户加密），
//! 前端只持有条目元信息。复制走剪贴板并 30 秒后自动清空。
use keyring::Entry;
use tauri::AppHandle;
use tauri_plugin_clipboard_manager::ClipboardExt;

const SERVICE: &str = "DBan";

fn entry(id: &str) -> Result<Entry, String> {
    Entry::new(SERVICE, id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn save_secret(id: String, secret: String) -> Result<(), String> {
    let credential = entry(&id)?;
    credential
        .set_password(&secret)
        .map_err(|e| format!("写入 Windows 凭据管理器失败：{e}"))?;
    let stored = match credential.get_password() {
        Ok(value) => value,
        Err(e) => {
            let _ = credential.delete_credential();
            return Err(format!("密码已写入但回读失败：{e}"));
        }
    };
    if stored != secret {
        let _ = credential.delete_credential();
        return Err("密码写入校验失败".into());
    }
    Ok(())
}

#[tauri::command]
pub fn get_secret(id: String) -> Result<Option<String>, String> {
    match entry(&id)?.get_password() {
        Ok(p) => Ok(Some(p)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(format!("读取 Windows 凭据管理器失败：{e}")),
    }
}

#[tauri::command]
pub fn delete_secret(id: String) -> Result<(), String> {
    match entry(&id)?.delete_credential() {
        Ok(()) => Ok(()),
        Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(format!("删除 Windows 凭据失败：{e}")),
    }
}

#[tauri::command]
pub fn copy_secret(app: AppHandle, id: String) -> Result<(), String> {
    let secret = get_secret(id)?.ok_or("条目不存在")?;
    app.clipboard()
        .write_text(secret.clone())
        .map_err(|e| e.to_string())?;
    let h = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_secs(30));
        if matches!(h.clipboard().read_text(), Ok(current) if current == secret) {
            let _ = h.clipboard().write_text(String::new());
        }
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{delete_secret, get_secret, save_secret};

    #[test]
    #[ignore = "requires an interactive Windows user logon session"]
    fn windows_credential_round_trip() {
        let id = format!(
            "dban-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let secret = "DBan credential round-trip test".to_string();

        save_secret(id.clone(), secret.clone()).expect("save credential");
        assert_eq!(
            get_secret(id.clone()).expect("read credential"),
            Some(secret)
        );
        delete_secret(id.clone()).expect("delete credential");
        assert_eq!(get_secret(id).expect("verify deletion"), None);
    }
}
