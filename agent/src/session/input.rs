use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use tokio::sync::Notify;
use tracing;

/// 从 WSS/IPC 推送到指定会话的输入消息。
#[derive(Debug, Clone)]
pub struct InputMessage {
    pub session_id: String,
    pub text: String,
    /// 来源: "ios" / "local" / "desktop"
    pub source: String,
}

/// 每会话 FIFO 输入队列 + Notify 唤醒机制。
///
/// PTY stdin 写入循环通过 `register_session` 获取 `Arc<Notify>`，
/// 等待 `push` 触发后调用 `pop` 取出输入。
pub struct InputMerger {
    queues: tokio::sync::Mutex<HashMap<String, VecDeque<InputMessage>>>,
    notifies: tokio::sync::Mutex<HashMap<String, Arc<Notify>>>,
}

impl InputMerger {
    pub fn new() -> Self {
        Self {
            queues: tokio::sync::Mutex::new(HashMap::new()),
            notifies: tokio::sync::Mutex::new(HashMap::new()),
        }
    }

    /// 将消息入队并通知等待的 PTY stdin 循环。
    pub async fn push(&self, msg: InputMessage) {
        let sid = msg.session_id.clone();
        let len = msg.text.len();
        let preview = if msg.text.chars().count() <= 50 { msg.text.clone() } else { format!("{}...", msg.text.chars().take(50).collect::<String>()) };
        tracing::info!(session_id = %sid, len = len, text = %preview, "📥 [MERGER] 消息入队");
        self.queues.lock().await.entry(sid.clone()).or_default().push_back(msg);
        // 如果该会话有注册的 Notify，唤醒它
        if let Some(notify) = self.notifies.lock().await.get(&sid) {
            notify.notify_one();
            tracing::debug!(session_id = %sid, "🔔 [MERGER] 已唤醒 stdin writer");
        }
    }

    /// 从指定会话的队列中取出一条消息（FIFO）。
    pub async fn pop(&self, session_id: &str) -> Option<InputMessage> {
        self.queues.lock().await.get_mut(session_id)?.pop_front()
    }

    /// 为会话注册一个 Notify，供 PTY stdin 循环等待。
    /// 返回 `Arc<Notify>`，调用方可以 `notify.notified().await` 阻塞等待。
    pub async fn register_session(&self, session_id: &str) -> Arc<Notify> {
        let notify = Arc::new(Notify::new());
        self.notifies
            .lock()
            .await
            .insert(session_id.to_string(), notify.clone());
        notify
    }

    /// 取消注册会话（清理队列和 Notify）。
    pub async fn unregister_session(&self, session_id: &str) {
        self.queues.lock().await.remove(session_id);
        self.notifies.lock().await.remove(session_id);
    }
}

// ── OutputFanout ───────────────────────────────────────────
