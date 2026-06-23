//! PtyOutputSink 的 Agent 实现 — WssSink (云端) + IpcSink (Desktop)。
//!
//! 实现 `kn_common::pty_trait::PtyOutputSink` trait，将 PTY 输出转发到不同目标。

use crate::proto::WsMessageBuilder;
use kn_common::pty_trait::PtyOutputSink;
use tokio::sync::mpsc;

/// WSS 输出 — PTY 数据包装为 output 消息推给云端。
///
/// `db_session_id` 是云端 DB 主键 (Long)，对齐 Java `handleOutput` 的 `to_session_id.asLong()`。
pub struct WssSink {
    pub db_session_id: i64,
    pub tx: mpsc::UnboundedSender<String>,
}

impl PtyOutputSink for WssSink {
    fn send(&self, data: &[u8]) -> Result<(), String> {
        if let Ok(text) = std::str::from_utf8(data) {
            let msg = WsMessageBuilder::output(self.db_session_id, text);
            self.tx.send(msg).map_err(|e| e.to_string())
        } else {
            Ok(())
        }
    }
}

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
