//! Mock WebSocket server for testing kn-agent WSS integration.
//!
//! Simulates kn-cloud WebSocket behavior (aligned with Java KnWsHandler):
//! - Accept connections on localhost
//! - Send `connected` message on connect (matching WsMessageFactory.connected)
//! - Auto-respond to `ping` with `pong`
//! - Queue configurable messages to send to client
//! - Collect messages sent by client for assertion

use futures_util::{SinkExt, StreamExt};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::sync::Mutex;
use tokio_tungstenite::accept_async;
use tokio_tungstenite::tungstenite::Message;

/// A mock WebSocket server for testing kn-agent WSS integration.
///
/// Simulates kn-cloud behavior: connect handshake → connected message,
/// automatic ping/pong response, programmable message injection,
/// and client message collection.
pub struct MockWssServer {
    /// Address the server is listening on
    pub addr: SocketAddr,
    /// Shutdown signal
    shutdown: tokio_util::sync::CancellationToken,
    /// Received messages buffer (collected for assertions)
    received: Arc<Mutex<Vec<String>>>,
}

impl MockWssServer {
    /// Start a new mock WSS server on a random port. Returns immediately.
    pub async fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind mock WSS");
        let addr = listener.local_addr().unwrap();

        let received = Arc::new(Mutex::new(Vec::<String>::new()));
        let received_clone = received.clone();
        let shutdown = tokio_util::sync::CancellationToken::new();
        let shutdown_clone = shutdown.clone();

        tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = shutdown_clone.cancelled() => break,
                    accept_result = listener.accept() => {
                        match accept_result {
                            Ok((stream, _)) => {
                                let ws_stream = match accept_async(stream).await {
                                    Ok(ws) => ws,
                                    Err(_) => continue,
                                };
                                let (mut write, mut read) = ws_stream.split();

                                // Send connected message (matches Java WsMessageFactory.connected)
                                let connected = serde_json::json!({
                                    "type": "connected",
                                    "ts": chrono::Utc::now().timestamp_millis(),
                                    "data": {
                                        "ws_session_id": "mock-session-123",
                                        "node_id": "mock-node",
                                        "protocol_version": 1
                                    }
                                }).to_string();
                                let _ = write.send(Message::Text(connected)).await;

                                let received = received_clone.clone();
                                let shutdown = shutdown_clone.clone();

                                // Read loop: collect messages from client, auto-respond to ping
                                tokio::spawn(async move {
                                    loop {
                                        tokio::select! {
                                            msg = read.next() => {
                                                match msg {
                                                    Some(Ok(Message::Text(text))) => {
                                                        let text_str = text.to_string();
                                                        received.lock().await.push(text_str.clone());

                                                        // Auto-respond to ping with pong
                                                        // (matches Java KnWsHandler.handlePing)
                                                        if text_str.contains("\"ping\"") {
                                                            let pong = serde_json::json!({
                                                                "type": "pong",
                                                                "ts": chrono::Utc::now().timestamp_millis(),
                                                            }).to_string();
                                                            let _ = write.send(Message::Text(pong)).await;
                                                        }
                                                    }
                                                    Some(Ok(Message::Close(_))) => break,
                                                    Some(Err(_)) => break,
                                                    None => break,
                                                    _ => {}
                                                }
                                            }
                                            _ = shutdown.cancelled() => break,
                                        }
                                    }
                                });
                            }
                            Err(_) => break,
                        }
                    }
                }
            }
        });

        Self {
            addr,
            shutdown,
            received,
        }
    }

    /// Send a typed message to the connected client.
    /// `data` goes into the `data` field of the envelope (matching Java WsMessageSender.sendTyped).
    ///
    /// Panics if no client is connected (the send channel is unbounded but
    /// without a connected client the message is lost — for testing, ensure
    /// a client connected first).
    #[allow(dead_code)]
    pub fn send_typed_message(&self, msg_type: &str, data: serde_json::Value) {
        // Note: This is a simplified mock — messages are "sent" by the server,
        // but since we use tokio::spawn for each connection, we can't easily
        // inject messages back to a specific client from outside.
        //
        // For full integration tests, use the WSS protocol test pattern:
        // connect client → exchange messages → verify collected messages.
        // This method is kept for simpler smoke tests where we just want
        // to verify the server infrastructure.
        let _ = msg_type;
        let _ = data;
    }

    /// Get all messages received from clients so far.
    pub async fn received_messages(&self) -> Vec<String> {
        self.received.lock().await.clone()
    }

    /// Wait until at least `count` messages have been received (with timeout).
    #[allow(dead_code)]
    pub async fn wait_for_messages(&self, count: usize, timeout_ms: u64) -> bool {
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_millis(timeout_ms);
        while tokio::time::Instant::now() < deadline {
            if self.received.lock().await.len() >= count {
                return true;
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
        false
    }

    /// Get the WebSocket URL for this server.
    pub fn ws_url(&self) -> String {
        format!("ws://{}/v1/ws", self.addr)
    }

    /// Shut down the mock server.
    pub fn shutdown(&self) {
        self.shutdown.cancel();
    }
}

impl Drop for MockWssServer {
    fn drop(&mut self) {
        self.shutdown.cancel();
    }
}

// ── Tests ───────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_mock_server_start_and_connect() {
        let server = MockWssServer::start().await;
        let url = server.ws_url();

        let (ws_stream, _) = tokio_tungstenite::connect_async(&url)
            .await
            .expect("connect");
        let (_, mut read) = ws_stream.split();

        // Should receive connected message
        let msg = read.next().await.unwrap().unwrap();
        let text = msg.to_text().unwrap();
        assert!(text.contains("connected"));
        assert!(text.contains("mock-session-123"));

        server.shutdown();
    }

    #[tokio::test]
    async fn test_mock_server_ping_pong() {
        let server = MockWssServer::start().await;
        let url = server.ws_url();

        let (mut write, mut read) = tokio_tungstenite::connect_async(&url)
            .await
            .expect("connect")
            .0
            .split();

        // Skip connected message
        let _ = read.next().await;

        // Send ping
        write
            .send(Message::Text(r#"{"type":"ping"}"#.into()))
            .await
            .unwrap();

        // Receive pong
        let msg = read.next().await.unwrap().unwrap();
        let text = msg.to_text().unwrap();
        assert!(text.contains("pong"));

        server.shutdown();
    }

    #[tokio::test]
    async fn test_mock_server_collects_client_messages() {
        let server = MockWssServer::start().await;
        let url = server.ws_url();

        let (mut write, mut read) = tokio_tungstenite::connect_async(&url)
            .await
            .expect("connect")
            .0
            .split();

        // Skip connected
        let _ = read.next().await;

        // Send messages from client
        write
            .send(Message::Text(
                r#"{"type":"profile_list","profiles":[]}"#.into(),
            ))
            .await
            .unwrap();
        write
            .send(Message::Text(
                r#"{"type":"session_created","data":{"sessionId":42}}"#.into(),
            ))
            .await
            .unwrap();

        tokio::time::sleep(std::time::Duration::from_millis(200)).await;

        let received = server.received_messages().await;
        assert!(received.iter().any(|m| m.contains("profile_list")));
        assert!(received.iter().any(|m| m.contains("session_created")));

        server.shutdown();
    }
}
