//! PtyOutputSink 的 Agent 实现 — IpcSink (Desktop)。
//!
//! 实现 `kn_common::pty_trait::PtyOutputSink` trait，将 PTY 输出转发到不同目标。

use kn_common::pty_trait::PtyOutputSink;
use tokio::sync::mpsc;

/// IPC 输出 — PTY 数据原文推给 Desktop（通过 Unix Socket）。
pub struct IpcSink {
    pub tx: mpsc::UnboundedSender<String>,
}

impl PtyOutputSink for IpcSink {
    fn send(&self, data: &[u8]) -> Result<(), String> {
        if let Ok(text) = std::str::from_utf8(data) {
            self.tx.send(text.to_string()).map_err(|e| e.to_string())
        } else {
            Ok(())
        }
    }
}
