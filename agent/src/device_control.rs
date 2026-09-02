//! Device-local authority for remote write commands.
//!
//! Cloud authenticates and routes public WebSocket traffic, but the Agent is
//! the final authority for commands that change this computer.  A lease is
//! intentionally in-memory: restarting the Agent revokes control instead of
//! leaving a stale remote owner behind. A transient Cloud/WSS reconnect keeps
//! the authority because the Agent process remains the same.

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
}

#[derive(Debug, Default)]
pub struct DeviceControl {
    lease: Option<Lease>,
}

impl DeviceControl {
    pub fn claim(&mut self, connection_id: String) -> Change {
        let previous_connection_id = self.lease.as_ref().and_then(|lease| {
            (lease.connection_id != connection_id).then(|| lease.connection_id.clone())
        });
        self.lease = Some(Lease {
            connection_id: connection_id.clone(),
        });
        Change {
            connection_id,
            previous_connection_id,
            is_controller: true,
            reason: "claimed",
        }
    }

    pub fn status(&self, connection_id: &str) -> bool {
        self.lease
            .as_ref()
            .is_some_and(|lease| lease.connection_id == connection_id)
    }

    /// The lease is durable for this Agent process. The Cloud never decides
    /// it: a forged or stale connection id is rejected here. A new explicit
    /// claim or Agent restart is the only revocation path.
    pub fn authorize_write(&self, connection_id: Option<&str>) -> bool {
        let Some(connection_id) = connection_id else {
            return false;
        };
        let Some(lease) = self.lease.as_ref() else {
            return false;
        };
        if lease.connection_id != connection_id {
            return false;
        }
        true
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

    #[test]
    fn lease_survives_a_transport_reconnect_while_the_agent_is_alive() {
        let mut control = DeviceControl::default();
        control.claim("ios-a".into());

        // A Cloud/WSS reconnect does not recreate DeviceControl. The caller
        // returns with the same verified logical-client identity.
        assert!(control.status("ios-a"));
        assert!(control.authorize_write(Some("ios-a")));
    }
}
