//! WSS protocol tests — verify kn-agent message parsing and format alignment
//! with Java kn-cloud (KnWsHandler + MessageTypes).
//!
//! All tests run without external cloud services — they test the protocol
//! layer (parse, serialization, format) and use MockWssServer for integration.

mod mock_wss;

use kn_agent::proto::{AgentIncoming, WsEnvelope, WsMessageBuilder};

// ── Message parsing tests (all known types) ─────────────────

#[test]
fn test_all_incoming_types_parse_without_panic() {
    // Verify every known incoming message type parses without panic.
    // Aligns with Java MessageTypes.java 15 types + KnWsHandler switch cases.
    let test_cases: Vec<(&str, &str)> = vec![
        // Heartbeat
        (r#"{"type":"pong","data":{"ts":123}}"#, "pong"),
        // Connection
        (r#"{"type":"connected","data":{"ws_session_id":"x","node_id":null,"protocol_version":1}}"#, "connected"),
        // Session lifecycle (cloud forwards from iOS to agent)
        (r#"{"type":"start_session","data":{"tool":"bash","fromUserId":1}}"#, "start_session"),
        // Message routing (cloud forwards from iOS to agent) — sessionNid in envelope sessionId
        (r#"{"type":"input","sessionId":"s_abc","data":{"seq":1,"content":"hi","fromUserId":1}}"#, "input"),
        (r#"{"type":"ctrl","data":{"to_session_id":"s_abc123","signal":"ctrl_c"}}"#, "ctrl"),
        (r#"{"type":"resize","data":{"to_session_id":"s_abc123","cols":48,"rows":18}}"#, "resize"),
        // Server → agent
        (r#"{"type":"error_notify","data":{"code":"ERR","message":"test"}}"#, "error_notify"),
        (r#"{"type":"profile_list_ack"}"#, "profile_list_ack"),
        // Forward compat
        (r#"{"type":"future_type","data":{}}"#, "unknown"),
    ];

    for (json_str, expected_type) in test_cases {
        let env: WsEnvelope = serde_json::from_str(json_str)
            .unwrap_or_else(|e| panic!("Failed to parse '{}' envelope: {}", expected_type, e));
        let msg = env.parse()
            .unwrap_or_else(|e| panic!("Failed to parse '{}' message: {}", expected_type, e));

        let actual = variant_name(&msg);
        assert_eq!(
            actual, expected_type,
            "Expected {} but got {} for: {}",
            expected_type, actual, json_str
        );
    }
}

fn variant_name(msg: &AgentIncoming) -> &'static str {
    match msg {
        AgentIncoming::Pong { .. } => "pong",
        AgentIncoming::Connected { .. } => "connected",
        AgentIncoming::StartSession { .. } => "start_session",
        AgentIncoming::Input { .. } => "input",
        AgentIncoming::Ctrl { .. } => "ctrl",
        AgentIncoming::Resize { .. } => "resize",
        AgentIncoming::ErrorNotify { .. } => "error_notify",
        AgentIncoming::ProfileListAck => "profile_list_ack",
        AgentIncoming::Unknown { .. } => "unknown",
    }
}

// ── Output message format tests (align with new String sessionNid protocol) ──────

#[test]
fn test_output_to_session_id_is_string_session_nid() {
    // 新协议: to_session_id 统一为 String (sessionNid)
    let msg = WsMessageBuilder::output("s_abc123", "hello world");
    let parsed: serde_json::Value = serde_json::from_str(&msg).unwrap();

    assert_eq!(parsed["type"], "output");
    let tsid = &parsed["data"]["to_session_id"];
    assert!(tsid.is_string(), "to_session_id must be a string (sessionNid), got {:?}", tsid);
    assert_eq!(tsid.as_str().unwrap(), "s_abc123");
    // Verify ansi_text field is present
    assert_eq!(parsed["data"]["ansi_text"], "hello world");
}

#[test]
fn test_output_format_matches_new_protocol() {
    // Full format verification:
    // - type: "output"
    // - data.to_session_id: String (sessionNid)
    // - data.ansi_text: String
    let msg = WsMessageBuilder::output("s_def456", "test\x1b[0m");
    let v: serde_json::Value = serde_json::from_str(&msg).unwrap();

    // Envelope-level type
    assert_eq!(v["type"], "output");
    // data fields
    assert!(v["data"]["to_session_id"].is_string());
    assert!(v["data"]["ansi_text"].is_string());
}

// ── All outbound builder format tests ───────────────────────

#[test]
fn test_all_outbound_builders_produce_valid_json() {
    // Verify all allowed agent outbound message types produce valid JSON
    // with correct "type" field. Aligns with Java ALLOWED_MESSAGES:
    //   kn-agent: ping, session_created, session_ended, output, profile_list, session_interrupted

    // ping
    let ping = WsMessageBuilder::ping();
    let v: serde_json::Value = serde_json::from_str(&ping).unwrap();
    assert_eq!(v["type"], "ping");

    // session_created
    let created = WsMessageBuilder::session_created("s_abc", "claude", "/tmp", None, 80, 24);
    let v: serde_json::Value = serde_json::from_str(&created).unwrap();
    assert_eq!(v["type"], "session_created");
    assert_eq!(v["data"]["sessionId"], "s_abc");
    assert_eq!(v["data"]["tool"], "claude");

    // session_ended
    let ended = WsMessageBuilder::session_ended("s_abc", "user_disconnected");
    let v: serde_json::Value = serde_json::from_str(&ended).unwrap();
    assert_eq!(v["type"], "session_ended");
    assert_eq!(v["data"]["sessionId"], "s_abc");
    assert_eq!(v["data"]["reason"], "user_disconnected");

    // output
    let output = WsMessageBuilder::output("s_abc", "ansi text");
    let v: serde_json::Value = serde_json::from_str(&output).unwrap();
    assert_eq!(v["type"], "output");
    assert_eq!(v["data"]["to_session_id"], "s_abc");
    assert_eq!(v["data"]["ansi_text"], "ansi text");

    // sessions_interrupted
    let interrupted = vec![kn_agent::proto::InterruptedSession {
        nid: "s_abc".into(),
        tool: "claude".into(),
        profile: Some("work".into()),
        cwd: "/tmp".into(),
        last_input: "help".into(),
        last_output_snippet: "sure".into(),
    }];
    let msg = WsMessageBuilder::sessions_interrupted(&interrupted);
    let v: serde_json::Value = serde_json::from_str(&msg).unwrap();
    assert_eq!(v["type"], "session_interrupted");
    let arr = v["data"]["sessions"].as_array().unwrap();
    assert_eq!(arr.len(), 1);
    assert_eq!(arr[0]["nid"], "s_abc");
    assert_eq!(arr[0]["tool"], "claude");
    // camelCase field names (Java uses camelCase via Jackson)
    assert_eq!(arr[0]["lastInput"], "help");
    assert_eq!(arr[0]["lastOutputSnippet"], "sure");
}

#[test]
fn test_session_ended_builder_matches_java_expectations() {
    let msg = WsMessageBuilder::session_ended("s_abc123", "process_exit");
    let v: serde_json::Value = serde_json::from_str(&msg).unwrap();

    assert_eq!(v["type"], "session_ended");
    assert!(v["data"]["sessionId"].is_string());
    assert_eq!(v["data"]["sessionId"], "s_abc123");
    assert_eq!(v["data"]["reason"], "process_exit");
}

// ── error_notify parsing tests ──────────────────────────────

#[test]
fn test_error_notify_with_full_data() {
    let json = serde_json::json!({
        "type": "error_notify",
        "data": {
            "code": "SESSION_LIMIT",
            "message": "Maximum 10 concurrent sessions allowed"
        }
    });
    let env: WsEnvelope = serde_json::from_value(json).unwrap();
    match env.parse().unwrap() {
        AgentIncoming::ErrorNotify { code, message } => {
            assert_eq!(code, "SESSION_LIMIT");
            assert_eq!(message, "Maximum 10 concurrent sessions allowed");
        }
        other => panic!("expected ErrorNotify, got {:?}", other),
    }
}

#[test]
fn test_error_notify_with_minimal_data() {
    let json = serde_json::json!({
        "type": "error_notify",
        "data": {
            "code": "INTERNAL_ERROR"
        }
    });
    let env: WsEnvelope = serde_json::from_value(json).unwrap();
    match env.parse().unwrap() {
        AgentIncoming::ErrorNotify { code, message } => {
            assert_eq!(code, "INTERNAL_ERROR");
            assert!(message.is_empty());
        }
        other => panic!("expected ErrorNotify, got {:?}", other),
    }
}

// ── start_session parsing (Java forward format) ─────────────

#[test]
fn test_start_session_parsing_matches_java_forward_format() {
    // Java WsMessageFactory.startSessionForward builds:
    // {"type":"start_session","ts":...,"sessionId":"s_nanoid",
    //  "data":{"sessionId":DB_ID,"sessionNid":"s_...","tool":"...","profile":"...",
    //          "cwd":"...","fromUserId":...}}
    let json = serde_json::json!({
        "type": "start_session",
        "ts": 1234567890i64,
        "sessionId": "s_abc123def456",
        "data": {
            "sessionId": 42,
            "sessionNid": "s_abc123def456",
            "tool": "claude",
            "profile": "work",
            "cwd": "/Users/test/project",
            "fromUserId": 100,
            "cols": 48,
            "rows": 18
        }
    });
    let env: WsEnvelope = serde_json::from_value(json).unwrap();
    match env.parse().unwrap() {
        AgentIncoming::StartSession {
            tool,
            profile,
            cwd,
            from_user_id,
            cols,
            rows,
        } => {
            assert_eq!(tool, "claude");
            assert_eq!(profile, Some("work".into()));
            assert_eq!(cwd, Some("/Users/test/project".into()));
            assert_eq!(from_user_id, 100);
            assert_eq!(cols, 48);
            assert_eq!(rows, 18);
        }
        other => panic!("expected StartSession, got {:?}", other),
    }
}

// ── input parsing (new protocol: sessionNid in envelope) ────

#[test]
fn test_input_parsing_matches_new_protocol() {
    // 新协议: sessionNid 在 envelope 级 sessionId，不再有 data.sessionId
    let json = serde_json::json!({
        "type": "input",
        "sessionId": "s_abc",
        "ts": 1234567890i64,
        "data": {
            "seq": 5,
            "content": "hello world",
            "fromUserId": 100
        }
    });
    let env: WsEnvelope = serde_json::from_value(json).unwrap();
    match env.parse().unwrap() {
        AgentIncoming::Input {
            session_nid,
            seq,
            content,
            ..
        } => {
            assert_eq!(session_nid, "s_abc");
            assert_eq!(seq, 5);
            assert_eq!(content, "hello world");
        }
        other => panic!("expected Input, got {:?}", other),
    }
}

// ── ctrl parsing (new protocol: String to_session_id) ───────

#[test]
fn test_ctrl_parsing_matches_new_protocol() {
    // 新协议: to_session_id 为 String (sessionNid)
    let json = serde_json::json!({
        "type": "ctrl",
        "ts": 1234567890i64,
        "data": {
            "to_session_id": "s_abc123",
            "signal": "ctrl_c"
        }
    });
    let env: WsEnvelope = serde_json::from_value(json).unwrap();
    match env.parse().unwrap() {
        AgentIncoming::Ctrl {
            session_nid,
            signal,
        } => {
            assert_eq!(session_nid, "s_abc123");
            assert_eq!(signal["signal"], "ctrl_c");
        }
        other => panic!("expected Ctrl, got {:?}", other),
    }
}

// ── Unknown message type handling ───────────────────────────

#[test]
fn test_unknown_type_is_not_an_error() {
    // Forward-compat: unknown types should NOT panic or error,
    // they should be logged and ignored.
    let json = serde_json::json!({
        "type": "future_protocol_v2_feature",
        "ts": 1234567890i64,
        "data": {"some": "field"}
    });
    let env: WsEnvelope = serde_json::from_value(json).unwrap();
    let msg = env.parse().unwrap();
    match msg {
        AgentIncoming::Unknown { msg_type, .. } => {
            assert_eq!(msg_type, "future_protocol_v2_feature");
        }
        other => panic!("expected Unknown, got {:?}", other),
    }
}

// ── ProfileInfo conversion ──────────────────────────────────

#[test]
fn test_profile_info_from_summary() {
    use kn_common::profile::ProfileSummary;
    let summary = ProfileSummary {
        name: "my-claude".into(),
        desc: "Work Claude profile".into(),
        env_count: 3,
        is_default: false,
        cli_type: Some("claude".into()),
        tags: None,
    };
    let info: kn_agent::proto::ProfileInfo = (&summary).into();
    assert_eq!(info.name, "my-claude");
    assert_eq!(info.tool, Some("claude".into()));
    assert_eq!(info.description, "Work Claude profile");
}
