//! 脱敏的 Agent 能力与健康摘要。
//!
//! 此模块只报告固定白名单中的结论，绝不序列化命令输出、路径、凭证或 Cloud 地址。

use serde::Serialize;
use std::sync::OnceLock;
use std::time::{Duration, Instant};
use tokio::process::Command;
use tokio::time::timeout;

const PROBE_TIMEOUT: Duration = Duration::from_secs(2);
const SNAPSHOT_CACHE_TTL: Duration = Duration::from_secs(60);

struct CachedSnapshot {
    created_at: Instant,
    snapshot: HealthSnapshot,
}

fn snapshot_cache() -> &'static tokio::sync::Mutex<Option<CachedSnapshot>> {
    static CACHE: OnceLock<tokio::sync::Mutex<Option<CachedSnapshot>>> = OnceLock::new();
    CACHE.get_or_init(|| tokio::sync::Mutex::new(None))
}

pub fn normalized_environment(value: Option<&str>) -> &'static str {
    match value {
        Some("development") => "development",
        Some("production") => "production",
        _ => "production",
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct HealthSnapshot {
    pub schema_version: u8,
    pub generated_at: i64,
    pub agent: AgentHealth,
    pub connection: ConnectionHealth,
    pub tools: Vec<ToolHealth>,
}

impl HealthSnapshot {
    #[doc(hidden)]
    pub fn new_for_test(
        version: &str,
        environment: &str,
        connection: ConnectionHealth,
        tools: Vec<ToolHealth>,
    ) -> Self {
        Self {
            schema_version: 1,
            generated_at: 0,
            agent: AgentHealth {
                version: version.to_string(),
                environment: environment.to_string(),
            },
            connection,
            tools,
        }
    }

    pub fn with_connection(mut self, connection: ConnectionHealth) -> Self {
        self.generated_at = chrono::Utc::now().timestamp_millis();
        self.connection = connection;
        self
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AgentHealth {
    pub version: String,
    pub environment: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionHealth {
    pub state: ConnectionHealthState,
}

impl ConnectionHealth {
    pub fn connected() -> Self {
        Self {
            state: ConnectionHealthState::Connected,
        }
    }

    pub fn from_agent_state(state: &str) -> Self {
        let state = match state {
            "connected" | "idle" | "running" => ConnectionHealthState::Connected,
            "reconnecting" | "bound_offline" => ConnectionHealthState::Reconnecting,
            "unbound" | "binding" => ConnectionHealthState::NotReady,
            _ => ConnectionHealthState::Unavailable,
        };
        Self { state }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ConnectionHealthState {
    Connected,
    Reconnecting,
    NotReady,
    Unavailable,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ToolHealth {
    pub name: String,
    pub state: ToolHealthState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
}

impl ToolHealth {
    pub fn available(name: &str, version: Option<String>) -> Self {
        Self {
            name: name.to_string(),
            state: ToolHealthState::Available,
            version,
        }
    }

    pub fn unavailable(name: &str, state: ToolHealthState) -> Self {
        Self {
            name: name.to_string(),
            state,
            version: None,
        }
    }

    /// `detail` 有意忽略：它可能是 CLI 输出，其中可能含用户名、路径或凭证。
    pub fn from_probe_failure(name: &str, state: ToolHealthState, _detail: &str) -> Self {
        Self::unavailable(name, state)
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ToolHealthState {
    Available,
    Missing,
    NotAuthenticated,
    TimedOut,
    Error,
}

pub async fn probe_snapshot(version: &str, environment: &str, agent_state: &str) -> HealthSnapshot {
    let (git, gh, codex, claude, qoderclicn) = tokio::join!(
        probe_tool("git"),
        probe_tool("gh"),
        probe_tool("codex"),
        probe_tool("claude"),
        probe_tool("qoderclicn"),
    );
    HealthSnapshot {
        schema_version: 1,
        generated_at: chrono::Utc::now().timestamp_millis(),
        agent: AgentHealth {
            version: version.to_string(),
            environment: environment.to_string(),
        },
        connection: ConnectionHealth::from_agent_state(agent_state),
        tools: vec![git, gh, codex, claude, qoderclicn],
    }
}

pub async fn cached_probe_snapshot(
    version: &str,
    environment: &str,
    agent_state: &str,
) -> HealthSnapshot {
    let connection = ConnectionHealth::from_agent_state(agent_state);
    let mut cache = snapshot_cache().lock().await;
    if let Some(cached) = cache.as_ref() {
        if cached.created_at.elapsed() < SNAPSHOT_CACHE_TTL
            && cached.snapshot.agent.version == version
            && cached.snapshot.agent.environment == environment
        {
            return cached.snapshot.clone().with_connection(connection);
        }
    }

    let snapshot = probe_snapshot(version, environment, agent_state).await;
    *cache = Some(CachedSnapshot {
        created_at: Instant::now(),
        snapshot: snapshot.clone(),
    });
    snapshot
}

async fn probe_tool(name: &str) -> ToolHealth {
    let version = match command_output(name, &["--version"]).await {
        Ok(Some(output)) => output,
        Ok(None) => return ToolHealth::unavailable(name, ToolHealthState::Missing),
        Err(ProbeFailure::TimedOut) => {
            return ToolHealth::unavailable(name, ToolHealthState::TimedOut)
        }
        Err(ProbeFailure::Error) => return ToolHealth::unavailable(name, ToolHealthState::Error),
    };

    if name == "gh" {
        match command_success("gh", &["auth", "status", "--hostname", "github.com"]).await {
            Ok(true) => {}
            Ok(false) => return ToolHealth::unavailable(name, ToolHealthState::NotAuthenticated),
            Err(ProbeFailure::TimedOut) => {
                return ToolHealth::unavailable(name, ToolHealthState::TimedOut)
            }
            Err(ProbeFailure::Error) => {
                return ToolHealth::unavailable(name, ToolHealthState::Error)
            }
        }
    }

    ToolHealth::available(name, extract_version(&version))
}

enum ProbeFailure {
    TimedOut,
    Error,
}

async fn command_output(program: &str, args: &[&str]) -> Result<Option<String>, ProbeFailure> {
    let mut command = Command::new(program);
    command.args(args).kill_on_drop(true);
    let output = match timeout(PROBE_TIMEOUT, command.output()).await {
        Ok(Ok(output)) => output,
        Ok(Err(error)) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Ok(Err(_)) => return Err(ProbeFailure::Error),
        Err(_) => return Err(ProbeFailure::TimedOut),
    };
    if output.status.success() {
        Ok(Some(String::from_utf8_lossy(&output.stdout).into_owned()))
    } else {
        Err(ProbeFailure::Error)
    }
}

async fn command_success(program: &str, args: &[&str]) -> Result<bool, ProbeFailure> {
    let mut command = Command::new(program);
    command.args(args).kill_on_drop(true);
    let output = match timeout(PROBE_TIMEOUT, command.output()).await {
        Ok(Ok(output)) => output,
        Ok(Err(_)) => return Err(ProbeFailure::Error),
        Err(_) => return Err(ProbeFailure::TimedOut),
    };
    Ok(output.status.success())
}

/// 从版本文本中只提取一个短版本 token，不复制任何原始输出。
fn extract_version(output: &str) -> Option<String> {
    let start = output.find(|character: char| character.is_ascii_digit())?;
    let token: String = output[start..]
        .chars()
        .take_while(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '.' | '-' | '+')
        })
        .take(32)
        .collect();
    (!token.is_empty()).then_some(token)
}

#[cfg(test)]
mod tests {
    use super::{command_output, extract_version, probe_snapshot, ProbeFailure};
    use std::fs;
    use std::process::{Command, Stdio};
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn version_extraction_discards_unrelated_output() {
        assert_eq!(
            extract_version("gh version 2.61.0 (2024-10-01)"),
            Some("2.61.0".to_string())
        );
        assert_eq!(extract_version("not-installed"), None);
    }

    #[tokio::test]
    async fn system_diagnostics_report_the_domestic_qoder_cli_name() {
        let snapshot = probe_snapshot("test", "development", "idle").await;
        let tool_names: Vec<_> = snapshot
            .tools
            .iter()
            .map(|tool| tool.name.as_str())
            .collect();

        assert!(tool_names.contains(&"qoderclicn"));
        assert!(!tool_names.contains(&"qoder"));
    }

    #[tokio::test]
    async fn timed_out_probe_terminates_the_child_process() {
        let marker = std::env::temp_dir().join(format!(
            "kn-health-probe-{}-{}.pid",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let script = format!("echo $$ > '{}'; exec sleep 10", marker.display());

        let result = command_output("sh", &["-c", &script]).await;
        assert!(matches!(result, Err(ProbeFailure::TimedOut)));

        let pid = fs::read_to_string(&marker).expect("probe should write its pid");
        let mut still_running = true;
        for _ in 0..20 {
            still_running = Command::new("kill")
                .args(["-0", pid.trim()])
                .stderr(Stdio::null())
                .status()
                .expect("kill command should run")
                .success();
            if !still_running {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(25)).await;
        }
        if still_running {
            let _ = Command::new("kill")
                .args(["-9", pid.trim()])
                .stderr(Stdio::null())
                .status();
        }
        let _ = fs::remove_file(marker);

        assert!(
            !still_running,
            "timed out health probe must not keep running"
        );
    }
}
