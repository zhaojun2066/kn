use kn_agent::health::{ConnectionHealth, HealthSnapshot, ToolHealth, ToolHealthState};

#[test]
fn snapshot_serialization_contains_only_the_public_health_contract() {
    let snapshot = HealthSnapshot::new_for_test(
        "1.2.3",
        "development",
        ConnectionHealth::connected(),
        vec![
            ToolHealth::available("git", Some("2.45.1".to_string())),
            ToolHealth::unavailable("gh", ToolHealthState::NotAuthenticated),
        ],
    );

    let json = serde_json::to_value(snapshot).expect("health snapshot should serialize");
    assert_eq!(json["schemaVersion"], 1);
    assert_eq!(json["agent"]["version"], "1.2.3");
    assert_eq!(json["agent"]["environment"], "development");
    assert_eq!(json["connection"]["state"], "connected");
    assert_eq!(json["tools"][0]["name"], "git");
    assert_eq!(json["tools"][1]["state"], "notAuthenticated");
    assert!(json.get("token").is_none());
    assert!(json.get("path").is_none());
    assert!(json.get("stdout").is_none());
    assert!(json.get("stderr").is_none());
    assert!(json.get("cloudUrl").is_none());
}

#[test]
fn tool_health_never_serializes_raw_probe_details() {
    let tool = ToolHealth::from_probe_failure(
        "gh",
        ToolHealthState::Error,
        "https://token:secret@github.com/org/private.git\ncommand output",
    );

    let json = serde_json::to_string(&tool).expect("tool health should serialize");
    assert!(json.contains("\"name\":\"gh\""));
    assert!(json.contains("\"state\":\"error\""));
    assert!(!json.contains("secret"));
    assert!(!json.contains("private.git"));
    assert!(!json.contains("command output"));
}

#[test]
fn cached_snapshot_refreshes_connection_without_mutating_tool_results() {
    let snapshot = HealthSnapshot::new_for_test(
        "1.2.3",
        "development",
        ConnectionHealth::connected(),
        vec![ToolHealth::available("git", Some("2.45.1".to_string()))],
    );

    let refreshed = snapshot.with_connection(ConnectionHealth::from_agent_state("reconnecting"));
    let json = serde_json::to_value(refreshed).expect("health snapshot should serialize");
    assert_eq!(json["connection"]["state"], "reconnecting");
    assert_eq!(json["tools"][0]["version"], "2.45.1");
}
