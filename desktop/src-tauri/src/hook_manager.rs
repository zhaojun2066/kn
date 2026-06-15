//! Hook Manager — scan hooks configured for Claude Code, Qoder, and Codex CLI.
//!
//! ## Hook formats
//!
//! - **Claude / Qoder**: `~/.claude/settings.json` (or `~/.qoder-cn/settings.json`) → `hooks` field (JSON)
//! - **Codex**: `~/.codex/config.toml` → `[[hooks.<EventType>]]` arrays (TOML)

use crate::atomic_rename;
use crate::with_cross_process_lock;
use crate::with_write_lock;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

// ── Types ──────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HookEntry {
    pub id: String,
    pub cli: String,
    pub event_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub matcher: Option<String>,
    pub command: String,
    pub hook_type: String,
    pub enabled: bool,
    pub source: String,
    pub path: String,
    pub group_idx: usize,
    pub hook_idx: usize,
    /// Timeout in seconds (Codex only, optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout: Option<u32>,
    /// Status message shown while hook runs (Codex only, optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status_message: Option<String>,
    /// User-authored display name (from hook-meta.yaml)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// User-authored description (from hook-meta.yaml)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Project name for project-level hooks (derived from project root dir name)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HookManagerData {
    pub hooks: Vec<HookEntry>,
}

// ── Helpers ────────────────────────────────────────────────────

fn home_dir() -> PathBuf {
    crate::home_dir()
}

/// Generate a short (8-char hex) hash of a path string.
/// Used to create unique scope keys for project-level hook IDs.
/// Atomically write content to a file: create .bak backup, write to .tmp, fsync, rename.
/// This prevents data corruption from crashes or concurrent writes mid-write.
fn atomic_write(path: &std::path::Path, content: &str) -> Result<(), String> {
    with_write_lock(|| {
    with_cross_process_lock(|| {
    // Compute backup path up front — needed for recovery on Windows rename failure.
    let mut bak_name = path.to_string_lossy().to_string();
    bak_name.push_str(".bak");

    // Create backup before modifying (best-effort)
    if path.exists() {
        let _ = fs::copy(path, &bak_name);
    }

    // Write to temp file, then rename
    let mut tmp_name = path.to_string_lossy().to_string();
    tmp_name.push_str(".tmp");
    let tmp_path = std::path::PathBuf::from(&tmp_name);

    fs::write(&tmp_path, content).map_err(|e| format!("写入临时文件失败: {}", e))?;

    // fsync on Unix to flush OS buffers before rename
    #[cfg(unix)]
    {
        if let Ok(file) = std::fs::File::open(&tmp_path) {
            use std::os::unix::io::AsRawFd;
            unsafe {
                libc::fsync(file.as_raw_fd());
            }
        }
    }

    // Use atomic_rename which handles overwrite atomically on all platforms
    // (MoveFileExW on Windows, standard rename on Unix)
    if let Err(e) = atomic_rename(&tmp_path, path) {
        let _ = fs::remove_file(&tmp_path);
        // Attempt to restore from backup if we deleted the target above
        if !path.exists() {
            let _ = fs::copy(&bak_name, path);
        }
        // Clean up backup regardless
        let _ = fs::remove_file(&bak_name);
        return Err(format!("替换文件失败: {}", e));
    }
    // Clean up backup on success
    let _ = fs::remove_file(&bak_name);
    Ok(())
    }) // with_cross_process_lock
    }) // with_write_lock
}

// ── Claude / Qoder hooks (JSON in settings.json) ──────────────

/// Scan hooks from a Claude/Qoder settings.json file.
///
/// Format:
/// ```json
/// {
///   "hooks": {
///     "Stop": [
///       {
///         "matcher": "",
///         "hooks": [
///           { "type": "command", "command": "echo hello" }
///         ]
///       }
///     ]
///   }
/// }
/// ```
fn scan_json_hooks(cli: &str, settings_path: &PathBuf, source: &str, project_name: Option<String>) -> Vec<HookEntry> {
    let mut hooks = Vec::new();
    let text = match fs::read_to_string(settings_path) {
        Ok(t) => t,
        Err(e) => {
            eprintln!(
                "[hook_manager] 无法读取 {} settings.json ({}): {}",
                cli,
                settings_path.display(),
                e
            );
            return hooks;
        }
    };
    let root: serde_json::Value = match serde_json::from_str(&text) {
        Ok(v) => v,
        Err(e) => {
            eprintln!(
                "[hook_manager] {} settings.json JSON 解析失败 ({}): {}",
                cli,
                settings_path.display(),
                e
            );
            return hooks;
        }
    };
    let hooks_obj = match root.get("hooks").and_then(|v| v.as_object()) {
        Some(o) => o,
        None => return hooks,
    };

    let path_str = settings_path.to_string_lossy().to_string();

    // Compute a unique scope key for hook IDs.
    // User-level: "user". Project-level: hash of project root directory.
    let scope_key: String = if source == "project" {
        // settings_path is e.g. /foo/proj/.claude/settings.json
        // parent() → /foo/proj/.claude → parent() → /foo/proj
        if let Some(proj_root) = settings_path
            .parent()
            .and_then(|p| p.parent())
        {
            crate::hash_path(&proj_root.to_string_lossy())
        } else {
            "project".into()
        }
    } else {
        source.to_string()
    };

    for (event_type, event_array) in hooks_obj {
        let arr = match event_array.as_array() {
            Some(a) => a,
            None => continue,
        };
        for (group_idx, group) in arr.iter().enumerate() {
            let matcher = group
                .get("matcher")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .filter(|s| !s.is_empty());

            let inner_hooks = match group.get("hooks").and_then(|v| v.as_array()) {
                Some(a) => a,
                None => continue,
            };
            for (hook_idx, hook) in inner_hooks.iter().enumerate() {
                let command = hook
                    .get("command")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let hook_type = hook
                    .get("type")
                    .and_then(|v| v.as_str())
                    .unwrap_or("command")
                    .to_string();

                // Check for disabled marker
                let disabled = hook
                    .get("_disabled")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);

                hooks.push(HookEntry {
                    id: format!("{}:hook:{}:{}:{}:{}", cli, scope_key, event_type, group_idx, hook_idx),
                    cli: cli.to_string(),
                    event_type: event_type.clone(),
                    matcher: matcher.clone(),
                    command,
                    hook_type,
                    enabled: !disabled,
                    source: source.to_string(),
                    path: path_str.clone(),
                    group_idx,
                    hook_idx,
                    timeout: None,
                    status_message: None,
                    name: None,
                    description: None,
                    project_name: project_name.clone(),
                });
            }
        }
    }

    hooks
}

fn scan_claude_hooks() -> Vec<HookEntry> {
    let path = home_dir().join(".claude").join("settings.json");
    scan_json_hooks("claude", &path, "user", None)
}

fn scan_qoder_hooks() -> Vec<HookEntry> {
    let path = home_dir().join(".qoder-cn").join("settings.json");
    scan_json_hooks("qoder", &path, "user", None)
}

// ── Codex hooks (TOML in config.toml) ─────────────────────────

/// Scan hooks from Codex config.toml.
///
/// Format (official):
/// ```toml
/// [features]
/// hooks = true
///
/// [[hooks.PreToolUse]]
/// matcher = "^Bash$"
///
/// [[hooks.PreToolUse.hooks]]
/// type = "command"
/// command = "python3 script.py"
/// timeout = 30
/// statusMessage = "Checking..."
/// ```
///
/// Also supports flat format (legacy):
/// ```toml
/// [[hooks.Stop]]
/// command = "echo hello"
/// type = "command"
/// ```
fn scan_codex_hooks_at(config_path: &PathBuf, source: &str, project_name: Option<String>) -> Vec<HookEntry> {
    let mut hooks = Vec::new();
    let text = match fs::read_to_string(&config_path) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("[hook_manager] 无法读取 Codex 配置: {}", e);
            return hooks;
        }
    };
    let root: toml::Value = match text.parse() {
        Ok(v) => v,
        Err(e) => {
            eprintln!("[hook_manager] Codex TOML 解析失败: {}", e);
            return hooks;
        }
    };

    let path_str = config_path.to_string_lossy().to_string();

    // Compute unique scope key (same logic as scan_json_hooks)
    let scope_key: String = if source == "project" {
        if let Some(proj_root) = config_path
            .parent()
            .and_then(|p| p.parent())
        {
            crate::hash_path(&proj_root.to_string_lossy())
        } else {
            "project".into()
        }
    } else {
        source.to_string()
    };

    // Check features.hooks = true
    let hooks_enabled = root
        .get("features")
        .and_then(|v| v.get("hooks"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    if !hooks_enabled {
        return hooks;
    }

    // Parse [[hooks.<EventType>]] arrays
    let hooks_table = match root.get("hooks").and_then(|v| v.as_table()) {
        Some(t) => t,
        None => return hooks,
    };

    for (event_type, event_value) in hooks_table {
        // Skip non-event keys like "state"
        if event_type == "state" {
            continue;
        }
        let arr = match event_value.as_array() {
            Some(a) => a,
            None => continue,
        };

        // Determine if this is nested format ([[hooks.Event.hooks]]) or flat format
        let has_nested = arr
            .iter()
            .any(|entry| entry.get("hooks").and_then(|v| v.as_array()).is_some());

        if has_nested {
            // Nested format: [[hooks.Event]] { matcher?, hooks: [...] }
            for (group_idx, group) in arr.iter().enumerate() {
                let matcher = group
                    .get("matcher")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
                    .filter(|s| !s.is_empty());

                let inner_hooks = match group.get("hooks").and_then(|v| v.as_array()) {
                    Some(a) => a,
                    None => continue,
                };
                for (hook_idx, hook_val) in inner_hooks.iter().enumerate() {
                    let command = hook_val
                        .get("command")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let hook_type = hook_val
                        .get("type")
                        .and_then(|v| v.as_str())
                        .unwrap_or("command")
                        .to_string();
                    let timeout = hook_val
                        .get("timeout")
                        .and_then(|v| v.as_integer())
                        .map(|t| t as u32);
                    let status_message = hook_val
                        .get("statusMessage")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string());

                    let enabled = !command.is_empty();

                    hooks.push(HookEntry {
                        id: format!("codex:hook:{}:{}:{}:{}", scope_key, event_type, group_idx, hook_idx),
                        cli: "codex".to_string(),
                        event_type: event_type.clone(),
                        matcher: matcher.clone(),
                        command,
                        hook_type,
                        enabled,
                        source: source.to_string(),
                        path: path_str.clone(),
                        group_idx,
                        hook_idx,
                        timeout,
                        status_message,
                        name: None,
                        description: None,
                        project_name: project_name.clone(),
                    });
                }
            }
        } else {
            // Flat/legacy format: [[hooks.Stop]] { command, type }
            for (idx, hook_val) in arr.iter().enumerate() {
                let command = hook_val
                    .get("command")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let hook_type = hook_val
                    .get("type")
                    .and_then(|v| v.as_str())
                    .unwrap_or("command")
                    .to_string();

                let enabled = !command.is_empty();

                hooks.push(HookEntry {
                    id: format!("codex:hook:{}:{}:0:{}", scope_key, event_type, idx),
                    cli: "codex".to_string(),
                    event_type: event_type.clone(),
                    matcher: None,
                    command,
                    hook_type,
                    enabled,
                    source: source.to_string(),
                    path: path_str.clone(),
                    group_idx: 0,
                    hook_idx: idx,
                    timeout: None,
                    status_message: None,
                    name: None,
                    description: None,
                    project_name: project_name.clone(),
                });
            }
        }
    }

    hooks
}

fn scan_codex_hooks() -> Vec<HookEntry> {
    let path = home_dir().join(".codex").join("config.toml");
    scan_codex_hooks_at(&path, "user", None)
}

// ── Project-level hook scanning ───────────────────────────────

fn scan_claude_project_hooks(project_root: &std::path::Path) -> Vec<HookEntry> {
    let path = project_root.join(".claude").join("settings.json");
    let pn = crate::project_name_from_root(project_root);
    scan_json_hooks("claude", &path, "project", pn)
}

fn scan_qoder_project_hooks(project_root: &std::path::Path) -> Vec<HookEntry> {
    // Qoder: user-level = ~/.qoder-cn/  ,  project-level = <project>/.qoder/
    let path = project_root.join(".qoder").join("settings.json");
    let pn = crate::project_name_from_root(project_root);
    scan_json_hooks("qoder", &path, "project", pn)
}

fn scan_codex_project_hooks(project_root: &std::path::Path) -> Vec<HookEntry> {
    let path = project_root.join(".codex").join("config.toml");
    let pn = crate::project_name_from_root(project_root);
    scan_codex_hooks_at(&path, "project", pn)
}

// ── Main scan command ─────────────────────────────────────────

#[tauri::command]
pub fn scan_hooks(project_path: Option<String>) -> HookManagerData {
    let mut hooks = Vec::new();
    hooks.extend(scan_claude_hooks());
    hooks.extend(scan_qoder_hooks());
    hooks.extend(scan_codex_hooks());

    // Project-level hooks
    if let Some(ref p) = project_path {
        let root = std::path::Path::new(p);
        if root.exists() && root.is_dir() {
            hooks.extend(scan_claude_project_hooks(root));
            hooks.extend(scan_qoder_project_hooks(root));
            hooks.extend(scan_codex_project_hooks(root));
        }
    }

    // Merge metadata from hook-meta.yaml
    let meta_map = crate::hook_meta::load_meta_map();
    for hook in &mut hooks {
        let key = format!(
            "{}:{}:{}:{}",
            hook.cli, hook.event_type, hook.group_idx, hook.hook_idx
        );
        if let Some(meta) = meta_map.get(&key) {
            hook.name = Some(meta.name.clone());
            hook.description = meta.description.clone();
        }

        // Mark system-managed hooks (e.g., token usage tracking) as read-only.
        // These are hooks installed by the profile manager itself — not user/store hooks.
        if hook.command.contains("record-usage.py") {
            hook.source = "system".into();
            // Default name/description for the built-in token tracking hook.
            // User-defined metadata from hook-meta.yaml takes precedence (merged above).
            if hook.name.is_none() {
                hook.name = Some("Token 用量追踪".into());
                hook.description = Some("会话结束时自动记录 token 用量（输入/输出）到 SQLite 数据库，用于费用统计和控制台仪表盘展示。".into());
            }
        }
    }

    HookManagerData { hooks }
}

// ─ Toggle hook (enable/disable) ──────────────────────────────

#[tauri::command]
pub fn toggle_hook(
    cli: String,
    event_type: String,
    group_idx: usize,
    hook_idx: usize,
    enabled: bool,
    path: String,
) -> Result<(), String> {
    let config_path = std::path::PathBuf::from(&path);
    match cli.as_str() {
        "claude" | "qoder" => toggle_json_hook(
            &config_path,
            &event_type,
            group_idx,
            hook_idx,
            enabled,
        ),
        "codex" => toggle_codex_hook_at(&config_path, &event_type, group_idx, hook_idx, enabled),
        _ => Err(format!("不支持的 CLI: {}", cli)),
    }
}

fn toggle_json_hook(
    path: &std::path::Path,
    event_type: &str,
    group_idx: usize,
    hook_idx: usize,
    enabled: bool,
) -> Result<(), String> {
    let text = fs::read_to_string(path).map_err(|e| format!("读取失败: {}", e))?;
    let mut root: serde_json::Value =
        serde_json::from_str(&text).map_err(|e| format!("解析失败: {}", e))?;

    let hooks = root
        .get_mut("hooks")
        .and_then(|v| v.as_object_mut())
        .ok_or("hooks 字段不存在")?;

    let group = hooks
        .get_mut(event_type)
        .and_then(|v| v.as_array_mut())
        .and_then(|arr| arr.get_mut(group_idx))
        .ok_or("事件类型或分组不存在")?;

    let hook = group
        .get_mut("hooks")
        .and_then(|v| v.as_array_mut())
        .and_then(|arr| arr.get_mut(hook_idx))
        .ok_or("hook 不存在")?;

    if enabled {
        hook.as_object_mut().map(|o| o.remove("_disabled"));
    } else {
        hook.as_object_mut()
            .map(|o| o.insert("_disabled".to_string(), serde_json::Value::Bool(true)));
    }

    let new_text = serde_json::to_string_pretty(&root).map_err(|e| format!("序列化失败: {}", e))?;
    atomic_write(path, &new_text)?;
    Ok(())
}

#[allow(dead_code)]
fn toggle_codex_hook(
    event_type: &str,
    group_idx: usize,
    hook_idx: usize,
    enabled: bool,
) -> Result<(), String> {
    let config_path = home_dir().join(".codex").join("config.toml");
    toggle_codex_hook_at(&config_path, event_type, group_idx, hook_idx, enabled)
}

fn toggle_codex_hook_at(
    config_path: &std::path::PathBuf,
    event_type: &str,
    group_idx: usize,
    hook_idx: usize,
    enabled: bool,
) -> Result<(), String> {
    let text = fs::read_to_string(&config_path).map_err(|e| format!("读取失败: {}", e))?;

    // Use toml_edit for comment-preserving mutation
    let mut doc: toml_edit::DocumentMut =
        text.parse().map_err(|e| format!("TOML 解析失败: {}", e))?;

    let hooks_table = doc
        .get_mut("hooks")
        .and_then(|v| v.as_table_mut())
        .ok_or("hooks 字段不存在")?;

    // Codex TOML uses [[array-of-tables]] for both nested and flat formats
    let event_arr = hooks_table
        .get_mut(event_type)
        .and_then(|v| v.as_array_of_tables_mut())
        .ok_or("事件类型不存在或格式不支持")?;

    // Determine format: nested has a "hooks" sub-array of tables
    let has_nested = event_arr.iter().any(|entry| {
        entry
            .get("hooks")
            .and_then(|v| v.as_array_of_tables())
            .is_some()
    });

    if has_nested {
        // Nested: [[hooks.EventType]] { matcher?, hooks: [[hooks.EventType.hooks]] }
        let group = event_arr.get_mut(group_idx).ok_or("分组不存在")?;
        let inner_arr = group
            .get_mut("hooks")
            .and_then(|v| v.as_array_of_tables_mut())
            .ok_or("hooks 子数组不存在")?;
        let hook_table = inner_arr.get_mut(hook_idx).ok_or("hook 不存在")?;

        if enabled {
            let backup = hook_table
                .get("_command_backup")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            if let Some(cmd) = backup {
                hook_table.insert("command", toml_edit::value(&cmd));
                hook_table.remove("_command_backup");
            } else {
                return Err(
                    "无法启用：原始命令已丢失。请手动编辑 config.toml 恢复 command 字段。"
                        .to_string(),
                );
            }
        } else {
            let cmd = hook_table
                .get("command")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            if let Some(cmd) = cmd {
                if !cmd.is_empty() {
                    hook_table.insert("_command_backup", toml_edit::value(&cmd));
                    hook_table.insert("command", toml_edit::value(""));
                }
            }
        }
    } else {
        // Flat: [[hooks.EventType]] { command, type } — each entry IS a hook
        // group_idx is always 0, hook_idx is the real index
        let hook_table = event_arr.get_mut(hook_idx).ok_or("hook 不存在")?;

        if enabled {
            let backup = hook_table
                .get("_command_backup")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            if let Some(cmd) = backup {
                hook_table.insert("command", toml_edit::value(&cmd));
                hook_table.remove("_command_backup");
            } else {
                return Err(
                    "无法启用：原始命令已丢失。请手动编辑 config.toml 恢复 command 字段。"
                        .to_string(),
                );
            }
        } else {
            let cmd = hook_table
                .get("command")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            if let Some(cmd) = cmd {
                if !cmd.is_empty() {
                    hook_table.insert("_command_backup", toml_edit::value(&cmd));
                    hook_table.insert("command", toml_edit::value(""));
                }
            }
        }
    }

    let new_text = doc.to_string();
    atomic_write(&config_path, &new_text)?;
    Ok(())
}

// ── Delete hook ───────────────────────────────────────────────

#[tauri::command]
pub fn delete_hook(
    cli: String,
    event_type: String,
    group_idx: usize,
    hook_idx: usize,
    path: String,
) -> Result<(), String> {
    let config_path = std::path::PathBuf::from(&path);
    let result = match cli.as_str() {
        "claude" | "qoder" => delete_json_hook(
            &config_path,
            &event_type,
            group_idx,
            hook_idx,
        ),
        "codex" => delete_codex_hook_at(&config_path, &event_type, group_idx, hook_idx),
        _ => Err(format!("不支持的 CLI: {}", cli)),
    };

    // Clean up metadata regardless of result (best-effort)
    let _ = crate::hook_meta::delete_hook_meta(cli, event_type, group_idx, hook_idx);

    result
}

fn delete_json_hook(
    path: &std::path::Path,
    event_type: &str,
    group_idx: usize,
    hook_idx: usize,
) -> Result<(), String> {
    let text = fs::read_to_string(path).map_err(|e| format!("读取失败: {}", e))?;
    let mut root: serde_json::Value =
        serde_json::from_str(&text).map_err(|e| format!("解析失败: {}", e))?;

    let hooks = root
        .get_mut("hooks")
        .and_then(|v| v.as_object_mut())
        .ok_or("hooks 字段不存在")?;

    let event_arr = hooks
        .get_mut(event_type)
        .and_then(|v| v.as_array_mut())
        .ok_or("事件类型或分组不存在")?;

    if group_idx >= event_arr.len() {
        return Err("分组索引超出范围".to_string());
    }

    let group = &mut event_arr[group_idx];
    let hook_arr = group
        .get_mut("hooks")
        .and_then(|v| v.as_array_mut())
        .ok_or("hooks 数组不存在")?;

    if hook_idx >= hook_arr.len() {
        return Err("hook 索引超出范围".to_string());
    }
    hook_arr.remove(hook_idx);

    // If the group's inner hooks array is now empty, remove the group from the event type array
    if hook_arr.is_empty() {
        event_arr.remove(group_idx);
    }

    // If the event type array is now empty, remove it from the hooks object
    if event_arr.is_empty() {
        hooks.remove(event_type);
    }

    let new_text = serde_json::to_string_pretty(&root).map_err(|e| format!("序列化失败: {}", e))?;
    atomic_write(path, &new_text)?;
    Ok(())
}

#[allow(dead_code)]
fn delete_codex_hook(event_type: &str, group_idx: usize, hook_idx: usize) -> Result<(), String> {
    let config_path = home_dir().join(".codex").join("config.toml");
    delete_codex_hook_at(&config_path, event_type, group_idx, hook_idx)
}

fn delete_codex_hook_at(config_path: &std::path::PathBuf, event_type: &str, group_idx: usize, hook_idx: usize) -> Result<(), String> {
    let text = fs::read_to_string(&config_path).map_err(|e| format!("读取失败: {}", e))?;

    // Use toml_edit for comment-preserving mutation
    let mut doc: toml_edit::DocumentMut =
        text.parse().map_err(|e| format!("TOML 解析失败: {}", e))?;

    let hooks = doc
        .get_mut("hooks")
        .and_then(|v| v.as_table_mut())
        .ok_or("hooks 字段不存在")?;

    let event_arr = hooks
        .get_mut(event_type)
        .and_then(|v| v.as_array_of_tables_mut())
        .ok_or("事件类型不存在或格式不支持")?;

    // Determine nested vs flat
    let has_nested = event_arr.iter().any(|entry| {
        entry
            .get("hooks")
            .and_then(|v| v.as_array_of_tables())
            .is_some()
    });

    if has_nested {
        // Nested: remove inner hook, clean up empty groups
        if group_idx >= event_arr.len() {
            return Err("分组索引超出范围".to_string());
        }
        let group = event_arr.get_mut(group_idx).ok_or("分组不存在")?;
        let inner_arr = group
            .get_mut("hooks")
            .and_then(|v| v.as_array_of_tables_mut())
            .ok_or("hooks 子数组不存在")?;

        if hook_idx >= inner_arr.len() {
            return Err("hook 索引超出范围".to_string());
        }
        inner_arr.remove(hook_idx);

        if inner_arr.is_empty() {
            event_arr.remove(group_idx);
        }
    } else {
        // Flat: each [[hooks.EventType]] IS a hook
        if hook_idx >= event_arr.len() {
            return Err("hook 索引超出范围".to_string());
        }
        event_arr.remove(hook_idx);
    }

    // Remove empty event type from hooks table
    // Re-borrow hooks after mutable borrows are dropped
    let hooks = doc
        .get_mut("hooks")
        .and_then(|v| v.as_table_mut())
        .ok_or("hooks 字段不存在")?;
    let is_empty = hooks
        .get(event_type)
        .and_then(|v| v.as_array_of_tables())
        .map(|a| a.is_empty())
        .unwrap_or(false);
    if is_empty {
        hooks.remove(event_type);
    }

    let new_text = doc.to_string();
    atomic_write(&config_path, &new_text)?;
    Ok(())
}

// ── Create hook ───────────────────────────────────────────────

#[tauri::command]
pub fn create_hook(
    cli: String,
    event_type: String,
    matcher: String,
    command: String,
    hook_type: String,
) -> Result<(), String> {
    let ht = if hook_type.is_empty() {
        "command".to_string()
    } else {
        hook_type
    };
    match cli.as_str() {
        "claude" => create_json_hook(
            &home_dir().join(".claude").join("settings.json"),
            &event_type,
            &matcher,
            &command,
            &ht,
        ),
        "qoder" => create_json_hook(
            &home_dir().join(".qoder-cn").join("settings.json"),
            &event_type,
            &matcher,
            &command,
            &ht,
        ),
        "codex" => create_codex_hook_new(&event_type, &matcher, &command, &ht),
        _ => Err(format!("不支持的 CLI: {}", cli)),
    }
}

pub fn create_json_hook(
    path: &std::path::Path,
    event_type: &str,
    matcher: &str,
    command: &str,
    hook_type: &str,
) -> Result<(), String> {
    let mut root: serde_json::Value = if path.exists() {
        let text = fs::read_to_string(path).map_err(|e| format!("读取失败: {}", e))?;
        match serde_json::from_str(&text) {
            Ok(v) => v,
            Err(e) => {
                // Back up corrupted file before overwriting
                let bak = path.with_extension("json.bak");
                let _ = fs::write(&bak, &text);
                eprintln!(
                    "警告: {} 解析失败 ({}), 已备份到 {}",
                    path.display(),
                    e,
                    bak.display()
                );
                return Err(format!(
                    "配置文件解析失败: {}. 原文件已备份到 {}",
                    e,
                    bak.display()
                ));
            }
        }
    } else {
        serde_json::Value::Object(serde_json::Map::new())
    };

    if root.get("hooks").is_none() {
        root.as_object_mut().map(|o| {
            o.insert(
                "hooks".to_string(),
                serde_json::Value::Object(serde_json::Map::new()),
            )
        });
    }

    let hooks = root
        .get_mut("hooks")
        .and_then(|v| v.as_object_mut())
        .ok_or("hooks 字段格式错误")?;

    if !hooks.contains_key(event_type) {
        hooks.insert(event_type.to_string(), serde_json::Value::Array(Vec::new()));
    }

    let event_arr = hooks
        .get_mut(event_type)
        .and_then(|v| v.as_array_mut())
        .ok_or("事件类型数组格式错误")?;

    let group = event_arr.iter_mut().find(|g| {
        let g_matcher = g.get("matcher").and_then(|v| v.as_str()).unwrap_or("");
        g_matcher == matcher
    });

    if let Some(group) = group {
        let hook_arr = group
            .get_mut("hooks")
            .and_then(|v| v.as_array_mut())
            .ok_or("hooks 数组格式错误")?;

        // Check for duplicate: same type + command combination is already present
        let duplicate = hook_arr.iter().any(|h| {
            h.get("type").and_then(|v| v.as_str()) == Some(hook_type)
                && h.get("command").and_then(|v| v.as_str()) == Some(command)
        });
        if duplicate {
            return Ok(()); // idempotent — identical hook already exists
        }

        hook_arr.push(serde_json::json!({
            "type": hook_type,
            "command": command
        }));
    } else {
        event_arr.push(serde_json::json!({
            "matcher": matcher,
            "hooks": [{
                "type": hook_type,
                "command": command
            }]
        }));
    }

    let new_text = serde_json::to_string_pretty(&root).map_err(|e| format!("序列化失败: {}", e))?;

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("创建目录失败: {}", e))?;
    }

    atomic_write(path, &new_text)?;
    Ok(())
}

pub fn create_codex_hook_new(
    event_type: &str,
    matcher: &str,
    command: &str,
    hook_type: &str,
) -> Result<(), String> {
    let config_path = home_dir().join(".codex").join("config.toml");

    // Use toml_edit for comment-preserving mutation
    let mut doc: toml_edit::DocumentMut = if config_path.exists() {
        let text = fs::read_to_string(&config_path).map_err(|e| format!("读取失败: {}", e))?;
        if text.trim().is_empty() {
            toml_edit::DocumentMut::new()
        } else {
            text.parse().map_err(|e| format!("TOML 解析失败: {}", e))?
        }
    } else {
        toml_edit::DocumentMut::new()
    };

    // Ensure features.hooks = true
    {
        if !doc.contains_key("features") {
            doc.insert("features", toml_edit::table());
        }
        let feat_table = doc["features"].as_table_mut().ok_or("features 格式错误")?;
        feat_table.insert("hooks", toml_edit::value(true));
    }

    // Ensure hooks table exists
    if !doc.contains_key("hooks") {
        doc.insert("hooks", toml_edit::table());
    }

    let hooks = doc["hooks"].as_table_mut().ok_or("hooks 字段格式错误")?;

    // Build the group entry [[hooks.EventType]] { matcher?, hooks: [[hooks.EventType.hooks]] }
    let mut group_table = toml_edit::Table::new();
    if !matcher.is_empty() {
        group_table.insert("matcher", toml_edit::value(matcher));
    }
    // Inner hooks as array of inline tables
    let mut inner_arr = toml_edit::ArrayOfTables::new();
    let mut inner_item = toml_edit::Table::new();
    inner_item.insert("type", toml_edit::value(hook_type));
    inner_item.insert("command", toml_edit::value(command));
    inner_arr.push(inner_item);
    group_table.insert("hooks", toml_edit::Item::ArrayOfTables(inner_arr));

    // Insert into hooks.<EventType> array
    if !hooks.contains_key(event_type) {
        hooks.insert(
            event_type,
            toml_edit::Item::ArrayOfTables(toml_edit::ArrayOfTables::new()),
        );
    } else if hooks[event_type].as_array_of_tables().is_none() {
        // Existing entry is a plain array, not ArrayOfTables (e.g. PreToolUse = []
        // written by the Codex CLI itself). Replace with an empty ArrayOfTables so
        // we can add hook entries. Codex plain arrays are always empty placeholders,
        // so this is a lossless conversion.
        hooks.insert(
            event_type,
            toml_edit::Item::ArrayOfTables(toml_edit::ArrayOfTables::new()),
        );
    }

    let event_arr = hooks[event_type]
        .as_array_of_tables_mut()
        .ok_or("事件类型数组格式错误")?;
    // Check for duplicate before pushing: same matcher + same inner hook (type + command)
    let already_exists = event_arr.iter().any(|existing_group| {
        let same_matcher = existing_group
            .get("matcher")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            == matcher;
        if !same_matcher {
            return false;
        }
        // Check if any inner hook matches type + command
        existing_group
            .get("hooks")
            .and_then(|v| v.as_array_of_tables())
            .map(|inner_hooks| {
                inner_hooks.iter().any(|h| {
                    h.get("type").and_then(|v| v.as_str()) == Some(hook_type)
                        && h.get("command").and_then(|v| v.as_str()) == Some(command)
                })
            })
            .unwrap_or(false)
    });
    if already_exists {
        return Ok(()); // idempotent — identical hook already exists
    }

    event_arr.push(group_table);

    // Create parent directory if needed
    if let Some(parent) = config_path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("创建目录失败: {}", e))?;
    }

    let new_text = doc.to_string();
    atomic_write(&config_path, &new_text)?;
    Ok(())
}

// ═══════════════════════════════════════════════════════════════
//  HOOK MOVE / COPY OPERATIONS
// ═══════════════════════════════════════════════════════════════

/// Info returned after a hook move for undo support.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HookMoveUndoInfo {
    pub resource_name: String,
    pub from_scope: String,
    pub to_scope: String,
    pub cli: String,
    pub event_type: String,
    /// Source position (used for restore-to-source)
    pub group_idx: usize,
    pub hook_idx: usize,
    /// JSON snapshot of the hook entry before move
    pub hook_snapshot: serde_json::Value,
    pub from_path: String,
    pub to_path: String,
    /// Target position after write (used for precise undo deletion).
    /// None for legacy undo info or if source == target (shouldn't happen for moves).
    pub target_group_idx: Option<usize>,
    pub target_hook_idx: Option<usize>,
}

/// Move a hook entry from one settings file to another.
#[tauri::command]
pub fn move_hook_entry(
    cli: String,
    event_type: String,
    group_idx: usize,
    hook_idx: usize,
    from_path: String,
    to_path: String,
    from_scope: String,
    to_scope: String,
) -> Result<HookMoveUndoInfo, String> {
    let from = std::path::PathBuf::from(&from_path);
    let to = std::path::PathBuf::from(&to_path);
    if cli == "codex" {
        move_codex_hook_entry(&cli, &event_type, group_idx, hook_idx, &from, &to, &from_scope, &to_scope)
    } else {
        move_json_hook_entry(&cli, &event_type, group_idx, hook_idx, &from, &to, &from_scope, &to_scope)
    }
}

/// Copy a hook entry from one settings file to another.
#[tauri::command]
pub fn copy_hook_entry(
    cli: String,
    event_type: String,
    group_idx: usize,
    hook_idx: usize,
    from_path: String,
    to_path: String,
) -> Result<(), String> {
    let from = std::path::PathBuf::from(&from_path);
    let to = std::path::PathBuf::from(&to_path);
    if cli == "codex" {
        copy_codex_hook_entry(&event_type, group_idx, hook_idx, &from, &to)
    } else {
        copy_json_hook_entry(&event_type, group_idx, hook_idx, &from, &to)
    }
}

/// Restore a hook snapshot to a given config file. Used to implement copy
/// (move to dest, then restore snapshot to source).
#[tauri::command]
pub fn restore_hook_snapshot(
    snapshot: serde_json::Value,
    event_type: String,
    path: String,
    cli: String,
) -> Result<(), String> {
    let p = std::path::PathBuf::from(&path);
    if cli == "codex" {
        let _ = append_codex_hook_entry(&snapshot, &event_type, &p)?;
    } else {
        let _ = write_json_hook_entry(&snapshot, &event_type, &p)?;
    }
    Ok(())
}

/// Undo a hook move: three-step target removal (position verify → snapshot search → safe delete),
/// then restore to source.
#[tauri::command]
pub fn undo_move_hook(undo_info: HookMoveUndoInfo) -> Result<(), String> {
    let to = std::path::PathBuf::from(&undo_info.to_path);
    let from = std::path::PathBuf::from(&undo_info.from_path);

    if undo_info.cli == "codex" {
        remove_codex_hook_for_undo(&undo_info, &to)?;
        restore_codex_hook_entry(&undo_info, &from)?;
    } else {
        remove_json_hook_for_undo(&undo_info, &to)?;
        restore_json_hook_entry(&undo_info, &from)?;
    }
    Ok(())
}

/// Try to remove a JSON hook from the target using the three-step strategy:
/// 1. Position-based removal with snapshot verification (fast path, 99% of cases)
/// 2. Fallback: search by content — only delete on unique match
/// 3. Multiple matches → error, don't guess
fn remove_json_hook_for_undo(undo: &HookMoveUndoInfo, path: &std::path::PathBuf) -> Result<(), String> {
    // Step 1: Try position-based removal with verification
    if let (Some(g_idx), Some(h_idx)) = (undo.target_group_idx, undo.target_hook_idx) {
        if verify_json_hook_at_position(&undo.hook_snapshot, &undo.event_type, g_idx, h_idx, path) {
            // Position match confirmed — safe to delete by index
            return remove_json_hook_by_index(&undo.event_type, g_idx, h_idx, path);
        }
    }

    // Step 2: Position failed or unavailable — fallback to content matching
    let count = count_json_hook_matches(&undo.hook_snapshot, &undo.event_type, path);
    match count {
        0 => Ok(()), // Target already missing the hook, nothing to remove
        1 => remove_json_hook_by_snapshot(&undo.hook_snapshot, &undo.event_type, path),
        _ => Err("撤销失败：目标已有重复 Hook，请手动处理".into()),
    }
}

/// Try to remove a Codex TOML hook from the target using the three-step strategy.
fn remove_codex_hook_for_undo(undo: &HookMoveUndoInfo, path: &std::path::PathBuf) -> Result<(), String> {
    // Step 1: Try position-based removal with verification
    if let (Some(g_idx), Some(h_idx)) = (undo.target_group_idx, undo.target_hook_idx) {
        if verify_codex_hook_at_position(&undo.hook_snapshot, &undo.event_type, g_idx, h_idx, path) {
            return remove_codex_hook_entry_by_idx(&undo.event_type, g_idx, h_idx, path);
        }
    }

    // Step 2: Fallback to content matching
    let count = count_codex_hook_matches(&undo.hook_snapshot, &undo.event_type, path);
    match count {
        0 => Ok(()),
        1 => remove_codex_hook_by_snapshot(&undo.hook_snapshot, &undo.event_type, path),
        _ => Err("撤销失败：目标已有重复 Hook，请手动处理".into()),
    }
}

/// Check whether the hook at (event_type, group_idx, hook_idx) in a JSON config file
/// matches the given snapshot (by command + type + matcher).
fn verify_json_hook_at_position(snapshot: &serde_json::Value, event_type: &str, group_idx: usize, hook_idx: usize, path: &std::path::PathBuf) -> bool {
    let text = match fs::read_to_string(path) { Ok(t) => t, Err(_) => return false };
    let root: serde_json::Value = match serde_json::from_str(&text) { Ok(r) => r, Err(_) => return false };
    let hooks = match root.get("hooks").and_then(|v| v.as_object()) { Some(h) => h, None => return false };
    let arr = match hooks.get(event_type).and_then(|v| v.as_array()) { Some(a) => a, None => return false };
    let group = match arr.get(group_idx) { Some(g) => g, None => return false };
    let inner = match group.get("hooks").and_then(|v| v.as_array()) { Some(i) => i, None => return false };
    let hook = match inner.get(hook_idx) { Some(h) => h, None => return false };

    let snapshot_matcher = snapshot.get("matcher").and_then(|v| v.as_str()).unwrap_or("");
    let group_matcher = group.get("matcher").and_then(|v| v.as_str()).unwrap_or("");

    group_matcher == snapshot_matcher
        && hook.get("command").and_then(|v| v.as_str()).unwrap_or("")
        == snapshot.get("command").and_then(|v| v.as_str()).unwrap_or("")
        && hook.get("type").and_then(|v| v.as_str()).unwrap_or("command")
        == snapshot.get("type").and_then(|v| v.as_str()).unwrap_or("command")
}

/// Check whether the hook at (event_type, group_idx, hook_idx) in a Codex TOML config file
/// matches the given snapshot.
fn verify_codex_hook_at_position(snapshot: &serde_json::Value, event_type: &str, group_idx: usize, hook_idx: usize, path: &std::path::PathBuf) -> bool {
    let text = match fs::read_to_string(path) { Ok(t) => t, Err(_) => return false };
    let root: toml::Value = match text.parse() { Ok(r) => r, Err(_) => return false };
    let hooks_table = match root.get("hooks").and_then(|v| v.as_table()) { Some(h) => h, None => return false };
    let arr = match hooks_table.get(event_type).and_then(|v| v.as_array()) { Some(a) => a, None => return false };
    let group = match arr.get(group_idx) { Some(g) => g, None => return false };
    let inner = match group.get("hooks").and_then(|v| v.as_array()) { Some(i) => i, None => return false };
    let hook = match inner.get(hook_idx) { Some(h) => h, None => return false };

    let snapshot_matcher = snapshot.get("matcher").and_then(|v| v.as_str()).unwrap_or("");
    let group_matcher = group.get("matcher").and_then(|v| v.as_str()).unwrap_or("");

    group_matcher == snapshot_matcher
        && hook.get("command").and_then(|v| v.as_str()).unwrap_or("")
        == snapshot.get("command").and_then(|v| v.as_str()).unwrap_or("")
        && hook.get("type").and_then(|v| v.as_str()).unwrap_or("command")
        == snapshot.get("type").and_then(|v| v.as_str()).unwrap_or("command")
}

/// Count how many hooks in a JSON config file match the given snapshot by content.
fn count_json_hook_matches(snapshot: &serde_json::Value, event_type: &str, path: &std::path::PathBuf) -> usize {
    let text = match fs::read_to_string(path) { Ok(t) => t, Err(_) => return 0 };
    let root: serde_json::Value = match serde_json::from_str(&text) { Ok(r) => r, Err(_) => return 0 };
    let cmd = snapshot.get("command").and_then(|v| v.as_str()).unwrap_or("");
    let hook_type = snapshot.get("type").and_then(|v| v.as_str()).unwrap_or("command");
    let matcher = snapshot.get("matcher").and_then(|v| v.as_str()).unwrap_or("");

    let mut count = 0;
    if let Some(hooks) = root.get("hooks").and_then(|v| v.as_object()) {
        if let Some(arr) = hooks.get(event_type).and_then(|v| v.as_array()) {
            for g in arr {
                let g_matcher = g.get("matcher").and_then(|v| v.as_str()).unwrap_or("");
                if g_matcher != matcher { continue; }
                if let Some(inner) = g.get("hooks").and_then(|v| v.as_array()) {
                    count += inner.iter().filter(|h| {
                        h.get("command").and_then(|v| v.as_str()).unwrap_or("") == cmd
                            && h.get("type").and_then(|v| v.as_str()).unwrap_or("command") == hook_type
                    }).count();
                }
            }
        }
    }
    count
}

/// Count how many hooks in a Codex TOML config file match the given snapshot by content.
fn count_codex_hook_matches(snapshot: &serde_json::Value, event_type: &str, path: &std::path::PathBuf) -> usize {
    let text = match fs::read_to_string(path) { Ok(t) => t, Err(_) => return 0 };
    let root: toml::Value = match text.parse() { Ok(r) => r, Err(_) => return 0 };
    let cmd = snapshot.get("command").and_then(|v| v.as_str()).unwrap_or("");
    let hook_type = snapshot.get("type").and_then(|v| v.as_str()).unwrap_or("command");
    let matcher = snapshot.get("matcher").and_then(|v| v.as_str()).unwrap_or("");

    let mut count = 0;
    if let Some(hooks_table) = root.get("hooks").and_then(|v| v.as_table()) {
        if let Some(arr) = hooks_table.get(event_type).and_then(|v| v.as_array()) {
            for g in arr {
                let g_matcher = g.get("matcher").and_then(|v| v.as_str()).unwrap_or("");
                if g_matcher != matcher { continue; }
                if let Some(inner) = g.get("hooks").and_then(|v| v.as_array()) {
                    count += inner.iter().filter(|h| {
                        h.get("command").and_then(|v| v.as_str()).unwrap_or("") == cmd
                            && h.get("type").and_then(|v| v.as_str()).unwrap_or("command") == hook_type
                    }).count();
                }
            }
        }
    }
    count
}

/// Remove a JSON hook by exact (event_type, group_idx, hook_idx) — no content verification.
/// Only used when the caller has already verified the position is correct.
fn remove_json_hook_by_index(event_type: &str, group_idx: usize, hook_idx: usize, path: &std::path::PathBuf) -> Result<(), String> {
    let text = fs::read_to_string(path).map_err(|e| format!("读取失败: {}", e))?;
    let mut root: serde_json::Value = serde_json::from_str(&text).map_err(|e| format!("解析 JSON 失败: {}", e))?;
    if let Some(hooks) = root.get_mut("hooks").and_then(|v| v.as_object_mut()) {
        if let Some(arr) = hooks.get_mut(event_type).and_then(|v| v.as_array_mut()) {
            if let Some(g) = arr.get_mut(group_idx) {
                if let Some(inner) = g.get_mut("hooks").and_then(|v| v.as_array_mut()) {
                    if hook_idx < inner.len() { inner.remove(hook_idx); }
                }
                if g.get("hooks").and_then(|v| v.as_array()).map_or(true, |a| a.is_empty()) { arr.remove(group_idx); }
            }
        }
    }
    let new_text = serde_json::to_string_pretty(&root).map_err(|e| format!("序列化失败: {}", e))?;
    atomic_write(path, &new_text)?;
    Ok(())
}

// ── JSON hook helpers ───────────────────────────────────────

fn move_json_hook_entry(
    cli: &str,
    event_type: &str,
    group_idx: usize,
    hook_idx: usize,
    from: &std::path::PathBuf,
    to: &std::path::PathBuf,
    from_scope: &str,
    to_scope: &str,
) -> Result<HookMoveUndoInfo, String> {
    let text = fs::read_to_string(from).map_err(|e| format!("读取源文件失败: {}", e))?;
    let mut root: serde_json::Value = serde_json::from_str(&text).map_err(|e| format!("解析 JSON 失败: {}", e))?;
    let hooks = root.get_mut("hooks").and_then(|v| v.as_object_mut()).ok_or("源文件无 hooks")?;
    let event_arr = hooks.get_mut(event_type).and_then(|v| v.as_array_mut()).ok_or_else(|| format!("事件 '{}' 不存在", event_type))?;
    let group = event_arr.get_mut(group_idx).ok_or("group_idx 越界")?;
    // Read matcher before getting mutable reference to inner hooks (borrow checker)
    let group_matcher = group.get("matcher").and_then(|v| v.as_str()).map(|s| s.to_string()).filter(|s| !s.is_empty());
    let inner = group.get_mut("hooks").and_then(|v| v.as_array_mut()).ok_or("hooks 数组不存在")?;
    let mut entry = inner.get(hook_idx).ok_or("hook_idx 越界")?.clone();
    // Preserve the matcher from the source group
    if let Some(ref matcher) = group_matcher {
        if entry.get("matcher").is_none() {
            if let Some(obj) = entry.as_object_mut() {
                obj.insert("matcher".to_string(), serde_json::Value::String(matcher.clone()));
            }
        }
    }
    let snapshot = entry.clone();
    let name = entry.get("command").and_then(|v| v.as_str()).unwrap_or("unknown").to_string();

    // Backup both source and destination before any mutation
    let _ = fs::copy(from, from.with_extension("json.bak"));
    if to.exists() { let _ = fs::copy(to, to.with_extension("json.bak")); }

    // Write to dest FIRST — if this fails, source is untouched, no data loss
    let (target_group_idx, target_hook_idx) = write_json_hook_entry(&entry, event_type, to)?;

    // Only after target write succeeds, remove from source
    inner.remove(hook_idx);
    if inner.is_empty() { event_arr.remove(group_idx); }
    let new_src = serde_json::to_string_pretty(&root).map_err(|e| format!("序列化失败: {}", e))?;
    atomic_write(from, &new_src)?;

    Ok(HookMoveUndoInfo {
        resource_name: name, from_scope: from_scope.into(), to_scope: to_scope.into(),
        cli: cli.into(), event_type: event_type.into(), group_idx, hook_idx,
        hook_snapshot: snapshot,
        from_path: from.to_string_lossy().into(), to_path: to.to_string_lossy().into(),
        target_group_idx: Some(target_group_idx),
        target_hook_idx: Some(target_hook_idx),
    })
}

fn copy_json_hook_entry(event_type: &str, group_idx: usize, hook_idx: usize, from: &std::path::PathBuf, to: &std::path::PathBuf) -> Result<(), String> {
    let text = fs::read_to_string(from).map_err(|e| format!("读取源文件失败: {}", e))?;
    let root: serde_json::Value = serde_json::from_str(&text).map_err(|e| format!("解析 JSON 失败: {}", e))?;
    let hooks = root.get("hooks").and_then(|v| v.as_object()).ok_or("源文件无 hooks")?;
    let event_arr = hooks.get(event_type).and_then(|v| v.as_array()).ok_or_else(|| format!("事件 '{}' 不存在", event_type))?;
    let group = event_arr.get(group_idx).ok_or("group_idx 越界")?;
    let inner = group.get("hooks").and_then(|v| v.as_array()).ok_or("hooks 数组不存在")?;
    let mut entry = inner.get(hook_idx).ok_or("hook_idx 越界")?.clone();
    // Preserve the matcher from the source group so the destination keeps it
    if let Some(matcher) = group.get("matcher").and_then(|v| v.as_str()) {
        if !matcher.is_empty() && entry.get("matcher").is_none() {
            if let Some(obj) = entry.as_object_mut() {
                obj.insert("matcher".to_string(), serde_json::Value::String(matcher.to_string()));
            }
        }
    }
    let _ = write_json_hook_entry(&entry, event_type, to)?;
    Ok(())
}

/// Write a hook entry to a JSON config file. Returns the target (group_idx, hook_idx)
/// where the entry was inserted, for precise undo targeting.
fn write_json_hook_entry(entry: &serde_json::Value, event_type: &str, path: &std::path::PathBuf) -> Result<(usize, usize), String> {
    let text = if path.exists() { fs::read_to_string(path).unwrap_or_default() } else { "{}".into() };
    let mut root: serde_json::Value = serde_json::from_str(&text).unwrap_or(serde_json::json!({}));
    let hooks = root.as_object_mut().ok_or("根对象不是 JSON object")?;
    let hooks_val = hooks.entry("hooks".to_string()).or_insert_with(|| serde_json::json!({}));
    let hooks_obj = hooks_val.as_object_mut().ok_or("hooks 不是对象")?;
    let arr = if let Some(a) = hooks_obj.get_mut(event_type).and_then(|v| v.as_array_mut()) {
        a
    } else {
        // Use .insert() instead of [] indexing — serde_json::Map's IndexMut panics on missing keys
        hooks_obj.insert(event_type.to_string(), serde_json::json!([]));
        hooks_obj.get_mut(event_type).and_then(|v| v.as_array_mut()).ok_or("无法创建事件数组")?
    };
    let matcher = entry.get("matcher").and_then(|v| v.as_str()).unwrap_or("");

    let (target_group_idx, target_hook_idx): (usize, usize);

    // Find existing group with matching matcher (use position() to avoid borrow conflicts)
    let group_pos = arr.iter().position(|g| g.get("matcher").and_then(|v| v.as_str()).unwrap_or("") == matcher);
    if let Some(g_idx) = group_pos {
        target_group_idx = g_idx;
        let g = &mut arr[g_idx];
        if let Some(inner_arr) = g.get_mut("hooks").and_then(|v| v.as_array_mut()) {
            // Check for existing hook with same type+command — overwrite instead of append
            let cmd = entry.get("command").and_then(|v| v.as_str()).unwrap_or("");
            let ht = entry.get("type").and_then(|v| v.as_str()).unwrap_or("command");
            if let Some(existing_pos) = inner_arr.iter().position(|h| {
                h.get("command").and_then(|v| v.as_str()).unwrap_or("") == cmd
                    && h.get("type").and_then(|v| v.as_str()).unwrap_or("command") == ht
            }) {
                target_hook_idx = existing_pos;
                inner_arr[existing_pos] = entry.clone();
            } else {
                target_hook_idx = inner_arr.len();
                inner_arr.push(entry.clone());
            }
        } else {
            // The group exists but has no valid hooks array — create one
            target_hook_idx = 0;
            if let Some(obj) = g.as_object_mut() {
                obj.insert("hooks".to_string(), serde_json::json!([entry]));
            } else {
                // g is not an Object — can't add to it, replace with a new group
                arr[g_idx] = serde_json::json!({"matcher": matcher, "hooks": [entry]});
            }
        }
    } else {
        target_group_idx = arr.len();
        target_hook_idx = 0;
        arr.push(serde_json::json!({"matcher": matcher, "hooks": [entry]}));
    }

    if let Some(parent) = path.parent() { fs::create_dir_all(parent).map_err(|e| format!("创建目录失败: {}", e))?; }
    let new_text = serde_json::to_string_pretty(&root).map_err(|e| format!("序列化失败: {}", e))?;
    atomic_write(path, &new_text)?;
    Ok((target_group_idx, target_hook_idx))
}

#[allow(dead_code)]
fn remove_json_hook_entry(event_type: &str, group_idx: usize, hook_idx: usize, path: &std::path::PathBuf) -> Result<(), String> {
    if !path.exists() { return Ok(()); }
    let text = fs::read_to_string(path).map_err(|e| format!("读取失败: {}", e))?;
    let mut root: serde_json::Value = serde_json::from_str(&text).map_err(|e| format!("解析 JSON 失败: {}", e))?;
    if let Some(hooks) = root.get_mut("hooks").and_then(|v| v.as_object_mut()) {
        if let Some(arr) = hooks.get_mut(event_type).and_then(|v| v.as_array_mut()) {
            if let Some(g) = arr.get_mut(group_idx) {
                if let Some(inner) = g.get_mut("hooks").and_then(|v| v.as_array_mut()) {
                    if hook_idx < inner.len() { inner.remove(hook_idx); }
                }
                if g.get("hooks").and_then(|v| v.as_array()).map_or(true, |a| a.is_empty()) { arr.remove(group_idx); }
            }
        }
    }
    let new_text = serde_json::to_string_pretty(&root).map_err(|e| format!("序列化失败: {}", e))?;
    atomic_write(path, &new_text)?;
    Ok(())
}

/// Remove a JSON hook by matching snapshot content (command + type within matcher group).
/// Safer than index-based removal for undo operations where target indices may differ from source.
fn remove_json_hook_by_snapshot(snapshot: &serde_json::Value, event_type: &str, path: &std::path::PathBuf) -> Result<(), String> {
    if !path.exists() { return Ok(()); }
    let text = fs::read_to_string(path).map_err(|e| format!("读取失败: {}", e))?;
    let mut root: serde_json::Value = serde_json::from_str(&text).map_err(|e| format!("解析 JSON 失败: {}", e))?;

    let cmd = snapshot.get("command").and_then(|v| v.as_str()).unwrap_or("");
    let hook_type = snapshot.get("type").and_then(|v| v.as_str()).unwrap_or("command");
    let matcher = snapshot.get("matcher").and_then(|v| v.as_str()).unwrap_or("");

    if let Some(hooks) = root.get_mut("hooks").and_then(|v| v.as_object_mut()) {
        if let Some(arr) = hooks.get_mut(event_type).and_then(|v| v.as_array_mut()) {
            for (g_idx, g) in arr.iter_mut().enumerate() {
                let g_matcher = g.get("matcher").and_then(|v| v.as_str()).unwrap_or("");
                if g_matcher != matcher { continue; }
                if let Some(inner) = g.get_mut("hooks").and_then(|v| v.as_array_mut()) {
                    if let Some(pos) = inner.iter().position(|h| {
                        h.get("command").and_then(|v| v.as_str()).unwrap_or("") == cmd
                            && h.get("type").and_then(|v| v.as_str()).unwrap_or("command") == hook_type
                    }) {
                        inner.remove(pos);
                        if inner.is_empty() { arr.remove(g_idx); }
                        break;
                    }
                }
            }
        }
    }

    let new_text = serde_json::to_string_pretty(&root).map_err(|e| format!("序列化失败: {}", e))?;
    atomic_write(path, &new_text)?;
    Ok(())
}

fn restore_json_hook_entry(undo: &HookMoveUndoInfo, path: &std::path::PathBuf) -> Result<(), String> {
    let _ = write_json_hook_entry(&undo.hook_snapshot, &undo.event_type, path)?;
    Ok(())
}

// ── Codex TOML hook helpers ─────────────────────────────────

fn move_codex_hook_entry(
    cli: &str,
    event_type: &str,
    group_idx: usize,
    hook_idx: usize,
    from: &std::path::PathBuf,
    to: &std::path::PathBuf,
    from_scope: &str,
    to_scope: &str,
) -> Result<HookMoveUndoInfo, String> {
    let snapshot = extract_codex_hook_snapshot(event_type, group_idx, hook_idx, from)?;
    let name = snapshot.get("command").and_then(|v| v.as_str()).unwrap_or("unknown").to_string();

    // Backup both source and destination before any mutation
    let _ = fs::copy(from, from.with_extension("toml.bak"));
    if to.exists() { let _ = fs::copy(to, to.with_extension("toml.bak")); }

    // Write to dest FIRST — if this fails, source is untouched
    let (target_group_idx, target_hook_idx) = append_codex_hook_entry(&snapshot, event_type, to)?;

    // Only after target write succeeds, remove from source
    remove_codex_hook_entry_by_idx(event_type, group_idx, hook_idx, from)?;

    Ok(HookMoveUndoInfo {
        resource_name: name, from_scope: from_scope.into(), to_scope: to_scope.into(),
        cli: cli.into(), event_type: event_type.into(), group_idx, hook_idx,
        hook_snapshot: snapshot,
        from_path: from.to_string_lossy().into(), to_path: to.to_string_lossy().into(),
        target_group_idx: Some(target_group_idx),
        target_hook_idx: Some(target_hook_idx),
    })
}

fn copy_codex_hook_entry(event_type: &str, group_idx: usize, hook_idx: usize, from: &std::path::PathBuf, to: &std::path::PathBuf) -> Result<(), String> {
    let snapshot = extract_codex_hook_snapshot(event_type, group_idx, hook_idx, from)?;
    let _ = append_codex_hook_entry(&snapshot, event_type, to)?;
    Ok(())
}

fn extract_codex_hook_snapshot(event_type: &str, group_idx: usize, hook_idx: usize, path: &std::path::PathBuf) -> Result<serde_json::Value, String> {
    let text = fs::read_to_string(path).map_err(|e| format!("读取 Codex 配置失败: {}", e))?;
    let root: toml::Value = text.parse().map_err(|e| format!("解析 TOML 失败: {}", e))?;
    // Navigate hooks → event_type (toml::Value::get does NOT support dotted keys)
    let hooks_table = root.get("hooks").and_then(|v| v.as_table()).ok_or("hooks 不存在")?;
    let arr = hooks_table.get(event_type).and_then(|v| v.as_array()).ok_or_else(|| format!("hooks.{} 不存在", event_type))?;
    let group = arr.get(group_idx).ok_or("group_idx 越界")?;
    let inner = group.get("hooks").and_then(|v| v.as_array()).ok_or("hooks 数组不存在")?;
    let hook = inner.get(hook_idx).ok_or("hook_idx 越界")?;
    let mut m = serde_json::Map::new();
    if let Some(v) = hook.get("type").and_then(|v| v.as_str()) { m.insert("type".into(), v.into()); }
    if let Some(v) = hook.get("command").and_then(|v| v.as_str()) { m.insert("command".into(), v.into()); }
    if let Some(v) = group.get("matcher").and_then(|v| v.as_str()) { m.insert("matcher".into(), v.into()); }
    if let Some(v) = hook.get("timeout").and_then(|v| v.as_integer()) { m.insert("timeout".into(), serde_json::json!(v)); }
    if let Some(v) = hook.get("statusMessage").and_then(|v| v.as_str()) { m.insert("statusMessage".into(), v.into()); }
    Ok(serde_json::Value::Object(m))
}

#[allow(dead_code)]
fn remove_codex_hook_entry(event_type: &str, path: &std::path::PathBuf) -> Result<(), String> {
    if !path.exists() { return Ok(()); }
    let text = fs::read_to_string(path).map_err(|e| format!("读取失败: {}", e))?;
    let mut doc: toml::Table = text.parse().map_err(|e| format!("解析 TOML 失败: {}", e))?;
    // Navigate hooks → event_type (toml::Table::get_mut does NOT support dotted keys)
    if let Some(hooks_val) = doc.get_mut("hooks").and_then(|v| v.as_table_mut()).and_then(|h| h.get_mut(event_type)) {
        // For array-of-tables, we need to work with the inner tables
        if let Some(tables) = hooks_val.as_array_mut() {
            // Each element is a toml::Value::Table
            for g in tables.iter_mut().rev() {
                if let Some(t) = g.as_table_mut() {
                    if let Some(inner) = t.get_mut("hooks").and_then(|v| v.as_array_mut()) {
                        if !inner.is_empty() { inner.pop(); break; }
                    }
                }
            }
            tables.retain(|g| g.as_table().and_then(|t| t.get("hooks").and_then(|v| v.as_array()))
                .map_or(false, |a| !a.is_empty()));
        }
    }
    let new_text = toml::to_string(&doc).map_err(|e| format!("序列化 TOML 失败: {}", e))?;
    atomic_write(path, &new_text)?;
    Ok(())
}

fn remove_codex_hook_entry_by_idx(event_type: &str, group_idx: usize, hook_idx: usize, path: &std::path::PathBuf) -> Result<(), String> {
    let text = fs::read_to_string(path).map_err(|e| format!("读取失败: {}", e))?;
    let mut doc: toml::Table = text.parse().map_err(|e| format!("解析 TOML 失败: {}", e))?;
    // Navigate hooks → event_type (toml::Table::get_mut does NOT support dotted keys)
    if let Some(tables) = doc.get_mut("hooks").and_then(|v| v.as_table_mut()).and_then(|h| h.get_mut(event_type)).and_then(|v| v.as_array_mut()) {
        if let Some(g) = tables.get_mut(group_idx).and_then(|v| v.as_table_mut()) {
            if let Some(inner) = g.get_mut("hooks").and_then(|v| v.as_array_mut()) {
                if hook_idx < inner.len() { inner.remove(hook_idx); }
            }
            if g.get("hooks").and_then(|v| v.as_array()).map_or(true, |a| a.is_empty()) { tables.remove(group_idx); }
        }
        tables.retain(|g| g.as_table().and_then(|t| t.get("hooks").and_then(|v| v.as_array()))
            .map_or(false, |a| !a.is_empty()));
    }
    let new_text = toml::to_string(&doc).map_err(|e| format!("序列化 TOML 失败: {}", e))?;
    atomic_write(path, &new_text)?;
    Ok(())
}

/// Remove a Codex TOML hook by matching snapshot content (command + type within matcher group).
/// Safer than index-based removal for undo operations where target indices may differ from source.
fn remove_codex_hook_by_snapshot(snapshot: &serde_json::Value, event_type: &str, path: &std::path::PathBuf) -> Result<(), String> {
    if !path.exists() { return Ok(()); }
    let text = fs::read_to_string(path).map_err(|e| format!("读取失败: {}", e))?;
    let mut doc: toml::Table = text.parse().map_err(|e| format!("解析 TOML 失败: {}", e))?;

    let cmd = snapshot.get("command").and_then(|v| v.as_str()).unwrap_or("");
    let hook_type = snapshot.get("type").and_then(|v| v.as_str()).unwrap_or("command");
    let matcher = snapshot.get("matcher").and_then(|v| v.as_str()).unwrap_or("");

    // Navigate hooks → event_type
    if let Some(tables) = doc.get_mut("hooks").and_then(|v| v.as_table_mut()).and_then(|h| h.get_mut(event_type)).and_then(|v| v.as_array_mut()) {
        for (g_idx, g) in tables.iter_mut().enumerate() {
            let g_matcher = g.get("matcher").and_then(|v| v.as_str()).unwrap_or("");
            if g_matcher != matcher { continue; }
            if let Some(t) = g.as_table_mut() {
                if let Some(inner) = t.get_mut("hooks").and_then(|v| v.as_array_mut()) {
                    if let Some(pos) = inner.iter().position(|h| {
                        h.get("command").and_then(|v| v.as_str()).unwrap_or("") == cmd
                            && h.get("type").and_then(|v| v.as_str()).unwrap_or("command") == hook_type
                    }) {
                        inner.remove(pos);
                        if inner.is_empty() { tables.remove(g_idx); }
                        break;
                    }
                }
            }
        }
        // Clean up empty groups
        tables.retain(|g| g.as_table().and_then(|t| t.get("hooks").and_then(|v| v.as_array()))
            .map_or(false, |a| !a.is_empty()));
    }

    let new_text = toml::to_string(&doc).map_err(|e| format!("序列化 TOML 失败: {}", e))?;
    atomic_write(path, &new_text)?;
    Ok(())
}

/// Append a hook to a Codex TOML config file, preserving [[double-bracket]] format.
/// Returns the target (group_idx, hook_idx) where the entry was inserted.
/// Uses toml_edit (not toml) so the output keeps the same structure as create_codex_hook_new.
fn append_codex_hook_entry(snapshot: &serde_json::Value, event_type: &str, path: &std::path::PathBuf) -> Result<(usize, usize), String> {
    let matcher = snapshot.get("matcher").and_then(|v| v.as_str()).unwrap_or("");
    let cmd = snapshot.get("command").and_then(|v| v.as_str()).unwrap_or("");
    let ht = snapshot.get("type").and_then(|v| v.as_str()).unwrap_or("command");

    let mut doc: toml_edit::DocumentMut = if path.exists() {
        let text = fs::read_to_string(path).map_err(|e| format!("读取 Codex 配置失败: {}", e))?;
        if text.trim().is_empty() {
            toml_edit::DocumentMut::new()
        } else {
            text.parse().map_err(|e| format!("TOML 解析失败: {}", e))?
        }
    } else {
        toml_edit::DocumentMut::new()
    };

    // Ensure features.hooks = true (required for Codex hook scanning)
    {
        if !doc.contains_key("features") {
            doc.insert("features", toml_edit::table());
        }
        let feat_table = doc["features"].as_table_mut().ok_or("features 格式错误")?;
        feat_table.insert("hooks", toml_edit::value(true));
    }

    // Ensure hooks table exists
    if !doc.contains_key("hooks") {
        doc.insert("hooks", toml_edit::table());
    }
    let hooks = doc["hooks"].as_table_mut().ok_or("hooks 字段格式错误")?;

    // Ensure the event type array exists as ArrayOfTables
    if !hooks.contains_key(event_type) {
        hooks.insert(event_type, toml_edit::Item::ArrayOfTables(toml_edit::ArrayOfTables::new()));
    } else if hooks[event_type].as_array_of_tables().is_none() {
        // Existing entry is a plain array (e.g. PreToolUse = [] written by Codex CLI)
        hooks.insert(event_type, toml_edit::Item::ArrayOfTables(toml_edit::ArrayOfTables::new()));
    }

    let event_arr = hooks[event_type]
        .as_array_of_tables_mut()
        .ok_or("事件类型数组格式错误")?;

    // Find existing group with matching matcher
    let group_pos = event_arr.iter().position(|g| {
        g.get("matcher").and_then(|v| v.as_str()).unwrap_or("") == matcher
    });

    let target_group_idx: usize;
    let target_hook_idx: usize;

    if let Some(g_idx) = group_pos {
        target_group_idx = g_idx;
        // Group exists — check for existing inner hook to overwrite
        if let Some(inner_arr) = event_arr.get_mut(g_idx)
            .and_then(|g| g.get_mut("hooks"))
            .and_then(|v| v.as_array_of_tables_mut())
        {
            let hook_pos = inner_arr.iter().position(|h| {
                h.get("command").and_then(|v| v.as_str()).unwrap_or("") == cmd
                    && h.get("type").and_then(|v| v.as_str()).unwrap_or("command") == ht
            });
            if let Some(h_idx) = hook_pos {
                // Overwrite existing
                let mut new_inner = toml_edit::Table::new();
                new_inner.insert("type", toml_edit::value(ht));
                new_inner.insert("command", toml_edit::value(cmd));
                inner_arr.remove(h_idx);
                inner_arr.push(new_inner);
                target_hook_idx = inner_arr.len() - 1;
            } else {
                // Append to existing group
                target_hook_idx = inner_arr.len();
                let mut new_inner = toml_edit::Table::new();
                new_inner.insert("type", toml_edit::value(ht));
                new_inner.insert("command", toml_edit::value(cmd));
                inner_arr.push(new_inner);
            }
        } else {
            // Group exists but has no valid hooks array — create one
            target_hook_idx = 0;
            let mut inner_arr = toml_edit::ArrayOfTables::new();
            let mut new_inner = toml_edit::Table::new();
            new_inner.insert("type", toml_edit::value(ht));
            new_inner.insert("command", toml_edit::value(cmd));
            inner_arr.push(new_inner);
            if let Some(g) = event_arr.get_mut(g_idx) {
                g.insert("hooks", toml_edit::Item::ArrayOfTables(inner_arr));
            }
        }
    } else {
        // New group
        target_group_idx = event_arr.len();
        target_hook_idx = 0;
        let mut group = toml_edit::Table::new();
        if !matcher.is_empty() {
            group.insert("matcher", toml_edit::value(matcher));
        }
        let mut inner_arr = toml_edit::ArrayOfTables::new();
        let mut new_inner = toml_edit::Table::new();
        new_inner.insert("type", toml_edit::value(ht));
        new_inner.insert("command", toml_edit::value(cmd));
        inner_arr.push(new_inner);
        group.insert("hooks", toml_edit::Item::ArrayOfTables(inner_arr));
        event_arr.push(group);
    }

    if let Some(parent) = path.parent() { fs::create_dir_all(parent).map_err(|e| format!("创建目录失败: {}", e))?; }
    let new_text = doc.to_string();
    atomic_write(path, &new_text)?;
    Ok((target_group_idx, target_hook_idx))
}

fn restore_codex_hook_entry(undo: &HookMoveUndoInfo, path: &std::path::PathBuf) -> Result<(), String> {
    let _ = append_codex_hook_entry(&undo.hook_snapshot, &undo.event_type, path)?;
    Ok(())
}

/// Update a hook's command in-place (without delete + recreate).
/// For JSON (Claude/Qoder): directly edits the JSON value in the config file.
/// For TOML (Codex): uses snapshot → delete → create → rollback on failure.
#[tauri::command]
pub fn set_hook_command(
    cli: String,
    event_type: String,
    group_idx: usize,
    hook_idx: usize,
    path: String,
    command: String,
) -> Result<(), String> {
    let config_path = std::path::PathBuf::from(&path);
    match cli.as_str() {
        "claude" | "qoder" => {
            set_json_hook_command(&config_path, &event_type, group_idx, hook_idx, &command)
        }
        "codex" => {
            // TOML arrays cannot be edited in-place; use snapshot → delete → create
            let hook_snapshot =
                snapshot_codex_hook_at(&config_path, &event_type, group_idx, hook_idx)?;
            let matcher = hook_snapshot
                .get("matcher")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let hook_type = hook_snapshot
                .get("type")
                .and_then(|v| v.as_str())
                .unwrap_or("command")
                .to_string();
            delete_codex_hook_at(&config_path, &event_type, group_idx, hook_idx)?;
            let result =
                create_codex_hook_new(&event_type, &matcher, &command, &hook_type);
            if result.is_err() {
                // Rollback: restore the original hook
                let _ = append_codex_hook_entry(&hook_snapshot, &event_type, &config_path);
            }
            result
        }
        _ => Err(format!("不支持的 CLI: {}", cli)),
    }
}

/// Directly edit a JSON hook's command field (Claude/Qoder).
fn set_json_hook_command(
    path: &std::path::Path,
    event_type: &str,
    group_idx: usize,
    hook_idx: usize,
    command: &str,
) -> Result<(), String> {
    let text = fs::read_to_string(path).map_err(|e| format!("读取失败: {}", e))?;
    let mut root: serde_json::Value =
        serde_json::from_str(&text).map_err(|e| format!("JSON 解析失败: {}", e))?;

    let hook = root
        .pointer_mut(&format!("/hooks/{}/{}", event_type, group_idx))
        .and_then(|group| group.get_mut("hooks"))
        .and_then(|hooks| hooks.get_mut(hook_idx))
        .ok_or_else(|| "hook 不存在".to_string())?;

    // Rotating backup before modifying (3 generations, matches profile_cmd pattern)
    let bak3 = path.with_extension("json.bak.3");
    let bak2 = path.with_extension("json.bak.2");
    let bak1 = path.with_extension("json.bak.1");
    let bak  = path.with_extension("json.bak");
    let _ = fs::rename(&bak2, &bak3);
    let _ = fs::rename(&bak1, &bak2);
    let _ = fs::rename(&bak, &bak1);
    let _ = fs::copy(path, &bak);

    hook["command"] = serde_json::Value::String(command.to_string());

    let new_text =
        serde_json::to_string_pretty(&root).map_err(|e| format!("序列化失败: {}", e))?;
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, &new_text).map_err(|e| format!("写入失败: {}", e))?;
    #[cfg(unix)]
    {
        if let Ok(f) = fs::File::open(&tmp) {
            use std::os::unix::io::AsRawFd;
            unsafe { libc::fsync(f.as_raw_fd()); }
        }
    }
    crate::atomic_rename(&tmp, path)?;
    Ok(())
}

/// Snapshot a Codex TOML hook entry for rollback safety.
fn snapshot_codex_hook_at(
    path: &std::path::Path,
    event_type: &str,
    group_idx: usize,
    hook_idx: usize,
) -> Result<serde_json::Value, String> {
    let text = fs::read_to_string(path).map_err(|e| format!("读取失败: {}", e))?;
    let toml_val: toml::Value =
        toml::from_str(&text).map_err(|e| format!("TOML 解析失败: {}", e))?;
    let inner_arr = toml_val
        .get("hooks")
        .and_then(|h| h.get(event_type))
        .and_then(|arr| arr.as_array())
        .and_then(|a| a.get(group_idx))
        .and_then(|g| g.get("hooks"))
        .and_then(|h| h.as_array())
        .ok_or_else(|| "hooks 列表不存在".to_string())?;
    let hook = inner_arr.get(hook_idx).ok_or_else(|| "hook 不存在".to_string())?;
    let json_str =
        serde_json::to_string(&hook).map_err(|e| format!("序列化失败: {}", e))?;
    serde_json::from_str(&json_str).map_err(|e| format!("JSON 解析失败: {}", e))
}
