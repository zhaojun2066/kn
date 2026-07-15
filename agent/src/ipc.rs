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
//! | bind / bindStartOrResume | —                             | Create or resume device binding      |
//! | bindingStatus      | —                                  | Read durable binding progress        |
//! | bindCancel         | —                                  | Explicitly cancel device binding     |
//! | pause              | —                                  | Pause agent                          |
//! | resume             | —                                  | Resume agent                         |
//! | new_session        | tool, profile?, cwd?, cols?, rows? | Create session + spawn PTY + CLI     |
//! | attach             | nid                                | Create pty.sock, bridge PTY I/O      |
//! | input              | nid, text                          | Write text to PTY stdin              |
//! | ctrl               | nid, signal                        | Send ctrl_c/ctrl_d/ctrl_z to PTY     |
//! | resize             | nid, cols, rows                    | Update terminal size                 |
//! | kill_session       | nid                                | SIGKILL PTY + end session + notify cloud |
//! | register_session   | tool, profile?, cwd, source?       | Register desktop PTY (Relay, no PTY spawn) |
//! | relay_exit         | nid, reason?                       | Mark desktop-owned Relay PTY as ended     |
//! | set_remote_enabled | nid, enabled                       | Toggle iOS visibility/control            |
//! | relay_output       | nid, data                          | Forward desktop-owned PTY output         |
//! | poll_relay_input   | nid                                | Drain queued iOS input for Relay session |
//! | get_version        | —                                  | Return agent version                 |
//! | redeem             | code                               | Redeem card code (requires binding)  |

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixListener;
use tokio::sync::{mpsc, Mutex};
use tokio_util::sync::CancellationToken;

use crate::ack::AckRegistry;
use crate::error::{AgentError, Result};
use crate::session::{InputMerger, InputMessage, SessionManager, SessionSummary, ViewportOwner};
use crate::state::{StateEvent, StateMachine};

/// The durable bind worker has one local owner.  In particular, a user may
/// cancel while we are still waiting for phone approval, but once a provisional
/// token has been received the operation is deliberately non-cancellable: the
/// Cloud may already have committed its matching formal device.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BindPhase {
    WaitingPhone,
    SavingCredential,
    Activating,
    Finalizing,
}

impl BindPhase {
    fn can_cancel(self) -> bool {
        matches!(self, Self::WaitingPhone)
    }
}

struct BindingWorker {
    cancel: CancellationToken,
    generation: u64,
    phase: BindPhase,
}

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
    bind_cancel: Arc<Mutex<Option<BindingWorker>>>,
    /// Generation counter: incremented on each new bind, prevents stale
    /// background tasks from corrupting state after a cancel+rebind cycle.
    bind_generation: Arc<AtomicU64>,
    /// Channel to signal the main loop to start WSS after a successful bind.
    wss_trigger: mpsc::UnboundedSender<()>,
    /// Global WSS outgoing channel, shared with main loop.
    outgoing_tx_ref: Arc<tokio::sync::Mutex<Option<mpsc::UnboundedSender<String>>>>,
    /// ACK registry for session_created → session_created_ack correlation.
    ack_registry: Arc<AckRegistry>,
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
        wss_trigger: mpsc::UnboundedSender<()>,
        outgoing_tx_ref: Arc<tokio::sync::Mutex<Option<mpsc::UnboundedSender<String>>>>,
        ack_registry: Arc<AckRegistry>,
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
            wss_trigger,
            outgoing_tx_ref,
            ack_registry,
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
            std::fs::set_permissions(&self.socket_path, std::fs::Permissions::from_mode(0o600))?;
        }

        tracing::info!("IPC 服务器已启动: {}", self.socket_path.display());

        // A dialog is only a view. Restore the durable binding worker after an
        // Agent restart so a phone confirmation is never stranded in Redis.
        let recovery = self.clone_refs();
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(250)).await;
            if let Some(activation) = crate::device::load_pending_activation() {
                // Never discard an activation marker merely because its Redis
                // TTL elapsed.  bind-activate is idempotent against MySQL and
                // is the only safe way to learn whether the response was lost
                // after the formal device committed.
                if recovery.transition_to_binding().await.is_ok() {
                    recovery
                        .start_binding_poll(pending_for_activation(&activation))
                        .await;
                }
                return;
            }
            let Some(pending) = crate::device::load_pending_binding() else {
                return;
            };
            if binding_expired(&pending) {
                let _ = crate::device::clear_pending_binding();
                return;
            }
            if recovery.transition_to_binding().await.is_ok() {
                recovery.start_binding_poll(pending).await;
            }
        });

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
            wss_trigger: self.wss_trigger.clone(),
            outgoing_tx_ref: self.outgoing_tx_ref.clone(),
            ack_registry: self.ack_registry.clone(),
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
    bind_cancel: Arc<Mutex<Option<BindingWorker>>>,
    bind_generation: Arc<AtomicU64>,
    wss_trigger: mpsc::UnboundedSender<()>,
    outgoing_tx_ref: Arc<tokio::sync::Mutex<Option<mpsc::UnboundedSender<String>>>>,
    ack_registry: Arc<AckRegistry>,
}

fn unix_now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis().try_into().unwrap_or(u64::MAX))
        .unwrap_or(u64::MAX)
}

fn remaining_secs(expires_at_ms: u64) -> u64 {
    let now = unix_now_ms();
    expires_at_ms.saturating_sub(now).saturating_add(999) / 1_000
}

fn binding_expired_at(expires_at_ms: u64) -> bool {
    expires_at_ms <= unix_now_ms()
}

fn binding_expired(pending: &crate::device::PendingBinding) -> bool {
    binding_expired_at(pending.pairing_expires_at_ms)
}

fn pending_for_activation(
    activation: &crate::device::PendingActivation,
) -> crate::device::PendingBinding {
    crate::device::PendingBinding {
        pairing_id: activation.pairing_id.clone(),
        // These fields are never exposed or polled when an activation marker
        // exists; they merely let the single recovery worker retain its common
        // input type after a crash between Cloud commit and local cleanup.
        approval_code: String::new(),
        poll_secret: activation.poll_secret.clone(),
        confirm_url: String::new(),
        qr_expires_at_ms: 0,
        pairing_expires_at_ms: activation.pairing_expires_at_ms,
    }
}

fn binding_status_json() -> serde_json::Value {
    if let Some(activation) = crate::device::load_pending_activation() {
        return serde_json::json!({
            "state": "activationUncertain",
            "pairingId": activation.pairing_id,
            "message": "电脑正在确认正式绑定，暂时不能取消",
        });
    }
    let Some(pending) = crate::device::load_pending_binding() else {
        return serde_json::json!({"state": "idle"});
    };
    if binding_expired(&pending) {
        return serde_json::json!({
            "state": "expired",
            "pairingId": pending.pairing_id,
        });
    }
    serde_json::json!({
        "state": "waitingAgent",
        "pairingId": pending.pairing_id,
        "bindCode": pending.approval_code,
        "confirmUrl": pending.confirm_url,
        "expiresIn": remaining_secs(pending.qr_expires_at_ms),
        "pairingExpiresIn": remaining_secs(pending.pairing_expires_at_ms),
    })
}

fn bind_init_error(req: &IpcRequest, error: AgentError) -> String {
    tracing::warn!("bind-init 失败: {}", error);
    let message = if error.to_string().contains("connect") || error.to_string().contains("timeout")
    {
        "绑定服务不可用，请检查网络连接后重试"
    } else {
        "绑定失败，请稍后重试"
    };
    err_response(&req.id, "BIND_ERROR", message)
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
            "bind" | "bindStartOrResume" => self.handle_bind(req).await,
            "bindingStatus" => self.handle_binding_status(req).await,
            "pause" => self.handle_pause(req).await,
            "resume" => self.handle_resume(req).await,
            "new_session" => self.handle_new_session(req).await,
            "attach" => self.handle_attach(req).await,
            "input" => self.handle_input(req).await,
            "ctrl" => self.handle_ctrl(req).await,
            "resize" => self.handle_resize(req).await,
            "kill_session" => self.handle_kill_session(req).await,
            "register_session" => self.handle_register_session(req).await,
            "relay_exit" => self.handle_relay_exit(req).await,
            "relay_output" => self.handle_relay_output(req).await,
            "poll_relay_input" => self.handle_poll_relay_input(req).await,
            "set_remote_enabled" => self.handle_set_remote_enabled(req).await,
            "get_version" => self.handle_get_version(req).await,
            "redeem" => self.handle_redeem(req).await,
            "cancel_bind" | "bindCancel" => self.handle_cancel_bind(req).await,
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
        let binding = binding_status_json();
        ok_response(
            &req.id,
            serde_json::json!({
                "state": state.name(),
                "crash_count": self.state.crash_count(),
                "safe_mode": self.state.in_safe_mode(),
                "uptime_secs": self.state.uptime_secs(),
                "hostname": self.hostname,
                "purchase_url": self.purchase_url,
                "binding": binding,
            }),
        )
    }

    /// `bindingStatus` — durable status for a pairing which can survive the Desktop dialog.
    async fn handle_binding_status(&self, req: &IpcRequest) -> String {
        ok_response(&req.id, binding_status_json())
    }

    /// `sessions` — list all sessions.
    async fn handle_sessions(&self, req: &IpcRequest) -> String {
        match self.sessions.list().await {
            Ok(sessions) => {
                let items: Vec<serde_json::Value> = sessions.iter().map(session_to_json).collect();
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

    /// `bind` / `bindStartOrResume` — 创建或恢复双确认绑定。
    async fn handle_bind(&self, req: &IpcRequest) -> String {
        // Validate config BEFORE transitioning state (B3: prevent stuck Binding)
        if self.bind_http_url.is_empty() {
            return err_response(&req.id, "CONFIG_ERROR", "bind_http_url 未配置");
        }

        // A durable pending pairing always wins over a new request. This protects the
        // user from consuming another QR code when they merely closed/reopened Desktop.
        let pending = if let Some(activation) = crate::device::load_pending_activation() {
            // A durable marker always wins, even after its original pairing
            // expiry.  We must probe Cloud's idempotent activation path before
            // considering rollback or issuing another QR code.
            pending_for_activation(&activation)
        } else {
            match crate::device::load_pending_binding() {
                Some(pending) if !binding_expired(&pending) => pending,
                Some(_) => {
                    let _ = crate::device::clear_pending_binding();
                    match crate::device::bind_init(&self.bind_http_url, &self.machine_id).await {
                        Ok(pending) => {
                            if let Err(e) = crate::device::save_pending_binding(&pending) {
                                let _ =
                                    crate::device::bind_cancel(&self.bind_http_url, &pending).await;
                                return err_response(
                                    &req.id,
                                    "LOCAL_STORAGE_ERROR",
                                    &e.to_string(),
                                );
                            }
                            pending
                        }
                        Err(e) => return bind_init_error(req, e),
                    }
                }
                None => match crate::device::bind_init(&self.bind_http_url, &self.machine_id).await
                {
                    Ok(pending) => {
                        if let Err(e) = crate::device::save_pending_binding(&pending) {
                            let _ = crate::device::bind_cancel(&self.bind_http_url, &pending).await;
                            return err_response(&req.id, "LOCAL_STORAGE_ERROR", &e.to_string());
                        }
                        pending
                    }
                    Err(e) => return bind_init_error(req, e),
                },
            }
        };

        if let Err(e) = self.transition_to_binding().await {
            return err_response(&req.id, "STATE_ERROR", &e.to_string());
        }
        self.start_binding_poll(pending.clone()).await;

        let qr_expires_in = remaining_secs(pending.qr_expires_at_ms);
        ok_response(
            &req.id,
            serde_json::json!({
                "status": "binding_started",
                "pairingId": pending.pairing_id,
                "bindCode": pending.approval_code,
                "expiresIn": qr_expires_in,
                "pairingExpiresIn": remaining_secs(pending.pairing_expires_at_ms),
                "confirmUrl": pending.confirm_url,
            }),
        )
    }

    /// Start exactly one background worker for a durable pairing.
    async fn start_binding_poll(&self, pending: crate::device::PendingBinding) {
        {
            let guard = self.bind_cancel.lock().await;
            if guard.is_some() {
                return;
            }
        }
        let state = self.state.clone();
        let bind_url = self.bind_http_url.clone();
        let machine_id = self.machine_id.clone();
        let bind_cancel = CancellationToken::new();
        let generation = self.bind_generation.fetch_add(1, Ordering::Relaxed) + 1;
        let bind_gen = self.bind_generation.clone();
        let wss_trigger = self.wss_trigger.clone();
        let bind_cancel_ref = self.bind_cancel.clone();
        let initial_phase = match crate::device::load_pending_activation() {
            Some(activation) if activation.pairing_id == pending.pairing_id => {
                BindPhase::Activating
            }
            _ => BindPhase::WaitingPhone,
        };
        {
            let mut guard = self.bind_cancel.lock().await;
            if guard.is_some() {
                return;
            }
            *guard = Some(BindingWorker {
                cancel: bind_cancel.clone(),
                generation,
                phase: initial_phase,
            });
        }
        tokio::spawn(async move {
            let activation = match crate::device::load_pending_activation() {
                Some(existing) if existing.pairing_id == pending.pairing_id => existing,
                _ => {
                    let token = match crate::device::bind_poll(
                        &bind_url,
                        &pending,
                        bind_cancel.clone(),
                    )
                    .await
                    {
                        Ok(v) => v,
                        Err(e) => {
                            if bind_gen.load(Ordering::Relaxed) == generation {
                                tracing::warn!(pairing_id = %pending.pairing_id, "绑定轮询停止: {}", e);
                                if !matches!(e, AgentError::Shutdown) {
                                    let _ = crate::device::clear_pending_binding();
                                }
                                let _ = state.transition(StateEvent::BindTimeout).await;
                                let mut guard = bind_cancel_ref.lock().await;
                                if guard.as_ref().map(|worker| worker.generation)
                                    == Some(generation)
                                {
                                    *guard = None;
                                }
                            }
                            return;
                        }
                    };
                    if bind_gen.load(Ordering::Relaxed) != generation {
                        return;
                    }
                    // This is the cancellation boundary.  Once we own a
                    // provisional token, cancellation must not race marker
                    // persistence or a possibly committed Cloud activate.
                    {
                        let mut guard = bind_cancel_ref.lock().await;
                        let Some(worker) = guard.as_mut() else {
                            return;
                        };
                        if worker.generation != generation || !worker.phase.can_cancel() {
                            return;
                        }
                        worker.phase = BindPhase::SavingCredential;
                    }
                    crate::device::PendingActivation {
                        pairing_id: pending.pairing_id.clone(),
                        poll_secret: pending.poll_secret.clone(),
                        machine_id,
                        device_token: token,
                        previous_device_token: crate::device::load_device_token(),
                        pairing_expires_at_ms: pending.pairing_expires_at_ms,
                    }
                }
            };
            let mut activated = false;
            // Once this marker exists WSS is hard-blocked.  Do not use the
            // pairing expiry as a rollback signal: Cloud's idempotent endpoint
            // first checks MySQL for the same machine/token, so it safely
            // resolves a lost success response even after Redis has expired.
            loop {
                if bind_gen.load(Ordering::Relaxed) != generation {
                    return;
                }
                if !activated {
                    let saved = crate::device::save_pending_activation(&activation)
                        .and_then(|_| crate::device::save_device_token(&activation.device_token));
                    if let Err(e) = saved {
                        tracing::error!("无法安全保存绑定凭证，将重试: {}", e);
                        tokio::time::sleep(std::time::Duration::from_secs(30)).await;
                        continue;
                    }
                    {
                        let mut guard = bind_cancel_ref.lock().await;
                        if let Some(worker) = guard
                            .as_mut()
                            .filter(|worker| worker.generation == generation)
                        {
                            worker.phase = BindPhase::Activating;
                        } else {
                            return;
                        }
                    }
                    match crate::device::bind_activate(&bind_url, &activation).await {
                        Ok(_) => {
                            activated = true;
                            let mut guard = bind_cancel_ref.lock().await;
                            if let Some(worker) = guard
                                .as_mut()
                                .filter(|worker| worker.generation == generation)
                            {
                                worker.phase = BindPhase::Finalizing;
                            }
                        }
                        Err(e) if crate::device::is_terminal_bind_activation_error(&e) => {
                            // Cloud's terminal response is only accepted after
                            // its idempotent MySQL lookup.  It therefore proves
                            // this provisional token was not activated.
                            tracing::warn!("设备激活被 Cloud 拒绝，不再重试: {}", e);
                            let _ = crate::device::rollback_unactivated_token(&activation);
                            let _ = crate::device::clear_pending_activation();
                            let _ = crate::device::clear_pending_binding();
                            let _ = state.transition(StateEvent::BindTimeout).await;
                            let mut guard = bind_cancel_ref.lock().await;
                            if guard.as_ref().map(|worker| worker.generation) == Some(generation) {
                                *guard = None;
                            }
                            return;
                        }
                        Err(e) => {
                            tracing::warn!("设备激活结果不确定，将保留本地标记重试: {}", e);
                            tokio::time::sleep(std::time::Duration::from_secs(30)).await;
                            continue;
                        }
                    }
                }

                // Both cleanup files are part of the activation protocol.  A
                // failure leaves the activation marker in place and therefore
                // keeps WSS blocked until the exact recovery can complete.
                if let Err(e) = crate::device::clear_pending_binding() {
                    tracing::error!(
                        "Cloud 已激活设备，但 pending binding 清理失败，将收尾重试: {}",
                        e
                    );
                    tokio::time::sleep(std::time::Duration::from_secs(30)).await;
                    continue;
                }
                if let Err(e) = crate::device::clear_pending_activation() {
                    tracing::error!(
                        "Cloud 已激活设备，但 activation marker 清理失败，将收尾重试: {}",
                        e
                    );
                    tokio::time::sleep(std::time::Duration::from_secs(30)).await;
                    continue;
                }
                let _ = state.transition(StateEvent::BindResult).await;
                let _ = wss_trigger.send(());
                let mut guard = bind_cancel_ref.lock().await;
                if guard.as_ref().map(|worker| worker.generation) == Some(generation) {
                    *guard = None;
                }
                return;
            }
        });
    }

    /// Main finishes its startup state transition asynchronously. Wait briefly
    /// for `Unbound` rather than launching a durable worker from `Starting`,
    /// which would make a later BindResult transition invalid.
    async fn transition_to_binding(&self) -> Result<()> {
        for _ in 0..20 {
            match self.state.current().await {
                crate::state::AgentState::Unbound | crate::state::AgentState::Binding => {
                    self.state.transition(StateEvent::BindInit).await?;
                    return Ok(());
                }
                crate::state::AgentState::Starting => {
                    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                }
                state => {
                    return Err(AgentError::StateTransition {
                        from: state.name().to_owned(),
                        event: "BindInit".into(),
                    });
                }
            }
        }
        Err(AgentError::StateTransition {
            from: "starting".into(),
            event: "BindInit".into(),
        })
    }

    /// `cancel_bind` — cancel an in-progress device binding.
    ///
    /// Called by the frontend when the user dismisses the BindDialog or
    /// the BindDialog times out. Cancels the background polling task so
    /// it doesn't waste resources.
    async fn handle_cancel_bind(&self, req: &IpcRequest) -> String {
        let current_gen = self.bind_generation.load(Ordering::Relaxed);
        let mut guard = self.bind_cancel.lock().await;
        if let Some(worker) = guard.as_ref() {
            if !worker.phase.can_cancel() || crate::device::load_pending_activation().is_some() {
                return ok_response(
                    &req.id,
                    serde_json::json!({
                        "status": "activation_uncertain",
                        "message": "电脑正在确认正式绑定，暂时不能取消"
                    }),
                );
            }
        }
        if let Some(worker) = guard.take() {
            // R1: Only cancel if this is still the latest bind (stale cancel from old dialog)
            if worker.generation != current_gen {
                tracing::info!(
                    "取消绑定请求已过期 (gen={}, current={})，忽略",
                    worker.generation,
                    current_gen
                );
                return ok_response(&req.id, serde_json::json!({"status": "stale_cancel"}));
            }
            worker.cancel.cancel();
            let pending_to_cancel = crate::device::load_pending_binding();
            drop(guard);
            if let Some(pending) = pending_to_cancel {
                if let Err(e) = crate::device::bind_cancel(&self.bind_http_url, &pending).await {
                    tracing::warn!("Cloud 绑定取消未确认，将等待 Redis TTL: {}", e);
                }
            }
            // The phase lock proves no marker exists at this point.  Never
            // delete one here: a worker that has reached credential saving is
            // activation-uncertain and must finish its idempotent recovery.
            // The Cloud request awaited above.  If a new dialog started in
            // that interval, its generation owns the durable files and state.
            if self.bind_generation.load(Ordering::Relaxed) == current_gen {
                let _ = crate::device::clear_pending_binding();
                let _ = self.state.transition(StateEvent::BindTimeout).await;
            }
            tracing::info!("绑定申请已显式取消");
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

        // Create session record first
        match self
            .sessions
            .create(
                nid.clone(),
                "desktop".to_string(),
                tool.to_string(),
                profile.clone(),
                cwd.clone(),
                crate::session::SessionKind::Native,
            )
            .await
        {
            Ok(session) => {
                // Desktop PTY sessions should NOT have remote control enabled by default.
                let _ = self.sessions.set_remote_enabled(&nid, false).await;

                // Apply custom dimensions if non-default
                if cols != 80 || rows != 24 {
                    let _ = self.sessions.resize(&nid, cols, rows).await;
                }

                // Spawn PTY + CLI process
                let (wss_tx, wss_rx) = mpsc::unbounded_channel::<String>();
                let (ipc_tx, _ipc_rx) = mpsc::unbounded_channel::<String>();

                // 本地会话不需要同步到云端，等用户开启远程时再发 session_created

                // ── WSS 转发 task：OutputFanout → 全局 outgoing 通道 ──
                let out2 = self.outgoing_tx_ref.clone();
                tokio::spawn(async move {
                    let mut rx = wss_rx;
                    while let Some(msg) = rx.recv().await {
                        if let Some(tx) = out2.lock().await.as_ref() {
                            let _ = tx.send(msg);
                        }
                    }
                    tracing::debug!("📤 [OUTPUT] WSS 转发 task 退出 (desktop)");
                });

                let merger = self.input_merger.clone();
                let session_nid = nid.clone();
                let sessions = self.sessions.clone();
                let tool_owned = tool.to_string();
                let profile_owned = profile.clone();
                let cwd_owned = cwd.clone();
                let remote_enabled = Some(session.remote_enabled.clone());
                // clone 在 remote_enabled 被 move 进 start_session 之前，供错误清理使用
                let remote_enabled_for_cleanup = remote_enabled.clone();
                let out = self.outgoing_tx_ref.clone();

                tokio::spawn(async move {
                    let sessions_for_cleanup = sessions.clone();
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
                            remote_enabled,
                        )
                        .await
                    {
                        Ok(_fanout) => {
                            tracing::info!(nid = %session_nid, tool = %tool_owned, "PTY session started");
                        }
                        Err(e) => {
                            tracing::error!(nid = %session_nid, error = %e, "PTY session start failed — cleaning up orphaned session record");
                            // 清理残留的 session 记录，防止变成永久僵尸会话
                            let _ = sessions_for_cleanup.end(&session_nid).await;
                            if let Ok(Some(msg)) = sessions_for_cleanup
                                .report_session_ended(&session_nid, "start_failed")
                                .await
                            {
                                // 只对开启了远程的会话同步到云端
                                let is_remote = remote_enabled_for_cleanup
                                    .as_ref()
                                    .map(|f| f.load(std::sync::atomic::Ordering::Relaxed))
                                    .unwrap_or(false);
                                if is_remote {
                                    if let Some(tx) = out.lock().await.as_ref() {
                                        let _ = tx.send(msg.to_json());
                                    }
                                }
                            }
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
            Ok(sock_path) => ok_response(
                &req.id,
                serde_json::json!({
                    "ok": true,
                    "nid": nid,
                    "pty_sock": sock_path.to_string_lossy()
                }),
            ),
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
                let _ = self
                    .sessions
                    .set_viewport_owner(&nid, ViewportOwner::Desktop)
                    .await;
                self.input_merger
                    .push(InputMessage {
                        session_id: nid.clone(),
                        text,
                        source: "desktop".into(),
                    })
                    .await;
                ok_response(&req.id, serde_json::json!({"ok": true, "nid": nid}))
            }
            Ok(None) => err_response(&req.id, "NOT_FOUND", &format!("会话未找到: {}", nid)),
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
                let _ = self
                    .sessions
                    .set_viewport_owner(&nid, ViewportOwner::Desktop)
                    .await;
                // Push ctrl byte as text into PTY stdin
                let text = String::from_utf8_lossy(&byte).to_string();
                self.input_merger
                    .push(InputMessage {
                        session_id: nid.clone(),
                        text,
                        source: "desktop".into(),
                    })
                    .await;
                ok_response(
                    &req.id,
                    serde_json::json!({"ok": true, "signal": signal, "nid": nid}),
                )
            }
            Ok(None) => err_response(&req.id, "NOT_FOUND", &format!("会话未找到: {}", nid)),
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

        match self
            .sessions
            .resize_from_source(nid, cols, rows, ViewportOwner::Desktop)
            .await
        {
            Ok(_) => ok_response(
                &req.id,
                serde_json::json!({"ok": true, "nid": nid, "cols": cols, "rows": rows}),
            ),
            Err(e) => err_response(&req.id, "INTERNAL", &e.to_string()),
        }
    }

    /// `kill_session` — SIGKILL PTY process + end session + notify cloud.
    async fn handle_kill_session(&self, req: &IpcRequest) -> String {
        let nid = match req.params.get("nid").and_then(|v| v.as_str()) {
            Some(n) => n,
            None => return err_response(&req.id, "INVALID_PARAMS", "缺少 nid 参数"),
        };
        let reason = req
            .params
            .get("reason")
            .and_then(|v| v.as_str())
            .unwrap_or("process_killed");

        match self.sessions.get(nid).await {
            Ok(Some(session)) => {
                let nid = session.nid.clone();
                let remote_was_enabled = session
                    .remote_enabled
                    .load(std::sync::atomic::Ordering::Relaxed);
                match self.sessions.kill_session(&nid).await {
                    Ok(()) => {
                        // Report session_ended — 只有开启了远程的会话才同步到云端
                        if let Ok(Some(msg)) =
                            self.sessions.report_session_ended(&nid, reason).await
                        {
                            if remote_was_enabled {
                                if let Some(tx) = self.outgoing_tx_ref.lock().await.as_ref() {
                                    let _ = tx.send(msg.to_json());
                                    tracing::info!(nid = %nid, reason = %reason, "session_ended 已发送到 Cloud");
                                }
                            }
                        }
                        ok_response(&req.id, serde_json::json!({"ok": true, "nid": nid}))
                    }
                    Err(e) => err_response(&req.id, "INTERNAL", &e.to_string()),
                }
            }
            Ok(None) => err_response(&req.id, "NOT_FOUND", &format!("会话未找到: {}", nid)),
            Err(e) => err_response(&req.id, "INTERNAL", &e.to_string()),
        }
    }

    /// `register_session` — register a desktop-owned PTY session with the agent.
    ///
    /// Unlike `new_session`, this does NOT spawn a PTY. The desktop already owns
    /// the PTY and just needs the agent to track the session for WSS/cloud sync.
    ///
    /// Params:
    /// - `tool` (string): CLI tool name (claude | codex | qoder)
    /// - `profile` (string, optional): profile name for env injection
    /// - `cwd` (string): working directory
    /// - `source` (string, default "desktop"): session origin
    async fn handle_register_session(&self, req: &IpcRequest) -> String {
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
        let cwd = req
            .params
            .get("cwd")
            .and_then(|v| v.as_str())
            .unwrap_or(".");
        let source = req
            .params
            .get("source")
            .and_then(|v| v.as_str())
            .unwrap_or("desktop")
            .to_string();
        let pid = req.params.get("pid").and_then(|v| v.as_u64()).unwrap_or(0) as u32;

        let nid = format!("s_{}", nanoid::nanoid!(12));

        // Create session record (Relay — agent doesn't own the PTY)
        match self
            .sessions
            .create(
                nid.clone(),
                source.clone(),
                tool.to_string(),
                profile.clone(),
                cwd.to_string(),
                crate::session::SessionKind::Relay,
            )
            .await
        {
            Ok(session) => {
                let _ = self.sessions.mark_running(&nid).await;

                // Desktop PTY sessions should NOT have remote control enabled by default.
                // The user must explicitly enable remote via the AgentPanel.
                let _ = self.sessions.set_remote_enabled(&nid, false).await;

                // 存储 PID（用于心跳检测 + agent 重启恢复）
                if pid > 0 {
                    self.sessions.set_child_pid(&nid, pid).await;
                    if let Ok(Some(current)) = self.sessions.get(&nid).await {
                        let _ = crate::session::persistence::write_session_record(&current, pid);
                    }
                }

                // 本地会话不需要同步到云端，等用户开启远程时再发 session_created

                ok_response(
                    &req.id,
                    serde_json::json!({
                        "nid": session.nid,
                        "tool": session.tool,
                        "profile": session.profile,
                        "cwd": session.cwd,
                        "source": session.source,
                        "status": "registered",
                        "created_at": session.created_at.to_rfc3339(),
                    }),
                )
            }
            Err(e) => err_response(&req.id, "INTERNAL", &e.to_string()),
        }
    }

    /// `relay_exit` — mark a desktop-owned Relay PTY as ended without killing a process.
    async fn handle_relay_exit(&self, req: &IpcRequest) -> String {
        let nid = match req.params.get("nid").and_then(|v| v.as_str()) {
            Some(n) => n,
            None => return err_response(&req.id, "INVALID_PARAMS", "缺少 nid 参数"),
        };
        let reason = req
            .params
            .get("reason")
            .and_then(|v| v.as_str())
            .unwrap_or("process_exit");

        match self.sessions.get(nid).await {
            Ok(Some(session)) => {
                if session.kind != crate::session::SessionKind::Relay {
                    return err_response(&req.id, "INVALID_PARAMS", "relay_exit 仅支持 Relay 会话");
                }

                let remote_was_enabled = session
                    .remote_enabled
                    .load(std::sync::atomic::Ordering::Relaxed);

                if let Ok(Some(msg)) = self.sessions.report_session_ended(nid, reason).await {
                    if remote_was_enabled {
                        if let Some(tx) = self.outgoing_tx_ref.lock().await.as_ref() {
                            let _ = tx.send(msg.to_json());
                            tracing::info!(nid = %nid, reason = %reason, "Relay session_ended 已发送到 Cloud");
                        }
                    }
                }
                self.sessions.clear_child_pid(nid).await;

                ok_response(
                    &req.id,
                    serde_json::json!({"ok": true, "nid": nid, "reason": reason}),
                )
            }
            Ok(None) => err_response(&req.id, "NOT_FOUND", &format!("会话未找到: {}", nid)),
            Err(e) => err_response(&req.id, "INTERNAL", &e.to_string()),
        }
    }

    /// `set_remote_enabled` — enable/disable remote control for a session.
    ///
    /// 开启: 发 session_created → 等 ACK → 成功才设 remote_enabled=true
    /// 关闭: 发 session_ended(remote_disabled) → fire-and-forget → 立即设 remote_enabled=false
    async fn handle_set_remote_enabled(&self, req: &IpcRequest) -> String {
        let nid = match req.params.get("nid").and_then(|v| v.as_str()) {
            Some(n) => n,
            None => return err_response(&req.id, "INVALID_PARAMS", "缺少 nid 参数"),
        };
        let enabled = req
            .params
            .get("enabled")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);

        if enabled {
            // ── 检查 WSS 连接 ──
            let outgoing_tx = {
                let guard = self.outgoing_tx_ref.lock().await;
                match guard.as_ref() {
                    Some(tx) => tx.clone(),
                    None => {
                        return err_response(
                            &req.id,
                            "WSS_NOT_CONNECTED",
                            "Agent 未连接到云端，请先绑定设备",
                        );
                    }
                }
            };

            // ── 开启远程 ──
            // 1. 复活已结束会话
            let _ = self.sessions.reactivate_if_ended(nid).await;

            // 2. 发 session_created + 等 ACK
            if let Ok(Some(session)) = self.sessions.get(nid).await {
                let msg = crate::proto::WsMessageBuilder::session_created_with_msg_id(
                    nid,
                    &session.tool,
                    &session.cwd,
                    session.profile.as_deref(),
                    session.cols,
                    session.rows,
                    "desktop",
                    Some(&format!("desktop-{}", nid)),
                );
                let rx = self.ack_registry.register(nid).await;
                if let Err(e) = outgoing_tx.send(msg) {
                    let _ = self
                        .ack_registry
                        .resolve(
                            nid,
                            crate::ack::AckResult::Error(format!("send failed: {}", e)),
                        )
                        .await;
                    return err_response(&req.id, "WSS_SEND_FAILED", &e.to_string());
                }

                tracing::info!(nid = %nid, "等待 WSS ACK");
                match tokio::time::timeout(tokio::time::Duration::from_secs(10), rx).await {
                    Ok(Ok(crate::ack::AckResult::Ok)) => {
                        // ACK 成功 → 原子检查 limit + 设 remote_enabled=true
                        match self.sessions.try_enable_remote(nid).await {
                            Ok(()) => {
                                tracing::info!(nid = %nid, "远程已开启");
                            }
                            Err(AgentError::SessionLimit { current, max }) => {
                                // 并发超限 → 清理云端残留
                                let cleanup = crate::proto::WsMessageBuilder::session_ended(
                                    nid,
                                    "remote_disabled",
                                );
                                let _ = outgoing_tx.send(cleanup);
                                return err_response(
                                    &req.id,
                                    "REMOTE_LIMIT",
                                    &format!("已达上限({}/{})，请先关闭其他远程", current, max),
                                );
                            }
                            Err(e) => {
                                return err_response(&req.id, "INTERNAL", &e.to_string());
                            }
                        }
                    }
                    Ok(Ok(crate::ack::AckResult::Error(e))) => {
                        return err_response(&req.id, "WSS_ACK_ERROR", &e);
                    }
                    _ => {
                        return err_response(&req.id, "WSS_ACK_TIMEOUT", "云端确认超时，请重试");
                    }
                }
            }
        } else {
            // ── 关闭远程 ──
            // fire-and-forget: 云端不返回 session_ended_ack，不等 ACK。
            // 即使当前 WSS 不在线，也先关闭本地 remote_enabled，防止重连后再次暴露。
            if let Some(tx) = self.outgoing_tx_ref.lock().await.as_ref() {
                let msg = crate::proto::WsMessageBuilder::session_ended(nid, "remote_disabled");
                let _ = tx.send(msg);
            }
            let _ = self.sessions.set_remote_enabled(nid, false).await;
            tracing::info!(nid = %nid, "远程已关闭（fire-and-forget）");
        }

        ok_response(
            &req.id,
            serde_json::json!({"ok": true, "nid": nid, "remote_enabled": enabled}),
        )
    }

    /// `relay_output` — desktop-owned PTY output for a Relay session.
    async fn handle_relay_output(&self, req: &IpcRequest) -> String {
        let nid = match req.params.get("nid").and_then(|v| v.as_str()) {
            Some(n) => n,
            None => return err_response(&req.id, "INVALID_PARAMS", "缺少 nid 参数"),
        };
        let data = match req.params.get("data").and_then(|v| v.as_str()) {
            Some(d) => d,
            None => return err_response(&req.id, "INVALID_PARAMS", "缺少 data 参数"),
        };

        match self.sessions.get(nid).await {
            Ok(Some(session)) => {
                if session.kind != crate::session::SessionKind::Relay {
                    return err_response(
                        &req.id,
                        "INVALID_PARAMS",
                        "relay_output 仅支持 Relay 会话",
                    );
                }
                session.record_output_snippet(data);
                crate::session::OutputFanout::append_log_static(nid, data.as_bytes());

                if session
                    .remote_enabled
                    .load(std::sync::atomic::Ordering::Relaxed)
                {
                    if let Some(tx) = self.outgoing_tx_ref.lock().await.as_ref() {
                        let _ = tx.send(crate::proto::WsMessageBuilder::output(nid, data));
                    }
                }
                ok_response(&req.id, serde_json::json!({"ok": true, "nid": nid}))
            }
            Ok(None) => err_response(&req.id, "NOT_FOUND", &format!("会话未找到: {}", nid)),
            Err(e) => err_response(&req.id, "INTERNAL", &e.to_string()),
        }
    }

    /// `poll_relay_input` — desktop polls remote input queued for a Relay session.
    async fn handle_poll_relay_input(&self, req: &IpcRequest) -> String {
        let nid = match req.params.get("nid").and_then(|v| v.as_str()) {
            Some(n) => n,
            None => return err_response(&req.id, "INVALID_PARAMS", "缺少 nid 参数"),
        };

        match self.sessions.get(nid).await {
            Ok(Some(session)) => {
                let status = match session.status {
                    crate::session::SessionStatus::Created => "created",
                    crate::session::SessionStatus::Running => "running",
                    crate::session::SessionStatus::Ended => "ended",
                };
                let inputs = if session.kind == crate::session::SessionKind::Relay
                    && session.status != crate::session::SessionStatus::Ended
                {
                    match self.sessions.take_relay_inputs(nid).await {
                        Ok(items) => items,
                        Err(e) => return err_response(&req.id, "INTERNAL", &e.to_string()),
                    }
                } else {
                    Vec::new()
                };

                ok_response(
                    &req.id,
                    serde_json::json!({
                        "ok": true,
                        "nid": nid,
                        "inputs": inputs,
                        "status": status,
                        "ended": session.status == crate::session::SessionStatus::Ended,
                        "cols": session.cols,
                        "rows": session.rows,
                        "viewport_owner": session.viewport_owner.as_str(),
                    }),
                )
            }
            Ok(None) => ok_response(
                &req.id,
                serde_json::json!({
                    "ok": true,
                    "nid": nid,
                    "inputs": [],
                    "status": "ended",
                    "ended": true,
                }),
            ),
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
    let kind = match s.kind {
        crate::session::SessionKind::Native => "Native",
        crate::session::SessionKind::Relay => "Relay",
    };

    serde_json::json!({
        "nid": s.nid,
        "kind": kind,
        "source": s.source,
        "tool": s.tool,
        "profile": s.profile,
        "cwd": s.cwd,
        "cols": s.cols,
        "rows": s.rows,
        "viewport_owner": s.viewport_owner.as_str(),
        "created_at": s.created_at.to_rfc3339(),
        "status": match s.status {
            crate::session::SessionStatus::Created => "created",
            crate::session::SessionStatus::Running => "running",
            crate::session::SessionStatus::Ended => "ended",
        },
        "remote_enabled": s.remote_enabled,
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
        let parsed: serde_json::Value = serde_json::from_str(resp.trim_end()).unwrap();
        assert_eq!(parsed["id"], "abc");
        assert_eq!(parsed["result"]["key"], "value");
    }

    #[test]
    fn test_err_response_format() {
        let resp = err_response("abc", "NOT_FOUND", "session not found");
        assert!(resp.ends_with('\n'));
        let parsed: serde_json::Value = serde_json::from_str(resp.trim_end()).unwrap();
        assert_eq!(parsed["id"], "abc");
        assert_eq!(parsed["error"]["code"], "NOT_FOUND");
        assert_eq!(parsed["error"]["message"], "session not found");
    }

    #[test]
    fn test_session_to_json_includes_kind_and_source() {
        let summary = crate::session::types::SessionSummary {
            nid: "s_test".to_string(),
            kind: crate::session::types::SessionKind::Native,
            tool: "claude".to_string(),
            profile: Some("work".to_string()),
            cwd: "/tmp/project".to_string(),
            source: "desktop".to_string(),
            cols: 100,
            rows: 30,
            viewport_owner: crate::session::types::ViewportOwner::Desktop,
            created_at: chrono::Utc::now(),
            status: crate::session::SessionStatus::Running,
            remote_enabled: false,
        };

        let json = session_to_json(&summary);

        assert_eq!(json["kind"], "Native");
        assert_eq!(json["source"], "desktop");
        assert_eq!(json["nid"], "s_test");
    }

    #[test]
    fn test_parse_error_no_id() {
        let resp = parse_error("expected value");
        let parsed: serde_json::Value = serde_json::from_str(resp.trim_end()).unwrap();
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

    #[test]
    fn binding_phase_allows_cancel_only_before_credential_persistence() {
        assert!(BindPhase::WaitingPhone.can_cancel());
        assert!(!BindPhase::SavingCredential.can_cancel());
        assert!(!BindPhase::Activating.can_cancel());
        assert!(!BindPhase::Finalizing.can_cancel());
    }
}
