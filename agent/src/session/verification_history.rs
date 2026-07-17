//! 验证结果摘要的本地持久化。
//!
//! 列表只读取此摘要；完整日志仍由 `verify-runs` 管理并按更短 TTL 清理。

use serde::{Deserialize, Serialize};
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const SUMMARY_TTL: Duration = Duration::from_secs(7 * 24 * 60 * 60);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LastVerification {
    pub run_id: String,
    pub state: String,
    #[serde(rename = "startedAt")]
    pub started_at_ms: u64,
    #[serde(rename = "finishedAt")]
    pub finished_at_ms: Option<u64>,
    pub duration_ms: u64,
    pub target: String,
    pub environment: String,
    pub command_source: String,
    pub build_state: Option<String>,
    pub test_state: Option<String>,
    pub log_available: bool,
    pub is_running: bool,
}

impl LastVerification {
    pub fn as_json(&self) -> serde_json::Value {
        serde_json::to_value(self).unwrap_or(serde_json::Value::Null)
    }
}

#[derive(Debug, Clone)]
pub struct VerificationHistory {
    root: PathBuf,
}

impl VerificationHistory {
    pub fn default_at_config_dir() -> Self {
        Self::at(
            kn_common::path::config_dir()
                .join("verification-history")
                .join("v1"),
        )
    }

    pub fn at(root: impl AsRef<Path>) -> Self {
        Self {
            root: root.as_ref().to_path_buf(),
        }
    }

    pub fn save(&self, project_key: &str, summary: &LastVerification) -> io::Result<()> {
        let dir = self.root.join(project_key_digest(project_key));
        create_private_dir(&dir)?;
        let destination = dir.join("latest.json");
        let temporary = dir.join(format!(".latest-{}.tmp", std::process::id()));
        let bytes = serde_json::to_vec(summary)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;

        let result = (|| {
            let mut file = OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .open(&temporary)?;
            set_private_file_permissions(&file)?;
            file.write_all(&bytes)?;
            file.sync_all()?;
            drop(file);
            fs::rename(&temporary, &destination)?;
            sync_directory(&dir)?;
            Ok(())
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temporary);
        }
        result
    }

    pub fn load(&self, project_key: &str, now: SystemTime) -> Option<LastVerification> {
        let path = self
            .root
            .join(project_key_digest(project_key))
            .join("latest.json");
        let bytes = fs::read(&path).ok()?;
        let mut summary: LastVerification = serde_json::from_slice(&bytes).ok()?;
        if is_expired(&summary, now) {
            let _ = fs::remove_file(path);
            return None;
        }
        if summary.is_running {
            summary.state = "interrupted".to_string();
            summary.is_running = false;
            summary.finished_at_ms = Some(unix_millis(now));
            summary.log_available = false;
            let _ = self.save(project_key, &summary);
        }
        Some(summary)
    }
}

fn project_key_digest(project_key: &str) -> String {
    // `kn_common::path::hash_path` is SHA-256 based and keeps project paths
    // out of the agent's local persistence directory names.
    kn_common::path::hash_path(project_key)
}

fn is_expired(summary: &LastVerification, now: SystemTime) -> bool {
    let completed = summary.finished_at_ms.unwrap_or(summary.started_at_ms);
    let completed = UNIX_EPOCH + Duration::from_millis(completed);
    now.duration_since(completed)
        .map(|age| age > SUMMARY_TTL)
        .unwrap_or(false)
}

fn unix_millis(time: SystemTime) -> u64 {
    time.duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn create_private_dir(path: &Path) -> io::Result<()> {
    fs::create_dir_all(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

fn set_private_file_permissions(file: &File) -> io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        file.set_permissions(fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}

fn sync_directory(path: &Path) -> io::Result<()> {
    #[cfg(unix)]
    {
        File::open(path)?.sync_all()?;
    }
    Ok(())
}
