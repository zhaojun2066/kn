//! ACK 基础设施 — WSS 层 request-response 确认机制。
//!
//! Agent 发送 `session_created` 后，通过 `AckRegistry` 注册一个 oneshot channel，
//! 等待 WSS 返回 `session_created_ack`。支持：
//! - 同步等待（Desktop 开启远程）
//! - 带重试的等待（iOS 创建会话，3 次重试）
//! - 重连重同步（WSS 重连后重新确认）

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{oneshot, Mutex};

/// ACK 结果：成功或带错误消息的失败。
#[derive(Debug, Clone)]
pub enum AckResult {
    /// cloud 确认成功
    Ok,
    /// cloud 返回明确错误
    Error(String),
}

/// ACK 注册表。以 session_nid 为键，存储等待 ACK 的 oneshot sender。
///
/// - `register()`: 注册等待，返回 receiver。重复注册会替换旧的 sender。
/// - `resolve()`: 唤醒等待者，返回是否成功找到（已过期则返回 false）。
pub struct AckRegistry {
    pending: Arc<Mutex<HashMap<String, oneshot::Sender<AckResult>>>>,
}

impl AckRegistry {
    pub fn new() -> Self {
        Self {
            pending: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// 注册一个待确认的 session_nid。返回 oneshot receiver 供调用方等待。
    /// 如果同一个 nid 已有注册（如重试场景），旧的 sender 会被替换（丢弃）。
    pub async fn register(&self, session_nid: &str) -> oneshot::Receiver<AckResult> {
        let (tx, rx) = oneshot::channel();
        self.pending
            .lock()
            .await
            .insert(session_nid.to_string(), tx);
        rx
    }

    /// 解析待确认的 session_nid。调用方（handle_incoming）收到 session_created_ack 时调用。
    /// 返回 true 表示成功唤醒等待者，false 表示没有等待者（已超时或未注册）。
    pub async fn resolve(&self, session_nid: &str, result: AckResult) -> bool {
        if let Some(tx) = self.pending.lock().await.remove(session_nid) {
            let _ = tx.send(result);
            true
        } else {
            false
        }
    }
}

// ── Tests ──────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_register_and_resolve_ok() {
        let registry = AckRegistry::new();
        let rx = registry.register("s_test123").await;

        let resolved = registry.resolve("s_test123", AckResult::Ok).await;
        assert!(resolved);

        let result = rx.await.unwrap();
        match result {
            AckResult::Ok => {}
            _ => panic!("expected Ok"),
        }
    }

    #[tokio::test]
    async fn test_register_and_resolve_error() {
        let registry = AckRegistry::new();
        let rx = registry.register("s_test456").await;

        registry
            .resolve("s_test456", AckResult::Error("test error".into()))
            .await;

        match rx.await.unwrap() {
            AckResult::Error(msg) => assert_eq!(msg, "test error"),
            _ => panic!("expected Error"),
        }
    }

    #[tokio::test]
    async fn test_resolve_without_register_returns_false() {
        let registry = AckRegistry::new();
        let resolved = registry.resolve("s_nonexistent", AckResult::Ok).await;
        assert!(!resolved);
    }

    #[tokio::test]
    async fn test_reregister_replaces_old() {
        let registry = AckRegistry::new();
        let _rx1 = registry.register("s_dup").await;
        // 第二次 register 替换了第一次的 sender
        let rx2 = registry.register("s_dup").await;

        registry.resolve("s_dup", AckResult::Ok).await;

        // rx1 的 sender 已被替换，它的 receiver 返回 Cancelled
        // rx2 正常收到
        let result = rx2.await.unwrap();
        match result {
            AckResult::Ok => {}
            _ => panic!("expected Ok"),
        }
    }

    #[tokio::test]
    async fn test_drop_receiver_timeout() {
        // 模拟超时：register 拿到 rx，但 drop rx 后 resolve 仍可以 send（tx 端不报错）
        let registry = AckRegistry::new();
        let rx = registry.register("s_timeout").await;
        drop(rx); // 模拟超时——调用方不再等待

        // resolve 应该成功发送（因为没有 receiver 了，tx.send 返回 Err，但我们忽略）
        let resolved = registry.resolve("s_timeout", AckResult::Ok).await;
        assert!(resolved);
    }
}
