//! 账号密码：密码本体存 Windows 凭据管理器（DPAPI，按用户加密），
//! 前端只持有条目元信息。复制走剪贴板并 30 秒后自动清空。
use keyring::Entry;
use serde::{Deserialize, Serialize};
use tauri::AppHandle;
use tauri_plugin_clipboard_manager::ClipboardExt;
use tauri_plugin_store::StoreExt;

const SERVICE: &str = "DBan";

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VaultItem {
    id: String,
    site: String,
    account: String,
}

fn vault_store(
    app: &AppHandle,
) -> Result<std::sync::Arc<tauri_plugin_store::Store<tauri::Wry>>, String> {
    app.store("dban.json").map_err(|e| e.to_string())
}

fn load_vaults(app: &AppHandle) -> Result<Vec<VaultItem>, String> {
    let store = vault_store(app)?;
    match store.get("vaults") {
        Some(value) => {
            serde_json::from_value(value).map_err(|e| format!("读取密码元数据失败：{e}"))
        }
        None => Ok(Vec::new()),
    }
}

fn save_vaults(app: &AppHandle, vaults: &[VaultItem]) -> Result<(), String> {
    let store = vault_store(app)?;
    store.set(
        "vaults",
        serde_json::to_value(vaults).map_err(|e| e.to_string())?,
    );
    store.save().map_err(|e| format!("保存密码元数据失败：{e}"))
}

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

#[tauri::command]
pub fn create_vault_entry(
    app: AppHandle,
    id: String,
    site: String,
    account: String,
    secret: String,
) -> Result<VaultItem, String> {
    let item = VaultItem {
        id: id.clone(),
        site,
        account,
    };
    let mut vaults = load_vaults(&app)?;
    if vaults.iter().any(|current| current.id == id) {
        return Err("密码条目已存在".into());
    }
    save_secret(id.clone(), secret)?;
    vaults.push(item.clone());
    if let Err(e) = save_vaults(&app, &vaults) {
        let _ = delete_secret(id);
        return Err(e);
    }
    Ok(item)
}

#[tauri::command]
pub fn remove_vault_entry(app: AppHandle, id: String) -> Result<(), String> {
    let old_secret = get_secret(id.clone())?;
    let mut vaults = load_vaults(&app)?;
    let previous = vaults.clone();
    vaults.retain(|item| item.id != id);
    delete_secret(id.clone())?;
    if let Err(e) = save_vaults(&app, &vaults) {
        if let Some(secret) = old_secret {
            let _ = save_secret(id, secret);
        }
        let _ = save_vaults(&app, &previous);
        return Err(e);
    }
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
