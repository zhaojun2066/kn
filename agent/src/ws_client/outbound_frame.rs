use serde_json::{json, Map, Value};

pub const MAX_TEXT_FRAME_BYTES: usize = 768 * 1024;

pub fn protect_outbound_text(text: String) -> Vec<String> {
    let bytes = text.len();
    let parsed: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
    let message_type = parsed["type"].as_str().unwrap_or("unknown");
    tracing::debug!(message_type, bytes, "WSS 出站帧");
    if bytes <= MAX_TEXT_FRAME_BYTES {
        return vec![text];
    }

    tracing::warn!(
        message_type,
        bytes,
        limit = MAX_TEXT_FRAME_BYTES,
        "WSS 出站帧超过限制"
    );
    let replacement = if is_project_result(message_type) {
        project_too_large_result(message_type, &parsed)
    } else {
        terminal_too_large_error()
    };
    if replacement.len() <= MAX_TEXT_FRAME_BYTES {
        vec![replacement]
    } else {
        tracing::warn!(
            message_type,
            bytes = replacement.len(),
            limit = MAX_TEXT_FRAME_BYTES,
            "WSS 轻量替代帧仍超过限制，已丢弃"
        );
        Vec::new()
    }
}

fn is_project_result(message_type: &str) -> bool {
    message_type.starts_with("project_")
        && (message_type.ends_with("_result") || message_type.ends_with("_progress"))
}

fn project_too_large_result(message_type: &str, original: &Value) -> String {
    let mut data = Map::new();
    if let Some(source) = original.get("data").and_then(Value::as_object) {
        for key in [
            "projectKey",
            "deviceId",
            "projectPath",
            "requestId",
            "runId",
            "stage",
        ] {
            if let Some(value) = source.get(key) {
                data.insert(key.to_string(), bounded_association(value));
            }
        }
    }
    data.insert(
        "status".to_string(),
        Value::String("responseTooLarge".to_string()),
    );
    data.insert(
        "message".to_string(),
        Value::String(too_large_message(message_type).to_string()),
    );
    if message_type == "project_git_status_result" {
        data.insert("files".to_string(), json!([]));
        data.insert("totalFiles".to_string(), json!(0));
        data.insert("offset".to_string(), json!(0));
        data.insert("nextOffset".to_string(), json!(0));
        data.insert("hasMore".to_string(), json!(false));
        data.insert("truncated".to_string(), json!(false));
        data.insert("snapshotId".to_string(), Value::Null);
    } else if message_type == "project_verify_log_window_result" {
        // Keep the iOS log-window DTO decodable even when the original window
        // was too large to send. Cloud adds the same defaults defensively.
        data.insert("startLine".to_string(), json!(0));
        data.insert("endLine".to_string(), json!(0));
        data.insert("centerLine".to_string(), json!(0));
        data.insert("lines".to_string(), json!([]));
        data.insert("hasEarlier".to_string(), json!(false));
        data.insert("hasLater".to_string(), json!(false));
        data.insert("contentTruncated".to_string(), json!(true));
    }
    json!({"type": message_type, "data": data}).to_string()
}

fn bounded_association(value: &Value) -> Value {
    match value {
        Value::String(text) => Value::String(text.chars().take(512).collect()),
        Value::Number(_) | Value::Bool(_) | Value::Null => value.clone(),
        _ => Value::Null,
    }
}

fn terminal_too_large_error() -> String {
    json!({
        "type": "error_notify",
        "data": {
            "code": "responseTooLarge",
            "message": "终端输出内容过大，已丢弃本段输出"
        }
    })
    .to_string()
}

fn too_large_message(message_type: &str) -> &'static str {
    if message_type.contains("git_status") {
        "Git 变更内容过大，请使用分页加载更多"
    } else if message_type.contains("change_summary") {
        "变更摘要内容过大，请缩小范围后重试"
    } else if message_type.contains("file_diff") {
        "该文件 Diff 过大，请在电脑端查看"
    } else if message_type.contains("verify_log_window") {
        "日志窗口内容过大，请缩小查看范围"
    } else if message_type.contains("verify_log_issues") {
        "错误内容过多，已停止本次传输"
    } else if message_type.contains("pr_") {
        "PR 详情内容过大，请在 GitHub 查看"
    } else {
        "电脑返回内容过大，请缩小范围后重试"
    }
}
