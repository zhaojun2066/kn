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
#[expect(dead_code)]
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
        let _ = write_ai_profile_file(&projects[idx].path, projects[idx].default_profile.as_deref());
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
    for line in content.lines() {
        let trimmed = line.trim();
        if let Some(val) = trimmed.strip_prefix("default_profile:") {
            let profile = val.trim().trim_matches('"').trim_matches('\'');
            if profile.is_empty() { return Ok(None); }
            return Ok(Some(profile.to_string()));
        }
    }
    Ok(None)
}
