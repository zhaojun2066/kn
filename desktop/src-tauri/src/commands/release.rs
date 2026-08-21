//! Self-hosted desktop release discovery with version and SHA-256 validation.

use semver::Version;
use serde::Deserialize;
use std::error::Error;
use tauri::command;

use super::app_config::load_app_config;

#[derive(Deserialize)]
struct ApiEnvelope<T> {
    code: i32,
    message: String,
    data: T,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ReleaseApiResponse {
    available: bool,
    mandatory: bool,
    version: Option<String>,
    notes: Option<String>,
    url: Option<String>,
    sha256: Option<String>,
    agent_version: Option<String>,
    min_protocol_version: Option<u32>,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VerifiedRelease {
    version: String,
    notes: String,
    url: String,
    sha256: String,
    mandatory: bool,
    agent_version: String,
    min_protocol_version: u32,
}

fn platform() -> &'static str {
    if cfg!(target_arch = "aarch64") {
        "darwin-aarch64"
    } else {
        "darwin-x86_64"
    }
}

fn describe_reqwest_error(error: &reqwest::Error) -> String {
    let mut message = error.to_string();
    let mut source = error.source();
    while let Some(err) = source {
        message.push_str(": ");
        message.push_str(&err.to_string());
        source = err.source();
    }
    message
}

#[command]
pub async fn check_desktop_release(
    app: tauri::AppHandle,
) -> Result<Option<VerifiedRelease>, String> {
    let config = load_app_config()?;
    let api_url = config.release_api_url.ok_or("未配置自有服务器更新接口")?;
    let current = app.package_info().version.to_string();
    let current_version = Version::parse(&current).map_err(|_| "当前版本不符合 SemVer")?;
    let url = format!(
        "{}?platform={}&currentVersion={}",
        api_url,
        platform(),
        current
    );

    let text = tauri::async_runtime::spawn_blocking(move || {
        reqwest::blocking::Client::new()
            .get(url)
            .send()
            .map_err(|e| format!("请求更新接口失败: {}", describe_reqwest_error(&e)))?
            .error_for_status()
            .map_err(|e| format!("更新接口 HTTP 错误: {}", describe_reqwest_error(&e)))?
            .text()
            .map_err(|e| format!("读取更新接口失败: {}", describe_reqwest_error(&e)))
    })
    .await
    .map_err(|e| format!("更新检查任务失败: {e}"))??;

    let envelope: ApiEnvelope<ReleaseApiResponse> =
        serde_json::from_str(&text).map_err(|e| format!("更新接口响应无效: {e}"))?;
    if envelope.code != 0 {
        return Err(format!("更新接口拒绝请求: {}", envelope.message));
    }
    let response = envelope.data;
    if !response.available {
        return Ok(None);
    }

    let version = response.version.ok_or("发布版本缺失")?;
    let released = Version::parse(&version).map_err(|_| "发布版本不符合 SemVer")?;
    if released <= current_version {
        return Err("拒绝版本回退或重复更新".into());
    }
    let url = response.url.ok_or("下载地址缺失")?;
    if !valid_download_url(&url) {
        return Err("下载地址必须使用 HTTPS".into());
    }
    let sha256 = response.sha256.ok_or("SHA-256 缺失")?.to_ascii_lowercase();
    if sha256.len() != 64 || !sha256.bytes().all(|c| c.is_ascii_hexdigit()) {
        return Err("SHA-256 格式无效".into());
    }
    let notes = response.notes.unwrap_or_default();
    let agent_version = response.agent_version.ok_or("Agent 版本缺失")?;
    let min_protocol_version = response.min_protocol_version.ok_or("最低协议版本缺失")?;
    if agent_version != version {
        return Err("桌面版本与 Agent 版本不一致".into());
    }

    Ok(Some(VerifiedRelease {
        version,
        notes,
        url,
        sha256,
        mandatory: response.mandatory,
        agent_version,
        min_protocol_version,
    }))
}

fn valid_download_url(url: &str) -> bool {
    url.starts_with("https://") || (cfg!(debug_assertions) && url.starts_with("http://"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn platform_is_supported_macos_target() {
        assert!(matches!(platform(), "darwin-aarch64" | "darwin-x86_64"));
    }

    #[test]
    fn debug_build_accepts_local_download_url() {
        assert!(valid_download_url(
            "https://api.knshark.com/releases/kn.dmg"
        ));
        if cfg!(debug_assertions) {
            assert!(valid_download_url("http://localhost:8080/releases/kn.dmg"));
        }
    }
}
