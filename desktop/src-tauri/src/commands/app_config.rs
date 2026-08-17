//! Runtime config read/write — runtime-config.json discovery and persistence.

use tauri::command;

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
    let paths = vec![
        exe_dir.join("../Resources/runtime-config.json"),
        cwd.join("runtime-config.json"),
        crate::config_dir().join("runtime-config.json"),
    ];
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

/// Production Agent endpoints are bundled with the desktop app so an
/// installation cannot be redirected through a mutable local config file.
pub(crate) fn production_cloud_urls() -> (String, String) {
    let config = load_app_config().ok();
    let ws = config
        .as_ref()
        .and_then(|item| item.cloud_ws_url.as_ref())
        .filter(|url| url.starts_with("wss://"))
        .cloned()
        .unwrap_or_else(|| "wss://api.shark.kim/v1/ws".to_string());
    let http = config
        .as_ref()
        .and_then(|item| item.cloud_http_url.as_ref())
        .filter(|url| url.starts_with("https://"))
        .cloned()
        .unwrap_or_else(|| "https://api.shark.kim".to_string());
    (ws, http)
}

#[command]
pub fn read_app_config() -> Result<AppConfig, String> {
    load_app_config()
}

#[command]
#[allow(dead_code)]
pub fn write_app_config(config: AppConfig) -> Result<(), String> {
    let path = crate::config_dir().join("runtime-config.json");
    let dir = path.parent().ok_or("更新配置目录无效")?;
    std::fs::create_dir_all(dir).map_err(|e| format!("创建目录失败: {}", e))?;
    let content =
        serde_json::to_string_pretty(&config).map_err(|e| format!("序列化失败: {}", e))?;
    std::fs::write(&path, &content).map_err(|e| format!("写入失败: {}", e))
}
