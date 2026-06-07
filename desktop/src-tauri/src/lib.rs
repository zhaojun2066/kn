mod agent_manager;
mod commands;
mod hook_manager;
mod hook_meta;
mod hook_store;
mod profile_cmd;
mod pty;
mod skill_manager;
mod usage;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

/// Shared config directory.
///
/// Respects `CLAUDE_PROFILES_HOME` env var (matching Python `lib/config.py`),
/// falling back to `~/.claude-profiles` on all platforms.
pub(crate) fn config_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("CLAUDE_PROFILES_HOME") {
        let p = PathBuf::from(&dir);
        if p.is_absolute() {
            return p;
        }
    }
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_else(|_| ".".into());
    PathBuf::from(&home).join(".claude-profiles")
}

/// Resolve the user home directory across platforms.
///
/// Reads `HOME` first, falling back to `USERPROFILE` on Windows (where
/// GUI apps launched from Explorer typically lack `HOME`). Returns `.`
/// as a last resort.
pub(crate) fn home_dir() -> PathBuf {
    let raw = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_else(|_| ".".into());
    PathBuf::from(&raw)
}

/// Resolve the user's Documents folder on Windows.
///
/// Uses `[Environment]::GetFolderPath('MyDocuments')` to handle folder
/// redirection (OneDrive, Group Policy, custom locations). Falls back to
/// `$HOME/Documents`, then `$HOME/OneDrive/Documents`.
pub(crate) fn windows_documents_dir() -> PathBuf {
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_else(|_| ".".into());
    let home_path = std::path::Path::new(&home);

    let api_result = std::process::Command::new("powershell")
        .args([
            "-NoProfile",
            "-Command",
            "[Environment]::GetFolderPath('MyDocuments')",
        ])
        .output()
        .ok()
        .and_then(|o| {
            let s = String::from_utf8_lossy(&o.stdout).trim().to_string();
            if s.is_empty() {
                None
            } else {
                Some(PathBuf::from(s))
            }
        });
    api_result.unwrap_or_else(|| {
        let default_docs = home_path.join("Documents");
        if default_docs.exists() {
            default_docs
        } else {
            let onedrive_docs = home_path.join("OneDrive").join("Documents");
            if onedrive_docs.exists() {
                onedrive_docs
            } else {
                default_docs
            }
        }
    })
}

pub fn run() {
    let pty_state = Arc::new(Mutex::new(pty::PtyState {
        handles: HashMap::new(),
    }));

    let cancel_state = std::sync::Mutex::new(skill_manager::CancelState {
        cancelled: Arc::new(std::sync::atomic::AtomicBool::new(false)),
    });

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(pty_state)
        .manage(cancel_state)
        .invoke_handler(tauri::generate_handler![
            commands::list_profiles,
            commands::show_profile,
            commands::get_env,
            commands::add_profile,
            commands::remove_profile,
            commands::set_env_var,
            commands::unset_env_var,
            commands::set_default_profile,
            commands::get_default_profile,
            commands::init_profiles,
            commands::ensure_shell_rc,
            commands::write_file,
            commands::read_file,
            commands::read_file_base64,
            commands::list_directory_tree,
            commands::scan_system_configs,
            commands::read_app_config,
            commands::temp_dir,
            commands::is_debug_build,
            commands::get_platform_info,
            commands::get_app_version,
            commands::fetch_url,
            commands::download_file,
            commands::verify_sha256,
            commands::open_file,
            commands::check_environment,
            commands::config_backup_exists,
            commands::backup_config,
            commands::restore_config_backup,
            commands::batch_export_profiles,
            commands::batch_delete_profiles,
            pty::start_pty,
            pty::write_pty,
            pty::resize_pty,
            pty::kill_pty,
            usage::get_usage,
            usage::get_daily_usage,
            usage::get_pricing,
            usage::set_pricing,
            usage::replace_pricing,
            usage::get_usage_tracking_enabled,
            usage::set_usage_tracking_enabled,
            usage::ensure_usage_hooks,
            skill_manager::scan_skills,
            skill_manager::toggle_plugin,
            skill_manager::toggle_standalone_skill,
            skill_manager::check_updates,
            skill_manager::cancel_check_updates,
            skill_manager::update_plugin,
            skill_manager::list_marketplace_plugins,
            skill_manager::install_plugin,
            skill_manager::uninstall_plugin,
            skill_manager::install_standalone_skill,
            skill_manager::uninstall_standalone_skill,
            skill_manager::toggle_command,
            skill_manager::uninstall_command,
            skill_manager::add_marketplace,
            skill_manager::remove_marketplace,
            agent_manager::scan_agents,
            agent_manager::build_dependency_graph,
            agent_manager::analyze_impact,
            agent_manager::toggle_agent,
            agent_manager::read_agent_content,
            skill_manager::read_skill_content,
            hook_manager::scan_hooks,
            hook_manager::toggle_hook,
            hook_manager::delete_hook,
            hook_manager::create_hook,
            hook_meta::get_hook_meta,
            hook_meta::set_hook_meta,
            hook_meta::delete_hook_meta,
            hook_store::list_store_hooks,
            hook_store::install_store_hook,
            hook_store::uninstall_store_hook,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
