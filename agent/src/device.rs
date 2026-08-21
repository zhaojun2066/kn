//! 设备绑定流程 — HTTP bind-init → 轮询 bind-result → 原子保存 device_token。
//!
//! 绑定流程:
//! 1. POST /api/v1/device/bind-init → 获取 6 位绑定码
//! 2. 打印绑定码（Phase 1 stderr；Phase 2 通过 IPC 发给 Desktop 显示为 QR 码）
//! 3. 轮询 GET /api/v1/device/bind-result?code=xxx（每 2s，最多 5 分钟）
//! 4. 收到 device_token → 原子保存到 ~/.kn/agent/device_token (0600)

use crate::error::{AgentError, Result};
use std::io::Write;
use std::path::PathBuf;
use std::sync::OnceLock;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
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

/// Translate Cloud's stable business codes into recovery semantics.  Bind
/// activation is special: a transport/server failure is ambiguous and must
/// retain the local marker, whereas a rejected authorization/limit request is
/// safe to stop after the caller has made its idempotent final probe.
fn classify_bind_activation_error(code: i32, message: impl Into<String>) -> AgentError {
    let message = message.into();
    match code {
        // Common client errors plus all pairing/device business rejections are
        // final for this exact pairing/token.  429 and 5xx remain retryable.
        400 | 401 | 403 | 404 | 2001..=2006 | 3001 => AgentError::BindActivationTerminal(message),
        _ => AgentError::BindActivationRetryable(message),
    }
}

// ── API 响应类型（camelCase，与 Cloud Java records 对齐）───

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct BindInitData {
    pairing_id: String,
    approval_code: String,
    poll_secret: String,
    confirm_url: String,
    qr_expires_in: u64,
    pairing_expires_in: u64,
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct BindResultData {
    #[serde(default)]
    device_token: Option<String>,
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    device_id: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingBinding {
    pub pairing_id: String,
    pub approval_code: String,
    pub poll_secret: String,
    pub confirm_url: String,
    pub qr_expires_at_ms: u64,
    pub pairing_expires_at_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingActivation {
    pub pairing_id: String,
    pub poll_secret: String,
    pub machine_id: String,
    pub device_token: String,
    /// Token that existed before this unactivated pairing overwrote the normal
    /// token file. It is restored if activation can never complete.
    #[serde(default)]
    pub previous_device_token: Option<String>,
    pub pairing_expires_at_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BindActivation {
    pub status: String,
    pub device_id: Option<u64>,
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct RedeemResponseData {
    plan: String,
    days: i32,
}

// ── Public API ──────────────────────────────────────────────

/// 向云端创建待 Agent 确认的配对申请。凭证只在 Agent 本机落盘后才可激活。
pub async fn bind_init(http_url: &str, machine_id: &str) -> Result<PendingBinding> {
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
    let now_ms = now_ms()?;
    let pending = PendingBinding {
        pairing_id: data.pairing_id,
        approval_code: data.approval_code,
        poll_secret: data.poll_secret,
        confirm_url: data.confirm_url,
        qr_expires_at_ms: now_ms.saturating_add(data.qr_expires_in.saturating_mul(1_000)),
        pairing_expires_at_ms: now_ms.saturating_add(data.pairing_expires_in.saturating_mul(1_000)),
    };
    tracing::info!(
        pairing_id = %pending.pairing_id,
        qr_expires_in_secs = data.qr_expires_in,
        "已创建待确认设备绑定申请"
    );
    Ok(pending)
}

/// 轮询手机确认后的临时凭证。二维码过期不会终止轮询；配对申请到期才会结束。
pub async fn bind_poll(
    http_url: &str,
    pending: &PendingBinding,
    shutdown: CancellationToken,
) -> Result<String> {
    let mut waited = Duration::ZERO;
    loop {
        if now_ms()? >= pending.pairing_expires_at_ms {
            return Err(AgentError::Timeout("绑定申请已过期".into()));
        }
        let interval = if waited < Duration::from_secs(300) {
            Duration::from_secs(2)
        } else {
            Duration::from_secs(30)
        };
        tokio::select! {
            _ = shutdown.cancelled() => return Err(AgentError::Shutdown),
            _ = tokio::time::sleep(interval) => {}
        }
        waited = waited.saturating_add(interval);
        let response = http()
            .get(format!("{}/api/v1/device/bind-result", http_url))
            .query(&[
                ("pairingId", pending.pairing_id.as_str()),
                ("pollSecret", pending.poll_secret.as_str()),
            ])
            .send()
            .await;
        let Ok(response) = response else {
            continue;
        };
        if !response.status().is_success() {
            continue;
        }
        let Ok(envelope) = response.json::<CloudEnvelope<BindResultData>>().await else {
            continue;
        };
        let Ok(data) = envelope.into_data() else {
            continue;
        };
        if let Some(token) = data.device_token.filter(|token| !token.is_empty()) {
            tracing::info!(pairing_id = %pending.pairing_id, "手机已授权设备绑定，收到待激活凭证");
            return Ok(token);
        }
        match data.status.as_deref() {
            Some("rejected") | Some("cancelled") | Some("expired") => {
                return Err(AgentError::Protocol(format!(
                    "绑定申请已{}",
                    data.status.unwrap_or_default()
                )));
            }
            _ => {}
        }
    }
}

/// Cancel is allowed only before the phone confirms the pairing. This one-shot
/// probe closes the race where the phone confirms just before Desktop clicks close.
pub async fn bind_has_phone_confirmation(http_url: &str, pending: &PendingBinding) -> Result<bool> {
    let envelope: CloudEnvelope<BindResultData> = http()
        .get(format!("{}/api/v1/device/bind-result", http_url))
        .query(&[
            ("pairingId", pending.pairing_id.as_str()),
            ("pollSecret", pending.poll_secret.as_str()),
        ])
        .send()
        .await
        .map_err(AgentError::Http)?
        .error_for_status()
        .map_err(AgentError::Http)?
        .json()
        .await
        .map_err(AgentError::Http)?;
    let data = envelope.into_data()?;
    if data.device_token.as_deref().is_some_and(|token| !token.is_empty()) {
        return Ok(true);
    }
    Ok(matches!(
        data.status.as_deref(),
        Some("waitingAgent") | Some("activating") | Some("active")
    ))
}

/// Agent 在本地凭证完成原子持久化后，才能调用此接口创建正式设备。
pub async fn bind_activate(
    http_url: &str,
    activation: &PendingActivation,
) -> Result<BindActivation> {
    let response = http()
        .post(format!("{}/api/v1/device/bind-activate", http_url))
        .json(&serde_json::json!({
            "pairingId": activation.pairing_id,
            "pollSecret": activation.poll_secret,
            "deviceToken": activation.device_token,
            "machineId": activation.machine_id,
        }))
        .send()
        .await
        .map_err(|error| AgentError::BindActivationRetryable(error.to_string()))?;
    let http_status = response.status();
    let body = response
        .bytes()
        .await
        .map_err(|error| AgentError::BindActivationRetryable(error.to_string()))?;
    let envelope: CloudEnvelope<BindResultData> = serde_json::from_slice(&body).map_err(|_| {
        AgentError::BindActivationRetryable(format!(
            "绑定服务返回了无法识别的响应 (HTTP {})",
            http_status.as_u16()
        ))
    })?;
    if envelope.code != 0 {
        return Err(classify_bind_activation_error(
            envelope.code,
            envelope
                .message
                .filter(|message| !message.is_empty())
                .unwrap_or_else(|| format!("云端错误 code={}", envelope.code)),
        ));
    }
    let data = envelope
        .data
        .ok_or_else(|| AgentError::BindActivationRetryable("云端激活响应缺少 data".into()))?;
    match data.status.as_deref() {
        Some("active") => Ok(BindActivation {
            status: "active".into(),
            device_id: data.device_id,
        }),
        Some("rejected") | Some("cancelled") | Some("expired") => Err(
            AgentError::BindActivationTerminal(data.status.unwrap_or_default()),
        ),
        Some(status) => Err(AgentError::BindActivationRetryable(format!(
            "绑定激活状态尚未完成: {status}"
        ))),
        None => Err(AgentError::BindActivationRetryable(
            "绑定激活响应缺少 status".into(),
        )),
    }
}

pub fn is_terminal_bind_activation_error(error: &AgentError) -> bool {
    matches!(error, AgentError::BindActivationTerminal(_))
}

/// 显式取消待确认申请。关闭桌面弹窗不会调用它。
pub async fn bind_cancel(http_url: &str, pending: &PendingBinding) -> Result<()> {
    let envelope: CloudEnvelope<serde_json::Value> = http()
        .post(format!("{}/api/v1/device/bind-cancel", http_url))
        .json(&serde_json::json!({
            "pairingId": pending.pairing_id,
            "pollSecret": pending.poll_secret,
        }))
        .send()
        .await
        .map_err(AgentError::Http)?
        .error_for_status()
        .map_err(AgentError::Http)?
        .json()
        .await
        .map_err(AgentError::Http)?;
    if envelope.code != 0 {
        return Err(AgentError::Protocol(
            envelope
                .message
                .unwrap_or_else(|| "Cloud 拒绝取消绑定".into()),
        ));
    }
    Ok(())
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
    let pending = bind_init(http_url, machine_id).await?;
    save_pending_binding(&pending)?;
    let token = bind_poll(http_url, &pending, shutdown).await?;
    let activation = PendingActivation {
        pairing_id: pending.pairing_id,
        poll_secret: pending.poll_secret,
        machine_id: machine_id.to_owned(),
        device_token: token.clone(),
        previous_device_token: load_device_token(),
        pairing_expires_at_ms: pending.pairing_expires_at_ms,
    };
    save_pending_activation(&activation)?;
    save_device_token(&token)?;
    bind_activate(http_url, &activation).await?;
    clear_pending_activation()?;
    clear_pending_binding()?;
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
    let body_bytes = response.bytes().await.map_err(|e| AgentError::Http(e))?;

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

/// Revoke the current machine binding before removing the local credential.
pub async fn self_unbind(http_url: &str, device_token: &str) -> Result<()> {
    let response = http()
        .post(format!("{}/api/v1/device/self-unbind", http_url))
        .header("Authorization", format!("Bearer {}", device_token))
        .send()
        .await
        .map_err(AgentError::Http)?;
    let status = response.status();
    let body = response.bytes().await.map_err(AgentError::Http)?;
    let envelope: CloudEnvelope<serde_json::Value> = serde_json::from_slice(&body)
        .map_err(|_| AgentError::Protocol(format!("self-unbind 响应无效 (HTTP {})", status)))?;
    envelope.into_data().map(|_| ())
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
    kn_common::path::config_dir()
        .join("agent")
        .join("device_token")
}

pub(crate) fn pending_binding_path() -> PathBuf {
    kn_common::path::config_dir()
        .join("agent")
        .join("pending_binding.json")
}

pub(crate) fn pending_activation_path() -> PathBuf {
    kn_common::path::config_dir()
        .join("agent")
        .join("pending_activation.json")
}

pub fn load_pending_binding() -> Option<PendingBinding> {
    read_secure_json(&pending_binding_path())
}

pub fn save_pending_binding(pending: &PendingBinding) -> Result<()> {
    write_secure_json(&pending_binding_path(), pending)
}

pub fn clear_pending_binding() -> Result<()> {
    clear_secure_file(&pending_binding_path())
}

pub fn load_pending_activation() -> Option<PendingActivation> {
    read_secure_json(&pending_activation_path())
}

pub fn save_pending_activation(pending: &PendingActivation) -> Result<()> {
    write_secure_json(&pending_activation_path(), pending)
}

pub fn clear_pending_activation() -> Result<()> {
    clear_secure_file(&pending_activation_path())
}

/// Undo the local token promotion only when it belongs to an unactivated pairing.
/// An existing formal token is restored verbatim; a new token is removed by exact
/// value match so a concurrently replaced token is never deleted.
pub fn rollback_unactivated_token(activation: &PendingActivation) -> Result<()> {
    if let Some(previous) = activation.previous_device_token.as_deref() {
        return save_device_token(previous);
    }
    let path = device_token_path();
    let current = std::fs::read_to_string(&path).ok();
    if current.as_deref().map(str::trim) != Some(activation.device_token.as_str()) {
        return Ok(());
    }
    let deleting = path.with_extension("unactivated");
    std::fs::rename(&path, &deleting).map_err(AgentError::Io)?;
    std::fs::remove_file(&deleting).map_err(AgentError::Io)?;
    sync_parent_directory(&path)
}

/// The WSS lifecycle must never use a token that has not been activated by Cloud.
pub fn wss_is_blocked_by_pending_activation() -> bool {
    pending_activation_path().exists()
}

fn read_secure_json<T: serde::de::DeserializeOwned>(path: &PathBuf) -> Option<T> {
    std::fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
}

fn write_secure_json<T: serde::Serialize>(path: &PathBuf, value: &T) -> Result<()> {
    let bytes = serde_json::to_vec(value)
        .map_err(|e| AgentError::Protocol(format!("绑定状态序列化失败: {e}")))?;
    write_secure_atomic(path, &bytes)
}

fn clear_secure_file(path: &PathBuf) -> Result<()> {
    match std::fs::remove_file(path) {
        Ok(()) => {
            sync_parent_directory(path)?;
            Ok(())
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(AgentError::Io(e)),
    }
}

fn write_secure_atomic(path: &PathBuf, bytes: &[u8]) -> Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| AgentError::Protocol("绑定状态缺少父目录".into()))?;
    std::fs::create_dir_all(parent).map_err(AgentError::Io)?;
    let tmp = path.with_extension("tmp");
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(&tmp)
        .map_err(AgentError::Io)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600))
            .map_err(AgentError::Io)?;
    }
    file.write_all(bytes).map_err(AgentError::Io)?;
    file.sync_all().map_err(AgentError::Io)?;
    drop(file);
    if let Err(e) = std::fs::rename(&tmp, path) {
        let _ = std::fs::remove_file(&tmp);
        return Err(AgentError::Io(e));
    }
    sync_parent_directory(path)
}

fn sync_parent_directory(path: &PathBuf) -> Result<()> {
    #[cfg(unix)]
    {
        let parent = path
            .parent()
            .ok_or_else(|| AgentError::Protocol("绑定状态缺少父目录".into()))?;
        std::fs::File::open(parent)
            .and_then(|dir| dir.sync_all())
            .map_err(AgentError::Io)?;
    }
    Ok(())
}

fn now_ms() -> Result<u64> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis().try_into().unwrap_or(u64::MAX))
        .map_err(|e| AgentError::Protocol(format!("系统时间异常: {e}")))
}

/// 原子保存 device_token（tmp → fsync → rename，权限 0600）。
pub(crate) fn save_device_token(token: &str) -> Result<()> {
    let path = device_token_path();
    write_secure_atomic(&path, token.as_bytes())?;

    tracing::info!("device_token 已保存到 {}", path.display());
    Ok(())
}

/// 返回生产环境 device_token 路径（使用 home_dir，忽略 KN_HOME）。
/// 用于安全检查：任何情况下都不能操作这个路径。
fn production_device_token_path() -> PathBuf {
    kn_common::path::home_dir()
        .join(".kn")
        .join("agent")
        .join("device_token")
}

/// 删除本地 device_token 文件（在 token 失效/吊销时调用）。
/// 使用原子 rename→delete 策略避免部分读取。
///
/// # 硬安全阀
/// 如果目标路径等于生产环境路径 `$HOME/.kn/agent/device_token`，直接拒绝。
/// 这个文件**永远**不能通过代码删除，无论 KN_HOME 如何设置。
pub fn delete_device_token() {
    let path = device_token_path();

    // 硬安全阀：生产路径绝对不能删。先规范化再比较，防止 symlink 绕过。
    let prod = production_device_token_path();
    let canonical_target = std::fs::canonicalize(&path).unwrap_or_else(|_| path.clone());
    let canonical_prod = std::fs::canonicalize(&prod).unwrap_or_else(|_| prod.clone());
    if canonical_target == canonical_prod {
        tracing::warn!(
            "delete_device_token 被硬拦截：目标路径是生产环境 token 文件 {}",
            prod.display()
        );
        return;
    }
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

/// Move an invalid local token out of the active credential path.
///
/// This is used when Cloud rejects WebSocket authentication. Keeping the bad
/// token at `device_token` would make every restart re-enter the reconnect loop
/// instead of showing the binding flow. The renamed file is retained for local
/// troubleshooting but is never used for future connections.
pub fn quarantine_invalid_device_token(reason: &str) -> Result<bool> {
    let path = device_token_path();
    if !path.exists() {
        return Ok(false);
    }
    let suffix = chrono::Utc::now().timestamp_millis();
    let backup = path.with_file_name(format!("device_token.invalid-{suffix}"));
    std::fs::rename(&path, &backup).map_err(AgentError::Io)?;
    tracing::warn!(
        reason = reason,
        backup = %backup.display(),
        "device_token 已标记为失效并移出 active 路径"
    );
    Ok(true)
}

/// Remove a production token only after Cloud has accepted a self-unbind.
/// This intentionally has a separate name from the defensive test helper.
pub fn clear_device_token_after_unbind() -> Result<()> {
    let path = device_token_path();
    match std::fs::remove_file(&path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(AgentError::Io(error)),
    }
}

// ── Tests ───────────────────────────────────────────────────

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::sync::Mutex;

    // NOTE: These tests manipulate the process-global KN_HOME env var.
    // std::env::set_var is NOT thread-safe — concurrent tests can race and
    // temporarily leave KN_HOME unset, causing device_token_path() to fall
    // back to $HOME/.kn and operate on the user's REAL token file.
    //
    // To prevent this, we use a static Mutex to serialize all KN_HOME
    // manipulations across test threads, and we NEVER call remove_var().
    // Instead we save/restore the original value.
    static KN_HOME_MUTEX: Mutex<()> = Mutex::new(());

    /// 获取 KN_HOME 全局锁，防止并行测试更改环境变量导致路径退避到真实 ~/.kn/。
    pub(crate) fn kn_home_lock() -> std::sync::MutexGuard<'static, ()> {
        KN_HOME_MUTEX.lock().unwrap()
    }

    fn save_kn_home() -> Option<String> {
        std::env::var("KN_HOME").ok()
    }
    fn restore_kn_home(val: Option<String>) {
        match val {
            Some(v) => std::env::set_var("KN_HOME", v),
            None => std::env::remove_var("KN_HOME"),
        }
    }

    #[test]
    fn test_save_and_load_device_token() {
        let _guard = KN_HOME_MUTEX.lock().unwrap();
        let prev = save_kn_home();
        let dir = std::env::temp_dir().join(format!("kn-test-device-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        std::env::set_var("KN_HOME", dir.to_str().unwrap());

        let token = "test-device-token-12345";
        save_device_token(token).unwrap();
        let loaded = load_device_token().unwrap();
        assert_eq!(loaded, token);

        restore_kn_home(prev);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn test_load_device_token_not_found() {
        let _guard = KN_HOME_MUTEX.lock().unwrap();
        let prev = save_kn_home();
        let dir = std::env::temp_dir().join(format!("kn-test-device-empty-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        std::env::set_var("KN_HOME", dir.to_str().unwrap());

        assert!(load_device_token().is_none());

        restore_kn_home(prev);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn test_delete_device_token() {
        let _guard = KN_HOME_MUTEX.lock().unwrap();
        let prev = save_kn_home();
        let dir = std::env::temp_dir().join(format!("kn-test-device-del-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        std::env::set_var("KN_HOME", dir.to_str().unwrap());

        let token = "test-token-to-delete";
        save_device_token(token).unwrap();
        assert!(load_device_token().is_some());

        delete_device_token();
        assert!(load_device_token().is_none());

        // Calling delete again is a no-op (no crash)
        delete_device_token();
        assert!(load_device_token().is_none());

        restore_kn_home(prev);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn pending_binding_round_trips_without_exposing_the_poll_secret_in_the_filename() {
        let _guard = KN_HOME_MUTEX.lock().unwrap();
        let prev = save_kn_home();
        let dir =
            std::env::temp_dir().join(format!("kn-test-pending-binding-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::env::set_var("KN_HOME", dir.to_str().unwrap());

        let pending = PendingBinding {
            pairing_id: "pair_123".into(),
            approval_code: "123456".into(),
            poll_secret: "secret-not-in-a-path".into(),
            confirm_url: "https://example.test/confirm".into(),
            qr_expires_at_ms: 100,
            pairing_expires_at_ms: 200,
        };
        save_pending_binding(&pending).unwrap();

        assert_eq!(load_pending_binding().unwrap(), pending);
        assert!(!pending_binding_path()
            .display()
            .to_string()
            .contains(&pending.poll_secret));

        restore_kn_home(prev);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn pending_activation_keeps_token_until_activation_is_acknowledged() {
        let _guard = KN_HOME_MUTEX.lock().unwrap();
        let prev = save_kn_home();
        let dir =
            std::env::temp_dir().join(format!("kn-test-pending-activation-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::env::set_var("KN_HOME", dir.to_str().unwrap());

        let activation = PendingActivation {
            pairing_id: "pair_123".into(),
            poll_secret: "poll-secret".into(),
            machine_id: "machine_123".into(),
            device_token: "device-token".into(),
            previous_device_token: None,
            pairing_expires_at_ms: 200,
        };
        save_pending_activation(&activation).unwrap();
        assert!(wss_is_blocked_by_pending_activation());
        assert_eq!(load_pending_activation().unwrap(), activation);

        clear_pending_activation().unwrap();
        assert!(!wss_is_blocked_by_pending_activation());

        restore_kn_home(prev);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn activation_business_errors_distinguish_terminal_from_retryable() {
        assert!(matches!(
            classify_bind_activation_error(2002, "设备数已达上限"),
            AgentError::BindActivationTerminal(_)
        ));
        assert!(matches!(
            classify_bind_activation_error(2005, "设备指纹不匹配"),
            AgentError::BindActivationTerminal(_)
        ));
        assert!(matches!(
            classify_bind_activation_error(500, "服务暂不可用"),
            AgentError::BindActivationRetryable(_)
        ));
    }
}
