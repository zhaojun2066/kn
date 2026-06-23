//! launchd 守护进程管理。
//!
//! 管理 kn-agent 作为 macOS launchd 守护进程的安装、卸载和状态检查。
//! 使用现代 `launchctl bootstrap`/`bootout` API（macOS 10.10+）。

use crate::error::{AgentError, Result};
use std::path::{Path, PathBuf};

const AGENT_LABEL: &str = "com.kn.agent";

/// XML-escape a string for safe embedding in plist XML.
fn xml_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            _ => out.push(c),
        }
    }
    out
}

/// 生成 launchd plist XML 内容（路径经 XML 转义）。
pub fn generate_plist_content(agent_path: &Path, log_dir: &Path) -> String {
    let agent_escaped = xml_escape(&agent_path.display().to_string());
    let stdout_escaped = xml_escape(&log_dir.join("stdout.log").display().to_string());
    let stderr_escaped = xml_escape(&log_dir.join("stderr.log").display().to_string());

    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>{label}</string>
    <key>ProgramArguments</key>
    <array>
        <string>{agent_path}</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <true/>
    <key>ThrottleInterval</key>
    <integer>5</integer>
    <key>StandardOutPath</key>
    <string>{stdout}</string>
    <key>StandardErrorPath</key>
    <string>{stderr}</string>
    <key>EnvironmentVariables</key>
    <dict>
        <key>RUST_LOG</key>
        <string>info</string>
    </dict>
</dict>
</plist>"#,
        label = AGENT_LABEL,
        agent_path = agent_escaped,
        stdout = stdout_escaped,
        stderr = stderr_escaped,
    )
}

/// 获取当前用户的 UID。
///
/// # Safety
///
/// `libc::getuid()` is a simple syscall that always succeeds on Unix and has
/// no observable side effects. The wrapped call is safe in all contexts.
fn get_uid() -> u32 {
    // SAFETY: libc::getuid() is always available on macOS/Linux, cannot fail,
    // and has no side effects. The returned uid_t fits in u32 on all platforms
    // we target.
    unsafe { libc::getuid() }
}

/// 获取 plist 安装路径。
fn plist_path() -> PathBuf {
    let home = kn_common::path::home_dir();
    home.join("Library")
        .join("LaunchAgents")
        .join(format!("{}.plist", AGENT_LABEL))
}

/// 安装 agent 为 launchd 守护进程。
pub async fn install(agent_path: &Path, log_dir: &Path) -> Result<()> {
    let plist = plist_path();

    if let Some(parent) = plist.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("创建 LaunchAgents 目录失败: {}", e))?;
    }

    let content = generate_plist_content(agent_path, log_dir);
    std::fs::write(&plist, &content)
        .map_err(|e| format!("写入 plist 失败: {}", e))?;

    // 先卸载（如果已在运行中）
    if is_running().await {
        let _ = uninstall().await;
    }

    let uid = get_uid();
    let output = tokio::process::Command::new("launchctl")
        .args([
            "bootstrap",
            &format!("gui/{}", uid),
            &plist.display().to_string(),
        ])
        .output()
        .await
        .map_err(|e| AgentError::Other(format!("launchctl 执行失败: {}", e)))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.contains("already bootstrapped") || stderr.contains("Already bootstrapped") {
            return Ok(());
        }
        return Err(AgentError::Other(format!(
            "launchctl bootstrap 失败: {}",
            stderr
        )));
    }

    tracing::info!("Agent 已安装为 launchd 守护进程");
    Ok(())
}

/// 卸载 agent launchd 守护进程。
pub async fn uninstall() -> Result<()> {
    let uid = get_uid();

    let output = tokio::process::Command::new("launchctl")
        .args(["bootout", &format!("gui/{}/{}", uid, AGENT_LABEL)])
        .output()
        .await
        .map_err(|e| AgentError::Other(format!("launchctl 执行失败: {}", e)))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.contains("Could not find") || stderr.contains("No such process") {
            // 继续删除 plist
        } else {
            return Err(AgentError::Other(format!(
                "launchctl bootout 失败: {}",
                stderr
            )));
        }
    }

    let plist = plist_path();
    if plist.exists() {
        std::fs::remove_file(&plist).map_err(|e| format!("删除 plist 失败: {}", e))?;
    }

    tracing::info!("Agent launchd 守护进程已卸载");
    Ok(())
}

/// 检查 plist 是否已安装。
pub fn is_installed() -> bool {
    plist_path().exists()
}

/// 检查 agent 是否正在运行。
///
/// 通过 `launchctl print` 检查。如果 launchctl 本身失败（非零退出码
/// 可能是 "service not found"），返回 `false`。
pub async fn is_running() -> bool {
    let uid = get_uid();
    match tokio::process::Command::new("launchctl")
        .args(["print", &format!("gui/{}/{}", uid, AGENT_LABEL)])
        .output()
        .await
    {
        Ok(out) => out.status.success(),
        // launchctl 不可用或权限拒绝 → 保守地返回 false
        Err(e) => {
            tracing::debug!("launchctl print 失败: {}", e);
            false
        }
    }
}

/// 重启 agent（卸载 → 轮询等待退出 → 安装）。
pub async fn restart(agent_path: &Path, log_dir: &Path) -> Result<()> {
    if is_running().await {
        uninstall().await?;
        // 轮询等待进程完全退出（最多 5 秒）
        for _ in 0..50 {
            if !is_running().await {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
    }
    install(agent_path, log_dir).await
}

// ── Tests ───────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_xml_escape() {
        assert_eq!(xml_escape("normal_path"), "normal_path");
        assert_eq!(xml_escape("a & b"), "a &amp; b");
        assert_eq!(xml_escape("<tag>"), "&lt;tag&gt;");
        assert_eq!(xml_escape(r#""quoted""#), "&quot;quoted&quot;");
    }

    #[test]
    fn test_generate_plist_content() {
        let agent_path = Path::new("/usr/local/bin/kn-agent");
        let log_dir = Path::new("/Users/test/.kn/agent/logs");
        let content = generate_plist_content(agent_path, log_dir);

        assert!(content.contains("<key>Label</key>"));
        assert!(content.contains("<string>com.kn.agent</string>"));
        assert!(content.contains("<key>RunAtLoad</key>"));
        assert!(content.contains("<true/>"));
        assert!(content.contains("<key>KeepAlive</key>"));
        assert!(content.contains("<key>ThrottleInterval</key>"));
        assert!(content.contains("<integer>5</integer>"));
        assert!(content.contains("/usr/local/bin/kn-agent"));
        assert!(content.contains("stdout.log"));
        assert!(content.contains("stderr.log"));
    }

    #[test]
    fn test_generate_plist_escapes_special_chars() {
        let agent_path = Path::new("/usr/bin/foo & bar");
        let log_dir = Path::new("/tmp/test <log>");
        let content = generate_plist_content(agent_path, log_dir);
        // & should be escaped
        assert!(content.contains("foo &amp; bar"));
        // < and > should be escaped
        assert!(content.contains("test &lt;log&gt;"));
    }

    #[test]
    fn test_plist_path_format() {
        let path = plist_path();
        let path_str = path.display().to_string();
        assert!(path_str.ends_with("com.kn.agent.plist"));
        assert!(path_str.contains("LaunchAgents"));
    }
}
