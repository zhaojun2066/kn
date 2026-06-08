//! Project Manager — project registry and project-level resource discovery.
//!
//! Stores a lightweight project list in `~/.claude-profiles/projects.json`.
//! Each project maps a display name to an absolute directory path on disk.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

// ── Types ──────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProjectInfo {
    pub name: String,
    pub path: String,
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
    fs::rename(&tmp, &path).map_err(|e| format!("重命名失败: {}", e))?;
    Ok(())
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

    projects.push(ProjectInfo { name, path });
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
