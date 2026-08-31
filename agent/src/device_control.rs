//! Device-local authority for remote write commands.
//!
//! Cloud authenticates and routes public WebSocket traffic, but the Agent is
//! the final authority for commands that change this computer.  A lease is
//! intentionally in-memory: restarting or disconnecting the Agent revokes
//! control instead of leaving a stale remote owner behind.

use std::time::{Duration, Instant};

const LEASE_TTL: Duration = Duration::from_secs(90);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Change {
    pub connection_id: String,
    pub previous_connection_id: Option<String>,
    pub is_controller: bool,
    pub reason: &'static str,
}

#[derive(Debug, Clone)]
struct Lease {
    connection_id: String,
    expires_at: Instant,
}

#[derive(Debug, Default)]
pub struct DeviceControl {
    lease: Option<Lease>,
}

impl DeviceControl {
    pub fn claim(&mut self, connection_id: String) -> Change {
        self.expire_if_needed();
        let previous_connection_id = self.lease.as_ref().and_then(|lease| {
            (lease.connection_id != connection_id).then(|| lease.connection_id.clone())
        });
        self.lease = Some(Lease {
            connection_id: connection_id.clone(),
            expires_at: Instant::now() + LEASE_TTL,
        });
        Change {
            connection_id,
            previous_connection_id,
            is_controller: true,
            reason: "claimed",
        }
    }

    pub fn status(&mut self, connection_id: &str) -> bool {
        self.expire_if_needed();
        self.lease
            .as_ref()
            .is_some_and(|lease| lease.connection_id == connection_id)
    }

    /// A successful write renews the device-local lease.  The Cloud never
    /// decides this: a forged or stale connection id is rejected here.
    pub fn authorize_write(&mut self, connection_id: Option<&str>) -> bool {
        self.expire_if_needed();
        let Some(connection_id) = connection_id else {
            return false;
        };
        let Some(lease) = self.lease.as_mut() else {
            return false;
        };
        if lease.connection_id != connection_id {
            return false;
        }
        lease.expires_at = Instant::now() + LEASE_TTL;
        true
    }

    fn expire_if_needed(&mut self) {
        if self
            .lease
            .as_ref()
            .is_some_and(|lease| lease.expires_at <= Instant::now())
        {
            self.lease = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claim_replaces_only_the_previous_controller() {
        let mut control = DeviceControl::default();
        assert!(control
            .claim("web-a".into())
            .previous_connection_id
            .is_none());
        let change = control.claim("ios-b".into());
        assert_eq!(change.previous_connection_id.as_deref(), Some("web-a"));
        assert!(!control.authorize_write(Some("web-a")));
        assert!(control.authorize_write(Some("ios-b")));
    }

    #[test]
    fn missing_proof_never_authorizes_a_write() {
        let mut control = DeviceControl::default();
        control.claim("web-a".into());
        assert!(!control.authorize_write(None));
        assert!(!control.authorize_write(Some("web-b")));
    }
}
