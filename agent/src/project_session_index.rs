use crate::proto::ProjectSessionIndexEntry;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::time::Duration;

pub const MAX_PROJECT_SESSIONS: usize = 30;

pub struct ProjectSessionScan {
    pub sessions: Vec<ProjectSessionIndexEntry>,
    /// False means a CLI-specific fallback could not establish a complete view;
    /// Cloud must retain previously indexed rows instead of deleting them.
    pub complete: bool,
}

/// Builds the bounded, newest-first snapshot that is safe to send to Cloud.
/// The scanner deliberately hands this type metadata only; transcript content is
/// never part of the index representation.
pub fn build_snapshot(
    mut sessions: Vec<ProjectSessionIndexEntry>,
) -> Vec<ProjectSessionIndexEntry> {
    sessions.sort_by(|left, right| right.last_active_at.cmp(&left.last_active_at));
    let mut seen = std::collections::HashSet::new();
    sessions
        .into_iter()
        .filter(|session| {
            session.last_active_at > 0
                && seen.insert((session.cli.clone(), session.native_session_id.clone()))
        })
        .take(MAX_PROJECT_SESSIONS)
        .collect()
}

/// Keeps revision ownership local to an Agent process.  A revision advances only
/// when a new complete snapshot is ready to be published for that project.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct PersistedProjectRevisions {
    revisions: std::collections::HashMap<String, u64>,
}

pub struct ProjectRevisionClock {
    revisions: std::collections::HashMap<String, u64>,
    storage_path: Option<PathBuf>,
}

impl ProjectRevisionClock {
    pub fn at(storage_path: impl AsRef<Path>) -> Self {
        let storage_path = storage_path.as_ref().to_path_buf();
        let revisions = fs::read(&storage_path)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<PersistedProjectRevisions>(&bytes).ok())
            .unwrap_or_default()
            .revisions;
        Self {
            revisions,
            storage_path: Some(storage_path),
        }
    }

    pub fn default_at_config_dir() -> Self {
        Self::at(
            kn_common::path::config_dir()
                .join("project-session-index")
                .join("v1")
                .join("revisions.json"),
        )
    }

    pub fn next(&mut self, project_path: &str) -> std::io::Result<u64> {
        let previous = self.revisions.get(project_path).copied().unwrap_or(0);
        let next_revision = previous.saturating_add(1);
        self.revisions
            .insert(project_path.to_owned(), next_revision);
        if let Err(error) = self.persist() {
            self.revisions.insert(project_path.to_owned(), previous);
            return Err(error);
        }
        Ok(next_revision)
    }

    fn persist(&self) -> std::io::Result<()> {
        let Some(path) = &self.storage_path else {
            return Ok(());
        };
        let Some(parent) = path.parent() else {
            return Ok(());
        };
        fs::create_dir_all(parent)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(parent, fs::Permissions::from_mode(0o700));
        }
        let temporary = parent.join(format!(
            ".revisions-{}-{}.tmp",
            std::process::id(),
            std::time::SystemTime::now()
                .elapsed()
                .unwrap_or_default()
                .as_nanos()
        ));
        let payload = PersistedProjectRevisions {
            revisions: self.revisions.clone(),
        };
        let result = (|| -> std::io::Result<()> {
            let bytes = serde_json::to_vec(&payload).map_err(std::io::Error::other)?;
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .open(&temporary)?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                file.set_permissions(fs::Permissions::from_mode(0o600))?;
            }
            use std::io::Write;
            file.write_all(&bytes)?;
            file.sync_all()?;
            drop(file);
            fs::rename(&temporary, path)?;
            Ok(())
        })();
        if let Err(error) = &result {
            let _ = fs::remove_file(&temporary);
            tracing::warn!(path = %path.display(), %error, "无法持久化会话索引 revision");
        }
        result
    }
}

impl Default for ProjectRevisionClock {
    fn default() -> Self {
        Self {
            revisions: std::collections::HashMap::new(),
            storage_path: None,
        }
    }
}

/// Tracks local project activity so Qoder's CLI fallback is never a global
/// polling loop. The caller records native session creation and relevant file
/// changes; only then may the bounded fallback run for fifteen minutes.
#[derive(Default)]
pub struct ProjectActivityTracker {
    active_at_ms: std::collections::HashMap<String, u64>,
    last_qoderclicn_fallback_at_ms: std::collections::HashMap<String, u64>,
}

/// Coalesces bursts of file-system events per project. `begin` grants exactly
/// one caller the scan; events received while it runs become one follow-up
/// scan, returned by `finish`.
#[derive(Default)]
pub struct ProjectScanGate {
    state: std::sync::Mutex<ProjectScanGateState>,
}

#[derive(Default)]
struct ProjectScanGateState {
    running: std::collections::HashSet<String>,
    pending: std::collections::HashSet<String>,
}

impl ProjectScanGate {
    pub fn begin(&self, project_path: &str) -> bool {
        let mut state = self
            .state
            .lock()
            .expect("project session scan gate poisoned");
        if state.running.insert(project_path.to_owned()) {
            true
        } else {
            state.pending.insert(project_path.to_owned());
            false
        }
    }

    /// Finishes one scan and returns whether exactly one coalesced follow-up
    /// scan should run immediately for this project.
    pub fn finish(&self, project_path: &str) -> bool {
        let mut state = self
            .state
            .lock()
            .expect("project session scan gate poisoned");
        if state.pending.remove(project_path) {
            true
        } else {
            state.running.remove(project_path);
            false
        }
    }
}

/// Resolves only registered projects affected by native CLI history writes.
/// Claude/Qoder keep a project-encoded directory name; Codex records the cwd
/// in its first JSONL payload, so we inspect only that metadata line.
pub fn projects_affected_by_history_paths(
    changed_paths: &[PathBuf],
    registered_project_paths: &[String],
) -> Vec<String> {
    let mut affected = std::collections::BTreeSet::new();
    for changed_path in changed_paths {
        for project_path in registered_project_paths {
            if belongs_to_encoded_project_directory(changed_path, project_path)
                || codex_history_path_matches_project(changed_path, project_path)
            {
                affected.insert(project_path.clone());
            }
        }
    }
    affected.into_iter().collect()
}

fn belongs_to_encoded_project_directory(path: &Path, project_path: &str) -> bool {
    let encoded = format!(
        "-{}",
        project_path.trim_start_matches('/').replace('/', "-")
    );
    path.components()
        .any(|component| component.as_os_str() == std::ffi::OsStr::new(&encoded))
}

fn codex_history_path_matches_project(path: &Path, project_path: &str) -> bool {
    if path.extension().and_then(|extension| extension.to_str()) != Some("jsonl") {
        return false;
    }
    let Ok(file) = fs::File::open(path) else {
        return false;
    };
    let mut reader = BufReader::new(file);
    let mut first = String::new();
    if reader.read_line(&mut first).is_err() {
        return false;
    }
    let Ok(payload) = serde_json::from_str::<serde_json::Value>(&first) else {
        return false;
    };
    let Some(cwd) = payload
        .get("payload")
        .and_then(|payload| payload.get("cwd"))
        .and_then(|cwd| cwd.as_str())
    else {
        return false;
    };
    cwd == project_path
        || cwd
            .strip_prefix(project_path)
            .is_some_and(|suffix| suffix.starts_with('/'))
}

impl ProjectActivityTracker {
    pub const ACTIVE_WINDOW: Duration = Duration::from_secs(15 * 60);

    pub fn mark_active(&mut self, project_path: &str, now_ms: u64) {
        self.active_at_ms.insert(project_path.to_owned(), now_ms);
    }

    pub fn allows_qoderclicn_fallback(&self, project_path: &str, now_ms: u64) -> bool {
        self.active_at_ms
            .get(project_path)
            .is_some_and(|active_at| {
                now_ms.saturating_sub(*active_at) <= Self::ACTIVE_WINDOW.as_millis() as u64
            })
    }

    pub fn claim_qoderclicn_fallback(&mut self, project_path: &str, now_ms: u64) -> bool {
        if !self.allows_qoderclicn_fallback(project_path, now_ms) {
            return false;
        }
        if self
            .last_qoderclicn_fallback_at_ms
            .get(project_path)
            .is_some_and(|last| now_ms.saturating_sub(*last) < 60_000)
        {
            return false;
        }
        self.last_qoderclicn_fallback_at_ms
            .insert(project_path.to_owned(), now_ms);
        true
    }
}

/// Scans bounded metadata required for history navigation.  A short title may
/// be read from the CLI's session metadata or first user prompt; transcript
/// bodies and assistant output are never attached to the Cloud snapshot.
pub fn scan_project_history(
    project_path: &str,
    allow_qoderclicn_fallback: bool,
) -> ProjectSessionScan {
    let home = kn_common::path::home_dir();
    let mut sessions =
        scan_flat_project_dir(&home.join(".claude/projects"), project_path, "claude");
    let qoderclicn_sessions =
        scan_flat_project_dir(&home.join(".qoder-cn/projects"), project_path, "qoderclicn");
    let has_qoderclicn_filesystem_history = !qoderclicn_sessions.is_empty();
    // Filesystem history is authoritative and cheap. The CLI is only a
    // compatibility fallback for a missing/changed Qoderclicn history layout.
    let fallback_was_attempted = !has_qoderclicn_filesystem_history && allow_qoderclicn_fallback;
    let qoderclicn_fallback_succeeded =
        fallback_was_attempted && qoderclicn_fallback(project_path, &mut sessions);
    let qoderclicn_complete = qoderclicn_snapshot_is_complete(
        has_qoderclicn_filesystem_history,
        fallback_was_attempted,
        qoderclicn_fallback_succeeded,
    );
    sessions.extend(qoderclicn_sessions);
    let (codex_sessions, codex_complete) =
        scan_codex_sessions(&home.join(".codex/sessions"), project_path);
    sessions.extend(codex_sessions);
    ProjectSessionScan {
        sessions: build_snapshot(sessions),
        complete: qoderclicn_complete && codex_complete,
    }
}

fn qoderclicn_snapshot_is_complete(
    has_filesystem_history: bool,
    fallback_was_attempted: bool,
    fallback_succeeded: bool,
) -> bool {
    // No Qoder history and no eligible fallback is a complete absence, not a
    // failed scan. Only an attempted fallback that failed makes the snapshot
    // partial and therefore unsafe for Cloud-side deletion.
    has_filesystem_history || !fallback_was_attempted || fallback_succeeded
}

/// Mirrors the desktop scanner's filesystem-first policy. The command is only
/// a fallback and is bounded so an unavailable Qoder CLI never blocks WSS.
fn qoderclicn_fallback(project_path: &str, sessions: &mut Vec<ProjectSessionIndexEntry>) -> bool {
    let mut child = match std::process::Command::new("qoderclicn")
        .args(["--list-sessions"])
        .current_dir(project_path)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
    {
        Ok(child) => child,
        Err(_) => return false,
    };
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    loop {
        match child.try_wait() {
            Ok(Some(status)) if status.success() => {
                let output = child
                    .stdout
                    .take()
                    .and_then(|out| std::io::read_to_string(out).ok())
                    .unwrap_or_default();
                sessions.extend(parse_qoderclicn_list(&output));
                return true;
            }
            Ok(Some(_)) | Err(_) => return false,
            Ok(None) if std::time::Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                return false;
            }
            Ok(None) => std::thread::sleep(std::time::Duration::from_millis(50)),
        }
    }
}

fn parse_qoderclicn_list(output: &str) -> Vec<ProjectSessionIndexEntry> {
    let re = match regex_lite::Regex::new(r"^\s*\d+\.\s+(.+?)\s+\(.+?\)\s+\[([a-f0-9-]+)\]") {
        Ok(re) => re,
        Err(_) => return Vec::new(),
    };
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;
    output
        .lines()
        .filter_map(|line| {
            let captures = re.captures(line)?;
            Some(ProjectSessionIndexEntry {
                native_session_id: captures.get(2)?.as_str().to_owned(),
                cli: "qoderclicn".into(),
                profile: None,
                title: Some(captures.get(1)?.as_str().chars().take(80).collect()),
                summary: None,
                last_active_at: now,
            })
        })
        .collect()
}

fn scan_flat_project_dir(
    root: &Path,
    project_path: &str,
    cli: &str,
) -> Vec<ProjectSessionIndexEntry> {
    let encoded = format!(
        "-{}",
        project_path.trim_start_matches('/').replace('/', "-")
    );
    let dir = root.join(encoded);
    fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            (path.extension().and_then(|ext| ext.to_str()) == Some("jsonl"))
                .then(|| entry_for_file(&path, cli))
        })
        .collect()
}

fn scan_codex_sessions(root: &Path, project_path: &str) -> (Vec<ProjectSessionIndexEntry>, bool) {
    const MAX_FILES: usize = 300;
    let mut stack = vec![root.to_path_buf()];
    let mut candidates = Vec::new();
    while let Some(dir) = stack.pop() {
        for entry in fs::read_dir(dir).into_iter().flatten().flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            if path.extension().and_then(|ext| ext.to_str()) != Some("jsonl") {
                continue;
            }
            candidates.push(path);
        }
    }
    candidates.sort_by_key(|path| std::cmp::Reverse(modified_at(path)));
    let matching: Vec<_> = candidates
        .into_iter()
        .filter_map(|path| codex_entry_for_file(&path, project_path))
        .collect();
    let complete = matching.len() <= MAX_FILES;
    let result = matching.into_iter().take(MAX_FILES).collect();
    (result, complete)
}

fn entry_for_file(path: &Path, cli: &str) -> ProjectSessionIndexEntry {
    ProjectSessionIndexEntry {
        native_session_id: path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or_default()
            .to_owned(),
        cli: cli.to_owned(),
        profile: None,
        title: history_title_from_file(path, cli),
        summary: None,
        last_active_at: modified_at(path),
    }
}

fn codex_entry_for_file(path: &Path, project_path: &str) -> Option<ProjectSessionIndexEntry> {
    let file = fs::File::open(path).ok()?;
    let mut reader = BufReader::new(file);
    let mut first = String::new();
    reader.read_line(&mut first).ok()?;
    let payload = serde_json::from_str::<serde_json::Value>(&first)
        .ok()?
        .get("payload")?
        .clone();
    let cwd = payload.get("cwd")?.as_str()?;
    if cwd != project_path
        && !cwd
            .strip_prefix(project_path)
            .is_some_and(|suffix| suffix.starts_with('/'))
    {
        return None;
    }
    let title = extract_history_title(&mut reader, "codex");
    Some(ProjectSessionIndexEntry {
        native_session_id: payload.get("id")?.as_str()?.to_owned(),
        cli: "codex".into(),
        profile: None,
        title,
        summary: None,
        last_active_at: modified_at(path),
    })
}

fn history_title_from_file(path: &Path, cli: &str) -> Option<String> {
    let file = fs::File::open(path).ok()?;
    extract_history_title(BufReader::new(file), cli)
}

/// Mirrors the desktop project-session list, while keeping the payload bounded
/// and excluding assistant output.  Claude/Qoder expose `ai-title` and
/// `last-prompt`; Codex stores user turns as `response_item` messages.
fn extract_history_title<R: BufRead>(reader: R, cli: &str) -> Option<String> {
    const MAX_LINES: usize = 160;
    const MAX_TITLE_CHARS: usize = 80;
    let mut ai_title = None;
    let mut last_prompt = None;
    let mut first_user_prompt = None;

    for line in reader.lines().take(MAX_LINES) {
        let Ok(line) = line else {
            continue;
        };
        let Ok(value) = serde_json::from_str::<serde_json::Value>(&line) else {
            continue;
        };
        match cli {
            "claude" | "qoderclicn" => match value.get("type").and_then(|kind| kind.as_str()) {
                Some("ai-title") => {
                    ai_title = preview_text(
                        value.get("aiTitle").and_then(|text| text.as_str()),
                        MAX_TITLE_CHARS,
                    )
                }
                Some("last-prompt") => {
                    last_prompt = preview_text(
                        value.get("lastPrompt").and_then(|text| text.as_str()),
                        MAX_TITLE_CHARS,
                    )
                }
                Some("user")
                    if first_user_prompt.is_none()
                        && value.get("isMeta").and_then(|meta| meta.as_bool()) != Some(true) =>
                {
                    first_user_prompt = claude_message_text(&value, MAX_TITLE_CHARS);
                }
                _ => {}
            },
            "codex" if first_user_prompt.is_none() => {
                let payload = value.get("payload");
                if value.get("type").and_then(|kind| kind.as_str()) == Some("response_item")
                    && payload
                        .and_then(|item| item.get("type"))
                        .and_then(|kind| kind.as_str())
                        == Some("message")
                    && payload
                        .and_then(|item| item.get("role"))
                        .and_then(|role| role.as_str())
                        == Some("user")
                {
                    let user_text = payload
                        .and_then(|item| item.get("content"))
                        .and_then(|content| content.as_array())
                        .and_then(|parts| {
                            parts.iter().find(|part| {
                                part.get("type").and_then(|kind| kind.as_str())
                                    == Some("input_text")
                            })
                        })
                        .and_then(|part| part.get("text").and_then(|text| text.as_str()))
                        .filter(|text| !is_codex_bootstrap_prompt(text));
                    first_user_prompt =
                        user_text.and_then(|text| preview_text(Some(text), MAX_TITLE_CHARS));
                }
            }
            _ => {}
        }
    }
    ai_title.or(last_prompt).or(first_user_prompt)
}

/// Codex persists some client-injected startup context as a `user` message.
/// It is not a user request and must never become a history title.  Keep this
/// deliberately narrow so a real request that merely mentions AGENTS.md still
/// remains eligible for the title fallback.
fn is_codex_bootstrap_prompt(text: &str) -> bool {
    let text = text.trim_start();
    const CLIENT_INJECTED_PREFIXES: &[&str] = &[
        "<environment_context>",
        "<app-context>",
        "<skills_instructions>",
        "<permissions instructions>",
        "<recommended_plugins>",
        "<command-name>",
        "<command-message>",
        "<turn_aborted>",
        "<codex_internal_context",
        "<subagent_notification",
        "<codex_delegation>",
        "<task-notification>",
        "<local-command-stdout>",
    ];
    if CLIENT_INJECTED_PREFIXES
        .iter()
        .any(|prefix| text.starts_with(prefix))
    {
        return true;
    }

    text.starts_with("# AGENTS.md instructions for ")
        && (text.contains("<environment_context>")
            || text.contains("<app-context>")
            || text.contains("<skills_instructions>")
            || text.contains("<permissions instructions>")
            || text.contains("<INSTRUCTIONS>"))
}

fn claude_message_text(value: &serde_json::Value, max_chars: usize) -> Option<String> {
    let content = value.get("message")?.get("content")?;
    if let Some(text) = content.as_str() {
        return preview_text(Some(text), max_chars);
    }
    let text = content
        .as_array()?
        .iter()
        .filter(|part| part.get("type").and_then(|kind| kind.as_str()) == Some("text"))
        .filter_map(|part| part.get("text").and_then(|text| text.as_str()))
        .collect::<Vec<_>>()
        .join(" ");
    preview_text(Some(&text), max_chars)
}

fn preview_text(value: Option<&str>, max_chars: usize) -> Option<String> {
    let value = value?.split_whitespace().collect::<Vec<_>>().join(" ");
    let value = value.trim();
    (!value.is_empty()).then(|| value.chars().take(max_chars).collect())
}

fn modified_at(path: &Path) -> u64 {
    path.metadata()
        .ok()
        .and_then(|meta| meta.modified().ok())
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(id: &str, timestamp: u64) -> ProjectSessionIndexEntry {
        ProjectSessionIndexEntry {
            native_session_id: id.into(),
            cli: "codex".into(),
            profile: None,
            title: None,
            summary: None,
            last_active_at: timestamp,
        }
    }

    fn codex_user_record(text: &str) -> String {
        serde_json::json!({
            "type": "response_item",
            "payload": {
                "type": "message",
                "role": "user",
                "content": [{ "type": "input_text", "text": text }]
            }
        })
        .to_string()
    }

    #[test]
    fn preview_uses_cli_title_then_first_user_prompt_as_fallback() {
        let claude = concat!(
            r#"{"type":"user","message":{"content":"first request"}}"#,
            "\n",
            r#"{"type":"ai-title","aiTitle":"Generated title"}"#,
        );
        let codex = concat!(
            r#"{"type":"session_meta","payload":{"id":"s_1"}}"#,
            "\n",
            r#"{"type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"Fix history list"}]}}"#,
        );

        assert_eq!(
            extract_history_title(claude.as_bytes(), "claude"),
            Some("Generated title".into())
        );
        assert_eq!(
            extract_history_title(codex.as_bytes(), "codex"),
            Some("Fix history list".into())
        );
    }

    #[test]
    fn codex_history_title_skips_bootstrap_before_real_user_prompt() {
        let codex = concat!(
            r##"{"type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"# AGENTS.md instructions for /Users/example/project\n<environment_context>managed</environment_context>"}]}}"##,
            "\n",
            r#"{"type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"修复历史会话标题"}]}}"#,
        );

        assert_eq!(
            extract_history_title(codex.as_bytes(), "codex"),
            Some("修复历史会话标题".into())
        );
    }

    #[test]
    fn codex_history_title_keeps_real_request_that_mentions_agents_md() {
        let codex = r#"{"type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"请修改 AGENTS.md 的开发规范"}]}}"#;

        assert_eq!(
            extract_history_title(codex.as_bytes(), "codex"),
            Some("请修改 AGENTS.md 的开发规范".into())
        );
    }

    #[test]
    fn codex_history_title_is_none_when_only_bootstrap_exists() {
        let codex = r##"{"type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"# AGENTS.md instructions for /Users/example/project\n<skills_instructions>managed</skills_instructions>"}]}}"##;

        assert_eq!(extract_history_title(codex.as_bytes(), "codex"), None);
    }

    #[test]
    fn codex_history_title_skips_agents_instructions_block() {
        let codex = concat!(
            r##"{"type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"# AGENTS.md instructions for /Users/example/project\n<INSTRUCTIONS>managed</INSTRUCTIONS>"}]}}"##,
            "\n",
            r#"{"type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"修复历史会话标题"}]}}"#,
        );

        assert_eq!(
            extract_history_title(codex.as_bytes(), "codex"),
            Some("修复历史会话标题".into())
        );
    }

    #[test]
    fn codex_history_title_skips_plugin_and_command_bootstrap_blocks() {
        let codex = concat!(
            r##"{"type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"<recommended_plugins>\nplugin catalogue"}]}}"##,
            "\n",
            r##"{"type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"<command-name>review"}]}}"##,
            "\n",
            r#"{"type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"修复历史会话标题"}]}}"#,
        );

        assert_eq!(
            extract_history_title(codex.as_bytes(), "codex"),
            Some("修复历史会话标题".into())
        );
    }

    #[test]
    fn codex_history_title_keeps_real_request_that_mentions_bootstrap_markers() {
        let codex = r##"{"type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"请说明 <recommended_plugins> 字段的用途"}]}}"##;

        assert_eq!(
            extract_history_title(codex.as_bytes(), "codex"),
            Some("请说明 <recommended_plugins> 字段的用途".into())
        );
    }

    #[test]
    fn codex_history_title_skips_all_known_client_injected_context_blocks() {
        let injected = [
            "# AGENTS.md instructions for /Users/example/project\n<INSTRUCTIONS>managed</INSTRUCTIONS>",
            "<environment_context>managed</environment_context>",
            "<app-context>managed</app-context>",
            "<skills_instructions>managed</skills_instructions>",
            "<permissions instructions>managed</permissions instructions>",
            "<recommended_plugins>catalogue</recommended_plugins>",
            "<command-name>review</command-name>",
            "<command-message>managed</command-message>",
            "<turn_aborted>managed</turn_aborted>",
            "<codex_internal_context source=\"goal\">managed</codex_internal_context>",
            "<subagent_notification>managed</subagent_notification>",
            "<codex_delegation>managed</codex_delegation>",
            "<task-notification>managed</task-notification>",
            "<local-command-stdout>managed</local-command-stdout>",
        ];
        let mut records = injected
            .iter()
            .map(|text| codex_user_record(text))
            .collect::<Vec<_>>();
        records.push(codex_user_record("修复历史会话标题"));
        let codex = records.join("\n");

        assert_eq!(
            extract_history_title(codex.as_bytes(), "codex"),
            Some("修复历史会话标题".into())
        );
    }

    #[test]
    fn missing_qoderclicn_history_without_an_attempt_is_a_complete_absence() {
        assert!(qoderclicn_snapshot_is_complete(false, false, false));
    }

    #[test]
    fn snapshot_is_newest_first_and_limited_to_thirty() {
        let sessions = (0..35).map(|n| entry(&n.to_string(), n)).collect();
        let snapshot = build_snapshot(sessions);
        assert_eq!(snapshot.len(), 30);
        assert_eq!(snapshot.first().unwrap().native_session_id, "34");
        assert_eq!(snapshot.last().unwrap().native_session_id, "5");
    }

    #[test]
    fn revisions_are_monotonic_per_project() {
        let mut clock = ProjectRevisionClock::default();
        assert_eq!(clock.next("/a").unwrap(), 1);
        assert_eq!(clock.next("/b").unwrap(), 1);
        assert_eq!(clock.next("/a").unwrap(), 2);
    }

    #[test]
    fn revision_survives_clock_restart() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("revisions.json");
        let mut first = ProjectRevisionClock::at(&path);
        assert_eq!(first.next("/workspace/app").unwrap(), 1);
        drop(first);

        let mut restarted = ProjectRevisionClock::at(&path);
        assert_eq!(restarted.next("/workspace/app").unwrap(), 2);
    }

    #[test]
    fn qoderclicn_fallback_requires_recent_project_activity() {
        let tracker = ProjectActivityTracker::default();
        assert!(!tracker.allows_qoderclicn_fallback("/workspace/app", 1_000));
    }

    #[test]
    fn qoderclicn_fallback_expires_after_fifteen_minutes() {
        let mut tracker = ProjectActivityTracker::default();
        tracker.mark_active("/workspace/app", 1_000);
        assert!(tracker.allows_qoderclicn_fallback("/workspace/app", 1_000 + 15 * 60 * 1_000));
        assert!(!tracker.allows_qoderclicn_fallback("/workspace/app", 1_000 + 15 * 60 * 1_000 + 1));
    }

    #[test]
    fn qoderclicn_fallback_is_claimed_at_most_once_per_minute_per_project() {
        let mut tracker = ProjectActivityTracker::default();
        tracker.mark_active("/workspace/app", 1_000);
        assert!(tracker.claim_qoderclicn_fallback("/workspace/app", 1_000));
        assert!(!tracker.claim_qoderclicn_fallback("/workspace/app", 60_999));
        assert!(tracker.claim_qoderclicn_fallback("/workspace/app", 61_000));
    }

    #[test]
    fn scan_gate_coalesces_events_while_project_scan_is_running() {
        let gate = ProjectScanGate::default();
        assert!(gate.begin("/workspace/app"));
        assert!(!gate.begin("/workspace/app"));
        assert!(gate.finish("/workspace/app"));
        assert!(!gate.begin("/workspace/app"));
        assert!(gate.finish("/workspace/app"));
        assert!(!gate.finish("/workspace/app"));
    }

    #[test]
    fn history_events_only_schedule_the_matching_registered_project() {
        let directory = tempfile::tempdir().unwrap();
        let encoded = directory
            .path()
            .join("-workspace-app")
            .join("history.jsonl");
        fs::create_dir_all(encoded.parent().unwrap()).unwrap();
        fs::write(&encoded, "{}").unwrap();

        let affected = projects_affected_by_history_paths(
            &[encoded],
            &["/workspace/app".to_string(), "/workspace/other".to_string()],
        );
        assert_eq!(affected, vec!["/workspace/app"]);
    }

    #[test]
    fn only_an_attempted_and_failed_qoderclicn_fallback_marks_snapshot_incomplete() {
        assert!(!qoderclicn_snapshot_is_complete(false, true, false));
        assert!(qoderclicn_snapshot_is_complete(true, true, false));
        assert!(qoderclicn_snapshot_is_complete(false, true, true));
    }
}
