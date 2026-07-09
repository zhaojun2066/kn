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
        (
            r#"{"type":"connected","data":{"ws_session_id":"x","node_id":null,"protocol_version":1}}"#,
            "connected",
        ),
        // Session lifecycle (cloud forwards from iOS to agent)
        (
            r#"{"type":"start_session","data":{"profile":"default","fromUserId":1}}"#,
            "start_session",
        ),
        // Message routing (cloud forwards from iOS to agent) — sessionId identifies the session
        (
            r#"{"type":"input","data":{"sessionId":"s_abc","seq":1,"content":"hi","fromUserId":1}}"#,
            "input",
        ),
        (
            r#"{"type":"ctrl","data":{"sessionId":"s_abc123","signal":"ctrl_c"}}"#,
            "ctrl",
        ),
        (
            r#"{"type":"resize","data":{"sessionId":"s_abc123","cols":48,"rows":18}}"#,
            "resize",
        ),
        (
            r#"{"type":"kill_session","data":{"sessionId":"s_abc123","reason":"user_closed_tab"}}"#,
            "kill_session",
        ),
        // Server → agent
        (
            r#"{"type":"error_notify","data":{"code":"ERR","message":"test"}}"#,
            "error_notify",
        ),
        (r#"{"type":"profile_list_ack"}"#, "profile_list_ack"),
        // New: session_created_ack
        (
            r#"{"type":"session_created_ack","data":{"sessionId":"s_abc123","status":"ok"}}"#,
            "session_created_ack",
        ),
        (
            r#"{"type":"session_created_ack","data":{"sessionId":"s_def456","status":"error","error":"Redis timeout"}}"#,
            "session_created_ack",
        ),
        // New: resume_session
        (
            r#"{"type":"replay_output","data":{"sessionId":"s_abc123"}}"#,
            "replay_output",
        ),
        (
            r#"{"type":"resume_session","data":{"sessionId":"s_abc123"}}"#,
            "resume_session",
        ),
        (
            r#"{"type":"change_summary","data":{"sessionId":"s_abc123"}}"#,
            "change_summary",
        ),
        (
            r#"{"type":"change_file_diff","data":{"sessionId":"s_abc123","path":"Sources/App.swift"}}"#,
            "change_file_diff",
        ),
        (
            r#"{"type":"verify_changes","data":{"sessionId":"s_abc123","environment":"default","target":"all"}}"#,
            "verify_changes",
        ),
        (
            r#"{"type":"verify_plan","data":{"sessionId":"s_abc123","environment":"default"}}"#,
            "verify_plan",
        ),
        (
            r#"{"type":"cancel_verify_changes","data":{"sessionId":"s_abc123","runId":"v_1"}}"#,
            "cancel_verify_changes",
        ),
        // Forward compat
        (r#"{"type":"future_type","data":{}}"#, "unknown"),
    ];

    for (json_str, expected_type) in test_cases {
        let env: WsEnvelope = serde_json::from_str(json_str)
            .unwrap_or_else(|e| panic!("Failed to parse '{}' envelope: {}", expected_type, e));
        let msg = env
            .parse()
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
        AgentIncoming::ReplayOutput { .. } => "replay_output",
        AgentIncoming::SessionCreatedAck { .. } => "session_created_ack",
        AgentIncoming::ResumeSession { .. } => "resume_session",
        AgentIncoming::KillSession { .. } => "kill_session",
        AgentIncoming::ChangeSummary { .. } => "change_summary",
        AgentIncoming::ChangeFileDiff { .. } => "change_file_diff",
        AgentIncoming::VerifyChanges { .. } => "verify_changes",
        AgentIncoming::VerifyPlan { .. } => "verify_plan",
        AgentIncoming::CancelVerifyChanges { .. } => "cancel_verify_changes",
        AgentIncoming::VerifyStatus { .. } => "verify_status",
        AgentIncoming::VerifyLogWindow { .. } => "verify_log_window",
        AgentIncoming::VerifyLogIssues { .. } => "verify_log_issues",
        AgentIncoming::Unknown { .. } => "unknown",
    }
}

// ── Output message format tests (align with sessionId protocol) ──────

#[test]
fn test_output_session_id_is_string() {
    // 新协议: sessionId 统一为 String
    let msg = WsMessageBuilder::output("s_abc123", "hello world");
    let parsed: serde_json::Value = serde_json::from_str(&msg).unwrap();

    assert_eq!(parsed["type"], "output");
    let tsid = &parsed["data"]["sessionId"];
    assert!(
        tsid.is_string(),
        "sessionId must be a string, got {:?}",
        tsid
    );
    assert_eq!(tsid.as_str().unwrap(), "s_abc123");
    assert!(parsed["data"].get("to_session_id").is_none());
    // Verify ansi_text field is present
    assert_eq!(parsed["data"]["ansi_text"], "hello world");
}

#[test]
fn test_input_reads_session_id_from_data_only() {
    let json = r#"{"type":"input","sessionId":"s_wrong","data":{"sessionId":"s_data","seq":7,"content":"hi","fromUserId":1}}"#;
    let env: WsEnvelope = serde_json::from_str(json).unwrap();
    match env.parse().unwrap() {
        AgentIncoming::Input {
            session_nid,
            seq,
            content,
            from_user_id,
        } => {
            assert_eq!(session_nid, "s_data");
            assert_eq!(seq, 7);
            assert_eq!(content, "hi");
            assert_eq!(from_user_id, 1);
        }
        other => panic!("expected Input, got {:?}", other),
    }
}

#[test]
fn test_input_rejects_missing_data_session_id() {
    let json =
        r#"{"type":"input","sessionId":"s_top","data":{"seq":7,"content":"hi","fromUserId":1}}"#;
    let env: WsEnvelope = serde_json::from_str(json).unwrap();

    assert!(env.parse().is_err());
}

#[test]
fn test_output_format_matches_new_protocol() {
    // Full format verification:
    // - type: "output"
    // - data.sessionId: String
    // - data.ansi_text: String
    let msg = WsMessageBuilder::output("s_def456", "test\x1b[0m");
    let v: serde_json::Value = serde_json::from_str(&msg).unwrap();

    // Envelope-level type
    assert_eq!(v["type"], "output");
    // data fields
    assert!(v["data"]["sessionId"].is_string());
    assert!(v["data"]["ansi_text"].is_string());
    assert!(v["data"].get("to_session_id").is_none());
}

#[test]
fn test_cli_heartbeat_uses_session_id() {
    let sessions = vec![kn_agent::proto::HeartbeatSession {
        session_nid: "s_live123".into(),
        pid: 4242,
        state: "running".into(),
    }];
    let msg = WsMessageBuilder::cli_heartbeat(&sessions);
    let v: serde_json::Value = serde_json::from_str(&msg).unwrap();

    assert_eq!(v["type"], "cli_heartbeat");
    let session = &v["data"]["sessions"][0];
    assert_eq!(session["sessionId"], "s_live123");
    assert!(session.get("sessionNid").is_none());
    assert_eq!(session["pid"], 4242);
    assert_eq!(session["state"], "running");
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
    let created = WsMessageBuilder::session_created("s_abc", "claude", "/tmp", None, 80, 24, "ios");
    let v: serde_json::Value = serde_json::from_str(&created).unwrap();
    assert_eq!(v["type"], "session_created");
    assert_eq!(v["data"]["sessionId"], "s_abc");
    assert_eq!(v["data"]["tool"], "claude");
    assert_eq!(v["data"]["source"], "ios");

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
    assert_eq!(v["data"]["sessionId"], "s_abc");
    assert!(v["data"].get("to_session_id").is_none());
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
    // {"type":"start_session","ts":...,
    //  "data":{"profile":"...","cwd":"...","fromUserId":...}}
    let json = serde_json::json!({
        "type": "start_session",
        "ts": 1234567890i64,
        "data": {
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
            profile,
            cwd,
            from_user_id,
            cols,
            rows,
        } => {
            assert_eq!(profile, "work");
            assert_eq!(cwd, Some("/Users/test/project".into()));
            assert_eq!(from_user_id, 100);
            assert_eq!(cols, 48);
            assert_eq!(rows, 18);
        }
        other => panic!("expected StartSession, got {:?}", other),
    }
}

#[test]
fn test_start_session_requires_profile() {
    let json = serde_json::json!({
        "type": "start_session",
        "data": {
            "cwd": "/Users/test/project",
            "fromUserId": 100
        }
    });
    let env: WsEnvelope = serde_json::from_value(json).unwrap();
    let err = env.parse().unwrap_err();
    assert!(err.contains("profile"));
}

// ── input parsing (new protocol: sessionId in data) ────

#[test]
fn test_input_parsing_matches_new_protocol() {
    // 新协议: sessionId 在 data 内
    let json = serde_json::json!({
        "type": "input",
        "ts": 1234567890i64,
        "data": {
            "sessionId": "s_abc",
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

// ── ctrl parsing (new protocol: String sessionId) ───────

#[test]
fn test_ctrl_parsing_matches_new_protocol() {
    // 新协议: sessionId 为 String
    let json = serde_json::json!({
        "type": "ctrl",
        "ts": 1234567890i64,
        "data": {
            "sessionId": "s_abc123",
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
