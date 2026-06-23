//! 会话管理器 — SessionStore trait + MemorySessionStore + SessionManager。
//!
//! 管理 AI CLI 工具会话的生命周期。Phase 1 使用内存存储，
//! trait 抽象允许 Phase 2 轻松切换到持久化存储。

use crate::error::{AgentError, Result};
use crate::proto::WsMessageBuilder;
use crate::state::{AgentState, StateMachine};
use chrono::{DateTime, Utc};
use portable_pty::PtySystem;
use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixListener;
use tokio::sync::mpsc;
use tokio::sync::Notify;
use tokio::sync::RwLock;

// ── Session types ────────────────────────────────────────────

/// 会话状态
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionStatus {
    Created,
    Running,
    Ended,
}

/// 受管理的 AI CLI 会话。
#[derive(Debug, Clone)]
pub struct ManagedSession {
    /// 会话 nanoid（s_ + 12 字符），wire 标识符
    pub nid: String,
    /// 云端 DB 会话 ID（收到 start_session 后设置）
    pub db_id: Option<i64>,
    /// CLI 工具类型
    pub tool: String,
    /// Profile 名称
    pub profile: Option<String>,
    /// 工作目录
    pub cwd: String,
    /// 终端列数
    pub cols: u16,
    /// 终端行数
    pub rows: u16,
    /// 创建时间
    pub created_at: DateTime<Utc>,
    /// 当前状态
    pub status: SessionStatus,
    /// 最近的用户输入（截断至 200 字符，供 checkpoint 使用）
    pub last_input: Arc<std::sync::Mutex<String>>,
    /// 最近的 PTY 输出片段（截断至 500 字符，供 checkpoint 使用）
    pub last_output_snippet: Arc<std::sync::Mutex<String>>,
}

impl ManagedSession {
    /// 记录最近的用户输入（截断至 200 字符）。
    pub fn record_input(&self, text: &str) {
        let truncated: String = text.chars().take(200).collect();
        *self.last_input.lock().unwrap_or_else(|e| e.into_inner()) = truncated;
    }

    /// 记录最近的 PTY 输出片段（截断至 500 字符）。
    pub fn record_output_snippet(&self, text: &str) {
        let truncated: String = text.chars().take(500).collect();
        *self.last_output_snippet.lock().unwrap_or_else(|e| e.into_inner()) = truncated;
    }

    /// 获取最近的用户输入。
    pub fn last_input(&self) -> String {
        self.last_input.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    /// 获取最近的 PTY 输出片段。
    pub fn last_output_snippet(&self) -> String {
        self.last_output_snippet.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }
}

/// 会话摘要（用于列表展示）。
#[derive(Debug, Clone)]
pub struct SessionSummary {
    pub nid: String,
    pub db_id: Option<i64>,
    pub tool: String,
    pub profile: Option<String>,
    pub cwd: String,
    pub cols: u16,
    pub rows: u16,
    pub created_at: DateTime<Utc>,
    pub status: SessionStatus,
}

// ── PTY handle for attach ────────────────────────────────────

/// 存储 PTY writer + OutputFanout，供 `attach_pty` 桥接。
pub(crate) struct PtyAttachHandle {
    pub writer: Arc<tokio::sync::Mutex<Box<dyn std::io::Write + Send>>>,
    pub fanout: OutputFanout,
}

/// 计算 per-session PTY proxy socket 路径。
pub fn pty_sock_path(nid: &str) -> PathBuf {
    kn_common::path::agent_dir()
        .join("sessions")
        .join(nid)
        .join("pty.sock")
}

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
    async fn get_by_db_id(&self, db_id: i64) -> Result<Option<ManagedSession>>;
    /// 列出所有会话摘要。
    async fn list(&self) -> Result<Vec<SessionSummary>>;
    /// 活跃会话数量（非 Ended 状态）。
    async fn count_active(&self) -> Result<usize>;
    /// 总会话数量（含 Ended）。
    async fn count_total(&self) -> Result<usize>;
}

// ── MemorySessionStore ───────────────────────────────────────

/// Phase 1 内存存储实现。
pub struct MemorySessionStore {
    sessions: RwLock<HashMap<String, ManagedSession>>,
    /// db_id → nid 索引，用于按 DB ID 快速查找
    db_index: RwLock<HashMap<i64, String>>,
}

impl MemorySessionStore {
    pub fn new() -> Self {
        Self {
            sessions: RwLock::new(HashMap::new()),
            db_index: RwLock::new(HashMap::new()),
        }
    }
}

#[async_trait::async_trait]
impl SessionStore for MemorySessionStore {
    async fn insert(&self, session: ManagedSession) -> Result<()> {
        let nid = session.nid.clone();
        let db_id = session.db_id;

        // 先写 sessions，再写 db_index（保证引用目标先于索引存在）
        self.sessions.write().await.insert(nid.clone(), session);
        if let Some(id) = db_id {
            self.db_index.write().await.insert(id, nid);
        }
        Ok(())
    }

    async fn remove(&self, nid: &str) -> Result<Option<ManagedSession>> {
        let session = self.sessions.write().await.remove(nid);
        if let Some(ref s) = session {
            if let Some(id) = s.db_id {
                self.db_index.write().await.remove(&id);
            }
        }
        Ok(session)
    }

    async fn get(&self, nid: &str) -> Result<Option<ManagedSession>> {
        Ok(self.sessions.read().await.get(nid).cloned())
    }

    async fn get_by_db_id(&self, db_id: i64) -> Result<Option<ManagedSession>> {
        let nid = self.db_index.read().await.get(&db_id).cloned();
        match nid {
            Some(nid) => self.get(&nid).await,
            None => Ok(None),
        }
    }

    async fn list(&self) -> Result<Vec<SessionSummary>> {
        let sessions = self.sessions.read().await;
        let mut summaries: Vec<SessionSummary> = sessions
            .values()
            .map(|s| SessionSummary {
                nid: s.nid.clone(),
                db_id: s.db_id,
                tool: s.tool.clone(),
                profile: s.profile.clone(),
                cwd: s.cwd.clone(),
                cols: s.cols,
                rows: s.rows,
                created_at: s.created_at,
                status: s.status,
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
}

// ── InputMerger ────────────────────────────────────────────

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
        self.queues.lock().await.entry(sid.clone()).or_default().push_back(msg);
        // 如果该会话有注册的 Notify，唤醒它
        if let Some(notify) = self.notifies.lock().await.get(&sid) {
            notify.notify_one();
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

/// PTY 输出扇出到 WSS + IPC，带 100ms/64KB 批处理和 10KB 分块。
///
/// `broadcast()` 由 PTY reader 在 `spawn_blocking` 上下文中调用，
/// buffer 使用 `std::sync::Mutex`（锁持有时间极短）。
#[derive(Clone)]
pub struct OutputFanout {
    inner: Arc<OutputFanoutInner>,
    cancel: tokio_util::sync::CancellationToken,
}

struct OutputFanoutInner {
    wss_tx: Option<mpsc::UnboundedSender<String>>,
    ipc_tx: Option<mpsc::UnboundedSender<String>>,
    db_session_id: i64,
    buffer: std::sync::Mutex<Vec<u8>>,
    /// 额外的 output subscriber（供 attach_pty 注册）
    extra_subscribers: std::sync::Mutex<Vec<mpsc::UnboundedSender<Vec<u8>>>>,
}

impl OutputFanout {
    /// 创建 OutputFanout 并启动 100ms 定时 flush 任务。
    /// `cancel` 用于停止定时器（session 结束时触发）。
    ///
    /// `db_session_id` 是云端 DB 主键，对齐 Java `handleOutput` 中预期的 Long 类型。
    pub fn new(
        db_session_id: i64,
        wss: Option<mpsc::UnboundedSender<String>>,
        ipc: Option<mpsc::UnboundedSender<String>>,
        cancel: tokio_util::sync::CancellationToken,
    ) -> Self {
        let inner = Arc::new(OutputFanoutInner {
            wss_tx: wss,
            ipc_tx: ipc,
            db_session_id,
            buffer: std::sync::Mutex::new(Vec::new()),
            extra_subscribers: std::sync::Mutex::new(Vec::new()),
        });

        let inner_clone = inner.clone();
        let timer_cancel = cancel.clone();
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = tokio::time::sleep(Duration::from_millis(100)) => {
                        let (data, subscribers) = {
                            let mut buf = inner_clone.buffer.lock().unwrap_or_else(|e| e.into_inner());
                            if buf.is_empty() {
                                (Vec::new(), Vec::new())
                            } else {
                                let data = std::mem::take(&mut *buf);
                                // Clone subscriber list for flushing outside the lock
                                let subs = inner_clone.extra_subscribers.lock().unwrap_or_else(|e| e.into_inner()).clone();
                                (data, subs)
                            }
                        };
                        if !data.is_empty() {
                            // Send to extra subscribers first (raw bytes, before data is moved)
                            for tx in &subscribers {
                                let _ = tx.send(data.clone());
                            }
                            Self::flush_chunked(
                                inner_clone.db_session_id,
                                data,
                                inner_clone.wss_tx.clone(),
                                inner_clone.ipc_tx.clone(),
                            );
                        }
                    }
                    _ = timer_cancel.cancelled() => break,
                }
            }
        });

        OutputFanout { inner, cancel }
    }

    /// 注册额外的 output subscriber（供 attach_pty 使用）。
    /// 返回 receiver，调用方应持续读取并转发到客户端。
    pub fn register_subscriber(&self) -> mpsc::UnboundedReceiver<Vec<u8>> {
        let (tx, rx) = mpsc::unbounded_channel();
        self.inner.extra_subscribers.lock().unwrap_or_else(|e| e.into_inner()).push(tx);
        rx
    }

    /// 返回 session 的取消令牌（用于停止 stdin writer 等）。
    pub fn cancel_token(&self) -> tokio_util::sync::CancellationToken {
        self.cancel.clone()
    }

    /// PTY reader 调用此方法追加输出数据。
    ///
    /// 来自 `spawn_blocking` 上下文（同步），使用 `std::sync::Mutex`。
    /// 缓冲区达到 64KB 时立即 flush，否则等待 100ms 定时器。
    pub fn broadcast(&self, data: &[u8]) {
        let mut buf = self.inner.buffer.lock().unwrap_or_else(|e| e.into_inner());
        buf.extend_from_slice(data);
        if buf.len() >= 64 * 1024 {
            let data = std::mem::take(&mut *buf);
            drop(buf); // 释放锁后再 flush
            let wss = self.inner.wss_tx.clone();
            let ipc = self.inner.ipc_tx.clone();
            let db_id = self.inner.db_session_id;
            Self::flush_chunked(db_id, data, wss, ipc);
        }
    }

    /// 将数据按 10KB 分块，分别发送到 WSS 和 IPC 通道。
    fn flush_chunked(
        db_session_id: i64,
        data: Vec<u8>,
        wss_tx: Option<mpsc::UnboundedSender<String>>,
        ipc_tx: Option<mpsc::UnboundedSender<String>>,
    ) {
        const CHUNK_SIZE: usize = 10 * 1024; // 10KB
        for chunk in data.chunks(CHUNK_SIZE) {
            let text = String::from_utf8_lossy(chunk);
            if let Some(ref tx) = wss_tx {
                let msg = WsMessageBuilder::output(db_session_id, &text);
                let _ = tx.send(msg);
            }
            if let Some(ref tx) = ipc_tx {
                let _ = tx.send(text.to_string());
            }
        }
    }
}

// ── SessionManager ──────────────────────────────────────────

/// 会话编排器。管理会话生命周期并通知状态机。
pub struct SessionManager {
    store: Box<dyn SessionStore>,
    /// session_id → child PID 映射，用于 kill_session
    child_pids: tokio::sync::Mutex<HashMap<String, u32>>,
    /// session_id → PTY writer + OutputFanout，供 attach_pty 使用
    attach_handles: tokio::sync::Mutex<HashMap<String, PtyAttachHandle>>,
}

impl SessionManager {
    pub fn new(store: Box<dyn SessionStore>) -> Self {
        Self {
            store,
            child_pids: tokio::sync::Mutex::new(HashMap::new()),
            attach_handles: tokio::sync::Mutex::new(HashMap::new()),
        }
    }

    /// 创建新会话（收到 start_session 后调用）。
    pub async fn create(
        &self,
        nid: String,
        db_id: i64,
        tool: String,
        profile: Option<String>,
        cwd: String,
    ) -> Result<ManagedSession> {
        let session = ManagedSession {
            nid: nid.clone(),
            db_id: Some(db_id),
            tool,
            profile,
            cwd,
            cols: 80,
            rows: 24,
            created_at: Utc::now(),
            status: SessionStatus::Created,
            last_input: Arc::new(std::sync::Mutex::new(String::new())),
            last_output_snippet: Arc::new(std::sync::Mutex::new(String::new())),
        };

        self.store.insert(session.clone()).await?;
        tracing::info!(nid = %nid, db_id = %db_id, "会话已创建");
        Ok(session)
    }

    /// 标记会话为运行中。
    pub async fn mark_running(&self, nid: &str) -> Result<()> {
        let mut session = self
            .store
            .get(nid)
            .await?
            .ok_or_else(|| AgentError::SessionNotFound(nid.to_string()))?;
        session.status = SessionStatus::Running;
        self.store.insert(session).await?;
        Ok(())
    }

    /// 结束会话。
    pub async fn end(&self, nid: &str) -> Result<Option<ManagedSession>> {
        let mut session = match self.store.get(nid).await? {
            Some(s) => s,
            None => return Ok(None),
        };
        session.status = SessionStatus::Ended;
        self.store.insert(session.clone()).await?;
        tracing::info!(nid = %nid, "会话已结束");
        Ok(Some(session))
    }

    /// 强制终止会话（SIGKILL + 清理）。
    pub async fn kill_session(&self, nid: &str) -> Result<()> {
        tracing::info!(nid = %nid, "强制终止会话");

        // Kill PTY child process by PID
        if let Some(pid) = self.child_pids.lock().await.remove(nid) {
            #[cfg(unix)]
            unsafe {
                libc::kill(pid as i32, libc::SIGKILL);
            }
            tracing::info!(pid = pid, "已终止子进程");
        }

        // 清理 attach handle + proxy socket
        self.attach_handles.lock().await.remove(nid);
        let sock = pty_sock_path(nid);
        if sock.exists() {
            let _ = std::fs::remove_file(&sock);
        }

        let _ = self.end(nid).await;
        Ok(())
    }

    /// 存储 PTY writer + OutputFanout，供后续 `attach_pty` 使用。
    pub(crate) async fn store_attach_handle(&self, nid: &str, handle: PtyAttachHandle) {
        self.attach_handles.lock().await.insert(nid.to_string(), handle);
    }

    /// 创建 pty.sock 并桥接 PTY I/O，返回 socket 路径。
    ///
    /// 输出方向：订阅 OutputFanout → pty.sock（不走 PTY dup，避免分食输出）
    /// 输入方向：pty.sock → PTY writer
    pub async fn attach_pty(&self, nid: &str) -> std::result::Result<PathBuf, String> {
        let sock_path = pty_sock_path(nid);

        // 确保目录存在
        if let Some(parent) = sock_path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("mkdir: {}", e))?;
        }

        // 清理旧 socket
        if sock_path.exists() {
            std::fs::remove_file(&sock_path).map_err(|e| format!("remove old sock: {}", e))?;
        }

        let listener = UnixListener::bind(&sock_path)
            .map_err(|e| format!("bind pty.sock: {}", e))?;

        // 权限 0600
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&sock_path, std::fs::Permissions::from_mode(0o600));
        }

        let mut handles = self.attach_handles.lock().await;
        let handle = handles.remove(nid).ok_or_else(|| "session not found".to_string())?;
        drop(handles);

        // 从 OutputFanout 订阅输出（不 dup PTY reader，避免分食问题）
        let mut output_rx = handle.fanout.register_subscriber();

        let sid = nid.to_string();
        tokio::spawn(async move {
            let (stream, _addr) = match listener.accept().await {
                Ok(c) => c,
                Err(e) => {
                    tracing::warn!(session_id = %sid, error = %e, "pty.sock accept failed");
                    return;
                }
            };
            drop(listener);

            let (mut sock_reader, mut sock_writer) = stream.into_split();

            // OutputFanout → pty.sock（后台 task）
            let sid_clone = sid.clone();
            tokio::spawn(async move {
                while let Some(data) = output_rx.recv().await {
                    if sock_writer.write_all(&data).await.is_err() { break; }
                }
                tracing::debug!(session_id = %sid_clone, "output→socket writer exited");
            });

            // pty.sock → PTY writer（当前 task，连接断开即退出）
            let pty_writer = handle.writer;
            let mut buf = vec![0u8; 16384];
            loop {
                match sock_reader.read(&mut buf).await {
                    Ok(0) => break,
                    Ok(n) => {
                        let mut w = pty_writer.lock().await;
                        use std::io::Write;
                        if w.write_all(&buf[..n]).is_err() { break; }
                    }
                    Err(_) => break,
                }
            }
            tracing::debug!(session_id = %sid, "PTY proxy client disconnected");
        });

        Ok(sock_path)
    }

    /// 删除会话。
    pub async fn remove(&self, nid: &str) -> Result<Option<ManagedSession>> {
        self.store.remove(nid).await
    }

    /// 获取会话。
    pub async fn get(&self, nid: &str) -> Result<Option<ManagedSession>> {
        self.store.get(nid).await
    }

    /// 按 DB ID 获取会话。
    pub async fn get_by_db_id(&self, db_id: i64) -> Result<Option<ManagedSession>> {
        self.store.get_by_db_id(db_id).await
    }

    /// 列出所有会话。
    pub async fn list(&self) -> Result<Vec<SessionSummary>> {
        self.store.list().await
    }

    /// 活跃会话数量（非 Ended 状态）。
    pub async fn active_count(&self) -> Result<usize> {
        self.store.count_active().await
    }

    /// 获取所有会话 nanoid 列表。
    pub async fn all_nids(&self) -> Result<Vec<String>> {
        let summaries = self.store.list().await?;
        Ok(summaries.into_iter().map(|s| s.nid).collect())
    }

    /// 更新终端尺寸。
    pub async fn resize(&self, nid: &str, cols: u16, rows: u16) -> Result<()> {
        let mut session = self
            .store
            .get(nid)
            .await?
            .ok_or_else(|| AgentError::SessionNotFound(nid.to_string()))?;
        session.cols = cols;
        session.rows = rows;
        self.store.insert(session).await?;
        Ok(())
    }

    // ── PTY session lifecycle ────────────────────────────────

    /// 创建 PTY 会话并启动 CLI 进程。返回 OutputFanout 用于接收 PTY 输出。
    pub async fn start_session(
        &self,
        nid: &str,
        tool: &str,
        profile: Option<&str>,
        cwd: &str,
        cols: u16,
        rows: u16,
        wss_tx: mpsc::UnboundedSender<String>,
        ipc_tx: mpsc::UnboundedSender<String>,
        merger: std::sync::Arc<InputMerger>,
    ) -> std::result::Result<OutputFanout, String> {
        // 1. 查找 CLI 二进制
        let binary = resolve_tool_path(tool)?;

        // 2. 读 profile env vars
        let env_vars = if let Some(p) = profile {
            match kn_common::profile::get_env_cmd(p) {
                Ok(v) => Some(v.env),
                Err(e) => {
                    let _ = wss_tx.send(serde_json::json!({
                        "type": "error_notify",
                        "data": { "code": "config_parse_error", "message": format!("{}", e) }
                    }).to_string());
                    return Err(format!("config_parse_error: {}", e));
                }
            }
        } else {
            None
        };

        // 3. Tool 预处理
        let prep = prepare_tool_env(tool, &env_vars)?;

        // 4. openpty
        let pty_system = portable_pty::NativePtySystem::default();
        let size = portable_pty::PtySize { rows, cols, pixel_width: 0, pixel_height: 0 };
        let pair = pty_system.openpty(size)
            .map_err(|e| {
                let _ = wss_tx.send(serde_json::json!({
                    "type": "error_notify",
                    "data": { "code": "pty_alloc_failed", "message": format!("{}", e) }
                }).to_string());
                format!("pty_alloc_failed: {}", e)
            })?;

        // 5. spawn shell + CLI
        let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".into());
        let mut cmd = portable_pty::CommandBuilder::new(&shell);
        cmd.args(["-i", "-l"]);
        if !cwd.is_empty() { cmd.cwd(cwd); }

        for (k, v) in std::env::vars() { cmd.env(&k, &v); }
        if let Some(ref ev) = env_vars {
            for (k, v) in ev { cmd.env(k, v); }
        }
        // PATH 补齐 + TERM
        if cfg!(target_os = "macos") {
            let current_path = std::env::var("PATH").unwrap_or_default();
            let extra = ["/opt/homebrew/bin", "/opt/homebrew/sbin", "/usr/local/bin", "/usr/local/sbin"];
            let missing: Vec<&str> = extra.iter()
                .filter(|p| !current_path.split(':').any(|seg| seg == **p))
                .copied().collect();
            if !missing.is_empty() {
                cmd.env("PATH", format!("{}:{}", current_path, missing.join(":")));
            }
        }
        cmd.env("TERM", "xterm-256color");
        cmd.env("COLORTERM", "truecolor");
        cmd.env("TERM_PROGRAM", "kn");
        if std::env::var_os("LANG").is_none() { cmd.env("LANG", "en_US.UTF-8"); }

        // 构建 CLI 命令行: <binary> [--settings tmp.json] ...
        cmd.arg(&binary);
        for arg in &prep.extra_args { cmd.arg(arg); }

        let mut child = pair.slave.spawn_command(cmd)
            .map_err(|e| {
                let _ = wss_tx.send(serde_json::json!({
                    "type": "error_notify",
                    "data": { "code": "shell_spawn_failed", "message": format!("{}", e) }
                }).to_string());
                format!("shell_spawn_failed: {}", e)
            })?;

        drop(pair.slave);

        // 6. 创建 I/O 通道 + session 生命周期令牌
        let session_cancel = tokio_util::sync::CancellationToken::new();
        let mut reader = pair.master.try_clone_reader()
            .map_err(|e| format!("clone reader: {}", e))?;
        let writer: std::sync::Arc<tokio::sync::Mutex<Box<dyn std::io::Write + Send>>> =
            std::sync::Arc::new(tokio::sync::Mutex::new(Box::new(
                pair.master.take_writer().map_err(|e| format!("take writer: {}", e))?,
            )));

        // 7. OutputFanout（带取消令牌，session 结束后停止定时器）
        // 使用 DB session ID (Long) 对齐 Java handleOutput 期望的 to_session_id.asLong()
        let db_id = self
            .get(nid)
            .await
            .ok()
            .flatten()
            .and_then(|s| s.db_id)
            .unwrap_or(0);
        let fanout = OutputFanout::new(
            db_id,
            Some(wss_tx),
            Some(ipc_tx),
            session_cancel.clone(),
        );
        self.mark_running(nid).await.map_err(|e| e.to_string())?;

        // 存储 fanout + writer，供 attach_pty 使用
        self.store_attach_handle(nid, PtyAttachHandle {
            writer: writer.clone(),
            fanout: fanout.clone(),
        }).await;

        // 8. PTY stdout 读取线程（spawn_blocking）+ child 回收 (B1)
        let child_pid = child.process_id().unwrap_or_else(|| {
            tracing::error!(session_id = %nid, "PTY child has no PID — kill_session will be a no-op");
            0
        });
        let fanout_clone = fanout.clone();
        let reader_cancel = session_cancel.clone();
        let sid = nid.to_string();
        tokio::task::spawn_blocking(move || {
            let mut buf = [0u8; 16384];
            let result = loop {
                match reader.read(&mut buf) {
                    Ok(0) => break Ok(()),
                    Ok(n) => { fanout_clone.broadcast(&buf[..n]); }
                    Err(e) => break Err(e),
                }
            };
            // 等待子进程退出（回收僵尸进程）
            match child.wait() {
                Ok(status) => tracing::info!(session_id=%sid, exit_code=%status.exit_code(), "PTY 进程已退出"),
                Err(e) => tracing::warn!(session_id=%sid, error=%e, "PTY wait 失败"),
            }
            match result {
                Ok(()) => tracing::info!(session_id=%sid, "PTY EOF"),
                Err(e) => tracing::warn!(session_id=%sid, error=%e, "PTY read error"),
            }
            reader_cancel.cancel();
        });

        // 9. PTY stdin 写入循环（B2：session_cancel 时退出）
        let notify = merger.register_session(nid).await;
        let writer_clone = writer.clone();
        let writer_cancel = session_cancel.clone();
        let sid = nid.to_string();
        let merger_clone = merger.clone();
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = notify.notified() => {
                        while let Some(msg) = merger_clone.pop(&sid).await {
                            let mut w = writer_clone.lock().await;
                            let _ = w.write_all(msg.text.as_bytes());
                        }
                    }
                    _ = writer_cancel.cancelled() => {
                        tracing::debug!(session_id=%sid, "stdin writer 退出");
                        break;
                    }
                }
            }
            merger_clone.unregister_session(&sid).await;
        });

        // 存储 PID 供 kill_session 使用（跳过 0，防止误杀进程组）
        if child_pid > 0 {
            self.child_pids.lock().await.insert(nid.to_string(), child_pid);
        }

        Ok(fanout)
    }

    // ── Checkpoint ──────────────────────────────────────────

    /// 原子写入 per-session checkpoint JSON 到
    /// `~/.kn/agent/sessions/{nid}/checkpoint.json`。
    pub async fn save_checkpoint(&self, nid: &str) -> std::result::Result<(), String> {
        let session = self
            .store
            .get(nid)
            .await
            .map_err(|e| format!("{}", e))?
            .ok_or_else(|| format!("session not found: {}", nid))?;

        let status_str = match session.status {
            SessionStatus::Created => "created",
            SessionStatus::Running => "running",
            SessionStatus::Ended => "ended",
        };
        let checkpoint = serde_json::json!({
            "_format": 1,
            "nid": session.nid,
            "db_id": session.db_id,
            "tool": session.tool,
            "profile": session.profile,
            "cwd": session.cwd,
            "cols": session.cols,
            "rows": session.rows,
            "created_at": session.created_at.to_rfc3339(),
            "status": status_str,
            "last_input": session.last_input(),
            "last_output_snippet": session.last_output_snippet(),
        });

        let path = checkpoint_path(nid);
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| format!("create dir: {}", e))?;
        }

        let json =
            serde_json::to_string_pretty(&checkpoint).map_err(|e| format!("serialize: {}", e))?;

        // 原子写入: tmp → fsync → rename
        let tmp = path.with_extension("tmp");
        tokio::fs::write(&tmp, &json).await.map_err(|e| format!("write: {}", e))?;
        // fsync (use tokio::fs::File for async)
        if let Ok(f) = tokio::fs::File::open(&tmp).await {
            let _ = f.sync_all().await;
        }
        tokio::fs::rename(&tmp, &path).await.map_err(|e| format!("rename: {}", e))?;

        tracing::debug!(nid = %nid, "checkpoint 已保存");
        Ok(())
    }

    /// 启动每 30 秒 checkpoint 循环。
    ///
    /// 仅在 Connected / Running / Idle 状态执行 checkpoint，
    /// Stopped 状态退出循环。
    pub fn start_checkpoint_loop(sm: Arc<SessionManager>, state: Arc<StateMachine>) {
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(30)).await;

                let current = state.current().await;
                match current {
                    AgentState::Connected | AgentState::Running | AgentState::Idle => {}
                    AgentState::Stopped => break,
                    _ => continue,
                }

                if let Ok(nids) = sm.all_nids().await {
                    for nid in &nids {
                        if let Err(e) = sm.save_checkpoint(nid).await {
                            tracing::warn!(nid = %nid, error = %e, "checkpoint 保存失败");
                        }
                    }
                }
            }
            tracing::info!("checkpoint 循环已退出");
        });
    }

}

/// 计算 per-session checkpoint 文件路径。
fn checkpoint_path(nid: &str) -> std::path::PathBuf {
    kn_common::path::agent_dir()
        .join("sessions")
        .join(nid)
        .join("checkpoint.json")
}

// ── Crash recovery: checkpoint loading ──────────────────────

/// Checkpoint 文件的反序列化格式（与 save_checkpoint 输出的 JSON 对应）。
#[derive(Debug, serde::Deserialize)]
struct CheckpointFile {
    nid: String,
    tool: String,
    #[serde(default)]
    profile: Option<String>,
    #[serde(default)]
    cwd: String,
    #[serde(default, rename = "last_input")]
    last_input: String,
    #[serde(default, rename = "last_output_snippet")]
    last_output_snippet: String,
}

/// 扫描 `~/.kn/agent/sessions/*/checkpoint.json`，返回中断会话列表。
///
/// 在 WSS 重连后调用，用于上报崩溃前正在进行的会话。
pub fn load_checkpoints() -> Vec<crate::proto::InterruptedSession> {
    let sessions_dir = kn_common::path::agent_dir().join("sessions");
    if !sessions_dir.exists() {
        return Vec::new();
    }

    let mut results = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&sessions_dir) {
        for entry in entries.flatten() {
            let cp_path = entry.path().join("checkpoint.json");
            if !cp_path.exists() {
                continue;
            }
            match std::fs::read_to_string(&cp_path) {
                Ok(json) => {
                    match serde_json::from_str::<CheckpointFile>(&json) {
                        Ok(cp) => {
                            results.push(crate::proto::InterruptedSession {
                                nid: cp.nid,
                                tool: cp.tool,
                                profile: cp.profile,
                                cwd: cp.cwd,
                                last_input: cp.last_input,
                                last_output_snippet: cp.last_output_snippet,
                            });
                        }
                        Err(e) => {
                            tracing::warn!(
                                path = %cp_path.display(),
                                error = %e,
                                "checkpoint 解析失败，跳过"
                            );
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!(
                        path = %cp_path.display(),
                        error = %e,
                        "checkpoint 读取失败，跳过"
                    );
                }
            }
        }
    }

    tracing::info!(count = results.len(), "已加载 checkpoint");

    results
}

/// 删除所有 checkpoint 目录（在成功上报 `session_interrupted` 后调用）。
pub fn cleanup_checkpoints() {
    let sessions_dir = kn_common::path::agent_dir().join("sessions");
    if sessions_dir.exists() {
        match std::fs::remove_dir_all(&sessions_dir) {
            Ok(()) => tracing::info!("checkpoint 已清理"),
            Err(e) => tracing::warn!(error = %e, "checkpoint 清理失败"),
        }
    }
}

// ── CLI Tool helpers ────────────────────────────────────────

/// 根据 tool 名称查找 CLI 二进制路径。
pub fn resolve_tool_path(tool: &str) -> std::result::Result<String, String> {
    let candidates: &[&str] = match tool {
        "claude" => &["claude"],
        "codex" => &["codex"],
        "qoder" => &["qoder", "codex"],
        "qoderclicn" => &["qoder", "codex"],
        "bash" => &["bash"],
        _ => return Err(format!("未知 tool: {}", tool)),
    };
    kn_common::path::find_binary(candidates)
        .ok_or_else(|| format!("未找到 {} 二进制", tool))
}

struct ToolPrep {
    extra_args: Vec<String>,
}

/// Tool 启动前预处理。
fn prepare_tool_env(
    tool: &str,
    _env_vars: &Option<std::collections::HashMap<String, String>>,
) -> std::result::Result<ToolPrep, String> {
    match tool {
        "claude" => {
            // Claude: 通过 --settings 注入 env vars（临时文件在 session end 时由 ToolCleanupGuard 删除）
            let tmp = std::env::temp_dir().join(format!(
                "kn-claude-{}-{}.json",
                std::process::id(),
                chrono::Utc::now().timestamp_millis()
            ));
            let settings = serde_json::json!({"env": _env_vars});
            std::fs::write(&tmp, serde_json::to_string(&settings).map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())?;
            Ok(ToolPrep {
                extra_args: vec!["--settings".into(), tmp.to_string_lossy().to_string()],
            })
        }
        "bash" | "codex" | "qoder" | "qoderclicn" => {
            // Bash / Codex / Qoder: 通过环境变量注入，无需额外参数
            Ok(ToolPrep { extra_args: vec![] })
        }
        _ => Ok(ToolPrep { extra_args: vec![] }),
    }
}

// ── Tests ───────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_manager() -> SessionManager {
        SessionManager::new(Box::new(MemorySessionStore::new()))
    }

    #[tokio::test]
    async fn test_create_and_get() {
        let mgr = make_manager();
        let session = mgr
            .create(
                "s_test123".into(),
                42,
                "claude".into(),
                Some("my-profile".into()),
                "/tmp".into(),
            )
            .await
            .unwrap();

        assert_eq!(session.nid, "s_test123");
        assert_eq!(session.db_id, Some(42));

        let found = mgr.get("s_test123").await.unwrap().unwrap();
        assert_eq!(found.tool, "claude");
    }

    #[tokio::test]
    async fn test_mark_running_and_end() {
        let mgr = make_manager();
        mgr.create("s_test".into(), 1, "claude".into(), None, "/tmp".into())
            .await
            .unwrap();

        mgr.mark_running("s_test").await.unwrap();
        assert_eq!(mgr.get("s_test").await.unwrap().unwrap().status, SessionStatus::Running);

        mgr.end("s_test").await.unwrap();
        assert_eq!(mgr.get("s_test").await.unwrap().unwrap().status, SessionStatus::Ended);
    }

    #[tokio::test]
    async fn test_remove() {
        let mgr = make_manager();
        mgr.create("s_test".into(), 1, "claude".into(), None, "/tmp".into())
            .await
            .unwrap();
        assert_eq!(mgr.active_count().await.unwrap(), 1);
        mgr.remove("s_test").await.unwrap();
        assert_eq!(mgr.active_count().await.unwrap(), 0);
    }

    #[tokio::test]
    async fn test_get_by_db_id() {
        let mgr = make_manager();
        mgr.create("s_abc".into(), 99, "codex".into(), None, "/tmp".into())
            .await
            .unwrap();
        let found = mgr.get_by_db_id(99).await.unwrap().unwrap();
        assert_eq!(found.nid, "s_abc");
    }

    #[tokio::test]
    async fn test_list() {
        let mgr = make_manager();
        mgr.create("s_a".into(), 1, "claude".into(), None, "/tmp".into())
            .await
            .unwrap();
        mgr.create("s_b".into(), 2, "codex".into(), None, "/tmp".into())
            .await
            .unwrap();
        let list = mgr.list().await.unwrap();
        assert_eq!(list.len(), 2);
    }

    #[tokio::test]
    async fn test_resize() {
        let mgr = make_manager();
        mgr.create("s_test".into(), 1, "claude".into(), None, "/tmp".into())
            .await
            .unwrap();
        mgr.resize("s_test", 120, 40).await.unwrap();
        let s = mgr.get("s_test").await.unwrap().unwrap();
        assert_eq!(s.cols, 120);
        assert_eq!(s.rows, 40);
    }
}
