use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::sync::{Mutex as AsyncMutex, OwnedMutexGuard, OwnedSemaphorePermit, Semaphore};

const DEFAULT_CONCURRENT_OPERATIONS: usize = 4;

/// Serializes Git and GitHub delivery work per registered project without
/// holding up delivery operations for any other project.
pub struct ProjectOperationGate {
    locks: Mutex<HashMap<String, Arc<AsyncMutex<()>>>>,
    permits: Arc<Semaphore>,
}

impl ProjectOperationGate {
    pub fn with_limit(max_concurrent_operations: usize) -> Self {
        assert!(
            max_concurrent_operations > 0,
            "project delivery concurrency limit must be positive"
        );
        Self {
            locks: Mutex::new(HashMap::new()),
            permits: Arc::new(Semaphore::new(max_concurrent_operations)),
        }
    }

    pub async fn lock(&self, project_key: &str) -> ProjectOperationGuard {
        let lock = {
            let mut locks = self.locks.lock().expect("project operation gate poisoned");
            locks
                .entry(project_key.to_string())
                .or_insert_with(|| Arc::new(AsyncMutex::new(())))
                .clone()
        };
        // Acquire the project lock first. Waiting operations for the same
        // project therefore do not consume global command capacity.
        let project = lock.lock_owned().await;
        let permit = self
            .permits
            .clone()
            .acquire_owned()
            .await
            .expect("project delivery semaphore must remain open");
        ProjectOperationGuard {
            _project: project,
            _permit: permit,
        }
    }
}

impl Default for ProjectOperationGate {
    fn default() -> Self {
        Self::with_limit(DEFAULT_CONCURRENT_OPERATIONS)
    }
}

pub struct ProjectOperationGuard {
    _project: OwnedMutexGuard<()>,
    _permit: OwnedSemaphorePermit,
}
