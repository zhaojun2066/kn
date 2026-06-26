//! Open paths in external applications (terminal, editor, finder).

use tauri::command;

use super::network::find_binary;

#[command]
pub fn open_in_terminal(path: String) -> Result<(), String> {
    let mut spawned = false;
    for app in &["iTerm", "Warp", "Terminal"] {
        if std::process::Command::new("open")
            .args(["-a", app, &path])
            .spawn()
            .is_ok()
        {
            spawned = true;
            break;
        }
    }
    if !spawned { return Err("未找到可用的终端应用 (iTerm/Warp/Terminal)".into()); }
    Ok(())
}

#[command]
pub fn open_file(path: String) -> Result<(), String> {
    std::process::Command::new("/usr/bin/open")
        .arg(&path)
        .spawn()
        .map_err(|e| format!("{}", e))?;
    Ok(())
}

/// Open a path in an external editor or file manager.
/// `editor`: "code" (VS Code), "cursor" (Cursor), "idea" (IntelliJ IDEA), "terminal", "finder"
#[command]
pub fn open_in_editor(path: String, editor: String) -> Result<(), String> {
    match editor.as_str() {
        "code" => {
            if let Some(bin) = find_binary(&["code"]) {
                std::process::Command::new(&bin).arg(&path).spawn()
                    .map_err(|_| "启动 VS Code 失败，请确认已安装或重试".to_string())?;
            } else {
                std::process::Command::new("open")
                    .args(["-a", "Visual Studio Code", &path])
                    .spawn()
                    .map_err(|_| "未找到 VS Code。请确认已安装，或在 VS Code 中执行 Cmd+Shift+P → 'Install code command in PATH'".to_string())?;
            }
            Ok(())
        }
        "cursor" => {
            if let Some(bin) = find_binary(&["cursor"]) {
                std::process::Command::new(&bin).arg(&path).spawn()
                    .map_err(|_| "启动 Cursor 失败，请确认已安装或重试".to_string())?;
            } else {
                std::process::Command::new("open")
                    .args(["-a", "Cursor", &path])
                    .spawn()
                    .map_err(|_| "未找到 Cursor。请确认已安装，或在 Cursor 中执行 Cmd+Shift+P → 'Install cursor command in PATH'".to_string())?;
            }
            Ok(())
        }
        "idea" => {
            if let Some(bin) = find_binary(&["idea"]) {
                std::process::Command::new(&bin).arg(&path).spawn()
                    .map_err(|_| "启动 IntelliJ IDEA 失败，请确认已安装或重试".to_string())?;
            } else {
                std::process::Command::new("open")
                    .args(["-a", "IntelliJ IDEA", &path])
                    .spawn()
                    .map_err(|_| "未找到 IntelliJ IDEA。请确认已安装，下载: https://jetbrains.com/idea".to_string())?;
            }
            Ok(())
        }
        "terminal" => open_in_terminal(path),
        "finder" => open_file(path),
        _ => Err(format!("不支持的编辑器: {}", editor)),
    }
}
