//! Project Manager — project registry and project-level resource discovery.
//!
//! Stores a lightweight project list in `~/.claude-profiles/projects.json`.
//! Each project maps a display name to an absolute directory path on disk.

use crate::atomic_rename;
use crate::with_write_lock;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

// ── Types ──────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProjectInfo {
    pub name: String,
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_profile: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    #[serde(default)]
    pub pinned: bool,
}

/// Lightweight session stats for a project (fast, no title extraction).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectStats {
    pub project_path: String,
    pub session_count: u32,
    pub latest_timestamp: u64,
    pub cli_types: Vec<String>,
    pub claude_count: u32,
    pub codex_count: u32,
    pub qoder_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionInfo {
    pub session_id: String,
    pub title: String,
    pub cli: String,
    pub profile: Option<String>,
    pub project_path: String,
    pub work_dir: String,
    pub timestamp: u64,
    pub status: String,
}

pub type ProjectList = Vec<ProjectInfo>;

// ── Paths ──────────────────────────────────────────────────────

fn projects_file() -> PathBuf {
    crate::config_dir().join("projects.json")
}

// ── Persistence ────────────────────────────────────────────────

fn load_projects() -> ProjectList {
    let path = projects_file();
    if !path.exists() {
        return Vec::new();
    }
    match fs::read_to_string(&path) {
        Ok(text) => serde_json::from_str(&text).unwrap_or_default(),
        Err(_) => Vec::new(),
    }
}

fn save_projects(projects: &ProjectList) -> Result<(), String> {
    with_write_lock(|| {
    let path = projects_file();
    // Ensure parent directory exists
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("创建目录失败: {}", e))?;
    }
    let text =
        serde_json::to_string_pretty(projects).map_err(|e| format!("序列化失败: {}", e))?;
    // Backup before overwriting
    if path.exists() {
        let bak = path.with_extension("json.bak");
        let _ = fs::copy(&path, &bak);
    }
    // Atomic write: tmp → fsync → rename
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, &text).map_err(|e| format!("写入失败: {}", e))?;
    #[cfg(unix)]
    {
        if let Ok(file) = fs::File::open(&tmp) {
            use std::os::unix::io::AsRawFd;
            unsafe {
                libc::fsync(file.as_raw_fd());
            }
        }
    }
    atomic_rename(&tmp, &path)?;
    Ok(())
    }) // with_write_lock
}

// ── Validation ─────────────────────────────────────────────────

fn validate_project_name(name: &str) -> Result<(), String> {
    if name.is_empty() {
        return Err("项目名称不能为空".into());
    }
    if name.len() > 128 {
        return Err("项目名称过长（最多 128 字符）".into());
    }
    if name.contains('/') || name.contains('\\') || name.contains("..") {
        return Err(format!("非法项目名称: '{}'（包含路径分隔符）", name));
    }
    Ok(())
}

fn validate_project_path(path: &str) -> Result<(), String> {
    let p = Path::new(path);
    if !p.is_absolute() {
        return Err("项目路径必须是绝对路径".into());
    }
    if !p.exists() {
        return Err(format!("目录不存在: {}", path));
    }
    if !p.is_dir() {
        return Err(format!("路径不是目录: {}", path));
    }
    Ok(())
}

// ── Tauri Commands ─────────────────────────────────────────────

#[tauri::command]
pub fn list_projects() -> ProjectList {
    load_projects()
}

#[tauri::command]
pub fn add_project(name: String, path: String) -> Result<(), String> {
    validate_project_name(&name)?;
    validate_project_path(&path)?;

    let mut projects = load_projects();

    // Check duplicate name
    if projects.iter().any(|p| p.name == name) {
        return Err(format!("项目 '{}' 已存在", name));
    }

    projects.push(ProjectInfo { name, path, default_profile: None, description: None, pinned: false });
    save_projects(&projects)
}

#[tauri::command]
pub fn remove_project(name: String) -> Result<(), String> {
    let mut projects = load_projects();
    let len_before = projects.len();
    projects.retain(|p| p.name != name);
    if projects.len() == len_before {
        return Err(format!("项目 '{}' 不存在", name));
    }
    save_projects(&projects)
}

// ── Update project ─────────────────────────────────────────────

#[tauri::command]
pub fn update_project(name: String, new_name: Option<String>, new_path: Option<String>, default_profile: Option<String>, description: Option<String>, pinned: Option<bool>) -> Result<(), String> {
    let mut projects = load_projects();
    let idx = projects.iter().position(|p| p.name == name)
        .ok_or_else(|| format!("项目 '{}' 不存在", name))?;

    if let Some(ref nn) = new_name {
        if nn != &name && projects.iter().any(|p| p.name == *nn) {
            return Err(format!("项目 '{}' 已存在", nn));
        }
        validate_project_name(nn)?;
        projects[idx].name = nn.clone();
    }
    if let Some(ref np) = new_path {
        validate_project_path(np)?;
        projects[idx].path = np.clone();
    }
    if let Some(dp) = default_profile.as_ref() {
        if dp.is_empty() {
            projects[idx].default_profile = None;
        } else {
            projects[idx].default_profile = Some(dp.clone());
        }
    }
    if let Some(ref desc) = description {
        if desc.is_empty() {
            projects[idx].description = None;
        } else {
            projects[idx].description = Some(desc.clone());
        }
    }
    if let Some(p) = pinned {
        projects[idx].pinned = p;
    }
    save_projects(&projects)?;

    if default_profile.is_some() {
        if let Err(e) = write_ai_profile_file(&projects[idx].path, projects[idx].default_profile.as_deref()) {
            eprintln!("[project_manager] 写入 .ai-profile 失败: {}", e);
        }
    }
    Ok(())
}

#[tauri::command]
pub fn toggle_pin_project(name: String, pinned: bool) -> Result<(), String> {
    let mut projects = load_projects();
    let idx = projects.iter().position(|p| p.name == name)
        .ok_or_else(|| format!("项目 '{}' 不存在", name))?;
    projects[idx].pinned = pinned;
    save_projects(&projects)
}

/// Get lightweight session stats for a list of project paths.
/// For each project, returns session count, latest timestamp, and CLI types present.
/// Designed for fast sidebar display — stat-only for Claude/Qoder, first-line read for Codex.
#[tauri::command]
pub fn get_project_stats(project_paths: Vec<String>) -> std::collections::HashMap<String, ProjectStats> {
    let home = crate::home_dir();
    let mut map = std::collections::HashMap::new();

    for path in &project_paths {
        let mut latest = 0u64;
        let mut cli_types: Vec<String> = Vec::new();

        // ── Claude ──
        let encoded = encode_project_path(path);
        let claude_dir = home.join(".claude").join("projects").join(&encoded);
        let mut claude_count = 0u32;
        if let Ok(entries) = fs::read_dir(&claude_dir) {
            for entry in entries.flatten() {
                let p = entry.path();
                if p.extension().and_then(|e| e.to_str()) == Some("jsonl") {
                    claude_count += 1;
                    if let Ok(meta) = p.metadata() {
                        if let Ok(mtime) = meta.modified() {
                            if let Ok(dur) = mtime.duration_since(std::time::UNIX_EPOCH) {
                                let ts = dur.as_millis() as u64;
                                if ts > latest { latest = ts; }
                            }
                        }
                    }
                }
            }
        }
        if claude_count > 0 { cli_types.push("claude".to_string()); }

        // ── Qoder ──
        let qoder_dir = home.join(".qoder-cn").join("projects").join(&encoded);
        let mut qoder_count = 0u32;
        if let Ok(entries) = fs::read_dir(&qoder_dir) {
            for entry in entries.flatten() {
                let p = entry.path();
                if p.extension().and_then(|e| e.to_str()) == Some("jsonl") {
                    qoder_count += 1;
                    if let Ok(meta) = p.metadata() {
                        if let Ok(mtime) = meta.modified() {
                            if let Ok(dur) = mtime.duration_since(std::time::UNIX_EPOCH) {
                                let ts = dur.as_millis() as u64;
                                if ts > latest { latest = ts; }
                            }
                        }
                    }
                }
            }
        }
        if qoder_count > 0 { cli_types.push("qoder".to_string()); }

        // ── Codex: walk sessions tree, filter by cwd ──
        let codex_root = home.join(".codex").join("sessions");
        let mut codex_count = 0u32;
        let mut codex_latest = 0u64;
        collect_codex_stats(&codex_root, path, &mut codex_count, &mut codex_latest);
        if codex_count > 0 {
            if codex_latest > latest { latest = codex_latest; }
            cli_types.push("codex".to_string());
        }

        let total = claude_count + qoder_count + codex_count;

        map.insert(path.clone(), ProjectStats {
            project_path: path.clone(),
            session_count: total,
            latest_timestamp: latest,
            cli_types,
            claude_count,
            codex_count,
            qoder_count,
        });
    }

    map
}

/// Walk Codex sessions directory and count sessions matching a project path.
fn collect_codex_stats(root: &std::path::Path, project_path: &str, count: &mut u32, latest: &mut u64) {
    let mut stack: Vec<std::path::PathBuf> = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        if let Ok(entries) = fs::read_dir(&dir) {
            for entry in entries.flatten() {
                let p = entry.path();
                if p.is_dir() {
                    stack.push(p);
                } else if p.extension().and_then(|e| e.to_str()) == Some("jsonl") {
                    if let Some(session_cwd) = read_codex_session_cwd_fast(&p) {
                        if session_cwd == project_path
                            || session_cwd.starts_with(&format!("{}/", project_path))
                            || project_path.starts_with(&format!("{}/", session_cwd))
                        {
                            *count += 1;
                            if let Ok(meta) = p.metadata() {
                                if let Ok(mtime) = meta.modified() {
                                    if let Ok(dur) = mtime.duration_since(std::time::UNIX_EPOCH) {
                                        let ts = dur.as_millis() as u64;
                                        if ts > *latest { *latest = ts; }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Quick first-line read of a Codex session to get the cwd (no title extraction).
fn read_codex_session_cwd_fast(path: &std::path::Path) -> Option<String> {
    use std::io::{BufRead, BufReader};
    let file = fs::File::open(path).ok()?;
    let mut reader = BufReader::new(file);
    let mut first_line = String::new();
    reader.read_line(&mut first_line).ok()?;
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(&first_line) {
        if v.get("type").and_then(|t| t.as_str()) == Some("session_meta") {
            if let Some(payload) = v.get("payload") {
                return payload.get("cwd").and_then(|c| c.as_str()).map(|s| s.to_string());
            }
        }
    }
    None
}

// ── .ai-profile file helpers ──────────────────────────────────

fn ai_profile_path(project_path: &str) -> PathBuf {
    Path::new(project_path).join(".ai-profile")
}

fn write_ai_profile_file(project_path: &str, default_profile: Option<&str>) -> Result<(), String> {
    let path = ai_profile_path(project_path);
    match default_profile {
        Some(p) if !p.is_empty() => {
            let content = format!("default_profile: {}\n", p);
            fs::write(&path, &content).map_err(|e| format!("写入 .ai-profile 失败: {}", e))?;
        }
        _ => {
            if path.exists() {
                fs::remove_file(&path).map_err(|e| format!("删除 .ai-profile 失败: {}", e))?;
            }
        }
    }
    Ok(())
}

#[tauri::command]
pub fn write_ai_profile(project_path: String, default_profile: String) -> Result<(), String> {
    write_ai_profile_file(&project_path, Some(&default_profile))
}

#[tauri::command]
pub fn read_ai_profile(project_path: String) -> Result<Option<String>, String> {
    let path = ai_profile_path(&project_path);
    if !path.exists() {
        return Ok(None);
    }
    let content = fs::read_to_string(&path).map_err(|e| format!("读取 .ai-profile 失败: {}", e))?;
    // Try YAML-style "default_profile: <name>"
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') { continue; }
        if let Some(val) = trimmed.strip_prefix("default_profile:") {
            let profile = val.trim().trim_matches('"').trim_matches('\'');
            if !profile.is_empty() { return Ok(Some(profile.to_string())); }
        }
    }
    // Fallback: use the first non-empty, non-comment line as bare profile name
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') { continue; }
        return Ok(Some(trimmed.to_string()));
    }
    Ok(None)
}

// ── Session Scanning ───────────────────────────────────────────

/// Encode an absolute path for Claude/Qoder project directory naming:
/// "/Users/xxx/my-project" → "-Users-xxx-my-project"
fn encode_project_path(path: &str) -> String {
    let cleaned = path.trim_start_matches('/');
    format!("-{}", cleaned.replace('/', "-"))
}

/// Scan Claude Code sessions for a given project path.
///
/// Two-phase approach for performance:
/// 1. Stat all jsonl files to get timestamps (fast, no file content read)
/// 2. Sort by timestamp, take the `max_sessions` most recent
/// 3. Extract titles only for those top sessions
fn scan_claude_sessions(project_path: &str, max_sessions: usize) -> Vec<SessionInfo> {
    let home = crate::home_dir();
    let encoded = encode_project_path(project_path);
    let sessions_dir = home.join(".claude").join("projects").join(&encoded);
    if !sessions_dir.is_dir() {
        return Vec::new();
    }

    // Phase 1: collect paths + timestamps (stat only, no file content)
    let mut entries: Vec<(String, std::path::PathBuf, u64)> = Vec::new();
    if let Ok(dir_entries) = fs::read_dir(&sessions_dir) {
        for entry in dir_entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
                continue;
            }
            let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
            let timestamp = path.metadata().ok()
                .and_then(|m| m.modified().ok())
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0);
            entries.push((stem.to_string(), path, timestamp));
        }
    }

    // Sort by timestamp descending, keep only the most recent
    entries.sort_by(|a, b| b.2.cmp(&a.2));
    entries.truncate(max_sessions);

    // Phase 2: extract titles only for the top sessions
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;

    let mut sessions: Vec<SessionInfo> = Vec::with_capacity(entries.len());
    for (session_id, path, timestamp) in entries {
        let title = extract_claude_title(&path).unwrap_or_else(|| "无标题".to_string());
        let status = if timestamp > 0 && (now_ms - timestamp) < 3600_000 {
            "active"
        } else {
            "ended"
        };
        sessions.push(SessionInfo {
            session_id,
            title,
            cli: "claude".to_string(),
            profile: None,
            project_path: project_path.to_string(),
            work_dir: project_path.to_string(),
            timestamp,
            status: status.to_string(),
        });
    }

    sessions
}

/// Extract a human-readable title from a Claude Code jsonl transcript file.
///
/// Uses BufReader (streaming) to avoid loading the entire file into memory.
/// Scans at most 80 lines in a single pass — ai-title and last-prompt always
/// appear early in the file.
fn extract_claude_title(jsonl_path: &Path) -> Option<String> {
    use std::io::{BufRead, BufReader};
    let file = fs::File::open(jsonl_path).ok()?;
    let reader = BufReader::new(file);

    let mut last_prompt: Option<String> = None;
    let mut first_user_msg: Option<String> = None;

    for line in reader.lines().take(80) {
        let line = line.ok()?;
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&line) {
            match v.get("type").and_then(|t| t.as_str()) {
                Some("ai-title") => {
                    if let Some(title) = v.get("aiTitle").and_then(|t| t.as_str()) {
                        let title = title.trim();
                        if !title.is_empty() {
                            return Some(title.chars().take(80).collect());
                        }
                    }
                }
                Some("last-prompt") => {
                    if let Some(prompt) = v.get("lastPrompt").and_then(|p| p.as_str()) {
                        let p = prompt.trim();
                        if !p.is_empty() {
                            last_prompt = Some(p.chars().take(80).collect());
                        }
                    }
                }
                Some("user") => {
                    if first_user_msg.is_none()
                        && v.get("isMeta").and_then(|m| m.as_bool()) != Some(true)
                    {
                        if let Some(msg) = v.get("message") {
                            if let Some(content) = msg.get("content") {
                                if let Some(text) = content.as_str() {
                                    let t = text.trim();
                                    if !t.is_empty() {
                                        first_user_msg = Some(t.chars().take(80).collect());
                                    }
                                }
                            }
                        }
                    }
                }
                _ => {}
            }
        }
    }

    // Priority: last-prompt > first user message
    last_prompt.or(first_user_msg)
}

/// Scan Codex sessions and filter by project path.
///
/// Walks `~/.codex/sessions/` directory tree directly (NOT session_index.jsonl,
/// which is not reliably updated). For each transcript file, reads the first line
/// (`session_meta`) to get the session id, cwd, and timestamp. Titles are resolved
/// from session_index.jsonl when available, falling back to the first user message
/// in the transcript.
fn scan_codex_sessions(project_path: &str, max_sessions: usize) -> Vec<SessionInfo> {
    let home = crate::home_dir();
    let sessions_root = home.join(".codex").join("sessions");
    if !sessions_root.is_dir() {
        return Vec::new();
    }

    // Load session_index.jsonl for titles (may be stale, that's ok — just for titles)
    let index_titles = load_codex_index_titles(&home);

    // Walk the sessions directory tree and collect matching sessions
    let mut sessions: Vec<SessionInfo> = Vec::new();
    walk_codex_sessions_dir(
        &sessions_root,
        project_path,
        &index_titles,
        &mut sessions,
    );

    // Sort by timestamp descending, take top
    sessions.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
    sessions.truncate(max_sessions);
    sessions
}

/// Build a HashMap<session_id, title> from session_index.jsonl.
fn load_codex_index_titles(home: &Path) -> std::collections::HashMap<String, String> {
    let mut map = std::collections::HashMap::new();
    let index_path = home.join(".codex").join("session_index.jsonl");
    let content = match fs::read_to_string(&index_path) {
        Ok(c) => c,
        Err(_) => return map,
    };
    for line in content.lines() {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(line.trim()) {
            if let (Some(id), Some(name)) = (
                v.get("id").and_then(|i| i.as_str()),
                v.get("thread_name").and_then(|t| t.as_str()),
            ) {
                map.insert(id.to_string(), name.chars().take(80).collect());
            }
        }
    }
    map
}

/// Recursively walk the `~/.codex/sessions/YYYY/MM/DD/` tree.
fn walk_codex_sessions_dir(
    root: &Path,
    project_path: &str,
    index_titles: &std::collections::HashMap<String, String>,
    out: &mut Vec<SessionInfo>,
) {
    let mut stack: Vec<PathBuf> = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        if let Ok(entries) = fs::read_dir(&dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    stack.push(path);
                } else if path.extension().and_then(|e| e.to_str()) == Some("jsonl") {
                    if let Some(session) = read_codex_session_meta(&path, project_path, index_titles) {
                        out.push(session);
                    }
                }
            }
        }
    }
}

/// Read session_meta from the first line of a Codex transcript file.
/// Returns Some only if the session's cwd matches the project path.
fn read_codex_session_meta(
    path: &Path,
    project_path: &str,
    index_titles: &std::collections::HashMap<String, String>,
) -> Option<SessionInfo> {
    use std::io::{BufRead, BufReader};
    let file = fs::File::open(path).ok()?;
    let mut reader = BufReader::new(file);
    let mut first_line = String::new();
    reader.read_line(&mut first_line).ok()?;

    let v: serde_json::Value = serde_json::from_str(&first_line).ok()?;
    if v.get("type").and_then(|t| t.as_str()) != Some("session_meta") {
        return None;
    }

    let payload = v.get("payload")?;
    let id = payload.get("id").and_then(|i| i.as_str())?;
    let cwd = payload.get("cwd").and_then(|c| c.as_str())?;

    // Project path matching
    let matches = cwd == project_path
        || cwd.starts_with(&format!("{}/", project_path))
        || project_path.starts_with(&format!("{}/", cwd));
    if !matches {
        return None;
    }

    let timestamp = payload
        .get("timestamp")
        .and_then(|t| t.as_str())
        .and_then(|s| parse_iso8601_to_ms(s))
        .unwrap_or(0);

    // Title: prefer index, fall back to first user message from transcript
    let title = index_titles
        .get(id)
        .cloned()
        .or_else(|| read_codex_first_user_msg(&mut reader))
        .unwrap_or_else(|| "无标题".to_string());

    Some(SessionInfo {
        session_id: id.to_string(),
        title,
        cli: "codex".to_string(),
        profile: None,
        project_path: project_path.to_string(),
        work_dir: cwd.to_string(),
        timestamp,
        status: "ended".to_string(),
    })
}

/// Read the first meaningful user message from a Codex transcript (after session_meta).
/// Codex messages are in response_item events with payload.type="message".
fn read_codex_first_user_msg<R: std::io::BufRead>(reader: &mut R) -> Option<String> {
    for _ in 0..150 {
        let mut line = String::new();
        if reader.read_line(&mut line).ok()? == 0 {
            break;
        }
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&line) {
            let ty = v.get("type").and_then(|t| t.as_str()).unwrap_or("");
            if ty != "response_item" {
                continue;
            }
            let payload = v.get("payload")?;
            if payload.get("type").and_then(|t| t.as_str()) != Some("message") {
                continue;
            }
            // Only user messages
            let role = payload.get("role").and_then(|r| r.as_str()).unwrap_or("");
            if role != "user" {
                continue;
            }
            if let Some(content) = payload.get("content").and_then(|c| c.as_array()) {
                for part in content {
                    let part_type = part.get("type").and_then(|t| t.as_str()).unwrap_or("");
                    if part_type == "input_text" {
                        if let Some(text) = part.get("text").and_then(|t| t.as_str()) {
                            let t = text.trim();
                            // Skip system instructions
                            if !t.is_empty() && !t.starts_with('<') && t.len() > 5 {
                                return Some(t.chars().take(80).collect());
                            }
                        }
                    }
                }
            }
        }
    }
    None
}

fn parse_iso8601_to_ms(s: &str) -> Option<u64> {
    let cleaned = s.replace('T', " ").replace('Z', "");
    let parts: Vec<&str> = cleaned.split(|c: char| c == '-' || c == ':' || c == ' ' || c == '.').collect();
    if parts.len() < 6 { return None; }
    let year: i64 = parts[0].parse().ok()?;
    let month: u32 = parts[1].parse().ok()?;
    let day: u32 = parts[2].parse().ok()?;
    let hour: u32 = parts[3].parse().ok()?;
    let min: u32 = parts[4].parse().ok()?;
    let sec: u32 = parts[5].parse().ok()?;
    let days_before_year = |y: i64| -> i64 {
        let y = y - 1;
        365 * y + y / 4 - y / 100 + y / 400
    };
    let days_in_month = |m: u32, leap: bool| -> i64 {
        match m {
            1 => 31, 2 => if leap { 29 } else { 28 }, 3 => 31, 4 => 30,
            5 => 31, 6 => 30, 7 => 31, 8 => 31, 9 => 30, 10 => 31,
            11 => 30, 12 => 31, _ => 0,
        }
    };
    let is_leap = |y: i64| -> bool { (y % 4 == 0 && y % 100 != 0) || y % 400 == 0 };
    let epoch_days = days_before_year(1970);
    let total_days = days_before_year(year) - epoch_days
        + (1..month).map(|m| days_in_month(m, is_leap(year))).sum::<i64>()
        + (day as i64) - 1;
    let total_secs = total_days * 86400 + hour as i64 * 3600 + min as i64 * 60 + sec as i64;
    Some(total_secs.max(0) as u64 * 1000)
}

/// Scan Qoder sessions — filesystem first (fast readdir), CLI as fallback with timeout.
fn scan_qoder_sessions(project_path: &str) -> Vec<SessionInfo> {
    // Filesystem scan is fast (pure readdir + jsonl parsing), try it first.
    let fs_sessions = scan_qoder_filesystem(project_path);
    if !fs_sessions.is_empty() {
        return fs_sessions;
    }
    // Fall back to CLI only when filesystem scan is empty (e.g. sessions were
    // created by a newer qoder version that changed its storage format).
    scan_qoder_cli(project_path).unwrap_or_default()
}

fn scan_qoder_cli(project_path: &str) -> Option<Vec<SessionInfo>> {
    let binary = crate::commands::find_binary(&["qoderclicn"])?;
    let mut child = std::process::Command::new(binary)
        .args(["--list-sessions"])
        .current_dir(project_path)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .ok()?;

    // Wait up to 2 seconds for the CLI to respond, then kill if still running
    let timeout = std::time::Duration::from_secs(2);
    let start = std::time::Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                if !status.success() {
                    return None;
                }
                let stdout = std::io::read_to_string(child.stdout.take()?).ok()?;
                return parse_qoder_list_output(&stdout, project_path);
            }
            Ok(None) => {
                if start.elapsed() > timeout {
                    let _ = child.kill();
                    let _ = child.wait();
                    return None;
                }
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
            Err(_) => return None,
        }
    }
}

/// Parse qoderclicn --list-sessions output:
/// "  1. 帮我检查下 终端分屏 (11 hours ago) [f2b8d3d2-...]"
fn parse_qoder_list_output(output: &str, project_path: &str) -> Option<Vec<SessionInfo>> {
    let mut sessions = Vec::new();
    let re = regex_lite::Regex::new(
        r"^\s*(\d+)\.\s+(.+?)\s+\((.+?)\)\s+\[([a-f0-9-]+)\]"
    ).ok()?;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;

    for line in output.lines() {
        if let Some(caps) = re.captures(line) {
            let session_id = caps.get(4).map(|m| m.as_str()).unwrap_or("").to_string();
            let title: String = caps.get(2).map(|m| m.as_str()).unwrap_or("无标题")
                .chars().take(80).collect();
            let time_ago = caps.get(3).map(|m| m.as_str()).unwrap_or("");
            let timestamp = parse_qoder_time_ago(time_ago, now);

            sessions.push(SessionInfo {
                session_id,
                title,
                cli: "qoder".to_string(),
                profile: None,
                project_path: project_path.to_string(),
                work_dir: project_path.to_string(),
                timestamp,
                status: "ended".to_string(),
            });
        }
    }
    if sessions.is_empty() { return None; }
    sessions.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
    Some(sessions)
}

fn parse_qoder_time_ago(time_ago: &str, now_ms: u64) -> u64 {
    let ago = time_ago.trim();
    let parts: Vec<&str> = ago.split_whitespace().collect();
    if parts.len() < 2 { return 0; }
    let num: f64 = parts[0].parse().unwrap_or(0.0);
    let unit = parts[1].to_lowercase();
    let ms_per_unit: f64 = match unit.as_str() {
        "minute" | "minutes" => 60_000.0,
        "hour" | "hours" => 3_600_000.0,
        "day" | "days" => 86_400_000.0,
        "week" | "weeks" => 604_800_000.0,
        "month" | "months" => 2_592_000_000.0,
        _ => 0.0,
    };
    if ms_per_unit == 0.0 { return 0; }
    let elapsed = (num * ms_per_unit) as u64;
    now_ms.saturating_sub(elapsed)
}

fn scan_qoder_filesystem(project_path: &str) -> Vec<SessionInfo> {
    let home = crate::home_dir();
    let encoded = encode_project_path(project_path);
    let sessions_dir = home.join(".qoder-cn").join("projects").join(&encoded);
    if !sessions_dir.is_dir() {
        return Vec::new();
    }
    let mut sessions: Vec<SessionInfo> = Vec::new();
    if let Ok(entries) = fs::read_dir(&sessions_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
                continue;
            }
            let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
            let session_id = stem.to_string();
            let title = extract_claude_title(&path).unwrap_or_else(|| "无标题".to_string());
            let timestamp = path.metadata().ok()
                .and_then(|m| m.modified().ok())
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0);
            sessions.push(SessionInfo {
                session_id,
                title,
                cli: "qoder".to_string(),
                profile: None,
                project_path: project_path.to_string(),
                work_dir: project_path.to_string(),
                timestamp,
                status: "ended".to_string(),
            });
        }
    }
    sessions.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
    sessions
}

/// Read the first few meaningful messages from a session transcript for inline preview.
#[tauri::command]
pub fn read_session_preview(cli: String, project_path: String, session_id: String) -> Vec<String> {
    let home = crate::home_dir();
    let file_path = find_session_file(&home, &cli, &project_path, &session_id);
    let file_path = match file_path {
        Some(p) => p,
        None => return Vec::new(),
    };

    use std::io::{BufRead, BufReader};
    let file = match fs::File::open(&file_path) {
        Ok(f) => f,
        Err(_) => return Vec::new(),
    };
    let reader = BufReader::new(file);
    let mut messages: Vec<String> = Vec::new();

    for line in reader.lines().take(150) {
        let line = match line {
            Ok(l) => l,
            Err(_) => continue,
        };
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&line) {
            let ty = v.get("type").and_then(|t| t.as_str()).unwrap_or("");
            let text = match ty {
                "user" => {
                    v.get("message")
                        .and_then(|m| m.get("content"))
                        .and_then(|c| c.as_str())
                        .map(|s| s.to_string())
                        .or_else(|| {
                            v.get("message")
                                .and_then(|m| m.get("content"))
                                .and_then(|c| c.as_array())
                                .and_then(|parts| {
                                    parts.iter()
                                        .filter(|p| p.get("type").and_then(|t| t.as_str()) == Some("text"))
                                        .filter_map(|p| p.get("text").and_then(|t| t.as_str()))
                                        .collect::<Vec<_>>()
                                        .join("")
                                        .into()
                                })
                        })
                }
                "assistant" => {
                    v.get("message")
                        .and_then(|m| m.get("content"))
                        .and_then(|c| c.as_array())
                        .and_then(|parts| {
                            let texts: Vec<&str> = parts.iter()
                                .filter(|p| p.get("type").and_then(|t| t.as_str()) == Some("text"))
                                .filter_map(|p| p.get("text").and_then(|t| t.as_str()))
                                .collect();
                            if texts.is_empty() { None } else { Some(texts.join(" ")) }
                        })
                        .or_else(|| {
                            v.get("message")
                                .and_then(|m| m.get("content"))
                                .and_then(|c| c.as_str())
                                .map(|s| s.to_string())
                        })
                }
                // Codex format: messages are nested in response_item.payload
                // payload = {type:"message", content:[{type:"input_text"|"output_text", text:"..."}]}
                "response_item" => {
                    v.get("payload").and_then(|payload| {
                        if payload.get("type").and_then(|t| t.as_str()) != Some("message") {
                            return None;
                        }
                        payload.get("content")
                            .and_then(|c| c.as_array())
                            .map(|parts| {
                                parts.iter()
                                    .filter(|p| {
                                        let t = p.get("type").and_then(|t| t.as_str()).unwrap_or("");
                                        t == "input_text" || t == "output_text"
                                    })
                                    .filter_map(|p| p.get("text").and_then(|t| t.as_str()))
                                    .collect::<Vec<_>>()
                                    .join(" ")
                            })
                            .filter(|s| !s.is_empty())
                    })
                }
                _ => None,
            };

            if let Some(t) = text {
                let trimmed = t.trim();
                if trimmed.len() > 5
                    && !trimmed.starts_with('<')
                    && !trimmed.starts_with("Filesystem")
                {
                    messages.push(trimmed.chars().take(200).collect());
                    if messages.len() >= 4 { break; }
                }
            }
        }
    }

    messages
}

/// Find a session transcript file on disk.
fn find_session_file(home: &Path, cli: &str, project_path: &str, session_id: &str) -> Option<PathBuf> {
    match cli {
        "claude" => {
            let encoded = encode_project_path(project_path);
            let dir = home.join(".claude").join("projects").join(&encoded);
            let path = dir.join(format!("{}.jsonl", session_id));
            if path.exists() { Some(path) } else { None }
        }
        "codex" => {
            // Walk sessions tree to find the file containing this session ID
            let root = home.join(".codex").join("sessions");
            let mut stack = vec![root];
            while let Some(dir) = stack.pop() {
                if let Ok(entries) = fs::read_dir(&dir) {
                    for entry in entries.flatten() {
                        let p = entry.path();
                        if p.is_dir() {
                            stack.push(p);
                        } else if p.extension().and_then(|e| e.to_str()) == Some("jsonl") {
                            let fname = p.file_name()
                                .and_then(|n| n.to_str())
                                .unwrap_or("");
                            if fname.contains(session_id) {
                                return Some(p);
                            }
                        }
                    }
                }
            }
            None
        }
        "qoder" => {
            let encoded = encode_project_path(project_path);
            let dir = home.join(".qoder-cn").join("projects").join(&encoded);
            let path = dir.join(format!("{}.jsonl", session_id));
            if path.exists() { Some(path) } else { None }
        }
        _ => None,
    }
}

#[tauri::command]
pub fn scan_project_sessions(project_path: String, cli: Option<String>) -> Vec<SessionInfo> {
    const MAX_SESSIONS: usize = 50;
    let cli_filter = cli.unwrap_or_default();
    let mut all: Vec<SessionInfo> = Vec::new();

    if cli_filter.is_empty() || cli_filter == "claude" {
        all.extend(scan_claude_sessions(&project_path, MAX_SESSIONS));
    }
    if cli_filter.is_empty() || cli_filter == "codex" {
        all.extend(scan_codex_sessions(&project_path, MAX_SESSIONS));
    }
    if cli_filter.is_empty() || cli_filter == "qoder" {
        all.extend(scan_qoder_sessions(&project_path));
    }

    all.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
    all.truncate(MAX_SESSIONS);
    all
}
