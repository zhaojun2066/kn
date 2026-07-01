/// TOML 字符串转义（用于 Codex CLI -c 参数注入）。
pub(crate) fn toml_string(s: &str) -> String {
    let escaped = s
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\'', "\\'");
    format!("\"{}\"", escaped)
}

/// 根据 tool 名称查找 CLI 二进制路径。
pub fn resolve_tool_path(tool: &str) -> std::result::Result<String, String> {
    let candidates: &[&str] = match tool {
        "claude" => &["claude"],
        "codex" => &["codex"],
        "qoder" => &["qoder", "codex"],
        "qoderclicn" => &["qoder", "codex"],
        "bash" => &["bash"],
        _ => return Err(format!("未知 tool: {}", tool)),
    };
    kn_common::path::find_binary(candidates).ok_or_else(|| format!("未找到 {} 二进制", tool))
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

// ── Tests ───────────────────────────────────────────────────
