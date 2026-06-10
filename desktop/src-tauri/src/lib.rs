mod agent_manager;
mod commands;
mod hook_logs;
mod hook_manager;
mod hook_meta;
mod hook_store;
mod profile_cmd;
mod project_manager;
mod pty;
mod skill_manager;
mod usage;

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// Global write lock — serializes all config file writes to prevent
/// data corruption when multiple Tauri commands run concurrently.
/// (The Python CLI already uses fcntl.flock for its own writes.)
static WRITE_LOCK: std::sync::LazyLock<Mutex<()>> =
    std::sync::LazyLock::new(|| Mutex::new(()));

/// Acquire the global write lock and run the closure.
/// All file-write operations should pass through this to avoid races.
pub(crate) fn with_write_lock<F, T>(f: F) -> Result<T, String>
where
    F: FnOnce() -> Result<T, String>,
{
    let _guard = WRITE_LOCK
        .lock()
        .map_err(|e| format!("write lock poisoned: {}", e))?;
    f()
}

/// Atomically rename `src` to `dst`, overwriting `dst` if it exists.
///
/// On Unix this is `std::fs::rename` (atomic + overwrites). On Windows,
/// `std::fs::rename` fails if the destination exists, so we use
/// `MoveFileExW` with `MOVEFILE_REPLACE_EXISTING` which performs the
/// replace atomically — avoiding the TOCTOU race of remove-then-rename.
pub(crate) fn atomic_rename(src: &std::path::Path, dst: &std::path::Path) -> Result<(), String> {
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        use std::ffi::OsStr;

        fn to_wide(s: &OsStr) -> Vec<u16> {
            s.encode_wide().chain(Some(0)).collect()
        }

        extern "system" {
            fn MoveFileExW(
                lpExistingFileName: *const u16,
                lpNewFileName: *const u16,
                dwFlags: u32,
            ) -> i32;
        }

        const MOVEFILE_REPLACE_EXISTING: u32 = 0x1;
        const MOVEFILE_WRITE_THROUGH: u32 = 0x8;

        let src_wide = to_wide(src.as_os_str());
        let dst_wide = to_wide(dst.as_os_str());
        let ret = unsafe {
            MoveFileExW(
                src_wide.as_ptr(),
                dst_wide.as_ptr(),
                MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
            )
        };
        if ret == 0 {
            return Err(format!(
                "MoveFileEx 失败: {} -> {}: {}",
                src.display(),
                dst.display(),
                std::io::Error::last_os_error()
            ));
        }
        Ok(())
    }
    #[cfg(not(windows))]
    {
        std::fs::rename(src, dst).map_err(|e| format!("rename 失败: {} -> {}: {}", src.display(), dst.display(), e))
    }
}

/// Generate a short (8-char hex) hash of a path string.
/// Used to create unique scope keys for project-level skill/agent/command IDs
/// and hook IDs to prevent collisions across projects.
pub(crate) fn hash_path(path: &str) -> String {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    path.hash(&mut hasher);
    format!("{:08x}", hasher.finish())
}

/// Derive a project name from a project root directory path.
/// Uses the last component of the path (the directory name).
pub(crate) fn project_name_from_root(root: &Path) -> Option<String> {
    root.file_name()
        .and_then(|n| n.to_str())
        .map(|s| s.to_string())
}

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
/// GUI apps launched from Explorer typically lack `HOME`).
/// If both env vars are missing, tries shell fallback (powershell on Windows,
/// `echo $HOME` via `sh` on Unix). Returns `.` as a last resort.
pub(crate) fn home_dir() -> PathBuf {
    if let Ok(home) = std::env::var("HOME") {
        return PathBuf::from(home);
    }
    if let Ok(home) = std::env::var("USERPROFILE") {
        return PathBuf::from(home);
    }
    // Fallback: try shell to resolve home directory
    if cfg!(target_os = "windows") {
        if let Ok(output) = std::process::Command::new("powershell.exe")
            .args(["-NoProfile", "-Command", "Write-Output $env:USERPROFILE"])
            .output()
        {
            let s = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !s.is_empty() {
                return PathBuf::from(s);
            }
        }
    } else if let Ok(output) = std::process::Command::new("sh")
        .args(["-c", "echo $HOME"])
        .output()
    {
        let s = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !s.is_empty() {
            return PathBuf::from(s);
        }
    }
    PathBuf::from(".")
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
            commands::get_home_dir,
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
            commands::open_in_terminal,
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
            usage::get_usage_by_project,
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
            agent_manager::delete_agent,
            agent_manager::read_agent_content,
            skill_manager::read_skill_content,
            skill_manager::move_skill_file,
            skill_manager::copy_skill_file,
            skill_manager::undo_move_skill,
            project_manager::list_projects,
            project_manager::add_project,
            project_manager::remove_project,
            project_manager::update_project,
            project_manager::write_ai_profile,
            project_manager::read_ai_profile,
            project_manager::scan_project_sessions,
            hook_manager::scan_hooks,
            hook_manager::toggle_hook,
            hook_manager::delete_hook,
            hook_manager::create_hook,
            hook_manager::move_hook_entry,
            hook_manager::copy_hook_entry,
            hook_manager::undo_move_hook,
            hook_manager::restore_hook_snapshot,
            hook_manager::set_hook_command,
            hook_meta::get_hook_meta,
            hook_meta::set_hook_meta,
            hook_meta::delete_hook_meta,
            hook_store::list_store_hooks,
            hook_store::install_store_hook,
            hook_store::uninstall_store_hook,
            hook_logs::get_hook_execution_logs,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
