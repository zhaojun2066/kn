//! Hook Metadata Store — persists user-authored names and descriptions for hooks.
//!
//! Stores metadata in `~/.claude-profiles/hook-meta.yaml`, keyed by the
//! four-tuple `{cli, event_type, group_idx, hook_idx}` that uniquely identifies
//! each hook across all CLI config files.
//!
//! The metadata file is a simple YAML array:
//! ```yaml
//! - cli: claude
//!   event_type: PreToolUse
//!   group_idx: 0
//!   hook_idx: 0
//!   name: "阻止 rm -rf"
//!   description: "拦截危险的递归删除命令"
//! ```

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

use crate::config_dir;

// ── Types ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HookMeta {
    pub cli: String,
    pub event_type: String,
    pub group_idx: usize,
    pub hook_idx: usize,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

// ── Helpers ────────────────────────────────────────────────────────

fn meta_path() -> PathBuf {
    config_dir().join("hook-meta.yaml")
}

fn load_all() -> Vec<HookMeta> {
    let path = meta_path();
    if !path.exists() {
        return Vec::new();
    }
    let text = match fs::read_to_string(&path) {
        Ok(t) => t,
        Err(_) => return Vec::new(),
    };
    if text.trim().is_empty() {
        return Vec::new();
    }
    serde_yaml::from_str(&text).unwrap_or_default()
}

fn save_all(metas: &[HookMeta]) -> Result<(), String> {
    let path = meta_path();
    // Ensure directory exists
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("创建目录失败: {}", e))?;
    }
    let text = serde_yaml::to_string(metas).map_err(|e| format!("序列化 YAML 失败: {}", e))?;

    // Backup + atomic write via tmp file + rename
    let mut bak_name = path.to_string_lossy().to_string();
    bak_name.push_str(".bak");
    if path.exists() {
        let _ = fs::copy(&path, &bak_name);
    }

    let mut tmp_name = path.to_string_lossy().to_string();
    tmp_name.push_str(".tmp");
    let tmp_path = PathBuf::from(&tmp_name);
    fs::write(&tmp_path, &text).map_err(|e| format!("写入临时文件失败: {}", e))?;
    #[cfg(unix)]
    {
        if let Ok(file) = std::fs::File::open(&tmp_path) {
            use std::os::unix::io::AsRawFd;
            unsafe {
                libc::fsync(file.as_raw_fd());
            }
        }
    }
    // On Windows, std::fs::rename does not overwrite existing destinations.
    #[cfg(windows)]
    {
        if path.exists() {
            fs::remove_file(&path).map_err(|e| format!("删除文件失败: {}", e))?;
        }
    }
    if let Err(e) = fs::rename(&tmp_path, &path) {
        let _ = fs::remove_file(&tmp_path);
        if !path.exists() {
            let _ = fs::copy(&bak_name, &path);
        }
        let _ = fs::remove_file(&bak_name);
        return Err(format!("替换文件失败: {}", e));
    }
    let _ = fs::remove_file(&bak_name);
    Ok(())
}

/// Look up metadata for a specific hook. Returns `None` if no entry exists.
pub fn find_meta(
    cli: &str,
    event_type: &str,
    group_idx: usize,
    hook_idx: usize,
) -> Option<HookMeta> {
    load_all().into_iter().find(|m| {
        m.cli == cli
            && m.event_type == event_type
            && m.group_idx == group_idx
            && m.hook_idx == hook_idx
    })
}

/// Load all metadata into a lookup map for batch merging.
pub fn load_meta_map() -> std::collections::HashMap<String, HookMeta> {
    let mut map = std::collections::HashMap::new();
    for meta in load_all() {
        let key = format!(
            "{}:{}:{}:{}",
            meta.cli, meta.event_type, meta.group_idx, meta.hook_idx
        );
        map.insert(key, meta);
    }
    map
}

// ── Commands ───────────────────────────────────────────────────────

#[tauri::command]
pub fn get_hook_meta(
    cli: String,
    event_type: String,
    group_idx: usize,
    hook_idx: usize,
) -> Option<HookMeta> {
    find_meta(&cli, &event_type, group_idx, hook_idx)
}

/// Upsert metadata for a hook. Creates a new entry if none exists.
#[tauri::command]
pub fn set_hook_meta(
    cli: String,
    event_type: String,
    group_idx: usize,
    hook_idx: usize,
    name: String,
    description: Option<String>,
) -> Result<(), String> {
    let mut metas = load_all();

    // Remove existing entry if any
    metas.retain(|m| {
        !(m.cli == cli
            && m.event_type == event_type
            && m.group_idx == group_idx
            && m.hook_idx == hook_idx)
    });

    metas.push(HookMeta {
        cli,
        event_type,
        group_idx,
        hook_idx,
        name,
        description,
    });

    save_all(&metas)
}

/// Delete metadata for a specific hook.
#[tauri::command]
pub fn delete_hook_meta(
    cli: String,
    event_type: String,
    group_idx: usize,
    hook_idx: usize,
) -> Result<(), String> {
    let mut metas = load_all();
    let len_before = metas.len();
    metas.retain(|m| {
        !(m.cli == cli
            && m.event_type == event_type
            && m.group_idx == group_idx
            && m.hook_idx == hook_idx)
    });
    if metas.len() == len_before {
        // No entry found — not an error, just a no-op
        return Ok(());
    }
    save_all(&metas)
}
