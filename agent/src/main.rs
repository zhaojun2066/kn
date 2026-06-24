//! kn-agent — PTY 多路复用守护进程
//!
//! 让用户通过 iOS 远程控制 Mac 上运行的 AI CLI 工具（Claude Code、Codex 等）。

#![allow(dead_code)]

use clap::Parser;
use kn_agent::{
    bind, config, device, ipc, proto, session, state, ws_client,
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
fn load_projects() -> Vec<ProjectInfo> {
    let path = kn_common::path::config_dir().join("projects.json");

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
}

/// 发送 project_list 到云端。
async fn send_project_list(
    outgoing: &std::sync::Arc<tokio::sync::Mutex<Option<tokio::sync::mpsc::UnboundedSender<String>>>>,
) {
    let projects = load_projects();
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

    let (tx, rx) = std::sync::mpsc::channel();

    let mut watcher = match notify::recommended_watcher(
        move |res: Result<Event, notify::Error>| {
            if let Ok(event) = res {
                if matches!(event.kind, EventKind::Modify(_) | EventKind::Create(_)) {
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
            match rx.recv() {
                Ok(()) => {
                    // 简单防抖：收到事件后等 2 秒，期间的新事件被丢弃
                    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
                    // 排空积压事件
                    while rx.try_recv().is_ok() {}
                    send_project_list(&outgoing).await;
                }
                Err(std::sync::mpsc::RecvError) => break,
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

    // ── 10. 始终启动 IPC 服务器 ──
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
        );
        let ipc_shutdown = shutdown.clone();
        tokio::spawn(async move {
            if let Err(e) = ipc.run(ipc_shutdown).await {
                tracing::error!("IPC 服务器错误: {}", e);
            }
        });
    }

    // ── 11. 有 token 时，并行运行 WSS 连接 ──
    let _project_watcher;
    if has_token {
        let t = token.unwrap();
        state_machine
            .transition(state::StateEvent::WsConnected { has_token: true })
            .await?;

        tracing::info!("已找到 device_token，并行运行 WSS 连接 + IPC 服务...");

        // 消息通道
        let (incoming_tx, mut incoming_rx) =
            mpsc::unbounded_channel::<proto::AgentIncoming>();
        let outgoing_tx_ref = Arc::new(tokio::sync::Mutex::new(
            None::<mpsc::UnboundedSender<String>>,
        ));

        let ws_state = state_machine.clone();
        let ws_shutdown = shutdown.clone();
        let ws_token = t.clone();
        let ws_url = cfg.cloud_url.clone();
        let ws_machine = cfg.machine_id.clone();
        let ws_version = env!("CARGO_PKG_VERSION").to_string();
        let ws_os = cfg.os_version.clone();
        let ws_host = cfg.hostname.clone();
        let ws_outgoing = outgoing_tx_ref.clone();
        let ws_sessions = sessions.clone();
        let ws_input_merger = input_merger.clone();

        // WSS 任务：在后台运行连接循环 + 消息处理
        // IPC 服务器已在上一步启动，二者并行运行
        //
        // 注意：必须在 spawn 前 clone 所需的值，因为外层 loop 和内层
        // ws_client::run_ws_loop 都需要使用它们。
        let ws_state_inner = ws_state.clone();
        let ws_outgoing_inner = ws_outgoing.clone();
        let ws_shutdown_inner = ws_shutdown.clone();

        tokio::spawn(async move {
            let mut ws_handle = tokio::spawn(async move {
                ws_client::run_ws_loop(
                    &ws_token,
                    &ws_url,
                    &ws_machine,
                    &ws_version,
                    &ws_os,
                    &ws_host,
                    ws_state_inner,
                    ws_outgoing_inner,
                    incoming_tx,
                    ws_shutdown_inner,
                )
                .await
            });

            loop {
                tokio::select! {
                    result = &mut ws_handle => {
                        match result {
                            Ok(Err(ref e)) if e.to_string().contains("AUTH_REJECTED") => {
                                tracing::warn!("device_token 已失效，切换至未绑定状态（IPC 仍运行），保留旧 token 文件");
                                // 不删除 token 文件：保留旧 token 以便重新绑定时覆盖，
                                // 同时支持后端 URL 切换（切回原后端时 token 仍有效）
                                let _ = ws_state
                                    .transition(state::StateEvent::WsConnected { has_token: false })
                                    .await;
                            }
                            Ok(Ok(())) => tracing::info!("WSS 循环正常退出"),
                            Ok(Err(e)) => tracing::error!("WSS 循环错误: {}", e),
                            Err(e) => tracing::error!("WSS 任务 panic: {}", e),
                        }
                        break;
                    }
                    _ = ws_shutdown.cancelled() => {
                        tracing::info!("WSS 任务收到关闭信号");
                        break;
                    }
                    msg = incoming_rx.recv() => {
                        match msg {
                            Some(m) => {
                                handle_incoming(
                                    m,
                                    ws_state.clone(),
                                    ws_outgoing.clone(),
                                    ws_sessions.clone(),
                                    ws_input_merger.clone(),
                                ).await;
                            }
                            None => {
                                tracing::info!("WSS 入站消息通道已关闭");
                                break;
                            }
                        }
                    }
                }
            }
        });

        // 启动 projects.json 文件监听，变化时自动重新上报
        _project_watcher = start_project_watcher(outgoing_tx_ref.clone());
    } else {
        // 无 token：直接进入 Unbound 状态，等待桌面应用通过 IPC 发起绑定
        state_machine
            .transition(state::StateEvent::WsConnected { has_token: false })
            .await?;
        _project_watcher = None;
    }

    // ── 12. 等待关闭信号 ──
    tracing::info!(
        "Agent 就绪: IPC={}, WSS={}",
        cfg.ipc_socket_path.display(),
        if has_token { "enabled" } else { "disabled (no token)" }
    );
    tracing::info!("使用以下方式绑定:");
    tracing::info!("  1. 运行 'kn-agent bind' 开始绑定流程");
    tracing::info!("  2. 在 iOS App 中扫描二维码");
    tracing::info!("  3. 通过 IPC 发送 bind 请求");

    shutdown.cancelled().await;

    // ── 13. 优雅关闭 ──
    state_machine
        .transition(state::StateEvent::Stop)
        .await?;
    tracing::info!("Agent 已停止");

    Ok(())
}

// ── Message handling ────────────────────────────────────────

async fn handle_incoming(
    msg: proto::AgentIncoming,
    state: Arc<state::StateMachine>,
    outgoing: Arc<tokio::sync::Mutex<Option<mpsc::UnboundedSender<String>>>>,
    sessions: Arc<session::SessionManager>,
    input_merger: Arc<session::InputMerger>,
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

            // 崩溃恢复：加载 checkpoint → 上报中断会话 → 清理
            let interrupted = session::load_checkpoints();
            if !interrupted.is_empty() {
                tracing::info!(count = interrupted.len(), "检测到中断会话，上报云端");
                let msg = proto::WsMessageBuilder::sessions_interrupted(&interrupted);
                if let Some(tx) = outgoing.lock().await.as_ref() {
                    let _ = tx.send(msg);
                }
                session::cleanup_checkpoints();
            }
        }
        proto::AgentIncoming::StartSession {
            db_session_id,
            session_nid,
            tool,
            profile,
            cwd,
            from_user_id,
        } => {
            tracing::info!(
                nid = %session_nid,
                db_id = db_session_id,
                tool = %tool,
                profile = ?profile,
                user = from_user_id,
                "收到远程启动会话请求"
            );

            let cwd_resolved = cwd.unwrap_or_else(|| ".".into());
            let cols: u16 = 80;
            let rows: u16 = 24;

            // 1. Create session record
            match sessions
                .create(
                    session_nid.clone(),
                    db_session_id,
                    tool.clone(),
                    profile.clone(),
                    cwd_resolved.clone(),
                )
                .await
            {
                Ok(_session) => {
                    // 2. Send session_created confirmation to cloud
                    let created_msg = proto::WsMessageBuilder::session_created(db_session_id);
                    if let Some(tx) = outgoing.lock().await.as_ref() {
                        let _ = tx.send(created_msg);
                    }

                    // 3. Spawn PTY + CLI process
                    let (wss_tx, _wss_rx) = mpsc::unbounded_channel::<String>();
                    let (ipc_tx, _ipc_rx) = mpsc::unbounded_channel::<String>();
                    let s = sessions.clone();
                    let m = input_merger.clone();
                    let nid = session_nid.clone();
                    let t = tool.clone();
                    let p = profile.clone();
                    let c = cwd_resolved.clone();

                    tokio::spawn(async move {
                        match s
                            .start_session(&nid, &t, p.as_deref(), &c, cols, rows, wss_tx, ipc_tx, m)
                            .await
                        {
                            Ok(_fanout) => {
                                tracing::info!(nid = %nid, tool = %t, "WSS PTY session started");
                            }
                            Err(e) => {
                                tracing::error!(nid = %nid, error = %e, "WSS PTY session start failed");
                                // 注意: agent_error 不在 Java 白名单中，无法通过 WSS 发送；
                                // 错误仅记录到本地日志。如需云端感知，应先修改 Java ALLOWED_MESSAGES。
                            }
                        }
                    });

                    // 4. Transition to Running state
                    let _ = state
                        .transition(state::StateEvent::SessionStarted)
                        .await;
                }
                Err(e) => {
                    tracing::error!(error = %e, "创建会话失败");
                }
            }
        }
        proto::AgentIncoming::Input {
            db_session_id,
            seq,
            content,
            ..
        } => {
            tracing::debug!(
                db_id = db_session_id,
                seq = seq,
                len = content.len(),
                "收到远程输入"
            );

            // Lookup session by DB id and route input to PTY stdin
            match sessions.get_by_db_id(db_session_id).await {
                Ok(Some(session_summary)) => {
                    input_merger
                        .push(session::InputMessage {
                            session_id: session_summary.nid,
                            text: content,
                            source: "ios".into(),
                        })
                        .await;
                }
                Ok(None) => {
                    tracing::warn!(db_id = db_session_id, "Input 目标会话不存在");
                }
                Err(e) => {
                    tracing::error!(db_id = db_session_id, error = %e, "Input 查询会话失败");
                }
            }
        }
        proto::AgentIncoming::Ctrl {
            db_session_id,
            signal,
        } => {
            tracing::debug!(
                db_id = db_session_id,
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

            match sessions.get_by_db_id(db_session_id).await {
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
                    tracing::warn!(db_id = db_session_id, "Ctrl 目标会话不存在");
                }
                Err(e) => {
                    tracing::error!(db_id = db_session_id, error = %e, "Ctrl 查询会话失败");
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
        proto::AgentIncoming::Unknown { msg_type, .. } => {
            tracing::debug!("未知消息类型: {}", msg_type);
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
