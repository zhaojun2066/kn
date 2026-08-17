//! Recovery operations for the launchd-managed production Agent.
use tauri::{command, Manager};

fn service_name() -> String {
    format!(
        "gui/{}/{}",
        unsafe { libc::getuid() },
        crate::agent_runtime::AgentRuntime::current().launchd_label
    )
}

fn write_plist(runtime: &crate::agent_runtime::AgentRuntime) -> Result<std::path::PathBuf, String> {
    let home = crate::home_dir();
    let dir = home.join("Library/LaunchAgents");
    std::fs::create_dir_all(&dir).map_err(|e| format!("创建 LaunchAgents 目录失败: {e}"))?;
    let path = dir.join(format!("{}.plist", runtime.launchd_label));
    let log_dir = runtime.agent_dir().join("logs");
    std::fs::create_dir_all(&log_dir).map_err(|e| format!("创建 Agent 日志目录失败: {e}"))?;
    let agent = crate::agent_runtime::escape_plist_value(
        &runtime.agent_dir().join("kn-agent").display().to_string(),
    );
    let config =
        crate::agent_runtime::escape_plist_value(&runtime.config_dir.display().to_string());
    let stdout_log =
        crate::agent_runtime::escape_plist_value(&log_dir.join("stdout.log").display().to_string());
    let stderr_log =
        crate::agent_runtime::escape_plist_value(&log_dir.join("stderr.log").display().to_string());
    let (cloud_ws_url, cloud_http_url) = super::app_config::production_cloud_urls();
    let cloud_ws_url = crate::agent_runtime::escape_plist_value(&cloud_ws_url);
    let cloud_http_url = crate::agent_runtime::escape_plist_value(&cloud_http_url);
    let content = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict><key>Label</key><string>{}</string><key>ProgramArguments</key><array><string>{}</string></array><key>RunAtLoad</key><true/><key>KeepAlive</key><true/><key>ThrottleInterval</key><integer>5</integer><key>StandardOutPath</key><string>{}</string><key>StandardErrorPath</key><string>{}</string><key>EnvironmentVariables</key><dict><key>PATH</key><string>/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin</string><key>RUST_LOG</key><string>info</string><key>KN_CLOUD_URL</key><string>{}</string><key>KN_CLOUD_HTTP_URL</key><string>{}</string><key>KN_HOME</key><string>{}</string><key>KN_RUNTIME_ENV</key><string>{}</string></dict></dict></plist>"#,
        runtime.launchd_label,
        agent,
        stdout_log,
        stderr_log,
        cloud_ws_url,
        cloud_http_url,
        config,
        runtime.environment_name()
    );
    std::fs::write(&path, content).map_err(|e| format!("写入 Agent 服务配置失败: {e}"))?;
    Ok(path)
}

#[command]
pub fn restart_agent() -> Result<(), String> {
    let service = service_name();
    let output = std::process::Command::new("launchctl")
        .args(["kickstart", "-k", &service])
        .output()
        .map_err(|e| format!("无法调用 launchctl: {e}"))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(format!(
            "重启 Agent 失败: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ))
    }
}

#[command]
pub fn repair_agent(app: tauri::AppHandle) -> Result<(), String> {
    let runtime = crate::agent_runtime::AgentRuntime::current();
    let source = app
        .path()
        .resource_dir()
        .map_err(|e| format!("找不到 App 资源: {e}"))?
        .join("resources/kn-agent");
    if !source.is_file() {
        return Err("安装包内缺少 kn-agent，请重新下载完整安装包".into());
    }
    let dir = runtime.agent_dir();
    std::fs::create_dir_all(&dir).map_err(|e| format!("创建 Agent 目录失败: {e}"))?;
    let target = dir.join("kn-agent");
    let tmp = dir.join("kn-agent.repair.tmp");
    std::fs::copy(&source, &tmp).map_err(|e| format!("复制内置 Agent 失败: {e}"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o755))
            .map_err(|e| format!("设置 Agent 权限失败: {e}"))?;
    }
    std::fs::File::open(&tmp)
        .and_then(|file| file.sync_all())
        .map_err(|e| format!("同步 Agent 文件失败: {e}"))?;
    let plist = match write_plist(&runtime) {
        Ok(path) => path,
        Err(error) => {
            let _ = std::fs::remove_file(&tmp);
            return Err(error);
        }
    };
    let service = service_name();
    let _ = std::process::Command::new("launchctl")
        .args(["bootout", &service])
        .output();
    let backup = dir.join("kn-agent.repair.bak");
    let had_target = target.exists();
    if had_target {
        let _ = std::fs::remove_file(&backup);
        std::fs::rename(&target, &backup)
            .map_err(|e| format!("备份现有 Agent 失败，未修改当前安装: {e}"))?;
    }
    if let Err(error) = std::fs::rename(&tmp, &target) {
        if had_target { let _ = std::fs::rename(&backup, &target); }
        return Err(format!("原子替换 Agent 失败，已保留原安装: {error}"));
    }
    let domain = format!("gui/{}", unsafe { libc::getuid() });
    let output = std::process::Command::new("launchctl")
        .args(["bootstrap", &domain, &plist.display().to_string()])
        .output()
        .map_err(|e| format!("无法注册 Agent 服务: {e}"));
    match output {
        Ok(result) if result.status.success() => {
            let _ = std::fs::remove_file(&backup);
            Ok(())
        }
        Ok(result) => {
            let _ = std::fs::remove_file(&target);
            if had_target {
                let _ = std::fs::rename(&backup, &target);
                let _ = std::process::Command::new("launchctl")
                    .args(["bootstrap", &domain, &plist.display().to_string()])
                    .output();
            }
            Err(format!(
                "注册 Agent 服务失败，已恢复原安装: {}",
                String::from_utf8_lossy(&result.stderr).trim()
            ))
        }
        Err(error) => {
            let _ = std::fs::remove_file(&target);
            if had_target { let _ = std::fs::rename(&backup, &target); }
            Err(format!("无法注册 Agent 服务，已恢复原安装: {error}"))
        }
    }
}
