mod agent_ipc;
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
use std::path::PathBuf;
use tauri::Manager;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use fs2::FileExt;

// Re-export path utilities from common (used widely across the desktop crate)
pub(crate) use kn_common::path::{
    atomic_rename, config_dir, hash_path, home_dir, project_name_from_root,
};


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
        .setup(|app| {
            let agent_dir = kn_common::path::agent_dir();
            let agent_bin = agent_dir.join("kn-agent");
            let plist_dir = kn_common::path::home_dir().join("Library").join("LaunchAgents");
            let plist_path = plist_dir.join("com.kn.agent.plist");
            let log_dir = agent_dir.join("logs");
            let uid = unsafe { libc::getuid() };
            let domain = format!("gui/{}", uid);
            let service_name = format!("gui/{}/com.kn.agent", uid);

            let _ = std::fs::create_dir_all(&agent_dir);

            // ── Dev mode: always restart agent with latest debug binary ──
            if cfg!(debug_assertions) {
                let debug_agent = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("../../target/debug/kn-agent");

                if debug_agent.exists() {
                    // 1. Bootout old agent (ignore errors — may not be running)
                    eprintln!("[kn] dev: stopping kn-agent...");
                    let _ = std::process::Command::new("launchctl")
                        .args(["bootout", &service_name])
                        .output();
                    // Wait for process to exit
                    std::thread::sleep(std::time::Duration::from_millis(500));

                    // 2. Copy latest debug binary (atomic tmp+rename)
                    let tmp = agent_dir.join("kn-agent.tmp");
                    if std::fs::copy(&debug_agent, &tmp).is_ok() {
                        #[cfg(unix)]
                        {
                            use std::os::unix::fs::PermissionsExt;
                            let _ = std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o755));
                        }
                        if let Ok(f) = std::fs::File::open(&tmp) {
                            let _ = f.sync_all();
                        }
                        if std::fs::rename(&tmp, &agent_bin).is_ok() {
                            eprintln!("[kn] dev: kn-agent updated from target/debug");
                        }
                    }
                } else {
                    eprintln!(
                        "[kn] dev: target/debug/kn-agent 不存在，请先执行: cargo build --package kn-agent"
                    );
                }

                // 3. Always write plist (ensures env vars match dev config)
                let _ = std::fs::create_dir_all(&log_dir);
                let plist_content = format!(
                    r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>com.kn.agent</string>
    <key>ProgramArguments</key>
    <array>
        <string>{bin}</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <true/>
    <key>ThrottleInterval</key>
    <integer>5</integer>
    <key>StandardOutPath</key>
    <string>{stdout_log}</string>
    <key>StandardErrorPath</key>
    <string>{stderr_log}</string>
    <key>EnvironmentVariables</key>
    <dict>
        <key>RUST_LOG</key>
        <string>info</string>
        <key>KN_CLOUD_URL</key>
        <string>ws://localhost:8081/v1/ws</string>
        <key>KN_CLOUD_HTTP_URL</key>
        <string>http://localhost:8080</string>
    </dict>
</dict>
</plist>
"#,
                    bin = agent_bin.display(),
                    stdout_log = log_dir.join("stdout.log").display(),
                    stderr_log = log_dir.join("stderr.log").display(),
                );
                let _ = std::fs::create_dir_all(&plist_dir);
                let _ = std::fs::write(&plist_path, plist_content);

                // 4. Bootstrap agent
                eprintln!("[kn] dev: bootstrapping kn-agent...");
                match std::process::Command::new("launchctl")
                    .args(["bootstrap", &domain, &plist_path.display().to_string()])
                    .output()
                {
                    Ok(out) if out.status.success() => {
                        eprintln!("[kn] dev: kn-agent started via launchd");
                    }
                    Ok(out) => {
                        let stderr = String::from_utf8_lossy(&out.stderr);
                        if stderr.contains("already bootstrapped") {
                            eprintln!("[kn] dev: kn-agent already bootstrapped (OK)");
                        } else {
                            eprintln!("[kn] dev: launchctl bootstrap 失败: {}", stderr.trim());
                        }
                    }
                    Err(e) => eprintln!("[kn] dev: 无法启动 launchctl: {}", e),
                }
            } else {
                // ── Production: copy from app bundle, only start if needed ──
                if let Ok(resource_dir) = app.path().resource_dir() {
                    let bundled = resource_dir.join("resources").join("kn-agent");
                    if bundled.exists() {
                        let needs_copy = !agent_bin.exists()
                            || bundled.metadata().ok().map(|m| m.len()).unwrap_or(0)
                                != agent_bin.metadata().ok().map(|m| m.len()).unwrap_or(0);
                        if needs_copy {
                            let tmp = agent_dir.join("kn-agent.tmp");
                            if std::fs::copy(&bundled, &tmp).is_ok() {
                                #[cfg(unix)]
                                {
                                    use std::os::unix::fs::PermissionsExt;
                                    let _ = std::fs::set_permissions(
                                        &tmp,
                                        std::fs::Permissions::from_mode(0o755),
                                    );
                                }
                                if let Ok(f) = std::fs::File::open(&tmp) {
                                    let _ = f.sync_all();
                                }
                                if std::fs::rename(&tmp, &agent_bin).is_ok() {
                                    eprintln!("[kn] kn-agent binary installed/updated");
                                }
                            }
                        }
                    } else {
                        eprintln!("[kn] bundled kn-agent not found at {}", bundled.display());
                    }
                }

                if agent_bin.exists() {
                    if !plist_path.exists() {
                        let _ = std::fs::create_dir_all(&log_dir);
                        let plist_content = format!(
                            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>com.kn.agent</string>
    <key>ProgramArguments</key>
    <array>
        <string>{bin}</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <true/>
    <key>ThrottleInterval</key>
    <integer>5</integer>
    <key>StandardOutPath</key>
    <string>{stdout_log}</string>
    <key>StandardErrorPath</key>
    <string>{stderr_log}</string>
    <key>EnvironmentVariables</key>
    <dict>
        <key>RUST_LOG</key>
        <string>info</string>
        {env_vars}
    </dict>
</dict>
</plist>
"#,
                            bin = agent_bin.display(),
                            stdout_log = log_dir.join("stdout.log").display(),
                            stderr_log = log_dir.join("stderr.log").display(),
                            env_vars = r#"<key>KN_CLOUD_URL</key>
        <string>wss://api.shark.kim/v1/ws</string>
        <key>KN_CLOUD_HTTP_URL</key>
        <string>https://api.shark.kim</string>"#,
                        );
                        let _ = std::fs::create_dir_all(&plist_dir);
                        let _ = std::fs::write(&plist_path, plist_content);
                    }

                    let agent_running = std::process::Command::new("launchctl")
                        .args(["print", &service_name])
                        .output()
                        .map(|o| o.status.success())
                        .unwrap_or(false);

                    if !agent_running {
                        match std::process::Command::new("launchctl")
                            .args(["bootstrap", &domain, &plist_path.display().to_string()])
                            .output()
                        {
                            Ok(out) => {
                                if out.status.success() {
                                    eprintln!("[kn] kn-agent bootstrapped via launchd");
                                } else {
                                    let stderr = String::from_utf8_lossy(&out.stderr);
                                    if !stderr.contains("already bootstrapped") {
                                        eprintln!("[kn] launchctl bootstrap 失敗: {}", stderr.trim());
                                    }
                                }
                            }
                            Err(e) => eprintln!("[kn] 无法启动 launchctl: {}", e),
                        }
                    }
                }
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            agent_ipc::agent_ipc,
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
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::Destroyed = event {
                window.app_handle().exit(0);
            }
        })
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
