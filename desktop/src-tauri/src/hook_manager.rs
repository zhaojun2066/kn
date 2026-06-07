//! Hook Manager — scan hooks configured for Claude Code, Qoder, and Codex CLI.
//!
//! ## Hook formats
//!
//! - **Claude / Qoder**: `~/.claude/settings.json` (or `~/.qoder-cn/settings.json`) → `hooks` field (JSON)
//! - **Codex**: `~/.codex/config.toml` → `[[hooks.<EventType>]]` arrays (TOML)

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

/// Atomically write content to a file: create .bak backup, write to .tmp, fsync, rename.
/// This prevents data corruption from crashes or concurrent writes mid-write.
fn atomic_write(path: &std::path::Path, content: &str) -> Result<(), String> {
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

    // On Windows, std::fs::rename does not overwrite existing destinations.
    #[cfg(windows)]
    {
        if path.exists() {
            fs::remove_file(path).map_err(|e| format!("删除文件失败: {}", e))?;
        }
    }
    if let Err(e) = fs::rename(&tmp_path, path) {
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
fn scan_json_hooks(cli: &str, settings_path: &PathBuf) -> Vec<HookEntry> {
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
                    id: format!("{}:hook:{}:{}:{}", cli, event_type, group_idx, hook_idx),
                    cli: cli.to_string(),
                    event_type: event_type.clone(),
                    matcher: matcher.clone(),
                    command,
                    hook_type,
                    enabled: !disabled,
                    source: "user".to_string(),
                    path: path_str.clone(),
                    group_idx,
                    hook_idx,
                    timeout: None,
                    status_message: None,
                    name: None,
                    description: None,
                });
            }
        }
    }

    hooks
}

fn scan_claude_hooks() -> Vec<HookEntry> {
    let path = home_dir().join(".claude").join("settings.json");
    scan_json_hooks("claude", &path)
}

fn scan_qoder_hooks() -> Vec<HookEntry> {
    let path = home_dir().join(".qoder-cn").join("settings.json");
    scan_json_hooks("qoder", &path)
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
fn scan_codex_hooks() -> Vec<HookEntry> {
    let mut hooks = Vec::new();
    let config_path = home_dir().join(".codex").join("config.toml");
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
                        id: format!("codex:hook:{}:{}:{}", event_type, group_idx, hook_idx),
                        cli: "codex".to_string(),
                        event_type: event_type.clone(),
                        matcher: matcher.clone(),
                        command,
                        hook_type,
                        enabled,
                        source: "user".to_string(),
                        path: path_str.clone(),
                        group_idx,
                        hook_idx,
                        timeout,
                        status_message,
                        name: None,
                        description: None,
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
                    id: format!("codex:hook:{}:0:{}", event_type, idx),
                    cli: "codex".to_string(),
                    event_type: event_type.clone(),
                    matcher: None,
                    command,
                    hook_type,
                    enabled,
                    source: "user".to_string(),
                    path: path_str.clone(),
                    group_idx: 0,
                    hook_idx: idx,
                    timeout: None,
                    status_message: None,
                    name: None,
                    description: None,
                });
            }
        }
    }

    hooks
}

// ── Main scan command ─────────────────────────────────────────

#[tauri::command]
pub fn scan_hooks() -> HookManagerData {
    let mut hooks = Vec::new();
    hooks.extend(scan_claude_hooks());
    hooks.extend(scan_qoder_hooks());
    hooks.extend(scan_codex_hooks());

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
) -> Result<(), String> {
    let home = home_dir();
    match cli.as_str() {
        "claude" => toggle_json_hook(
            &home.join(".claude").join("settings.json"),
            &event_type,
            group_idx,
            hook_idx,
            enabled,
        ),
        "qoder" => toggle_json_hook(
            &home.join(".qoder-cn").join("settings.json"),
            &event_type,
            group_idx,
            hook_idx,
            enabled,
        ),
        "codex" => toggle_codex_hook(&event_type, group_idx, hook_idx, enabled),
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

fn toggle_codex_hook(
    event_type: &str,
    group_idx: usize,
    hook_idx: usize,
    enabled: bool,
) -> Result<(), String> {
    let config_path = home_dir().join(".codex").join("config.toml");
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
) -> Result<(), String> {
    let result = match cli.as_str() {
        "claude" => delete_json_hook(
            &home_dir().join(".claude").join("settings.json"),
            &event_type,
            group_idx,
            hook_idx,
        ),
        "qoder" => delete_json_hook(
            &home_dir().join(".qoder-cn").join("settings.json"),
            &event_type,
            group_idx,
            hook_idx,
        ),
        "codex" => delete_codex_hook(&event_type, group_idx, hook_idx),
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

fn delete_codex_hook(event_type: &str, group_idx: usize, hook_idx: usize) -> Result<(), String> {
    let config_path = home_dir().join(".codex").join("config.toml");
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
        serde_json::from_str(&text).unwrap_or(serde_json::Value::Object(serde_json::Map::new()))
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
    }

    let event_arr = hooks[event_type]
        .as_array_of_tables_mut()
        .ok_or("事件类型数组格式错误")?;
    event_arr.push(group_table);

    // Create parent directory if needed
    if let Some(parent) = config_path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("创建目录失败: {}", e))?;
    }

    let new_text = doc.to_string();
    atomic_write(&config_path, &new_text)?;
    Ok(())
}
