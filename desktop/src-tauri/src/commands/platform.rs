//! Platform info, build mode, version, and temp dir.

use tauri::command;

#[command]
pub fn temp_dir() -> String {
    std::env::temp_dir().display().to_string()
}

#[derive(Debug, serde::Serialize)]
pub struct PlatformInfo {
    pub os: String,
    pub arch: String,
}

#[command]
pub fn is_debug_build() -> bool {
    cfg!(debug_assertions)
}

#[command]
pub fn get_platform_info() -> PlatformInfo {
    PlatformInfo {
        os: "macos".into(),
        arch: if cfg!(target_arch = "aarch64") { "aarch64".into() } else { "x86_64".into() },
    }
}

#[command]
pub fn get_app_version(app: tauri::AppHandle) -> String {
    app.config().version.clone().unwrap_or_else(|| "0.0.0".into())
}
