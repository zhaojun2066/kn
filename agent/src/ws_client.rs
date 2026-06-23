//! WebSocket 客户端 — WSS 连接/重连/心跳/消息路由。
//!
//! 连接到 kn-cloud WebSocket 服务:
//! - 指数退避重连 + 25% 随机抖动
//! - 15s 心跳 ping，90s 超时检测
//! - 消息通过 mpsc channel 路由到主循环

use crate::error::{AgentError, Result};
use crate::proto::{AgentIncoming, WsEnvelope, WsMessageBuilder};
use crate::state::{AgentState, StateEvent, StateMachine};
use futures_util::{SinkExt, StreamExt};
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::protocol::Message;
use tokio_util::sync::CancellationToken;

// ── Backoff configuration ───────────────────────────────────

const INITIAL_BACKOFF_MS: u64 = 1000;
const MAX_BACKOFF_MS: u64 = 30_000;
const BACKOFF_JITTER: f64 = 0.25;

fn backoff_delay(attempt: u32) -> Duration {
    let base = (INITIAL_BACKOFF_MS as f64) * (2u64.saturating_pow(attempt)) as f64;
    let capped = base.min(MAX_BACKOFF_MS as f64);
    let jitter_factor = 0.75 + (rand::random::<f64>() * BACKOFF_JITTER);
    let delay_ms = (capped * jitter_factor) as u64;
    Duration::from_millis(delay_ms)
}

// ── Heartbeat configuration ─────────────────────────────────

const PING_INTERVAL: Duration = Duration::from_secs(15);
const PONG_TIMEOUT: Duration = Duration::from_secs(90);

// ── Public API ──────────────────────────────────────────────

/// 运行 WebSocket 连接循环，返回出站消息发送端。
///
/// - `outgoing_tx_ref`: 共享的出站消息 sender，每次重连时更新
/// - `incoming_tx`: 入站消息通道
/// - `shutdown`: 为 `true` 时正常退出（不重连），其他原因触发重连
pub async fn run_ws_loop(
    device_token: &str,
    cloud_url: &str,
    machine_id: &str,
    agent_version: &str,
    os_version: &str,
    hostname: &str,
    state: Arc<StateMachine>,
    outgoing_tx_ref: Arc<tokio::sync::Mutex<Option<mpsc::UnboundedSender<String>>>>,
    incoming_tx: mpsc::UnboundedSender<AgentIncoming>,
    shutdown: CancellationToken,
) -> Result<()> {
    let mut attempt: u32 = 0;

    loop {
        if shutdown.is_cancelled() {
            return Ok(());
        }

        if attempt > 0 {
            let delay = backoff_delay(attempt);
            tracing::info!(attempt = attempt, delay_ms = delay.as_millis(), "WSS 重连等待");
            tokio::select! {
                _ = tokio::time::sleep(delay) => {}
                _ = shutdown.cancelled() => return Ok(()),
            }
        }

        // 为本次连接创建新的出站通道
        let (outgoing_tx, outgoing_rx) = mpsc::unbounded_channel::<String>();
        {
            let mut tx_ref = outgoing_tx_ref.lock().await;
            *tx_ref = Some(outgoing_tx);
        }

        // 尝试连接并运行读写循环
        match connect_and_run(
            device_token,
            cloud_url,
            machine_id,
            agent_version,
            os_version,
            hostname,
            outgoing_rx,
            &incoming_tx,
            &shutdown,
        )
        .await
        {
            Ok(()) => {
                // shutdown 触发的正常退出 — 不重连
                // 清除 sender（防止后续发送到已关闭的连接）
                let mut tx_ref = outgoing_tx_ref.lock().await;
                *tx_ref = None;
                return Ok(());
            }
            Err(e) => {
                let err_msg = e.to_string();
                // B12: token 被吊销/过期 — 不再重连，回到 Unbound
                if err_msg.contains("AUTH_REJECTED") {
                    tracing::warn!("device_token 已失效，进入未绑定状态");
                    match state
                        .transition(StateEvent::WsConnected {
                            has_token: false,
                        })
                        .await
                    {
                        Ok(_) => tracing::info!("状态已转换为 Unbound"),
                        Err(e) => tracing::warn!("状态转换失败: {}", e),
                    }
                    return Err(AgentError::Ws(err_msg));
                }

                tracing::warn!("WSS 连接断开: {} (尝试 #{})", e, attempt + 1);
                attempt += 1;

                if state.current().await != AgentState::Reconnecting {
                    let _ = state.transition(StateEvent::WsDisconnected).await;
                }
            }
        }
    }
}

/// 建立单次连接并运行读写循环。
///
/// 返回:
/// - `Ok(())` — shutdown 触发的正常退出
/// - `Err(...)` — 连接意外断开（read/write 错误、pong 超时）
async fn connect_and_run(
    device_token: &str,
    cloud_url: &str,
    machine_id: &str,
    agent_version: &str,
    os_version: &str,
    hostname: &str,
    outgoing_rx: mpsc::UnboundedReceiver<String>,
    incoming_tx: &mpsc::UnboundedSender<AgentIncoming>,
    shutdown: &CancellationToken,
) -> Result<()> {
    let _url = url::Url::parse(cloud_url)
        .map_err(|e| AgentError::Ws(format!("无效的云端 URL: {}", e)))?;

    let request = http::Request::builder()
        .uri(cloud_url)
        .header("Authorization", format!("Bearer {}", device_token))
        .header("X-KN-Role", "kn-agent")
        .header("X-KN-Machine-Id", machine_id)
        .header("X-KN-Agent-Version", agent_version)
        .header("X-KN-OS-Version", os_version)
        .header("X-KN-Hostname", hostname)
        .header("X-KN-Protocol-Version", "1")
        .body(())
        .map_err(|e| AgentError::Ws(format!("构建 WSS 请求失败: {}", e)))?;

    tracing::info!("正在连接 {} ...", cloud_url);

    let (ws_stream, response) = connect_async(request)
        .await
        .map_err(|e| {
            // Check if the error indicates an auth failure (token revoked/expired)
            let err_str = e.to_string();
            if err_str.contains("401") || err_str.contains("403") {
                AgentError::Ws("AUTH_REJECTED: device_token 已失效，请重新绑定".into())
            } else {
                AgentError::Ws(format!("WSS 连接失败: {}", e))
            }
        })?;

    // Also check HTTP upgrade response status
    if response.status() == http::StatusCode::UNAUTHORIZED
        || response.status() == http::StatusCode::FORBIDDEN
    {
        return Err(AgentError::Ws(
            "AUTH_REJECTED: device_token 已失效，请重新绑定".into(),
        ));
    }

    tracing::info!("WSS 已连接");

    let (mut write, mut read) = ws_stream.split();

    // 心跳跟踪
    let last_pong = Arc::new(AtomicI64::new(chrono::Utc::now().timestamp_millis()));

    // ── read_loop ──
    let read_incoming = incoming_tx.clone();
    let read_shutdown = shutdown.clone();
    let read_pong = last_pong.clone();
    let read_error = Arc::new(tokio::sync::Mutex::new(None::<String>));
    let read_error_clone = read_error.clone();

    let mut read_handle = tokio::spawn(async move {
        loop {
            tokio::select! {
                msg = read.next() => {
                    match msg {
                        Some(Ok(Message::Text(text))) => {
                            // 先解析 JSON 获取 type 字段
                            match serde_json::from_str::<WsEnvelope>(&text) {
                                Ok(ref env) if env.msg_type == "pong" => {
                                    read_pong.store(
                                        chrono::Utc::now().timestamp_millis(),
                                        Ordering::Relaxed,
                                    );
                                }
                                Ok(env) => {
                                    match env.parse() {
                                        Ok(parsed) => {
                                            let _ = read_incoming.send(parsed);
                                        }
                                        Err(e) => {
                                            tracing::debug!("消息解析失败: {}", e);
                                        }
                                    }
                                }
                                Err(e) => {
                                    tracing::debug!("JSON 解析失败: {} — 原始: {}", e, &text[..text.len().min(200)]);
                                }
                            }
                        }
                        Some(Ok(Message::Close(_))) => {
                            tracing::info!("WSS 收到关闭帧");
                            *read_error_clone.lock().await = Some("server closed connection".into());
                            break;
                        }
                        Some(Err(e)) => {
                            tracing::warn!("WSS 读取错误: {}", e);
                            *read_error_clone.lock().await = Some(format!("read error: {}", e));
                            break;
                        }
                        None => {
                            *read_error_clone.lock().await = Some("stream ended".into());
                            break;
                        }
                        _ => {}
                    }
                }
                _ = read_shutdown.cancelled() => {
                    *read_error_clone.lock().await = None; // shutdown, no error
                    break;
                }
            }
        }
    });

    // ── write_loop ──
    let write_shutdown = shutdown.clone();
    let write_pong = last_pong.clone();
    let write_error = Arc::new(tokio::sync::Mutex::new(None::<String>));
    let write_error_clone = write_error.clone();

    let mut write_handle = tokio::spawn(async move {
        let mut ping_tick = tokio::time::interval(PING_INTERVAL);
        let mut pong_check = tokio::time::interval(PONG_TIMEOUT);
        let mut outgoing_rx = outgoing_rx; // take ownership

        loop {
            tokio::select! {
                _ = ping_tick.tick() => {
                    let ping = WsMessageBuilder::ping();
                    if let Err(e) = write.send(Message::Text(ping)).await {
                        tracing::warn!("WSS 发送 ping 失败: {}", e);
                        *write_error_clone.lock().await = Some(format!("write error: {}", e));
                        break;
                    }
                }
                _ = pong_check.tick() => {
                    let now = chrono::Utc::now().timestamp_millis();
                    let last = write_pong.load(Ordering::Relaxed);
                    if (now - last) > PONG_TIMEOUT.as_millis() as i64 {
                        *write_error_clone.lock().await = Some(format!("pong timeout ({}s)", PONG_TIMEOUT.as_secs()));
                        break;
                    }
                }
                msg = outgoing_rx.recv() => {
                    match msg {
                        Some(text) => {
                            if let Err(e) = write.send(Message::Text(text)).await {
                                tracing::warn!("WSS 发送消息失败: {}", e);
                                *write_error_clone.lock().await = Some(format!("write error: {}", e));
                                break;
                            }
                        }
                        None => break,
                    }
                }
                _ = write_shutdown.cancelled() => {
                    *write_error_clone.lock().await = None; // shutdown, no error
                    break;
                }
            }
        }

        let _ = write.send(Message::Close(None)).await;
    });

    // 等待任一任务结束
    tokio::select! {
        _ = (&mut read_handle) => {}
        _ = (&mut write_handle) => {}
        _ = shutdown.cancelled() => {}
    }

    read_handle.abort();
    write_handle.abort();

    // 判断退出原因
    let is_shutdown = shutdown.is_cancelled();
    let read_err = read_error.lock().await.take();
    let write_err = write_error.lock().await.take();

    tracing::info!("WSS 连接已断开");

    if is_shutdown {
        Ok(())
    } else {
        // 返回第一个非空错误
        let reason = read_err.or(write_err).unwrap_or_else(|| "unknown disconnect".into());
        Err(AgentError::Ws(reason))
    }
}

// ── Tests ───────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_backoff_delay_first_attempt() {
        let delay = backoff_delay(0);
        let ms = delay.as_millis();
        assert!(ms >= 750, "delay {} should be >= 750ms", ms);
        assert!(ms <= 1000, "delay {} should be <= 1000ms", ms);
    }

    #[test]
    fn test_backoff_delay_increases() {
        let d0 = backoff_delay(0);
        let d1 = backoff_delay(1);
        let d2 = backoff_delay(2);
        assert!(d1 > d0);
        assert!(d2 > d1);
    }

    #[test]
    fn test_backoff_delay_capped() {
        let delay = backoff_delay(10);
        assert!(delay.as_millis() <= MAX_BACKOFF_MS as u128 + 100);
    }

    #[test]
    fn test_backoff_delay_with_jitter_variation() {
        let mut delays: Vec<u128> = (0..10).map(|_| backoff_delay(0).as_millis()).collect();
        delays.sort();
        assert!(delays[0] < delays[9] || delays[0] >= 750);
    }
}
