//! Tauri command — Desktop 前端通过此命令访问 Agent IPC
//!
//! 前端调 invoke("agent_ipc", { method, params }) → Rust 连 Agent Unix Socket → 返回结果

use std::time::Duration;

use kn_common::path::agent_dir;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;

/// 最大响应行长度（1MB，防止恶意/异常 Agent OOM）
const MAX_RESPONSE_LEN: usize = 1_048_576;

fn ipc_socket_path() -> std::path::PathBuf {
    agent_dir().join("ipc.sock")
}

#[tauri::command]
pub async fn agent_ipc(
    method: String,
    params: Option<serde_json::Value>,
) -> Result<serde_json::Value, String> {
    let stream = tokio::time::timeout(
        Duration::from_secs(5),
        UnixStream::connect(ipc_socket_path()),
    )
    .await
    .map_err(|_| "Agent IPC 连接超时（5 秒）".to_string())?
    .map_err(|e| format!("Agent IPC 连接失败: {}", e))?;

    let request = serde_json::json!({
        "id": "desktop",
        "method": method,
        "params": params.unwrap_or(serde_json::json!({}))
    });
    let mut line = serde_json::to_string(&request).map_err(|e| e.to_string())?;
    line.push('\n');

    let (reader, mut writer) = stream.into_split();

    // 写请求（5 秒超时）
    tokio::time::timeout(Duration::from_secs(5), writer.write_all(line.as_bytes()))
        .await
        .map_err(|_| "IPC 写入超时（5 秒）".to_string())?
        .map_err(|e| format!("IPC 写入失败: {}", e))?;
    drop(writer); // 半关闭写端，通知对端请求已完整发送

    // 读响应（5 秒超时）
    let mut buf_reader = BufReader::new(reader);
    let mut response = String::new();
    tokio::time::timeout(Duration::from_secs(5), buf_reader.read_line(&mut response))
        .await
        .map_err(|_| "IPC 读取超时（5 秒）".to_string())?
        .map_err(|e| format!("IPC 读取失败: {}", e))?;

    if response.len() > MAX_RESPONSE_LEN {
        return Err("Agent 响应过大".into());
    }

    let parsed: serde_json::Value =
        serde_json::from_str(&response).map_err(|e| format!("IPC 响应解析失败: {}", e))?;

    // Agent IPC wraps result in {"id":"...","result":{...}}, extract it
    if let Some(err) = parsed.get("error") {
        return Err(format!(
            "Agent 错误: {}",
            err.get("message")
                .and_then(|m| m.as_str())
                .unwrap_or("unknown")
        ));
    }
    Ok(parsed.get("result").cloned().unwrap_or(parsed))
}
