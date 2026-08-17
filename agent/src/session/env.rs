use tokio::process::Command;
use tokio::time::{timeout, Duration};

/// TOML 字符串转义（用于 Codex CLI -c 参数注入）。
pub(crate) fn toml_string(s: &str) -> String {
    let escaped = s
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\'', "\\'");
    format!("\"{}\"", escaped)
}

/// 根据 tool 名称返回允许查找的 CLI 二进制候选。
pub fn tool_binary_candidates(tool: &str) -> std::result::Result<&'static [&'static str], String> {
    let candidates = match tool {
        "claude" => &["claude"],
        "codex" => &["codex"],
        "qoder" => &["qoder"],
        "qoderclicn" => &["qoderclicn"],
        "bash" => &["bash"],
        _ => return Err(format!("未知 tool: {}", tool)),
    };
    Ok(candidates)
}

/// 根据 tool 名称查找 CLI 二进制路径。
pub fn resolve_tool_path(tool: &str) -> std::result::Result<String, String> {
    let candidates = tool_binary_candidates(tool)?;
    kn_common::path::find_binary(candidates).ok_or_else(|| format!("未找到 {} 二进制", tool))
}

/// Reads the CLI version once for Cloud's version-specific command catalog.
/// Failure is deliberately non-fatal: Cloud will select the tool's stable
/// default catalog when the version is unavailable.
pub async fn resolve_cli_version(tool: &str) -> Option<String> {
    let path = resolve_tool_path(tool).ok()?;
    let output = timeout(
        Duration::from_secs(2),
        Command::new(path).arg("--version").output(),
    )
    .await
    .ok()?
    .ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    stdout
        .split_whitespace()
        .chain(stderr.split_whitespace())
        .find(|token| looks_like_semver(token))
        .map(|token| {
            token
                .trim_matches(|c: char| {
                    !c.is_ascii_alphanumeric() && c != '.' && c != '-' && c != '+'
                })
                .to_string()
        })
}

fn looks_like_semver(value: &str) -> bool {
    let value = value
        .trim_matches(|c: char| !c.is_ascii_alphanumeric() && c != '.' && c != '-' && c != '+');
    let value = value.strip_prefix('v').unwrap_or(value);
    let mut parts = value.splitn(3, '.');
    parts
        .next()
        .is_some_and(|part| !part.is_empty() && part.chars().all(|c| c.is_ascii_digit()))
        && parts
            .next()
            .is_some_and(|part| !part.is_empty() && part.chars().all(|c| c.is_ascii_digit()))
        && parts.next().is_some_and(|part| {
            !part.is_empty() && part.chars().next().is_some_and(|c| c.is_ascii_digit())
        })
}

/// Normalizes supported CLI identifiers without merging distinct products.
/// `qoder` and `qoderclicn` are intentionally never aliases.
pub fn normalized_cli(tool: &str) -> Option<&'static str> {
    match tool.trim().to_ascii_lowercase().as_str() {
        "claude" => Some("claude"),
        "codex" => Some("codex"),
        "qoder" => Some("qoder"),
        "qoderclicn" => Some("qoderclicn"),
        _ => None,
    }
}

/// 构造原生 CLI 的恢复参数。profile 由 kn 的 profile 环境层注入，
/// 因而不作为 shell 字符串拼接的一部分。
pub fn history_resume_args(tool: &str, native_session_id: &str) -> Result<Vec<String>, String> {
    let native_session_id = native_session_id.trim();
    if native_session_id.is_empty()
        || native_session_id.len() > 512
        || native_session_id.chars().any(char::is_control)
    {
        return Err("invalid_native_session_id".to_string());
    }

    let flag = match normalized_cli(tool) {
        Some("claude") => "--resume",
        Some("codex") => "resume",
        Some("qoderclicn") => "-r",
        _ => return Err("unsupported_cli".to_string()),
    };
    Ok(vec![flag.to_string(), native_session_id.to_string()])
}

pub fn history_resume_cli_matches_profile(requested_cli: &str, profile_tool: &str) -> bool {
    normalized_cli(requested_cli).is_some_and(|requested| {
        normalized_cli(profile_tool).is_some_and(|profile| requested == profile)
    })
}

#[cfg(test)]
mod history_resume_tests {
    use super::{history_resume_args, history_resume_cli_matches_profile};

    #[test]
    fn history_resume_arguments_follow_each_cli_native_syntax() {
        assert_eq!(
            history_resume_args("claude", "session-claude"),
            Ok(vec!["--resume".to_string(), "session-claude".to_string()])
        );
        assert_eq!(
            history_resume_args("codex", "session-codex"),
            Ok(vec!["resume".to_string(), "session-codex".to_string()])
        );
        assert_eq!(
            history_resume_args("qoderclicn", "session-qoder"),
            Ok(vec!["-r".to_string(), "session-qoder".to_string()])
        );
    }

    #[test]
    fn history_resume_does_not_treat_qoder_as_qoderclicn() {
        assert!(history_resume_args("qoder", "session-qoder").is_err());
    }

    #[test]
    fn history_resume_arguments_reject_empty_or_control_character_session_ids() {
        assert!(history_resume_args("codex", " ").is_err());
        assert!(history_resume_args("codex", "session\nnext").is_err());
    }

    #[test]
    fn history_resume_requires_profile_tool_to_match_requested_cli() {
        assert!(!history_resume_cli_matches_profile("qoder", "qoderclicn"));
        assert!(history_resume_cli_matches_profile("codex", "codex"));
        assert!(!history_resume_cli_matches_profile("claude", "codex"));
        assert!(!history_resume_cli_matches_profile("unknown", "codex"));
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProfileToolResolveError {
    ProfileNotFound,
    ProfileInvalid,
    ToolNotFound,
}

impl ProfileToolResolveError {
    pub fn reason(self) -> &'static str {
        match self {
            Self::ProfileNotFound => "profile_not_found",
            Self::ProfileInvalid => "profile_invalid",
            Self::ToolNotFound => "tool_not_found",
        }
    }
}

/// 远程启动时从 profile 配置解析真实 CLI tool。
pub fn resolve_tool_from_profile(profile: &str) -> Result<String, ProfileToolResolveError> {
    let env_output = kn_common::profile::get_env_cmd(profile)
        .map_err(|_| ProfileToolResolveError::ProfileNotFound)?;
    let tool = kn_common::profile::detect_cli_type(&env_output.env)
        .ok_or(ProfileToolResolveError::ProfileInvalid)?;
    resolve_tool_path(&tool).map_err(|_| ProfileToolResolveError::ToolNotFound)?;
    Ok(tool)
}

pub(crate) struct ToolPrep {
    pub(crate) extra_args: Vec<String>,
}

/// Tool 启动前预处理。
pub(crate) fn prepare_tool_env(
    tool: &str,
    _env_vars: &Option<std::collections::HashMap<String, String>>,
) -> std::result::Result<ToolPrep, String> {
    match tool {
        "claude" => {
            // Claude: 通过 --settings 注入 env vars（临时文件在 session end 时由 ToolCleanupGuard 删除）
            let tmp = std::env::temp_dir().join(format!(
                "kn-claude-{}-{}.json",
                std::process::id(),
                chrono::Utc::now().timestamp_millis()
            ));
            let settings_env = _env_vars.clone().unwrap_or_default();
            let settings = serde_json::json!({"env": settings_env});
            std::fs::write(
                &tmp,
                serde_json::to_string(&settings).map_err(|e| e.to_string())?,
            )
            .map_err(|e| e.to_string())?;
            Ok(ToolPrep {
                extra_args: vec!["--settings".into(), tmp.to_string_lossy().to_string()],
            })
        }
        "codex" => {
            // Codex ignores OPENAI_API_KEY / OPENAI_BASE_URL / OPENAI_MODEL env vars.
            // It reads only ~/.codex/auth.json + ~/.codex/config.toml + -c flags.
            // Mirror the shell wrapper logic (shell/ai-profile.sh):
            //   1. Write API key to auth.json (with backup)
            //   2. Pass base_url via -c model_providers.custom.base_url=...
            //   3. Pass model via -c model=...
            let mut extra_args = Vec::new();
            if let Some(ref env) = _env_vars {
                // 1. Write auth.json
                if let Some(apikey) = env.get("OPENAI_API_KEY") {
                    let codex_dir = kn_common::path::home_dir().join(".codex");
                    let auth_path = codex_dir.join("auth.json");
                    let bak_path = codex_dir.join("auth.json.kn-bak");
                    // Backup existing auth.json
                    if auth_path.exists() {
                        let _ = std::fs::copy(&auth_path, &bak_path);
                    }
                    if let Err(e) = std::fs::create_dir_all(&codex_dir) {
                        return Err(format!("failed to create ~/.codex dir: {}", e));
                    }
                    let auth_content =
                        format!(r#"{{"auth_mode":"apikey","OPENAI_API_KEY":"{}"}}"#, apikey);
                    std::fs::write(&auth_path, auth_content)
                        .map_err(|e| format!("failed to write auth.json: {}", e))?;
                }
                // 2. Model via -c (TOML-quoted: model="gpt-5.5")
                if let Some(model) = env.get("OPENAI_MODEL") {
                    let val = toml_string(model);
                    extra_args.push("-c".into());
                    extra_args.push(format!("model={}", val));
                }
                // 3. Base URL via -c (TOML-quoted: base_url="https://...")
                if let Some(base_url) = env.get("OPENAI_BASE_URL") {
                    let val = toml_string(base_url);
                    extra_args.push("-c".into());
                    extra_args.push(format!("model_providers.custom.base_url={}", val));
                }
            }
            Ok(ToolPrep { extra_args })
        }
        "bash" | "qoder" | "qoderclicn" => {
            // Bash / Qoder: 通过环境变量注入，无需额外参数
            Ok(ToolPrep { extra_args: vec![] })
        }
        _ => Ok(ToolPrep { extra_args: vec![] }),
    }
}

#[cfg(test)]
mod tests {
    use super::{looks_like_semver, tool_binary_candidates};

    #[test]
    fn qoder_tools_do_not_fallback_to_codex() {
        assert_eq!(tool_binary_candidates("qoder").unwrap(), &["qoder"]);
        assert_eq!(
            tool_binary_candidates("qoderclicn").unwrap(),
            &["qoderclicn"]
        );
        assert_eq!(tool_binary_candidates("codex").unwrap(), &["codex"]);
        assert_eq!(tool_binary_candidates("claude").unwrap(), &["claude"]);
    }

    #[test]
    fn semver_detection_accepts_cli_version_tokens_only() {
        assert!(looks_like_semver("1.4.2"));
        assert!(looks_like_semver("v1.4.2"));
        assert!(!looks_like_semver("version"));
        assert!(!looks_like_semver("1.4"));
    }
}
