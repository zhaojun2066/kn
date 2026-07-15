use rusqlite::{params, Connection};
use std::path::{Path, PathBuf};

pub struct DeliveryOutboxStore {
    path: PathBuf,
}

impl DeliveryOutboxStore {
    pub fn open(path: PathBuf) -> Result<Self, String> {
        let store = Self { path };
        let connection = store.connection()?;
        connection
            .execute_batch(
                "CREATE TABLE IF NOT EXISTS delivery_outbox (
                    request_id TEXT PRIMARY KEY,
                    message TEXT NOT NULL,
                    created_at INTEGER NOT NULL DEFAULT (unixepoch())
                );
                CREATE INDEX IF NOT EXISTS delivery_outbox_created_at ON delivery_outbox(created_at);",
            )
            .map_err(|error| error.to_string())?;
        Ok(store)
    }

    pub fn default_path(config_dir: &Path) -> PathBuf {
        config_dir.join("delivery-outbox.sqlite")
    }

    pub fn enqueue(&self, request_id: &str, message: &str, capacity: usize) -> Result<(), String> {
        let connection = self.connection()?;
        connection
            .execute(
                "INSERT INTO delivery_outbox(request_id, message) VALUES (?1, ?2)
                 ON CONFLICT(request_id) DO UPDATE SET message = excluded.message",
                params![request_id, message],
            )
            .map_err(|error| error.to_string())?;
        connection
            .execute(
                "DELETE FROM delivery_outbox WHERE request_id IN (
                    SELECT request_id FROM delivery_outbox ORDER BY rowid ASC LIMIT
                    MAX((SELECT COUNT(*) FROM delivery_outbox) - ?1, 0)
                )",
                params![capacity as i64],
            )
            .map_err(|error| error.to_string())?;
        Ok(())
    }

    pub fn pending(&self) -> Result<Vec<String>, String> {
        let connection = self.connection()?;
        let mut statement = connection
            .prepare("SELECT message FROM delivery_outbox ORDER BY rowid ASC")
            .map_err(|error| error.to_string())?;
        let messages = statement
            .query_map([], |row| row.get(0))
            .map_err(|error| error.to_string())?
            .collect::<Result<Vec<String>, _>>()
            .map_err(|error| error.to_string())?;
        Ok(messages)
    }

    pub fn acknowledge(&self, request_id: &str) -> Result<(), String> {
        self.connection()?
            .execute(
                "DELETE FROM delivery_outbox WHERE request_id = ?1",
                params![request_id],
            )
            .map_err(|error| error.to_string())?;
        Ok(())
    }

    fn connection(&self) -> Result<Connection, String> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        Connection::open(&self.path).map_err(|error| error.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn survives_reopen_and_acknowledges_by_request_id() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("delivery-outbox.sqlite");
        DeliveryOutboxStore::open(path.clone())
            .unwrap()
            .enqueue("req-1", "result", 8)
            .unwrap();
        let store = DeliveryOutboxStore::open(path).unwrap();
        assert_eq!(store.pending().unwrap(), ["result"]);
        store.acknowledge("req-1").unwrap();
        assert!(store.pending().unwrap().is_empty());
    }
}
