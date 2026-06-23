//! Tauri command — Desktop 前端通过此命令访问 Agent IPC
//!
//! 前端调 invoke("agent_ipc", { method, params }) → Rust 连 Agent Unix Socket → 返回结果

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::time::Duration;

use kn_common::path::agent_dir;

/// 最大响应行长度（1MB，防止恶意/异常 Agent OOM）
const MAX_RESPONSE_LEN: usize = 1_048_576;

fn ipc_socket_path() -> std::path::PathBuf {
    agent_dir().join("ipc.sock")
}

#[tauri::command]
pub fn agent_ipc(
    method: String,
    params: Option<serde_json::Value>,
) -> Result<serde_json::Value, String> {
    let mut stream = UnixStream::connect(ipc_socket_path())
        .map_err(|e| format!("Agent IPC 连接失败: {}", e))?;

    // Set timeouts to prevent hanging if Agent is unresponsive
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .and(stream.set_write_timeout(Some(Duration::from_secs(5))))
        .map_err(|e| format!("设置超时失败: {}", e))?;

    let request = serde_json::json!({
        "id": "desktop",
        "method": method,
        "params": params.unwrap_or(serde_json::json!({}))
    });
    let mut line = serde_json::to_string(&request).map_err(|e| e.to_string())?;
    line.push('\n');
    stream
        .write_all(line.as_bytes())
        .map_err(|e| format!("IPC 写入失败: {}", e))?;

    let mut reader = BufReader::with_capacity(8192, stream);
    let mut response = String::new();
    reader
        .read_line(&mut response)
        .map_err(|e| format!("IPC 读取失败: {}", e))?;

    if response.len() > MAX_RESPONSE_LEN {
        return Err("Agent 响应过大".into());
    }

    let parsed: serde_json::Value =
        serde_json::from_str(&response).map_err(|e| format!("IPC 响应解析失败: {}", e))?;

    // Agent IPC wraps result in {"id":"...","result":{...}}, extract it
    if let Some(err) = parsed.get("error") {
        return Err(format!("Agent 错误: {}", err.get("message").and_then(|m| m.as_str()).unwrap_or("unknown")));
    }
    Ok(parsed.get("result").cloned().unwrap_or(parsed))
}
