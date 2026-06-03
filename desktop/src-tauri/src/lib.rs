mod commands;
mod profile_cmd;
mod pty;
mod usage;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

pub fn run() {
    let pty_state = Arc::new(Mutex::new(pty::PtyState {
        handles: HashMap::new(),
    }));

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(pty_state)
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
            commands::scan_system_configs,
            commands::read_app_config,
            commands::write_app_config,
            commands::temp_dir,
            commands::is_debug_build,
            commands::get_platform_info,
            commands::get_app_version,
            commands::fetch_url,
            commands::download_file,
            commands::verify_sha256,
            commands::open_file,
            commands::new_window,
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
            usage::get_usage_tracking_enabled,
            usage::set_usage_tracking_enabled,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
