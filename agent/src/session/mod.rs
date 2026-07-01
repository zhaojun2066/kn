//! 会话管理器 — SessionStore trait + MemorySessionStore + SessionManager。
//!
//! 管理 AI CLI 工具会话的生命周期。Phase 1 使用内存存储，
//! trait 抽象允许 Phase 2 轻松切换到持久化存储。

use std::path::PathBuf;
use std::sync::Arc;

pub mod env;
pub mod input;
mod manager;
pub mod output;
pub mod persistence;
pub mod store;
pub mod types;

/// 存储 PTY writer + OutputFanout，供 `attach_pty` 桥接。
/// 放在 mod.rs 以避免 types.rs ↔ output.rs 循环依赖。
pub(crate) struct PtyAttachHandle {
    pub writer: Arc<tokio::sync::Mutex<Box<dyn std::io::Write + Send>>>,
    pub fanout: output::OutputFanout,
}

/// 计算 per-session PTY proxy socket 路径。
pub fn pty_sock_path(nid: &str) -> PathBuf {
    kn_common::path::agent_dir()
        .join("sessions")
        .join(nid)
        .join("pty.sock")
}

// Re-export public API (保持与拆分前完全一致的对外接口)
pub use env::resolve_tool_path;
pub use input::{InputMerger, InputMessage};
pub use manager::SessionManager;
pub use output::OutputFanout;
pub use store::{MemorySessionStore, SessionStore};
pub use types::{ManagedSession, SessionKind, SessionStatus, SessionSummary, ViewportOwner};
