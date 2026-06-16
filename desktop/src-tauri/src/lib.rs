mod agent_manager;
mod commands;
mod hook_logs;
mod hook_manager;
mod hook_meta;
pub mod hook_store;
mod profile_cmd;
mod project_manager;
mod pty;
mod skill_manager;
mod usage;

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use fs2::FileExt;


/// Global write lock — serializes all config file writes to prevent
/// data corruption when multiple Tauri commands run concurrently.
/// (The Python CLI already uses fcntl.flock for its own writes.)
static WRITE_LOCK: std::sync::LazyLock<Mutex<()>> =
    std::sync::LazyLock::new(|| Mutex::new(()));

#[cfg(test)]
pub(crate) static TEST_ENV_LOCK: std::sync::LazyLock<Mutex<()>> =
    std::sync::LazyLock::new(|| Mutex::new(()));

/// Acquire the global write lock and run the closure.
/// All file-write operations should pass through this to avoid races.
///
/// # Lock ordering
/// `with_write_lock` (process mutex) MUST be acquired BEFORE
/// `with_cross_process_lock` (fs2 file lock). Reversing this order
/// could deadlock when the Python CLI holds the file lock while
/// a Rust command holds the mutex and vice versa.
pub(crate) fn with_write_lock<F, T>(f: F) -> Result<T, String>
where
    F: FnOnce() -> Result<T, String>,
{
    let _guard = WRITE_LOCK
        .lock()
        .map_err(|e| format!("write lock poisoned: {}", e))?;
    f()
}

/// Acquire **both** the process mutex and cross-process file lock,
/// then run the closure. This is the canonical way to perform a
/// write that must be safe from concurrent Rust↔Python access.
///
/// Always use this (or the two-step `with_write_lock` →
/// `with_cross_process_lock` in that order) instead of calling
/// `with_cross_process_lock` alone.
pub(crate) fn with_write_lock_exclusive<F, T>(f: F) -> Result<T, String>
where
    F: FnOnce() -> Result<T, String>,
{
    with_write_lock(|| with_cross_process_lock(f))
}

/// Acquire a cross-process file lock (via fs2, interoperable with Python
/// fcntl.flock on Unix) and run the closure. Serializes writes between
/// the Rust desktop app and the Python CLI / hook recorder scripts.
///
/// Uses `.config.lock` in the config directory as the coordination file.
/// 5-second busy-wait timeout, retrying every 50ms.
///
/// **Important**: this function MUST be called inside `with_write_lock`.
/// Always prefer `with_write_lock_exclusive` which combines both locks
/// in the correct order.
pub(crate) fn with_cross_process_lock<F, T>(f: F) -> Result<T, String>
where
    F: FnOnce() -> Result<T, String>,
{
    let lock_path = config_dir().join(".config.lock");
    if let Some(parent) = lock_path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("创建锁目录失败: {}", e))?;
    }
    let lock_fh = std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(false)
        .open(&lock_path)
        .map_err(|e| format!("无法打开锁文件: {}", e))?;

    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        match lock_fh.try_lock_exclusive() {
            Ok(()) => break,
            Err(_) => {
                if Instant::now() > deadline {
                    return Err("无法获取跨进程锁 (5s 超时)".into());
                }
                std::thread::sleep(Duration::from_millis(50));
            }
        }
    }

    let result = f();
    let _ = lock_fh.unlock();
    result
}

/// Atomically rename `src` to `dst`, overwriting `dst` if it exists.
pub(crate) fn atomic_rename(src: &std::path::Path, dst: &std::path::Path) -> Result<(), String> {
    std::fs::rename(src, dst).map_err(|e| format!("rename 失败: {} -> {}: {}", src.display(), dst.display(), e))
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
/// Respects `KN_HOME` env var first, then `CLAUDE_PROFILES_HOME` (legacy),
/// falling back to `~/.kn` on all platforms.
///
/// Environment variable values are validated: must be absolute and
/// must not contain `..` path-traversal components.
pub(crate) fn config_dir() -> PathBuf {
    // KN_HOME takes precedence (new name), CLAUDE_PROFILES_HOME for backward compat
    for var in &["KN_HOME", "CLAUDE_PROFILES_HOME"] {
        if let Ok(dir) = std::env::var(var) {
            let p = PathBuf::from(&dir);
            // Must be absolute AND must not contain path traversal
            if p.is_absolute() && !p.components().any(|c| c == std::path::Component::ParentDir) {
                return p;
            }
        }
    }
    let home = std::env::var("HOME").unwrap_or_else(|_| {
        std::env::temp_dir()
            .to_string_lossy()
            .to_string()
    });
    PathBuf::from(&home).join(".kn")
}

/// One-time migration: if `~/.kn/` doesn't exist but `~/.claude-profiles/` does,
/// rename the legacy directory to the new name.
///
/// Called from `ensure_shell_rc` during app startup. Best-effort — failures are
/// logged but never block the app. The old directory is left in place on failure.
pub(crate) fn migrate_legacy_config_dir() {
    let new_dir = config_dir();
    if new_dir.exists() {
        return; // Already migrated or fresh install
    }

    let home = std::env::var("HOME").unwrap_or_default();
    if home.is_empty() {
        return;
    }
    let legacy_dir = PathBuf::from(&home).join(".claude-profiles");
    if !legacy_dir.exists() {
        return; // No legacy data to migrate
    }

    // Best-effort rename — if it fails (e.g. permissions), the old dir
    // stays and config_dir() will still use ~/.kn for new writes.
    // Users can manually migrate or set KN_HOME to point to the old dir.
    if let Err(e) = std::fs::rename(&legacy_dir, &new_dir) {
        eprintln!(
            "[kn] 无法迁移配置目录 {} → {}: {}。旧数据保留，新配置将写入 {}。",
            legacy_dir.display(),
            new_dir.display(),
            e,
            new_dir.display()
        );
    }
}

/// Resolve the user home directory.
///
/// Reads `HOME` first, falling back to `echo $HOME` via `sh`,
/// and finally to temp dir as a last resort.
pub(crate) fn home_dir() -> PathBuf {
    if let Ok(home) = std::env::var("HOME") {
        return PathBuf::from(home);
    }
    // Fallback: try shell to resolve home directory
    if let Ok(output) = std::process::Command::new("sh")
        .args(["-c", "echo $HOME"])
        .output()
    {
        let s = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !s.is_empty() {
            return PathBuf::from(s);
        }
    }
    // Last resort: temp dir is always available and writable,
    // unlike CWD which may be "/" in macOS .app bundles
    std::env::temp_dir()
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
            commands::list_directory_children,
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
            commands::open_in_editor,
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
            project_manager::toggle_pin_project,
            project_manager::get_project_stats,
            project_manager::get_project_overview,
            project_manager::write_ai_profile,
            project_manager::read_ai_profile,
            project_manager::read_session_preview,
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_config_dir_rejects_parent_dir() {
        // Verify path traversal detection works on the underlying logic
        let p = std::path::PathBuf::from("/tmp/../etc");
        let has_parent = p.components().any(|c| c == std::path::Component::ParentDir);
        assert!(has_parent, "test input should contain '..'");

        let p2 = std::path::PathBuf::from("/tmp/valid");
        let has_parent2 = p2.components().any(|c| c == std::path::Component::ParentDir);
        assert!(!has_parent2, "clean path should not have parent dir components");
    }

    #[test]
    fn test_atomic_rename_basic() {
        let dir = std::env::temp_dir().join("kn-test-atomic-rename");
        fs::create_dir_all(&dir).unwrap();
        let src = dir.join("source.txt");
        let dst = dir.join("target.txt");
        let _ = fs::remove_file(&dst);

        fs::write(&src, "hello world").unwrap();
        atomic_rename(&src, &dst).unwrap();
        assert!(!src.exists());
        assert!(dst.exists());
        assert_eq!(fs::read_to_string(&dst).unwrap(), "hello world");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_atomic_rename_overwrite_existing() {
        let dir = std::env::temp_dir().join("kn-test-atomic-overwrite");
        fs::create_dir_all(&dir).unwrap();
        let src = dir.join("source.txt");
        let dst = dir.join("target.txt");

        fs::write(&src, "new").unwrap();
        fs::write(&dst, "old").unwrap();
        atomic_rename(&src, &dst).unwrap();
        assert!(!src.exists());
        assert_eq!(fs::read_to_string(&dst).unwrap(), "new");
        let _ = fs::remove_dir_all(&dir);
    }


    #[test]
    fn test_hash_path_deterministic() {
        let a = hash_path("/Users/test/project");
        let b = hash_path("/Users/test/project");
        assert_eq!(a, b);
        assert!(!a.is_empty() && a.len() >= 8);
        assert!(a.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn test_project_name_from_root_valid() {
        let name = project_name_from_root(std::path::Path::new("/Users/alice/myproject"));
        assert_eq!(name, Some("myproject".to_string()));
    }

    #[test]
    fn test_project_name_from_root_root_dir() {
        let name = project_name_from_root(std::path::Path::new("/"));
        assert!(name.is_none() || name == Some("".to_string()));
    }
}
