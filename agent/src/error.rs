use kn_common::error::CommonError;
use thiserror::Error;

/// Agent 专用错误类型层级。
#[derive(Error, Debug)]
pub enum AgentError {
    #[error("公共错误: {0}")]
    Common(#[from] CommonError),

    #[error("WebSocket 错误: {0}")]
    Ws(String),

    #[error("协议错误: {0}")]
    Protocol(String),

    /// Cloud has definitively rejected a pending bind activation. Retrying the
    /// exact request cannot make it succeed (for example the device limit or
    /// a machine/token mismatch), so the local recovery worker must stop.
    #[error("绑定激活被拒绝: {0}")]
    BindActivationTerminal(String),

    /// Cloud or the network did not give a final activation result. The
    /// pending marker must be preserved and the idempotent request retried.
    #[error("绑定激活待重试: {0}")]
    BindActivationRetryable(String),

    #[error("HTTP 错误: {0}")]
    Http(#[from] reqwest::Error),

    #[error("状态转换错误: 从 {from} 收到事件 {event}")]
    StateTransition { from: String, event: String },

    #[error("设备未绑定")]
    NotBound,

    #[error("会话数量已达上限: 当前 {current}, 最大 {max}")]
    SessionLimit { current: usize, max: usize },

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
