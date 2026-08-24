//! kn-agent — PTY 多路复用守护进程
//!
//! 让用户通过 iOS 远程控制 Mac 上运行的 AI CLI 工具（Claude Code、Codex 等）。

#![allow(dead_code)]

use clap::Parser;
use kn_agent::{
    ack, bind, config, device, error::AgentError, ipc, proto, session, state, ws_client,
};
use notify::{Event, EventKind, RecursiveMode, Watcher};
use serde::Deserialize;
use std::collections::VecDeque;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

/// kn-agent — 设备绑定与 PTY 多路复用守护进程
#[derive(Parser)]
#[command(name = "kn-agent", version)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(clap::Subcommand)]
enum Command {
    /// 绑定设备到 kn iOS App
    Bind,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProjectInfo {
    name: String,
    path: String,
    default_profile: Option<String>,
    description: Option<String>,
}

// ── Project loading & watching ─────────────────────────────────

/// 读取 ~/.kn/projects.json，返回项目列表。
/// 文件不存在或解析失败时返回空列表（静默降级）。
///
/// 使用 spawn_blocking 将文件 I/O 移出 Tokio 异步运行时，
/// 避免在 worker 线程上执行阻塞操作。
async fn load_projects() -> Vec<ProjectInfo> {
    let path = kn_common::path::config_dir().join("projects.json");

    tokio::task::spawn_blocking(move || match std::fs::read_to_string(&path) {
        Ok(content) => serde_json::from_str::<Vec<ProjectInfo>>(&content).unwrap_or_else(|e| {
            tracing::warn!("解析 projects.json 失败: {}", e);
            vec![]
        }),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            tracing::debug!("projects.json 不存在，跳过项目上报");
            vec![]
        }
        Err(e) => {
            tracing::warn!("读取 projects.json 失败: {}", e);
            vec![]
        }
    })
    .await
    .unwrap_or_else(|_| {
        tracing::warn!("spawn_blocking 执行失败，跳过项目上报");
        vec![]
    })
}

async fn registered_project_path(project_path: &str) -> Option<String> {
    let registered = load_projects()
        .await
        .into_iter()
        .map(|project| std::path::PathBuf::from(project.path));
    canonical_registered_project_path(registered, project_path)
        .map(|path| path.to_string_lossy().into_owned())
}

fn canonical_registered_project_path(
    registered_paths: impl IntoIterator<Item = std::path::PathBuf>,
    project_path: &str,
) -> Option<std::path::PathBuf> {
    let requested = canonical_project_path(std::path::PathBuf::from(project_path));
    registered_paths
        .into_iter()
        .map(canonical_project_path)
        .find(|candidate| candidate == &requested)
}

fn canonical_project_path(path: std::path::PathBuf) -> std::path::PathBuf {
    path.canonicalize().unwrap_or(path)
}

async fn is_registered_project_path(project_path: &str) -> bool {
    registered_project_path(project_path).await.is_some()
}

const DELIVERY_OUTBOX_CAPACITY: usize = 128;
const DELIVERY_SEND_RETRY_INTERVAL: Duration = Duration::from_millis(200);
const DELIVERY_SEND_RETRY_TIMEOUT: Duration = Duration::from_secs(10);
const DELIVERY_ACK_RETRY_INTERVAL: Duration = Duration::from_secs(5);

struct DeliveryOutbox {
    messages: tokio::sync::Mutex<VecDeque<String>>,
    capacity: usize,
    store: Option<Arc<kn_agent::delivery_outbox_store::DeliveryOutboxStore>>,
}

impl DeliveryOutbox {
    fn with_capacity(capacity: usize) -> Self {
        assert!(capacity > 0, "delivery outbox capacity must be positive");
        Self {
            messages: tokio::sync::Mutex::new(VecDeque::with_capacity(capacity)),
            capacity,
            store: None,
        }
    }

    async fn enqueue(&self, message: String) {
        if let (Some(store), Some(request_id)) = (&self.store, delivery_request_id(&message)) {
            if let Err(error) = store.enqueue(&request_id, &message, self.capacity) {
                tracing::warn!(%error, "写入交付 outbox SQLite 失败");
            }
        }
        let mut messages = self.messages.lock().await;
        if let Some(request_id) = delivery_request_id(&message) {
            messages.retain(|queued| delivery_request_id(queued).as_deref() != Some(&request_id));
        }
        if messages.len() == self.capacity {
            messages.pop_front();
            tracing::warn!(
                capacity = self.capacity,
                "项目交付结果 outbox 已满，丢弃最早结果"
            );
        }
        messages.push_back(message);
    }

    async fn take_front(&self) -> Option<String> {
        self.messages.lock().await.front().cloned()
    }

    async fn len(&self) -> usize {
        self.messages.lock().await.len()
    }

    async fn acknowledge(&self, request_id: &str) -> bool {
        if let Some(store) = &self.store {
            if let Err(error) = store.acknowledge(request_id) {
                tracing::warn!(%error, %request_id, "确认交付 outbox SQLite 失败");
                return false;
            }
        }
        let mut messages = self.messages.lock().await;
        let before = messages.len();
        messages.retain(|message| delivery_request_id(message).as_deref() != Some(request_id));
        before != messages.len()
    }
}

fn delivery_request_id(message: &str) -> Option<String> {
    serde_json::from_str::<serde_json::Value>(message)
        .ok()?
        .get("data")?
        .get("requestId")?
        .as_str()
        .map(str::to_owned)
}

impl Default for DeliveryOutbox {
    fn default() -> Self {
        let mut outbox = Self::with_capacity(DELIVERY_OUTBOX_CAPACITY);
        let path = kn_agent::delivery_outbox_store::DeliveryOutboxStore::default_path(
            &kn_common::path::config_dir(),
        );
        match kn_agent::delivery_outbox_store::DeliveryOutboxStore::open(path) {
            Ok(store) => {
                if let Ok(pending) = store.pending() {
                    outbox.messages = tokio::sync::Mutex::new(pending.into());
                }
                outbox.store = Some(Arc::new(store));
            }
            Err(error) => tracing::warn!(%error, "打开交付 outbox SQLite 失败"),
        }
        outbox
    }
}

enum DeliverySendAttempt {
    Sent,
    NoSender(String),
    SenderClosed(String),
}

async fn try_send_project_delivery_message(
    outgoing: &Arc<tokio::sync::Mutex<Option<mpsc::UnboundedSender<String>>>>,
    message: String,
) -> DeliverySendAttempt {
    let sender = outgoing.lock().await.as_ref().cloned();
    match sender {
        Some(sender) => match sender.send(message) {
            Ok(()) => DeliverySendAttempt::Sent,
            Err(error) => DeliverySendAttempt::SenderClosed(error.0),
        },
        None => DeliverySendAttempt::NoSender(message),
    }
}

async fn send_project_delivery_message(
    outgoing: &Arc<tokio::sync::Mutex<Option<mpsc::UnboundedSender<String>>>>,
    message: String,
    message_type: &'static str,
) -> Result<(), String> {
    let deadline = Instant::now() + DELIVERY_SEND_RETRY_TIMEOUT;
    let mut message = message;
    let mut failed_sends = 0;

    loop {
        match try_send_project_delivery_message(outgoing, message).await {
            DeliverySendAttempt::Sent => return Ok(()),
            DeliverySendAttempt::NoSender(unsent) => {
                message = unsent;
                tracing::debug!(message_type, "项目交付消息等待 WSS 通道恢复");
            }
            DeliverySendAttempt::SenderClosed(unsent) => {
                message = unsent;
                failed_sends += 1;
                tracing::warn!(
                    message_type,
                    failed_sends,
                    "项目交付消息发送失败，等待 WSS 通道更新"
                );
                if failed_sends >= 2 {
                    tracing::warn!(message_type, "项目交付消息两次发送失败，停止重试");
                    return Err(message);
                }
            }
        }

        let now = Instant::now();
        if now >= deadline {
            tracing::warn!(message_type, "项目交付消息等待 WSS 通道超时");
            return Err(message);
        }
        tokio::time::sleep((deadline - now).min(DELIVERY_SEND_RETRY_INTERVAL)).await;
    }
}

async fn send_project_delivery_result(
    outgoing: &Arc<tokio::sync::Mutex<Option<mpsc::UnboundedSender<String>>>>,
    outbox: &DeliveryOutbox,
    message: String,
    _message_type: &'static str,
) {
    outbox.enqueue(message).await;
    flush_delivery_outbox(outbox, outgoing).await;
}

async fn flush_delivery_outbox(
    outbox: &DeliveryOutbox,
    outgoing: &Arc<tokio::sync::Mutex<Option<mpsc::UnboundedSender<String>>>>,
) {
    if let Some(message) = outbox.take_front().await {
        match try_send_project_delivery_message(outgoing, message).await {
            DeliverySendAttempt::Sent => {}
            DeliverySendAttempt::NoSender(_) => {
                tracing::debug!("项目交付 outbox 等待 WSS 通道恢复");
                return;
            }
            DeliverySendAttempt::SenderClosed(_) => {
                tracing::warn!("项目交付 outbox 刷新失败，保留未发送结果");
                return;
            }
        }
    }
}

async fn delivery_outbox_retry_loop(
    outbox: Arc<DeliveryOutbox>,
    outgoing: Arc<tokio::sync::Mutex<Option<mpsc::UnboundedSender<String>>>>,
    retry_interval: Duration,
    shutdown: CancellationToken,
) {
    loop {
        tokio::select! {
            _ = shutdown.cancelled() => return,
            _ = tokio::time::sleep(retry_interval) => flush_delivery_outbox(&outbox, &outgoing).await,
        }
    }
}

fn canonical_project_key(device_id: u64, project_path: &str) -> String {
    format!("{device_id}:{}", project_path.trim())
}

/// 发送 project_list 到云端。
async fn send_project_list(
    outgoing: &std::sync::Arc<
        tokio::sync::Mutex<Option<tokio::sync::mpsc::UnboundedSender<String>>>,
    >,
) {
    let projects = load_projects().await;
    let info: Vec<proto::ProjectInfoOut> = projects
        .iter()
        .map(|p| proto::ProjectInfoOut {
            name: p.name.clone(),
            path: p.path.clone(),
            default_profile: p.default_profile.clone(),
            description: p.description.clone(),
        })
        .collect();
    let msg = proto::WsMessageBuilder::project_list(&info);
    if let Some(tx) = outgoing.lock().await.as_ref() {
        let _ = tx.send(msg);
        tracing::info!(count = info.len(), "已上报项目列表");
    }
}

/// Publishes bounded, complete metadata snapshots after reconnect and local
/// session activity. Scanning is offloaded so it never blocks WSS dispatch.
async fn send_project_session_indexes(
    outgoing: &Arc<tokio::sync::Mutex<Option<mpsc::UnboundedSender<String>>>>,
    revisions: &Arc<tokio::sync::Mutex<kn_agent::project_session_index::ProjectRevisionClock>>,
    activity: &Arc<tokio::sync::Mutex<kn_agent::project_session_index::ProjectActivityTracker>>,
    scan_gate: &Arc<kn_agent::project_session_index::ProjectScanGate>,
) {
    let projects = load_projects().await;
    for project in projects {
        let project_path = project.path;
        send_project_session_index(outgoing, revisions, activity, scan_gate, project_path).await;
    }
}

async fn send_project_session_index(
    outgoing: &Arc<tokio::sync::Mutex<Option<mpsc::UnboundedSender<String>>>>,
    revisions: &Arc<tokio::sync::Mutex<kn_agent::project_session_index::ProjectRevisionClock>>,
    activity: &Arc<tokio::sync::Mutex<kn_agent::project_session_index::ProjectActivityTracker>>,
    scan_gate: &Arc<kn_agent::project_session_index::ProjectScanGate>,
    project_path: String,
) {
    if !scan_gate.begin(&project_path) {
        return;
    }

    loop {
        let allow_qoderclicn_fallback = activity
            .lock()
            .await
            .claim_qoderclicn_fallback(&project_path, unix_millis());
        let scan_path = project_path.clone();
        let scan = tokio::task::spawn_blocking(move || {
            kn_agent::project_session_index::scan_project_history(
                &scan_path,
                allow_qoderclicn_fallback,
            )
        })
        .await
        .unwrap_or_else(|error| {
            tracing::warn!(project_path = %project_path, %error, "会话索引扫描任务失败");
            kn_agent::project_session_index::ProjectSessionScan {
                sessions: Vec::new(),
                complete: false,
            }
        });
        let revision = match revisions.lock().await.next(&project_path) {
            Ok(revision) => revision,
            Err(error) => {
                tracing::warn!(project_path = %project_path, %error, "会话索引 revision 未持久化，跳过发送");
                continue;
            }
        };
        let message = proto::WsMessageBuilder::project_session_index(
            &project_path,
            revision,
            scan.complete,
            &scan.sessions,
        );
        if let Some(tx) = outgoing.lock().await.as_ref() {
            if tx.send(message).is_err() {
                tracing::warn!(project_path = %project_path, "会话索引快照发送失败");
            } else {
                tracing::info!(
                    project_path = %project_path,
                    revision,
                    complete = scan.complete,
                    session_count = scan.sessions.len(),
                    "已上报项目会话索引"
                );
            }
        }
        if !scan_gate.finish(&project_path) {
            break;
        }
    }
}

fn unix_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

/// 启动 projects.json 文件监听，变化时自动重新上报。
/// 返回 watcher handle，需要保持存活（drop 时停止监听）。
fn start_project_watcher(
    outgoing: std::sync::Arc<
        tokio::sync::Mutex<Option<tokio::sync::mpsc::UnboundedSender<String>>>,
    >,
) -> Option<notify::RecommendedWatcher> {
    let path = kn_common::path::config_dir().join("projects.json");

    // 使用 tokio::sync::mpsc 避免在 Tokio 任务中阻塞 worker 线程
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<()>();

    // 记录要监听的文件名，用于在回调中过滤
    let target_name = path
        .file_name()
        .map(|n| n.to_os_string())
        .unwrap_or_default();

    let mut watcher = match notify::recommended_watcher(move |res: Result<Event, notify::Error>| {
        if let Ok(event) = res {
            // 只响应 projects.json 的变更（原子替换 → 父目录下其他文件不改触发）
            let is_projects = event
                .paths
                .iter()
                .any(|p| p.file_name().map_or(false, |n| n == target_name));
            if is_projects && matches!(event.kind, EventKind::Modify(_) | EventKind::Create(_)) {
                tracing::debug!(paths = ?event.paths, "projects.json 变更，触发重新上报");
                let _ = tx.send(());
            }
        }
    }) {
        Ok(w) => w,
        Err(e) => {
            tracing::warn!("创建文件监听器失败: {}", e);
            return None;
        }
    };

    // Watch the parent directory (kn config dir) since projects.json
    // might be atomically replaced (write to temp → rename)
    let watch_dir = path.parent().unwrap_or(&path);
    if let Err(e) = watcher.watch(watch_dir, RecursiveMode::NonRecursive) {
        tracing::warn!("注册文件监听失败: {}", e);
        return None;
    }

    // Spawn a task to handle events with debounce
    tokio::spawn(async move {
        loop {
            match rx.recv().await {
                Some(()) => {
                    // 简单防抖：收到事件后等 2 秒，期间的新事件被丢弃
                    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
                    // 排空积压事件
                    while rx.try_recv().is_ok() {}
                    send_project_list(&outgoing).await;
                }
                None => break,
            }
        }
    });

    Some(watcher)
}

/// Watches native CLI history roots. Events are debounced before resolving the
/// affected registered project, and the per-project scan gate coalesces any
/// event that races with an already-running scan.
fn start_project_session_history_watcher(
    outgoing: Arc<tokio::sync::Mutex<Option<mpsc::UnboundedSender<String>>>>,
    revisions: Arc<tokio::sync::Mutex<kn_agent::project_session_index::ProjectRevisionClock>>,
    activity: Arc<tokio::sync::Mutex<kn_agent::project_session_index::ProjectActivityTracker>>,
    scan_gate: Arc<kn_agent::project_session_index::ProjectScanGate>,
) -> Option<notify::RecommendedWatcher> {
    let home = kn_common::path::home_dir();
    let roots = [
        home.join(".claude/projects"),
        home.join(".codex/sessions"),
        home.join(".qoder-cn/projects"),
    ];
    let (tx, mut rx) = mpsc::unbounded_channel::<Vec<std::path::PathBuf>>();
    let mut watcher =
        match notify::recommended_watcher(move |result: Result<Event, notify::Error>| {
            if let Ok(event) = result {
                if matches!(
                    event.kind,
                    EventKind::Modify(_) | EventKind::Create(_) | EventKind::Remove(_)
                ) {
                    let _ = tx.send(event.paths);
                }
            }
        }) {
            Ok(watcher) => watcher,
            Err(error) => {
                tracing::warn!(%error, "创建会话历史文件监听器失败");
                return None;
            }
        };

    let mut watched_root_count = 0usize;
    for root in roots {
        if !root.exists() {
            tracing::debug!(path = %root.display(), "会话历史目录不存在，跳过监听");
            continue;
        }
        match watcher.watch(&root, RecursiveMode::Recursive) {
            Ok(()) => watched_root_count += 1,
            Err(error) => {
                tracing::warn!(path = %root.display(), %error, "注册会话历史目录监听失败")
            }
        }
    }
    if watched_root_count == 0 {
        return None;
    }

    tokio::spawn(async move {
        while let Some(mut paths) = rx.recv().await {
            tokio::time::sleep(Duration::from_secs(2)).await;
            while let Ok(mut more_paths) = rx.try_recv() {
                paths.append(&mut more_paths);
            }
            let projects = load_projects().await;
            let project_paths: Vec<String> =
                projects.into_iter().map(|project| project.path).collect();
            let affected = kn_agent::project_session_index::projects_affected_by_history_paths(
                &paths,
                &project_paths,
            );
            for project_path in affected {
                activity
                    .lock()
                    .await
                    .mark_active(&project_path, unix_millis());
                send_project_session_index(
                    &outgoing,
                    &revisions,
                    &activity,
                    &scan_gate,
                    project_path,
                )
                .await;
            }
        }
    });

    Some(watcher)
}

fn start_qoderclicn_history_fallback_loop(
    outgoing: Arc<tokio::sync::Mutex<Option<mpsc::UnboundedSender<String>>>>,
    revisions: Arc<tokio::sync::Mutex<kn_agent::project_session_index::ProjectRevisionClock>>,
    activity: Arc<tokio::sync::Mutex<kn_agent::project_session_index::ProjectActivityTracker>>,
    scan_gate: Arc<kn_agent::project_session_index::ProjectScanGate>,
    shutdown: CancellationToken,
) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(60));
        interval.tick().await;
        loop {
            tokio::select! {
                _ = shutdown.cancelled() => return,
                _ = interval.tick() => {
                    if outgoing.lock().await.is_none() {
                        continue;
                    }
                    let now = unix_millis();
                    let projects = load_projects().await;
                    for project in projects {
                        if !activity.lock().await.allows_qoderclicn_fallback(&project.path, now) {
                            continue;
                        }
                        send_project_session_index(
                            &outgoing,
                            &revisions,
                            &activity,
                            &scan_gate,
                            project.path,
                        ).await;
                    }
                }
            }
        }
    });
}

#[tokio::main]
async fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    // ── 1. 加载配置 ──
    let cfg = config::AgentConfig::load()?;

    // ── 处理 bind 子命令 ──
    if let Some(Command::Bind) = cli.command {
        bind::run_bind_command(cfg).await?;
        return Ok(());
    }

    // ── 2. 初始化日志 ──
    init_logging(&cfg.log_dir)?;
    tracing::info!(
        version = env!("CARGO_PKG_VERSION"),
        git = env!("KN_BUILD_GIT_SHA"),
        "kn-agent 启动"
    );
    tracing::info!(
        "配置: cloud={}, dir={}, machine_id={}",
        cfg.cloud_url,
        cfg.config_dir.display(),
        cfg.machine_id
    );

    // ── 3. 确保目录存在 ──
    ensure_dirs(&cfg.agent_dir, &cfg.log_dir)?;

    // Parser profiles are optional runtime metadata. A failed refresh never
    // blocks the Agent: the validated on-disk cache and built-in parsers remain
    // the safe fallback for offline or older Cloud deployments.
    let mut parser_profiles = session::terminal_profiles::TerminalProfileStore::new(
        cfg.agent_dir.join("terminal-parser-profiles.json"),
    );
    let _ = parser_profiles.load_cached();
    session::terminal_profiles::set_active(parser_profiles.current().cloned());
    let profile_http_url = cfg.cloud_http_url.clone();
    tokio::spawn(async move {
        let client = reqwest::Client::new();
        let result = tokio::time::timeout(
            std::time::Duration::from_secs(3),
            parser_profiles.refresh_from_cloud(&client, &profile_http_url),
        )
        .await
        .unwrap_or_else(|_| {
            Err(session::terminal_profiles::ProfileError::Io(
                "profile refresh timeout".into(),
            ))
        });
        match result {
            Ok(()) => session::terminal_profiles::set_active(parser_profiles.current().cloned()),
            Err(error) => tracing::debug!(
                ?error,
                "parser profile refresh skipped; using cached or built-in rules"
            ),
        }
    });

    // 仅清理带有 kn-agent 旁证且所属进程已退出的 Git 锁；普通
    // `.git/index.lock` 永远不自动删除，避免误伤用户正在执行的 Git 操作。
    for project in load_projects().await {
        session::git_delivery::recover_stale_agent_lock(&project.path).await;
    }

    // ── 4. 崩溃计数 ──
    let crash_count = state::StateMachine::load_crash_count();
    if crash_count > 0 {
        tracing::info!("上次崩溃计数: {}", crash_count);
    }

    // ── 5. 创建状态机 ──
    let state_machine = Arc::new(state::StateMachine::new(crash_count));

    // ── 6. 关闭信号 ──
    let shutdown = CancellationToken::new();
    let shutdown_clone = shutdown.clone();
    tokio::spawn(async move {
        tokio::signal::ctrl_c().await.ok();
        tracing::info!("收到中断信号，正在关闭...");
        shutdown_clone.cancel();
    });

    // ── 7. 启动 → 递增崩溃计数 ──
    state_machine.transition(state::StateEvent::Start).await?;
    let new_count = state_machine.increment_crash();
    state::StateMachine::persist_crash_count(new_count);

    if state_machine.in_safe_mode() {
        tracing::warn!("安全模式：崩溃 {} 次，仅限查询操作", new_count);
    }

    let sm = state_machine.clone();
    tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_secs(60)).await;
        sm.reset_crash();
        state::StateMachine::clear_crash_count();
        tracing::info!("崩溃计数已重置（运行超过 60s）");
    });

    // ── 8. 检查 device_token ──
    // A device token is not WSS-eligible until Cloud has acknowledged the
    // activation marker. This prevents a crash between local token persistence
    // and bind-activate from connecting an unregistered device.
    let activation_pending = device::wss_is_blocked_by_pending_activation();
    let token = if activation_pending {
        tracing::info!("检测到待激活绑定，WSS 将在 Cloud 确认后启动");
        None
    } else {
        device::load_device_token()
    };
    let has_token = token.as_ref().map_or(false, |t| !t.is_empty());

    // ── 9. 创建共享的会话管理器和输入合并器 ──
    // IPC 和 WSS 共用同一套 sessions/input_merger，确保无论云端连接状态如何，
    // 桌面应用都能通过 IPC 与 Agent 通信。
    let store = Box::new(session::MemorySessionStore::new());
    let sessions = Arc::new(session::SessionManager::new(store));
    let input_merger = Arc::new(session::InputMerger::new());

    // ── 9.5. 恢复上次异常退出时残留的会话 ──
    match session::persistence::recover_surviving_sessions(&sessions).await {
        Ok(n) if n > 0 => tracing::info!("恢复了 {} 个残留会话", n),
        Ok(_) => {}
        Err(e) => tracing::warn!(error = %e, "会话恢复扫描失败"),
    }

    // ── 10. WSS 触发通道 ──
    // 用于在主循环中触发 WSS 连接（初始启动 + 绑定完成后）
    let (wss_trigger_tx, mut wss_trigger_rx) = mpsc::unbounded_channel::<()>();

    // ── 11. 始终启动 IPC 服务器 ──
    // ── WSS outgoing channel（提前声明，IPC 模块需要引用） ──
    let outgoing_tx_ref: Arc<tokio::sync::Mutex<Option<mpsc::UnboundedSender<String>>>> =
        Arc::new(tokio::sync::Mutex::new(None));
    let wss_cancel_ref: Arc<tokio::sync::Mutex<Option<CancellationToken>>> =
        Arc::new(tokio::sync::Mutex::new(None));

    // ── ACK 注册表（session_created → session_created_ack 关联） ──
    let ack_registry = Arc::new(ack::AckRegistry::new());

    // Git/PR 操作会执行受限的外部命令。每个项目串行执行，但绝不能阻塞
    // WSS 入站循环，否则终端输入和心跳会在 push 或 gh 查询期间排队。
    let project_delivery_gate =
        Arc::new(kn_agent::project_delivery::ProjectOperationGate::default());
    let delivery_outbox = Arc::new(DeliveryOutbox::default());
    {
        let outbox = delivery_outbox.clone();
        let outgoing = outgoing_tx_ref.clone();
        let retry_shutdown = shutdown.clone();
        tokio::spawn(async move {
            delivery_outbox_retry_loop(
                outbox,
                outgoing,
                DELIVERY_ACK_RETRY_INTERVAL,
                retry_shutdown,
            )
            .await;
        });
    }

    // IPC 服务器独立于 WSS 连接运行。即使云端不可达（如 dev 模式下
    // kn-cloud 未启动），桌面应用仍能通过 Unix socket 查询 Agent 状态。
    {
        let ipc = ipc::IpcServer::new(
            cfg.ipc_socket_path.clone(),
            state_machine.clone(),
            sessions.clone(),
            cfg.cloud_http_url.clone(),
            cfg.machine_id.clone(),
            cfg.hostname.clone(),
            cfg.purchase_url.clone(),
            input_merger.clone(),
            wss_trigger_tx.clone(),
            outgoing_tx_ref.clone(),
            wss_cancel_ref.clone(),
            ack_registry.clone(),
        );
        let ipc_shutdown = shutdown.clone();
        tokio::spawn(async move {
            if let Err(e) = ipc.run(ipc_shutdown).await {
                tracing::error!("IPC 服务器错误: {}", e);
            }
        });
    }

    // ── 12. WSS lifecycle management ──
    // WSS 连接生命周期由主事件循环统一管理：
    // - 启动时若有 token → 通过 wss_trigger 触发连接
    // - 绑定完成后 → IPC handler 通过 wss_trigger 触发连接
    // - AUTH_REJECTED → 回到 Unbound 状态，等待下次绑定触发
    // - 其他断开 → 由 run_ws_loop 内部自动重连（指数退避）

    let mut wss_active = false;
    let mut wss_task = None; // Option<JoinHandle<kn_agent::error::Result<()>>>
    let mut incoming_rx: Option<mpsc::UnboundedReceiver<proto::AgentIncoming>> = None;
    let mut _project_watcher: Option<notify::RecommendedWatcher> = None;
    let mut _project_session_history_watcher: Option<notify::RecommendedWatcher> = None;
    let project_session_revisions = Arc::new(tokio::sync::Mutex::new(
        kn_agent::project_session_index::ProjectRevisionClock::default_at_config_dir(),
    ));
    let project_session_activity = Arc::new(tokio::sync::Mutex::new(
        kn_agent::project_session_index::ProjectActivityTracker::default(),
    ));
    let project_session_scan_gate =
        Arc::new(kn_agent::project_session_index::ProjectScanGate::default());
    start_qoderclicn_history_fallback_loop(
        outgoing_tx_ref.clone(),
        project_session_revisions.clone(),
        project_session_activity.clone(),
        project_session_scan_gate.clone(),
        shutdown.clone(),
    );

    // 初始状态转换
    if has_token {
        state_machine
            .transition(state::StateEvent::WsConnected { has_token: true })
            .await?;
        // 通过 trigger 通道统一触发 WSS 启动
        let _ = wss_trigger_tx.send(());
    } else {
        state_machine
            .transition(state::StateEvent::WsConnected { has_token: false })
            .await?;
        _project_watcher = None;
    }

    tracing::info!(
        "Agent 就绪: IPC={}, WSS={}",
        cfg.ipc_socket_path.display(),
        if has_token {
            "initializing"
        } else {
            "disabled (no token)"
        }
    );
    tracing::info!("使用以下方式绑定:");
    tracing::info!("  1. 运行 'kn-agent bind' 开始绑定流程");
    tracing::info!("  2. 在 iOS App 中扫描二维码");
    tracing::info!("  3. 通过 IPC 发送 bind 请求");

    // ── 13. Main event loop ──
    loop {
        tokio::select! {
            _ = shutdown.cancelled() => {
                tracing::info!("收到关闭信号");
                break;
            }

            // ── WSS 触发：初始启动 或 绑定完成 ──
            Some(()) = wss_trigger_rx.recv() => {
                if wss_active {
                    tracing::debug!("WSS 已在运行中，忽略重复触发");
                    continue;
                }

                if device::wss_is_blocked_by_pending_activation() {
                    tracing::info!("WSS 触发被待激活绑定门控，等待本地收尾完成");
                    continue;
                }
                let t = match device::load_device_token() {
                    Some(tok) if !tok.is_empty() => tok,
                    _ => {
                        tracing::warn!("WSS 触发但未找到 device_token，跳过");
                        continue;
                    }
                };

                tracing::info!("正在启动 WSS 连接...");

                // Do not announce Connected before the actual WebSocket
                // handshake.  `ws_client::connect_and_run` performs that
                // transition only after Cloud accepts the socket.

                // 创建入站消息通道
                let (incoming_tx, rx) = mpsc::unbounded_channel::<proto::AgentIncoming>();
                incoming_rx = Some(rx);

                // 启动 project watcher
                _project_watcher = start_project_watcher(outgoing_tx_ref.clone());
                _project_session_history_watcher = start_project_session_history_watcher(
                    outgoing_tx_ref.clone(),
                    project_session_revisions.clone(),
                    project_session_activity.clone(),
                    project_session_scan_gate.clone(),
                );

                // 复制 WSS 所需参数
                let ws_token = t;
                let ws_url = cfg.cloud_url.clone();
                let ws_machine = cfg.machine_id.clone();
                let ws_version = env!("CARGO_PKG_VERSION").to_string();
                let ws_os = cfg.os_version.clone();
                let ws_host = cfg.hostname.clone();
                let ws_state = state_machine.clone();
                let ws_outgoing = outgoing_tx_ref.clone();
                let ws_shutdown = shutdown.child_token();
                *wss_cancel_ref.lock().await = Some(ws_shutdown.clone());

                // 在后台 spawn run_ws_loop（内部有无限重连逻辑）
                wss_task = Some(tokio::spawn(async move {
                    ws_client::run_ws_loop(
                        &ws_token,
                        &ws_url,
                        &ws_machine,
                        &ws_version,
                        &ws_os,
                        &ws_host,
                        ws_state,
                        ws_outgoing,
                        incoming_tx,
                        ws_shutdown,
                    )
                    .await
                }));

                wss_active = true;
                tracing::info!("WSS 连接任务已启动");

                // 启动 CLI 心跳循环（每 15s 上报进程存活状态）
                let hb_sessions = sessions.clone();
                let hb_outgoing = outgoing_tx_ref.clone();
                let hb_shutdown = shutdown.clone();
                tokio::spawn(async move {
                    cli_heartbeat_loop(hb_sessions, hb_outgoing, hb_shutdown).await;
                });
                let task_event_sessions = sessions.clone();
                let task_event_outgoing = outgoing_tx_ref.clone();
                let task_event_shutdown = shutdown.clone();
                tokio::spawn(async move {
                    task_complete_queue_loop(task_event_sessions, task_event_outgoing, task_event_shutdown).await;
                });
            }

            // ── 处理 WSS 入站消息 ──
            msg = async {
                match incoming_rx.as_mut() {
                    Some(rx) => rx.recv().await,
                    None => std::future::pending().await,
                }
            } => {
                if let Some(m) = msg {
                    let state = state_machine.clone();
                    let outgoing = outgoing_tx_ref.clone();
                    let session_manager = sessions.clone();
                    let input = input_merger.clone();
                    let acknowledgements = ack_registry.clone();
                    let delivery_gate = project_delivery_gate.clone();
                    let outbox = delivery_outbox.clone();
                    let session_revisions = project_session_revisions.clone();
                    let session_activity = project_session_activity.clone();
                    let session_scan_gate = project_session_scan_gate.clone();
                    if should_dispatch_in_background(&m) {
                        tokio::spawn(async move {
                            handle_incoming(
                                m,
                                state,
                                outgoing,
                                session_manager,
                                input,
                                acknowledgements,
                                delivery_gate,
                                outbox,
                                session_revisions,
                                session_activity,
                                session_scan_gate,
                            )
                            .await;
                        });
                    } else {
                        handle_incoming(
                            m,
                            state,
                            outgoing,
                            session_manager,
                            input,
                            acknowledgements,
                            delivery_gate,
                            outbox,
                            session_revisions,
                            session_activity,
                            session_scan_gate,
                        )
                        .await;
                    }
                }
            }

            // ── 处理 WSS 任务退出 ──
            result = async {
                match wss_task.as_mut() {
                    Some(task) => Some(task.await),
                    None => std::future::pending().await,
                }
            } => {
                match result {
                    Some(Ok(Err(ref e))) if e.to_string().contains("AUTH_REJECTED") => {
                        tracing::warn!(
                            "device_token 已失效，切换至未绑定状态（IPC 仍运行）"
                        );
                        if let Err(error) = device::quarantine_invalid_device_token("wss_auth_rejected") {
                            tracing::warn!("失效 device_token 备份失败: {}", error);
                        }
                        let _ = device::clear_pending_activation();
                        let _ = device::clear_pending_binding();
                        let _ = state_machine
                            .transition(state::StateEvent::TokenRevoked)
                            .await;
                    }
                    Some(Ok(Ok(()))) => {
                        tracing::info!("WSS 循环正常退出");
                        if shutdown.is_cancelled() {
                            break;
                        }
                        let _ = state_machine
                            .transition(state::StateEvent::WsConnected { has_token: false })
                            .await;
                    }
                    Some(Ok(Err(e))) => {
                        tracing::error!("WSS 循环错误: {}", e);
                        let _ = state_machine
                            .transition(state::StateEvent::WsConnected { has_token: false })
                            .await;
                    }
                    Some(Err(e)) => {
                        tracing::error!("WSS 任务 panic: {}", e);
                        let _ = state_machine
                            .transition(state::StateEvent::WsConnected { has_token: false })
                            .await;
                    }
                    None => {
                        tracing::debug!("WSS task handle 为 None，忽略");
                        continue;
                    }
                }

                // 清理 WSS 状态，等待下次触发
                wss_active = false;
                wss_task = None;
                incoming_rx = None;
            }
        }
    }

    // ── 14. 优雅关闭 ──
    state_machine.transition(state::StateEvent::Stop).await?;
    tracing::info!("Agent 已停止");

    Ok(())
}

// ── Message handling ────────────────────────────────────────

async fn task_complete_queue_loop(
    sessions: Arc<session::SessionManager>,
    outgoing: Arc<tokio::sync::Mutex<Option<mpsc::UnboundedSender<String>>>>,
    shutdown: CancellationToken,
) {
    loop {
        tokio::select! {
            _ = tokio::time::sleep(std::time::Duration::from_secs(15)) => {
                kn_agent::task_events::flush_task_complete_queue(sessions.clone(), outgoing.clone()).await;
            }
            _ = shutdown.cancelled() => return,
        }
    }
}

/// CLI 心跳循环：每 15s 检查所有活跃会话的进程存活状态，上报给 cloud。
///
/// 替代旧的 checkpoint + session_interrupted 机制。
async fn cli_heartbeat_loop(
    sessions: Arc<session::SessionManager>,
    outgoing: Arc<tokio::sync::Mutex<Option<mpsc::UnboundedSender<String>>>>,
    shutdown: CancellationToken,
) {
    loop {
        tokio::select! {
            _ = tokio::time::sleep(std::time::Duration::from_secs(15)) => {},
            _ = shutdown.cancelled() => {
                tracing::info!("CLI 心跳循环收到关闭信号，退出");
                return;
            }
        }

        let mut alive_sessions: Vec<proto::HeartbeatSession> = Vec::new();

        // 遍历所有非 Ended 且开启远程的会话
        if let Ok(summaries) = sessions.list().await {
            for s in &summaries {
                if s.status == session::SessionStatus::Ended {
                    continue;
                }
                if !s.remote_enabled {
                    continue;
                }

                // 尝试检查进程是否存活 (kill(pid, 0))
                let pid_opt = sessions.get_child_pid(&s.nid).await;
                let (state, pid) = if let Some(p) = pid_opt {
                    // kill(pid, 0) 不发送信号，仅检查进程是否存在
                    let is_alive = unsafe { libc::kill(p as i32, 0) == 0 };
                    if is_alive {
                        ("running", p)
                    } else {
                        // 进程已死，补发 session_ended
                        tracing::warn!(nid = %s.nid, pid = p, "CLI 进程已死亡，上报 session_ended");
                        if let Ok(Some(msg)) =
                            sessions.report_session_ended(&s.nid, "process_exit").await
                        {
                            if let Some(tx) = outgoing.lock().await.as_ref() {
                                let _ = tx.send(msg.to_json());
                            }
                        }
                        // 已死亡的会话不放入心跳（已发送 session_ended）
                        continue;
                    }
                } else {
                    // 没有记录的 pid，无法做进程存活检测，但仍需上报，
                    // 确保云端 cli:heartbeat:{nid} key 被刷新
                    ("no_pid", 0)
                };

                alive_sessions.push(proto::HeartbeatSession {
                    session_nid: s.nid.clone(),
                    pid,
                    state: state.to_string(),
                });
            }
        }

        // 发送 cli_heartbeat 给 cloud
        let count = alive_sessions.len();
        if let Some(tx) = outgoing.lock().await.as_ref() {
            let msg = proto::WsMessageBuilder::cli_heartbeat(&alive_sessions);
            match tx.send(msg) {
                Ok(_) => tracing::info!(count = count, "💓 [HEARTBEAT] cli_heartbeat 已发送"),
                Err(e) => tracing::warn!(count = count, error = %e, "💓 [HEARTBEAT] 发送失败"),
            }
        } else {
            tracing::warn!("💓 [HEARTBEAT] WSS 通道不可用，跳过");
        }
    }
}

fn should_dispatch_in_background(message: &proto::AgentIncoming) -> bool {
    matches!(
        message,
        proto::AgentIncoming::ProjectGitStatus { .. }
            | proto::AgentIncoming::ProjectListStatus { .. }
            | proto::AgentIncoming::ProjectGitCommit { .. }
            | proto::AgentIncoming::ProjectGitPush { .. }
            | proto::AgentIncoming::ProjectGitBranches { .. }
            | proto::AgentIncoming::ProjectGitCheckout { .. }
            | proto::AgentIncoming::ProjectGitCreateBranch { .. }
            | proto::AgentIncoming::ProjectPrStatus { .. }
            | proto::AgentIncoming::ProjectPrDetails { .. }
            | proto::AgentIncoming::ProjectPrCreate { .. }
            | proto::AgentIncoming::DeviceHealth { .. }
    )
}

async fn project_has_active_terminal(
    sessions: &session::SessionManager,
    project_path: &str,
) -> bool {
    sessions
        .list()
        .await
        .map(|items| {
            items.into_iter().any(|item| {
                item.cwd.trim_end_matches('/') == project_path.trim_end_matches('/')
                    && item.status != session::SessionStatus::Ended
            })
        })
        .unwrap_or(true)
}

fn project_verification_is_running(project_key: &str) -> bool {
    crate::session::verify_changes::status(project_key)["status"].as_str() == Some("running")
}

fn project_operation_key(project_path: &str) -> String {
    format!("path:{}", project_path.trim_end_matches('/'))
}

/// Cloud 的 `session_created_ack` 只有 Desktop 重新同步路径会携带这些稳定的
/// RemoteAccessGuard 拒绝码。它们表示账号已经不能使用远程功能，重试不会成功；
/// Redis、网络或未知错误则一律保留远程状态，等待下次连接重试。
fn is_permanent_reconnect_ack_error(source: &str, error: &str) -> bool {
    if !source.eq_ignore_ascii_case("desktop") {
        return false;
    }

    let code = error.split_once(':').map_or(error, |(code, _)| code).trim();
    matches!(
        code,
        "membershipExpired" | "membershipInactive" | "membershipGracePeriod" | "userNotFound"
    )
}

async fn handle_incoming(
    msg: proto::AgentIncoming,
    state: Arc<state::StateMachine>,
    outgoing: Arc<tokio::sync::Mutex<Option<mpsc::UnboundedSender<String>>>>,
    sessions: Arc<session::SessionManager>,
    input_merger: Arc<session::InputMerger>,
    ack_registry: Arc<ack::AckRegistry>, // Phase 3 开始使用
    project_delivery_gate: Arc<kn_agent::project_delivery::ProjectOperationGate>,
    delivery_outbox: Arc<DeliveryOutbox>,
    project_session_revisions: Arc<
        tokio::sync::Mutex<kn_agent::project_session_index::ProjectRevisionClock>,
    >,
    project_session_activity: Arc<
        tokio::sync::Mutex<kn_agent::project_session_index::ProjectActivityTracker>,
    >,
    project_session_scan_gate: Arc<kn_agent::project_session_index::ProjectScanGate>,
) {
    match msg {
        proto::AgentIncoming::Pong { remote_access, .. } => {
            state.set_remote_access(remote_access).await;
        }
        proto::AgentIncoming::CliHeartbeatAck {
            remote_access,
            blocked_session_ids,
            ..
        } => {
            state.set_remote_access(remote_access.clone()).await;
            if let Some(status) = remote_access.filter(|s| !s.allowed) {
                tracing::warn!(
                    code = %status.code,
                    blocked = blocked_session_ids.len(),
                    "远程权限不可用，Cloud 已阻止远程会话通信"
                );
            }
        }
        proto::AgentIncoming::ProjectDeliveryAck { request_id } => {
            if delivery_outbox.acknowledge(&request_id).await {
                flush_delivery_outbox(&delivery_outbox, &outgoing).await;
            }
        }
        proto::AgentIncoming::TaskCompletedAck { event_id, status } => {
            kn_agent::task_events::acknowledge_task_complete_event(&event_id);
            if status == "ok" {
                tracing::debug!(event_id = %event_id, "本轮回复完成事件已入库或去重");
            } else {
                tracing::warn!(event_id = %event_id, status = %status, "本轮回复完成事件被 Cloud 拒收，本地队列已丢弃");
            }
        }
        proto::AgentIncoming::DeviceHealth {
            device_id,
            request_id,
        } => {
            let current_state = state.current().await;
            let environment = kn_agent::health::normalized_environment(
                std::env::var("KN_RUNTIME_ENV").ok().as_deref(),
            );
            let summary = kn_agent::health::cached_probe_snapshot(
                env!("CARGO_PKG_VERSION"),
                environment,
                current_state.name(),
            )
            .await;
            let response = proto::WsMessageBuilder::device_health_result(
                device_id,
                &request_id,
                serde_json::to_value(summary).unwrap_or_else(|_| {
                    serde_json::json!({
                        "schemaVersion": 1,
                        "tools": []
                    })
                }),
            );
            if let Some(tx) = outgoing.lock().await.as_ref() {
                let _ = tx.send(response);
            }
        }
        proto::AgentIncoming::Connected {
            ws_session_id,
            protocol_version,
            ..
        } => {
            tracing::info!(
                "云端已连接: session={}, protocol=v{}",
                ws_session_id,
                protocol_version.unwrap_or(1)
            );
            let outbox = delivery_outbox.clone();
            let reconnect_outgoing = outgoing.clone();
            tokio::spawn(async move {
                flush_delivery_outbox(&outbox, &reconnect_outgoing).await;
            });
            // 上报 profile 列表
            if let Ok(profiles) = kn_common::profile::list_profiles_cmd() {
                let info: Vec<proto::ProfileInfo> =
                    profiles.profiles.iter().map(|p| p.into()).collect();
                let msg = proto::WsMessageBuilder::profile_list(&info);
                if let Some(tx) = outgoing.lock().await.as_ref() {
                    let _ = tx.send(msg);
                }
            }

            // 上报 project 列表
            send_project_list(&outgoing).await;
            send_project_session_indexes(
                &outgoing,
                &project_session_revisions,
                &project_session_activity,
                &project_session_scan_gate,
            )
            .await;

            // WSS 重连后，重新同步所有开启远程的会话到云端。
            // agent 是会话状态的权威来源：即使云端因心跳超时把会话标为 ended，
            // agent 重发 session_created 应能复活会话。
            if let Ok(summaries) = sessions.list().await {
                for s in &summaries {
                    if s.status == session::SessionStatus::Ended {
                        continue;
                    }
                    if !s.remote_enabled {
                        continue;
                    }

                    let ack_nid = s.nid.clone();
                    let ack_tool = s.tool.clone();
                    let ack_cwd = s.cwd.clone();
                    let ack_profile = s.profile.clone();
                    let ack_cols = s.cols;
                    let ack_rows = s.rows;
                    let ack_source = s.source.clone();
                    let ack_outgoing = outgoing.clone();
                    let ack_registry = ack_registry.clone();
                    let ack_sessions = sessions.clone();

                    tokio::spawn(async move {
                        const MAX_RETRIES: u32 = 3;
                        let backoffs = [1u64, 2];

                        for attempt in 0..MAX_RETRIES {
                            let msg_id = format!("reconnect-{}", ack_nid);
                            let msg = proto::WsMessageBuilder::session_created_with_msg_id(
                                &ack_nid,
                                &ack_tool,
                                &ack_cwd,
                                ack_profile.as_deref(),
                                ack_cols,
                                ack_rows,
                                &ack_source,
                                Some(&msg_id),
                            );

                            // 必须先登记 ACK，再把消息交给 WSS writer。否则本机/低延迟
                            // Cloud 可在 register 前返回 ACK，导致 ACK 被丢失并错误关闭远程。
                            let Some(rx) = ack_registry.register_if_absent(&ack_nid).await else {
                                tracing::info!(nid = %ack_nid, "reconnect: 已有 session_created 确认进行中，跳过重复同步");
                                return;
                            };
                            let send_ok = {
                                let guard = ack_outgoing.lock().await;
                                match guard.as_ref() {
                                    Some(tx) => tx.send(msg).is_ok(),
                                    None => false,
                                }
                            };

                            if !send_ok {
                                let _ = ack_registry
                                    .resolve(
                                        &ack_nid,
                                        crate::ack::AckResult::Error("send failed".to_string()),
                                    )
                                    .await;
                                tracing::warn!(nid = %ack_nid, attempt = attempt, "reconnect: session_created 发送失败");
                            } else {
                                tracing::info!(nid = %ack_nid, attempt = attempt, "🔄 [RECONNECT] 重连后补发 session_created，等待 ACK");
                                match tokio::time::timeout(tokio::time::Duration::from_secs(10), rx)
                                    .await
                                {
                                    Ok(Ok(crate::ack::AckResult::Ok)) => {
                                        tracing::info!(nid = %ack_nid, "reconnect: ACK 成功，会话已恢复");
                                        return;
                                    }
                                    Ok(Ok(crate::ack::AckResult::Error(error))) => {
                                        ack_registry.cancel(&ack_nid).await;
                                        if is_permanent_reconnect_ack_error(&ack_source, &error) {
                                            match ack_sessions
                                                .set_remote_enabled(&ack_nid, false)
                                                .await
                                            {
                                                Ok(()) => tracing::warn!(
                                                    nid = %ack_nid,
                                                    error = %error,
                                                    "reconnect: Cloud 永久拒绝远程会话，已关闭本地远程状态"
                                                ),
                                                Err(disable_error) => tracing::error!(
                                                    nid = %ack_nid,
                                                    error = %disable_error,
                                                    original_error = %error,
                                                    "reconnect: Cloud 永久拒绝后关闭本地远程状态失败"
                                                ),
                                            }
                                            return;
                                        }

                                        // Redis、网络或未知错误均可能恢复；保留用户远程设置
                                        // 并重试，不能将其误判为用户主动关闭。
                                        tracing::warn!(nid = %ack_nid, attempt = attempt, error = %error, "reconnect: Cloud 暂时拒绝 session_created，将重试");
                                    }
                                    Ok(Err(_)) | Err(_) => {
                                        ack_registry.cancel(&ack_nid).await;
                                        tracing::warn!(nid = %ack_nid, attempt = attempt, "reconnect: session_created ACK 超时");
                                    }
                                }
                            }

                            if attempt + 1 < MAX_RETRIES {
                                tokio::time::sleep(tokio::time::Duration::from_secs(
                                    backoffs[attempt as usize],
                                ))
                                .await;
                            }
                        }

                        // WSS 断线、休眠唤醒或暂时拥塞均不代表用户关闭了远程。
                        // 保留 remote_enabled，下一次连接仍会重新同步该运行中的会话。
                        tracing::warn!(nid = %ack_nid, "reconnect: 未收到 ACK，保留远程状态等待后续重连");
                    });
                }
            }
        }
        proto::AgentIncoming::StartSession {
            profile,
            cwd,
            from_user_id,
            cols,
            rows,
            expected_cli,
            cli_args,
        } => {
            let resolved_tool = match session::env::resolve_tool_from_profile(&profile) {
                Ok(tool) => tool,
                Err(err) => {
                    tracing::warn!(
                        profile = %profile,
                        reason = err.reason(),
                        user = from_user_id,
                        "远程启动失败：profile 无法解析为可用 tool"
                    );
                    let msg = proto::WsMessageBuilder::session_start_failed(&profile, err.reason());
                    if let Some(tx) = outgoing.lock().await.as_ref() {
                        let _ = tx.send(msg);
                    }
                    return;
                }
            };
            if let Some(expected_cli) = expected_cli.as_deref() {
                if !session::env::history_resume_cli_matches_profile(expected_cli, &resolved_tool) {
                    tracing::warn!(
                        profile = %profile,
                        expected_cli = %expected_cli,
                        resolved_tool = %resolved_tool,
                        user = from_user_id,
                        "本地历史恢复失败：profile 与 CLI 不匹配"
                    );
                    let msg = proto::WsMessageBuilder::session_start_failed(
                        &profile,
                        "profile_cli_mismatch",
                    );
                    if let Some(tx) = outgoing.lock().await.as_ref() {
                        let _ = tx.send(msg);
                    }
                    return;
                }
            }
            let launch_kind = if expected_cli.is_some() {
                session::SessionLaunchKind::LocalHistoryResume
            } else {
                session::SessionLaunchKind::Standard
            };
            let cli_version = session::env::resolve_cli_version(&resolved_tool).await;
            // Agent 自行生成 sessionId，cloud 不再预分配
            let session_nid = format!("s_{}", nanoid::nanoid!(12));
            tracing::info!(
                nid = %session_nid,
                tool = %resolved_tool,
                profile = %profile,
                user = from_user_id,
                "收到远程启动会话请求"
            );

            let cwd_resolved = cwd.unwrap_or_else(|| ".".into());
            // Project Git operations and terminal creation must not cross: a
            // session either exists before the branch safety check, or starts
            // only after the checkout has completed.
            let _project_operation =
                if let Some(project_path) = registered_project_path(&cwd_resolved).await {
                    Some(
                        project_delivery_gate
                            .lock(&project_operation_key(&project_path))
                            .await,
                    )
                } else {
                    None
                };

            // 1. Create session record（create 内部持有 create_mutex，count+insert 原子性，会话数限制由 create 统一保证）
            match sessions
                .create(
                    session_nid.clone(),
                    "ios".to_string(),
                    resolved_tool.clone(),
                    Some(profile.clone()),
                    cwd_resolved.clone(),
                    crate::session::SessionKind::Native,
                )
                .await
            {
                Ok(session) => {
                    // iOS 远程会话：显式开启 remote_enabled（create 默认 false）
                    let _ = sessions.set_remote_enabled(&session_nid, true).await;
                    project_session_activity
                        .lock()
                        .await
                        .mark_active(&cwd_resolved, unix_millis());

                    // A newly created session may cause the native CLI to
                    // persist a history file shortly after launch. Refresh in
                    // the background; this never delays session creation.
                    let index_outgoing = outgoing.clone();
                    let index_revisions = project_session_revisions.clone();
                    let index_activity = project_session_activity.clone();
                    let index_scan_gate = project_session_scan_gate.clone();
                    tokio::spawn(async move {
                        tokio::time::sleep(Duration::from_secs(2)).await;
                        send_project_session_indexes(
                            &index_outgoing,
                            &index_revisions,
                            &index_activity,
                            &index_scan_gate,
                        )
                        .await;
                    });

                    // Spawn PTY + CLI process (before ACK — process needs to be running)
                    let (wss_tx, mut wss_rx) = mpsc::unbounded_channel::<String>();
                    let (ipc_tx, mut ipc_rx) = mpsc::unbounded_channel::<String>();

                    // 转发 task: OutputFanout 的输出 → 全局 WSS outgoing 通道
                    let out = outgoing.clone();
                    tokio::spawn(async move {
                        while let Some(msg) = wss_rx.recv().await {
                            let len = msg.len();
                            if let Some(tx) = out.lock().await.as_ref() {
                                match tx.send(msg) {
                                    Ok(_) => {
                                        tracing::debug!(len = len, "📤 [OUTPUT] 已转发到全局 WSS")
                                    }
                                    Err(e) => {
                                        tracing::error!(len = len, error = %e, "📤 [OUTPUT] 转发失败")
                                    }
                                }
                            } else {
                                tracing::warn!(len = len, "📤 [OUTPUT] outgoing 通道不可用");
                            }
                        }
                        tracing::warn!("📤 [OUTPUT] wss_rx 通道关闭, 转发 task 退出");
                    });

                    // IPC 输出消费 task（避免 channel full 阻塞 OutputFanout）
                    tokio::spawn(async move {
                        while let Some(msg) = ipc_rx.recv().await {
                            tracing::debug!(msg_len = msg.len(), "IPC output channel drained");
                        }
                    });
                    let s = sessions.clone();
                    let m = input_merger.clone();
                    let nid = session_nid.clone();
                    let t = resolved_tool.clone();
                    let p = profile.clone();
                    let c = cwd_resolved.clone();
                    let history_cli_args = cli_args.clone();
                    let remote_enabled = Some(session.remote_enabled.clone());
                    let out = outgoing.clone();

                    tokio::spawn(async move {
                        let s_cleanup = s.clone();
                        match s
                            .start_session_with_args(
                                &nid,
                                &t,
                                Some(p.as_str()),
                                &c,
                                cols,
                                rows,
                                &history_cli_args,
                                launch_kind,
                                wss_tx,
                                ipc_tx,
                                m,
                                remote_enabled,
                            )
                            .await
                        {
                            Ok(_fanout) => {
                                tracing::info!(nid = %nid, tool = %t, "WSS PTY session started");
                            }
                            Err(e) => {
                                tracing::error!(nid = %nid, error = %e, "WSS PTY session start failed — cleaning up orphaned session record");
                                // 清理残留的 session 记录，防止变成永久僵尸会话
                                let _ = s_cleanup.end(&nid).await;
                                let failed = proto::WsMessageBuilder::session_start_failed(
                                    &p,
                                    "spawn_failed",
                                );
                                if let Some(tx) = out.lock().await.as_ref() {
                                    let _ = tx.send(failed);
                                }
                                if let Ok(Some(msg)) =
                                    s_cleanup.report_session_ended(&nid, "start_failed").await
                                {
                                    if let Some(tx) = out.lock().await.as_ref() {
                                        let _ = tx.send(msg.to_json());
                                    }
                                }
                            }
                        }
                    });

                    // ACK retry task: send session_created to WSS, retry up to 3 times
                    // If all retries fail → kill the PTY process and clean up
                    let ack_sessions = sessions.clone();
                    let ack_outgoing = outgoing.clone();
                    let ack_registry = ack_registry.clone();
                    let ack_nid = session_nid.clone();
                    let ack_tool = resolved_tool.clone();
                    let ack_cwd = cwd_resolved.clone();
                    let ack_profile = profile.clone();
                    let ack_cols = cols;
                    let ack_rows = rows;
                    let ack_cli_version = cli_version.clone();

                    tokio::spawn(async move {
                        const MAX_RETRIES: u32 = 3;
                        let backoffs = [1u64, 2, 4];

                        for attempt in 0..MAX_RETRIES {
                            let msg_id = format!("ios-{}", ack_nid);
                            let msg =
                                proto::WsMessageBuilder::session_created_with_msg_id_and_version(
                                    &ack_nid,
                                    &ack_tool,
                                    &ack_cwd,
                                    Some(ack_profile.as_str()),
                                    ack_cols,
                                    ack_rows,
                                    "ios",
                                    Some(&msg_id),
                                    ack_cli_version.as_deref(),
                                );

                            // 先注册再发送，避免低延迟 ACK 在 receiver 建立前被丢弃。
                            let Some(rx) = ack_registry.register_if_absent(&ack_nid).await else {
                                // 重连同步正在确认同一会话；它成功即可，不能由这里的
                                // 重复任务覆盖 receiver 后误杀刚创建的 PTY。
                                tracing::info!(nid = %ack_nid, "session_created 确认已在进行，交由现有任务完成");
                                return;
                            };

                            let send_ok = {
                                let guard = ack_outgoing.lock().await;
                                match guard.as_ref() {
                                    Some(tx) => tx.send(msg).is_ok(),
                                    None => false,
                                }
                            };

                            if !send_ok {
                                let _ = ack_registry
                                    .resolve(
                                        &ack_nid,
                                        crate::ack::AckResult::Error("send failed".to_string()),
                                    )
                                    .await;
                                tracing::warn!(nid = %ack_nid, attempt = attempt, "WSS channel 不可用");
                                if attempt + 1 < MAX_RETRIES {
                                    tokio::time::sleep(tokio::time::Duration::from_secs(
                                        backoffs[attempt as usize],
                                    ))
                                    .await;
                                    continue;
                                }
                                break;
                            }

                            tracing::info!(nid = %ack_nid, attempt = attempt, "session_created 已发送，等待 ACK");
                            match tokio::time::timeout(tokio::time::Duration::from_secs(10), rx)
                                .await
                            {
                                Ok(Ok(crate::ack::AckResult::Ok)) => {
                                    tracing::info!(nid = %ack_nid, attempt = attempt, "session_created ACK 成功");
                                    return;
                                }
                                Ok(Ok(crate::ack::AckResult::Error(e))) => {
                                    // Cloud 明确拒绝，不重试
                                    tracing::error!(nid = %ack_nid, error = %e, "session_created ACK 被云端拒绝");
                                    break;
                                }
                                Ok(Err(_)) | Err(_) => {
                                    // Timeout or oneshot sender dropped
                                    ack_registry.cancel(&ack_nid).await;
                                    tracing::warn!(nid = %ack_nid, attempt = attempt, "session_created ACK 超时");
                                    if attempt + 1 < MAX_RETRIES {
                                        tokio::time::sleep(tokio::time::Duration::from_secs(
                                            backoffs[attempt as usize],
                                        ))
                                        .await;
                                        continue;
                                    }
                                }
                            }
                        }

                        // All retries exhausted or cloud rejected → kill PTY + clean up
                        tracing::error!(nid = %ack_nid, "所有 session_created ACK 重试失败，终止会话");
                        let _ = ack_sessions.kill_session(&ack_nid).await;
                        let _ = ack_sessions
                            .report_session_ended(&ack_nid, "wss_ack_failed")
                            .await;
                    });

                    // Transition to Running state
                    let _ = state.transition(state::StateEvent::SessionStarted).await;
                }
                Err(e) => {
                    tracing::error!(error = %e, "创建会话失败");
                    // SessionLimit 错误需通知云端，让 iOS 显示友好提示
                    if let AgentError::SessionLimit { current, max } = &e {
                        let err = proto::WsMessageBuilder::error_notify(
                            "SESSION_LIMIT",
                            &format!("Agent 会话数已满 ({}/{}), 请关闭之前的会话", current, max),
                        );
                        if let Some(tx) = outgoing.lock().await.as_ref() {
                            let _ = tx.send(err);
                        }
                    }
                }
            }
        }
        proto::AgentIncoming::Input {
            session_nid,
            seq,
            content,
            ..
        } => {
            tracing::info!(
                nid = %session_nid,
                seq = seq,
                content = %content,
                "📱 [INPUT] 收到远程输入"
            );

            // Intercept /exit command: force-kill PTY process and report session_ended,
            // instead of passing it through as raw stdin text (which CLI may not handle
            // if stuck in a subprocess like vim).
            let content_trimmed = content.trim();
            if content_trimmed == "/exit" || content_trimmed.starts_with("/exit ") {
                tracing::info!(nid = %session_nid, "🛑 [EXIT] 收到 /exit 命令，强制终止会话");

                match sessions.get(&session_nid).await {
                    Ok(Some(session_summary)) => {
                        if !session_summary
                            .remote_enabled
                            .load(std::sync::atomic::Ordering::Relaxed)
                        {
                            tracing::warn!(nid = %session_nid, "忽略未开启远程的 /exit 输入");
                            return;
                        }
                        let nid = session_summary.nid;
                        let is_remote = session_summary
                            .remote_enabled
                            .load(std::sync::atomic::Ordering::Relaxed);

                        // 1. Report session_ended FIRST (atomic swap ensures this reason wins
                        //    over any concurrent "process_exit" from PTY EOF handler)
                        // 只对开启了远程的会话同步 session_ended 到云端
                        match sessions.report_session_ended(&nid, "user_exit").await {
                            Ok(Some(msg)) => {
                                if is_remote {
                                    if let Some(tx) = outgoing.lock().await.as_ref() {
                                        let _ = tx.send(msg.to_json());
                                        tracing::info!(session_nid = %session_nid, nid = %nid, "session_ended (user_exit) 已发送到 Cloud");
                                    }
                                }
                            }
                            Ok(None) => {
                                tracing::warn!(session_nid = %session_nid, nid = %nid, "session_ended 已上报过，跳过");
                            }
                            Err(e) => {
                                tracing::error!(session_nid = %session_nid, nid = %nid, error = %e, "session_ended 发送失败");
                            }
                        }

                        // 2. Kill the PTY process (SIGKILL + cleanup)
                        if let Err(e) = sessions.kill_session(&nid).await {
                            tracing::error!(session_nid = %session_nid, nid = %nid, error = %e, "kill_session 失败");
                        }
                    }
                    Ok(None) => {
                        tracing::warn!(nid = %session_nid, "/exit 目标会话不存在");
                    }
                    Err(e) => {
                        tracing::error!(nid = %session_nid, error = %e, "/exit 查询会话失败");
                    }
                }
            } else {
                // Normal input: route to PTY stdin
                match sessions.get(&session_nid).await {
                    Ok(Some(session_summary)) => {
                        if !session_summary
                            .remote_enabled
                            .load(std::sync::atomic::Ordering::Relaxed)
                        {
                            tracing::warn!(nid = %session_nid, "忽略未开启远程的 input");
                            return;
                        }
                        let nid = session_summary.nid.clone();
                        let text = content.clone();
                        if session_summary.kind == session::SessionKind::Relay {
                            if let Err(e) = sessions.queue_relay_input(&nid, text).await {
                                tracing::error!(nid = %nid, error = %e, "Relay input 入队失败");
                            } else {
                                tracing::info!(nid = %nid, "📱 [INPUT] 已推入 Relay 队列");
                            }
                        } else {
                            input_merger
                                .push(session::InputMessage {
                                    session_id: nid.clone(),
                                    text,
                                    source: "ios".into(),
                                })
                                .await;
                            tracing::info!(nid = %nid, "📱 [INPUT] 已推入 InputMerger 队列");
                        }
                    }
                    Ok(None) => {
                        tracing::warn!(nid = %session_nid, "Input 目标会话不存在");
                    }
                    Err(e) => {
                        tracing::error!(nid = %session_nid, error = %e, "Input 查询会话失败");
                    }
                }
            }
        }
        proto::AgentIncoming::Ctrl {
            session_nid,
            signal,
        } => {
            tracing::debug!(
                nid = %session_nid,
                signal = ?signal,
                "收到远程控制信号"
            );

            // Extract signal name from ctrl message data (Java forwards signal as raw JSON)
            let signal_name = signal.get("signal").and_then(|v| v.as_str()).unwrap_or("");

            let byte = match signal_name {
                "ctrl_c" => vec![0x03u8],
                "ctrl_d" => vec![0x04u8],
                "ctrl_z" => vec![0x1au8],
                other => {
                    tracing::warn!(signal = other, "未知控制信号");
                    return;
                }
            };

            match sessions.get(&session_nid).await {
                Ok(Some(session_summary)) => {
                    if !session_summary
                        .remote_enabled
                        .load(std::sync::atomic::Ordering::Relaxed)
                    {
                        tracing::warn!(nid = %session_nid, "忽略未开启远程的 ctrl");
                        return;
                    }
                    let nid = session_summary.nid.clone();
                    let text = String::from_utf8_lossy(&byte).to_string();
                    if session_summary.kind == session::SessionKind::Relay {
                        if let Err(e) = sessions.queue_relay_input(&nid, text).await {
                            tracing::error!(nid = %nid, error = %e, "Relay ctrl 入队失败");
                        }
                    } else {
                        input_merger
                            .push(session::InputMessage {
                                session_id: nid,
                                text,
                                source: "ios".into(),
                            })
                            .await;
                    }
                }
                Ok(None) => {
                    tracing::warn!(nid = %session_nid, "Ctrl 目标会话不存在");
                }
                Err(e) => {
                    tracing::error!(nid = %session_nid, error = %e, "Ctrl 查询会话失败");
                }
            }
        }
        proto::AgentIncoming::Resize {
            session_nid,
            cols,
            rows,
        } => {
            tracing::debug!(
                nid = %session_nid,
                cols = cols,
                rows = rows,
                "收到远程 resize"
            );

            match sessions.get(&session_nid).await {
                Ok(Some(session_summary)) => {
                    if !session_summary
                        .remote_enabled
                        .load(std::sync::atomic::Ordering::Relaxed)
                    {
                        tracing::warn!(nid = %session_nid, "忽略未开启远程的 resize");
                        return;
                    }
                    if let Err(e) = sessions
                        .resize_from_source(
                            &session_summary.nid,
                            cols,
                            rows,
                            session::ViewportOwner::Ios,
                        )
                        .await
                    {
                        tracing::error!(nid = %session_nid, error = %e, "Resize 会话失败");
                    }
                }
                Ok(None) => {
                    tracing::warn!(nid = %session_nid, "Resize 目标会话不存在");
                }
                Err(e) => {
                    tracing::error!(nid = %session_nid, error = %e, "Resize 查询会话失败");
                }
            }
        }
        proto::AgentIncoming::ErrorNotify { code, message } => {
            tracing::error!(
                code = %code,
                message = %message,
                "云端错误通知"
            );
        }
        proto::AgentIncoming::ProfileListAck => {
            tracing::debug!("Profile 列表已确认");
        }
        proto::AgentIncoming::ReplayOutput { session_nid } => {
            tracing::info!(
                nid = %session_nid,
                "收到 replay_output 请求，读取本地输出日志"
            );

            let replay = session::OutputFanout::replay_log_result(&session_nid);
            match replay.status {
                "ok" => {
                    // 环形日志存储的是原始字节（包含 ANSI escape），直接转为 String
                    tracing::info!(
                        nid = %session_nid,
                        bytes = replay.bytes,
                        "回放输出日志"
                    );

                    // 分块发送：每块最多 32KB，避免单条 WSS 消息过大
                    const CHUNK_SIZE: usize = 32 * 1024;
                    let ansi_text = String::from_utf8_lossy(&replay.data).into_owned();
                    let mut offset = 0;
                    let mut chunks = 0usize;
                    while offset < ansi_text.len() {
                        let end = std::cmp::min(offset + CHUNK_SIZE, ansi_text.len());
                        // 在 UTF-8 字符边界切割，避免截断多字节字符
                        let mut chunk_end = end;
                        while chunk_end > offset && !ansi_text.is_char_boundary(chunk_end) {
                            chunk_end -= 1;
                        }
                        let chunk = &ansi_text[offset..chunk_end];
                        let msg = proto::WsMessageBuilder::output(&session_nid, chunk);
                        if let Some(tx) = outgoing.lock().await.as_ref() {
                            let _ = tx.send(msg);
                        }
                        chunks += 1;
                        offset = chunk_end;
                    }
                    let done = proto::WsMessageBuilder::replay_output_done(
                        &session_nid,
                        "ok",
                        replay.bytes,
                        chunks,
                        None,
                    );
                    if let Some(tx) = outgoing.lock().await.as_ref() {
                        let _ = tx.send(done);
                    }
                }
                "empty" => {
                    tracing::warn!(
                        nid = %session_nid,
                        "replay_output: 未找到输出日志或日志为空"
                    );
                    let done = proto::WsMessageBuilder::replay_output_done(
                        &session_nid,
                        "empty",
                        0,
                        0,
                        None,
                    );
                    if let Some(tx) = outgoing.lock().await.as_ref() {
                        let _ = tx.send(done);
                    }
                }
                _ => {
                    let message = replay.message.as_deref().unwrap_or("读取输出日志失败");
                    tracing::warn!(
                        nid = %session_nid,
                        message = %message,
                        "replay_output: 读取输出日志失败"
                    );
                    let done = proto::WsMessageBuilder::replay_output_done(
                        &session_nid,
                        "error",
                        0,
                        0,
                        Some(message),
                    );
                    if let Some(tx) = outgoing.lock().await.as_ref() {
                        let _ = tx.send(done);
                    }
                }
            }
        }
        proto::AgentIncoming::ProjectChangeSummary {
            project_key: _project_key,
            device_id,
            project_path,
            request_id,
        } => {
            let project_key = canonical_project_key(device_id, &project_path);
            tracing::info!(project_key = %project_key, "收到 project_change_summary 请求");
            let data = if is_registered_project_path(&project_path).await {
                crate::session::git_preview::summary(&project_key, &project_path).await
            } else {
                serde_json::json!({"projectKey": &project_key, "status": "pathDenied", "files": [], "message": "项目未登记"})
            };
            let msg = proto::WsMessageBuilder::project_delivery_result(
                "project_change_summary_result",
                &project_key,
                device_id,
                &project_path,
                request_id.as_deref(),
                data,
            );
            if let Some(tx) = outgoing.lock().await.as_ref() {
                let _ = tx.send(msg);
            }
        }
        proto::AgentIncoming::ProjectChangeFileDiff {
            project_key: _project_key,
            device_id,
            project_path,
            path,
        } => {
            let project_key = canonical_project_key(device_id, &project_path);
            tracing::info!(project_key = %project_key, path = %path, "收到 project_change_file_diff 请求");
            let data = if is_registered_project_path(&project_path).await {
                crate::session::git_preview::file_diff(&project_key, &project_path, &path).await
            } else {
                serde_json::json!({"projectKey": &project_key, "path": path, "status": "pathDenied", "diffText": "", "message": "项目未登记"})
            };
            let msg = proto::WsMessageBuilder::project_result(
                "project_change_file_diff_result",
                &project_key,
                device_id,
                &project_path,
                data,
            );
            if let Some(tx) = outgoing.lock().await.as_ref() {
                let _ = tx.send(msg);
            }
        }
        proto::AgentIncoming::ProjectGitStatus {
            project_key: _project_key,
            device_id,
            project_path,
            request_id,
            offset,
            limit,
            snapshot_id,
        } => {
            let project_key = canonical_project_key(device_id, &project_path);
            let data = if let Some(registered_path) = registered_project_path(&project_path).await {
                let operation_project_key = project_operation_key(&registered_path);
                let _operation = project_delivery_gate.lock(&operation_project_key).await;
                crate::session::git_delivery::status_page(
                    &project_key,
                    &registered_path,
                    offset,
                    limit,
                    snapshot_id.as_deref(),
                )
                .await
            } else {
                serde_json::json!({
                    "projectKey": &project_key,
                    "status": "pathDenied",
                    "files": [],
                    "totalFiles": 0,
                    "offset": 0,
                    "nextOffset": 0,
                    "hasMore": false,
                    "truncated": false,
                    "snapshotId": null,
                    "message": "项目未登记"
                })
            };
            let msg = proto::WsMessageBuilder::project_delivery_result(
                "project_git_status_result",
                &project_key,
                device_id,
                &project_path,
                request_id.as_deref(),
                data,
            );
            // 只读状态可以由 iOS 安全重试；不要占用写操作的可靠结果 outbox。
            let _ =
                send_project_delivery_message(&outgoing, msg, "project_git_status_result").await;
        }
        proto::AgentIncoming::ProjectListStatus {
            project_key: _project_key,
            device_id,
            project_path,
            request_id,
        } => {
            let project_key = canonical_project_key(device_id, &project_path);
            let data = if let Some(registered_path) = registered_project_path(&project_path).await {
                let last_verification =
                    crate::session::verify_changes::last_verification(&project_key);
                // 与提交、推送共用项目级 gate，避免读取 index/refs 的中间态或撞上 index.lock。
                let operation_project_key = project_operation_key(&registered_path);
                let _operation = project_delivery_gate.lock(&operation_project_key).await;
                crate::session::project_list_status::read(
                    &project_key,
                    &registered_path,
                    last_verification,
                )
                .await
            } else {
                serde_json::json!({
                    "projectKey": &project_key,
                    "status": "pathDenied",
                    "git": {"state": "unavailable", "branch": null, "hasUpstream": false, "ahead": 0, "behind": 0},
                    "lastVerification": null,
                    "message": "项目未登记"
                })
            };
            let msg = proto::WsMessageBuilder::project_delivery_result(
                "project_list_status_result",
                &project_key,
                device_id,
                &project_path,
                request_id.as_deref(),
                data,
            );
            let _ =
                send_project_delivery_message(&outgoing, msg, "project_list_status_result").await;
        }
        proto::AgentIncoming::ProjectGitCommit {
            project_key: _project_key,
            device_id,
            project_path,
            message,
            paths,
            scope,
            snapshot_id,
            request_id,
        } => {
            let project_key = canonical_project_key(device_id, &project_path);
            let data = if let Some(registered_path) = registered_project_path(&project_path).await {
                let operation_project_key = project_operation_key(&registered_path);
                let _operation = project_delivery_gate.lock(&operation_project_key).await;
                match scope.as_str() {
                    "selected" => {
                        crate::session::git_delivery::commit(
                            &project_key,
                            &registered_path,
                            &message,
                            &paths,
                        )
                        .await
                    }
                    "selectedAndStaged" => {
                        crate::session::git_delivery::commit_selected_and_staged(
                            &project_key,
                            &registered_path,
                            &message,
                            &paths,
                            snapshot_id.as_deref(),
                        )
                        .await
                    }
                    "allWorkingTree" => {
                        crate::session::git_delivery::commit_all_working_tree(
                            &project_key,
                            &registered_path,
                            &message,
                        )
                        .await
                    }
                    _ => serde_json::json!({"projectKey": &project_key, "status": "invalidScope"}),
                }
            } else {
                serde_json::json!({"status": "pathDenied", "message": "项目未登记"})
            };
            let msg = proto::WsMessageBuilder::project_delivery_result(
                "project_git_commit_result",
                &project_key,
                device_id,
                &project_path,
                request_id.as_deref(),
                data,
            );
            send_project_delivery_result(
                &outgoing,
                &delivery_outbox,
                msg,
                "project_git_commit_result",
            )
            .await;
        }
        proto::AgentIncoming::ProjectGitPush {
            project_key: _project_key,
            device_id,
            project_path,
            request_id,
        } => {
            let project_key = canonical_project_key(device_id, &project_path);
            let data = if let Some(registered_path) = registered_project_path(&project_path).await {
                let operation_project_key = project_operation_key(&registered_path);
                let _operation = project_delivery_gate.lock(&operation_project_key).await;
                let progress_outgoing = outgoing.clone();
                let progress_project_key = project_key.clone();
                let progress_project_path = project_path.clone();
                let progress_request_id = request_id.clone();
                crate::session::git_delivery::push(&project_key, &registered_path, move |message| {
                    let progress = proto::WsMessageBuilder::project_delivery_result(
                        "project_git_push_progress",
                        &progress_project_key,
                        device_id,
                        &progress_project_path,
                        progress_request_id.as_deref(),
                        serde_json::json!({"status": "running", "stage": "pushing", "message": message}),
                    );
                    let outgoing = progress_outgoing.clone();
                    tokio::spawn(async move {
                        let _ = send_project_delivery_message(
                            &outgoing,
                            progress,
                            "project_git_push_progress",
                        )
                        .await;
                    });
                })
                .await
            } else {
                serde_json::json!({"status": "pathDenied", "message": "项目未登记"})
            };
            let msg = proto::WsMessageBuilder::project_delivery_result(
                "project_git_push_result",
                &project_key,
                device_id,
                &project_path,
                request_id.as_deref(),
                data,
            );
            send_project_delivery_result(
                &outgoing,
                &delivery_outbox,
                msg,
                "project_git_push_result",
            )
            .await;
        }
        proto::AgentIncoming::ProjectGitBranches {
            project_key: _,
            device_id,
            project_path,
            request_id,
        } => {
            let project_key = canonical_project_key(device_id, &project_path);
            let started_at = Instant::now();
            tracing::info!(project_key = %project_key, request_id = ?request_id, "收到 project_git_branches 请求");
            let data = if let Some(registered_path) = registered_project_path(&project_path).await {
                let _operation = project_delivery_gate
                    .lock(&project_operation_key(&registered_path))
                    .await;
                crate::session::git_delivery::branches(&project_key, &registered_path).await
            } else {
                serde_json::json!({"status":"pathDenied", "message":"项目未登记"})
            };
            tracing::info!(
                project_key = %project_key,
                request_id = ?request_id,
                status = ?data["status"].as_str(),
                elapsed_ms = started_at.elapsed().as_millis(),
                "完成 project_git_branches 请求"
            );
            let msg = proto::WsMessageBuilder::project_delivery_result(
                "project_git_branches_result",
                &project_key,
                device_id,
                &project_path,
                request_id.as_deref(),
                data,
            );
            let _ =
                send_project_delivery_message(&outgoing, msg, "project_git_branches_result").await;
        }
        proto::AgentIncoming::ProjectGitCheckout {
            project_key: _,
            device_id,
            project_path,
            branch,
            request_id,
        } => {
            let project_key = canonical_project_key(device_id, &project_path);
            let data = if let Some(registered_path) = registered_project_path(&project_path).await {
                let _operation = project_delivery_gate
                    .lock(&project_operation_key(&registered_path))
                    .await;
                if project_has_active_terminal(&sessions, &registered_path).await {
                    serde_json::json!({"status":"activeTerminal", "message":"请先结束该项目的终端会话"})
                } else if project_verification_is_running(&project_key) {
                    serde_json::json!({"status":"verificationRunning", "message":"请先等待构建或测试结束"})
                } else {
                    crate::session::git_delivery::checkout_branch(
                        &project_key,
                        &registered_path,
                        &branch,
                    )
                    .await
                }
            } else {
                serde_json::json!({"status":"pathDenied", "message":"项目未登记"})
            };
            let msg = proto::WsMessageBuilder::project_delivery_result(
                "project_git_checkout_result",
                &project_key,
                device_id,
                &project_path,
                request_id.as_deref(),
                data,
            );
            send_project_delivery_result(
                &outgoing,
                &delivery_outbox,
                msg,
                "project_git_checkout_result",
            )
            .await;
        }
        proto::AgentIncoming::ProjectGitCreateBranch {
            project_key: _,
            device_id,
            project_path,
            branch,
            base,
            request_id,
        } => {
            let project_key = canonical_project_key(device_id, &project_path);
            let data = if let Some(registered_path) = registered_project_path(&project_path).await {
                let _operation = project_delivery_gate
                    .lock(&project_operation_key(&registered_path))
                    .await;
                if project_has_active_terminal(&sessions, &registered_path).await {
                    serde_json::json!({"status":"activeTerminal", "message":"请先结束该项目的终端会话"})
                } else if project_verification_is_running(&project_key) {
                    serde_json::json!({"status":"verificationRunning", "message":"请先等待构建或测试结束"})
                } else {
                    crate::session::git_delivery::create_and_checkout_branch(
                        &project_key,
                        &registered_path,
                        &branch,
                        &base,
                    )
                    .await
                }
            } else {
                serde_json::json!({"status":"pathDenied", "message":"项目未登记"})
            };
            let msg = proto::WsMessageBuilder::project_delivery_result(
                "project_git_create_branch_result",
                &project_key,
                device_id,
                &project_path,
                request_id.as_deref(),
                data,
            );
            send_project_delivery_result(
                &outgoing,
                &delivery_outbox,
                msg,
                "project_git_create_branch_result",
            )
            .await;
        }
        proto::AgentIncoming::ProjectPrStatus {
            project_key: _project_key,
            device_id,
            project_path,
            request_id,
        } => {
            let project_key = canonical_project_key(device_id, &project_path);
            let data = if let Some(registered_path) = registered_project_path(&project_path).await {
                let operation_project_key = project_operation_key(&registered_path);
                let _operation = project_delivery_gate.lock(&operation_project_key).await;
                crate::session::pr_delivery::status(&project_key, &registered_path).await
            } else {
                serde_json::json!({"status": "pathDenied", "message": "项目未登记"})
            };
            let msg = proto::WsMessageBuilder::project_delivery_result(
                "project_pr_status_result",
                &project_key,
                device_id,
                &project_path,
                request_id.as_deref(),
                data,
            );
            // 只读状态可以由 iOS 安全重试；不要占用写操作的可靠结果 outbox。
            let _ = send_project_delivery_message(&outgoing, msg, "project_pr_status_result").await;
        }
        proto::AgentIncoming::ProjectPrDetails {
            project_key: _project_key,
            device_id,
            project_path,
            request_id,
        } => {
            let project_key = canonical_project_key(device_id, &project_path);
            let data = if let Some(registered_path) = registered_project_path(&project_path).await {
                let operation_project_key = project_operation_key(&registered_path);
                let _operation = project_delivery_gate.lock(&operation_project_key).await;
                crate::session::pr_delivery::details(&project_key, &registered_path).await
            } else {
                serde_json::json!({"status": "pathDenied", "message": "项目未登记"})
            };
            let msg = proto::WsMessageBuilder::project_delivery_result(
                "project_pr_details_result",
                &project_key,
                device_id,
                &project_path,
                request_id.as_deref(),
                data,
            );
            // 只读状态可以由 iOS 安全重试；不要占用写操作的可靠结果 outbox。
            let _ =
                send_project_delivery_message(&outgoing, msg, "project_pr_details_result").await;
        }
        proto::AgentIncoming::ProjectPrCreate {
            project_key: _project_key,
            device_id,
            project_path,
            base,
            title,
            body,
            request_id,
        } => {
            let project_key = canonical_project_key(device_id, &project_path);
            let data = if let Some(registered_path) = registered_project_path(&project_path).await {
                let operation_project_key = project_operation_key(&registered_path);
                let _operation = project_delivery_gate.lock(&operation_project_key).await;
                crate::session::pr_delivery::create(
                    &project_key,
                    &registered_path,
                    &base,
                    &title,
                    &body,
                )
                .await
            } else {
                serde_json::json!({"status": "pathDenied", "message": "项目未登记"})
            };
            let msg = proto::WsMessageBuilder::project_delivery_result(
                "project_pr_create_result",
                &project_key,
                device_id,
                &project_path,
                request_id.as_deref(),
                data,
            );
            send_project_delivery_result(
                &outgoing,
                &delivery_outbox,
                msg,
                "project_pr_create_result",
            )
            .await;
        }
        proto::AgentIncoming::ProjectVerifyPlan {
            project_key: _project_key,
            device_id,
            project_path,
            environment,
            request_id,
        } => {
            let project_key = canonical_project_key(device_id, &project_path);
            tracing::info!(project_key = %project_key, environment = %environment, "收到 project_verify_plan 请求");
            let data = if is_registered_project_path(&project_path).await {
                crate::session::verify_changes::preview(&project_key, &project_path, &environment)
                    .await
            } else {
                serde_json::json!({
                    "projectKey": &project_key,
                    "status": "pathDenied",
                    "environment": environment,
                    "commandSource": "auto",
                    "availableEnvironments": [],
                    "detectedLanguages": [],
                    "message": "项目未登记"
                })
            };
            let msg = proto::WsMessageBuilder::project_delivery_result(
                "project_verify_plan_result",
                &project_key,
                device_id,
                &project_path,
                request_id.as_deref(),
                data,
            );
            if let Some(tx) = outgoing.lock().await.as_ref() {
                let _ = tx.send(msg);
            }
        }
        proto::AgentIncoming::ProjectVerifyChanges {
            project_key: _project_key,
            device_id,
            project_path,
            environment,
            target,
            request_id,
        } => {
            let project_key = canonical_project_key(device_id, &project_path);
            tracing::info!(project_key = %project_key, environment = %environment, target = %target, "收到 project_verify_changes 请求");
            let Some(target) = crate::session::verify_changes::VerifyTarget::parse(&target) else {
                let data = crate::session::verify_changes::invalid_target_result(
                    &project_key,
                    &environment,
                );
                let msg = proto::WsMessageBuilder::project_delivery_result(
                    "project_verify_changes_result",
                    &project_key,
                    device_id,
                    &project_path,
                    request_id.as_deref(),
                    data,
                );
                if let Some(tx) = outgoing.lock().await.as_ref() {
                    let _ = tx.send(msg);
                }
                return;
            };
            if !is_registered_project_path(&project_path).await {
                let data = serde_json::json!({
                    "projectKey": &project_key,
                    "runId": "",
                    "status": "pathDenied",
                    "environment": environment,
                    "target": target.as_str(),
                    "commandSource": "auto",
                    "durationMs": 0,
                    "stages": [],
                    "message": "项目未登记"
                });
                let msg = proto::WsMessageBuilder::project_delivery_result(
                    "project_verify_changes_result",
                    &project_key,
                    device_id,
                    &project_path,
                    request_id.as_deref(),
                    data,
                );
                if let Some(tx) = outgoing.lock().await.as_ref() {
                    let _ = tx.send(msg);
                }
                return;
            }
            let out = outgoing.clone();
            tokio::spawn(async move {
                let tx = out.lock().await.as_ref().cloned();
                let data = crate::session::verify_changes::verify(
                    &project_key,
                    &project_path,
                    &environment,
                    target,
                    tx.clone(),
                    (device_id, project_path.clone()),
                    request_id.as_deref(),
                )
                .await;
                let msg = proto::WsMessageBuilder::project_delivery_result(
                    "project_verify_changes_result",
                    &project_key,
                    device_id,
                    &project_path,
                    request_id.as_deref(),
                    data,
                );
                if let Some(tx) = tx {
                    let _ = tx.send(msg);
                }
            });
        }
        proto::AgentIncoming::ProjectCancelVerify {
            project_key: _project_key,
            device_id,
            project_path,
            run_id,
        } => {
            let project_key = canonical_project_key(device_id, &project_path);
            tracing::info!(project_key = %project_key, run_id = %run_id, "收到 project_cancel_verify 请求");
            if let Some((environment, target, command_source, started, request_id)) =
                crate::session::verify_changes::cancel(&project_key, &run_id)
            {
                if let Some(tx) = outgoing.lock().await.as_ref().cloned() {
                    let reporter =
                        crate::session::verify_changes::ProgressReporter::new_project_with_started(
                            &project_key,
                            device_id,
                            &project_path,
                            &run_id,
                            &environment,
                            target,
                            &command_source,
                            Some(tx),
                            started,
                            request_id.as_deref(),
                        );
                    reporter.send_cancelling();
                }
            }
        }
        proto::AgentIncoming::ProjectVerifyStatus {
            project_key: _project_key,
            device_id,
            project_path,
            request_id,
        } => {
            let project_key = canonical_project_key(device_id, &project_path);
            let data = crate::session::verify_changes::status(&project_key);
            let msg = proto::WsMessageBuilder::project_delivery_result(
                "project_verify_status_result",
                &project_key,
                device_id,
                &project_path,
                request_id.as_deref(),
                data,
            );
            if let Some(tx) = outgoing.lock().await.as_ref() {
                let _ = tx.send(msg);
            }
        }
        proto::AgentIncoming::ProjectVerifyLogWindow {
            project_key: _project_key,
            device_id,
            project_path,
            request_id,
            run_id,
            stage,
            center_line,
            before,
            after,
        } => {
            let project_key = canonical_project_key(device_id, &project_path);
            let stage_name = crate::session::verify_changes::parse_stage_name(&stage);
            let data = if let Some(stage_name) = stage_name {
                crate::session::verify_changes::log_window(
                    &project_key,
                    &run_id,
                    stage_name,
                    center_line,
                    before,
                    after,
                )
            } else {
                serde_json::json!({
                    "projectKey": &project_key,
                    "runId": run_id,
                    "stage": stage,
                    "status": "stageNotFound",
                    "startLine": 0,
                    "endLine": 0,
                    "centerLine": center_line,
                    "lines": [],
                    "hasEarlier": false,
                    "hasLater": false,
                    "message": "阶段日志不存在"
                })
            };
            let msg = proto::WsMessageBuilder::project_delivery_result(
                "project_verify_log_window_result",
                &project_key,
                device_id,
                &project_path,
                request_id.as_deref(),
                data,
            );
            if let Some(tx) = outgoing.lock().await.as_ref() {
                let _ = tx.send(msg);
            }
        }
        proto::AgentIncoming::ProjectVerifyLogIssues {
            project_key: _project_key,
            device_id,
            project_path,
            request_id,
            run_id,
            stages,
            rules_version,
            matchers,
            limit,
        } => {
            let project_key = canonical_project_key(device_id, &project_path);
            let stage_names = stages
                .iter()
                .filter_map(|stage| crate::session::verify_changes::parse_stage_name(stage))
                .collect::<Vec<_>>();
            let data = crate::session::verify_changes::log_issues(
                &project_key,
                &run_id,
                if stage_names.is_empty() {
                    vec![
                        crate::session::verify_changes::StageName::Build,
                        crate::session::verify_changes::StageName::Test,
                    ]
                } else {
                    stage_names
                },
                &rules_version,
                &matchers,
                limit,
            );
            let msg = proto::WsMessageBuilder::project_delivery_result(
                "project_verify_log_issues_result",
                &project_key,
                device_id,
                &project_path,
                request_id.as_deref(),
                data,
            );
            if let Some(tx) = outgoing.lock().await.as_ref() {
                let _ = tx.send(msg);
            }
        }
        proto::AgentIncoming::Unknown { msg_type, .. } => {
            tracing::debug!("未知消息类型: {}", msg_type);
        }
        proto::AgentIncoming::SessionCreatedAck {
            session_nid,
            status,
            error,
        } => {
            let result = if status == "ok" {
                crate::ack::AckResult::Ok
            } else {
                crate::ack::AckResult::Error(error.unwrap_or(status.clone()))
            };
            let resolved = ack_registry.resolve(&session_nid, result).await;
            tracing::info!(nid = %session_nid, status = %status, resolved = resolved, "收到 session_created_ack");
        }
        proto::AgentIncoming::ResumeSession { session_nid } => {
            tracing::info!(nid = %session_nid, "收到 resume_session");
            match sessions.get(&session_nid).await {
                Ok(Some(summary))
                    if summary.status != crate::session::SessionStatus::Ended
                        && summary
                            .remote_enabled
                            .load(std::sync::atomic::Ordering::Relaxed) =>
                {
                    // 会话存在且未结束 → 重发 session_created 恢复云端状态
                    let msg = proto::WsMessageBuilder::session_created(
                        &summary.nid,
                        &summary.tool,
                        &summary.cwd,
                        summary.profile.as_deref(),
                        summary.cols,
                        summary.rows,
                        &summary.source,
                    );
                    if let Some(tx) = outgoing.lock().await.as_ref() {
                        let _ = tx.send(msg);
                        tracing::info!(nid = %session_nid, "resume: session_created 已重发");
                    }
                }
                _ => {
                    // 会话不存在或已结束 → 清理云端残留状态
                    tracing::warn!(nid = %session_nid, "resume: 会话不存在或已结束，清理云端状态");
                    let msg = proto::WsMessageBuilder::session_ended(&session_nid, "stale_redis");
                    if let Some(tx) = outgoing.lock().await.as_ref() {
                        let _ = tx.send(msg);
                    }
                }
            }
        }
        proto::AgentIncoming::KillSession {
            session_nid,
            reason,
        } => {
            tracing::info!(nid = %session_nid, reason = %reason, "收到 kill_session");
            match sessions.get(&session_nid).await {
                Ok(Some(session_summary)) => {
                    if !session_summary
                        .remote_enabled
                        .load(std::sync::atomic::Ordering::Relaxed)
                    {
                        tracing::warn!(nid = %session_nid, "忽略未开启远程的 kill_session");
                        return;
                    }
                    let nid = session_summary.nid;
                    let is_remote = session_summary
                        .remote_enabled
                        .load(std::sync::atomic::Ordering::Relaxed);

                    match sessions.report_session_ended(&nid, &reason).await {
                        Ok(Some(msg)) => {
                            if is_remote {
                                if let Some(tx) = outgoing.lock().await.as_ref() {
                                    let _ = tx.send(msg.to_json());
                                    tracing::info!(nid = %nid, reason = %reason, "session_ended 已发送到 Cloud");
                                }
                            }
                        }
                        Ok(None) => {
                            tracing::warn!(nid = %nid, "session_ended 已上报过，跳过");
                        }
                        Err(e) => {
                            tracing::error!(nid = %nid, error = %e, "session_ended 发送失败");
                        }
                    }

                    if let Err(e) = sessions.kill_session(&nid).await {
                        tracing::error!(nid = %nid, error = %e, "kill_session 失败");
                    }
                }
                Ok(None) => {
                    tracing::warn!(nid = %session_nid, "kill_session 目标会话不存在");
                }
                Err(e) => {
                    tracing::error!(nid = %session_nid, error = %e, "kill_session 查询会话失败");
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reconnect_permanent_desktop_access_rejections_disable_remote_state() {
        for error in [
            "membershipExpired: 会员已过期，无法开启远程会话",
            "membershipInactive: 会员已过期或账号已禁用，无法开启远程会话",
            "membershipGracePeriod: 会员已到期，无法开启远程会话",
            "userNotFound: 用户不存在",
        ] {
            assert!(is_permanent_reconnect_ack_error("desktop", error));
        }
    }

    #[test]
    fn reconnect_transient_or_non_desktop_errors_keep_remote_state() {
        assert!(!is_permanent_reconnect_ack_error(
            "desktop",
            "Redis write failed: connection reset"
        ));
        assert!(!is_permanent_reconnect_ack_error("desktop", "send failed"));
        assert!(!is_permanent_reconnect_ack_error(
            "ios",
            "membershipExpired: 会员已过期，无法开启远程会话"
        ));
    }

    #[tokio::test]
    async fn delivery_outbox_flushes_completed_result_when_sender_returns() {
        let outbox = DeliveryOutbox::with_capacity(2);
        outbox
            .enqueue("{\"data\":{\"requestId\":\"req-one\"}}".to_string())
            .await;
        let (sender, mut receiver) = mpsc::unbounded_channel();
        let outgoing = Arc::new(tokio::sync::Mutex::new(Some(sender)));

        flush_delivery_outbox(&outbox, &outgoing).await;

        assert_eq!(
            receiver.recv().await,
            Some("{\"data\":{\"requestId\":\"req-one\"}}".to_string())
        );
        assert_eq!(outbox.len().await, 1);
    }

    #[tokio::test]
    async fn delivery_acknowledgement_sends_the_next_pending_result() {
        let outbox = Arc::new(DeliveryOutbox::with_capacity(2));
        let first = "{\"data\":{\"requestId\":\"req-one\"}}".to_string();
        let second = "{\"data\":{\"requestId\":\"req-two\"}}".to_string();
        outbox.enqueue(first.clone()).await;
        outbox.enqueue(second.clone()).await;

        let (sender, mut receiver) = mpsc::unbounded_channel();
        let outgoing = Arc::new(tokio::sync::Mutex::new(Some(sender)));
        flush_delivery_outbox(&outbox, &outgoing).await;
        assert_eq!(receiver.recv().await, Some(first));

        handle_incoming(
            proto::AgentIncoming::ProjectDeliveryAck {
                request_id: "req-one".to_string(),
            },
            Arc::new(state::StateMachine::new(0)),
            outgoing,
            Arc::new(session::SessionManager::new(Box::new(
                session::MemorySessionStore::new(),
            ))),
            Arc::new(session::InputMerger::new()),
            Arc::new(ack::AckRegistry::new()),
            Arc::new(kn_agent::project_delivery::ProjectOperationGate::default()),
            outbox,
            Arc::new(tokio::sync::Mutex::new(
                kn_agent::project_session_index::ProjectRevisionClock::default(),
            )),
            Arc::new(tokio::sync::Mutex::new(
                kn_agent::project_session_index::ProjectActivityTracker::default(),
            )),
            Arc::new(kn_agent::project_session_index::ProjectScanGate::default()),
        )
        .await;

        assert_eq!(
            tokio::time::timeout(Duration::from_millis(100), receiver.recv()).await,
            Ok(Some(second))
        );
    }

    #[tokio::test]
    async fn delivery_outbox_retries_an_unacknowledged_result_while_connected() {
        let outbox = Arc::new(DeliveryOutbox::with_capacity(2));
        let message = "{\"data\":{\"requestId\":\"req-retry\"}}".to_string();
        outbox.enqueue(message.clone()).await;
        let (sender, mut receiver) = mpsc::unbounded_channel();
        let outgoing = Arc::new(tokio::sync::Mutex::new(Some(sender)));
        let shutdown = CancellationToken::new();

        let retry_task = tokio::spawn(delivery_outbox_retry_loop(
            outbox,
            outgoing,
            Duration::from_millis(10),
            shutdown.clone(),
        ));

        assert_eq!(
            tokio::time::timeout(Duration::from_millis(100), receiver.recv()).await,
            Ok(Some(message))
        );
        shutdown.cancel();
        retry_task.await.expect("retry loop exits cleanly");
    }

    #[tokio::test]
    async fn delivery_outbox_keeps_completed_result_after_flush_send_failure() {
        let outbox = DeliveryOutbox::with_capacity(2);
        outbox.enqueue("result-one".to_string()).await;
        let (sender, receiver) = mpsc::unbounded_channel::<String>();
        drop(receiver);
        let outgoing = Arc::new(tokio::sync::Mutex::new(Some(sender)));

        flush_delivery_outbox(&outbox, &outgoing).await;

        assert_eq!(outbox.len().await, 1);
    }

    #[tokio::test]
    async fn delivery_outbox_discards_oldest_when_bounded_capacity_is_reached() {
        let outbox = DeliveryOutbox::with_capacity(2);
        outbox
            .enqueue("{\"data\":{\"requestId\":\"first\"}}".to_string())
            .await;
        outbox
            .enqueue("{\"data\":{\"requestId\":\"second\"}}".to_string())
            .await;
        outbox
            .enqueue("{\"data\":{\"requestId\":\"third\"}}".to_string())
            .await;

        assert_eq!(
            outbox.take_front().await,
            Some("{\"data\":{\"requestId\":\"second\"}}".to_string())
        );
        assert!(outbox.acknowledge("second").await);
        assert_eq!(
            outbox.take_front().await,
            Some("{\"data\":{\"requestId\":\"third\"}}".to_string())
        );
    }

    #[test]
    fn background_dispatch_keeps_health_and_delivery_operations_off_the_serial_wss_loop() {
        let deliveries = [
            proto::AgentIncoming::ProjectGitStatus {
                project_key: "42:/repo".to_string(),
                device_id: 42,
                project_path: "/repo".to_string(),
                request_id: None,
                offset: 0,
                limit: 100,
                snapshot_id: None,
            },
            proto::AgentIncoming::ProjectGitCommit {
                project_key: "42:/repo".to_string(),
                device_id: 42,
                project_path: "/repo".to_string(),
                message: "commit".to_string(),
                paths: vec!["README.md".to_string()],
                scope: "selected".to_string(),
                snapshot_id: None,
                request_id: None,
            },
            proto::AgentIncoming::ProjectGitPush {
                project_key: "42:/repo".to_string(),
                device_id: 42,
                project_path: "/repo".to_string(),
                request_id: None,
            },
            proto::AgentIncoming::ProjectPrStatus {
                project_key: "42:/repo".to_string(),
                device_id: 42,
                project_path: "/repo".to_string(),
                request_id: None,
            },
            proto::AgentIncoming::ProjectPrDetails {
                project_key: "42:/repo".to_string(),
                device_id: 42,
                project_path: "/repo".to_string(),
                request_id: None,
            },
            proto::AgentIncoming::ProjectPrCreate {
                project_key: "42:/repo".to_string(),
                device_id: 42,
                project_path: "/repo".to_string(),
                base: "main".to_string(),
                title: "Create PR".to_string(),
                body: String::new(),
                request_id: None,
            },
        ];

        assert!(deliveries.iter().all(should_dispatch_in_background));
        assert!(!should_dispatch_in_background(
            &proto::AgentIncoming::Input {
                session_nid: "s_test".to_string(),
                seq: 1,
                content: "keep terminal responsive".to_string(),
                from_user_id: 1,
            }
        ));
        assert!(should_dispatch_in_background(
            &proto::AgentIncoming::DeviceHealth {
                device_id: 1,
                request_id: "health-request".to_string(),
            }
        ));
    }

    #[test]
    fn registered_project_path_canonicalizes_aliases_before_locking() {
        let project = tempfile::tempdir().expect("temporary project");
        let registered = project
            .path()
            .canonicalize()
            .expect("canonical project path");
        let alias = project.path().join(".");

        let canonical = canonical_registered_project_path(
            vec![registered.clone()],
            alias.to_str().expect("utf8 alias"),
        )
        .expect("registered alias should be recognized");

        assert_eq!(canonical, registered);
        assert_eq!(
            canonical_project_key(42, canonical.to_str().expect("utf8 canonical path")),
            canonical_project_key(42, registered.to_str().expect("utf8 registered path"))
        );
    }
}

// ── Logging ─────────────────────────────────────────────────

fn init_logging(log_dir: &std::path::Path) -> std::result::Result<(), Box<dyn std::error::Error>> {
    use tracing_subscriber::filter::EnvFilter;
    use tracing_subscriber::fmt;
    use tracing_subscriber::layer::SubscriberExt;
    use tracing_subscriber::util::SubscriberInitExt;
    use tracing_subscriber::Layer;

    std::fs::create_dir_all(log_dir)?;

    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    let stderr_layer = fmt::layer().with_target(false).with_filter(env_filter);

    let file_appender = tracing_appender::rolling::daily(log_dir, "agent");
    let file_layer = fmt::layer()
        .json()
        .with_writer(file_appender)
        .with_filter(EnvFilter::new("info"));

    tracing_subscriber::registry()
        .with(stderr_layer)
        .with(file_layer)
        .init();

    let log_dir = log_dir.to_path_buf();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(3600));
        loop {
            interval.tick().await;
            cleanup_old_logs(&log_dir, 7);
        }
    });

    Ok(())
}

fn cleanup_old_logs(log_dir: &std::path::Path, max_age_days: i64) {
    let cutoff = std::time::SystemTime::now()
        - std::time::Duration::from_secs((max_age_days * 86400) as u64);

    if let Ok(entries) = std::fs::read_dir(log_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            if name_str.starts_with("agent.") {
                if let Ok(meta) = entry.metadata() {
                    if let Ok(modified) = meta.modified() {
                        if modified < cutoff {
                            let _ = std::fs::remove_file(&path);
                        }
                    }
                }
            }
        }
    }
}

fn ensure_dirs(
    agent_dir: &std::path::Path,
    log_dir: &std::path::Path,
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    std::fs::create_dir_all(agent_dir)?;
    std::fs::create_dir_all(log_dir)?;
    Ok(())
}
