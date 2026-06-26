//! Profile CRUD passthrough — thin wrappers around profile_cmd.
//! Also includes config backup/restore.

use crate::profile_cmd;
use tauri::command;

// ── Config backup ──

fn config_dir() -> std::path::PathBuf {
    kn_common::path::config_dir()
}
fn config_file() -> std::path::PathBuf {
    config_dir().join("config.yaml")
}
fn backup_file() -> std::path::PathBuf {
    config_dir().join("config.yaml.bak")
}

#[command]
pub fn config_backup_exists() -> bool {
    backup_file().exists()
}

#[command]
pub fn backup_config() -> Result<String, String> {
    let cfg = config_file();
    let bak = backup_file();
    if !cfg.exists() {
        return Err("配置文件不存在".into());
    }
    std::fs::copy(&cfg, &bak).map_err(|e| format!("备份失败: {}", e))?;
    Ok("配置已备份".into())
}

#[command]
pub fn restore_config_backup() -> Result<String, String> {
    let bak = backup_file();
    let cfg = config_file();
    if !bak.exists() {
        return Err("备份文件不存在".into());
    }
    if cfg.exists() {
        let pre_restore = config_dir().join("config.yaml.pre-restore");
        std::fs::copy(&cfg, &pre_restore).map_err(|e| format!("无法创建恢复前备份: {}", e))?;
    }
    std::fs::copy(&bak, &cfg).map_err(|e| format!("恢复失败: {}", e))?;
    Ok("配置已从备份恢复".into())
}

#[command]
pub fn batch_export_profiles(names: Vec<String>) -> Result<String, String> {
    let mut results: Vec<serde_json::Value> = Vec::new();
    for name in &names {
        let detail = profile_cmd::show_profile_cmd(name)?;
        results.push(serde_json::json!({
            "name": detail.name,
            "desc": detail.desc,
            "env": detail.env,
        }));
    }
    serde_json::to_string_pretty(&results).map_err(|e| format!("JSON 序列化失败: {}", e))
}

#[command]
pub fn batch_delete_profiles(names: Vec<String>) -> Result<Vec<String>, String> {
    let mut deleted: Vec<String> = Vec::new();
    for name in &names {
        match profile_cmd::remove_profile_cmd(name) {
            Ok(r) if r.ok => { deleted.push(name.clone()); }
            Ok(_) => {}
            Err(e) => return Err(format!("删除 '{}' 失败: {}", name, e)),
        }
    }
    Ok(deleted)
}

// ── Profile CRUD ──

#[command]
pub fn list_profiles() -> Result<profile_cmd::ProfileList, String> {
    profile_cmd::list_profiles_cmd()
}

#[command]
pub fn show_profile(name: String) -> Result<profile_cmd::ProfileDetail, String> {
    profile_cmd::show_profile_cmd(&name)
}

#[command]
pub fn get_env(name: String) -> Result<profile_cmd::EnvOutput, String> {
    profile_cmd::get_env_cmd(&name)
}

#[command]
pub fn add_profile(name: String, desc: Option<String>) -> Result<profile_cmd::MutationResult, String> {
    profile_cmd::add_profile_cmd(&name, desc.as_deref())
}

#[command]
pub fn remove_profile(name: String) -> Result<profile_cmd::MutationResult, String> {
    profile_cmd::remove_profile_cmd(&name)
}

#[command]
pub fn set_env_var(name: String, key: String, value: String) -> Result<profile_cmd::MutationResult, String> {
    profile_cmd::set_env_var_cmd(&name, &key, &value)
}

#[command]
pub fn unset_env_var(name: String, key: String) -> Result<profile_cmd::MutationResult, String> {
    profile_cmd::unset_env_var_cmd(&name, &key)
}

#[command]
pub fn set_default_profile(name: String) -> Result<profile_cmd::MutationResult, String> {
    profile_cmd::set_default_profile_cmd(&name)
}

#[command]
pub fn get_default_profile() -> Result<String, String> {
    profile_cmd::get_default_profile_cmd()
}

#[command]
pub fn init_profiles() -> Result<profile_cmd::MutationResult, String> {
    profile_cmd::init_profiles_cmd()
}

#[command]
pub fn ensure_shell_rc() -> Result<String, String> {
    profile_cmd::ensure_shell_rc()
}

// ── Tests ──

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_backup_file_path() {
        let bak = backup_file();
        assert!(bak.ends_with("config.yaml.bak"), "backup_file should end with config.yaml.bak, got: {:?}", bak);
    }

    #[test]
    fn test_config_backup_exists_non_panic() {
        let _ = config_backup_exists();
    }

    fn temp_config_setup() -> (std::sync::MutexGuard<'static, ()>, tempfile::TempDir) {
        let guard = crate::TEST_ENV_LOCK.lock().unwrap();
        let dir = tempfile::tempdir().unwrap();
        std::env::set_var("CLAUDE_PROFILES_HOME", dir.path().to_string_lossy().to_string());
        (guard, dir)
    }

    fn cleanup_config(dir: tempfile::TempDir) {
        std::env::remove_var("KN_HOME");
        std::env::remove_var("CLAUDE_PROFILES_HOME");
        drop(dir);
    }

    #[test]
    fn test_crud_flow_full_lifecycle() {
        let (_guard, dir) = temp_config_setup();

        assert!(add_profile("alpha".into(), Some("first".into())).unwrap().ok);
        assert!(add_profile("beta".into(), None).unwrap().ok);
        assert!(add_profile("gamma".into(), Some("third".into())).unwrap().ok);

        let list = list_profiles().unwrap();
        assert_eq!(list.profiles.len(), 3);

        assert!(set_env_var("alpha".into(), "KEY1".into(), "val1".into()).unwrap().ok);
        let detail = show_profile("alpha".into()).unwrap();
        assert_eq!(detail.env.get("KEY1"), Some(&"val1".to_string()));

        assert!(set_default_profile("beta".into()).unwrap().ok);
        assert_eq!(get_default_profile().unwrap(), "beta");

        assert!(remove_profile("gamma".into()).unwrap().ok);
        let list2 = list_profiles().unwrap();
        assert_eq!(list2.profiles.len(), 2);

        assert!(remove_profile("beta".into()).unwrap().ok);
        assert_eq!(get_default_profile().unwrap(), "alpha");

        assert!(!add_profile("BAD NAME".into(), None).unwrap().ok);

        cleanup_config(dir);
    }
}
