use std::io::Write;
use std::sync::{Arc, Mutex};

/// PTY 输出接收器 trait — Desktop (ChannelSink) 和 Agent (WssSink, IpcSink) 各自实现。
/// 用于将 PTY 输出数据转发到不同的目标（Tauri Channel、WebSocket、IPC Socket）。
pub trait PtyOutputSink: Send + 'static {
    /// 发送原始字节数据到目标
    fn send(&self, data: &[u8]) -> Result<(), String>;
    /// PTY 就绪通知
    fn on_ready(&self) -> Result<(), String> {
        Ok(())
    }
    /// PTY 退出通知（携带退出码）
    fn on_exit(&self, _code: i32) -> Result<(), String> {
        Ok(())
    }
    /// PTY 错误通知
    fn on_error(&self, _msg: &str) -> Result<(), String> {
        Ok(())
    }
}

/// Thread-safe writer handle — allows independent locking for `write_pty`.
pub type SharedWriter = Arc<Mutex<Box<dyn Write + Send>>>;

/// Shared child process handle — allows `kill_pty` to terminate the process
/// even if the reader thread is blocked on I/O.
pub type SharedChild = Arc<Mutex<Option<Box<dyn portable_pty::Child + Send>>>>;
