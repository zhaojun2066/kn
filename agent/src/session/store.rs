use crate::error::Result;
use crate::session::types::{ManagedSession, SessionStatus, SessionSummary};
use std::collections::HashMap;
use std::sync::atomic::Ordering;
use tokio::sync::RwLock;

// ── SessionStore trait ───────────────────────────────────────

/// 会话存储后端抽象。
/// Phase 1: MemorySessionStore
/// Phase 2: 可添加 DiskSessionStore（checkpoint 持久化）
#[async_trait::async_trait]
pub trait SessionStore: Send + Sync {
    /// 插入新会话。
    async fn insert(&self, session: ManagedSession) -> Result<()>;
    /// 删除会话并返回。
    async fn remove(&self, nid: &str) -> Result<Option<ManagedSession>>;
    /// 按 nanoid 查找会话。
    async fn get(&self, nid: &str) -> Result<Option<ManagedSession>>;
    /// 按 DB ID 查找会话。
    /// 列出所有会话摘要。
    async fn list(&self) -> Result<Vec<SessionSummary>>;
    /// 活跃会话数量（非 Ended 状态）。
    async fn count_active(&self) -> Result<usize>;
    /// 总会话数量（含 Ended）。
    async fn count_total(&self) -> Result<usize>;
    /// 已开启远程控制的会话数量。
    async fn count_remote_enabled(&self) -> Result<usize>;
}

// ── MemorySessionStore ───────────────────────────────────────

/// Phase 1 内存存储实现。
pub struct MemorySessionStore {
    sessions: RwLock<HashMap<String, ManagedSession>>,
}

impl MemorySessionStore {
    pub fn new() -> Self {
        Self {
            sessions: RwLock::new(HashMap::new()),
        }
    }
}

#[async_trait::async_trait]
impl SessionStore for MemorySessionStore {
    async fn insert(&self, session: ManagedSession) -> Result<()> {
        let nid = session.nid.clone();

        self.sessions.write().await.insert(nid.clone(), session);
        Ok(())
    }

    async fn remove(&self, nid: &str) -> Result<Option<ManagedSession>> {
        let session = self.sessions.write().await.remove(nid);
        Ok(session)
    }

    async fn get(&self, nid: &str) -> Result<Option<ManagedSession>> {
        Ok(self.sessions.read().await.get(nid).cloned())
    }

    async fn list(&self) -> Result<Vec<SessionSummary>> {
        let sessions = self.sessions.read().await;
        let mut summaries: Vec<SessionSummary> = sessions
            .values()
            .map(|s| SessionSummary {
                nid: s.nid.clone(),
                kind: s.kind,
                source: s.source.clone(),
                tool: s.tool.clone(),
                profile: s.profile.clone(),
                cwd: s.cwd.clone(),
                cols: s.cols,
                rows: s.rows,
                viewport_owner: s.viewport_owner,
                created_at: s.created_at,
                status: s.status,
                remote_enabled: s.remote_enabled.load(Ordering::Relaxed),
                display_summary: s
                    .display_summary
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .clone(),
            })
            .collect();
        summaries.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        Ok(summaries)
    }

    async fn count_active(&self) -> Result<usize> {
        Ok(self
            .sessions
            .read()
            .await
            .values()
            .filter(|s| s.status != SessionStatus::Ended)
            .count())
    }

    async fn count_total(&self) -> Result<usize> {
        Ok(self.sessions.read().await.len())
    }

    async fn count_remote_enabled(&self) -> Result<usize> {
        Ok(self
            .sessions
            .read()
            .await
            .values()
            .filter(|s| {
                s.status != SessionStatus::Ended && s.remote_enabled.load(Ordering::Relaxed)
            })
            .count())
    }
}

// ── InputMerger ────────────────────────────────────────────
