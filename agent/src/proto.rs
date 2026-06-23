//! WSS 消息协议类型 — 两阶段反序列化匹配 kn-cloud 信封格式。
//!
//! 权威来源：`kn-cloud/kn-cloud-ws/.../MessageTypes.java` (15 种消息类型)
//!            + `KnWsHandler.java` 角色白名单 (ALLOWED_MESSAGES)
//!
//! ```text
//! 信封: {"type": "...", "ts": <epoch_ms>, "sessionId"?: "s_nanoid", "data": {...}}
//! ```
//!
//! ## Agent 出站 (agent → cloud) — 6 种，对齐 Java ALLOWED_MESSAGES:
//! - ping, session_created, session_ended, output, profile_list, session_interrupted
//!
//! ## Agent 入站 (cloud → agent):
//! - 心跳: pong
//! - 连接: connected
//! - 会话 (iOS→cloud→agent 转发): start_session, input, ctrl
//! - 确认: profile_list_ack
//! - 错误: error_notify (Java sendError 可向任意客户端发送)
//!
//! 不在 agent 白名单/不需要 agent 关注的消息:
//! - start_session_ack, ack (仅 mobile)
//! - kill_session, agent_error (Java 代码中未实现)

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
    StartSession {
        /// 云端的 DB 会话 ID（Long）
        db_session_id: i64,
        /// 会话 nanoid（s_ + 12 字符）
        session_nid: String,
        /// CLI 工具（claude/codex/qoder/bash）
        tool: String,
        /// Profile 名称
        profile: Option<String>,
        /// 工作目录
        cwd: Option<String>,
        /// 发起用户 ID
        from_user_id: u64,
    },
    /// 用户输入文本
    Input {
        db_session_id: i64,
        seq: u64,
        content: String,
        from_user_id: u64,
    },
    /// 控制信号（Ctrl+C、Ctrl+D 等）
    Ctrl {
        db_session_id: i64,
        signal: serde_json::Value,
    },
    /// 云端错误通知（对齐 Java MessageTypes.ERROR_NOTIFY + sendError()）
    ErrorNotify {
        code: String,
        message: String,
    },
    /// 配置文件列表确认
    ProfileListAck,
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
                Ok(AgentIncoming::StartSession {
                    db_session_id: data["sessionId"].as_i64().unwrap_or(0),
                    session_nid: self
                        .session_id
                        .clone()
                        .or_else(|| data["sessionNid"].as_str().map(String::from))
                        .unwrap_or_default(),
                    tool: data["tool"].as_str().unwrap_or("bash").to_string(),
                    profile: data["profile"].as_str().map(String::from),
                    cwd: data["cwd"].as_str().map(String::from),
                    from_user_id: data["fromUserId"].as_u64().unwrap_or(0),
                })
            }
            "input" => {
                let data = self
                    .data
                    .as_ref()
                    .ok_or_else(|| "input 缺少 data 字段".to_string())?;
                Ok(AgentIncoming::Input {
                    db_session_id: data["sessionId"].as_i64().unwrap_or(0),
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
                // Cloud forwards ctrl with to_session_id (not sessionId)
                Ok(AgentIncoming::Ctrl {
                    db_session_id: data["to_session_id"].as_i64().unwrap_or(0),
                    signal: data.clone(),
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

    /// 会话创建确认。
    pub fn session_created(db_session_id: i64) -> String {
        serde_json::json!({
            "type": "session_created",
            "data": { "sessionId": db_session_id }
        })
        .to_string()
    }

    /// 会话结束通知。
    pub fn session_ended(db_session_id: i64, reason: &str) -> String {
        serde_json::json!({
            "type": "session_ended",
            "data": {
                "sessionId": db_session_id,
                "reason": reason
            }
        })
        .to_string()
    }

    /// PTY 输出数据。
    pub fn output(to_session_id: i64, ansi_text: &str) -> String {
        serde_json::json!({
            "type": "output",
            "data": {
                "to_session_id": to_session_id,
                "ansi_text": ansi_text
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
            "sessionId": "s_abc123def456",
            "data": {
                "sessionId": 42,
                "sessionNid": "s_abc123def456",
                "tool": "claude",
                "profile": "my-profile",
                "cwd": "/Users/test/project",
                "fromUserId": 100
            }
        });
        let env: WsEnvelope = serde_json::from_value(json).unwrap();
        let msg = env.parse().unwrap();
        match msg {
            AgentIncoming::StartSession {
                db_session_id,
                session_nid,
                tool,
                profile,
                cwd,
                from_user_id,
            } => {
                assert_eq!(db_session_id, 42);
                assert_eq!(session_nid, "s_abc123def456");
                assert_eq!(tool, "claude");
                assert_eq!(profile, Some("my-profile".into()));
                assert_eq!(cwd, Some("/Users/test/project".into()));
                assert_eq!(from_user_id, 100);
            }
            _ => panic!("expected StartSession"),
        }
    }

    #[test]
    fn test_parse_input() {
        let json = serde_json::json!({
            "type": "input",
            "sessionId": "s_abc",
            "data": {
                "sessionId": 42,
                "seq": 5,
                "content": "hello world",
                "fromUserId": 100
            }
        });
        let env: WsEnvelope = serde_json::from_value(json).unwrap();
        let msg = env.parse().unwrap();
        match msg {
            AgentIncoming::Input {
                db_session_id,
                seq,
                content,
                from_user_id,
            } => {
                assert_eq!(db_session_id, 42);
                assert_eq!(seq, 5);
                assert_eq!(content, "hello world");
                assert_eq!(from_user_id, 100);
            }
            _ => panic!("expected Input"),
        }
    }

    #[test]
    fn test_outbound_ping() {
        let json = WsMessageBuilder::ping();
        assert!(json.contains("ping"));
    }

    #[test]
    fn test_outbound_session_created() {
        let json = WsMessageBuilder::session_created(42);
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["type"], "session_created");
        assert_eq!(parsed["data"]["sessionId"], 42);
    }

    #[test]
    fn test_outbound_output() {
        let json = WsMessageBuilder::output(42, "hello\x1b[0m");
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["type"], "output");
        assert_eq!(parsed["data"]["to_session_id"], 42);
        assert_eq!(parsed["data"]["ansi_text"], "hello\x1b[0m");
    }

    #[test]
    fn test_output_message_to_session_id_is_number() {
        // 对齐 Java handleOutput: Long sessionId = data.get("to_session_id").asLong();
        let msg = WsMessageBuilder::output(42, "hello");
        let parsed: serde_json::Value = serde_json::from_str(&msg).unwrap();
        let tsid = &parsed["data"]["to_session_id"];
        assert!(tsid.is_number(), "to_session_id must be a number, got: {:?}", tsid);
        assert!(!tsid.is_string(), "to_session_id must NOT be a string");
        assert_eq!(tsid.as_i64().unwrap(), 42);
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
