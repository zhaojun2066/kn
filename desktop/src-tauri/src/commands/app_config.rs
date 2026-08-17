//! Runtime config read/write — runtime-config.json discovery and persistence.

use tauri::command;

const RUNTIME_CONFIG_FILE: &str = "runtime-config.json";
const DEV_RUNTIME_CONFIG_FILE: &str = "runtime-config.dev.json";
const DEFAULT_CLOUD_WS_URL: &str = "wss://api.knshark.com/v1/ws";
const DEFAULT_CLOUD_HTTP_URL: &str = "https://api.knshark.com";

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct AppConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub release_api_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cloud_ws_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cloud_http_url: Option<String>,
}

pub(crate) fn load_app_config() -> Result<AppConfig, String> {
    let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()))
        .unwrap_or_else(|| cwd.clone());
    let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut paths = Vec::new();
    if cfg!(debug_assertions) {
        paths.extend([
            cwd.join(DEV_RUNTIME_CONFIG_FILE),
            manifest_dir.join(DEV_RUNTIME_CONFIG_FILE),
            exe_dir.join(DEV_RUNTIME_CONFIG_FILE),
            crate::config_dir().join(DEV_RUNTIME_CONFIG_FILE),
        ]);
    }
    paths.extend([
        exe_dir.join("../Resources").join(RUNTIME_CONFIG_FILE),
        cwd.join(RUNTIME_CONFIG_FILE),
        manifest_dir.join(RUNTIME_CONFIG_FILE),
        crate::config_dir().join(RUNTIME_CONFIG_FILE),
    ]);
    for path in &paths {
        if path.exists() {
            let content = std::fs::read_to_string(path).map_err(|e| format!("读取失败: {}", e))?;
            return serde_json::from_str(&content).map_err(|e| format!("解析失败: {}", e));
        }
    }
    Ok(AppConfig {
        release_api_url: None,
        cloud_ws_url: None,
        cloud_http_url: None,
    })
}

/// Cloud endpoints come from the selected runtime config. Debug builds allow
/// local HTTP/WS endpoints; release builds only accept HTTPS/WSS values.
pub(crate) fn production_cloud_urls() -> (String, String) {
    let config = load_app_config().ok();
    let ws = config
        .as_ref()
        .and_then(|item| item.cloud_ws_url.as_ref())
        .filter(|url| valid_ws_url(url))
        .cloned()
        .unwrap_or_else(|| DEFAULT_CLOUD_WS_URL.to_string());
    let http = config
        .as_ref()
        .and_then(|item| item.cloud_http_url.as_ref())
        .filter(|url| valid_http_url(url))
        .cloned()
        .unwrap_or_else(|| DEFAULT_CLOUD_HTTP_URL.to_string());
    (ws, http)
}

fn valid_ws_url(url: &str) -> bool {
    url.starts_with("wss://") || (cfg!(debug_assertions) && url.starts_with("ws://"))
}

fn valid_http_url(url: &str) -> bool {
    url.starts_with("https://") || (cfg!(debug_assertions) && url.starts_with("http://"))
}

#[command]
pub fn read_app_config() -> Result<AppConfig, String> {
    load_app_config()
}

#[command]
#[allow(dead_code)]
pub fn write_app_config(config: AppConfig) -> Result<(), String> {
    let file_name = if cfg!(debug_assertions) {
        DEV_RUNTIME_CONFIG_FILE
    } else {
        RUNTIME_CONFIG_FILE
    };
    let path = crate::config_dir().join(file_name);
    let dir = path.parent().ok_or("更新配置目录无效")?;
    std::fs::create_dir_all(dir).map_err(|e| format!("创建目录失败: {}", e))?;
    let content =
        serde_json::to_string_pretty(&config).map_err(|e| format!("序列化失败: {}", e))?;
    std::fs::write(&path, &content).map_err(|e| format!("写入失败: {}", e))
}
