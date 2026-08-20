//! System config scanning — discover Claude/Codex/Qoder CLI configs under ~/.
//! Also provides `home_dir()` used across other command modules.

use tauri::command;

#[derive(Debug, serde::Serialize)]
pub struct ScanResult {
    pub profiles: Vec<ScanProfile>,
}

#[derive(Debug, serde::Serialize, Clone)]
pub struct ScanProfile {
    pub name: String,
    pub cli_type: String,
    pub auth_mode: String,
    pub provider_id: String,
    pub env: std::collections::HashMap<String, String>,
    pub source: String,
}

pub(crate) fn home_dir() -> std::path::PathBuf {
    kn_common::path::home_dir()
}

#[command]
pub fn get_home_dir() -> String {
    home_dir().to_string_lossy().to_string()
}

fn read_json_file(path: &std::path::Path) -> Result<serde_json::Value, String> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| format!("读取 {} 失败: {}", path.display(), e))?;
    serde_json::from_str(&content).map_err(|e| format!("解析 {} 失败: {}", path.display(), e))
}

const RESERVED_PROFILE_NAMES: &[&str] = &["claude", "codex", "qoderclicn", "profile", "ai", "help"];

fn sanitize_scan_name(name: &str) -> String {
    if RESERVED_PROFILE_NAMES.contains(&name) {
        format!("{}-config", name)
    } else {
        name.to_string()
    }
}

#[command]
pub fn scan_system_configs() -> Result<ScanResult, String> {
    scan_system_configs_from_home(home_dir())
}

fn scan_system_configs_from_home(home: std::path::PathBuf) -> Result<ScanResult, String> {
    let mut profiles = Vec::new();
    let mut checked = Vec::new();

    // ~/.claude/settings.json
    let claude_path = home.join(".claude").join("settings.json");
    let claude_str = claude_path.display().to_string();
    checked.push(claude_str.clone());
    if let Ok(json) = read_json_file(&claude_path) {
        let mut env = std::collections::HashMap::new();
        if let Some(env_obj) = json.get("env").and_then(|e| e.as_object()) {
            for (k, v) in env_obj {
                if let Some(s) = v.as_str() {
                    env.insert(k.clone(), s.to_string());
                }
            }
        }
        if !env.is_empty() {
            profiles.push(ScanProfile {
                name: sanitize_scan_name("claude"),
                cli_type: "claude".into(),
                auth_mode: "api_key".into(),
                provider_id: "imported".into(),
                env,
                source: claude_str,
            });
        }
    }

    // ~/.codex/auth.json
    let codex_auth = home.join(".codex").join("auth.json");
    let codex_auth_str = codex_auth.display().to_string();
    checked.push(codex_auth_str.clone());
    let mut codex_env = std::collections::HashMap::new();
    let mut codex_has_api_key = false;
    if let Ok(json) = read_json_file(&codex_auth) {
        if let Some(key) = json.get("OPENAI_API_KEY").and_then(|v| v.as_str()) {
            codex_env.insert("OPENAI_API_KEY".into(), key.to_string());
            codex_has_api_key = true;
        }
    }

    // ~/.codex/config.toml
    let codex_config = home.join(".codex").join("config.toml");
    let codex_config_str = codex_config.display().to_string();
    checked.push(codex_config_str.clone());
    if let Ok(content) = std::fs::read_to_string(&codex_config) {
        if let Ok(root) = toml::from_str::<toml::Value>(&content) {
            if let Some(model) = root.get("model").and_then(|v| v.as_str()) {
                if !model.is_empty() {
                    codex_env.insert("OPENAI_MODEL".into(), model.to_string());
                }
            }
            if let Some(base_url) = root.get("base_url").and_then(|v| v.as_str()) {
                if !base_url.is_empty() {
                    codex_env.insert("OPENAI_BASE_URL".into(), base_url.to_string());
                }
            }
        }
    }

    if codex_has_api_key {
        codex_env.insert("_KN_AUTH_MODE".into(), "api_key".into());
        codex_env.insert("_KN_PROVIDER_ID".into(), "openai-official".into());
        profiles.push(ScanProfile {
            name: sanitize_scan_name("codex"),
            cli_type: "codex".into(),
            auth_mode: "api_key".into(),
            provider_id: "openai-official".into(),
            env: codex_env,
            source: format!("{}, {}", codex_auth_str, codex_config_str),
        });
    }

    // ~/.qoder-cn/
    let qoder_dir = home.join(".qoder-cn");
    let qoder_str = qoder_dir.display().to_string();
    checked.push(qoder_str.clone());
    if qoder_dir.exists() {
        let mut qoder_env = std::collections::HashMap::new();
        let settings_path = qoder_dir.join("settings.json");
        if let Ok(json) = read_json_file(&settings_path) {
            if let Some(token) = json.get("personalAccessToken").and_then(|v| v.as_str()) {
                qoder_env.insert("QODERCN_PERSONAL_ACCESS_TOKEN".into(), token.to_string());
            }
        }
        if !qoder_env.is_empty() {
            qoder_env.insert("_KN_AUTH_MODE".into(), "token".into());
            qoder_env.insert("_KN_PROVIDER_ID".into(), "qodercn-pat".into());
            profiles.push(ScanProfile {
                name: sanitize_scan_name("qoder-cn"),
                cli_type: "qoderclicn".into(),
                auth_mode: "token".into(),
                provider_id: "qodercn-pat".into(),
                env: qoder_env,
                source: qoder_str,
            });
        }
    }

    if profiles.is_empty() {
        return Err(format!("未找到配置。\n已检查:\n{}", checked.join("\n")));
    }
    Ok(ScanResult { profiles })
}

// ── Tests ──

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_home_dir_returns_existing_path() {
        let home = home_dir();
        assert!(
            home.exists(),
            "home_dir should return an existing path: {:?}",
            home
        );
        assert!(home.is_absolute());
    }

    #[test]
    fn qoder_dir_without_token_does_not_create_candidate() {
        let tmp = tempfile::tempdir().unwrap();
        let qoder = tmp.path().join(".qoder-cn");
        std::fs::create_dir_all(&qoder).unwrap();
        std::fs::write(qoder.join("settings.json"), r#"{"model":"qoder"}"#).unwrap();

        let err = scan_system_configs_from_home(tmp.path().to_path_buf()).unwrap_err();
        assert!(err.contains("未找到配置"));
    }

    #[test]
    fn codex_requires_api_key_candidate() {
        let tmp = tempfile::tempdir().unwrap();
        let codex = tmp.path().join(".codex");
        std::fs::create_dir_all(&codex).unwrap();
        std::fs::write(codex.join("auth.json"), r#"{"auth_mode":"chatgpt","tokens":{}}"#).unwrap();
        std::fs::write(codex.join("config.toml"), r#"model = "gpt-5-codex""#).unwrap();
        let err = scan_system_configs_from_home(tmp.path().to_path_buf()).unwrap_err();
        assert!(err.contains("未找到配置"));

        std::fs::write(codex.join("auth.json"), r#"{"auth_mode":"apikey","OPENAI_API_KEY":"sk-test"}"#).unwrap();
        let result = scan_system_configs_from_home(tmp.path().to_path_buf()).unwrap();
        assert_eq!(result.profiles.len(), 1);
        assert_eq!(result.profiles[0].cli_type, "codex");
        assert_eq!(result.profiles[0].auth_mode, "api_key");
        assert_eq!(result.profiles[0].env.get("OPENAI_API_KEY").unwrap(), "sk-test");
    }
}
