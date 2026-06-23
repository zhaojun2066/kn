//! 设备绑定流程 — HTTP bind-init → 轮询 bind-result → 原子保存 device_token。
//!
//! 绑定流程:
//! 1. POST /api/v1/device/bind-init → 获取 6 位绑定码
//! 2. 打印绑定码（Phase 1 stderr；Phase 2 通过 IPC 发给 Desktop 显示为 QR 码）
//! 3. 轮询 GET /api/v1/device/bind-result?code=xxx（每 2s，最多 5 分钟）
//! 4. 收到 device_token → 原子保存到 ~/.kn/agent/device_token (0600)

use crate::error::{AgentError, Result};
use std::path::PathBuf;
use std::sync::OnceLock;
use std::time::Duration;
use tokio_util::sync::CancellationToken;

// ── Shared HTTP client ─────────────────────────────────────
// Reuse a single reqwest::Client for connection pooling across
// all HTTP calls (bind_init, bind_poll, redeem).

fn http() -> &'static reqwest::Client {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .expect("build reqwest client")
    })
}

// ── Cloud API 响应信封 ─────────────────────────────────────

/// kn-cloud 统一响应格式 `ApiResponse<T>`。
/// 所有 HTTP API 都包装在这一层里：`{"code":0,"message":"ok","data":{...}}`。
#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct CloudEnvelope<T> {
    #[allow(dead_code)]
    code: i32,
    #[allow(dead_code)]
    #[serde(default)]
    message: Option<String>,
    data: Option<T>,
}

impl<T> CloudEnvelope<T> {
    /// 提取 `data` 字段，若 `code != 0` 则返回服务端错误信息。
    fn into_data(self) -> crate::error::Result<T> {
        if self.code != 0 {
            return Err(crate::error::AgentError::Protocol(
                self.message
                    .filter(|m| !m.is_empty())
                    .unwrap_or_else(|| format!("云端错误 code={}", self.code)),
            ));
        }
        self.data
            .ok_or_else(|| crate::error::AgentError::Protocol("云端响应缺少 data 字段".into()))
    }
}

// ── API 响应类型（camelCase，与 Cloud Java records 对齐）───

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct BindInitData {
    bind_code: String,
    expires_in: u64,
    #[serde(default)]
    confirm_url: String,
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct BindResultData {
    #[serde(default)]
    device_token: Option<String>,
    #[serde(default)]
    status: Option<String>,
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct RedeemResponseData {
    plan: String,
    days: i32,
}

// ── Public API ──────────────────────────────────────────────

/// 第一步：向云端请求绑定码。返回 `(bind_code, expires_in_secs, confirm_url)`。
///
/// POST /api/v1/device/bind-init
pub async fn bind_init(http_url: &str, machine_id: &str) -> Result<(String, u64, String)> {
    tracing::info!("正在初始化设备绑定...");
    let envelope: CloudEnvelope<BindInitData> = http()
        .post(format!("{}/api/v1/device/bind-init", http_url))
        .json(&serde_json::json!({"machineId": machine_id}))
        .send()
        .await
        .map_err(|e| AgentError::Http(e))?
        .error_for_status()
        .map_err(|e| AgentError::Http(e))?
        .json()
        .await
        .map_err(|e| AgentError::Http(e))?;

    let data = envelope.into_data()?;
    let bind_code = data.bind_code;
    let expires_in = data.expires_in.min(300); // 最多 5 分钟
    let confirm_url = data.confirm_url;
    tracing::info!(
        "绑定码: {} ({} 秒内有效), confirmUrl: {}",
        bind_code,
        expires_in,
        confirm_url,
    );

    Ok((bind_code, expires_in, confirm_url))
}

/// 第二步：轮询绑定结果，返回 device_token。
///
/// GET /api/v1/device/bind-result?code=xxx（每 2s，最多 expires_in 秒）
pub async fn bind_poll(
    http_url: &str,
    bind_code: &str,
    expires_in: u64,
    shutdown: CancellationToken,
) -> Result<String> {
    let poll_interval = Duration::from_secs(2);
    let poll_timeout = Duration::from_secs(expires_in);

    let result = tokio::time::timeout(poll_timeout, async {
        let mut interval = tokio::time::interval(poll_interval);
        // 跳过第一个立即触发
        interval.tick().await;

        loop {
            tokio::select! {
                _ = shutdown.cancelled() => {
                    return Err(AgentError::Shutdown);
                }
                _ = interval.tick() => {
                    match http()
                        .get(format!("{}/api/v1/device/bind-result", http_url))
                        .query(&[("code", &bind_code)])
                        .send()
                        .await
                    {
                        Ok(resp) => {
                            if resp.status().is_success() {
                                match resp.json::<CloudEnvelope<BindResultData>>().await {
                                    Ok(envelope) => {
                                        if let Ok(data) = envelope.into_data() {
                                            if let Some(token) = data.device_token {
                                                if !token.is_empty() {
                                                    tracing::info!("绑定成功！收到 device_token");
                                                    return Ok(token);
                                                }
                                            }
                                            // status = "pending" — 继续等待
                                        }
                                    }
                                    Err(_) => continue,
                                }
                            }
                        }
                        Err(_) => continue,
                    }
                }
            }
        }
    })
    .await;

    match result {
        Ok(Ok(token)) => Ok(token),
        Ok(Err(e)) => Err(e),
        Err(_elapsed) => Err(AgentError::Timeout("绑定超时".into())),
    }
}

/// 执行完整设备绑定流程（bind_init → bind_poll → save）。返回 device_token。
///
/// 通过 HTTP 与 kn-cloud API 通信:
/// - `POST /api/v1/device/bind-init` — 初始化绑定
/// - `GET /api/v1/device/bind-result?code=xxx` — 轮询结果
pub async fn bind_device(
    http_url: &str,
    machine_id: &str,
    shutdown: CancellationToken,
) -> Result<String> {
    let (bind_code, expires_in, _confirm_url) = bind_init(http_url, machine_id).await?;
    let token = bind_poll(http_url, &bind_code, expires_in, shutdown).await?;
    save_device_token(&token)?;
    Ok(token)
}

/// 卡密兑换：用 device_token 鉴权，向云端兑换卡密。
///
/// POST /api/v1/device/redeem
/// Authorization: Bearer <device_token>
/// Body: {"code": "KN-..."}
pub async fn redeem(http_url: &str, device_token: &str, code: &str) -> Result<(String, i32)> {
    tracing::info!("正在兑换卡密...");

    let response = http()
        .post(format!("{}/api/v1/device/redeem", http_url))
        .header("Authorization", format!("Bearer {}", device_token))
        .json(&serde_json::json!({"code": code}))
        .send()
        .await
        .map_err(|e| AgentError::Http(e))?;

    // Try to parse the JSON body for structured error info, regardless
    // of HTTP status code. kn-cloud returns business errors (code≠0, message)
    // with HTTP 200, and auth errors with HTTP 401 + JSON body.
    let status = response.status();
    let body_bytes = response
        .bytes()
        .await
        .map_err(|e| AgentError::Http(e))?;

    let envelope: CloudEnvelope<RedeemResponseData> =
        serde_json::from_slice(&body_bytes).map_err(|_e| {
            // If JSON parsing fails, include HTTP status in the error
            AgentError::Protocol(format!(
                "服务器响应异常 (HTTP {}): {}",
                status.as_u16(),
                String::from_utf8_lossy(&body_bytes).trim()
            ))
        })?;

    // Try to extract structured error info from the JSON envelope.
    // kn-cloud returns business errors (code≠0) with HTTP 200, and
    // auth errors (401) with a JSON body. Parse first, then decide.
    if envelope.code != 0 {
        let msg = envelope
            .message
            .filter(|m| !m.is_empty())
            .unwrap_or_else(|| format!("云端错误 code={}", envelope.code));
        if status.is_client_error() || status.is_server_error() {
            return Err(AgentError::Protocol(format!(
                "HTTP {} — {}",
                status.as_u16(),
                msg
            )));
        }
        return Err(AgentError::Protocol(msg));
    }

    let data = envelope
        .data
        .ok_or_else(|| AgentError::Protocol("云端响应缺少 data 字段".into()))?;

    tracing::info!("兑换成功: plan={}, days={}", data.plan, data.days);
    Ok((data.plan, data.days))
}

/// 从磁盘加载 device_token。无 token 时返回 None。
pub fn load_device_token() -> Option<String> {
    let path = device_token_path();
    std::fs::read_to_string(&path)
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

// ── Helpers ─────────────────────────────────────────────────

fn device_token_path() -> PathBuf {
    kn_common::path::config_dir().join("agent").join("device_token")
}

/// 原子保存 device_token（tmp → fsync → rename，权限 0600）。
pub(crate) fn save_device_token(token: &str) -> Result<()> {
    let path = device_token_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| AgentError::Io(e))?;
    }

    let tmp = path.with_extension("tmp");

    // 写入临时文件
    std::fs::write(&tmp, token).map_err(|e| AgentError::Io(e))?;

    // 设置权限 0600
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600)).ok();
    }

    // fsync
    if let Ok(f) = std::fs::File::open(&tmp) {
        let _ = f.sync_all();
    }

    // 原子 rename。失败时清理临时文件（敏感凭据不应残留）
    if let Err(e) = std::fs::rename(&tmp, &path) {
        let _ = std::fs::remove_file(&tmp);
        return Err(AgentError::Io(e));
    }

    tracing::info!("device_token 已保存到 {}", path.display());
    Ok(())
}

/// 删除本地 device_token 文件（在 token 失效/吊销时调用）。
/// 使用原子 rename→delete 策略避免部分读取。
pub fn delete_device_token() {
    let path = device_token_path();
    if path.exists() {
        let tmp = path.with_extension("deleting");
        // 先将文件移走（原子操作），防止其他读取者读到部分内容
        if std::fs::rename(&path, &tmp).is_ok() {
            let _ = std::fs::remove_file(&tmp);
            tracing::info!("device_token 已删除 (token 失效)");
        } else {
            // rename 失败时直接删除
            let _ = std::fs::remove_file(&path);
            tracing::warn!("device_token 删除失败，已尝试直接删除");
        }
    }
}

// ── Tests ───────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // NOTE: These tests set the process-global KN_HOME environment variable.
    // Since KN_HOME is shared across all test threads, these tests MUST run
    // serially. Run with: cargo test --package kn-agent -- --test-threads=1
    // (Same constraint as state.rs tests.)

    #[test]
    fn test_save_and_load_device_token() {
        let dir = std::env::temp_dir().join(format!("kn-test-device-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        // KN_HOME is process-global — must run with --test-threads=1 (see module docs)
        std::env::set_var("KN_HOME", dir.to_str().unwrap());

        let token = "test-device-token-12345";
        save_device_token(token).unwrap();
        let loaded = load_device_token().unwrap();
        assert_eq!(loaded, token);

        std::env::remove_var("KN_HOME");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn test_load_device_token_not_found() {
        let dir = std::env::temp_dir().join(format!("kn-test-device-empty-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        // KN_HOME is process-global — must run with --test-threads=1 (see module docs)
        std::env::set_var("KN_HOME", dir.to_str().unwrap());

        assert!(load_device_token().is_none());

        std::env::remove_var("KN_HOME");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn test_delete_device_token() {
        let dir = std::env::temp_dir().join(format!("kn-test-device-del-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        // KN_HOME is process-global — must run with --test-threads=1 (see module docs)
        std::env::set_var("KN_HOME", dir.to_str().unwrap());

        let token = "test-token-to-delete";
        save_device_token(token).unwrap();
        assert!(load_device_token().is_some());

        delete_device_token();
        assert!(load_device_token().is_none());

        // Calling delete again is a no-op (no crash)
        delete_device_token();
        assert!(load_device_token().is_none());

        std::env::remove_var("KN_HOME");
        let _ = std::fs::remove_dir_all(dir);
    }
}
