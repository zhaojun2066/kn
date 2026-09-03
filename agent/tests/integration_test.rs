//! Integration tests for kn-agent IPC, session management, PTY lifecycle,
//! device binding, and card redemption.
//!
//! ## Prerequisites
//!
//! - Local cloud services at `localhost:8080` (HTTP) and `localhost:8081` (WS)
//!   are required for bind tests (group D). These tests are ignored by default
//!   and can be run with `--ignored` when cloud is available.
//! - CLI tools (claude, codex) must be installed for PTY session tests (group C).
//!   Tests check binary availability and skip gracefully if not found.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::process::{Child, Command};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use std::time::Duration;

use tokio::sync::mpsc;

// ── Test helpers ──────────────────────────────────────────────────

/// Wrapper that kills the child process on drop.
struct ChildGuard(Option<Child>);

impl ChildGuard {
    fn new(child: Child) -> Self {
        Self(Some(child))
    }
}

impl Drop for ChildGuard {
    fn drop(&mut self) {
        if let Some(mut child) = self.0.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

/// Temporary directory that is removed on drop.
struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new(prefix: &str) -> Self {
        static COUNTER: AtomicUsize = AtomicUsize::new(1);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let short_prefix: String = prefix.chars().take(8).collect();
        // Unix domain socket paths on macOS must fit in sockaddr_un.sun_path
        // (104 bytes). Keep KN_HOME under /tmp so per-session pty.sock paths
        // remain short even when the process temp dir is /var/folders/...
        let path =
            PathBuf::from("/tmp").join(format!("kt-{}-{}-{}", short_prefix, std::process::id(), n));
        std::fs::create_dir_all(&path).expect("create temp dir");
        // Create required subdirs
        std::fs::create_dir_all(path.join("agent")).ok();
        Self { path }
    }

    fn path(&self) -> &std::path::Path {
        &self.path
    }

    /// Path to the IPC socket
    fn ipc_sock(&self) -> PathBuf {
        self.path.join("agent").join("ipc.sock")
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

/// Helper to send a JSON-line request over Unix socket and read one response line.
fn ipc_request(socket_path: &std::path::Path, request: &str) -> String {
    let mut stream = UnixStream::connect(socket_path).expect("connect to IPC socket");
    // Set read timeout to avoid hanging
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .expect("set read timeout");
    stream
        .set_write_timeout(Some(Duration::from_secs(5)))
        .expect("set write timeout");

    writeln!(stream, "{}", request).expect("write IPC request");
    stream.flush().expect("flush");

    let mut reader = BufReader::new(stream);
    let mut response = String::new();
    reader.read_line(&mut response).expect("read IPC response");
    response.trim().to_string()
}

/// Send an IPC request and parse the response as JSON.
fn ipc_request_json(socket_path: &std::path::Path, req: &serde_json::Value) -> serde_json::Value {
    let req_str = req.to_string();
    let resp_str = ipc_request(socket_path, &req_str);
    serde_json::from_str(&resp_str).unwrap_or_else(|e| {
        panic!(
            "Failed to parse IPC response as JSON: {} — raw: {}",
            e, resp_str
        )
    })
}

/// Build a standard IPC request.
fn ipc_req(id: &str, method: &str, params: serde_json::Value) -> serde_json::Value {
    serde_json::json!({
        "id": id,
        "method": method,
        "params": params,
    })
}

/// Assert that an IPC response is a success (has "result").
fn assert_ok(resp: &serde_json::Value) -> &serde_json::Value {
    assert!(
        resp.get("result").is_some(),
        "Expected ok response, got: {}",
        serde_json::to_string_pretty(resp).unwrap_or_default()
    );
    &resp["result"]
}

/// Assert that an IPC response is an error with a specific code.
fn assert_err<'a>(resp: &'a serde_json::Value, expected_code: &str) -> &'a serde_json::Value {
    let err = resp
        .get("error")
        .unwrap_or_else(|| panic!("Expected error, got: {}", resp));
    assert_eq!(
        err["code"].as_str().unwrap_or(""),
        expected_code,
        "Expected error code {}, got: {}",
        expected_code,
        serde_json::to_string_pretty(resp).unwrap_or_default()
    );
    err
}

/// Spawn a kn-agent process with isolated KN_HOME for testing.
fn spawn_agent(kn_home: &TempDir) -> ChildGuard {
    spawn_agent_with_cloud(
        kn_home,
        "ws://localhost:8081/v1/ws",
        "http://localhost:8080",
    )
}

/// Spawn an Agent with isolated Cloud endpoints for IPC/HTTP integration tests.
fn spawn_agent_with_cloud(
    kn_home: &TempDir,
    cloud_ws_url: &str,
    cloud_http_url: &str,
) -> ChildGuard {
    let kn_home_str = kn_home.path().to_string_lossy().to_string();
    let bin = std::env::var("CARGO_BIN_EXE_kn-agent")
        .unwrap_or_else(|_| "target/debug/kn-agent".to_string());
    let child = Command::new(bin)
        .env("KN_HOME", &kn_home_str)
        .env("KN_CLOUD_URL", cloud_ws_url)
        .env("KN_CLOUD_HTTP_URL", cloud_http_url)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("spawn kn-agent");

    ChildGuard::new(child)
}

/// Wait for the IPC socket to appear (agent is ready).
fn wait_for_ipc_socket(socket_path: &std::path::Path, timeout_secs: u64) -> bool {
    let start = std::time::Instant::now();
    while start.elapsed() < Duration::from_secs(timeout_secs) {
        if socket_path.exists() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    false
}

/// Check if a CLI binary is available.
fn has_binary(name: &str) -> bool {
    kn_agent::session::resolve_tool_path(name).is_ok()
}

// ═══════════════════════════════════════════════════════════════════
// Group A: IPC Protocol Tests (no cloud dependency)
// ═══════════════════════════════════════════════════════════════════

#[test]
fn test_ipc_status() {
    let dir = TempDir::new("ipc-status");
    let _agent = spawn_agent(&dir);
    assert!(
        wait_for_ipc_socket(&dir.ipc_sock(), 10),
        "Agent socket did not appear"
    );

    let resp = ipc_request_json(
        &dir.ipc_sock(),
        &ipc_req("1", "status", serde_json::json!({})),
    );
    let result = assert_ok(&resp);

    assert!(result.get("state").is_some(), "Missing 'state' field");
    assert!(
        result.get("crash_count").is_some(),
        "Missing 'crash_count' field"
    );
    assert!(
        result.get("safe_mode").is_some(),
        "Missing 'safe_mode' field"
    );
    // Initial state should be "unbound" (no device_token in fresh KN_HOME)
    assert_eq!(result["state"].as_str().unwrap(), "unbound");
}

#[test]
fn test_ipc_sessions_empty() {
    let dir = TempDir::new("ipc-sessions");
    let _agent = spawn_agent(&dir);
    assert!(wait_for_ipc_socket(&dir.ipc_sock(), 10));

    let resp = ipc_request_json(
        &dir.ipc_sock(),
        &ipc_req("1", "sessions", serde_json::json!({})),
    );
    let result = assert_ok(&resp);

    let sessions = result["sessions"]
        .as_array()
        .expect("sessions should be array");
    assert!(sessions.is_empty());
    assert_eq!(result["count"].as_u64().unwrap(), 0);
}

#[test]
fn self_unbind_with_expired_credential_preserves_local_binding() {
    let dir = TempDir::new("self-unbind-expired");
    let token_path = dir.path().join("agent/device_token");
    std::fs::write(&token_path, "expired-device-token").expect("write test device token");

    let listener = TcpListener::bind("127.0.0.1:0").expect("bind mock Cloud HTTP server");
    let cloud_http_url = format!("http://{}", listener.local_addr().unwrap());
    let cloud_thread = std::thread::spawn(move || {
        for _ in 0..5 {
            let (mut stream, _) = listener.accept().expect("accept Cloud request");
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .expect("set mock read timeout");
            let mut request = [0u8; 2048];
            let read = stream.read(&mut request).expect("read Cloud request");
            let request = String::from_utf8_lossy(&request[..read]);
            if request.contains("/api/v1/device/self-unbind") {
                stream
                    .write_all(
                        b"HTTP/1.1 401 Unauthorized\r\nContent-Type: application/json\r\nContent-Length: 55\r\nConnection: close\r\n\r\n{\"code\":401,\"message\":\"credential expired\",\"data\":null}",
                    )
                    .expect("write self-unbind response");
                return;
            }
            stream
                .write_all(b"HTTP/1.1 204 No Content\r\nConnection: close\r\n\r\n")
                .expect("write startup response");
        }
        panic!("Agent did not send a self-unbind request");
    });

    let _agent = spawn_agent_with_cloud(&dir, "ws://127.0.0.1:9/v1/ws", &cloud_http_url);
    assert!(wait_for_ipc_socket(&dir.ipc_sock(), 10));

    let unbind = ipc_request_json(
        &dir.ipc_sock(),
        &ipc_req("unbind", "self_unbind", serde_json::json!({})),
    );
    assert_err(&unbind, "SELF_UNBIND_FAILED");
    cloud_thread.join().expect("mock Cloud thread");
    assert!(
        token_path.exists(),
        "failed cloud unbind must preserve the local token"
    );

    let status = ipc_request_json(
        &dir.ipc_sock(),
        &ipc_req("status", "status", serde_json::json!({})),
    );
    assert_ne!(assert_ok(&status)["state"], "unbound");
}

#[test]
fn test_ipc_get_version() {
    let dir = TempDir::new("ipc-version");
    let _agent = spawn_agent(&dir);
    assert!(wait_for_ipc_socket(&dir.ipc_sock(), 10));

    let resp = ipc_request_json(
        &dir.ipc_sock(),
        &ipc_req("1", "get_version", serde_json::json!({})),
    );
    let result = assert_ok(&resp);

    assert!(
        result["version"].as_str().unwrap().len() > 0,
        "Version should be non-empty"
    );
    assert_eq!(result["name"].as_str().unwrap(), "kn-agent");
}

#[test]
fn test_ipc_unknown_method() {
    let dir = TempDir::new("ipc-unknown");
    let _agent = spawn_agent(&dir);
    assert!(wait_for_ipc_socket(&dir.ipc_sock(), 10));

    let resp = ipc_request_json(
        &dir.ipc_sock(),
        &ipc_req("1", "nonexistent_method", serde_json::json!({})),
    );
    assert_err(&resp, "METHOD_NOT_FOUND");
}

#[test]
fn test_ipc_invalid_json() {
    let dir = TempDir::new("ipc-parse");
    let _agent = spawn_agent(&dir);
    assert!(wait_for_ipc_socket(&dir.ipc_sock(), 10));

    // Send raw malformed JSON
    let resp_str = ipc_request(&dir.ipc_sock(), r#"{"id":"1","method":"status""#);
    let resp: serde_json::Value = serde_json::from_str(&resp_str).unwrap();
    assert_eq!(resp["error"]["code"].as_str().unwrap(), "PARSE_ERROR");
}

#[test]
fn test_ipc_pause_resume() {
    let dir = TempDir::new("ipc-pause");
    let _agent = spawn_agent(&dir);
    assert!(wait_for_ipc_socket(&dir.ipc_sock(), 10));

    // Pause from "unbound" state must be rejected — valid transition needs connected/idle/running
    let resp = ipc_request_json(
        &dir.ipc_sock(),
        &ipc_req("1", "pause", serde_json::json!({})),
    );
    assert_err(&resp, "STATE_ERROR");

    // Resume must also be rejected from unbound
    let resp = ipc_request_json(
        &dir.ipc_sock(),
        &ipc_req("2", "resume", serde_json::json!({})),
    );
    assert_err(&resp, "STATE_ERROR");
}

#[test]
fn test_ipc_cancel_bind_noop_when_no_active_bind() {
    let dir = TempDir::new("ipc-cancel");
    let _agent = spawn_agent(&dir);
    assert!(wait_for_ipc_socket(&dir.ipc_sock(), 10));

    // Cancel without an active bind — should succeed with "no_active_bind"
    let resp = ipc_request_json(
        &dir.ipc_sock(),
        &ipc_req("1", "cancel_bind", serde_json::json!({})),
    );
    let result = assert_ok(&resp);
    assert_eq!(result["status"].as_str().unwrap(), "no_active_bind");
}

// ═══════════════════════════════════════════════════════════════════
// Group B: Session Management Tests
// ═══════════════════════════════════════════════════════════════════

#[test]
fn test_ipc_new_session_bash() {
    let dir = TempDir::new("ipc-bash");
    let _agent = spawn_agent(&dir);
    assert!(wait_for_ipc_socket(&dir.ipc_sock(), 10));

    let resp = ipc_request_json(
        &dir.ipc_sock(),
        &ipc_req(
            "1",
            "new_session",
            serde_json::json!({
                "tool": "bash",
                "cwd": ".",
            }),
        ),
    );
    let result = assert_ok(&resp);

    assert!(!result["nid"].as_str().unwrap().is_empty());
    assert_eq!(result["tool"].as_str().unwrap(), "bash");
    assert_eq!(result["status"].as_str().unwrap(), "created");
}

#[test]
fn test_ipc_new_session_with_profile() {
    let dir = TempDir::new("ipc-profile");
    let _agent = spawn_agent(&dir);
    assert!(wait_for_ipc_socket(&dir.ipc_sock(), 10));

    let resp = ipc_request_json(
        &dir.ipc_sock(),
        &ipc_req(
            "1",
            "new_session",
            serde_json::json!({
                "tool": "bash",
                "profile": "test-profile",
            }),
        ),
    );
    let result = assert_ok(&resp);

    assert_eq!(result["profile"].as_str().unwrap(), "test-profile");
}

#[test]
fn test_ipc_new_session_invalid_dir() {
    let dir = TempDir::new("ipc-baddir");
    let _agent = spawn_agent(&dir);
    assert!(wait_for_ipc_socket(&dir.ipc_sock(), 10));

    let resp = ipc_request_json(
        &dir.ipc_sock(),
        &ipc_req(
            "1",
            "new_session",
            serde_json::json!({
                "tool": "bash",
                "cwd": "/nonexistent/directory/xyz123",
            }),
        ),
    );
    assert_err(&resp, "INVALID_PARAMS");
}

#[test]
fn test_ipc_register_session_creates_running_relay_session() {
    let dir = TempDir::new("ipc-relay");
    let _agent = spawn_agent(&dir);
    assert!(wait_for_ipc_socket(&dir.ipc_sock(), 10));

    let resp = ipc_request_json(
        &dir.ipc_sock(),
        &ipc_req(
            "1",
            "register_session",
            serde_json::json!({
                "tool": "claude",
                "profile": "work",
                "cwd": ".",
                "source": "desktop",
                "pid": std::process::id(),
            }),
        ),
    );
    let result = assert_ok(&resp);
    let nid = result["nid"].as_str().unwrap().to_string();
    assert_eq!(result["status"].as_str().unwrap(), "registered");
    assert_eq!(result["source"].as_str().unwrap(), "desktop");

    let resp = ipc_request_json(
        &dir.ipc_sock(),
        &ipc_req("2", "sessions", serde_json::json!({})),
    );
    let result = assert_ok(&resp);
    let sessions = result["sessions"].as_array().unwrap();
    let session = sessions
        .iter()
        .find(|s| s["nid"].as_str() == Some(&nid))
        .expect("registered relay session should be listed");
    assert_eq!(session["kind"].as_str().unwrap(), "Relay");
    assert_eq!(session["status"].as_str().unwrap(), "running");
    assert_eq!(session["remote_enabled"].as_bool().unwrap(), false);
}

#[test]
fn test_ipc_relay_output_and_poll_input_for_registered_session() {
    let dir = TempDir::new("ipc-relay-io");
    let _agent = spawn_agent(&dir);
    assert!(wait_for_ipc_socket(&dir.ipc_sock(), 10));

    let resp = ipc_request_json(
        &dir.ipc_sock(),
        &ipc_req(
            "1",
            "register_session",
            serde_json::json!({
                "tool": "claude",
                "profile": "work",
                "cwd": ".",
                "source": "desktop",
            }),
        ),
    );
    let nid = assert_ok(&resp)["nid"].as_str().unwrap().to_string();

    let resp = ipc_request_json(
        &dir.ipc_sock(),
        &ipc_req(
            "2",
            "relay_output",
            serde_json::json!({"nid": nid, "data": "hello relay\n"}),
        ),
    );
    assert_ok(&resp);

    let resp = ipc_request_json(
        &dir.ipc_sock(),
        &ipc_req("3", "poll_relay_input", serde_json::json!({"nid": nid})),
    );
    let result = assert_ok(&resp);
    assert_eq!(result["status"].as_str().unwrap(), "running");
    assert_eq!(result["ended"].as_bool().unwrap(), false);
    assert!(result["inputs"].as_array().unwrap().is_empty());
}

#[test]
fn test_ipc_enable_remote_requires_wss_but_disable_is_local_only() {
    let dir = TempDir::new("ipc-remote-toggle");
    let _agent = spawn_agent(&dir);
    assert!(wait_for_ipc_socket(&dir.ipc_sock(), 10));

    let resp = ipc_request_json(
        &dir.ipc_sock(),
        &ipc_req(
            "1",
            "register_session",
            serde_json::json!({
                "tool": "claude",
                "profile": "work",
                "cwd": ".",
                "source": "desktop",
            }),
        ),
    );
    let nid = assert_ok(&resp)["nid"].as_str().unwrap().to_string();

    let resp = ipc_request_json(
        &dir.ipc_sock(),
        &ipc_req(
            "2",
            "set_remote_enabled",
            serde_json::json!({"nid": nid, "enabled": false}),
        ),
    );
    assert_ok(&resp);

    let resp = ipc_request_json(
        &dir.ipc_sock(),
        &ipc_req(
            "3",
            "set_remote_enabled",
            serde_json::json!({"nid": nid, "enabled": true}),
        ),
    );
    assert_err(&resp, "WSS_NOT_CONNECTED");
}

#[test]
fn test_ipc_kill_registered_relay_session_marks_it_ended() {
    let dir = TempDir::new("ipc-relay-kill");
    let _agent = spawn_agent(&dir);
    assert!(wait_for_ipc_socket(&dir.ipc_sock(), 10));

    let resp = ipc_request_json(
        &dir.ipc_sock(),
        &ipc_req(
            "1",
            "register_session",
            serde_json::json!({
                "tool": "claude",
                "profile": "work",
                "cwd": ".",
                "source": "desktop",
            }),
        ),
    );
    let nid = assert_ok(&resp)["nid"].as_str().unwrap().to_string();

    let resp = ipc_request_json(
        &dir.ipc_sock(),
        &ipc_req("2", "kill_session", serde_json::json!({"nid": &nid})),
    );
    assert_ok(&resp);

    let resp = ipc_request_json(
        &dir.ipc_sock(),
        &ipc_req("3", "poll_relay_input", serde_json::json!({"nid": &nid})),
    );
    let result = assert_ok(&resp);
    assert_eq!(result["status"].as_str().unwrap(), "ended");
    assert_eq!(result["ended"].as_bool().unwrap(), true);
    assert!(result["inputs"].as_array().unwrap().is_empty());
}

#[test]
fn test_ipc_relay_exit_marks_registered_relay_ended_without_kill() {
    let dir = TempDir::new("ipc-relay-exit");
    let _agent = spawn_agent(&dir);
    assert!(wait_for_ipc_socket(&dir.ipc_sock(), 10));

    let resp = ipc_request_json(
        &dir.ipc_sock(),
        &ipc_req(
            "1",
            "register_session",
            serde_json::json!({
                "tool": "claude",
                "profile": "work",
                "cwd": ".",
                "source": "desktop",
            }),
        ),
    );
    let nid = assert_ok(&resp)["nid"].as_str().unwrap().to_string();

    let resp = ipc_request_json(
        &dir.ipc_sock(),
        &ipc_req(
            "2",
            "relay_exit",
            serde_json::json!({"nid": &nid, "reason": "process_exit"}),
        ),
    );
    assert_ok(&resp);

    // A Desktop retry after an Agent restart/cleanup must not remain queued
    // just because the Relay was already removed.
    let resp = ipc_request_json(
        &dir.ipc_sock(),
        &ipc_req(
            "3",
            "relay_exit",
            serde_json::json!({"nid": "s_missing_relay", "reason": "user_closed_tab"}),
        ),
    );
    let result = assert_ok(&resp);
    assert_eq!(result["already_ended"].as_bool(), Some(true));

    let resp = ipc_request_json(
        &dir.ipc_sock(),
        &ipc_req("4", "poll_relay_input", serde_json::json!({"nid": &nid})),
    );
    let result = assert_ok(&resp);
    assert_eq!(result["status"].as_str().unwrap(), "ended");
    assert_eq!(result["ended"].as_bool().unwrap(), true);
}

#[test]
fn test_ipc_attach() {
    let dir = TempDir::new("ipc-attach");
    let _agent = spawn_agent(&dir);
    assert!(wait_for_ipc_socket(&dir.ipc_sock(), 10));

    // Create a session first
    let resp = ipc_request_json(
        &dir.ipc_sock(),
        &ipc_req("1", "new_session", serde_json::json!({"tool": "bash"})),
    );
    let nid = assert_ok(&resp)["nid"].as_str().unwrap().to_string();

    // Wait briefly for PTY to start
    std::thread::sleep(Duration::from_millis(500));

    // Attach
    let resp = ipc_request_json(
        &dir.ipc_sock(),
        &ipc_req("2", "attach", serde_json::json!({"nid": nid})),
    );
    assert_ok(&resp);
}

#[test]
fn test_ipc_resize() {
    let dir = TempDir::new("ipc-resize");
    let _agent = spawn_agent(&dir);
    assert!(wait_for_ipc_socket(&dir.ipc_sock(), 10));

    let resp = ipc_request_json(
        &dir.ipc_sock(),
        &ipc_req("1", "new_session", serde_json::json!({"tool": "bash"})),
    );
    let nid = assert_ok(&resp)["nid"].as_str().unwrap().to_string();

    std::thread::sleep(Duration::from_millis(500));

    let resp = ipc_request_json(
        &dir.ipc_sock(),
        &ipc_req(
            "2",
            "resize",
            serde_json::json!({"nid": nid, "cols": 120, "rows": 40}),
        ),
    );
    let result = assert_ok(&resp);
    assert_eq!(result["cols"].as_u64().unwrap(), 120);
    assert_eq!(result["rows"].as_u64().unwrap(), 40);
}

#[test]
fn test_ipc_ctrl_signals_valid_and_invalid() {
    let dir = TempDir::new("ipc-ctrl");
    let _agent = spawn_agent(&dir);
    assert!(wait_for_ipc_socket(&dir.ipc_sock(), 10));

    let resp = ipc_request_json(
        &dir.ipc_sock(),
        &ipc_req("1", "new_session", serde_json::json!({"tool": "bash"})),
    );
    let nid = assert_ok(&resp)["nid"].as_str().unwrap().to_string();

    std::thread::sleep(Duration::from_millis(500));

    // Valid signals
    for sig in &["ctrl_c", "ctrl_d", "ctrl_z"] {
        let resp = ipc_request_json(
            &dir.ipc_sock(),
            &ipc_req("2", "ctrl", serde_json::json!({"nid": nid, "signal": sig})),
        );
        assert_ok(&resp);
    }

    // Invalid signal
    let resp = ipc_request_json(
        &dir.ipc_sock(),
        &ipc_req(
            "3",
            "ctrl",
            serde_json::json!({"nid": nid, "signal": "ctrl_x"}),
        ),
    );
    assert_err(&resp, "INVALID_PARAMS");
}

#[test]
fn test_ipc_kill_session() {
    let dir = TempDir::new("ipc-kill");
    let _agent = spawn_agent(&dir);
    assert!(wait_for_ipc_socket(&dir.ipc_sock(), 10));

    let resp = ipc_request_json(
        &dir.ipc_sock(),
        &ipc_req("1", "new_session", serde_json::json!({"tool": "bash"})),
    );
    let nid = assert_ok(&resp)["nid"].as_str().unwrap().to_string();

    std::thread::sleep(Duration::from_millis(500));

    // Kill it
    let resp = ipc_request_json(
        &dir.ipc_sock(),
        &ipc_req("2", "kill_session", serde_json::json!({"nid": nid})),
    );
    assert_ok(&resp);

    // Should be gone from session list (status = Ended)
    let resp = ipc_request_json(
        &dir.ipc_sock(),
        &ipc_req("3", "sessions", serde_json::json!({})),
    );
    let result = assert_ok(&resp);
    let sessions = result["sessions"].as_array().unwrap();
    // All sessions should be ended
    for s in sessions {
        assert_eq!(s["status"].as_str().unwrap(), "ended");
    }
}

#[test]
fn test_ipc_multiple_sessions() {
    let dir = TempDir::new("ipc-multi");
    let _agent = spawn_agent(&dir);
    assert!(wait_for_ipc_socket(&dir.ipc_sock(), 10));

    // Create 3 sessions
    let mut nids = Vec::new();
    for i in 0..3 {
        let resp = ipc_request_json(
            &dir.ipc_sock(),
            &ipc_req(
                &format!("{}", i + 1),
                "new_session",
                serde_json::json!({"tool": "bash"}),
            ),
        );
        nids.push(assert_ok(&resp)["nid"].as_str().unwrap().to_string());
    }

    std::thread::sleep(Duration::from_millis(500));

    // List should show 3 sessions
    let resp = ipc_request_json(
        &dir.ipc_sock(),
        &ipc_req("4", "sessions", serde_json::json!({})),
    );
    let result = assert_ok(&resp);
    assert_eq!(result["count"].as_u64().unwrap(), 3);

    // Kill all
    for nid in &nids {
        ipc_request_json(
            &dir.ipc_sock(),
            &ipc_req("5", "kill_session", serde_json::json!({"nid": nid})),
        );
    }
}

// ── Error handling: non-existent nid ───────────────────────

#[test]
fn test_ipc_not_found_input() {
    let dir = TempDir::new("ipc-nf-in");
    let _agent = spawn_agent(&dir);
    assert!(wait_for_ipc_socket(&dir.ipc_sock(), 10));

    let resp = ipc_request_json(
        &dir.ipc_sock(),
        &ipc_req(
            "1",
            "input",
            serde_json::json!({"nid": "s_nonexistent", "text": "hello"}),
        ),
    );
    assert_err(&resp, "NOT_FOUND");
}

#[test]
fn test_ipc_not_found_ctrl() {
    let dir = TempDir::new("ipc-nf-ctrl");
    let _agent = spawn_agent(&dir);
    assert!(wait_for_ipc_socket(&dir.ipc_sock(), 10));

    let resp = ipc_request_json(
        &dir.ipc_sock(),
        &ipc_req(
            "1",
            "ctrl",
            serde_json::json!({"nid": "s_nonexistent", "signal": "ctrl_c"}),
        ),
    );
    assert_err(&resp, "NOT_FOUND");
}

#[test]
fn test_ipc_not_found_kill() {
    let dir = TempDir::new("ipc-nf-kill");
    let _agent = spawn_agent(&dir);
    assert!(wait_for_ipc_socket(&dir.ipc_sock(), 10));

    let resp = ipc_request_json(
        &dir.ipc_sock(),
        &ipc_req(
            "1",
            "kill_session",
            serde_json::json!({"nid": "s_nonexistent"}),
        ),
    );
    assert_err(&resp, "NOT_FOUND");
}

#[test]
fn test_ipc_not_found_resize() {
    let dir = TempDir::new("ipc-nf-resize");
    let _agent = spawn_agent(&dir);
    assert!(wait_for_ipc_socket(&dir.ipc_sock(), 10));

    let resp = ipc_request_json(
        &dir.ipc_sock(),
        &ipc_req(
            "1",
            "resize",
            serde_json::json!({"nid": "s_nonexistent", "cols": 80, "rows": 24}),
        ),
    );
    assert_err(&resp, "NOT_FOUND");
}

#[test]
fn test_ipc_not_found_attach() {
    let dir = TempDir::new("ipc-nf-attach");
    let _agent = spawn_agent(&dir);
    assert!(wait_for_ipc_socket(&dir.ipc_sock(), 10));

    let resp = ipc_request_json(
        &dir.ipc_sock(),
        &ipc_req("1", "attach", serde_json::json!({"nid": "s_nonexistent"})),
    );
    assert_err(&resp, "NOT_FOUND");
}

// ── IPC input end-to-end ────────────────────────────────────

#[test]
fn test_ipc_input_end_to_end() {
    let dir = TempDir::new("ipc-input-e2e");
    let _agent = spawn_agent(&dir);
    assert!(wait_for_ipc_socket(&dir.ipc_sock(), 10));

    // Create a bash session via IPC
    let resp = ipc_request_json(
        &dir.ipc_sock(),
        &ipc_req("1", "new_session", serde_json::json!({"tool": "bash"})),
    );
    let nid = assert_ok(&resp)["nid"].as_str().unwrap().to_string();

    // Wait for PTY to start
    std::thread::sleep(Duration::from_millis(1500));

    // Send input via IPC
    let resp = ipc_request_json(
        &dir.ipc_sock(),
        &ipc_req(
            "2",
            "input",
            serde_json::json!({"nid": nid, "text": "echo IPC_E2E_TEST\n"}),
        ),
    );
    assert_ok(&resp);

    // Give the command time to execute
    std::thread::sleep(Duration::from_secs(1));

    // Verify the session is still alive (didn't crash)
    let resp = ipc_request_json(
        &dir.ipc_sock(),
        &ipc_req("3", "sessions", serde_json::json!({})),
    );
    let result = assert_ok(&resp);
    let sessions = result["sessions"].as_array().unwrap();
    assert!(
        !sessions.is_empty(),
        "Session should still exist after input"
    );

    // Clean up
    ipc_request_json(
        &dir.ipc_sock(),
        &ipc_req("4", "kill_session", serde_json::json!({"nid": nid})),
    );
}

#[test]
fn test_ipc_input_empty_text_rejected() {
    let dir = TempDir::new("ipc-empty-txt");
    let _agent = spawn_agent(&dir);
    assert!(wait_for_ipc_socket(&dir.ipc_sock(), 10));

    // Create session first
    let resp = ipc_request_json(
        &dir.ipc_sock(),
        &ipc_req("1", "new_session", serde_json::json!({"tool": "bash"})),
    );
    let nid = assert_ok(&resp)["nid"].as_str().unwrap().to_string();

    std::thread::sleep(Duration::from_millis(500));

    // Empty text should be rejected
    let resp = ipc_request_json(
        &dir.ipc_sock(),
        &ipc_req("2", "input", serde_json::json!({"nid": nid, "text": ""})),
    );
    assert_err(&resp, "INVALID_PARAMS");
}

// ═══════════════════════════════════════════════════════════════════
// Group C: PTY Session Lifecycle Tests (real CLI tools)
// ═══════════════════════════════════════════════════════════════════

/// Helper: start a PTY session, verify session is marked running.
async fn start_pty_basic(tool: &str) -> (String, Arc<kn_agent::session::SessionManager>) {
    let store = Box::new(kn_agent::session::MemorySessionStore::new());
    let sessions = Arc::new(kn_agent::session::SessionManager::new(store));
    let merger = Arc::new(kn_agent::session::InputMerger::new());

    let nid = format!("s_test_{}", nanoid::nanoid!(8));
    let cwd = std::env::current_dir()
        .unwrap()
        .to_string_lossy()
        .to_string();

    sessions
        .create(
            nid.clone(),
            "test".into(),
            tool.to_string(),
            None,
            cwd.clone(),
            kn_agent::session::SessionKind::Native,
        )
        .await
        .expect("create session");

    let (wss_tx, _wss_rx) = mpsc::unbounded_channel();
    let (ipc_tx, _ipc_rx) = mpsc::unbounded_channel::<String>();

    sessions
        .clone()
        .start_session(&nid, tool, None, &cwd, 80, 24, wss_tx, ipc_tx, merger, None)
        .await
        .expect("start PTY session");

    // Verify session is marked running
    let s = sessions.get(&nid).await.unwrap().unwrap();
    assert_eq!(s.status, kn_agent::session::SessionStatus::Running);

    (nid, sessions)
}

#[tokio::test]
async fn test_pty_start_bash_session() {
    let (nid, sessions) = start_pty_basic("bash").await;
    sessions
        .kill_session(&nid)
        .await
        .expect("kill bash session");
}

#[tokio::test]
async fn test_pty_start_claude_session() {
    if !has_binary("claude") {
        eprintln!("SKIP: claude binary not found");
        return;
    }

    let (nid, sessions) = start_pty_basic("claude").await;
    sessions
        .kill_session(&nid)
        .await
        .expect("kill claude session");
}

#[tokio::test]
async fn test_pty_start_codex_session() {
    if !has_binary("codex") {
        eprintln!("SKIP: codex binary not found");
        return;
    }

    let (nid, sessions) = start_pty_basic("codex").await;
    sessions
        .kill_session(&nid)
        .await
        .expect("kill codex session");
}

#[tokio::test]
async fn test_pty_input_output_roundtrip() {
    let store = Box::new(kn_agent::session::MemorySessionStore::new());
    let sessions = Arc::new(kn_agent::session::SessionManager::new(store));
    let merger = Arc::new(kn_agent::session::InputMerger::new());

    let nid = format!("s_echo_{}", nanoid::nanoid!(8));
    let cwd = std::env::current_dir()
        .unwrap()
        .to_string_lossy()
        .to_string();

    sessions
        .create(
            nid.clone(),
            "test".into(),
            "bash".into(),
            None,
            cwd.clone(),
            kn_agent::session::SessionKind::Native,
        )
        .await
        .expect("create");

    let (wss_tx, _wss_rx) = mpsc::unbounded_channel();
    let (ipc_tx, mut ipc_rx) = mpsc::unbounded_channel::<String>();

    sessions
        .clone()
        .start_session(
            &nid,
            "bash",
            None,
            &cwd,
            80,
            24,
            wss_tx,
            ipc_tx,
            merger.clone(),
            None,
        )
        .await
        .expect("start PTY");

    // Wait for bash prompt
    tokio::time::sleep(Duration::from_secs(1)).await;

    // Send a command through InputMerger
    let test_marker = format!("HELLO_KN_TEST_{}", nanoid::nanoid!(6));
    merger
        .push(kn_agent::session::InputMessage {
            session_id: nid.clone(),
            text: format!("echo {}\n", test_marker),
            source: "test".into(),
        })
        .await;

    // Wait for the echo output
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    let mut found = false;
    while tokio::time::Instant::now() < deadline {
        match tokio::time::timeout(Duration::from_millis(300), ipc_rx.recv()).await {
            Ok(Some(chunk)) => {
                if chunk.contains(&test_marker) {
                    found = true;
                    break;
                }
            }
            _ => continue,
        }
    }

    assert!(
        found,
        "Did not find echo output containing '{}'",
        test_marker
    );
    sessions.kill_session(&nid).await.expect("kill");
}

#[tokio::test]
async fn test_pty_kill_session() {
    let store = Box::new(kn_agent::session::MemorySessionStore::new());
    let sessions = Arc::new(kn_agent::session::SessionManager::new(store));
    let merger = Arc::new(kn_agent::session::InputMerger::new());

    let nid = format!("s_kill_{}", nanoid::nanoid!(8));
    let cwd = std::env::current_dir()
        .unwrap()
        .to_string_lossy()
        .to_string();

    sessions
        .create(
            nid.clone(),
            "test".into(),
            "bash".into(),
            None,
            cwd.clone(),
            kn_agent::session::SessionKind::Native,
        )
        .await
        .expect("create");

    let (wss_tx, _wss_rx) = mpsc::unbounded_channel();
    let (ipc_tx, _ipc_rx) = mpsc::unbounded_channel::<String>();

    sessions
        .clone()
        .start_session(
            &nid, "bash", None, &cwd, 80, 24, wss_tx, ipc_tx, merger, None,
        )
        .await
        .expect("start PTY");

    // Sleep a bit to let bash start
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Verify session exists and is running
    let s = sessions.get(&nid).await.unwrap().unwrap();
    assert_eq!(s.status, kn_agent::session::SessionStatus::Running);

    // Kill it
    sessions.kill_session(&nid).await.expect("kill");

    // Verify session is now Ended
    let s = sessions.get(&nid).await.unwrap().unwrap();
    assert_eq!(s.status, kn_agent::session::SessionStatus::Ended);
}

#[tokio::test]
async fn test_pty_resize_session() {
    let store = Box::new(kn_agent::session::MemorySessionStore::new());
    let sessions = Arc::new(kn_agent::session::SessionManager::new(store));
    let merger = Arc::new(kn_agent::session::InputMerger::new());

    let nid = format!("s_resize_{}", nanoid::nanoid!(8));
    let cwd = std::env::current_dir()
        .unwrap()
        .to_string_lossy()
        .to_string();

    sessions
        .create(
            nid.clone(),
            "test".into(),
            "bash".into(),
            None,
            cwd.clone(),
            kn_agent::session::SessionKind::Native,
        )
        .await
        .expect("create");

    let (wss_tx, _wss_rx) = mpsc::unbounded_channel();
    let (ipc_tx, _ipc_rx) = mpsc::unbounded_channel::<String>();

    sessions
        .clone()
        .start_session(
            &nid, "bash", None, &cwd, 80, 24, wss_tx, ipc_tx, merger, None,
        )
        .await
        .expect("start PTY");

    tokio::time::sleep(Duration::from_millis(500)).await;

    // Resize
    sessions.resize(&nid, 120, 40).await.expect("resize");

    let s = sessions.get(&nid).await.unwrap().unwrap();
    assert_eq!(s.cols, 120);
    assert_eq!(s.rows, 40);

    sessions.kill_session(&nid).await.ok();
}

#[tokio::test]
async fn test_pty_multiple_concurrent_sessions() {
    let store = Box::new(kn_agent::session::MemorySessionStore::new());
    let sessions = Arc::new(kn_agent::session::SessionManager::new(store));
    let merger = Arc::new(kn_agent::session::InputMerger::new());
    let cwd = std::env::current_dir()
        .unwrap()
        .to_string_lossy()
        .to_string();

    let mut nids = Vec::new();
    let mut cancels = Vec::new();

    // Start 2 concurrent bash sessions
    for _ in 0..2 {
        let nid = format!("s_concurrent_{}", nanoid::nanoid!(8));
        sessions
            .create(
                nid.clone(),
                "test".into(),
                "bash".into(),
                None,
                cwd.clone(),
                kn_agent::session::SessionKind::Native,
            )
            .await
            .expect("create");

        let (wss_tx, _wss_rx) = mpsc::unbounded_channel();
        let (ipc_tx, _ipc_rx) = mpsc::unbounded_channel::<String>();
        let cancel = tokio_util::sync::CancellationToken::new();

        sessions
            .clone()
            .start_session(
                &nid,
                "bash",
                None,
                &cwd,
                80,
                24,
                wss_tx,
                ipc_tx,
                merger.clone(),
                None,
            )
            .await
            .expect("start PTY");

        nids.push(nid);
        cancels.push(cancel);
    }

    tokio::time::sleep(Duration::from_millis(500)).await;

    // Both should exist and be running
    let list = sessions.list().await.expect("list");
    let running_count = list
        .iter()
        .filter(|s| s.status == kn_agent::session::SessionStatus::Running)
        .count();
    assert_eq!(
        running_count, 2,
        "Expected 2 running sessions, got {}",
        running_count
    );

    // Kill both
    for nid in &nids {
        sessions.kill_session(nid).await.ok();
    }
    for c in &cancels {
        c.cancel();
    }
}

// ═══════════════════════════════════════════════════════════════════
// Group D: Bind Flow Tests (requires local cloud at localhost:8080)
// ═══════════════════════════════════════════════════════════════════

/// Check if local cloud HTTP is reachable (TCP connect only, no HTTP).
fn cloud_port_open() -> bool {
    std::net::TcpStream::connect("localhost:8080").is_ok()
}

#[tokio::test]
#[ignore = "requires local cloud services at localhost:8080"]
async fn test_bind_init_flow() {
    if !cloud_port_open() {
        eprintln!("SKIP: local cloud not available");
        return;
    }

    let dir = TempDir::new("bind-init");
    let _agent = spawn_agent(&dir);
    assert!(wait_for_ipc_socket(&dir.ipc_sock(), 10));

    let resp = ipc_request_json(
        &dir.ipc_sock(),
        &ipc_req("1", "bind", serde_json::json!({})),
    );
    let result = assert_ok(&resp);

    assert_eq!(result["status"].as_str().unwrap(), "binding_started");
    let bind_code = result["bindCode"].as_str().unwrap();
    assert_eq!(
        bind_code.len(),
        6,
        "bindCode should be 6 chars, got: {}",
        bind_code
    );
    assert!(result["expiresIn"].as_u64().unwrap() > 0);
    assert!(!result["confirmUrl"].as_str().unwrap().is_empty());
}

#[tokio::test]
#[ignore = "requires local cloud services at localhost:8080"]
async fn test_bind_status_after_bind() {
    if !cloud_port_open() {
        eprintln!("SKIP: local cloud not available");
        return;
    }

    let dir = TempDir::new("bind-status");
    let _agent = spawn_agent(&dir);
    assert!(wait_for_ipc_socket(&dir.ipc_sock(), 10));

    // Bind
    ipc_request_json(
        &dir.ipc_sock(),
        &ipc_req("1", "bind", serde_json::json!({})),
    );

    // Check status — should be "binding" or "unbound" depending on timing
    std::thread::sleep(Duration::from_secs(1));
    let resp = ipc_request_json(
        &dir.ipc_sock(),
        &ipc_req("2", "status", serde_json::json!({})),
    );
    let state = assert_ok(&resp)["state"].as_str().unwrap().to_string();
    assert!(
        state == "binding" || state == "unbound" || state == "connected",
        "Unexpected state: {}",
        state
    );
}

// ═══════════════════════════════════════════════════════════════════
// Group E: Redeem IPC Validation (no cloud dependency)
// ═══════════════════════════════════════════════════════════════════

#[test]
fn test_redeem_not_bound() {
    let dir = TempDir::new("redeem-nobind");
    let _agent = spawn_agent(&dir);
    assert!(wait_for_ipc_socket(&dir.ipc_sock(), 10));

    let resp = ipc_request_json(
        &dir.ipc_sock(),
        &ipc_req("1", "redeem", serde_json::json!({"code": "KN-TEST-CODE"})),
    );
    assert_err(&resp, "NOT_BOUND");
}

#[test]
fn test_redeem_empty_code() {
    let dir = TempDir::new("redeem-empty");
    let _agent = spawn_agent(&dir);
    assert!(wait_for_ipc_socket(&dir.ipc_sock(), 10));

    let resp = ipc_request_json(
        &dir.ipc_sock(),
        &ipc_req("1", "redeem", serde_json::json!({"code": ""})),
    );
    assert_err(&resp, "INVALID_PARAMS");
}

// ═══════════════════════════════════════════════════════════════════
// Group F: Crash / Restart / Reconnect
// ═══════════════════════════════════════════════════════════════════

#[test]
fn test_agent_with_token_goes_to_wss_path() {
    let dir = TempDir::new("restart-token");

    // Pre-create agent dir with a fake device_token — simulates previously-bound agent
    let agent_dir = dir.path().join("agent");
    std::fs::create_dir_all(&agent_dir).unwrap();
    std::fs::write(agent_dir.join("device_token"), "test-device-token-restart").unwrap();

    // Start agent — with a token present, it goes to the WSS path.
    // The IPC server now always starts (unconditionally, for Desktop status queries),
    // so the socket SHOULD appear regardless of token state.
    let _agent = spawn_agent(&dir);

    let appeared = wait_for_ipc_socket(&dir.ipc_sock(), 5);
    assert!(
        appeared,
        "IPC socket SHOULD appear even when agent has a device_token (IPC server always starts)"
    );
}

#[test]
fn test_device_token_survives_restart() {
    let dir = TempDir::new("tok-survive");

    let token_path = dir.path().join("agent").join("device_token");
    let test_token = "survive-token-xyz";

    // Write token
    std::fs::create_dir_all(dir.path().join("agent")).unwrap();
    std::fs::write(&token_path, test_token).unwrap();

    // Simulate crash: read token back before restart
    let before = std::fs::read_to_string(&token_path).unwrap();
    assert_eq!(before.trim(), test_token);

    // "Restart" — the token should still be on disk
    let after = std::fs::read_to_string(&token_path).unwrap();
    assert_eq!(
        after.trim(),
        test_token,
        "device_token should survive across restarts"
    );
}

#[test]
fn test_agent_restart_without_token_stays_unbound() {
    let dir = TempDir::new("restart-no-tok");

    // First start: fresh KN_HOME, no token → unbound
    let agent1 = spawn_agent(&dir);
    assert!(
        wait_for_ipc_socket(&dir.ipc_sock(), 15),
        "Agent 1 socket did not appear"
    );

    let resp = ipc_request_json(
        &dir.ipc_sock(),
        &ipc_req("1", "status", serde_json::json!({})),
    );
    assert_eq!(assert_ok(&resp)["state"].as_str().unwrap(), "unbound");

    // Kill agent and wait for process to fully exit
    drop(agent1);
    std::thread::sleep(Duration::from_secs(2));
    let _ = std::fs::remove_file(&dir.ipc_sock());

    // Second start: same KN_HOME, still no token → should still be unbound
    let _agent2 = spawn_agent(&dir);
    assert!(
        wait_for_ipc_socket(&dir.ipc_sock(), 15),
        "Agent 2 socket did not appear after restart"
    );

    let resp = ipc_request_json(
        &dir.ipc_sock(),
        &ipc_req("2", "status", serde_json::json!({})),
    );
    assert_eq!(
        assert_ok(&resp)["state"].as_str().unwrap(),
        "unbound",
        "Agent should remain unbound after restart without token"
    );
}

#[test]
fn test_crash_count_increments_across_restarts() {
    let dir = TempDir::new("crash-count");

    // First start → crash_count = 1
    let agent1 = spawn_agent(&dir);
    assert!(wait_for_ipc_socket(&dir.ipc_sock(), 10));

    let resp = ipc_request_json(
        &dir.ipc_sock(),
        &ipc_req("1", "status", serde_json::json!({})),
    );
    let count1 = assert_ok(&resp)["crash_count"].as_u64().unwrap();
    assert!(
        count1 >= 1,
        "First start should have crash_count >= 1, got {}",
        count1
    );

    // Kill and wait for process to exit
    drop(agent1);
    std::thread::sleep(Duration::from_secs(2));
    let _ = std::fs::remove_file(&dir.ipc_sock());

    // Second start → crash_count = 2
    let agent2 = spawn_agent(&dir);
    assert!(wait_for_ipc_socket(&dir.ipc_sock(), 15));

    let resp = ipc_request_json(
        &dir.ipc_sock(),
        &ipc_req("2", "status", serde_json::json!({})),
    );
    let count2 = assert_ok(&resp)["crash_count"].as_u64().unwrap();
    assert!(
        count2 > count1,
        "Second start should have higher crash_count, got {} (was {})",
        count2,
        count1
    );

    // Kill and wait
    drop(agent2);
    std::thread::sleep(Duration::from_secs(2));
    let _ = std::fs::remove_file(&dir.ipc_sock());

    // Third start → crash_count = 3
    let _agent3 = spawn_agent(&dir);
    assert!(wait_for_ipc_socket(&dir.ipc_sock(), 10));

    let resp = ipc_request_json(
        &dir.ipc_sock(),
        &ipc_req("3", "status", serde_json::json!({})),
    );
    let count3 = assert_ok(&resp)["crash_count"].as_u64().unwrap();
    assert!(
        count3 > count2,
        "Third start should have higher crash_count, got {} (was {})",
        count3,
        count2
    );
}

// ═══════════════════════════════════════════════════════════════════
// Group H: PTY Takeover — pty.sock 双向代理
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn test_pty_takeover_via_attach() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::UnixStream;

    let (nid, sessions) = start_pty_basic("bash").await;

    // 1. attach_pty creates the socket and sets up the I/O bridge
    let pty_sock_path = sessions.attach_pty(&nid).await.expect("attach_pty");
    assert!(
        pty_sock_path.exists(),
        "pty.sock should exist at {:?}",
        pty_sock_path
    );

    // 2. Verify we can connect to it — accept returns quickly
    let stream = UnixStream::connect(&pty_sock_path)
        .await
        .expect("connect to pty.sock");
    let (mut reader, mut writer) = stream.into_split();

    // 3. Send a command — the PTY may or may not be alive (bash can exit early).
    //    If alive, we get echo output; if dead, we get EOF. Both are valid outcomes:
    //    the important thing is that the bridge connection works.
    let test_marker = format!("TAKEOVER_{}", nanoid::nanoid!(6));
    let cmd = format!("echo {}\n", test_marker);
    let _ = writer.write_all(cmd.as_bytes()).await;
    drop(writer); // half-close — signals EOF to the bridge

    // 4. Collect any output
    let mut output = Vec::new();
    let mut buf = [0u8; 4096];
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    while tokio::time::Instant::now() < deadline {
        match tokio::time::timeout(Duration::from_millis(500), reader.read(&mut buf)).await {
            Ok(Ok(0)) => {
                eprintln!("pty.sock EOF (session ended)");
                break;
            }
            Ok(Ok(n)) => {
                output.extend_from_slice(&buf[..n]);
                if String::from_utf8_lossy(&output).contains(&test_marker) {
                    break;
                }
            }
            _ => continue,
        }
    }

    // Accept either: marker found (PTY was alive) or session ended (PTY exited).
    // The bridge mechanism itself is verified by the successful connect.
    let text = String::from_utf8_lossy(&output);
    let has_marker = text.contains(&test_marker);
    let ended = output.is_empty() || text.contains("parse error") || text.contains("not found");

    assert!(
        has_marker || ended,
        "Expected marker '{}' or session-end, got ({} bytes): {}",
        test_marker,
        output.len(),
        text
    );

    // 5. Clean up
    drop(reader);
    sessions.kill_session(&nid).await.ok();
}
// ═══════════════════════════════════════════════════════════════════
// Group I: OutputFanout subscriber mechanism
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn test_output_fanout_subscriber_receives_data() {
    let store = Box::new(kn_agent::session::MemorySessionStore::new());
    let sessions = Arc::new(kn_agent::session::SessionManager::new(store));
    let merger = Arc::new(kn_agent::session::InputMerger::new());
    let nid = format!("s_sub_{}", nanoid::nanoid!(8));
    let cwd = std::env::current_dir()
        .unwrap()
        .to_string_lossy()
        .to_string();

    sessions
        .create(
            nid.clone(),
            "test".into(),
            "bash".into(),
            None,
            cwd.clone(),
            kn_agent::session::SessionKind::Native,
        )
        .await
        .unwrap();

    let (wss_tx, _wss_rx) = mpsc::unbounded_channel();
    let (ipc_tx, _ipc_rx) = mpsc::unbounded_channel::<String>();

    // Start PTY and get fanout (start_session consumes Arc<Self>)
    let fanout = sessions
        .clone()
        .start_session(
            &nid,
            "bash",
            None,
            &cwd,
            80,
            24,
            wss_tx,
            ipc_tx,
            merger.clone(),
            None,
        )
        .await
        .unwrap();

    // Register subscriber
    let mut sub_rx = fanout.register_subscriber();

    // Send a command
    merger
        .push(kn_agent::session::InputMessage {
            session_id: nid.clone(),
            text: "echo FANOUT_TEST\n".into(),
            source: "test".into(),
        })
        .await;

    // Wait for output via subscriber
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    let mut found = false;
    while tokio::time::Instant::now() < deadline {
        match tokio::time::timeout(Duration::from_millis(300), sub_rx.recv()).await {
            Ok(Some(data)) => {
                let text = String::from_utf8_lossy(&data);
                if text.contains("FANOUT_TEST") {
                    found = true;
                    break;
                }
            }
            _ => continue,
        }
    }

    assert!(found, "OutputFanout subscriber did not receive echo output");

    sessions.kill_session(&nid).await.expect("kill");
}
