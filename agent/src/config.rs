use std::path::PathBuf;

/// Agent 运行时配置，从 `~/.kn/agent/config.json` 加载，支持环境变量覆盖。
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct AgentConfig {
    /// WebSocket 云端地址（wss://... 或 ws://...）
    pub cloud_url: String,
    /// HTTP API 地址
    pub cloud_http_url: String,
    /// 配置目录（~/.kn）
    pub config_dir: PathBuf,
    /// Agent 专用目录（~/.kn/agent）
    pub agent_dir: PathBuf,
    /// IPC socket 路径
    pub ipc_socket_path: PathBuf,
    /// 日志目录
    pub log_dir: PathBuf,
    /// 设备指纹（启动时缓存）
    pub machine_id: String,
    /// 主机名
    pub hostname: String,
    /// 操作系统版本
    pub os_version: String,
    /// 购买兑换码链接（从云端配置获取，env var KN_PURCHASE_URL 可覆盖）
    pub purchase_url: String,
}

impl AgentConfig {
    /// 加载配置：文件 + 环境变量覆盖
    pub fn load() -> crate::error::Result<Self> {
        let config_dir = kn_common::path::config_dir();
        let agent_dir = config_dir.join("agent");

        // 读取配置文件（不存在则用默认值）
        let file_config = Self::read_config_file(&agent_dir);

        // 环境变量覆盖
        let cloud_url = std::env::var("KN_CLOUD_URL")
            .ok()
            .or(file_config
                .as_ref()
                .and_then(|m| m.get("cloud_url").cloned()))
            .unwrap_or_else(|| "wss://api.shark.kim/v1/ws".to_string());

        let cloud_http_url = std::env::var("KN_CLOUD_HTTP_URL")
            .ok()
            .or(file_config
                .as_ref()
                .and_then(|m| m.get("cloud_http_url").cloned()))
            .unwrap_or_else(|| "https://api.shark.kim".to_string());

        let ipc_socket_path = agent_dir.join("ipc.sock");
        let log_dir = agent_dir.join("logs");

        // 设备信息
        let machine_id = kn_common::fingerprint::machine_id().unwrap_or_default();
        let hostname = hostname::get()
            .map(|h| h.to_string_lossy().to_string())
            .unwrap_or_else(|_| "unknown".to_string());
        let os_version = sysinfo::System::long_os_version()
            .unwrap_or_else(|| "unknown".to_string());

        let purchase_url = std::env::var("KN_PURCHASE_URL")
            .ok()
            .or(file_config
                .as_ref()
                .and_then(|m| m.get("purchase_url").cloned()))
            .unwrap_or_default();

        Ok(Self {
            cloud_url,
            cloud_http_url,
            config_dir,
            agent_dir,
            ipc_socket_path,
            log_dir,
            machine_id,
            hostname,
            os_version,
            purchase_url,
        })
    }

    fn read_config_file(
        agent_dir: &std::path::Path,
    ) -> Option<std::collections::HashMap<String, String>> {
        let path = agent_dir.join("config.json");
        let content = std::fs::read_to_string(&path).ok()?;
        match serde_json::from_str(&content) {
            Ok(cfg) => Some(cfg),
            Err(e) => {
                eprintln!("[kn-agent] 警告: config.json 解析失败 ({}), 使用默认配置", e);
                None
            }
        }
    }
}
