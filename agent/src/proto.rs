//! WSS 消息协议类型 — 两阶段反序列化匹配 kn-cloud 信封格式。
//!
//! 权威来源：`kn-cloud/kn-cloud-ws/.../MessageTypes.java` (15 种消息类型)
//!            + `KnWsHandler.java` 角色白名单 (ALLOWED_MESSAGES)
//!
//! ```text
//! 信封: {"type": "...", "ts": <epoch_ms>, "sessionId"?: "s_nanoid", "data": {...}}
//! ```
//!
//! ## Agent 出站 (agent → cloud) — 7 种，对齐 Java ALLOWED_MESSAGES:
//! - ping, session_created, session_ended, output, profile_list, project_list
//!
//! ## Agent 入站 (cloud → agent):
//! - 心跳: pong
//! - 连接: connected
//! - 会话 (iOS→cloud→agent 转发): start_session, input, ctrl
//! - 确认: profile_list_ack
//! - 错误: error_notify (Java sendError 可向任意客户端发送)
//! - 结束: kill_session
//!
//! 不在 agent 白名单/不需要 agent 关注的消息:
//! - start_session_ack, ack (仅 mobile)
//! - agent_error (Java 代码中未实现)

use serde::{Deserialize, Serialize};

// ── Raw envelope (Phase 1 deserialization) ───────────────────

/// 从云端接收的原始 JSON 信封。先解析为此结构，再按 type 分派。
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WsEnvelope {
    #[serde(rename = "type")]
    pub msg_type: String,
    #[serde(default)]
    pub ts: Option<i64>,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub data: Option<serde_json::Value>,
}

// ── Parsed incoming messages (Phase 2) ──────────────────────

/// Agent 接收的消息（类型安全，已分派）。
#[derive(Debug, Clone)]
pub enum AgentIncoming {
    /// 心跳响应
    Pong { ts: i64 },
    /// WSS 连接确认
    Connected {
        ws_session_id: String,
        node_id: Option<String>,
        protocol_version: Option<u32>,
    },
    /// 启动新会话（来自 iOS/Desktop 用户）
    /// sessionId 由 Agent 自行生成（"s_" + nanoid(12)），cloud 不再预分配。
    StartSession {
        /// CLI 工具（claude/codex/qoder/bash）
        tool: String,
        /// Profile 名称
        profile: Option<String>,
        /// 工作目录
        cwd: Option<String>,
        /// 发起用户 ID
        from_user_id: u64,
        /// 初始终端列数
        cols: u16,
        /// 初始终端行数
        rows: u16,
    },
    /// 用户输入文本（session 由 data.sessionId 标识）
    Input {
        session_nid: String,
        seq: u64,
        content: String,
        from_user_id: u64,
    },
    /// 控制信号（Ctrl+C、Ctrl+D 等）
    Ctrl {
        session_nid: String,
        signal: serde_json::Value,
    },
    /// 终端尺寸变化
    Resize {
        session_nid: String,
        cols: u16,
        rows: u16,
    },
    /// 云端错误通知（对齐 Java MessageTypes.ERROR_NOTIFY + sendError()）
    ErrorNotify { code: String, message: String },
    /// 配置文件列表确认
    ProfileListAck,
    /// 请求回放会话输出日志（iOS 恢复会话时发送）
    ReplayOutput { session_nid: String },
    /// WSS 对 session_created 的确认
    SessionCreatedAck {
        session_nid: String,
        status: String,
        error: Option<String>,
    },
    /// iOS 恢复会话请求（cloud 已查 Redis，agent 做本地验证）
    ResumeSession { session_nid: String },
    /// iOS/Cloud 请求强制结束远程会话
    KillSession { session_nid: String, reason: String },
    /// 未知消息类型
    Unknown {
        msg_type: String,
        raw: serde_json::Value,
    },
}

impl WsEnvelope {
    /// 将原始信封解析为类型化的 AgentIncoming。
    pub fn parse(&self) -> Result<AgentIncoming, String> {
        match self.msg_type.as_str() {
            "pong" => {
                let ts = self
                    .data
                    .as_ref()
                    .and_then(|d| d.get("ts"))
                    .and_then(|v| v.as_i64())
                    .unwrap_or(0);
                Ok(AgentIncoming::Pong { ts })
            }
            "connected" => {
                let ws_session_id = self
                    .data
                    .as_ref()
                    .and_then(|d| d.get("ws_session_id"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let node_id = self
                    .data
                    .as_ref()
                    .and_then(|d| d.get("node_id"))
                    .and_then(|v| v.as_str())
                    .map(String::from);
                let protocol_version = self
                    .data
                    .as_ref()
                    .and_then(|d| d.get("protocol_version"))
                    .and_then(|v| v.as_u64())
                    .map(|v| v as u32);
                Ok(AgentIncoming::Connected {
                    ws_session_id,
                    node_id,
                    protocol_version,
                })
            }
            "start_session" => {
                let data = self
                    .data
                    .as_ref()
                    .ok_or_else(|| "start_session 缺少 data 字段".to_string())?;
                // sessionId 由 Agent 自行生成，cloud 不再预分配
                Ok(AgentIncoming::StartSession {
                    tool: data["tool"].as_str().unwrap_or("bash").to_string(),
                    profile: data["profile"].as_str().map(String::from),
                    cwd: data["cwd"].as_str().map(String::from),
                    from_user_id: data["fromUserId"].as_u64().unwrap_or(0),
                    cols: data["cols"].as_u64().map(|v| v as u16).unwrap_or(80),
                    rows: data["rows"].as_u64().map(|v| v as u16).unwrap_or(24),
                })
            }
            "input" => {
                let data = self
                    .data
                    .as_ref()
                    .ok_or_else(|| "input 缺少 data 字段".to_string())?;
                // session 由 data.sessionId 标识，与 ctrl/resize/replay_output 一致。
                let session_nid = data["sessionId"].as_str().unwrap_or("").to_string();
                if session_nid.is_empty() {
                    return Err("input sessionId 为空".to_string());
                }
                Ok(AgentIncoming::Input {
                    session_nid,
                    seq: data["seq"].as_u64().unwrap_or(0),
                    content: data["content"].as_str().unwrap_or("").to_string(),
                    from_user_id: data["fromUserId"].as_u64().unwrap_or(0),
                })
            }
            "ctrl" => {
                let data = self
                    .data
                    .as_ref()
                    .ok_or_else(|| "ctrl 缺少 data 字段".to_string())?;
                // session 由 data.sessionId 标识
                let session_nid = data["sessionId"].as_str().unwrap_or("").to_string();
                Ok(AgentIncoming::Ctrl {
                    session_nid,
                    signal: data.clone(),
                })
            }
            "resize" => {
                let data = self
                    .data
                    .as_ref()
                    .ok_or_else(|| "resize 缺少 data 字段".to_string())?;
                let session_nid = data["sessionId"].as_str().unwrap_or("").to_string();
                Ok(AgentIncoming::Resize {
                    session_nid,
                    cols: data["cols"].as_u64().map(|v| v as u16).unwrap_or(80),
                    rows: data["rows"].as_u64().map(|v| v as u16).unwrap_or(24),
                })
            }
            "error_notify" => {
                let data = self
                    .data
                    .as_ref()
                    .ok_or_else(|| "error_notify 缺少 data 字段".to_string())?;
                Ok(AgentIncoming::ErrorNotify {
                    code: data["code"].as_str().unwrap_or("UNKNOWN").to_string(),
                    message: data["message"].as_str().unwrap_or("").to_string(),
                })
            }
            "profile_list_ack" => Ok(AgentIncoming::ProfileListAck),
            "replay_output" => {
                let data = self
                    .data
                    .as_ref()
                    .ok_or_else(|| "replay_output 缺少 data 字段".to_string())?;
                let session_nid = data["sessionId"].as_str().unwrap_or("").to_string();
                if session_nid.is_empty() {
                    return Err("replay_output sessionId 为空".to_string());
                }
                Ok(AgentIncoming::ReplayOutput { session_nid })
            }
            "session_created_ack" => {
                let data = self
                    .data
                    .as_ref()
                    .ok_or_else(|| "session_created_ack 缺少 data 字段".to_string())?;
                let session_nid = data["sessionId"].as_str().unwrap_or("").to_string();
                if session_nid.is_empty() {
                    return Err("session_created_ack sessionId 为空".to_string());
                }
                let status = data["status"].as_str().unwrap_or("error").to_string();
                let error = data["error"].as_str().map(String::from);
                Ok(AgentIncoming::SessionCreatedAck {
                    session_nid,
                    status,
                    error,
                })
            }
            "resume_session" => {
                let data = self
                    .data
                    .as_ref()
                    .ok_or_else(|| "resume_session 缺少 data 字段".to_string())?;
                let session_nid = data["sessionId"].as_str().unwrap_or("").to_string();
                if session_nid.is_empty() {
                    return Err("resume_session sessionId 为空".to_string());
                }
                Ok(AgentIncoming::ResumeSession { session_nid })
            }
            "kill_session" => {
                let data = self
                    .data
                    .as_ref()
                    .ok_or_else(|| "kill_session 缺少 data 字段".to_string())?;
                let session_nid = data["sessionId"].as_str().unwrap_or("").to_string();
                if session_nid.is_empty() {
                    return Err("kill_session sessionId 为空".to_string());
                }
                Ok(AgentIncoming::KillSession {
                    session_nid,
                    reason: data["reason"]
                        .as_str()
                        .unwrap_or("process_killed")
                        .to_string(),
                })
            }
            other => Ok(AgentIncoming::Unknown {
                msg_type: other.to_string(),
                raw: self.data.clone().unwrap_or(serde_json::Value::Null),
            }),
        }
    }
}

// ── Outbound messages ──────────────────────────────────────

/// Agent 发送给云端的消息构建器。每个方法返回预序列化的 JSON 字符串。
pub struct WsMessageBuilder;

impl WsMessageBuilder {
    /// 心跳 ping。
    pub fn ping() -> String {
        r#"{"type":"ping"}"#.to_string()
    }

    /// 会话创建确认。sessionId 已由 Agent 生成。
    /// 携带 tool/cwd/cols/rows/source 供 cloud 写入 Redis Hash，pid 由后续 heartbeat 更新。
    /// `source`: "ios" | "desktop" — 区分发起方，cloud 据此走不同注册逻辑。
    /// `msg_id`: 可选 ACK 关联 ID，cloud 在 session_created_ack 中原样返回。
    pub fn session_created(
        session_nid: &str,
        tool: &str,
        cwd: &str,
        profile: Option<&str>,
        cols: u16,
        rows: u16,
        source: &str,
    ) -> String {
        Self::session_created_with_msg_id(session_nid, tool, cwd, profile, cols, rows, source, None)
    }

    /// 带 msg_id 的 session_created，用于 ACK 关联。
    pub fn session_created_with_msg_id(
        session_nid: &str,
        tool: &str,
        cwd: &str,
        profile: Option<&str>,
        cols: u16,
        rows: u16,
        source: &str,
        msg_id: Option<&str>,
    ) -> String {
        let mut data = serde_json::json!({
            "sessionId": session_nid,
            "tool": tool,
            "cwd": cwd,
            "cols": cols,
            "rows": rows,
            "source": source
        });
        if let Some(p) = profile {
            data["profile"] = serde_json::Value::String(p.to_string());
        }
        if let Some(mid) = msg_id {
            data["msgId"] = serde_json::Value::String(mid.to_string());
        }
        serde_json::json!({
            "type": "session_created",
            "data": data
        })
        .to_string()
    }

    /// 会话结束通知。
    pub fn session_ended(session_nid: &str, reason: &str) -> String {
        serde_json::json!({
            "type": "session_ended",
            "data": {
                "sessionId": session_nid,
                "reason": reason
            }
        })
        .to_string()
    }

    /// PTY 输出数据。session 由 sessionId 标识。
    pub fn output(session_nid: &str, ansi_text: &str) -> String {
        serde_json::json!({
            "type": "output",
            "data": {
                "sessionId": session_nid,
                "ansi_text": ansi_text
            }
        })
        .to_string()
    }

    /// 会话输出回放完成通知。Cloud 转发给 iOS，用于区分“无历史输出”和“仍在等待输出”。
    pub fn replay_output_done(
        session_nid: &str,
        status: &str,
        bytes: usize,
        chunks: usize,
        message: Option<&str>,
    ) -> String {
        serde_json::json!({
            "type": "replay_output_done",
            "data": {
                "sessionId": session_nid,
                "status": status,
                "bytes": bytes,
                "chunks": chunks,
                "message": message
            }
        })
        .to_string()
    }

    /// 错误通知（Agent → Cloud → iOS）。
    pub fn error_notify(code: &str, message: &str) -> String {
        serde_json::json!({
            "type": "error_notify",
            "data": { "code": code, "message": message }
        })
        .to_string()
    }

    /// CLI 进程心跳（每 15s，替代 session_interrupted 崩溃恢复）。
    pub fn cli_heartbeat(sessions: &[HeartbeatSession]) -> String {
        serde_json::json!({
            "type": "cli_heartbeat",
            "data": {
                "sessions": sessions
            }
        })
        .to_string()
    }

    /// 上报可用 Profile 列表。
    pub fn profile_list(profiles: &[ProfileInfo]) -> String {
        serde_json::json!({
            "type": "profile_list",
            "profiles": profiles
        })
        .to_string()
    }

    /// 上报项目列表。
    pub fn project_list(projects: &[ProjectInfoOut]) -> String {
        serde_json::json!({
            "type": "project_list",
            "data": {
                "projects": projects
            }
        })
        .to_string()
    }

    /// 上报崩溃恢复——中断的会话列表。
    pub fn sessions_interrupted(sessions: &[InterruptedSession]) -> String {
        serde_json::json!({
            "type": "session_interrupted",
            "data": {
                "sessions": sessions
            }
        })
        .to_string()
    }
}

/// 中断会话信息（崩溃恢复时上报给云端）。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InterruptedSession {
    pub nid: String,
    pub tool: String,
    pub profile: Option<String>,
    pub cwd: String,
    #[serde(rename = "lastInput")]
    pub last_input: String,
    #[serde(rename = "lastOutputSnippet")]
    pub last_output_snippet: String,
}

/// CLI 进程心跳信息（每 15s 上报给云端）。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HeartbeatSession {
    #[serde(rename = "sessionId")]
    pub session_nid: String,
    pub pid: u32,
    pub state: String,
}

/// Profile 信息（上报给云端）。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileInfo {
    pub name: String,
    pub tool: Option<String>,
    pub description: String,
}

impl From<&kn_common::profile::ProfileSummary> for ProfileInfo {
    fn from(p: &kn_common::profile::ProfileSummary) -> Self {
        Self {
            name: p.name.clone(),
            tool: p.cli_type.clone(),
            description: p.desc.clone(),
        }
    }
}

// ── Outgoing message enum (type-safe) ──────────────────────

/// Agent 发送给云端的类型化消息。
/// 在 channel 边界携带结构化数据，序列化延迟到发送时。
#[derive(Debug, Clone)]
pub enum OutgoingMessage {
    /// 会话结束通知（session/manager.rs → main loop → WSS）
    SessionEnded { session_nid: String, reason: String },
}

impl OutgoingMessage {
    /// 序列化为 JSON 字符串（对齐 WsMessageBuilder 的 envelope 格式）。
    pub fn to_json(&self) -> String {
        match self {
            Self::SessionEnded {
                session_nid,
                reason,
            } => WsMessageBuilder::session_ended(session_nid, reason),
        }
    }
}

// ── Project info (outgoing) ────────────────────────────────

/// 项目信息（上报给云端）。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectInfoOut {
    pub name: String,
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_profile: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

// ── Tests ───────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_connected() {
        let json = serde_json::json!({
            "type": "connected",
            "ts": 1234567890,
            "data": {
                "ws_session_id": "abc123",
                "node_id": "node1",
                "protocol_version": 1
            }
        });
        let env: WsEnvelope = serde_json::from_value(json).unwrap();
        let msg = env.parse().unwrap();
        match msg {
            AgentIncoming::Connected {
                ws_session_id,
                node_id,
                protocol_version,
            } => {
                assert_eq!(ws_session_id, "abc123");
                assert_eq!(node_id, Some("node1".into()));
                assert_eq!(protocol_version, Some(1));
            }
            _ => panic!("expected Connected"),
        }
    }

    #[test]
    fn test_parse_start_session() {
        let json = serde_json::json!({
            "type": "start_session",
            "ts": 1234567890,
            "data": {
                "tool": "claude",
                "profile": "my-profile",
                "cwd": "/Users/test/project",
                "fromUserId": 100,
                "cols": 48,
                "rows": 18
            }
        });
        let env: WsEnvelope = serde_json::from_value(json).unwrap();
        let msg = env.parse().unwrap();
        match msg {
            AgentIncoming::StartSession {
                tool,
                profile,
                cwd,
                from_user_id,
                cols,
                rows,
            } => {
                assert_eq!(tool, "claude");
                assert_eq!(profile, Some("my-profile".into()));
                assert_eq!(cwd, Some("/Users/test/project".into()));
                assert_eq!(from_user_id, 100);
                assert_eq!(cols, 48);
                assert_eq!(rows, 18);
            }
            _ => panic!("expected StartSession"),
        }
    }

    #[test]
    fn test_parse_resize() {
        let json = serde_json::json!({
            "type": "resize",
            "data": {
                "sessionId": "s_abc123def456",
                "seq": 7,
                "cols": 52,
                "rows": 20
            }
        });
        let env: WsEnvelope = serde_json::from_value(json).unwrap();
        let msg = env.parse().unwrap();
        match msg {
            AgentIncoming::Resize {
                session_nid,
                cols,
                rows,
            } => {
                assert_eq!(session_nid, "s_abc123def456");
                assert_eq!(cols, 52);
                assert_eq!(rows, 20);
            }
            _ => panic!("expected Resize"),
        }
    }

    #[test]
    fn test_parse_input() {
        let json = serde_json::json!({
            "type": "input",
            "data": {
                "sessionId": "s_abc",
                "seq": 5,
                "content": "hello world",
                "fromUserId": 100
            }
        });
        let env: WsEnvelope = serde_json::from_value(json).unwrap();
        let msg = env.parse().unwrap();
        match msg {
            AgentIncoming::Input {
                session_nid,
                seq,
                content,
                from_user_id,
            } => {
                assert_eq!(session_nid, "s_abc");
                assert_eq!(seq, 5);
                assert_eq!(content, "hello world");
                assert_eq!(from_user_id, 100);
            }
            _ => panic!("expected Input"),
        }
    }

    #[test]
    fn test_parse_kill_session() {
        let json = serde_json::json!({
            "type": "kill_session",
            "data": {
                "sessionId": "s_abc",
                "reason": "user_closed_tab"
            }
        });
        let env: WsEnvelope = serde_json::from_value(json).unwrap();
        let msg = env.parse().unwrap();
        match msg {
            AgentIncoming::KillSession {
                session_nid,
                reason,
            } => {
                assert_eq!(session_nid, "s_abc");
                assert_eq!(reason, "user_closed_tab");
            }
            _ => panic!("expected KillSession"),
        }
    }

    #[test]
    fn test_parse_kill_session_defaults_to_process_killed() {
        let json = serde_json::json!({
            "type": "kill_session",
            "data": {
                "sessionId": "s_abc"
            }
        });
        let env: WsEnvelope = serde_json::from_value(json).unwrap();
        let msg = env.parse().unwrap();
        match msg {
            AgentIncoming::KillSession {
                session_nid,
                reason,
            } => {
                assert_eq!(session_nid, "s_abc");
                assert_eq!(reason, "process_killed");
            }
            _ => panic!("expected KillSession"),
        }
    }

    #[test]
    fn test_outbound_ping() {
        let json = WsMessageBuilder::ping();
        assert!(json.contains("ping"));
    }

    #[test]
    fn test_outbound_session_created() {
        let json = WsMessageBuilder::session_created(
            "s_abc123",
            "claude",
            "/home/user/proj",
            None,
            80,
            24,
            "ios",
        );
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["type"], "session_created");
        assert_eq!(parsed["data"]["sessionId"], "s_abc123");
        assert_eq!(parsed["data"]["tool"], "claude");
        assert_eq!(parsed["data"]["cwd"], "/home/user/proj");
        assert_eq!(parsed["data"]["cols"], 80);
        assert_eq!(parsed["data"]["rows"], 24);
        assert_eq!(parsed["data"]["source"], "ios");
    }

    #[test]
    fn test_outbound_session_created_with_msg_id_uses_string_s_prefixed_session_id() {
        let json = WsMessageBuilder::session_created_with_msg_id(
            "s_abc123def456",
            "claude",
            "/home/user/proj",
            Some("default"),
            52,
            18,
            "ios",
            Some("s_abc123def456-0"),
        );
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        let session_id = &parsed["data"]["sessionId"];

        assert_eq!(parsed["type"], "session_created");
        assert!(
            session_id.is_string(),
            "sessionId must be string, got: {session_id:?}"
        );
        assert_eq!(session_id.as_str().unwrap(), "s_abc123def456");
        assert!(session_id.as_str().unwrap().starts_with("s_"));
        assert!(parsed["data"].get("to_session_id").is_none());
        assert_eq!(parsed["data"]["msgId"], "s_abc123def456-0");
    }

    #[test]
    fn test_outbound_output() {
        let json = WsMessageBuilder::output("s_abc123", "hello\x1b[0m");
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["type"], "output");
        assert_eq!(parsed["data"]["sessionId"], "s_abc123");
        assert!(parsed["data"].get("to_session_id").is_none());
        assert_eq!(parsed["data"]["ansi_text"], "hello\x1b[0m");
    }

    #[test]
    fn test_outbound_replay_output_done() {
        let json = WsMessageBuilder::replay_output_done("s_abc123", "ok", 12345, 2, None);
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["type"], "replay_output_done");
        assert_eq!(parsed["data"]["sessionId"], "s_abc123");
        assert_eq!(parsed["data"]["status"], "ok");
        assert_eq!(parsed["data"]["bytes"], 12345);
        assert_eq!(parsed["data"]["chunks"], 2);
        assert!(parsed["data"]["message"].is_null());
    }

    #[test]
    fn test_outbound_replay_output_done_with_message() {
        let json =
            WsMessageBuilder::replay_output_done("s_abc123", "error", 0, 0, Some("read failed"));
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["type"], "replay_output_done");
        assert_eq!(parsed["data"]["sessionId"], "s_abc123");
        assert_eq!(parsed["data"]["status"], "error");
        assert_eq!(parsed["data"]["bytes"], 0);
        assert_eq!(parsed["data"]["chunks"], 0);
        assert_eq!(parsed["data"]["message"], "read failed");
    }

    #[test]
    fn test_output_message_to_session_id_is_string() {
        // 对齐新协议: sessionId 统一为 String
        let msg = WsMessageBuilder::output("s_abc123", "hello");
        let parsed: serde_json::Value = serde_json::from_str(&msg).unwrap();
        let tsid = &parsed["data"]["sessionId"];
        assert!(
            tsid.is_string(),
            "sessionId must be a string, got: {:?}",
            tsid
        );
        assert_eq!(tsid.as_str().unwrap(), "s_abc123");
    }

    #[test]
    fn test_parse_error_notify() {
        let json = serde_json::json!({
            "type": "error_notify",
            "ts": 1234567890,
            "data": {
                "code": "SESSION_LIMIT",
                "message": "Maximum 10 concurrent sessions allowed"
            }
        });
        let env: WsEnvelope = serde_json::from_value(json).unwrap();
        let msg = env.parse().unwrap();
        match msg {
            AgentIncoming::ErrorNotify { code, message } => {
                assert_eq!(code, "SESSION_LIMIT");
                assert_eq!(message, "Maximum 10 concurrent sessions allowed");
            }
            _ => panic!("expected ErrorNotify, got {:?}", msg),
        }
    }

    #[test]
    fn test_parse_error_notify_minimal() {
        // error_notify with minimal fields (server could send just a code)
        let json = serde_json::json!({
            "type": "error_notify",
            "data": {
                "code": "INTERNAL_ERROR"
            }
        });
        let env: WsEnvelope = serde_json::from_value(json).unwrap();
        let msg = env.parse().unwrap();
        match msg {
            AgentIncoming::ErrorNotify { code, message } => {
                assert_eq!(code, "INTERNAL_ERROR");
                assert!(message.is_empty());
            }
            _ => panic!("expected ErrorNotify"),
        }
    }
}
