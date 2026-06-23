//! kn-agent — PTY 多路复用守护进程
//!
//! 让用户通过 iOS 远程控制 Mac 上运行的 AI CLI 工具（Claude Code、Codex 等）。

#![allow(dead_code)]

use clap::Parser;
use kn_agent::{
    bind, config, device, ipc, proto, session, state, ws_client,
};
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

    // Track whether we need to fall back to IPC mode after WSS ends
    let mut fallback_to_ipc = !has_token;

    if has_token {
        let t = token.unwrap();
        state_machine
            .transition(state::StateEvent::WsConnected { has_token: true })
            .await?;

        tracing::info!("已找到 device_token，连接云端...");

        // 消息通道
        let (incoming_tx, mut incoming_rx) =
            mpsc::unbounded_channel::<proto::AgentIncoming>();
        // 出站消息共享 sender（ws_client 在每次连接时更新）
        let outgoing_tx_ref = Arc::new(tokio::sync::Mutex::new(
            None::<mpsc::UnboundedSender<String>>,
        ));

        // 创建会话管理器 + 输入合并器（WSS 路径，对齐 Java handleStartSession/handleInput 行为）
        let store = Box::new(session::MemorySessionStore::new());
        let sessions = Arc::new(session::SessionManager::new(store));
        let input_merger = Arc::new(session::InputMerger::new());

        // 启动 WSS 连接循环
        let ws_state = state_machine.clone();
        let ws_shutdown = shutdown.clone();
        let ws_token = t.clone();
        let ws_url = cfg.cloud_url.clone();
        let ws_machine = cfg.machine_id.clone();
        let ws_version = env!("CARGO_PKG_VERSION").to_string();
        let ws_os = cfg.os_version.clone();
        let ws_host = cfg.hostname.clone();
        let ws_outgoing = outgoing_tx_ref.clone();

        let mut ws_handle = tokio::spawn(async move {
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
        });

        // ── 主消息循环 ──
        let main_shutdown = shutdown.clone();
        let main_state = state_machine.clone();
        let main_outgoing = outgoing_tx_ref.clone();

        loop {
            tokio::select! {
                result = &mut ws_handle => {
                    match result {
                        Ok(Err(ref e)) if e.to_string().contains("AUTH_REJECTED") => {
                            tracing::warn!("device_token 已失效，删除并切换至 IPC 模式");
                            device::delete_device_token();
                            // 转换至 Unbound（状态转换已在 ws_client 中尝试，这里确保成功）
                            let _ = main_state
                                .transition(state::StateEvent::WsConnected { has_token: false })
                                .await;
                            fallback_to_ipc = true;
                        }
                        Ok(Ok(())) => tracing::info!("WSS 循环正常退出"),
                        Ok(Err(e)) => tracing::error!("WSS 循环错误: {}", e),
                        Err(e) => tracing::error!("WSS 任务 panic: {}", e),
                    }
                    break;
                }
                _ = main_shutdown.cancelled() => {
                    tracing::info!("主循环收到关闭信号");
                    break;
                }
                msg = incoming_rx.recv() => {
                    match msg {
                        Some(m) => {
                            handle_incoming(
                                m,
                                main_state.clone(),
                                main_outgoing.clone(),
                                sessions.clone(),
                                input_merger.clone(),
                            ).await;
                        }
                        None => {
                            tracing::info!("入站消息通道已关闭");
                            break;
                        }
                    }
                }
            }
        }
    }

    if fallback_to_ipc {
        // Only transition if not already Unbound (ws_client may have set it on AUTH_REJECTED)
        if state_machine.current().await != state::AgentState::Unbound {
            state_machine
                .transition(state::StateEvent::WsConnected { has_token: false })
                .await?;
        }

        tracing::info!("IPC 服务器已启动: {}", cfg.ipc_socket_path.display());
        tracing::info!("使用以下方式绑定:");
        tracing::info!("  1. 运行 'kn-agent bind' 开始绑定流程");
        tracing::info!("  2. 在 iOS App 中扫描显示的二维码");
        tracing::info!("  3. 通过 IPC 发送 bind 请求: echo '{{\"id\":\"1\",\"method\":\"bind\",\"params\":{{}}}}' | nc -U {}", cfg.ipc_socket_path.display());

        // Create session manager for IPC
        let store = Box::new(session::MemorySessionStore::new());
        let sessions = Arc::new(session::SessionManager::new(store));
        let input_merger = Arc::new(session::InputMerger::new());

        // Start IPC server
        let ipc = ipc::IpcServer::new(
            cfg.ipc_socket_path.clone(),
            state_machine.clone(),
            sessions,
            cfg.cloud_http_url.clone(),
            cfg.machine_id.clone(),
            cfg.hostname.clone(),
            cfg.purchase_url.clone(),
            input_merger,
        );
        let ipc_shutdown = shutdown.clone();
        let ipc_run_shutdown = shutdown.clone();

        tokio::select! {
            _ = ipc_shutdown.cancelled() => {
                tracing::info!("IPC 服务器收到关闭信号");
            }
            result = tokio::spawn(async move {
                if let Err(e) = ipc.run(ipc_run_shutdown).await {
                    tracing::error!("IPC 服务器错误: {}", e);
                }
            }) => {
                match result {
                    Ok(()) => tracing::info!("IPC 服务器正常退出"),
                    Err(e) => tracing::error!("IPC 任务 panic: {}", e),
                }
            }
        }
    }

    // ── 9. 优雅关闭 ──
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
