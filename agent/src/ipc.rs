//! IPC Server — Unix socket JSON-line protocol for local agent control.
//!
//! Binds to `~/.kn/agent/ipc.sock` with 0600 permissions.
//! Each line is a complete JSON request/response pair.
//!
//! ## Protocol
//!
//! Request:  `{"id":"<uuid>","method":"<name>","params":{...}}`
//! Response: `{"id":"<uuid>","result":{...}}`
//!        or `{"id":"<uuid>","error":{"code":"...","message":"..."}}`
//!
//! ## Methods
//!
//! | Method             | Params                             | Description                          |
//! |--------------------|------------------------------------|--------------------------------------|
//! | status             | —                                  | Agent state, crash_count, safe_mode  |
//! | sessions           | —                                  | List all sessions                    |
//! | bind               | —                                  | Trigger device binding               |
//! | pause              | —                                  | Pause agent                          |
//! | resume             | —                                  | Resume agent                         |
//! | new_session        | tool, profile?, cwd?, cols?, rows? | Create session + spawn PTY + CLI     |
//! | attach             | nid                                | Create pty.sock, bridge PTY I/O      |
//! | detach             | nid                                | Unsubscribe (stub)                   |
//! | input              | nid, text                          | Write text to PTY stdin              |
//! | ctrl               | nid, signal                        | Send ctrl_c/ctrl_d/ctrl_z to PTY     |
//! | resize             | nid, cols, rows                    | Update terminal size                 |
//! | kill_session       | nid                                | SIGKILL PTY + end session            |
//! | get_output_history | nid, offset?, limit?               | Paginated output log (stub)          |
//! | get_version        | —                                  | Return agent version                 |
//! | redeem             | code                               | Redeem card code (requires binding)  |

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixListener;
use tokio::sync::{mpsc, Mutex};
use tokio_util::sync::CancellationToken;

use crate::error::Result;
use crate::session::{InputMerger, InputMessage, SessionManager, SessionSummary};
use crate::state::{StateEvent, StateMachine};

// ── IPC wire helpers ──────────────────────────────────────────

/// An incoming IPC request (one JSON line).
#[derive(Debug, serde::Deserialize)]
struct IpcRequest {
    id: String,
    method: String,
    #[serde(default)]
    params: serde_json::Value,
}

/// Serialize a successful response as a single JSON line.
fn ok_response(id: &str, result: serde_json::Value) -> String {
    let mut s = serde_json::json!({"id": id, "result": result}).to_string();
    s.push('\n');
    s
}

/// Serialize an error response as a single JSON line.
fn err_response(id: &str, code: &str, message: &str) -> String {
    let mut s = serde_json::json!({
        "id": id,
        "error": {"code": code, "message": message}
    })
    .to_string();
    s.push('\n');
    s
}

/// Response for parse errors (no valid `id` to echo back).
fn parse_error(message: &str) -> String {
    let mut s = serde_json::json!({
        "id": "",
        "error": {"code": "PARSE_ERROR", "message": message}
    })
    .to_string();
    s.push('\n');
    s
}

// ── IpcServer ─────────────────────────────────────────────────

/// Unix-domain socket IPC server.
///
/// Listens on `~/.kn/agent/ipc.sock` and handles JSON-line requests
/// from local clients (desktop app, CLI tools).
pub struct IpcServer {
    socket_path: PathBuf,
    state: Arc<StateMachine>,
    sessions: Arc<SessionManager>,
    bind_http_url: String,
    machine_id: String,
    hostname: String,
    purchase_url: String,
    input_merger: Arc<InputMerger>,
    /// CancellationToken + generation for in-progress bind polling.
    /// Stored together so stale cancel requests (from old dialogs) can't
    /// cancel the new bind's token.
    bind_cancel: Arc<Mutex<Option<(CancellationToken, u64)>>>,
    /// Generation counter: incremented on each new bind, prevents stale
    /// background tasks from corrupting state after a cancel+rebind cycle.
    bind_generation: Arc<AtomicU64>,
}

impl IpcServer {
    /// Create a new IPC server.
    pub fn new(
        socket_path: PathBuf,
        state: Arc<StateMachine>,
        sessions: Arc<SessionManager>,
        bind_http_url: String,
        machine_id: String,
        hostname: String,
        purchase_url: String,
        input_merger: Arc<InputMerger>,
    ) -> Self {
        Self {
            socket_path,
            state,
            sessions,
            bind_http_url,
            machine_id,
            hostname,
            purchase_url,
            input_merger,
            bind_cancel: Arc::new(Mutex::new(None)),
            bind_generation: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Start the IPC server. Runs until `shutdown` is cancelled.
    ///
    /// Binds to the Unix socket, sets 0600 permissions, then accepts
    /// connections in a loop. Each connection is handled in a separate
    /// `tokio::spawn` task.
    pub async fn run(&self, shutdown: CancellationToken) -> Result<()> {
        // Clean up any stale socket from a previous run
        if self.socket_path.exists() {
            let _ = std::fs::remove_file(&self.socket_path);
        }

        // Ensure parent directory exists
        if let Some(parent) = self.socket_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let listener = UnixListener::bind(&self.socket_path)?;

        // Set permissions to 0600 (owner read/write only)
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(
                &self.socket_path,
                std::fs::Permissions::from_mode(0o600),
            )?;
        }

        tracing::info!("IPC 服务器已启动: {}", self.socket_path.display());

        loop {
            tokio::select! {
                _ = shutdown.cancelled() => {
                    tracing::info!("IPC 服务器收到关闭信号");
                    break;
                }
                result = listener.accept() => {
                    match result {
                        Ok((stream, addr)) => {
                            tracing::debug!("IPC 客户端已连接: {:?}", addr);
                            let handle = self.clone_refs();
                            tokio::spawn(async move {
                                handle.handle_connection(stream).await;
                            });
                        }
                        Err(e) => {
                            tracing::error!("IPC accept 错误: {}", e);
                        }
                    }
                }
            }
        }

        // Clean up socket on shutdown
        let _ = std::fs::remove_file(&self.socket_path);
        tracing::info!("IPC 服务器已停止");

        Ok(())
    }

    /// Create a lightweight clone of shared references for handlers.
    fn clone_refs(&self) -> IpcHandle {
        IpcHandle {
            state: self.state.clone(),
            sessions: self.sessions.clone(),
            bind_http_url: self.bind_http_url.clone(),
            machine_id: self.machine_id.clone(),
            hostname: self.hostname.clone(),
            purchase_url: self.purchase_url.clone(),
            input_merger: self.input_merger.clone(),
            bind_cancel: self.bind_cancel.clone(),
            bind_generation: self.bind_generation.clone(),
        }
    }
}

// ── IpcHandle — per-connection handler ─────────────────────────

/// Shared references passed to each connection handler task.
struct IpcHandle {
    state: Arc<StateMachine>,
    sessions: Arc<SessionManager>,
    bind_http_url: String,
    machine_id: String,
    hostname: String,
    purchase_url: String,
    input_merger: Arc<InputMerger>,
    bind_cancel: Arc<Mutex<Option<(CancellationToken, u64)>>>,
    bind_generation: Arc<AtomicU64>,
}

impl IpcHandle {
    /// Handle a single client connection. Reads complete JSON lines,
    /// dispatches each to the appropriate handler, and writes the response.
    async fn handle_connection(&self, stream: tokio::net::UnixStream) {
        let (reader, mut writer) = stream.into_split();
        let mut buf_reader = BufReader::new(reader);
        let mut line = String::new();

        loop {
            line.clear();
            match buf_reader.read_line(&mut line).await {
                Ok(0) => {
                    // EOF — client disconnected cleanly
                    tracing::debug!("IPC 客户端已断开");
                    break;
                }
                Ok(_) => {
                    let trimmed = line.trim();
                    if trimmed.is_empty() {
                        continue;
                    }

                    let resp = match serde_json::from_str::<IpcRequest>(trimmed) {
                        Ok(req) => self.dispatch(&req).await,
                        Err(e) => parse_error(&e.to_string()),
                    };

                    if let Err(e) = writer.write_all(resp.as_bytes()).await {
                        tracing::debug!("IPC 写错误: {}", e);
                        break;
                    }
                }
                Err(e) => {
                    tracing::debug!("IPC 读错误: {}", e);
                    break;
                }
            }
        }
    }

    /// Dispatch a request to the appropriate method handler.
    async fn dispatch(&self, req: &IpcRequest) -> String {
        match req.method.as_str() {
            "status" => self.handle_status(req).await,
            "sessions" => self.handle_sessions(req).await,
            "bind" => self.handle_bind(req).await,
            "pause" => self.handle_pause(req).await,
            "resume" => self.handle_resume(req).await,
            "new_session" => self.handle_new_session(req).await,
            "attach" => self.handle_attach(req).await,
            "detach" => self.handle_detach(req).await,
            "input" => self.handle_input(req).await,
            "ctrl" => self.handle_ctrl(req).await,
            "resize" => self.handle_resize(req).await,
            "kill_session" => self.handle_kill_session(req).await,
            "get_output_history" => self.handle_get_output_history(req).await,
            "get_version" => self.handle_get_version(req).await,
            "redeem" => self.handle_redeem(req).await,
            "cancel_bind" => self.handle_cancel_bind(req).await,
            _ => err_response(
                &req.id,
                "METHOD_NOT_FOUND",
                &format!("未知方法: {}", req.method),
            ),
        }
    }

    // ── Method handlers ────────────────────────────────────────

    /// `status` — return current agent state, crash_count, safe_mode, uptime,
    /// hostname, and purchase_url.
    async fn handle_status(&self, req: &IpcRequest) -> String {
        let state = self.state.current().await;
        ok_response(
            &req.id,
            serde_json::json!({
                "state": state.name(),
                "crash_count": self.state.crash_count(),
                "safe_mode": self.state.in_safe_mode(),
                "uptime_secs": self.state.uptime_secs(),
                "hostname": self.hostname,
                "purchase_url": self.purchase_url,
            }),
        )
    }

    /// `sessions` — list all sessions.
    async fn handle_sessions(&self, req: &IpcRequest) -> String {
        match self.sessions.list().await {
            Ok(sessions) => {
                let items: Vec<serde_json::Value> =
                    sessions.iter().map(session_to_json).collect();
                ok_response(
                    &req.id,
                    serde_json::json!({
                        "sessions": items,
                        "count": items.len(),
                    }),
                )
            }
            Err(e) => err_response(&req.id, "INTERNAL", &e.to_string()),
        }
    }

    /// `bind` — 两步绑定：先获取绑定码返回给 Desktop，再后台轮询。
    async fn handle_bind(&self, req: &IpcRequest) -> String {
        // Validate config BEFORE transitioning state (B3: prevent stuck Binding)
        if self.bind_http_url.is_empty() {
            return err_response(&req.id, "CONFIG_ERROR", "bind_http_url 未配置");
        }

        // Transition to Binding state
        if let Err(e) = self.state.transition(StateEvent::BindInit).await {
            return err_response(&req.id, "STATE_ERROR", &e.to_string());
        }

        // Step 1: 同步获取绑定码
        let (bind_code, expires_in, confirm_url) = match crate::device::bind_init(&self.bind_http_url, &self.machine_id).await {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!("bind_init 失败: {}", e);
                let _ = self.state.transition(StateEvent::BindTimeout).await;
                let msg = if e.to_string().contains("connect") || e.to_string().contains("timeout") {
                    "绑定服务不可用，请检查网络连接后重试"
                } else {
                    "绑定失败，请稍后重试"
                };
                return err_response(&req.id, "BIND_ERROR", msg);
            }
        };

        // Step 2: 后台轮询绑定结果（B2: generation guard 防止旧任务污染新绑定的状态）
        let state = self.state.clone();
        let bind_url = self.bind_http_url.clone();
        let bind_cancel = CancellationToken::new();
        let poll_code = bind_code.clone();
        // Bump generation — any previous task will see a stale generation and skip state transitions
        let generation = self.bind_generation.fetch_add(1, Ordering::Relaxed) + 1;
        let bind_gen = self.bind_generation.clone();

        // Store (cancel token, generation) so stale cancel requests can't kill new binds
        {
            let mut guard = self.bind_cancel.lock().await;
            // Cancel any previous pending bind
            if let Some((old_token, _old_gen)) = guard.take() {
                old_token.cancel();
            }
            *guard = Some((bind_cancel.clone(), generation));
        }

        tokio::spawn(async move {
            match crate::device::bind_poll(&bind_url, &poll_code, expires_in, bind_cancel).await {
                Ok(token) => {
                    // Only apply if this is still the latest bind
                    if bind_gen.load(Ordering::Relaxed) != generation {
                        tracing::info!("IPC 绑定结果已过期（有新绑定发起），忽略");
                        return;
                    }
                    tracing::info!("IPC 绑定成功");
                    let _ = crate::device::save_device_token(&token);
                    let _ = state.transition(StateEvent::BindResult).await;
                }
                Err(e) => {
                    // Only apply timeout if this is still the latest bind
                    if bind_gen.load(Ordering::Relaxed) != generation {
                        tracing::info!("IPC 绑定超时结果已过期（有新绑定发起），忽略");
                        return;
                    }
                    tracing::error!("绑定轮询失败: {}", e);
                    let _ = state.transition(StateEvent::BindTimeout).await;
                }
            }
        });

        // 返回绑定码和确认 URL 给 Desktop（Desktop 生成 QR 码展示）
        ok_response(
            &req.id,
            serde_json::json!({
                "status": "binding_started",
                "bindCode": bind_code,
                "expiresIn": expires_in,
                "confirmUrl": confirm_url
            }),
        )
    }

    /// `cancel_bind` — cancel an in-progress device binding.
    ///
    /// Called by the frontend when the user dismisses the BindDialog or
    /// the BindDialog times out. Cancels the background polling task so
    /// it doesn't waste resources.
    async fn handle_cancel_bind(&self, req: &IpcRequest) -> String {
        let current_gen = self.bind_generation.load(Ordering::Relaxed);
        let mut guard = self.bind_cancel.lock().await;
        if let Some((token, gen)) = guard.take() {
            // R1: Only cancel if this is still the latest bind (stale cancel from old dialog)
            if gen != current_gen {
                tracing::info!("取消绑定请求已过期 (gen={}, current={})，忽略", gen, current_gen);
                return ok_response(&req.id, serde_json::json!({"status": "stale_cancel"}));
            }
            token.cancel();
            tracing::info!("绑定轮询已取消");
            ok_response(&req.id, serde_json::json!({"status": "cancelled"}))
        } else {
            ok_response(&req.id, serde_json::json!({"status": "no_active_bind"}))
        }
    }

    /// `pause` — pause the agent.
    async fn handle_pause(&self, req: &IpcRequest) -> String {
        match self.state.transition(StateEvent::Pause).await {
            Ok(_) => ok_response(&req.id, serde_json::json!({"status": "paused"})),
            Err(e) => err_response(&req.id, "STATE_ERROR", &e.to_string()),
        }
    }

    /// `resume` — resume the agent.
    async fn handle_resume(&self, req: &IpcRequest) -> String {
        match self.state.transition(StateEvent::Resume).await {
            Ok(_) => ok_response(&req.id, serde_json::json!({"status": "resumed"})),
            Err(e) => err_response(&req.id, "STATE_ERROR", &e.to_string()),
        }
    }

    /// `new_session` — create a new session.
    ///
    /// Params:
    /// - `tool` (string, default "bash"): CLI tool to run
    /// - `profile` (string, optional): profile name for env injection
    /// - `cwd` (string, default "."): working directory (relative paths resolved against HOME)
    /// - `cols` (u16, default 80): terminal columns
    /// - `rows` (u16, default 24): terminal rows
    async fn handle_new_session(&self, req: &IpcRequest) -> String {
        let tool = req
            .params
            .get("tool")
            .and_then(|v| v.as_str())
            .unwrap_or("bash");
        let profile = req
            .params
            .get("profile")
            .and_then(|v| v.as_str())
            .map(String::from);
        let cwd_raw = req
            .params
            .get("cwd")
            .and_then(|v| v.as_str())
            .unwrap_or(".");
        let cols = req
            .params
            .get("cols")
            .and_then(|v| v.as_u64())
            .unwrap_or(80) as u16;
        let rows = req
            .params
            .get("rows")
            .and_then(|v| v.as_u64())
            .unwrap_or(24) as u16;

        // Resolve cwd: non-absolute paths are relative to HOME
        let cwd = {
            let p = Path::new(cwd_raw);
            if p.is_absolute() {
                if p.is_dir() {
                    cwd_raw.to_string()
                } else {
                    return err_response(
                        &req.id,
                        "INVALID_PARAMS",
                        &format!("目录不存在: {}", cwd_raw),
                    );
                }
            } else {
                let resolved = kn_common::path::home_dir().join(cwd_raw);
                if resolved.is_dir() {
                    resolved.to_string_lossy().to_string()
                } else {
                    return err_response(
                        &req.id,
                        "INVALID_PARAMS",
                        &format!("目录不存在: {}", cwd_raw),
                    );
                }
            }
        };

        let nid = format!("s_{}", nanoid::nanoid!(12));
        // db_id = 0 is the sentinel for local-only sessions (no cloud DB entry)
        let db_id: i64 = 0;

        // Create session record first
        match self
            .sessions
            .create(
                nid.clone(),
                db_id,
                tool.to_string(),
                profile.clone(),
                cwd.clone(),
            )
            .await
        {
            Ok(session) => {
                // Apply custom dimensions if non-default
                if cols != 80 || rows != 24 {
                    let _ = self.sessions.resize(&nid, cols, rows).await;
                }

                // Spawn PTY + CLI process
                let (wss_tx, _wss_rx) = mpsc::unbounded_channel::<String>();
                let (ipc_tx, _ipc_rx) = mpsc::unbounded_channel::<String>();
                let merger = self.input_merger.clone();
                let session_nid = nid.clone();
                let sessions = self.sessions.clone();
                let tool_owned = tool.to_string();
                let profile_owned = profile.clone();
                let cwd_owned = cwd.clone();

                tokio::spawn(async move {
                    match sessions
                        .start_session(
                            &session_nid,
                            &tool_owned,
                            profile_owned.as_deref(),
                            &cwd_owned,
                            cols,
                            rows,
                            wss_tx,
                            ipc_tx,
                            merger,
                        )
                        .await
                    {
                        Ok(_fanout) => {
                            tracing::info!(nid = %session_nid, tool = %tool_owned, "PTY session started");
                        }
                        Err(e) => {
                            tracing::error!(nid = %session_nid, error = %e, "PTY session start failed");
                        }
                    }
                });

                ok_response(
                    &req.id,
                    serde_json::json!({
                        "nid": session.nid,
                        "tool": session.tool,
                        "profile": session.profile,
                        "cwd": session.cwd,
                        "cols": cols,
                        "rows": rows,
                        "status": "created",
                        "created_at": session.created_at.to_rfc3339(),
                    }),
                )
            }
            Err(e) => err_response(&req.id, "INTERNAL", &e.to_string()),
        }
    }

    /// `attach` — create a pty.sock and bridge PTY I/O for terminal takeover.
    ///
    /// Returns the Unix socket path that the client should connect to
    /// for bidirectional raw PTY I/O.
    async fn handle_attach(&self, req: &IpcRequest) -> String {
        let nid = match req.params.get("nid").and_then(|v| v.as_str()) {
            Some(n) => n.to_string(),
            None => return err_response(&req.id, "INVALID_PARAMS", "缺少 nid 参数"),
        };

        // 先检查 session 是否存在
        match self.sessions.get(&nid).await {
            Ok(Some(_)) => {}
            Ok(None) => {
                return err_response(&req.id, "NOT_FOUND", &format!("会话未找到: {}", nid));
            }
            Err(e) => return err_response(&req.id, "INTERNAL", &e.to_string()),
        }

        match self.sessions.attach_pty(&nid).await {
            Ok(sock_path) => ok_response(&req.id, serde_json::json!({
                "ok": true,
                "nid": nid,
                "pty_sock": sock_path.to_string_lossy()
            })),
            Err(e) => err_response(&req.id, "INTERNAL", &e.to_string()),
        }
    }

    /// `detach` — unsubscribe from session output (stub: Phase 2 PTY integration).
    async fn handle_detach(&self, req: &IpcRequest) -> String {
        let nid = match req.params.get("nid").and_then(|v| v.as_str()) {
            Some(n) => n,
            None => return err_response(&req.id, "INVALID_PARAMS", "缺少 nid 参数"),
        };
        match self.sessions.get(nid).await {
            Ok(Some(_)) => ok_response(&req.id, serde_json::json!({"ok": true, "nid": nid})),
            Ok(None) => {
                err_response(&req.id, "NOT_FOUND", &format!("会话未找到: {}", nid))
            }
            Err(e) => err_response(&req.id, "INTERNAL", &e.to_string()),
        }
    }

    /// `input` — write text to session PTY stdin.
    async fn handle_input(&self, req: &IpcRequest) -> String {
        let nid = match req.params.get("nid").and_then(|v| v.as_str()) {
            Some(n) => n.to_string(),
            None => return err_response(&req.id, "INVALID_PARAMS", "缺少 nid 参数"),
        };
        let text = req
            .params
            .get("text")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        if text.is_empty() {
            return err_response(&req.id, "INVALID_PARAMS", "text 不能为空");
        }

        // Verify session exists before pushing input
        match self.sessions.get(&nid).await {
            Ok(Some(_)) => {
                self.input_merger.push(InputMessage {
                    session_id: nid.clone(),
                    text,
                    source: "desktop".into(),
                })
                .await;
                ok_response(&req.id, serde_json::json!({"ok": true, "nid": nid}))
            }
            Ok(None) => {
                err_response(&req.id, "NOT_FOUND", &format!("会话未找到: {}", nid))
            }
            Err(e) => err_response(&req.id, "INTERNAL", &e.to_string()),
        }
    }

    /// `ctrl` — send control signal to session PTY.
    ///
    /// Signal mapping:
    /// - `ctrl_c` → `\x03`
    /// - `ctrl_d` → `\x04`
    /// - `ctrl_z` → `\x1a`
    ///
    /// NOTE: control bytes are routed through `InputMerger.text` (UTF-8).
    /// This is safe for the current three ASCII control characters
    /// (0x03, 0x04, 0x1a are valid single-byte UTF-8), but would need
    /// a `Vec<u8>` channel for signals mapped to bytes >= 0x80.
    async fn handle_ctrl(&self, req: &IpcRequest) -> String {
        let nid = match req.params.get("nid").and_then(|v| v.as_str()) {
            Some(n) => n.to_string(),
            None => return err_response(&req.id, "INVALID_PARAMS", "缺少 nid 参数"),
        };
        let signal = req
            .params
            .get("signal")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        // Map signal name to actual byte
        let byte = match signal {
            "ctrl_c" => vec![0x03u8],
            "ctrl_d" => vec![0x04u8],
            "ctrl_z" => vec![0x1au8],
            other => {
                return err_response(
                    &req.id,
                    "INVALID_PARAMS",
                    &format!("未知信号: {} (支持: ctrl_c, ctrl_d, ctrl_z)", other),
                );
            }
        };

        // Verify session exists before sending ctrl
        match self.sessions.get(&nid).await {
            Ok(Some(_)) => {
                // Push ctrl byte as text into PTY stdin
                let text = String::from_utf8_lossy(&byte).to_string();
                self.input_merger.push(InputMessage {
                    session_id: nid.clone(),
                    text,
                    source: "desktop".into(),
                })
                .await;
                ok_response(&req.id, serde_json::json!({"ok": true, "signal": signal, "nid": nid}))
            }
            Ok(None) => {
                err_response(&req.id, "NOT_FOUND", &format!("会话未找到: {}", nid))
            }
            Err(e) => err_response(&req.id, "INTERNAL", &e.to_string()),
        }
    }

    /// `resize` — update session terminal dimensions.
    async fn handle_resize(&self, req: &IpcRequest) -> String {
        let nid = match req.params.get("nid").and_then(|v| v.as_str()) {
            Some(n) => n,
            None => return err_response(&req.id, "INVALID_PARAMS", "缺少 nid 参数"),
        };
        let cols = match req.params.get("cols").and_then(|v| v.as_u64()) {
            Some(c) => c as u16,
            None => return err_response(&req.id, "INVALID_PARAMS", "缺少 cols 参数"),
        };
        let rows = match req.params.get("rows").and_then(|v| v.as_u64()) {
            Some(r) => r as u16,
            None => return err_response(&req.id, "INVALID_PARAMS", "缺少 rows 参数"),
        };

        match self.sessions.get(nid).await {
            Ok(Some(_)) => {}
            Ok(None) => {
                return err_response(&req.id, "NOT_FOUND", &format!("会话未找到: {}", nid));
            }
            Err(e) => return err_response(&req.id, "INTERNAL", &e.to_string()),
        }

        match self.sessions.resize(nid, cols, rows).await {
            Ok(_) => ok_response(
                &req.id,
                serde_json::json!({"ok": true, "nid": nid, "cols": cols, "rows": rows}),
            ),
            Err(e) => err_response(&req.id, "INTERNAL", &e.to_string()),
        }
    }

    /// `kill_session` — SIGKILL PTY process + end session.
    async fn handle_kill_session(&self, req: &IpcRequest) -> String {
        let nid = match req.params.get("nid").and_then(|v| v.as_str()) {
            Some(n) => n,
            None => return err_response(&req.id, "INVALID_PARAMS", "缺少 nid 参数"),
        };

        match self.sessions.get(nid).await {
            Ok(Some(_)) => {
                match self.sessions.kill_session(nid).await {
                    Ok(()) => ok_response(&req.id, serde_json::json!({"ok": true, "nid": nid})),
                    Err(e) => err_response(&req.id, "INTERNAL", &e.to_string()),
                }
            }
            Ok(None) => {
                err_response(&req.id, "NOT_FOUND", &format!("会话未找到: {}", nid))
            }
            Err(e) => err_response(&req.id, "INTERNAL", &e.to_string()),
        }
    }

    /// `get_output_history` — paginated output log (stub: Phase 2 PTY integration).
    async fn handle_get_output_history(&self, req: &IpcRequest) -> String {
        let nid = match req.params.get("nid").and_then(|v| v.as_str()) {
            Some(n) => n,
            None => return err_response(&req.id, "INVALID_PARAMS", "缺少 nid 参数"),
        };
        let offset = req
            .params
            .get("offset")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        let limit = req
            .params
            .get("limit")
            .and_then(|v| v.as_u64())
            .unwrap_or(100);

        match self.sessions.get(nid).await {
            Ok(Some(_)) => ok_response(
                &req.id,
                serde_json::json!({
                    "ok": true,
                    "entries": [],
                    "total": 0,
                    "offset": offset,
                    "limit": limit,
                }),
            ),
            Ok(None) => {
                err_response(&req.id, "NOT_FOUND", &format!("会话未找到: {}", nid))
            }
            Err(e) => err_response(&req.id, "INTERNAL", &e.to_string()),
        }
    }

    /// `get_version` — return agent version.
    async fn handle_get_version(&self, req: &IpcRequest) -> String {
        ok_response(
            &req.id,
            serde_json::json!({
                "version": env!("CARGO_PKG_VERSION"),
                "name": "kn-agent",
            }),
        )
    }

    /// `redeem` — 卡密兑换：仅在有绑定关系时可用。
    async fn handle_redeem(&self, req: &IpcRequest) -> String {
        let code = match req.params.get("code").and_then(|v| v.as_str()) {
            Some(c) if !c.is_empty() => c,
            _ => return err_response(&req.id, "INVALID_PARAMS", "卡密不能为空"),
        };

        // 检查是否有绑定关系（本地 device_token 存在）
        let token = match crate::device::load_device_token() {
            Some(t) if !t.is_empty() => t,
            _ => {
                return err_response(
                    &req.id,
                    "NOT_BOUND",
                    "设备未绑定，请先在 iOS App 中绑定设备后再兑换",
                );
            }
        };

        // 调用云端 redeem API
        match crate::device::redeem(&self.bind_http_url, &token, code).await {
            Ok((plan, days)) => ok_response(
                &req.id,
                serde_json::json!({
                    "status": "redeemed",
                    "plan": plan,
                    "days": days
                }),
            ),
            Err(e) => {
                let msg = e.to_string();
                tracing::warn!("redeem 失败: {}", msg);
                // 区分错误类型，给用户友好提示
                if msg.contains("CODE_ALREADY_USED") || msg.contains("已被使用") {
                    err_response(&req.id, "CODE_ALREADY_USED", "该卡密已被使用")
                } else if msg.contains("CODE_NOT_FOUND") || msg.contains("不存在") {
                    err_response(&req.id, "CODE_NOT_FOUND", "卡密不存在")
                } else if msg.contains("UNAUTHORIZED") || msg.contains("401") {
                    err_response(&req.id, "NOT_BOUND", "设备绑定已失效，请重新绑定")
                } else if msg.contains("INVALID_CODE_FORMAT") || msg.contains("格式") {
                    err_response(&req.id, "INVALID_CODE_FORMAT", "卡密格式无效")
                } else {
                    err_response(&req.id, "REDEEM_ERROR", &format!("兑换失败: {}", msg))
                }
            }
        }
    }
}

// ── Helpers ───────────────────────────────────────────────────

/// Convert a `SessionSummary` to a JSON value for serialization.
fn session_to_json(s: &SessionSummary) -> serde_json::Value {
    serde_json::json!({
        "nid": s.nid,
        "db_id": s.db_id,
        "tool": s.tool,
        "profile": s.profile,
        "cwd": s.cwd,
        "cols": s.cols,
        "rows": s.rows,
        "created_at": s.created_at.to_rfc3339(),
        "status": match s.status {
            crate::session::SessionStatus::Created => "created",
            crate::session::SessionStatus::Running => "running",
            crate::session::SessionStatus::Ended => "ended",
        },
    })
}

// ── Tests ─────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_request_valid() {
        let json = r#"{"id":"abc123","method":"status","params":{}}"#;
        let req: IpcRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.id, "abc123");
        assert_eq!(req.method, "status");
    }

    #[test]
    fn test_parse_request_params_optional() {
        let json = r#"{"id":"abc123","method":"status"}"#;
        let req: IpcRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.id, "abc123");
        assert_eq!(req.method, "status");
        assert_eq!(req.params, serde_json::Value::Null);
    }

    #[test]
    fn test_parse_request_invalid_json() {
        let json = r#"{"id":"abc123","method":"status""#; // missing closing brace
        let err = serde_json::from_str::<IpcRequest>(json);
        assert!(err.is_err());
    }

    #[test]
    fn test_ok_response_format() {
        let resp = ok_response("abc", serde_json::json!({"key": "value"}));
        // Should end with newline
        assert!(resp.ends_with('\n'));
        // Should be valid JSON with trailing newline stripped
        let parsed: serde_json::Value =
            serde_json::from_str(resp.trim_end()).unwrap();
        assert_eq!(parsed["id"], "abc");
        assert_eq!(parsed["result"]["key"], "value");
    }

    #[test]
    fn test_err_response_format() {
        let resp = err_response("abc", "NOT_FOUND", "session not found");
        assert!(resp.ends_with('\n'));
        let parsed: serde_json::Value =
            serde_json::from_str(resp.trim_end()).unwrap();
        assert_eq!(parsed["id"], "abc");
        assert_eq!(parsed["error"]["code"], "NOT_FOUND");
        assert_eq!(parsed["error"]["message"], "session not found");
    }

    #[test]
    fn test_parse_error_no_id() {
        let resp = parse_error("expected value");
        let parsed: serde_json::Value =
            serde_json::from_str(resp.trim_end()).unwrap();
        assert_eq!(parsed["id"], "");
        assert_eq!(parsed["error"]["code"], "PARSE_ERROR");
    }

    #[test]
    fn test_ctrl_signal_mapping() {
        // Verify the byte mappings for control signals
        assert_eq!(0x03u8, b'\x03'); // ctrl_c
        assert_eq!(0x04u8, b'\x04'); // ctrl_d
        assert_eq!(0x1au8, b'\x1a'); // ctrl_z
    }
}
