//! `kn-agent bind` CLI 命令 — 设备绑定入口 + 绑定码展示。

use crate::config::AgentConfig;
use crate::device;
use crate::error::{AgentError, Result};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;
use tokio_util::sync::CancellationToken;

/// `kn-agent bind` 命令入口。
///
/// 流程：
/// 1. 调用 device::bind_init() 获取待确认配对申请
/// 2. 在终端显示 ASCII 框，展示绑定码和主机名
/// 3. 轮询 device::bind_poll() 等待 iOS App 授权
/// 4. 原子保存 device_token 后调用 bind-activate 创建正式设备
pub async fn run_bind_command(config: AgentConfig) -> Result<()> {
    // A daemon owns WSS lifecycle and durable recovery.  Never run an
    // independent CLI activation worker alongside it; delegate through the
    // same IPC method Desktop uses instead.
    if delegate_to_running_daemon(&config).await? {
        return Ok(());
    }

    let shutdown = shutdown_on_ctrl_c();

    // Resume the only ambiguous state first.  The marker survives an activate
    // response loss and must be probed idempotently before any new QR is made.
    if let Some(activation) = device::load_pending_activation() {
        eprintln!("[kn-agent] 正在恢复上次未完成的正式绑定确认...");
        finish_activation(&config, &activation, shutdown).await?;
        println!("\n设备绑定已确认。启动 kn-agent 守护进程后将自动上线。");
        return Ok(());
    }

    device::migrate_legacy_pending_binding()?;
    let replacement = device::load_legacy_binding_replacement();

    // ── Step 1: 请求绑定码 ──
    let pending = match device::load_pending_binding() {
        Some(pending) if pending.pairing_expires_at_ms > now_ms() => pending,
        Some(_) => {
            let _ = device::clear_pending_binding();
            let pending = device::bind_init(
                &config.cloud_http_url,
                &config.machine_id,
                replacement.as_ref(),
            )
            .await?;
            device::save_pending_binding(&pending)?;
            let _ = device::clear_legacy_binding_replacement();
            pending
        }
        None => {
            let pending = device::bind_init(
                &config.cloud_http_url,
                &config.machine_id,
                replacement.as_ref(),
            )
            .await?;
            device::save_pending_binding(&pending)?;
            let _ = device::clear_legacy_binding_replacement();
            pending
        }
    };

    // ── Step 2: 显示绑定框 ──
    let qr_expires_in = pending
        .qr_expires_at_ms
        .saturating_sub(now_ms())
        .saturating_add(999)
        / 1_000;
    display_bind_box(&pending.approval_code, &config.hostname, qr_expires_in);

    // ── Step 3: 轮询绑定结果 ──
    eprintln!("[kn-agent] 等待 iOS App 确认绑定...");
    let token = device::bind_poll(&config.cloud_http_url, &pending, shutdown.clone()).await?;

    // ── Step 4: 保存 token ──
    let activation = device::PendingActivation {
        pairing_id: pending.pairing_id.clone(),
        poll_secret: pending.poll_secret.clone(),
        machine_id: config.machine_id.clone(),
        device_token: token.clone(),
        previous_device_token: device::load_device_token(),
        pairing_expires_at_ms: pending.pairing_expires_at_ms,
    };
    device::save_pending_activation(&activation)?;
    device::save_device_token(&token)?;
    finish_activation(&config, &activation, shutdown).await?;

    // There is intentionally no WSS process in this command.  The durable
    // marker/token flow is complete; the normal daemon will establish WSS.
    println!("\n设备绑定已确认。启动 kn-agent 守护进程后将自动上线。");
    Ok(())
}

fn shutdown_on_ctrl_c() -> CancellationToken {
    let shutdown = CancellationToken::new();
    let shutdown_clone = shutdown.clone();
    tokio::spawn(async move {
        tokio::signal::ctrl_c().await.ok();
        shutdown_clone.cancel();
    });
    shutdown
}

/// Returns true when a daemon accepted ownership of this request.  A stale
/// socket is reported rather than bypassed: starting a second worker would
/// reintroduce the exact marker/WSS ownership race this protocol avoids.
async fn delegate_to_running_daemon(config: &AgentConfig) -> Result<bool> {
    if !config.ipc_socket_path.exists() {
        return Ok(false);
    }
    let stream = tokio::time::timeout(
        std::time::Duration::from_secs(2),
        UnixStream::connect(&config.ipc_socket_path),
    )
    .await
    .map_err(|_| AgentError::Other("检测到正在运行的 Agent，但 IPC 连接超时".into()))?
    .map_err(|error| AgentError::Other(format!("检测到 Agent IPC 但无法连接: {error}")))?;
    let request = serde_json::json!({
        "id": "kn-agent-bind-cli",
        "method": "bindStartOrResume",
        "params": {}
    });
    let mut line = serde_json::to_string(&request)?;
    line.push('\n');
    let (reader, mut writer) = stream.into_split();
    writer
        .write_all(line.as_bytes())
        .await
        .map_err(AgentError::Io)?;
    writer.shutdown().await.map_err(AgentError::Io)?;
    let mut response = String::new();
    BufReader::new(reader)
        .read_line(&mut response)
        .await
        .map_err(AgentError::Io)?;
    let value: serde_json::Value = serde_json::from_str(&response)?;
    if let Some(error) = value.get("error") {
        return Err(AgentError::Other(format!(
            "运行中的 Agent 未接受绑定请求: {}",
            error
                .get("message")
                .and_then(|message| message.as_str())
                .unwrap_or("未知错误")
        )));
    }
    println!("已将绑定请求交给正在运行的 kn-agent；它将负责确认并建立连接。");
    Ok(true)
}

/// Retain the marker on every ambiguous failure.  The next CLI invocation or
/// daemon startup retries the same idempotent request, including after the
/// Redis pairing TTL has elapsed.  A terminal Cloud response is safe to roll
/// back only because Cloud first probes MySQL by machineId + deviceToken.
async fn finish_activation(
    config: &AgentConfig,
    activation: &device::PendingActivation,
    shutdown: CancellationToken,
) -> Result<()> {
    let mut activated = false;
    loop {
        if activated {
            if let Err(error) = device::clear_pending_binding() {
                eprintln!("[kn-agent] 正式绑定已完成，但本地收尾未完成，将重试: {error}");
                tokio::select! {
                    _ = shutdown.cancelled() => return Err(AgentError::Shutdown),
                    _ = tokio::time::sleep(std::time::Duration::from_secs(30)) => continue,
                }
            }
            if let Err(error) = device::clear_pending_activation() {
                eprintln!("[kn-agent] 正式绑定已完成，但本地收尾未完成，将重试: {error}");
                tokio::select! {
                    _ = shutdown.cancelled() => return Err(AgentError::Shutdown),
                    _ = tokio::time::sleep(std::time::Duration::from_secs(30)) => continue,
                }
            }
            return Ok(());
        }
        tokio::select! {
            _ = shutdown.cancelled() => return Err(AgentError::Shutdown),
            result = device::bind_activate(&config.cloud_http_url, activation) => match result {
                Ok(_) => {
                    activated = true;
                }
                Err(error) if device::is_terminal_bind_activation_error(&error) => {
                    let _ = device::rollback_unactivated_token(activation);
                    let _ = device::clear_pending_activation();
                    let _ = device::clear_pending_binding();
                    return Err(error);
                }
                Err(error) => {
                    eprintln!("[kn-agent] Cloud 激活结果未确认，将在 30 秒后重试: {error}");
                    tokio::select! {
                        _ = shutdown.cancelled() => return Err(AgentError::Shutdown),
                        _ = tokio::time::sleep(std::time::Duration::from_secs(30)) => {}
                    }
                }
            }
        }
    }
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis().try_into().unwrap_or(u64::MAX))
        .unwrap_or(u64::MAX)
}

// ── Display helpers ───────────────────────────────────────────

/// 在终端输出 ASCII 绑定码展示框。
fn display_bind_box(bind_code: &str, hostname: &str, expires_in_secs: u64) {
    let mins = expires_in_secs / 60;
    let secs = expires_in_secs % 60;
    let validity = if secs == 0 {
        format!("{} 分钟", mins)
    } else {
        format!("{} 分 {} 秒", mins, secs)
    };

    let code_line = pad_box_line(&format!("绑定码: {}", bind_code));
    let host_line = pad_box_line(&format!("主机名: {}", hostname));
    let validity_line = pad_box_line(&format!("有效期: {}", validity));

    println!();
    println!("╔══════════════════════════════════╗");
    println!("{}", pad_box_line_center("📱 kn 设备绑定"));
    println!("{}", empty_box_line());
    println!("{}", code_line);
    println!("{}", host_line);
    println!("{}", empty_box_line());
    println!("{}", pad_box_line("请用 kn iOS App"));
    println!("{}", pad_box_line("输入以上绑定码完成绑定"));
    println!("{}", validity_line);
    println!("╚══════════════════════════════════╝");
    println!();
}

/// 内容左对齐 + 右侧空格填充到 34 列（内部宽度）。
fn pad_box_line(content: &str) -> String {
    let inner_width: usize = 34;
    let display_w = display_width(content);
    let right_pad = inner_width.saturating_sub(display_w);
    format!("║{}{}║", content, " ".repeat(right_pad))
}

/// 内容居中到 34 列（内部宽度）。
fn pad_box_line_center(content: &str) -> String {
    let inner_width: usize = 34;
    let display_w = display_width(content);
    let left_pad = inner_width.saturating_sub(display_w) / 2;
    let right_pad = inner_width.saturating_sub(display_w.saturating_add(left_pad));
    format!(
        "║{}{}{}║",
        " ".repeat(left_pad),
        content,
        " ".repeat(right_pad)
    )
}

fn empty_box_line() -> String {
    "║                                  ║".to_string()
}

/// 粗略估算终端列宽：ASCII 字符为 1，非 ASCII（CJK、emoji 等）为 2。
fn display_width(s: &str) -> usize {
    s.chars().map(|c| if c as u32 > 0x7F { 2 } else { 1 }).sum()
}
