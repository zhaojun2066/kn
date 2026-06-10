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

    projects.push(ProjectInfo { name, path, default_profile: None });
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
pub fn update_project(name: String, new_name: Option<String>, new_path: Option<String>, default_profile: Option<String>) -> Result<(), String> {
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
    save_projects(&projects)?;

    if default_profile.is_some() {
        if let Err(e) = write_ai_profile_file(&projects[idx].path, projects[idx].default_profile.as_deref()) {
            eprintln!("[project_manager] 写入 .ai-profile 失败: {}", e);
        }
    }
    Ok(())
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
fn scan_claude_sessions(project_path: &str) -> Vec<SessionInfo> {
    let home = crate::home_dir();
    let encoded = encode_project_path(project_path);
    let sessions_dir = home.join(".claude").join("projects").join(&encoded);
    if !sessions_dir.is_dir() {
        return Vec::new();
    }

    let mut sessions: Vec<SessionInfo> = Vec::new();
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;

    if let Ok(entries) = fs::read_dir(&sessions_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
            if ext != "jsonl" {
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

            // Heuristic: if modified within last hour, consider "active"
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
    }
    sessions.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
    sessions
}

/// Extract the first user prompt from a Claude jsonl transcript file.
fn extract_claude_title(jsonl_path: &Path) -> Option<String> {
    let content = fs::read_to_string(jsonl_path).ok()?;
    for line in content.lines().take(50) {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(line) {
            if v.get("role").and_then(|r| r.as_str()) == Some("user") {
                if let Some(content) = v.get("content").and_then(|c| c.as_str()) {
                    let title = content.trim();
                    if !title.is_empty() {
                        let short: String = title.chars().take(80).collect();
                        return Some(short);
                    }
                } else if let Some(parts) = v.get("content").and_then(|c| c.as_array()) {
                    for part in parts {
                        if part.get("type").and_then(|t| t.as_str()) == Some("text") {
                            if let Some(text) = part.get("text").and_then(|t| t.as_str()) {
                                let title = text.trim();
                                if !title.is_empty() {
                                    let short: String = title.chars().take(80).collect();
                                    return Some(short);
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    None
}

/// Scan Codex sessions from ~/.codex/session_index.jsonl.
fn scan_codex_sessions(project_path: &str) -> Vec<SessionInfo> {
    let home = crate::home_dir();
    let index_path = home.join(".codex").join("session_index.jsonl");
    if !index_path.exists() {
        return Vec::new();
    }
    let content = match fs::read_to_string(&index_path) {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };

    let mut sessions: Vec<SessionInfo> = Vec::new();
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(trimmed) {
            let id = v.get("id").and_then(|i| i.as_str()).unwrap_or("");
            let thread_name = v.get("thread_name").and_then(|t| t.as_str()).unwrap_or("无标题");
            let updated_at = v.get("updated_at").and_then(|t| t.as_str()).unwrap_or("");
            let timestamp = parse_iso8601_to_ms(updated_at).unwrap_or(0);
            let title: String = thread_name.chars().take(80).collect();

            sessions.push(SessionInfo {
                session_id: id.to_string(),
                title,
                cli: "codex".to_string(),
                profile: None,
                project_path: project_path.to_string(),
                work_dir: String::new(),
                timestamp,
                status: "ended".to_string(),
            });
        }
    }
    sessions.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
    sessions
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

/// Scan Qoder sessions — CLI command first, filesystem fallback.
fn scan_qoder_sessions(project_path: &str) -> Vec<SessionInfo> {
    if let Some(sessions) = scan_qoder_cli(project_path) {
        if !sessions.is_empty() {
            return sessions;
        }
    }
    scan_qoder_filesystem(project_path)
}

fn scan_qoder_cli(project_path: &str) -> Option<Vec<SessionInfo>> {
    let binary = crate::commands::find_binary(&["qoderclicn"])?;
    let output = std::process::Command::new(binary)
        .args(["--list-sessions"])
        .current_dir(project_path)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    parse_qoder_list_output(&stdout, project_path)
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

#[tauri::command]
pub fn scan_project_sessions(project_path: String, cli: Option<String>) -> Vec<SessionInfo> {
    let cli_filter = cli.unwrap_or_default();
    let mut all: Vec<SessionInfo> = Vec::new();

    if cli_filter.is_empty() || cli_filter == "claude" {
        all.extend(scan_claude_sessions(&project_path));
    }
    if cli_filter.is_empty() || cli_filter == "codex" {
        all.extend(scan_codex_sessions(&project_path));
    }
    if cli_filter.is_empty() || cli_filter == "qoder" {
        all.extend(scan_qoder_sessions(&project_path));
    }

    all.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
    all
}
