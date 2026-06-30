//! kn-agent — PTY 多路复用守护进程
//!
//! 让用户通过 iOS 远程控制 Mac 上运行的 AI CLI 工具（Claude Code、Codex 等）。

#![allow(dead_code)]

use clap::Parser;
use kn_agent::{
    ack, bind, config, device, error::AgentError, ipc, proto, session, state, ws_client,
};
use kn_common::project::ProjectInfo;
use notify::{Event, EventKind, RecursiveMode, Watcher};
use std::sync::Arc;
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

// ── Project loading & watching ─────────────────────────────────

/// 读取 ~/.kn/projects.json，返回项目列表。
/// 文件不存在或解析失败时返回空列表（静默降级）。
///
/// 使用 spawn_blocking 将文件 I/O 移出 Tokio 异步运行时，
/// 避免在 worker 线程上执行阻塞操作。
async fn load_projects() -> Vec<ProjectInfo> {
    let path = kn_common::path::config_dir().join("projects.json");

    tokio::task::spawn_blocking(move || {
        match std::fs::read_to_string(&path) {
            Ok(content) => {
                serde_json::from_str::<Vec<ProjectInfo>>(&content)
                    .unwrap_or_else(|e| {
                        tracing::warn!("解析 projects.json 失败: {}", e);
                        vec![]
                    })
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                tracing::debug!("projects.json 不存在，跳过项目上报");
                vec![]
            }
            Err(e) => {
                tracing::warn!("读取 projects.json 失败: {}", e);
                vec![]
            }
        }
    })
    .await
    .unwrap_or_else(|_| {
        tracing::warn!("spawn_blocking 执行失败，跳过项目上报");
        vec![]
    })
}

/// 发送 project_list 到云端。
async fn send_project_list(
    outgoing: &std::sync::Arc<tokio::sync::Mutex<Option<tokio::sync::mpsc::UnboundedSender<String>>>>,
) {
    let projects = load_projects().await;
    let info: Vec<proto::ProjectInfoOut> = projects.iter().map(|p| p.into()).collect();
    let msg = proto::WsMessageBuilder::project_list(&info);
    if let Some(tx) = outgoing.lock().await.as_ref() {
        let _ = tx.send(msg);
        tracing::info!(count = info.len(), "已上报项目列表");
    }
}

/// 启动 projects.json 文件监听，变化时自动重新上报。
/// 返回 watcher handle，需要保持存活（drop 时停止监听）。
fn start_project_watcher(
    outgoing: std::sync::Arc<tokio::sync::Mutex<Option<tokio::sync::mpsc::UnboundedSender<String>>>>,
) -> Option<notify::RecommendedWatcher> {
    let path = kn_common::path::config_dir().join("projects.json");

    // 使用 tokio::sync::mpsc 避免在 Tokio 任务中阻塞 worker 线程
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<()>();

    // 记录要监听的文件名，用于在回调中过滤
    let target_name = path
        .file_name()
        .map(|n| n.to_os_string())
        .unwrap_or_default();

    let mut watcher = match notify::recommended_watcher(
        move |res: Result<Event, notify::Error>| {
            if let Ok(event) = res {
                // 只响应 projects.json 的变更（原子替换 → 父目录下其他文件不改触发）
                let is_projects = event.paths.iter().any(|p| {
                    p.file_name().map_or(false, |n| n == target_name)
                });
                if is_projects
                    && matches!(event.kind, EventKind::Modify(_) | EventKind::Create(_))
                {
                    tracing::debug!(paths = ?event.paths, "projects.json 变更，触发重新上报");
                    let _ = tx.send(());
                }
            }
        },
    ) {
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
    tracing::info!("kn-agent v{} 启动", env!("CARGO_PKG_VERSION"));
    tracing::info!(
        "配置: cloud={}, dir={}, machine_id={}",
        cfg.cloud_url,
        cfg.config_dir.display(),
        cfg.machine_id
    );

    // ── 3. 确保目录存在 ──
    ensure_dirs(&cfg.agent_dir, &cfg.log_dir)?;

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
    state_machine
        .transition(state::StateEvent::Start)
        .await?;
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
    let token = device::load_device_token();
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

    // ── ACK 注册表（session_created → session_created_ack 关联） ──
    let ack_registry = Arc::new(ack::AckRegistry::new());

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
        if has_token { "initializing" } else { "disabled (no token)" }
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

                let t = match device::load_device_token() {
                    Some(tok) if !tok.is_empty() => tok,
                    _ => {
                        tracing::warn!("WSS 触发但未找到 device_token，跳过");
                        continue;
                    }
                };

                tracing::info!("正在启动 WSS 连接...");

                // 确保状态正确（绑定完成时可能已经是 Connected）
                let current = state_machine.current().await;
                if current != state::AgentState::Connected
                    && current != state::AgentState::Reconnecting
                {
                    let _ = state_machine
                        .transition(state::StateEvent::WsConnected { has_token: true })
                        .await;
                }

                // 创建入站消息通道
                let (incoming_tx, rx) = mpsc::unbounded_channel::<proto::AgentIncoming>();
                incoming_rx = Some(rx);

                // 启动 project watcher
                _project_watcher = start_project_watcher(outgoing_tx_ref.clone());

                // 复制 WSS 所需参数
                let ws_token = t;
                let ws_url = cfg.cloud_url.clone();
                let ws_machine = cfg.machine_id.clone();
                let ws_version = env!("CARGO_PKG_VERSION").to_string();
                let ws_os = cfg.os_version.clone();
                let ws_host = cfg.hostname.clone();
                let ws_state = state_machine.clone();
                let ws_outgoing = outgoing_tx_ref.clone();
                let ws_shutdown = shutdown.clone();

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
            }

            // ── 处理 WSS 入站消息 ──
            msg = async {
                match incoming_rx.as_mut() {
                    Some(rx) => rx.recv().await,
                    None => std::future::pending().await,
                }
            } => {
                if let Some(m) = msg {
                    handle_incoming(
                        m,
                        state_machine.clone(),
                        outgoing_tx_ref.clone(),
                        sessions.clone(),
                        input_merger.clone(),
                        ack_registry.clone(),
                    ).await;
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
                            "device_token 已失效，切换至未绑定状态（IPC 仍运行），保留旧 token 文件"
                        );
                        let _ = state_machine
                            .transition(state::StateEvent::WsConnected { has_token: false })
                            .await;
                    }
                    Some(Ok(Ok(()))) => {
                        tracing::info!("WSS 循环正常退出");
                        break; // shutdown 触发的正常退出
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
    state_machine
        .transition(state::StateEvent::Stop)
        .await?;
    tracing::info!("Agent 已停止");

    Ok(())
}

// ── Message handling ────────────────────────────────────────

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
                        if let Ok(Some(msg)) = sessions
                            .report_session_ended(&s.nid, "process_exit")
                            .await
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

async fn handle_incoming(
    msg: proto::AgentIncoming,
    state: Arc<state::StateMachine>,
    outgoing: Arc<tokio::sync::Mutex<Option<mpsc::UnboundedSender<String>>>>,
    sessions: Arc<session::SessionManager>,
    input_merger: Arc<session::InputMerger>,
    ack_registry: Arc<ack::AckRegistry>, // Phase 3 开始使用
) {

    match msg {
        proto::AgentIncoming::Pong { .. } => {}
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
            // 上报 profile 列表
            if let Ok(profiles) = kn_common::profile::list_profiles_cmd() {
                let info: Vec<proto::ProfileInfo> = profiles.profiles.iter().map(|p| p.into()).collect();
                let msg = proto::WsMessageBuilder::profile_list(&info);
                if let Some(tx) = outgoing.lock().await.as_ref() {
                    let _ = tx.send(msg);
                }
            }

            // 上报 project 列表
            send_project_list(&outgoing).await;

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
                        let msg_id = format!("reconnect-{}", ack_nid);
                        let msg = proto::WsMessageBuilder::session_created_with_msg_id(
                            &ack_nid, &ack_tool, &ack_cwd,
                            ack_profile.as_deref(),
                            ack_cols, ack_rows, &ack_source,
                            Some(&msg_id),
                        );

                        let send_ok = {
                            let guard = ack_outgoing.lock().await;
                            match guard.as_ref() {
                                Some(tx) => tx.send(msg).is_ok(),
                                None => false,
                            }
                        };

                        if !send_ok {
                            tracing::warn!(nid = %ack_nid, "reconnect: session_created 发送失败");
                            return;
                        }

                        tracing::info!(nid = %ack_nid, "🔄 [RECONNECT] 重连后补发 session_created，等待 ACK");
                        let rx = ack_registry.register(&ack_nid).await;
                        match tokio::time::timeout(
                            tokio::time::Duration::from_secs(10), rx
                        ).await {
                            Ok(Ok(crate::ack::AckResult::Ok)) => {
                                tracing::info!(nid = %ack_nid, "reconnect: ACK 成功，会话已恢复");
                            }
                            _ => {
                                // 重连 ACK 失败 → 降级，关闭远程
                                tracing::warn!(nid = %ack_nid, "reconnect: ACK 失败，关闭远程");
                                let _ = ack_sessions.set_remote_enabled(&ack_nid, false).await;
                            }
                        }
                    });
                }
            }
        }
        proto::AgentIncoming::StartSession {
            tool,
            profile,
            cwd,
            from_user_id,
            cols,
            rows,
        } => {
            // Agent 自行生成 sessionId，cloud 不再预分配
            let session_nid = format!("s_{}", nanoid::nanoid!(12));
            tracing::info!(
                nid = %session_nid,
                tool = %tool,
                profile = ?profile,
                user = from_user_id,
                "收到远程启动会话请求"
            );

            let cwd_resolved = cwd.unwrap_or_else(|| ".".into());

            // 1. Create session record（create 内部持有 create_mutex，count+insert 原子性，会话数限制由 create 统一保证）
            match sessions
                .create(
                    session_nid.clone(),
                    "ios".to_string(),
                    tool.clone(),
                    profile.clone(),
                    cwd_resolved.clone(),
                    crate::session::SessionKind::Native,
                )
                .await
            {
                Ok(session) => {
                    // iOS 远程会话：显式开启 remote_enabled（create 默认 false）
                    let _ = sessions.set_remote_enabled(&session_nid, true).await;

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
                                    Ok(_) => tracing::info!(len = len, "📤 [OUTPUT] 已转发到全局 WSS"),
                                    Err(e) => tracing::error!(len = len, error = %e, "📤 [OUTPUT] 转发失败"),
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
                    let t = tool.clone();
                    let p = profile.clone();
                    let c = cwd_resolved.clone();
                    let remote_enabled = Some(session.remote_enabled.clone());
                    let out = outgoing.clone();

                    tokio::spawn(async move {
                        let s_cleanup = s.clone();
                        match s
                            .start_session(&nid, &t, p.as_deref(), &c, cols, rows, wss_tx, ipc_tx, m, remote_enabled)
                            .await
                        {
                            Ok(_fanout) => {
                                tracing::info!(nid = %nid, tool = %t, "WSS PTY session started");
                            }
                            Err(e) => {
                                tracing::error!(nid = %nid, error = %e, "WSS PTY session start failed — cleaning up orphaned session record");
                                // 清理残留的 session 记录，防止变成永久僵尸会话
                                let _ = s_cleanup.end(&nid).await;
                                if let Ok(Some(msg)) = s_cleanup.report_session_ended(&nid, "start_failed").await {
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
                    let ack_tool = tool.clone();
                    let ack_cwd = cwd_resolved.clone();
                    let ack_profile = profile.clone();
                    let ack_cols = cols;
                    let ack_rows = rows;

                    tokio::spawn(async move {
                        const MAX_RETRIES: u32 = 3;
                        let backoffs = [1u64, 2, 4];

                        for attempt in 0..MAX_RETRIES {
                            let msg_id = format!("{}-{}", ack_nid, attempt);
                            let msg = proto::WsMessageBuilder::session_created_with_msg_id(
                                &ack_nid, &ack_tool, &ack_cwd,
                                ack_profile.as_deref(),
                                ack_cols, ack_rows, "ios",
                                Some(&msg_id),
                            );

                            let send_ok = {
                                let guard = ack_outgoing.lock().await;
                                match guard.as_ref() {
                                    Some(tx) => tx.send(msg).is_ok(),
                                    None => false,
                                }
                            };

                            if !send_ok {
                                tracing::warn!(nid = %ack_nid, attempt = attempt, "WSS channel 不可用");
                                if attempt + 1 < MAX_RETRIES {
                                    tokio::time::sleep(tokio::time::Duration::from_secs(backoffs[attempt as usize])).await;
                                    continue;
                                }
                                break;
                            }

                            tracing::info!(nid = %ack_nid, attempt = attempt, "session_created 已发送，等待 ACK");
                            let rx = ack_registry.register(&ack_nid).await;
                            match tokio::time::timeout(
                                tokio::time::Duration::from_secs(10), rx
                            ).await {
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
                                    tracing::warn!(nid = %ack_nid, attempt = attempt, "session_created ACK 超时");
                                    if attempt + 1 < MAX_RETRIES {
                                        tokio::time::sleep(tokio::time::Duration::from_secs(backoffs[attempt as usize])).await;
                                        continue;
                                    }
                                }
                            }
                        }

                        // All retries exhausted or cloud rejected → kill PTY + clean up
                        tracing::error!(nid = %ack_nid, "所有 session_created ACK 重试失败，终止会话");
                        let _ = ack_sessions.kill_session(&ack_nid).await;
                        let _ = ack_sessions.report_session_ended(&ack_nid, "wss_ack_failed").await;
                    });

                    // Transition to Running state
                    let _ = state
                        .transition(state::StateEvent::SessionStarted)
                        .await;
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
                        let nid = session_summary.nid;
                        let is_remote = session_summary.remote_enabled.load(std::sync::atomic::Ordering::Relaxed);

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
                        let nid = session_summary.nid.clone();
                        let text = content.clone();
                        input_merger
                            .push(session::InputMessage {
                                session_id: nid.clone(),
                                text,
                                source: "ios".into(),
                            })
                            .await;
                        tracing::info!(nid = %nid, "📱 [INPUT] 已推入 InputMerger 队列");
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
            let signal_name = signal
                .get("signal")
                .and_then(|v| v.as_str())
                .unwrap_or("");

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
                    let text = String::from_utf8_lossy(&byte).to_string();
                    input_merger
                        .push(session::InputMessage {
                            session_id: session_summary.nid,
                            text,
                            source: "ios".into(),
                        })
                        .await;
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
                    if let Err(e) = sessions.resize(&session_summary.nid, cols, rows).await {
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

            match session::OutputFanout::replay_log(&session_nid) {
                Some(data) => {
                    // 环形日志存储的是原始字节（包含 ANSI escape），直接转为 String
                    let ansi_text = String::from_utf8_lossy(&data).into_owned();
                    let parts = ansi_text.as_bytes().len();
                    tracing::info!(
                        nid = %session_nid,
                        bytes = parts,
                        "回放输出日志"
                    );

                    // 分块发送：每块最多 32KB，避免单条 WSS 消息过大
                    const CHUNK_SIZE: usize = 32 * 1024;
                    let mut offset = 0;
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
                        offset = chunk_end;
                    }
                }
                None => {
                    tracing::warn!(
                        nid = %session_nid,
                        "replay_output: 未找到输出日志或日志为空"
                    );
                }
            }
        }
        proto::AgentIncoming::Unknown { msg_type, .. } => {
            tracing::debug!("未知消息类型: {}", msg_type);
        }
        proto::AgentIncoming::SessionCreatedAck { session_nid, status, .. } => {
            let result = if status == "ok" {
                crate::ack::AckResult::Ok
            } else {
                crate::ack::AckResult::Error(status.clone())
            };
            let resolved = ack_registry.resolve(&session_nid, result).await;
            tracing::info!(nid = %session_nid, status = %status, resolved = resolved, "收到 session_created_ack");
        }
        proto::AgentIncoming::ResumeSession { session_nid } => {
            tracing::info!(nid = %session_nid, "收到 resume_session");
            match sessions.get(&session_nid).await {
                Ok(Some(summary)) if summary.status != crate::session::SessionStatus::Ended => {
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
    }
}

// ── Logging ─────────────────────────────────────────────────

fn init_logging(
    log_dir: &std::path::Path,
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    use tracing_subscriber::filter::EnvFilter;
    use tracing_subscriber::fmt;
    use tracing_subscriber::layer::SubscriberExt;
    use tracing_subscriber::util::SubscriberInitExt;
    use tracing_subscriber::Layer;

    std::fs::create_dir_all(log_dir)?;

    let env_filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

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
