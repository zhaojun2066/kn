//! Environment check for onboarding — detect installed CLIs, shell wrapper, config.
//!
//! The `check_environment` command returns a structured report of what's installed,
//! what's missing, and recommended install commands for each CLI.

use tauri::command;

use super::network::find_binary;
use super::system_scan::home_dir;

// ── Data types ──

#[derive(Debug, serde::Serialize)]
pub struct InstallOption {
    pub id: String,
    pub label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    pub description: String,
    pub recommended: bool,
    pub platforms: Vec<String>,
}

#[derive(Debug, serde::Serialize)]
pub struct EnvCheckItem {
    pub name: String,
    pub label: String,
    pub status: String,
    pub severity: String,
    pub category: String,
    pub detail: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detected_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub install_options: Option<Vec<InstallOption>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub install_cmd: Option<String>,
}

#[derive(Debug, serde::Serialize)]
pub struct EnvCheckResult {
    pub items: Vec<EnvCheckItem>,
    pub all_ok: bool,
}

// ── Binary detection ──

fn check_binary_on_path(name: &str) -> Option<String> {
    if let Some(path) = find_binary(&[name]) {
        let p = std::path::Path::new(&path);
        if (p.is_absolute() || path.contains('/') || path.contains('\\')) && p.exists() {
            return Some(path);
        }
    }
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string());
    let shell_args: &[&str] = &[
        "-lc",
        &format!(
            "command -v {} 2>/dev/null || (type {} 2>/dev/null | grep -v 'not found')",
            name, name
        ),
    ];
    if let Ok(output) = std::process::Command::new(&shell).args(shell_args).output() {
        let s = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !s.is_empty() && !s.contains("not found") {
            return Some(s);
        }
    }
    None
}

// ── Version detection ──

fn get_cli_version(binary_path: &str) -> Option<String> {
    // Strategy 1: resolve symlink → find sibling package.json → read "version"
    if let Ok(real) = std::fs::canonicalize(binary_path) {
        let mut dir = real.parent().map(|p| p.to_path_buf());
        while let Some(current) = dir {
            let pkg = current.join("package.json");
            if pkg.exists() {
                if let Ok(contents) = std::fs::read_to_string(&pkg) {
                    if let Some(ver) = extract_version_from_json(&contents) {
                        return Some(ver);
                    }
                }
            }
            if current.parent().is_none() || current.as_os_str().is_empty() {
                break;
            }
            dir = current.parent().map(|p| p.to_path_buf());
        }
    }
    // Strategy 2: run --version
    if let Ok(output) = std::process::Command::new(binary_path)
        .arg("--version")
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .output()
    {
        if output.status.success() {
            let s = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !s.is_empty() {
                return Some(s);
            }
        }
    }
    None
}

fn extract_version_from_json(json: &str) -> Option<String> {
    for line in json.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("\"version\"") {
            let after_key = rest.trim_start();
            if let Some(val_part) = after_key.strip_prefix(':') {
                let val = val_part.trim();
                let val = val.strip_suffix(',').unwrap_or(val);
                if val.len() >= 2 && val.starts_with('"') && val.ends_with('"') {
                    let inner = &val[1..val.len() - 1];
                    if !inner.is_empty() {
                        return Some(inner.to_string());
                    }
                }
            }
        }
    }
    None
}

// ── Install options ──

fn install_option(
    id: &str,
    label: &str,
    command: Option<&str>,
    description: &str,
    recommended: bool,
    platforms: &[&str],
) -> InstallOption {
    InstallOption {
        id: id.into(),
        label: label.into(),
        command: command.map(|c| c.into()),
        description: description.into(),
        recommended,
        platforms: platforms.iter().map(|p| (*p).into()).collect(),
    }
}

fn cli_install_options(name: &str) -> Vec<InstallOption> {
    match name {
        "claude" => vec![
            install_option(
                "official-script",
                "官方脚本",
                Some("curl -fsSL https://claude.ai/install.sh | bash"),
                "推荐安装方式，不假设 npm 全局目录。",
                true,
                &["macos"],
            ),
            install_option(
                "npm",
                "npm 全局安装",
                Some("npm i -g @anthropic-ai/claude-code"),
                "适合已经使用 Node/npm 管理 CLI 的用户。",
                false,
                &["macos"],
            ),
        ],
        "codex" => vec![
            install_option(
                "npm",
                "npm 全局安装",
                Some("npm i -g @openai/codex"),
                "适合已有 Node/npm 环境的用户。",
                true,
                &["macos"],
            ),
            install_option(
                "homebrew",
                "Homebrew 安装",
                Some("brew install codex"),
                "适合通过 Homebrew 管理 CLI 的用户。",
                false,
                &["macos"],
            ),
            install_option(
                "manual",
                "手动安装",
                None,
                "如使用 pnpm、公司镜像或其他包管理器，请按你的环境安装并确保 codex 在 PATH 中。",
                false,
                &["macos"],
            ),
        ],
        "qoderclicn" => vec![
            install_option(
                "npm",
                "npm 全局安装",
                Some("npm i -g @qodercn-ai/qoderclicn"),
                "沿用当前应用支持的 qoderclicn 命令名。",
                true,
                &["macos"],
            ),
            install_option(
                "manual",
                "官方/手动安装",
                None,
                "如你通过 Qoder 官方安装器或其他渠道安装，请确保 qoderclicn 在 PATH 中。",
                false,
                &["macos"],
            ),
        ],
        _ => Vec::new(),
    }
}

fn recommended_install_cmd(options: &[InstallOption]) -> Option<String> {
    options
        .iter()
        .find(|o| o.recommended)
        .and_then(|o| o.command.clone())
        .or_else(|| options.iter().find_map(|o| o.command.clone()))
}

fn cli_ok_item(name: &str, label: &str, path: String, version: Option<String>) -> EnvCheckItem {
    let detail = version.as_deref().unwrap_or(&path);
    EnvCheckItem {
        name: name.into(),
        label: label.into(),
        status: "ok".into(),
        severity: "ok".into(),
        category: "cli".into(),
        detail: detail.to_string(),
        detected_path: Some(path),
        version,
        install_options: None,
        install_cmd: None,
    }
}

fn cli_missing_item(name: &str, label: &str) -> EnvCheckItem {
    let options = cli_install_options(name);
    let install_cmd = recommended_install_cmd(&options);
    let detail = match install_cmd.as_deref() {
        Some(cmd) => format!("未安装，推荐: {}", cmd),
        None => "未安装，请按官方说明安装并加入 PATH".into(),
    };
    EnvCheckItem {
        name: name.into(),
        label: label.into(),
        status: "missing".into(),
        severity: "warn".into(),
        category: "cli".into(),
        detail,
        detected_path: None,
        version: None,
        install_options: if options.is_empty() {
            None
        } else {
            Some(options)
        },
        install_cmd,
    }
}

fn push_cli_item(items: &mut Vec<EnvCheckItem>, binary: &str, label: &str) {
    match check_binary_on_path(binary) {
        Some(path) => {
            let version = get_cli_version(&path);
            items.push(cli_ok_item(binary, label, path, version));
        }
        None => items.push(cli_missing_item(binary, label)),
    }
}

// ── Main command ──

#[command]
pub fn check_environment() -> EnvCheckResult {
    let home = home_dir();
    let mut items = Vec::new();

    push_cli_item(&mut items, "claude", "Claude Code");
    push_cli_item(&mut items, "codex", "Codex");
    push_cli_item(&mut items, "qoderclicn", "Qoder");

    // Shell wrapper
    let kn_dir = home.join(".kn");
    let shell_rc = kn_dir.join("shell-rc");
    if shell_rc.exists() {
        let in_rc = if let Ok(zshrc) = std::fs::read_to_string(home.join(".zshrc")) {
            zshrc.contains(".kn/shell-rc") || zshrc.contains(".claude-profiles")
        } else {
            false
        };
        items.push(EnvCheckItem {
            name: "shell-wrapper".into(),
            label: "Shell 集成".into(),
            status: if in_rc { "ok".into() } else { "warn".into() },
            severity: if in_rc { "ok".into() } else { "warn".into() },
            category: "shell".into(),
            detail: if in_rc {
                "已激活".into()
            } else {
                "已安装但未激活".into()
            },
            detected_path: Some(shell_rc.display().to_string()),
            version: None,
            install_options: None,
            install_cmd: None,
        });
    } else {
        let legacy_dir = home.join(".claude-profiles");
        let legacy_rc = legacy_dir.join("shell-rc");
        if legacy_rc.exists() {
            let in_rc = if let Ok(zshrc) = std::fs::read_to_string(home.join(".zshrc")) {
                zshrc.contains("shell-rc")
            } else {
                false
            };
            items.push(EnvCheckItem {
                name: "shell-wrapper".into(),
                label: "Shell 集成".into(),
                status: if in_rc { "ok".into() } else { "warn".into() },
                severity: if in_rc { "ok".into() } else { "warn".into() },
                category: "shell".into(),
                detail: if in_rc {
                    "已激活（旧目录 ~/.claude-profiles/，建议迁移）".into()
                } else {
                    "已安装但未激活（旧目录 ~/.claude-profiles/）".into()
                },
                detected_path: Some(legacy_rc.display().to_string()),
                version: None,
                install_options: None,
                install_cmd: None,
            });
        } else {
            items.push(EnvCheckItem {
                name: "shell-wrapper".into(),
                label: "Shell 集成".into(),
                status: "missing".into(),
                severity: "warn".into(),
                category: "shell".into(),
                detail: "未安装，应用启动时会尝试自动写入 shell 集成".into(),
                detected_path: None,
                version: None,
                install_options: None,
                install_cmd: None,
            });
        }
    }

    // Config directory
    let config_dir = home.join(".kn");
    let config_file = config_dir.join("config.yaml");
    let legacy_config_dir = home.join(".claude-profiles");
    let legacy_config_file = legacy_config_dir.join("config.yaml");
    if config_dir.exists() {
        if config_file.exists() {
            items.push(EnvCheckItem {
                name: "config".into(),
                label: "配置文件".into(),
                status: "ok".into(),
                severity: "ok".into(),
                category: "config".into(),
                detail: config_file.display().to_string(),
                detected_path: Some(config_file.display().to_string()),
                version: None,
                install_options: None,
                install_cmd: None,
            });
        } else {
            items.push(EnvCheckItem {
                name: "config".into(),
                label: "配置文件".into(),
                status: "warn".into(),
                severity: "warn".into(),
                category: "config".into(),
                detail: "目录存在但无配置文件".into(),
                detected_path: Some(config_dir.display().to_string()),
                version: None,
                install_options: None,
                install_cmd: None,
            });
        }
        if legacy_config_file.exists() {
            items.push(EnvCheckItem {
                name: "config-legacy".into(),
                label: "旧配置文件".into(),
                status: "info".into(),
                severity: "info".into(),
                category: "config".into(),
                detail: format!(
                    "旧目录仍存在: {}，建议迁移后清理",
                    legacy_config_dir.display()
                ),
                detected_path: Some(legacy_config_file.display().to_string()),
                version: None,
                install_options: None,
                install_cmd: None,
            });
        }
    } else if legacy_config_dir.exists() && legacy_config_file.exists() {
        items.push(EnvCheckItem {
            name: "config".into(),
            label: "配置文件".into(),
            status: "warn".into(),
            severity: "warn".into(),
            category: "config".into(),
            detail: format!(
                "旧目录: {} → 重启应用将自动迁移到 ~/.kn/",
                legacy_config_dir.display()
            ),
            detected_path: Some(legacy_config_file.display().to_string()),
            version: None,
            install_options: None,
            install_cmd: None,
        });
    } else {
        items.push(EnvCheckItem {
            name: "config".into(),
            label: "配置文件".into(),
            status: "missing".into(),
            severity: "warn".into(),
            category: "config".into(),
            detail: "目录不存在".into(),
            detected_path: None,
            version: None,
            install_options: None,
            install_cmd: None,
        });
    }

    let all_ok = items.iter().all(|i| i.status == "ok");
    EnvCheckResult { items, all_ok }
}

// ── Tests ──

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cli_missing_item_has_recommended_install_option() {
        let item = cli_missing_item("codex", "Codex");
        assert_eq!(item.status, "missing");
        assert_eq!(item.severity, "warn");
        assert_eq!(item.category, "cli");
        assert!(item.install_cmd.is_some());
        assert!(item
            .install_options
            .as_ref()
            .unwrap()
            .iter()
            .any(|o| o.recommended && o.command.is_some()));
    }

    #[test]
    fn test_qoder_install_command_is_consistent() {
        let item = cli_missing_item("qoderclicn", "Qoder");
        assert_eq!(
            item.install_cmd.as_deref(),
            Some("npm i -g @qodercn-ai/qoderclicn")
        );
    }
}
