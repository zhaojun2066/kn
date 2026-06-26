use crate::error::{AgentError, Result};
use crate::state::StateMachine;
use crate::session::env::{prepare_tool_env, resolve_tool_path};
use crate::session::input::InputMerger;
use crate::session::output::OutputFanout;
use crate::session::store::SessionStore;
use crate::session::types::{ManagedSession, SessionKind, SessionStatus, SessionSummary};
use super::{pty_sock_path, PtyAttachHandle};
use chrono::Utc;
use portable_pty::PtySystem;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixListener;
use tokio::sync::mpsc;
use tracing;

/// 会话编排器。管理会话生命周期并通知状态机。
pub struct SessionManager {
    store: Box<dyn SessionStore>,
    /// session_id → child PID 映射，用于 kill_session
    child_pids: tokio::sync::Mutex<HashMap<String, u32>>,
    /// session_id → PTY writer + OutputFanout，供 attach_pty 使用
    attach_handles: tokio::sync::Mutex<HashMap<String, PtyAttachHandle>>,
    /// 创建互斥锁：防止 check+insert 竞态导致超过 SESSION_LIMIT
    create_mutex: tokio::sync::Mutex<()>,
}

/// 全局会话数量上限
pub const SESSION_LIMIT: usize = 10;

impl SessionManager {
    pub fn new(store: Box<dyn SessionStore>) -> Self {
        Self {
            store,
            child_pids: tokio::sync::Mutex::new(HashMap::new()),
            attach_handles: tokio::sync::Mutex::new(HashMap::new()),
            create_mutex: tokio::sync::Mutex::new(()),
        }
    }

    /// 创建新会话（收到 start_session 后调用）。
    ///
    /// 内部持有 create_mutex，保证 count+insert 原子性，防止并发超限。
    /// 返回 `Err(AgentError::SessionLimit)` 当会话数已达 SESSION_LIMIT。
    pub async fn create(
        &self,
        nid: String,
        source: String,
        tool: String,
        profile: Option<String>,
        cwd: String,
        kind: SessionKind,
    ) -> Result<ManagedSession> {
        let _guard = self.create_mutex.lock().await;

        // Relay 会话不受 10 会话限制（本地 PTY 无限制）
        if kind == SessionKind::Native {
            let active = self.store.count_active().await?;
            if active >= SESSION_LIMIT {
                return Err(AgentError::SessionLimit {
                    current: active,
                    max: SESSION_LIMIT,
                });
            }
        }

        let session = ManagedSession {
            kind,
            nid: nid.clone(),
            source,
            tool,
            profile,
            cwd,
            cols: 80,
            rows: 24,
            created_at: Utc::now(),
            status: SessionStatus::Created,
            last_input: Arc::new(std::sync::Mutex::new(String::new())),
            last_output_snippet: Arc::new(std::sync::Mutex::new(String::new())),
            ended_reported: Arc::new(AtomicBool::new(false)),
            remote_enabled: Arc::new(AtomicBool::new(true)),
            relay_inputs: Arc::new(std::sync::Mutex::new(Vec::new())),
        };

        self.store.insert(session.clone()).await?;
        tracing::info!(nid = %nid, kind = ?kind, "会话已创建");
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

    /// 上报 session_ended 到云端，仅允许执行一次。
    pub async fn report_session_ended(
        &self,
        nid: &str,
        reason: &str,
    ) -> Result<Option<crate::proto::OutgoingMessage>> {
        let mut session = match self.store.get(nid).await? {
            Some(s) => s,
            None => return Ok(None),
        };
        if !session.mark_ended_reported() {
            return Ok(None);
        }

        let nid = session.nid.clone();

        session.status = SessionStatus::Ended;
        self.store.insert(session).await?;
        Ok(Some(crate::proto::OutgoingMessage::SessionEnded {
            session_nid: nid,
            reason: reason.to_string(),
        }))
    }

    /// 如果会话是 Ended 状态，重置为 Running（供 re-enable relay 使用）。
    pub async fn reactivate_if_ended(&self, nid: &str) -> Result<()> {
        if let Some(mut s) = self.store.get(nid).await? {
            if s.status == SessionStatus::Ended {
                s.status = SessionStatus::Running;
                self.store.insert(s).await?;
            }
        }
        Ok(())
    }

    /// 设置会话的远程控制开关。
    pub async fn set_remote_enabled(&self, nid: &str, enabled: bool) -> Result<()> {
        let session = self
            .store
            .get(nid)
            .await?
            .ok_or_else(|| AgentError::SessionNotFound(nid.to_string()))?;
        session.remote_enabled.store(enabled, Ordering::Relaxed);
        self.store.insert(session).await?;
        tracing::info!(nid = %nid, enabled = enabled, "远程控制状态已更新");
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

    /// 获取会话的 CLI 子进程 PID（用于进程存活检测）。
    pub async fn get_child_pid(&self, nid: &str) -> Option<u32> {
        self.child_pids.lock().await.get(nid).copied()
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
        self: Arc<Self>,
        nid: &str,
        tool: &str,
        profile: Option<&str>,
        cwd: &str,
        cols: u16,
        rows: u16,
        wss_tx: mpsc::UnboundedSender<String>,
        ipc_tx: mpsc::UnboundedSender<String>,
        merger: std::sync::Arc<InputMerger>,
        remote_enabled: Option<Arc<std::sync::atomic::AtomicBool>>,
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
        cmd.args(["-i", "-l", "-c"]);
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

        // 构建 CLI 命令行: zsh -i -l -c "exec <binary> [--settings tmp.json] ..."
        // 使用 exec 替换 shell 进程，确保 CLI 退出时 PTY 会话正确结束
        let mut exec_cmd = format!("exec {}", binary);
        for arg in &prep.extra_args { exec_cmd.push(' '); exec_cmd.push_str(arg); }
        cmd.arg(&exec_cmd);

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
        let fanout = OutputFanout::new(
            nid.to_string(),
            Some(wss_tx),
            Some(ipc_tx),
            session_cancel.clone(),
            remote_enabled,
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
        let (end_tx, mut end_rx) = mpsc::unbounded_channel::<()>();
        let end_wss_tx = fanout.inner.wss_tx.clone();
        let sid_for_end = sid.clone();
        let sessions_for_end = self.clone();
        tokio::spawn(async move {
            tracing::debug!(session_id = %sid_for_end, "session_ended 监听任务已启动");
            while end_rx.recv().await.is_some() {
                tracing::info!(session_id = %sid_for_end, "收到 session_ended 信号，准备上报");
                if let Some(msg) = sessions_for_end.report_session_ended(&sid_for_end, "process_exit").await.ok().flatten() {
                    if let Some(tx) = end_wss_tx.as_ref() {
                        let _ = tx.send(msg.to_json());
                        tracing::info!(session_id = %sid_for_end, "session_ended 已发送到 Cloud");
                    } else {
                        tracing::warn!(session_id = %sid_for_end, "wss_tx 不可用，无法发送 session_ended");
                    }
                } else {
                    tracing::warn!(session_id = %sid_for_end, "report_session_ended 返回 None，可能已经上报过或 session 不存在");
                }
            }
            tracing::debug!(session_id = %sid_for_end, "session_ended 监听任务退出");
        });
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
            tracing::info!(session_id=%sid, "PTY 进程已退出，准备发送 session_ended 信号");
            match end_tx.send(()) {
                Ok(_) => tracing::info!(session_id=%sid, "session_ended 信号已发送到监听任务"),
                Err(e) => tracing::error!(session_id=%sid, error=%e, "发送 session_ended 信号失败"),
            }

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
                            let txt = msg.text.clone();
                            let len = txt.len();
                            let preview = if txt.chars().count() <= 50 { txt.clone() } else { format!("{}...", txt.chars().take(50).collect::<String>()) };
                            let mut w = writer_clone.lock().await;
                            match w.write_all(msg.text.as_bytes()) {
                                Ok(_) => tracing::info!(session_id = %sid, len = len, text = %preview, "⌨️  [PTY-IN] 写入 PTY stdin"),
                                Err(e) => tracing::error!(session_id = %sid, error = %e, "⌨️  [PTY-IN] 写入失败"),
                            }
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

    // ── Checkpoint (DEPRECATED: 由 CLI 心跳 + Redis 替代) ───

    /// @deprecated 由 cli_heartbeat 心跳 + Redis 替代。保留方法签名兼容旧调用。
    #[deprecated(note = "use cli_heartbeat instead")]
    pub async fn save_checkpoint(&self, _nid: &str) -> std::result::Result<(), String> {
        Ok(())
    }

    /// @deprecated 由 SessionHeartbeatMonitor 替代。
    #[deprecated(note = "use cli_heartbeat instead")]
    pub fn start_checkpoint_loop(_sm: Arc<SessionManager>, _state: Arc<StateMachine>) {}
}

    // 原 checkpoint 实现已删除。以下内容仅保留标记。
// ── CLI Tool helpers ────────────────────────────────────────
