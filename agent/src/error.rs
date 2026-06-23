use thiserror::Error;
use kn_common::error::CommonError;

/// Agent 专用错误类型层级。
#[derive(Error, Debug)]
pub enum AgentError {
    #[error("公共错误: {0}")]
    Common(#[from] CommonError),

    #[error("WebSocket 错误: {0}")]
    Ws(String),

    #[error("协议错误: {0}")]
    Protocol(String),

    #[error("HTTP 错误: {0}")]
    Http(#[from] reqwest::Error),

    #[error("状态转换错误: 从 {from} 收到事件 {event}")]
    StateTransition { from: String, event: String },

    #[error("设备未绑定")]
    NotBound,

    #[error("会话未找到: {0}")]
    SessionNotFound(String),

    #[error("安全模式: 无法执行操作")]
    SafeMode,

    #[error("超时: {0}")]
    Timeout(String),

    #[error("关闭请求")]
    Shutdown,

    #[error("IO 错误: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON 错误: {0}")]
    Json(#[from] serde_json::Error),

    #[error("{0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, AgentError>;

// 便利转换
impl From<String> for AgentError {
    fn from(s: String) -> Self {
        AgentError::Other(s)
    }
}

impl From<&str> for AgentError {
    fn from(s: &str) -> Self {
        AgentError::Other(s.to_string())
    }
}
