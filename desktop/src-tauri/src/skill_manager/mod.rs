//! Skill & Plugin Manager — scan and toggle skills/plugins across Claude Code & Codex.
//!
//! # Data Model
//!
//! - **Plugin**: enable/disable unit. Contains zero or more skills (read-only children).
//! - **Standalone Skill**: individually toggleable skill not owned by any plugin.
//! - **System Skill**: built-in, read-only (Codex `.system/` directory).

use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tauri::Emitter;

mod content;
mod transfer;
mod types;

pub use types::*;

mod file_utils;
mod paths;

use file_utils::*;
use paths::*;

// ═══════════════════════════════════════════════════════════════
//  CLAUDE SCAN
// ═══════════════════════════════════════════════════════════════

fn scan_claude_plugins() -> Vec<PluginEntry> {
    let mut plugins = Vec::new();

    // Read installed_plugins.json
    let json_path = match claude_plugins_json() {
        Some(p) => p,
        None => return plugins,
    };
    let text = match fs::read_to_string(&json_path) {
        Ok(t) => t,
        Err(_) => return plugins,
    };
    let root: serde_json::Value = match serde_json::from_str(&text) {
        Ok(v) => v,
        Err(_) => return plugins,
    };

    // Read enabledPlugins from settings.json
    let enabled_plugins = read_claude_enabled_plugins();

    if let Some(plugins_obj) = root.get("plugins").and_then(|v| v.as_object()) {
        for (full_name, installs) in plugins_obj {
            // full_name format: "name@marketplace"
            let (name, marketplace) = parse_plugin_id(full_name);

            if let Some(arr) = installs.as_array() {
                for inst in arr {
                    let install_path = inst
                        .get("installPath")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    let version = inst
                        .get("version")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string());
                    let scope = inst.get("scope").and_then(|v| v.as_str()).unwrap_or("user");
                    let source = if scope == "user" {
                        "marketplace"
                    } else {
                        scope
                    };

                    let enabled = enabled_plugins
                        .get(full_name)
                        .or_else(|| {
                            // settings.json may store keys with "claude:plugin:" prefix
                            enabled_plugins.get(&format!("claude:plugin:{}", full_name))
                        })
                        .copied()
                        .unwrap_or(false);

                    // Enumerate skills in installPath/skills/
                    let skills_dir = PathBuf::from(install_path).join("skills");
                    let mut skills = enumerate_claude_skills(&skills_dir);

                    // Fallback: single-skill plugins (e.g., pm-skills) have
                    // SKILL.md at the install root instead of a skills/ subdirectory.
                    if skills.is_empty() {
                        let root_md = PathBuf::from(install_path).join("SKILL.md");
                        if root_md.exists() {
                            let desc = extract_description(&root_md);
                            skills.push(SkillEntry {
                                name: name.to_string(),
                                path: root_md.to_string_lossy().to_string(),
                                description: desc,
                            });
                        }
                    }

                    // Enumerate agents in installPath/agents/
                    let agents_dir = PathBuf::from(install_path).join("agents");
                    let agents = crate::agent_manager::scan_md_agents_in_dir(
                        "claude",
                        &agents_dir,
                        "plugin",
                        None,
                        None,
                    );

                    // Enumerate commands in installPath/commands/
                    let commands_dir = PathBuf::from(install_path).join("commands");
                    let commands = enumerate_commands("claude", &commands_dir);

                    plugins.push(PluginEntry {
                        id: format!("claude:plugin:{}", full_name),
                        cli: "claude".into(),
                        name: name.to_string(),
                        marketplace: marketplace.to_string(),
                        enabled,
                        version,
                        source: source.to_string(),
                        skills,
                        agents,
                        commands,
                    });
                }
            }
        }
    }

    plugins
}

fn parse_plugin_id(full: &str) -> (&str, &str) {
    match full.rsplit_once('@') {
        Some((name, mkt)) => (name, mkt),
        None => (full, ""),
    }
}

fn read_claude_enabled_plugins() -> HashMap<String, bool> {
    let mut map = HashMap::new();
    let path = match claude_settings_json() {
        Some(p) => p,
        None => return map,
    };
    let text = match fs::read_to_string(&path) {
        Ok(t) => t,
        Err(_) => return map,
    };
    let root: serde_json::Value = match serde_json::from_str(&text) {
        Ok(v) => v,
        Err(_) => return map,
    };
    if let Some(obj) = root.get("enabledPlugins").and_then(|v| v.as_object()) {
        for (k, v) in obj {
            map.insert(k.clone(), v.as_bool().unwrap_or(false));
        }
    }
    map
}

fn enumerate_claude_skills(skills_dir: &Path) -> Vec<SkillEntry> {
    let mut skills = Vec::new();
    let dir = match fs::read_dir(skills_dir) {
        Ok(d) => d,
        Err(_) => return skills,
    };
    for entry in dir.flatten() {
        let path = entry.path();
        let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        // Skip hidden files, non-.md files, disabled files
        if file_name.starts_with('.') {
            continue;
        }
        if file_name.ends_with(".disabled") {
            continue;
        }
        if path.is_dir() {
            // Claude skills can also be directories with an SKILL.md inside
            let skill_md = path.join("SKILL.md");
            if skill_md.exists() {
                let desc = extract_description(&skill_md);
                skills.push(SkillEntry {
                    name: file_name.to_string(),
                    path: path.to_string_lossy().to_string(),
                    description: desc,
                });
            }
            continue;
        }
        if !file_name.ends_with(".md") {
            continue;
        }
        let name = file_name.strip_suffix(".md").unwrap_or(file_name);
        let desc = extract_description(&path);
        skills.push(SkillEntry {
            name: name.to_string(),
            path: path.to_string_lossy().to_string(),
            description: desc,
        });
    }
    skills
}

/// Scan a commands directory for .md files and return CommandEntry list.
/// Handles .md (enabled) and .md.disabled (disabled) files.
fn enumerate_commands(cli: &str, commands_dir: &Path) -> Vec<CommandEntry> {
    let mut commands = Vec::new();
    let dir = match fs::read_dir(commands_dir) {
        Ok(d) => d,
        Err(_) => return commands,
    };
    for entry in dir.flatten() {
        let path = entry.path();
        let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if file_name.starts_with('.') {
            continue;
        }
        let (name, enabled) = if let Some(n) = file_name.strip_suffix(".md.disabled") {
            (n.to_string(), false)
        } else if let Some(n) = file_name.strip_suffix(".md") {
            (n.to_string(), true)
        } else {
            continue;
        };
        let desc = extract_description(&path);
        commands.push(CommandEntry {
            id: format!("{}:command:{}", cli, name),
            cli: cli.to_string(),
            name,
            path: path.to_string_lossy().to_string(),
            description: desc.unwrap_or_default(),
            enabled,
            project_name: None,
        });
    }
    commands
}

fn scan_claude_standalone_skills(
    plugin_skill_names: &std::collections::HashSet<String>,
) -> Vec<StandaloneSkill> {
    let mut skills = Vec::new();
    let skills_dir = match claude_skills_dir() {
        Some(d) => d,
        None => return skills,
    };
    let dir = match fs::read_dir(&skills_dir) {
        Ok(d) => d,
        Err(_) => return skills,
    };

    for entry in dir.flatten() {
        let path = entry.path();
        let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");

        // Skip hidden files/dirs
        if file_name.starts_with('.') {
            continue;
        }

        // Only accept: .md files, .md.disabled files, or directories with SKILL.md / SKILL.md.disabled
        let is_md = file_name.ends_with(".md") || file_name.ends_with(".md.disabled");
        let is_skill_dir = path.is_dir() && {
            let skill_md = path.join("SKILL.md");
            let skill_md_disabled = path.join("SKILL.md.disabled");
            skill_md.exists() || skill_md_disabled.exists()
        };
        if !is_md && !is_skill_dir {
            continue;
        }

        // Extract skill name (strip .md / .md.disabled suffix)
        let name = if file_name.ends_with(".md.disabled") {
            file_name.strip_suffix(".md.disabled").unwrap_or(file_name)
        } else if file_name.ends_with(".md") {
            file_name.strip_suffix(".md").unwrap_or(file_name)
        } else {
            file_name
        };

        // Check if this skill belongs to a known plugin (name-based, handles broken symlinks)
        if plugin_skill_names.contains(name) {
            continue;
        }

        let link_type = classify_entry(&path);
        // For directory skills, check SKILL.md (not .disabled) — the directory
        // name itself doesn't change when toggled, unlike flat .md files.
        let enabled = if is_skill_dir {
            path.join("SKILL.md").exists()
        } else {
            !file_name.ends_with(".disabled")
        };

        skills.push(StandaloneSkill {
            id: format!("claude:skill:{}", name),
            cli: "claude".into(),
            name: name.to_string(),
            enabled,
            link_type: link_type.to_string(),
            path: path.to_string_lossy().to_string(),
            project_name: None,
        });
    }

    skills
}

// ═══════════════════════════════════════════════════════════════
//  CODEX SCAN
// ═══════════════════════════════════════════════════════════════

#[derive(Debug, Deserialize, Default)]
struct CodexConfig {
    plugins: Option<HashMap<String, CodexPluginConfig>>,
    marketplaces: Option<HashMap<String, CodexMarketplaceConfig>>,
}

#[derive(Debug, Deserialize, Default)]
struct CodexPluginConfig {
    enabled: Option<bool>,
}

#[derive(Debug, Deserialize, Default)]
#[allow(dead_code)]
struct CodexMarketplaceConfig {
    #[serde(default)]
    source: Option<String>,
}

/// Parse Codex config.toml to get plugin enabled states, including
/// marketplaces registered via `codex plugin marketplace add`.
fn read_codex_plugin_states() -> HashMap<String, bool> {
    let mut states = HashMap::new();
    let path = match codex_config_toml() {
        Some(p) => p,
        None => return states,
    };
    let text = match fs::read_to_string(&path) {
        Ok(t) => t,
        Err(_) => return states,
    };

    // Parse with toml crate
    let config: CodexConfig = match toml::from_str(&text) {
        Ok(c) => c,
        Err(_) => return states,
    };

    if let Some(plugins) = config.plugins {
        for (full_name, cfg) in plugins {
            states.insert(full_name, cfg.enabled.unwrap_or(true));
        }
    }
    // Also read [marketplaces] section — user-added marketplaces are registered here
    // and the marketplace name serves as the plugin id (installed = true).
    if let Some(marketplaces) = config.marketplaces {
        for (mkt_name, _cfg) in marketplaces {
            states.insert(format!("{}@{}", mkt_name, mkt_name), true);
        }
    }
    states
}

/// Write a plugin entry into a project's `.codex/config.toml` `[plugins]` section.
/// Creates the directory and file if they don't exist.
fn write_codex_project_plugin_enabled(
    project_path: &str,
    plugin_name: &str,
    marketplace: &str,
    enabled: bool,
) -> Result<(), String> {
    let project_dir = validate_project_dir(project_path)?;
    let config_path = project_dir.join(".codex").join("config.toml");

    // Ensure .codex/ directory exists
    if let Some(parent) = config_path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("创建项目 .codex 目录失败: {}", e))?;
    }

    // Read existing config or start fresh
    let mut config: toml::Value = if config_path.exists() {
        let text = fs::read_to_string(&config_path)
            .map_err(|e| format!("读取项目 config.toml 失败: {}", e))?;
        toml::from_str(&text).unwrap_or(toml::Value::Table(toml::map::Map::new()))
    } else {
        toml::Value::Table(toml::map::Map::new())
    };

    // Set [plugins."name@marketplace"].enabled
    let key = format!("{}@{}", plugin_name, marketplace);
    let root = config
        .as_table_mut()
        .ok_or("项目 config.toml 根节点必须是 table".to_string())?;
    let plugins = root
        .entry("plugins".to_string())
        .or_insert_with(|| toml::Value::Table(toml::map::Map::new()))
        .as_table_mut()
        .ok_or("项目 config.toml 的 [plugins] 必须是 table".to_string())?;
    let plugin_cfg = plugins
        .entry(key)
        .or_insert_with(|| toml::Value::Table(toml::map::Map::new()))
        .as_table_mut()
        .ok_or("项目 plugin 配置必须是 table".to_string())?;
    plugin_cfg.insert("enabled".to_string(), toml::Value::Boolean(enabled));

    let content = toml::to_string_pretty(&config)
        .map_err(|e| format!("序列化项目 config.toml 失败: {}", e))?;
    fs::write(&config_path, content).map_err(|e| format!("写入项目 config.toml 失败: {}", e))?;
    Ok(())
}

/// Remove a plugin entry from a project's `.codex/config.toml` `[plugins]` section.
fn remove_codex_project_plugin(project_path: &str, plugin_full_id: &str) -> Result<(), String> {
    let project_dir = validate_project_dir(project_path)?;
    let config_path = project_dir.join(".codex").join("config.toml");
    if !config_path.exists() {
        return Ok(()); // Nothing to remove
    }
    let text = fs::read_to_string(&config_path)
        .map_err(|e| format!("读取项目 config.toml 失败: {}", e))?;
    let mut config: toml::Value =
        toml::from_str(&text).map_err(|_| "解析项目 config.toml 失败".to_string())?;

    if let Some(plugins) = config.get_mut("plugins") {
        if let Some(table) = plugins.as_table_mut() {
            table.remove(plugin_full_id);
            // Write back
            let content = toml::to_string_pretty(&config)
                .map_err(|e| format!("序列化项目 config.toml 失败: {}", e))?;
            fs::write(&config_path, content)
                .map_err(|e| format!("写入项目 config.toml 失败: {}", e))?;
        }
    }
    Ok(())
}

fn validate_project_dir(project_path: &str) -> Result<PathBuf, String> {
    let project_dir = PathBuf::from(project_path);
    if !project_dir.exists() {
        return Err(format!("项目目录不存在: {}", project_path));
    }
    if !project_dir.is_dir() {
        return Err(format!("项目路径不是目录: {}", project_path));
    }
    Ok(project_dir)
}

/// Remove a plugin entry from the user-level `~/.codex/config.toml` `[plugins]` section.
/// Used after `codex plugin add` when installing at project scope — we want the plugin
/// files in cache but NOT registered at user level.
fn remove_codex_user_plugin_config(plugin_name: &str, marketplace: &str) -> Result<(), String> {
    let config_path = match codex_config_toml() {
        Some(p) => p,
        None => return Ok(()), // No config — nothing to remove
    };
    if !config_path.exists() {
        return Ok(());
    }
    let text = fs::read_to_string(&config_path)
        .map_err(|e| format!("读取 Codex config.toml 失败: {}", e))?;
    let mut config: toml::Value =
        toml::from_str(&text).map_err(|_| "解析 Codex config.toml 失败".to_string())?;

    let key = format!("{}@{}", plugin_name, marketplace);
    if let Some(plugins) = config.get_mut("plugins") {
        if let Some(table) = plugins.as_table_mut() {
            table.remove(&key);
            let content = toml::to_string_pretty(&config)
                .map_err(|e| format!("序列化 Codex config.toml 失败: {}", e))?;
            fs::write(&config_path, content)
                .map_err(|e| format!("写入 Codex config.toml 失败: {}", e))?;
        }
    }
    Ok(())
}

fn scan_codex_plugins() -> Vec<PluginEntry> {
    let mut plugins = Vec::new();
    let plugin_states = read_codex_plugin_states();

    // Source 1: User-installed plugins in ~/.codex/plugins/
    if let Some(user_dir) = codex_plugins_dir() {
        if let Ok(entries) = fs::read_dir(&user_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if !path.is_dir() {
                    continue;
                }
                if let Some(plugin) = read_codex_plugin_manifest(&path, &plugin_states, "user") {
                    plugins.push(plugin);
                }
            }
        }

        // Source 1b: Cached marketplace plugins in ~/.codex/plugins/cache/
        // Structure: cache/<marketplace>/<plugin>/<version>/.codex-plugin/plugin.json
        let cache_dir = user_dir.join("cache");
        scan_codex_cache_dir(&cache_dir, &plugin_states, &mut plugins);
    }

    // Source 2: Marketplace plugins (bundled + runtime + user-added)
    if let Some(home) = home_dir() {
        let tmp = home.join(".codex").join(".tmp");

        // Bundled marketplace
        let bundled = tmp.join("bundled-marketplaces");
        scan_codex_marketplace_dir(&bundled, &plugin_states, "bundled", &mut plugins);

        // User-added marketplaces (codex plugin marketplace add ...)
        let user_markets = tmp.join("marketplaces");
        scan_codex_marketplace_dir(&user_markets, &plugin_states, "user", &mut plugins);

        // Runtime marketplace
        let runtime = home
            .join(".cache")
            .join("codex-runtimes")
            .join("codex-primary-runtime")
            .join("plugins");
        scan_codex_marketplace_dir(&runtime, &plugin_states, "bundled", &mut plugins);
    }

    // Deduplicate by plugin name: same plugin from different marketplaces
    // should appear only once in the UI. Keep the entry from the highest-
    // priority source (user > bundled > cache), breaking ties by version.
    deduplicate_plugins_by_name(&mut plugins);

    plugins
}

/// Scan the cache directory where Codex stores marketplace plugins locally.
/// Structure: cache/<marketplace>/<plugin>/<version>/.codex-plugin/plugin.json
fn scan_codex_cache_dir(
    root: &Path,
    states: &HashMap<String, bool>,
    plugins: &mut Vec<PluginEntry>,
) {
    if !root.exists() {
        return;
    }
    let dir = match fs::read_dir(root) {
        Ok(d) => d,
        Err(_) => return,
    };
    for mkt_entry in dir.flatten() {
        let mkt_path = mkt_entry.path();
        if !mkt_path.is_dir() {
            continue;
        }
        let mkt_name = mkt_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown");
        if mkt_name.starts_with('.') {
            continue;
        }
        // Iterate plugin dirs inside this marketplace
        let plugins_dir = match fs::read_dir(&mkt_path) {
            Ok(d) => d,
            Err(_) => continue,
        };
        for plugin_entry in plugins_dir.flatten() {
            let plugin_top = plugin_entry.path();
            if !plugin_top.is_dir() {
                continue;
            }
            // Inside each plugin dir, find the version subdirectory
            let version_dir = match fs::read_dir(&plugin_top) {
                Ok(d) => d,
                Err(_) => continue,
            };
            for version_entry in version_dir.flatten() {
                let version_path = version_entry.path();
                if !version_path.is_dir() {
                    continue;
                }
                let source = format!("cache@{}", mkt_name);
                if let Some(plugin) =
                    read_codex_plugin_manifest(&version_path, states, &source)
                {
                    if !plugins.iter().any(|existing| existing.id == plugin.id) {
                        plugins.push(plugin);
                    }
                }
            }
        }
    }
}

/// Deduplicate plugins that share the same name (from different marketplaces).
/// Priority: user > bundled > cache > flat.  Within the same priority tier,
/// keep the highest version.
fn deduplicate_plugins_by_name(plugins: &mut Vec<PluginEntry>) {
    let source_rank = |source: &str| -> u8 {
        if source.starts_with("user") {
            0
        } else if source.starts_with("bundled") {
            1
        } else if source.starts_with("cache") {
            2
        } else {
            3 // flat, unknown
        }
    };

    let mut seen: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    let mut drop_indices: Vec<usize> = Vec::new();

    for (i, plugin) in plugins.iter().enumerate() {
        if let Some(&existing_idx) = seen.get(&plugin.name) {
            let existing = &plugins[existing_idx];
            let existing_rank = source_rank(&existing.source);
            let this_rank = source_rank(&plugin.source);

            let keep_this = if this_rank < existing_rank {
                true
            } else if this_rank == existing_rank {
                // Same priority — keep higher version
                let this_ver = parse_semver(&plugin.version);
                let existing_ver = parse_semver(&existing.version);
                this_ver > existing_ver
            } else {
                false
            };

            if keep_this {
                // Drop the old, keep this one
                drop_indices.push(existing_idx);
                seen.insert(plugin.name.clone(), i);
            } else {
                drop_indices.push(i);
            }
        } else {
            seen.insert(plugin.name.clone(), i);
        }
    }

    // Collect kept items, merging enabled state from all same-name entries
    let name_to_enabled: std::collections::HashMap<String, bool> = plugins
        .iter()
        .fold(std::collections::HashMap::new(), |mut acc, p| {
            let entry = acc.entry(p.name.clone()).or_insert(false);
            *entry = *entry || p.enabled;
            acc
        });

    // Build the deduplicated list
    let all_indices: std::collections::HashSet<usize> = drop_indices.into_iter().collect();
    let mut result: Vec<PluginEntry> = Vec::new();
    for (i, mut plugin) in plugins.drain(..).enumerate() {
        if all_indices.contains(&i) {
            continue;
        }
        // Merge enabled state: enabled if ANY same-name instance was enabled
        if let Some(&merged) = name_to_enabled.get(&plugin.name) {
            plugin.enabled = merged;
        }
        result.push(plugin);
    }
    *plugins = result;
}

/// Parse a semver-like string into (major, minor, patch) for comparison.
fn parse_semver(version: &Option<String>) -> (u32, u32, u32) {
    let v = match version {
        Some(v) => v,
        None => return (0, 0, 0),
    };
    let parts: Vec<u32> = v
        .split(|c: char| !c.is_ascii_digit())
        .filter(|s| !s.is_empty())
        .take(3)
        .map(|s| s.parse::<u32>().unwrap_or(0))
        .collect();
    (
        parts.first().copied().unwrap_or(0),
        parts.get(1).copied().unwrap_or(0),
        parts.get(2).copied().unwrap_or(0),
    )
}

fn scan_codex_marketplace_dir(
    root: &Path,
    states: &HashMap<String, bool>,
    default_source: &str,
    plugins: &mut Vec<PluginEntry>,
) {
    if !root.exists() {
        return;
    }
    // Walk marketplace dirs: root/marketplace-name/plugins/plugin-name/  (nested)
    // Fallback: root/marketplace-name/                                 (flat — marketplace is the plugin)
    let dir = match fs::read_dir(root) {
        Ok(d) => d,
        Err(_) => return,
    };
    for marketplace_entry in dir.flatten() {
        let mkt_path = marketplace_entry.path();
        if !mkt_path.is_dir() {
            continue;
        }
        let mkt_name = mkt_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown");
        if mkt_name.starts_with('.') {
            continue;
        }

        let source = format!("{}@{}", default_source, mkt_name);

        // Try nested structure first: mkt-path/plugins/plugin-name/
        let plugins_dir = mkt_path.join("plugins");
        if plugins_dir.exists() {
            if let Ok(plugin_entries) = fs::read_dir(&plugins_dir) {
                for p in plugin_entries.flatten() {
                    let plugin_path = p.path();
                    if !plugin_path.is_dir() {
                        continue;
                    }
                    if let Some(plugin) = read_codex_plugin_manifest(&plugin_path, states, &source)
                    {
                        if !plugins.iter().any(|existing| existing.id == plugin.id) {
                            plugins.push(plugin);
                        }
                    }
                }
            }
        } else {
            // Flat fallback: mkt-path itself is the plugin directory
            if let Some(plugin) = read_codex_plugin_manifest(&mkt_path, states, &source) {
                if !plugins.iter().any(|existing| existing.id == plugin.id) {
                    plugins.push(plugin);
                }
            }
        }
    }
}

/// Try to read a plugin manifest from the given directory.
/// Checks `.codex-plugin/plugin.json` first, then falls back to `.claude-plugin/plugin.json`.
fn read_plugin_manifest_json(path: &Path) -> Option<(serde_json::Value, String)> {
    for dir_name in &[".codex-plugin", ".claude-plugin"] {
        let manifest_path = path.join(dir_name).join("plugin.json");
        if let Ok(text) = fs::read_to_string(&manifest_path) {
            if let Ok(manifest) = serde_json::from_str::<serde_json::Value>(&text) {
                return Some((manifest, text));
            }
        }
    }
    None
}

fn read_codex_plugin_manifest(
    path: &Path,
    states: &HashMap<String, bool>,
    source: &str,
) -> Option<PluginEntry> {
    let (manifest, _) = read_plugin_manifest_json(path)?;

    let name = manifest
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");
    let version = manifest
        .get("version")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let marketplace = source.split('@').next_back().unwrap_or(source);

    let full_id = format!("{}@{}", name, marketplace);
    let enabled = states.get(&full_id).copied().unwrap_or(true);

    // Look for skills subdirectory
    let skills_dir = manifest
        .get("skills")
        .and_then(|v| v.as_str())
        .map(|rel| path.join(rel))
        .unwrap_or_else(|| path.join("skills"));
    let skills = enumerate_codex_skills(&skills_dir);

    Some(PluginEntry {
        id: format!("codex:plugin:{}", full_id),
        cli: "codex".into(),
        name: name.to_string(),
        marketplace: marketplace.to_string(),
        enabled,
        version,
        source: source.to_string(),
        skills,
        agents: vec![],   // Codex .toml agent scanning deferred
        commands: vec![], // Codex has no commands concept
    })
}

fn enumerate_codex_skills(skills_dir: &Path) -> Vec<SkillEntry> {
    let mut skills = Vec::new();
    let dir = match fs::read_dir(skills_dir) {
        Ok(d) => d,
        Err(_) => return skills,
    };
    for entry in dir.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown");
        // A Codex skill is valid if it has SKILL.md (not disabled)
        let skill_md = path.join("SKILL.md");
        if skill_md.exists() {
            let desc = extract_description(&skill_md);
            skills.push(SkillEntry {
                name: name.to_string(),
                path: path.to_string_lossy().to_string(),
                description: desc,
            });
        }
    }
    skills
}

fn scan_codex_standalone_skills() -> Vec<StandaloneSkill> {
    let mut skills = Vec::new();
    let skills_dir = match codex_skills_dir() {
        Some(d) => d,
        None => return skills,
    };
    let dir = match fs::read_dir(&skills_dir) {
        Ok(d) => d,
        Err(_) => return skills,
    };

    for entry in dir.flatten() {
        let path = entry.path();
        let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");

        // Skip hidden (including .system/)
        if file_name.starts_with('.') {
            continue;
        }

        if path.is_symlink() || path.is_dir() {
            let has_skill_md = path.join("SKILL.md").exists();
            let has_skill_md_disabled = path.join("SKILL.md.disabled").exists();

            if !has_skill_md && !has_skill_md_disabled {
                // It might be a symlink to a single .md file (Claude-style skill used with Codex)
                let resolved = resolve_symlink(&path);
                if resolved.is_file() && resolved.extension().is_some_and(|e| e == "md") {
                    skills.push(StandaloneSkill {
                        id: format!("codex:skill:{}", file_name),
                        cli: "codex".into(),
                        name: file_name.to_string(),
                        enabled: true,
                        link_type: "symlink".into(),
                        path: path.to_string_lossy().to_string(),
                        project_name: None,
                    });
                }
                continue;
            }

            let enabled = has_skill_md;
            skills.push(StandaloneSkill {
                id: format!("codex:skill:{}", file_name),
                cli: "codex".into(),
                name: file_name.to_string(),
                enabled,
                link_type: classify_entry(&path).to_string(),
                path: path.to_string_lossy().to_string(),
                project_name: None,
            });
        }
    }

    skills
}

fn scan_codex_system_skills() -> Vec<StandaloneSkill> {
    let mut skills = Vec::new();
    let system_dir = match codex_skills_dir() {
        Some(d) => d.join(".system"),
        None => return skills,
    };
    let dir = match fs::read_dir(&system_dir) {
        Ok(d) => d,
        Err(_) => return skills,
    };

    for entry in dir.flatten() {
        let path = entry.path();
        let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");

        if file_name.starts_with('.') {
            continue;
        }
        if !path.is_dir() {
            continue;
        }
        if !path.join("SKILL.md").exists() {
            continue;
        }

        skills.push(StandaloneSkill {
            id: format!("codex:system:{}", file_name),
            cli: "codex".into(),
            name: file_name.to_string(),
            enabled: true, // always enabled, read-only
            link_type: "directory".into(),
            path: path.to_string_lossy().to_string(),
            project_name: None,
        });
    }

    skills
}

// ═══════════════════════════════════════════════════════════════
//  QODER SCAN
// ═══════════════════════════════════════════════════════════════

/// Scan Qoder standalone skills from ~/.qoder-cn/skills/ (domestic version).
///
/// Qoder skill format is identical to Codex: a directory containing SKILL.md.
/// There is no plugin concept — all skills are standalone.
/// Enable/disable: SKILL.md ↔ SKILL.md.disabled (same as Codex).
fn scan_qoder_standalone_skills() -> Vec<StandaloneSkill> {
    let mut skills = Vec::new();
    let skills_dir = match qoder_skills_dir() {
        Some(d) => d,
        None => return skills,
    };
    let dir = match fs::read_dir(&skills_dir) {
        Ok(d) => d,
        Err(_) => return skills,
    };

    for entry in dir.flatten() {
        let path = entry.path();
        let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");

        // Skip hidden files/dirs
        if file_name.starts_with('.') {
            continue;
        }

        if !path.is_dir() {
            // Qoder skills must be directories
            continue;
        }

        let has_skill_md = path.join("SKILL.md").exists();
        let has_skill_md_disabled = path.join("SKILL.md.disabled").exists();

        if !has_skill_md && !has_skill_md_disabled {
            continue;
        }

        let enabled = has_skill_md;
        skills.push(StandaloneSkill {
            id: format!("qoder:skill:{}", file_name),
            cli: "qoder".into(),
            name: file_name.to_string(),
            enabled,
            link_type: classify_entry(&path).to_string(),
            path: path.to_string_lossy().to_string(),
            project_name: None,
        });
    }

    skills
}

/// Toggle a Qoder standalone skill: rename `SKILL.md` ↔ `SKILL.md.disabled`.
/// Identical logic to Codex standalone skill toggle.
fn toggle_qoder_standalone_skill(skill_name: &str, enabled: bool) -> Result<(), String> {
    validate_skill_name(skill_name)?;
    let skills_dir = qoder_skills_dir().ok_or("无法找到 Qoder skills 目录")?;
    let skill_dir = skills_dir.join(skill_name);

    if !skill_dir.exists() {
        return Err(format!("Skill '{}' 不存在", skill_name));
    }

    let active_path = skill_dir.join("SKILL.md");
    let disabled_path = skill_dir.join("SKILL.md.disabled");

    if enabled {
        if disabled_path.exists() {
            fs::rename(&disabled_path, &active_path).map_err(|e| format!("重命名失败: {}", e))?;
        }
    } else {
        if active_path.exists() {
            fs::rename(&active_path, &disabled_path).map_err(|e| format!("重命名失败: {}", e))?;
        }
    }

    Ok(())
}

// ═══════════════════════════════════════════════════════════════
//  PROJECT-LEVEL SCAN

/// Scan Claude project-level standalone skills from `<project>/.claude/skills/`.
fn scan_claude_project_skills(
    project_root: &Path,
    plugin_skill_names: &std::collections::HashSet<String>,
) -> Vec<StandaloneSkill> {
    let skills_dir = project_root.join(".claude").join("skills");
    let pn = crate::project_name_from_root(project_root);
    scan_standalone_skills_in_dir(
        "claude",
        &skills_dir,
        plugin_skill_names,
        pn,
        Some(project_root),
    )
}

/// Scan Codex project-level standalone skills from `<project>/.codex/skills/`.
fn scan_codex_project_skills(project_root: &Path) -> Vec<StandaloneSkill> {
    let skills_dir = project_root.join(".codex").join("skills");
    let pn = crate::project_name_from_root(project_root);
    scan_codex_style_skills_in_dir("codex", &skills_dir, pn, Some(project_root))
}

/// Scan Qoder project-level standalone skills from `<project>/.qoder/skills/`.
fn scan_qoder_project_skills(project_root: &Path) -> Vec<StandaloneSkill> {
    // Qoder: user-level = ~/.qoder-cn/  ,  project-level = <project>/.qoder/
    let skills_dir = project_root.join(".qoder").join("skills");
    let pn = crate::project_name_from_root(project_root);
    scan_codex_style_skills_in_dir("qoder", &skills_dir, pn, Some(project_root))
}

/// Scan Claude project-level commands from `<project>/.claude/commands/`.
fn scan_claude_project_commands(project_root: &Path) -> Vec<CommandEntry> {
    let commands_dir = project_root.join(".claude").join("commands");
    let pn = crate::project_name_from_root(project_root);
    enumerate_commands_in_dir("claude", &commands_dir, pn, Some(project_root))
}

/// Scan Claude project-level plugins from `<project>/.claude/settings.json` `enabledPlugins`.
/// These are plugins that were installed with `--scope project`.
fn scan_claude_project_plugins(project_root: &Path) -> Vec<PluginEntry> {
    let mut plugins = Vec::new();
    let settings_path = project_root.join(".claude").join("settings.json");
    let text = match fs::read_to_string(&settings_path) {
        Ok(t) => t,
        Err(_) => return plugins,
    };
    let root_val: serde_json::Value = match serde_json::from_str(&text) {
        Ok(v) => v,
        Err(_) => return plugins,
    };
    let enabled_plugins = root_val.get("enabledPlugins").and_then(|v| v.as_object());

    if let Some(ep) = enabled_plugins {
        for (full_name, enabled_val) in ep {
            let (name, marketplace) = parse_plugin_id(full_name);
            let enabled = enabled_val.as_bool().unwrap_or(false);
            plugins.push(PluginEntry {
                id: format!(
                    "claude:project-plugin:{}:{}",
                    crate::hash_path(&project_root.to_string_lossy()),
                    full_name
                ),
                cli: "claude".into(),
                name: name.to_string(),
                marketplace: marketplace.to_string(),
                enabled,
                version: None,
                source: "project".to_string(),
                skills: vec![],
                agents: vec![],
                commands: vec![],
            });
        }
    }
    plugins
}

/// Scan Codex project-level plugins from `<project>/.codex/config.toml` `[plugins]`.
fn scan_codex_project_plugins(project_root: &Path) -> Vec<PluginEntry> {
    let mut plugins = Vec::new();
    let config_path = project_root.join(".codex").join("config.toml");
    let text = match fs::read_to_string(&config_path) {
        Ok(t) => t,
        Err(_) => return plugins,
    };
    let config: CodexConfig = match toml::from_str(&text) {
        Ok(c) => c,
        Err(_) => return plugins,
    };
    if let Some(plugins_map) = config.plugins {
        for (full_name, cfg) in plugins_map {
            let (name, marketplace) = parse_plugin_id(&full_name);
            plugins.push(PluginEntry {
                id: format!(
                    "codex:project-plugin:{}:{}",
                    crate::hash_path(&project_root.to_string_lossy()),
                    full_name
                ),
                cli: "codex".into(),
                name: name.to_string(),
                marketplace: marketplace.to_string(),
                enabled: cfg.enabled.unwrap_or(false),
                version: None,
                source: "project".to_string(),
                skills: vec![],
                agents: vec![],
                commands: vec![],
            });
        }
    }
    plugins
}

/// Generic standalone skill scanner for a given directory (Claude .md format).
fn scan_standalone_skills_in_dir(
    cli: &str,
    skills_dir: &Path,
    plugin_skill_names: &std::collections::HashSet<String>,
    project_name: Option<String>,
    project_root: Option<&Path>,
) -> Vec<StandaloneSkill> {
    let mut skills = Vec::new();
    let dir = match fs::read_dir(skills_dir) {
        Ok(d) => d,
        Err(_) => return skills,
    };
    for entry in dir.flatten() {
        let path = entry.path();
        let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if file_name.starts_with('.') {
            continue;
        }
        // Only accept: .md files, .md.disabled files, or directories with SKILL.md / SKILL.md.disabled
        let is_md = file_name.ends_with(".md") || file_name.ends_with(".md.disabled");
        let is_skill_dir = path.is_dir() && {
            let skill_md = path.join("SKILL.md");
            let skill_md_disabled = path.join("SKILL.md.disabled");
            skill_md.exists() || skill_md_disabled.exists()
        };
        if !is_md && !is_skill_dir {
            continue;
        }
        let name = if file_name.ends_with(".md.disabled") {
            file_name.strip_suffix(".md.disabled").unwrap_or(file_name)
        } else if file_name.ends_with(".md") {
            file_name.strip_suffix(".md").unwrap_or(file_name)
        } else {
            file_name
        };
        if plugin_skill_names.contains(name) {
            continue;
        }
        let link_type = classify_entry(&path);
        // For directory skills, check SKILL.md (not .disabled) — the directory
        // name itself doesn't change when toggled, unlike flat .md files.
        let enabled = if is_skill_dir {
            path.join("SKILL.md").exists()
        } else {
            !file_name.ends_with(".disabled")
        };
        let id = if let Some(root) = project_root {
            format!(
                "{}:project-skill:{}:{}",
                cli,
                crate::hash_path(&root.to_string_lossy()),
                name
            )
        } else {
            format!("{}:project-skill:{}", cli, name)
        };
        skills.push(StandaloneSkill {
            id,
            cli: cli.to_string(),
            name: name.to_string(),
            enabled,
            link_type: link_type.to_string(),
            path: path.to_string_lossy().to_string(),
            project_name: project_name.clone(),
        });
    }
    skills
}

/// Generic Codex/Qoder-style standalone skill scanner (directory + SKILL.md).
fn scan_codex_style_skills_in_dir(
    cli: &str,
    skills_dir: &Path,
    project_name: Option<String>,
    project_root: Option<&Path>,
) -> Vec<StandaloneSkill> {
    let mut skills = Vec::new();
    let dir = match fs::read_dir(skills_dir) {
        Ok(d) => d,
        Err(_) => return skills,
    };
    for entry in dir.flatten() {
        let path = entry.path();
        let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if file_name.starts_with('.') {
            continue;
        }
        if !path.is_dir() {
            continue;
        }
        let has_skill_md = path.join("SKILL.md").exists();
        let has_skill_md_disabled = path.join("SKILL.md.disabled").exists();
        if !has_skill_md && !has_skill_md_disabled {
            continue;
        }
        let enabled = has_skill_md;
        let id = if let Some(root) = project_root {
            format!(
                "{}:project-skill:{}:{}",
                cli,
                crate::hash_path(&root.to_string_lossy()),
                file_name
            )
        } else {
            format!("{}:project-skill:{}", cli, file_name)
        };
        skills.push(StandaloneSkill {
            id,
            cli: cli.to_string(),
            name: file_name.to_string(),
            enabled,
            link_type: classify_entry(&path).to_string(),
            path: path.to_string_lossy().to_string(),
            project_name: project_name.clone(),
        });
    }
    skills
}

/// Generic commands enumerator for a given directory.
fn enumerate_commands_in_dir(
    cli: &str,
    commands_dir: &Path,
    project_name: Option<String>,
    project_root: Option<&Path>,
) -> Vec<CommandEntry> {
    let mut commands = Vec::new();
    let dir = match fs::read_dir(commands_dir) {
        Ok(d) => d,
        Err(_) => return commands,
    };
    for entry in dir.flatten() {
        let path = entry.path();
        let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if file_name.starts_with('.') {
            continue;
        }
        let (name, enabled) = if let Some(n) = file_name.strip_suffix(".md.disabled") {
            (n.to_string(), false)
        } else if let Some(n) = file_name.strip_suffix(".md") {
            (n.to_string(), true)
        } else {
            continue;
        };
        let desc = extract_description(&path);
        let id = if let Some(root) = project_root {
            format!(
                "{}:project-command:{}:{}",
                cli,
                crate::hash_path(&root.to_string_lossy()),
                name
            )
        } else {
            format!("{}:project-command:{}", cli, name)
        };
        commands.push(CommandEntry {
            id,
            cli: cli.to_string(),
            name,
            path: path.to_string_lossy().to_string(),
            description: desc.unwrap_or_default(),
            enabled,
            project_name: project_name.clone(),
        });
    }
    commands
}

// ═══════════════════════════════════════════════════════════════
//  MAIN SCAN
// ═══════════════════════════════════════════════════════════════

pub fn scan_all(project_root: Option<&Path>) -> SkillManagerData {
    // Claude
    let claude_plugins = scan_claude_plugins();
    // Collect plugin-owned skill names for dedup (handles broken symlinks robustly)
    let claude_plugin_skill_names: std::collections::HashSet<String> = claude_plugins
        .iter()
        .flat_map(|p| p.skills.iter().map(|s| s.name.clone()))
        .collect();
    let claude_standalone = scan_claude_standalone_skills(&claude_plugin_skill_names);

    // Codex
    let codex_plugins = scan_codex_plugins();
    let codex_standalone = scan_codex_standalone_skills();
    let codex_system = scan_codex_system_skills();

    // Qoder — no plugins, only standalone skills
    let qoder_standalone = scan_qoder_standalone_skills();

    // Claude standalone commands
    let mut claude_commands = claude_commands_dir()
        .map(|d| enumerate_commands("claude", &d))
        .unwrap_or_default();

    // Merge user-level
    let mut plugins = claude_plugins;
    plugins.extend(codex_plugins);

    let mut standalone = claude_standalone;
    standalone.extend(codex_standalone);
    standalone.extend(qoder_standalone);

    // ── Project-level scanning ──
    if let Some(root) = project_root {
        // Snapshot user-level plugins before project merge, so we can
        // cross-reference and populate skills/agents/commands for project-level entries.
        let user_plugins_snapshot = plugins.clone();

        // Project skills
        let project_claude_skills = scan_claude_project_skills(root, &claude_plugin_skill_names);
        standalone.extend(project_claude_skills);

        let project_codex_skills = scan_codex_project_skills(root);
        standalone.extend(project_codex_skills);

        let project_qoder_skills = scan_qoder_project_skills(root);
        standalone.extend(project_qoder_skills);

        // Project commands
        let project_claude_commands = scan_claude_project_commands(root);
        claude_commands.extend(project_claude_commands);

        // Project plugins
        let project_claude_plugins = scan_claude_project_plugins(root);
        plugins.extend(project_claude_plugins);

        let project_codex_plugins = scan_codex_project_plugins(root);
        plugins.extend(project_codex_plugins);

        // Cross-reference: populate project-level plugins with skills/agents/commands
        // from their user-level counterparts (the plugin files live in user cache).
        for plugin in &mut plugins {
            if plugin.source == "project"
                && plugin.skills.is_empty()
                && plugin.agents.is_empty()
                && plugin.commands.is_empty()
            {
                if let Some(ref_plugin) = user_plugins_snapshot.iter().find(|up| {
                    up.name == plugin.name
                        && up.marketplace == plugin.marketplace
                        && up.cli == plugin.cli
                }) {
                    plugin.skills = ref_plugin.skills.clone();
                    plugin.agents = ref_plugin.agents.clone();
                    plugin.commands = ref_plugin.commands.clone();
                    if plugin.version.is_none() {
                        plugin.version = ref_plugin.version.clone();
                    }
                }
            }
        }
    }

    SkillManagerData {
        plugins,
        standalone_skills: standalone,
        system_skills: codex_system,
        commands: claude_commands,
    }
}

// ═══════════════════════════════════════════════════════════════
//  TOGGLE OPERATIONS
// ═══════════════════════════════════════════════════════════════

/// Toggle a Claude plugin: add/remove from settings.json `enabledPlugins`.
///
/// Handles both key formats that Claude Code uses:
/// - `"name@marketplace"` (common)
/// - `"claude:plugin:name@marketplace"` (legacy, seen with git-installed plugins like ecc)
pub fn toggle_claude_plugin(plugin_id: &str, enabled: bool) -> Result<(), String> {
    let path = claude_settings_json().ok_or("无法找到 Claude settings.json")?;
    toggle_claude_plugin_at(&path, plugin_id, enabled, false)
}

fn toggle_claude_project_plugin(
    plugin_id: &str,
    enabled: bool,
    project_path: &str,
) -> Result<(), String> {
    let project_dir = validate_project_dir(project_path)?;
    let settings_path = project_dir.join(".claude").join("settings.json");
    toggle_claude_plugin_at(&settings_path, plugin_id, enabled, true)
}

fn toggle_claude_plugin_at(
    path: &Path,
    plugin_id: &str,
    enabled: bool,
    keep_disabled_entry: bool,
) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("创建 Claude 配置目录失败: {}", e))?;
    }
    let text = fs::read_to_string(&path).map_err(|e| format!("读取失败: {}", e))?;
    let mut root: serde_json::Value =
        serde_json::from_str(&text).map_err(|e| format!("解析 JSON 失败: {}", e))?;

    // Backup
    let bak = path.with_extension("json.bak");
    let _ = fs::write(&bak, &text);

    let prefixed = format!("claude:plugin:{}", plugin_id);

    let obj = root["enabledPlugins"]
        .as_object_mut()
        .ok_or_else(|| "enabledPlugins 不是对象".to_string())?;

    if enabled {
        // Write with whichever key already exists, preferring the non-prefixed form.
        // Also clean up the alternate key to avoid duplicates.
        if obj.contains_key(&prefixed) {
            obj.remove(&prefixed);
        }
        obj.insert(plugin_id.to_string(), serde_json::Value::Bool(true));
    } else if keep_disabled_entry {
        obj.remove(&prefixed);
        obj.insert(plugin_id.to_string(), serde_json::Value::Bool(false));
    } else {
        // Remove both possible key formats
        obj.remove(plugin_id);
        obj.remove(&prefixed);
    }

    let new_text = serde_json::to_string_pretty(&root).map_err(|e| format!("序列化失败: {}", e))?;
    fs::write(&path, new_text).map_err(|e| format!("写入失败: {}", e))?;

    Ok(())
}

/// Toggle a Codex plugin: set `enabled = true/false` in config.toml.
/// Also toggles ALL marketplace variants of the same plugin so that
/// disabling a plugin once actually prevents Codex from loading it
/// from any source (built-in curated marketplace, user-added marketplace, etc.).
pub fn toggle_codex_plugin(plugin_id: &str, enabled: bool) -> Result<(), String> {
    let path = codex_config_toml().ok_or("无法找到 Codex config.toml")?;
    // Extract base name (before '@') to match all marketplace variants
    let base_name = plugin_id.split('@').next().unwrap_or(plugin_id);
    // Read config once, toggle all matching [plugins] entries
    let text =
        fs::read_to_string(&path).map_err(|e| format!("读取 Codex config.toml 失败: {}", e))?;
    // Backup
    let bak = path.with_extension("toml.bak");
    let _ = fs::write(&bak, &text);
    let mut config: toml::Table =
        toml::from_str(&text).map_err(|e| format!("解析 TOML 失败: {}", e))?;
    if !config.contains_key("plugins") {
        config.insert("plugins".into(), toml::Value::Table(toml::Table::new()));
    }
    let plugins = config["plugins"]
        .as_table_mut()
        .ok_or_else(|| "[plugins] 格式错误".to_string())?;
    // Collect all keys that match this plugin's base name
    let matching_keys: Vec<String> = plugins
        .keys()
        .filter(|k| {
            *k == base_name || k.starts_with(&format!("{}@", base_name))
        })
        .cloned()
        .collect();
    for key in &matching_keys {
        if let Some(plugin_table) = plugins.get_mut(key).and_then(|v| v.as_table_mut()) {
            plugin_table.insert("enabled".into(), toml::Value::Boolean(enabled));
        }
    }
    // Also scan on-disk for marketplace instances that don't have
    // a [plugins] entry yet, and create entries for them.
    let all_plugins = scan_codex_plugins();
    for p in &all_plugins {
        if p.name != base_name {
            continue;
        }
        let mkt_key = format!("{}@{}", p.name, p.marketplace);
        if !matching_keys.contains(&mkt_key)
            && !matching_keys.contains(&p.name)
        {
            let mut new_plugin = toml::Table::new();
            new_plugin.insert("enabled".into(), toml::Value::Boolean(enabled));
            plugins.insert(mkt_key, toml::Value::Table(new_plugin));
        }
    }
    // If no matching keys found at all (shouldn't happen, but be safe),
    // still toggle the original plugin_id
    if matching_keys.is_empty() && all_plugins.iter().all(|p| p.name != base_name) {
        if let Some(plugin_table) = plugins.get_mut(plugin_id).and_then(|v| v.as_table_mut()) {
            plugin_table.insert("enabled".into(), toml::Value::Boolean(enabled));
        } else {
            let mut new_plugin = toml::Table::new();
            new_plugin.insert("enabled".into(), toml::Value::Boolean(enabled));
            plugins.insert(plugin_id.to_string(), toml::Value::Table(new_plugin));
        }
    }
    let new_text = toml::to_string(&config).map_err(|e| format!("序列化失败: {}", e))?;
    fs::write(&path, new_text).map_err(|e| format!("写入失败: {}", e))?;

    // Flat-installed plugins (e.g. in .tmp/plugins/) are loaded by Codex
    // directly from the filesystem — config.toml [plugins] entries don't
    // control them.  Disable/enable by renaming their plugin.json.
    toggle_codex_flat_plugin_on_disk(base_name, enabled)?;

    Ok(())
}

/// Toggle a flat-installed plugin by renaming `.codex-plugin/plugin.json`
/// to `.codex-plugin/plugin.json.disabled` (disable) or the reverse (enable).
/// Searches `~/.codex/.tmp/plugins/plugins/<name>/`.
fn toggle_codex_flat_plugin_on_disk(name: &str, enabled: bool) -> Result<(), String> {
    let home = home_dir().ok_or("无法获取 HOME 目录")?;
    let flat_root = home.join(".codex").join(".tmp").join("plugins").join("plugins");
    if !flat_root.exists() {
        return Ok(());
    }
    let plugin_dir = flat_root.join(name);
    if !plugin_dir.exists() {
        return Ok(());
    }
    let manifest = plugin_dir.join(".codex-plugin").join("plugin.json");
    let disabled = plugin_dir.join(".codex-plugin").join("plugin.json.disabled");

    if enabled {
        // Enable: rename .disabled → active
        if disabled.exists() && !manifest.exists() {
            fs::rename(&disabled, &manifest)
                .map_err(|e| format!("启用 {} 失败: {}", name, e))?;
        }
    } else {
        // Disable: rename active → .disabled
        if manifest.exists() {
            fs::rename(&manifest, &disabled)
                .map_err(|e| format!("禁用 {} 失败: {}", name, e))?;
        }
    }
    Ok(())
}

fn toggle_codex_project_plugin(
    plugin_id: &str,
    enabled: bool,
    project_path: &str,
) -> Result<(), String> {
    let project_dir = validate_project_dir(project_path)?;
    let path = project_dir.join(".codex").join("config.toml");
    toggle_codex_plugin_at(&path, plugin_id, enabled)
}

fn toggle_codex_plugin_at(path: &Path, plugin_id: &str, enabled: bool) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("创建 Codex 配置目录失败: {}", e))?;
    }
    let text = fs::read_to_string(&path).map_err(|e| format!("读取失败: {}", e))?;

    // Backup
    let bak = path.with_extension("toml.bak");
    let _ = fs::write(&bak, &text);

    // Parse TOML
    let mut config: toml::Table =
        toml::from_str(&text).map_err(|e| format!("解析 TOML 失败: {}", e))?;

    // Navigate to plugins section
    if !config.contains_key("plugins") {
        config.insert("plugins".into(), toml::Value::Table(toml::Table::new()));
    }

    let plugins = config["plugins"]
        .as_table_mut()
        .ok_or_else(|| "[plugins] 格式错误".to_string())?;

    if let Some(plugin_table) = plugins.get_mut(plugin_id).and_then(|v| v.as_table_mut()) {
        plugin_table.insert("enabled".into(), toml::Value::Boolean(enabled));
    } else {
        // Plugin not in config yet — add it
        let mut new_plugin = toml::Table::new();
        new_plugin.insert("enabled".into(), toml::Value::Boolean(enabled));
        plugins.insert(plugin_id.to_string(), toml::Value::Table(new_plugin));
    }

    let new_text = toml::to_string(&config).map_err(|e| format!("序列化失败: {}", e))?;
    fs::write(&path, new_text).map_err(|e| format!("写入失败: {}", e))?;

    Ok(())
}

/// Toggle a Claude standalone skill: rename `xxx.md` ↔ `xxx.md.disabled`.
pub fn toggle_claude_standalone_skill(skill_name: &str, enabled: bool) -> Result<(), String> {
    validate_skill_name(skill_name)?;
    let skills_dir = claude_skills_dir().ok_or("无法找到 Claude skills 目录")?;

    let active_path = skills_dir.join(format!("{}.md", skill_name));
    let disabled_path = skills_dir.join(format!("{}.md.disabled", skill_name));

    if enabled {
        // Enable: rename .disabled → .md
        if disabled_path.exists() {
            fs::rename(&disabled_path, &active_path).map_err(|e| format!("重命名失败: {}", e))?;
        } else if !active_path.exists() {
            return Err(format!("Skill '{}' 不存在", skill_name));
        }
        // If active_path already exists, nothing to do
    } else {
        // Disable: rename .md → .md.disabled
        if active_path.exists() {
            fs::rename(&active_path, &disabled_path).map_err(|e| format!("重命名失败: {}", e))?;
        } else if !disabled_path.exists() {
            return Err(format!("Skill '{}' 不存在", skill_name));
        }
        // If already disabled, nothing to do
    }

    Ok(())
}

/// Toggle a Codex standalone skill: rename `SKILL.md` ↔ `SKILL.md.disabled`.
pub fn toggle_codex_standalone_skill(skill_name: &str, enabled: bool) -> Result<(), String> {
    validate_skill_name(skill_name)?;
    let skills_dir = codex_skills_dir().ok_or("无法找到 Codex skills 目录")?;
    let skill_dir = skills_dir.join(skill_name);

    if !skill_dir.exists() {
        // Try symlink case — skill might be a symlink to a single .md file
        let symlink_path = skills_dir.join(format!("{}.md", skill_name));
        if symlink_path.exists() {
            return toggle_codex_symlink_skill(&symlink_path, enabled);
        }
        return Err(format!("Skill '{}' 不存在", skill_name));
    }

    let active_path = skill_dir.join("SKILL.md");
    let disabled_path = skill_dir.join("SKILL.md.disabled");

    if enabled {
        if disabled_path.exists() {
            fs::rename(&disabled_path, &active_path).map_err(|e| format!("重命名失败: {}", e))?;
        }
    } else {
        if active_path.exists() {
            fs::rename(&active_path, &disabled_path).map_err(|e| format!("重命名失败: {}", e))?;
        }
    }

    Ok(())
}

fn toggle_codex_symlink_skill(symlink_path: &Path, enabled: bool) -> Result<(), String> {
    let parent = symlink_path.parent().unwrap_or(Path::new("."));
    let file_name = symlink_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("");

    let base = file_name.strip_suffix(".md").unwrap_or(file_name);
    let disabled_name = format!("{}.md.disabled", base);
    let active_name = format!("{}.md", base);

    let disabled_path = parent.join(&disabled_name);
    let active_path = parent.join(&active_name);

    if enabled {
        if disabled_path.exists() {
            fs::rename(&disabled_path, &active_path).map_err(|e| format!("重命名失败: {}", e))?;
        }
    } else {
        if active_path.exists() {
            fs::rename(&active_path, &disabled_path).map_err(|e| format!("重命名失败: {}", e))?;
        }
    }

    Ok(())
}

// ═══════════════════════════════════════════════════════════════
//  PLUGIN UPDATE CHECKING
// ═══════════════════════════════════════════════════════════════

/// Parse `known_marketplaces.json` and return a map of marketplace name → install location.
fn read_known_marketplaces() -> HashMap<String, String> {
    let mut map = HashMap::new();
    let path = match claude_known_marketplaces_json() {
        Some(p) => p,
        None => return map,
    };
    let text = match fs::read_to_string(&path) {
        Ok(t) => t,
        Err(_) => return map,
    };
    let root: serde_json::Value = match serde_json::from_str(&text) {
        Ok(v) => v,
        Err(_) => return map,
    };
    if let Some(obj) = root.as_object() {
        for (name, entry) in obj {
            if let Some(loc) = entry.get("installLocation").and_then(|v| v.as_str()) {
                map.insert(name.clone(), loc.to_string());
            }
        }
    }
    map
}

/// Read the latest available version for a plugin from its marketplace manifest.
fn get_marketplace_plugin_version(marketplace_dir: &Path, plugin_name: &str) -> Option<String> {
    let manifest_path = marketplace_dir
        .join(".claude-plugin")
        .join("marketplace.json");
    let text = fs::read_to_string(&manifest_path).ok()?;
    let root: serde_json::Value = serde_json::from_str(&text).ok()?;
    let plugins = root.get("plugins")?.as_array()?;
    for p in plugins {
        if p.get("name")?.as_str()? == plugin_name {
            return p.get("version")?.as_str().map(|v| v.to_string());
        }
    }
    None
}

/// Read installed plugins and their versions from `installed_plugins.json`.
fn read_installed_plugin_versions() -> HashMap<String, String> {
    // Returns: plugin_full_name → version
    let mut map = HashMap::new();
    let path = match claude_plugins_json() {
        Some(p) => p,
        None => return map,
    };
    let text = match fs::read_to_string(&path) {
        Ok(t) => t,
        Err(_) => return map,
    };
    let root: serde_json::Value = match serde_json::from_str(&text) {
        Ok(v) => v,
        Err(_) => return map,
    };
    if let Some(plugins) = root.get("plugins").and_then(|v| v.as_object()) {
        for (full_name, installs) in plugins {
            if let Some(arr) = installs.as_array() {
                if let Some(inst) = arr.first() {
                    if let Some(ver) = inst.get("version").and_then(|v| v.as_str()) {
                        map.insert(full_name.clone(), ver.to_string());
                    }
                }
            }
        }
    }
    map
}

/// Compare two semver-like version strings (e.g. "3.0.2" vs "3.0.1").
/// Returns true if `a` is strictly greater than `b`.
/// Handles leading 'v' prefix and variable-length segments gracefully.
fn is_version_greater(a: &str, b: &str) -> bool {
    let parse = |s: &str| -> Vec<u64> {
        s.trim_start_matches('v')
            .split('.')
            .filter_map(|p| p.parse::<u64>().ok())
            .collect()
    };
    let va = parse(a);
    let vb = parse(b);
    let len = va.len().max(vb.len());
    for i in 0..len {
        let na = va.get(i).copied().unwrap_or(0);
        let nb = vb.get(i).copied().unwrap_or(0);
        if na > nb {
            return true;
        }
        if na < nb {
            return false;
        }
    }
    false // equal
}

/// Check for available plugin updates by comparing installed version with
/// the latest version listed in the marketplace manifest.
/// Checks the cancel flag between each plugin to support user cancellation.
pub fn check_claude_updates(cancel: Arc<AtomicBool>) -> Vec<PluginUpdateInfo> {
    cancel.store(false, Ordering::SeqCst);

    let marketplaces = read_known_marketplaces();
    let installed = read_installed_plugin_versions();

    let mut results = Vec::new();
    for (full_name, current_version) in &installed {
        if cancel.load(Ordering::SeqCst) {
            break;
        }

        let marketplace_name = full_name.rsplit_once('@').map(|(_, mkt)| mkt).unwrap_or("");
        let plugin_name = full_name
            .rsplit_once('@')
            .map(|(n, _)| n)
            .unwrap_or(full_name);

        let latest_version = marketplaces
            .get(marketplace_name)
            .and_then(|mkt_dir| get_marketplace_plugin_version(Path::new(mkt_dir), plugin_name));

        let has_update = latest_version
            .as_ref()
            .is_some_and(|latest| is_version_greater(latest, current_version));

        results.push(PluginUpdateInfo {
            plugin_id: format!("claude:plugin:{}", full_name),
            current_version: current_version.clone(),
            current_sha: String::new(),
            latest_sha: latest_version.unwrap_or_default(),
            has_update,
        });
    }

    results
}

/// Execute plugin update via Claude CLI.
pub fn exec_claude_plugin_update(plugin_id: &str) -> Result<String, String> {
    let full_name = strip_id_prefix(plugin_id);
    let (name, marketplace) = match full_name.rsplit_once('@') {
        Some((n, m)) => (n, m),
        None => return Err(format!("无法解析 plugin ID: {}", plugin_id)),
    };

    // Step 1: sync marketplace
    let sync = std::process::Command::new(cli_binary("claude"))
        .args(["plugin", "marketplace", "update", marketplace])
        
        .output()
        .map_err(|e| format!("同步 marketplace 失败: {}", e))?;
    if !sync.status.success() {
        let err = String::from_utf8_lossy(&sync.stderr);
        return Err(format!("同步 marketplace 失败: {}", err.trim()));
    }

    // Step 2: update the plugin
    let update = std::process::Command::new(cli_binary("claude"))
        .args(["plugin", "update", full_name])
        
        .output()
        .map_err(|e| format!("更新插件失败: {}", e))?;
    if !update.status.success() {
        let err = String::from_utf8_lossy(&update.stderr);
        return Err(format!("更新失败: {}", err.trim()));
    }

    Ok(format!("{} 已更新到最新版本", name))
}

// ═══════════════════════════════════════════════════════════════
//  MARKETPLACE SCAN
// ═══════════════════════════════════════════════════════════════

/// Scan Claude marketplace: read marketplace manifests and cross-reference with installed plugins.
fn scan_claude_marketplace() -> MarketplaceData {
    let mut plugins = Vec::new();
    let mut marketplace_names = Vec::new();

    let known = read_known_marketplaces();
    let installed_versions = read_installed_plugin_versions();

    for (mkt_name, mkt_dir) in &known {
        marketplace_names.push(mkt_name.clone());

        let manifest_path = Path::new(mkt_dir)
            .join(".claude-plugin")
            .join("marketplace.json");
        let text = match fs::read_to_string(&manifest_path) {
            Ok(t) => t,
            Err(_) => continue,
        };
        let root: serde_json::Value = match serde_json::from_str(&text) {
            Ok(v) => v,
            Err(_) => continue,
        };
        let plugin_list = match root.get("plugins").and_then(|v| v.as_array()) {
            Some(arr) => arr,
            None => continue,
        };

        for p in plugin_list {
            let name = match p.get("name").and_then(|v| v.as_str()) {
                Some(n) => n.to_string(),
                None => continue,
            };
            let version = p
                .get("version")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            let description = p
                .get("description")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());

            // Count skills in the plugin's own directory (derived from the
            // "source" field in marketplace.json, e.g. "./skills/plugin-name").
            // Falls back to <mkt_dir>/skills/ if source is missing (legacy),
            // but correctly detects single-skill plugins (root SKILL.md) vs
            // multi-skill containers (skills/ subdirectory).
            let source = p.get("source").and_then(|v| v.as_str()).unwrap_or("");
            let plugin_dir = if source.starts_with("./") {
                Path::new(mkt_dir).join(source.trim_start_matches("./"))
            } else {
                // Legacy: no source field — use marketplace-level skills/
                Path::new(mkt_dir).join("skills")
            };
            let skill_count = if plugin_dir.join("SKILL.md").exists() {
                // Single-skill plugin (e.g., pm-skills style)
                1
            } else if plugin_dir.join("skills").exists() {
                // Multi-skill container plugin
                enumerate_claude_skills(&plugin_dir.join("skills")).len()
            } else if plugin_dir.exists() && plugin_dir.is_dir() {
                // Directory exists but no clear skill structure —
                // scan for .md files directly (paranoid fallback)
                fs::read_dir(&plugin_dir)
                    .map(|d| {
                        d.flatten()
                            .filter(|e| {
                                let path = e.path();
                                let n = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                                !n.starts_with('.')
                                    && (n.ends_with(".md")
                                        || (path.is_dir() && path.join("SKILL.md").exists()))
                            })
                            .count()
                    })
                    .unwrap_or(0)
            } else {
                0
            };

            let full_name = format!("{}@{}", name, mkt_name);
            let installed = installed_versions.contains_key(&full_name);

            plugins.push(MarketplacePluginEntry {
                name,
                marketplace: mkt_name.clone(),
                cli: "claude".into(),
                version,
                description,
                installed,
                user_installed: Some(installed),
                project_installed: None,
                skill_count,
            });
        }
    }

    MarketplaceData {
        plugins,
        marketplaces: marketplace_names,
    }
}

/// Scan Codex marketplace: read bundled + runtime marketplace plugin manifests.
fn scan_codex_marketplace() -> Vec<MarketplacePluginEntry> {
    let mut plugins = Vec::new();
    let installed_states = read_codex_plugin_states();

    if let Some(home) = home_dir() {
        // Three search roots, each with a different directory structure:
        // 1. bundled-marketplaces → root/mkt-name/plugins/plugin-name/.codex-plugin/
        // 2. runtime cache         → same structure as bundled-marketplaces
        // 3. marketplaces          → root/mkt-name/.claude-plugin/  (flat: mkt IS the plugin)
        let search_dirs: Vec<(PathBuf, &str, bool)> = vec![
            (
                home.join(".codex")
                    .join(".tmp")
                    .join("bundled-marketplaces"),
                "bundled",
                false, // has plugins/ subdirectory
            ),
            (
                home.join(".cache")
                    .join("codex-runtimes")
                    .join("codex-primary-runtime")
                    .join("plugins"),
                "bundled",
                false, // has plugins/ subdirectory
            ),
            (
                home.join(".codex").join(".tmp").join("marketplaces"),
                "user",
                true, // flat: marketplace dir IS the plugin dir
            ),
        ];

        for (root, source_label, is_flat) in &search_dirs {
            if !root.exists() {
                continue;
            }
            let dir = match fs::read_dir(root) {
                Ok(d) => d,
                Err(_) => continue,
            };
            for mkt_entry in dir.flatten() {
                let mkt_path = mkt_entry.path();
                if !mkt_path.is_dir() {
                    continue;
                }
                // Skip staging directories and hidden dirs
                let mkt_name = mkt_path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("unknown");
                if mkt_name.starts_with('.') {
                    continue;
                }

                if *is_flat {
                    // Flat structure: mkt_path itself is the plugin directory
                    scan_single_codex_plugin(
                        &mkt_path,
                        mkt_name,
                        mkt_name,
                        source_label,
                        &installed_states,
                        &mut plugins,
                    );
                } else {
                    // Nested structure: mkt_path/plugins/plugin-name/
                    let plugins_dir = mkt_path.join("plugins");
                    if !plugins_dir.exists() {
                        // Also try flat fallback for directories that have manifest at top level
                        if read_plugin_manifest_json(&mkt_path).is_some() {
                            scan_single_codex_plugin(
                                &mkt_path,
                                mkt_name,
                                mkt_name,
                                source_label,
                                &installed_states,
                                &mut plugins,
                            );
                        }
                        continue;
                    }
                    if let Ok(plugin_entries) = fs::read_dir(&plugins_dir) {
                        for p in plugin_entries.flatten() {
                            let plugin_path = p.path();
                            if !plugin_path.is_dir() {
                                continue;
                            }
                            let plugin_name = plugin_path
                                .file_name()
                                .and_then(|n| n.to_str())
                                .unwrap_or("unknown");
                            scan_single_codex_plugin(
                                &plugin_path,
                                plugin_name,
                                mkt_name,
                                source_label,
                                &installed_states,
                                &mut plugins,
                            );
                        }
                    }
                }
            }
        }
    }

    plugins
}

/// Read a single plugin from the given directory and add it to the list if valid.
fn scan_single_codex_plugin(
    plugin_path: &Path,
    plugin_name: &str,
    marketplace_name: &str,
    _source_label: &str,
    installed_states: &HashMap<String, bool>,
    plugins: &mut Vec<MarketplacePluginEntry>,
) {
    let (manifest, _text) = match read_plugin_manifest_json(plugin_path) {
        Some(m) => m,
        None => return,
    };

    let name = manifest
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or(plugin_name)
        .to_string();
    let version = manifest
        .get("version")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let description = manifest
        .get("description")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    // Count skills
    let skills_dir = manifest
        .get("skills")
        .and_then(|v| v.as_str())
        .map(|rel| plugin_path.join(rel))
        .unwrap_or_else(|| plugin_path.join("skills"));
    let skill_count = if skills_dir.exists() {
        fs::read_dir(&skills_dir)
            .map(|d| {
                d.flatten()
                    .filter(|e| {
                        e.path().is_dir()
                            && !e.file_name().to_str().is_none_or(|n| n.starts_with('.'))
                            && e.path().join("SKILL.md").exists()
                    })
                    .count()
            })
            .unwrap_or(0)
    } else {
        0
    };

    let full_id = format!("{}@{}", name, marketplace_name);
    let installed = installed_states.get(&full_id).copied().unwrap_or(false);

    // Avoid duplicates
    if !plugins.iter().any(|existing: &MarketplacePluginEntry| {
        existing.name == name && existing.marketplace == marketplace_name
    }) {
        plugins.push(MarketplacePluginEntry {
            name,
            marketplace: marketplace_name.to_string(),
            cli: "codex".into(),
            version,
            description,
            installed,
            user_installed: Some(installed),
            project_installed: None,
            skill_count,
        });
    }
}

#[tauri::command]
pub fn list_marketplace_plugins(cli: String, project_path: Option<String>) -> MarketplaceData {
    let mut plugins = Vec::new();
    let mut marketplaces: Vec<String> = Vec::new();

    match cli.as_str() {
        "all" | "claude" => {
            let claude_data = scan_claude_marketplace();
            marketplaces.extend(claude_data.marketplaces);
            plugins.extend(claude_data.plugins);
        }
        _ => {}
    }

    match cli.as_str() {
        "all" | "codex" => {
            let codex_plugins = scan_codex_marketplace();
            // Collect unique marketplace names from codex plugin entries
            for p in &codex_plugins {
                if !marketplaces.contains(&p.marketplace) {
                    marketplaces.push(p.marketplace.clone());
                }
            }
            plugins.extend(codex_plugins);
        }
        _ => {}
    }

    // Dedup marketplaces
    marketplaces.sort();
    marketplaces.dedup();

    if let Some(ref project_path) = project_path {
        annotate_project_marketplace_installs(&mut plugins, project_path);
    }

    MarketplaceData {
        plugins,
        marketplaces,
    }
}

fn annotate_project_marketplace_installs(
    plugins: &mut [MarketplacePluginEntry],
    project_path: &str,
) {
    let project_dir = match validate_project_dir(project_path) {
        Ok(dir) => dir,
        Err(_) => return,
    };
    let project_plugins = scan_project_plugin_keys(&project_dir);

    for plugin in plugins {
        let key = format!("{}:{}@{}", plugin.cli, plugin.name, plugin.marketplace);
        let project_installed = project_plugins.contains(&key);
        plugin.user_installed = Some(plugin.installed);
        plugin.project_installed = Some(project_installed);
        plugin.installed = project_installed;
    }
}

fn scan_project_plugin_keys(project_root: &Path) -> std::collections::HashSet<String> {
    let mut keys = std::collections::HashSet::new();

    for plugin in scan_claude_project_plugins(project_root) {
        keys.insert(format!("{}:{}@{}", plugin.cli, plugin.name, plugin.marketplace));
    }
    for plugin in scan_codex_project_plugins(project_root) {
        keys.insert(format!("{}:{}@{}", plugin.cli, plugin.name, plugin.marketplace));
    }

    keys
}

// ═══════════════════════════════════════════════════════════════
//  MARKETPLACE ADD / REMOVE
// ═══════════════════════════════════════════════════════════════

#[tauri::command]
pub async fn add_marketplace(
    app_handle: tauri::AppHandle,
    cli: String,
    source: String,
) -> Result<String, String> {
    let cli_clone = cli.clone();

    let result: Result<String, String> = match cli.as_str() {
        "claude" => {
            let src = source;
            tauri::async_runtime::spawn_blocking(move || run_cli_marketplace_add("claude", &src))
                .await
                .map_err(|e| format!("执行失败: {}", e))?
        }
        "codex" => {
            let src = source;
            tauri::async_runtime::spawn_blocking(move || run_cli_marketplace_add("codex", &src))
                .await
                .map_err(|e| format!("执行失败: {}", e))?
        }
        _ => return Err(format!("不支持的 CLI: {}", cli)),
    };

    // Emit event for other listeners (e.g., marketplace browser refresh)
    let _ = app_handle.emit(
        "marketplace-changed",
        serde_json::json!({
            "cli": cli_clone,
            "success": result.is_ok(),
            "message": result.clone().unwrap_or_else(|e| e),
        }),
    );

    result
}

/// Execute `plugin marketplace add <source>` for the given CLI.
/// On failure, tries to clean up leftover directory state and retries once.
fn run_cli_marketplace_add(cli_name: &str, source: &str) -> Result<String, String> {
    let binary = cli_binary(cli_name);

    let do_add = || -> Result<String, String> {
        let output = std::process::Command::new(&binary)
            .args(["plugin", "marketplace", "add", source])
            
            .output()
            .map_err(|e| format!("无法执行 {} ({}): {}", cli_name, binary, e))?;

        if output.status.success() {
            Ok("Marketplace 添加成功".to_string())
        } else {
            let err = String::from_utf8_lossy(&output.stderr).trim().to_string();
            let out = String::from_utf8_lossy(&output.stdout).trim().to_string();
            let msg = if err.is_empty() { out } else { err };
            Err(msg)
        }
    };

    match do_add() {
        Ok(msg) => Ok(msg),
        Err(_first_err) => {
            // Try to clean up leftover directory and retry
            let dir_name = guess_repo_name(source);
            if let Some(home) = home_dir() {
                let plugins_dir = home.join(".claude").join("plugins");
                let stale_dir = plugins_dir.join(&dir_name);
                if stale_dir.exists() {
                    let _ = std::fs::remove_dir_all(&stale_dir);
                }
            }
            // Also try cleaning known_marketplaces.json entry
            // Match by trailing path component to avoid false positives
            if let Some(mut known) = read_known_marketplaces_raw() {
                if let Some(obj) = known.as_object_mut() {
                    let dir_name = guess_repo_name(source);
                    let keys_to_remove: Vec<String> = obj
                        .iter()
                        .filter(|(_, v)| {
                            v.get("installLocation")
                                .and_then(|loc| loc.as_str())
                                .map(|loc| {
                                    // Exact match on the last path component (directory name)
                                    std::path::Path::new(loc)
                                        .file_name()
                                        .and_then(|n| n.to_str())
                                        .map(|n| n == dir_name.as_str())
                                        .unwrap_or(false)
                                })
                                .unwrap_or(false)
                        })
                        .map(|(k, _)| k.clone())
                        .collect();
                    for k in &keys_to_remove {
                        obj.remove(k);
                    }
                    if !keys_to_remove.is_empty() {
                        let _ = write_known_marketplaces(&known);
                    }
                }
            }

            // Retry once
            match do_add() {
                Ok(msg) => Ok(msg),
                Err(second_err) => Err(format!("添加失败: {}（已尝试自动清理后重试）", second_err)),
            }
        }
    }
}

/// Guess the local directory name for a marketplace from its source URL.
/// Returns a sanitized name suitable for filesystem use, or the original
/// source if no valid name can be extracted.
fn guess_repo_name(source: &str) -> String {
    // Handle "owner/repo" shorthand — must not contain URL scheme or backslash
    if let Some((_, repo)) = source.split_once('/') {
        if !source.contains("://") && !source.contains('\\') {
            return sanitize_dir_name(repo);
        }
    }
    // Handle full git URLs: https://github.com/owner/repo.git
    // Extract the last path segment, strip .git suffix
    if let Some(after_slash) = source.rsplit('/').next() {
        let name = after_slash.strip_suffix(".git").unwrap_or(after_slash);
        return sanitize_dir_name(name);
    }
    sanitize_dir_name(source)
}

/// Ensure a directory name is safe for filesystem operations.
/// Blocks path traversal (`..`, `/`, `\`) and other dangerous patterns.
fn sanitize_dir_name(name: &str) -> String {
    // Reject path traversal components
    if name.is_empty()
        || name.contains("..")
        || name.contains('/')
        || name.contains('\\')
        || name.starts_with('.')
        || name.starts_with('-')
        || name.len() > 128
    {
        // Return a hash-based safe fallback
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut h = DefaultHasher::new();
        name.hash(&mut h);
        return format!("repo-{:x}", h.finish());
    }
    name.to_string()
}

/// Read known_marketplaces.json as raw JSON Value.
fn read_known_marketplaces_raw() -> Option<serde_json::Value> {
    let path = claude_known_marketplaces_json()?;
    let text = fs::read_to_string(&path).ok()?;
    serde_json::from_str(&text).ok()
}

/// Write JSON to known_marketplaces.json.
fn write_known_marketplaces(value: &serde_json::Value) -> Result<(), String> {
    let path = claude_known_marketplaces_json().ok_or("无法找到 known_marketplaces.json")?;
    let text = serde_json::to_string_pretty(value).map_err(|e| format!("序列化失败: {}", e))?;
    fs::write(&path, text).map_err(|e| format!("写入失败: {}", e))
}

#[tauri::command]
pub async fn remove_marketplace(
    app_handle: tauri::AppHandle,
    cli: String,
    name: String,
) -> Result<String, String> {
    let cli_clone = cli.clone();
    let n = name.clone();

    let result: Result<String, String> = match cli.as_str() {
        "claude" => {
            let n = name;
            tauri::async_runtime::spawn_blocking(move || run_cli_marketplace_remove("claude", &n))
                .await
                .map_err(|e| format!("执行失败: {}", e))?
        }
        "codex" => {
            let n = name;
            tauri::async_runtime::spawn_blocking(move || run_cli_marketplace_remove("codex", &n))
                .await
                .map_err(|e| format!("执行失败: {}", e))?
        }
        _ => return Err(format!("不支持的 CLI: {}", cli)),
    };

    // Emit event for other listeners
    let _ = app_handle.emit(
        "marketplace-changed",
        serde_json::json!({
            "cli": cli_clone,
            "name": n,
            "success": result.is_ok(),
            "message": result.clone().unwrap_or_else(|e| e),
        }),
    );

    result
}

/// Execute `plugin marketplace remove <name>` for the given CLI.
fn run_cli_marketplace_remove(cli_name: &str, name: &str) -> Result<String, String> {
    let binary = cli_binary(cli_name);
    let output = std::process::Command::new(&binary)
        .args(["plugin", "marketplace", "remove", name])
        
        .output()
        .map_err(|e| format!("无法执行 {} ({}): {}", cli_name, binary, e))?;

    if output.status.success() {
        Ok(format!("Marketplace '{}' 已移除", name))
    } else {
        let err = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let out = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let msg = if err.is_empty() { out } else { err };
        Err(format!("移除失败: {}", msg))
    }
}

// ═══════════════════════════════════════════════════════════════
//  PLUGIN INSTALL / UNINSTALL
// ═══════════════════════════════════════════════════════════════

#[tauri::command]
pub async fn install_plugin(
    app_handle: tauri::AppHandle,
    cli: String,
    name: String,
    marketplace: String,
    project_path: Option<String>,
) -> Result<String, String> {
    let cli_clone = cli.clone();
    let n = name.clone();

    let result: Result<String, String> = match cli.as_str() {
        "claude" => {
            let full = format!("{}@{}", name, marketplace);
            let n = name;
            let project_dir = match project_path.as_deref() {
                Some(path) => Some(validate_project_dir(path)?),
                None => None,
            };
            let scope = if project_dir.is_some() {
                "project"
            } else {
                "user"
            };
            tauri::async_runtime::spawn_blocking(move || {
                run_cli_plugin_action(
                    "claude",
                    &["plugin", "install", &full, "--scope", scope],
                    project_dir.as_deref(),
                    &format!("{} 安装成功", n),
                    "安装失败",
                )
            })
            .await
            .map_err(|e| format!("执行失败: {}", e))?
        }
        "codex" => {
            let full = format!("{}@{}", name, marketplace);
            let n = name.clone();
            // Step 1: always download plugin files to user cache.
            // `codex plugin add` does two things: (a) downloads files, (b) registers in user config.
            let cache_result = tauri::async_runtime::spawn_blocking(move || {
                run_cli_plugin_action(
                    "codex",
                    &["plugin", "add", &full],
                    None,
                    &format!("{} 已下载到缓存", n),
                    "安装失败",
                )
            })
            .await
            .map_err(|e| format!("执行失败: {}", e))?;

            if let Some(ref proj) = project_path {
                // Project-scoped install: the plugin files are now in user cache
                // (~/.codex/plugins/). We need to:
                //   (1b) Remove the user-level config registration that `codex plugin add`
                //        created — the plugin should NOT be enabled at user scope.
                //   (2)  Write project-level config so the plugin is enabled for this project.
                if cache_result.is_ok() {
                    // Only clean up user config if download succeeded.
                    let _ = remove_codex_user_plugin_config(&name, &marketplace);
                    match write_codex_project_plugin_enabled(proj, &name, &marketplace, true) {
                        Ok(()) => Ok(format!("{} 已安装到项目", name)),
                        Err(e) => Err(format!(
                            "插件已下载到缓存，但项目级配置写入失败: {}\n请手动检查: {}/.codex/config.toml",
                            e, proj
                        )),
                    }
                } else {
                    // Download failed — propagate the error, don't touch project config.
                    cache_result
                }
            } else {
                // User-scoped install: keep the default behavior (cache + user config).
                cache_result
            }
        }
        _ => return Err(format!("不支持的 CLI: {}", cli)),
    };

    let _ = app_handle.emit(
        "plugin-install-complete",
        serde_json::json!({
        "name": n,
            "marketplace": marketplace,
            "installKey": format!("{}:{}:{}", cli_clone, marketplace, n),
            "cli": cli_clone,
            "success": result.is_ok(),
            "message": result.clone().unwrap_or_else(|e| e),
        }),
    );

    result
}

#[tauri::command]
pub async fn uninstall_plugin(
    app_handle: tauri::AppHandle,
    cli: String,
    plugin_id: String,
    project_path: Option<String>,
) -> Result<String, String> {
    let cli_clone = cli.clone();
    let name = strip_id_prefix(&plugin_id).to_string();

    let result: Result<String, String> = match cli.as_str() {
        "claude" => {
            let n = name.clone();
            if let Some(ref proj) = project_path {
                let project_dir = validate_project_dir(proj)?;
                // Project-level uninstall: use --scope project
                tauri::async_runtime::spawn_blocking(move || {
                    run_cli_plugin_action(
                        "claude",
                        &["plugin", "uninstall", &n, "--scope", "project", "-y"],
                        Some(&project_dir),
                        &format!("{} 已从项目中移除", n),
                        "删除失败",
                    )
                })
                .await
                .map_err(|e| format!("执行失败: {}", e))?
            } else {
                tauri::async_runtime::spawn_blocking(move || {
                    run_cli_plugin_action(
                        "claude",
                        &["plugin", "uninstall", &n, "-y"],
                        None,
                        &format!("{} 删除成功", n),
                        "删除失败",
                    )
                })
                .await
                .map_err(|e| format!("执行失败: {}", e))?
            }
        }
        "codex" => {
            let n = name.clone();
            if let Some(ref proj) = project_path {
                // Project-level: only remove from project config.toml, keep cache
                match remove_codex_project_plugin(proj, &n) {
                    Ok(()) => Ok(format!("{} 已从项目中移除", n)),
                    Err(e) => Err(e),
                }
            } else {
                tauri::async_runtime::spawn_blocking(move || {
                    run_cli_plugin_action(
                        "codex",
                        &["plugin", "remove", &n],
                        None,
                        &format!("{} 删除成功", n),
                        "删除失败",
                    )
                })
                .await
                .map_err(|e| format!("执行失败: {}", e))?
            }
        }
        _ => return Err(format!("不支持的 CLI: {}", cli)),
    };

    let _ = app_handle.emit(
        "plugin-uninstall-complete",
        serde_json::json!({
            "pluginId": plugin_id,
            "cli": cli_clone,
            "success": result.is_ok(),
            "message": result.clone().unwrap_or_else(|e| e),
        }),
    );

    result
}

/// Execute a CLI subprocess and return a standardized result.
fn run_cli_plugin_action(
    cli_name: &str,
    args: &[&str],
    current_dir: Option<&Path>,
    success_msg: &str,
    error_prefix: &str,
) -> Result<String, String> {
    let binary = cli_binary(cli_name);
    let output = run_command_with_timeout(
        cli_name,
        &binary,
        args,
        current_dir,
        Duration::from_secs(60),
    )?;

    if output.status.success() {
        Ok(success_msg.to_string())
    } else {
        let err = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let out = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let msg = if err.is_empty() { out } else { err };
        Err(format!("{}: {}", error_prefix, msg))
    }
}

fn run_command_with_timeout(
    cli_name: &str,
    binary: &str,
    args: &[&str],
    current_dir: Option<&Path>,
    timeout: Duration,
) -> Result<std::process::Output, String> {
    let mut command = std::process::Command::new(binary);
    command
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        ;
    if let Some(dir) = current_dir {
        command.current_dir(dir);
    }

    let mut child = command
        .spawn()
        .map_err(|e| format!("无法执行 {} ({}): {}", cli_name, binary, e))?;
    let start = Instant::now();

    loop {
        match child
            .try_wait()
            .map_err(|e| format!("等待 {} 执行失败: {}", cli_name, e))?
        {
            Some(_) => {
                return child
                    .wait_with_output()
                    .map_err(|e| format!("读取 {} 输出失败: {}", cli_name, e));
            }
            None if start.elapsed() >= timeout => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!(
                    "{} 执行超时（{} 秒）。请检查网络、Marketplace 状态或 CLI 是否在等待交互。",
                    cli_name,
                    timeout.as_secs()
                ));
            }
            None => std::thread::sleep(Duration::from_millis(100)),
        }
    }
}

// ═══════════════════════════════════════════════════════════════
//  STANDALONE SKILL INSTALL / UNINSTALL
// ═══════════════════════════════════════════════════════════════

/// Validate skill name: only allow alphanumeric, hyphens, underscores, dots.
/// Blocks path traversal (`../`, `/`) and shell special characters.
fn validate_skill_name(name: &str) -> Result<(), String> {
    if name.is_empty() {
        return Err("Skill 名称不能为空".into());
    }
    if name.contains('/') || name.contains('\\') || name.contains("..") {
        return Err(format!("非法 Skill 名称: '{}'（包含路径分隔符）", name));
    }
    // Reject names with shell-special characters
    if name.contains('$')
        || name.contains('`')
        || name.contains(';')
        || name.contains('|')
        || name.contains('&')
        || name.contains('<')
        || name.contains('>')
        || name.contains('\'')
        || name.contains('"')
    {
        return Err(format!("Skill 名称包含非法字符: '{}'", name));
    }
    // Reject names starting with '.' or '-'
    if name.starts_with('.') || name.starts_with('-') {
        return Err(format!("Skill 名称不能以 '.' 或 '-' 开头: '{}'", name));
    }
    if name.len() > 128 {
        return Err(format!("Skill 名称过长（最多 128 字符）: '{}'", name));
    }
    Ok(())
}

/// Install a Claude standalone skill from a local directory (containing SKILL.md)
/// or a single .md file (backward compatible).
fn install_claude_standalone_skill_to(
    source: &Path,
    skills_dir: &Path,
    overwrite: bool,
) -> Result<String, String> {
    if source.is_dir() {
        // Directory mode: validate SKILL.md, then copy entire directory
        require_skill_md(source)?;

        let name = source
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_string();
        validate_skill_name(&name)?;

        let dest_dir = skills_dir.join(&name);
        if dest_dir.exists() {
            if overwrite {
                fs::remove_dir_all(&dest_dir).map_err(|e| format!("无法移除旧版本: {}", e))?;
            } else {
                return Err(format!(
                    "Skill '{}' 已存在于 {}",
                    name,
                    skills_dir.display()
                ));
            }
        }

        fs::create_dir_all(&dest_dir).map_err(|e| format!("创建目录失败: {}", e))?;
        copy_dir_contents(source, &dest_dir)?;

        Ok(format!("Skill '{}' 安装成功", name))
    } else {
        // Backward compat: single .md file mode
        let file_name = source.file_name().and_then(|n| n.to_str()).unwrap_or("");
        let name = file_name.strip_suffix(".md").unwrap_or(file_name);

        validate_skill_name(name)?;

        let dest = skills_dir.join(format!("{}.md", name));

        if dest.exists() {
            if overwrite {
                fs::remove_file(&dest).map_err(|e| format!("无法移除旧版本: {}", e))?;
            } else {
                return Err(format!(
                    "Skill '{}' 已存在于 {}",
                    name,
                    skills_dir.display()
                ));
            }
        }

        // Copy the file to skills directory
        fs::copy(source, &dest).map_err(|e| format!("复制文件失败: {}", e))?;

        Ok(format!("Skill '{}' 安装成功", name))
    }
}

/// Install a Codex/Qoder standalone skill from a local directory or .md file.
fn install_codex_style_standalone_skill(
    source: &Path,
    skills_dir: &Path,
    overwrite: bool,
) -> Result<String, String> {
    let name = if source.is_dir() {
        source
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_string()
    } else if source.extension().is_some_and(|e| e == "md") {
        // Single .md file: extract name, create directory structure
        source
            .file_stem()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_string()
    } else {
        return Err("不支持的文件类型。请选择 .md 文件或包含 SKILL.md 的目录".into());
    };

    validate_skill_name(&name)?;

    let dest_dir = skills_dir.join(&name);

    if dest_dir.exists() {
        if overwrite {
            fs::remove_dir_all(&dest_dir).map_err(|e| format!("无法移除旧版本: {}", e))?;
        } else {
            return Err(format!("Skill '{}' 已存在", name));
        }
    }

    fs::create_dir_all(&dest_dir).map_err(|e| format!("创建目录失败: {}", e))?;

    if source.is_dir() {
        // Validate SKILL.md exists before copying
        require_skill_md(source)?;
        // Copy all contents from source directory
        copy_dir_contents(source, &dest_dir)?;
    } else {
        // Single .md file — copy as SKILL.md
        let dest_file = dest_dir.join("SKILL.md");
        fs::copy(source, &dest_file).map_err(|e| format!("复制文件失败: {}", e))?;
    }

    Ok(format!("Skill '{}' 安装成功", name))
}

/// Validate that a directory contains SKILL.md. Returns an error if it does not.
fn require_skill_md(dir: &Path) -> Result<(), String> {
    let skill_md = dir.join("SKILL.md");
    if !skill_md.exists() {
        return Err(format!("目录 '{}' 不包含 SKILL.md 文件", dir.display()));
    }
    if !skill_md.is_file() {
        return Err("SKILL.md 不是一个有效的文件".into());
    }
    Ok(())
}

/// Recursively copy directory contents.
/// Symlinks are skipped to prevent path-traversal and sensitive-file disclosure attacks.
fn copy_dir_contents(src: &Path, dst: &Path) -> Result<(), String> {
    for entry in fs::read_dir(src).map_err(|e| format!("读取目录失败: {}", e))? {
        let entry = entry.map_err(|e| format!("读取条目失败: {}", e))?;
        let path = entry.path();

        // Reject symlinks — they could point outside the source directory
        if path.is_symlink() {
            continue;
        }

        let dest_path = dst.join(entry.file_name());

        if path.is_dir() {
            fs::create_dir_all(&dest_path).map_err(|e| format!("创建目录失败: {}", e))?;
            copy_dir_contents(&path, &dest_path)?;
        } else {
            fs::copy(&path, &dest_path).map_err(|e| format!("复制文件失败: {}", e))?;
        }
    }
    Ok(())
}

#[tauri::command]
pub fn install_standalone_skill(
    cli: String,
    source_path: String,
    overwrite: Option<bool>,
    project_path: Option<String>,
) -> Result<String, String> {
    let src = Path::new(&source_path);
    if !src.exists() {
        return Err(format!("文件不存在: {}", source_path));
    }
    let overwrite = overwrite.unwrap_or(false);

    match cli.as_str() {
        "claude" => {
            let skills_dir = if let Some(ref proj) = project_path {
                PathBuf::from(proj).join(".claude").join("skills")
            } else {
                claude_skills_dir().ok_or("无法找到 Claude skills 目录")?
            };
            install_claude_standalone_skill_to(src, &skills_dir, overwrite)
        }
        "codex" => {
            let skills_dir = if let Some(ref proj) = project_path {
                PathBuf::from(proj).join(".codex").join("skills")
            } else {
                codex_skills_dir().ok_or("无法找到 Codex skills 目录")?
            };
            install_codex_style_standalone_skill(src, &skills_dir, overwrite)
        }
        "qoder" => {
            // Qoder: project-level uses .qoder (NOT .qoder-cn)
            let skills_dir = if let Some(ref proj) = project_path {
                PathBuf::from(proj).join(".qoder").join("skills")
            } else {
                qoder_skills_dir().ok_or("无法找到 Qoder skills 目录")?
            };
            install_codex_style_standalone_skill(src, &skills_dir, overwrite)
        }
        _ => Err(format!("不支持的 CLI: {}", cli)),
    }
}

#[tauri::command]
pub fn uninstall_standalone_skill(
    cli: String,
    skill_id: String,
    skill_path: Option<String>,
    skill_name: Option<String>,
) -> Result<String, String> {
    // If path is provided, delete directly (handles both user and project level)
    if let Some(ref p) = skill_path {
        let file_path = std::path::Path::new(p);
        if !file_path.exists() {
            let fallback_name = skill_name.as_deref().unwrap_or("unknown");
            return Err(format!("Skill '{}' 不存在", fallback_name));
        }
        let name = skill_name.as_deref().unwrap_or(
            file_path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("unknown"),
        );
        if file_path.is_symlink() || file_path.is_file() {
            fs::remove_file(file_path).map_err(|e| format!("删除失败: {}", e))?;
        } else if file_path.is_dir() {
            fs::remove_dir_all(file_path).map_err(|e| format!("删除失败: {}", e))?;
        }
        return Ok(format!("Skill '{}' 已删除", name));
    }
    // Fallback: ID-based lookup in user-level directory
    let name = strip_id_prefix(&skill_id);
    validate_skill_name(name)?;
    match cli.as_str() {
        "claude" => {
            let skills_dir = claude_skills_dir().ok_or("无法找到 Claude skills 目录")?;
            let active = skills_dir.join(format!("{}.md", name));
            let disabled = skills_dir.join(format!("{}.md.disabled", name));
            let skill_md_dir = skills_dir.join(name);

            if active.exists() || active.is_symlink() {
                fs::remove_file(&active).map_err(|e| format!("删除失败: {}", e))?;
                Ok(format!("Skill '{}' 已删除", name))
            } else if disabled.exists() || disabled.is_symlink() {
                fs::remove_file(&disabled).map_err(|e| format!("删除失败: {}", e))?;
                Ok(format!("Skill '{}'（已禁用）已删除", name))
            } else if skill_md_dir.exists() && skill_md_dir.is_dir() {
                fs::remove_dir_all(&skill_md_dir).map_err(|e| format!("删除失败: {}", e))?;
                Ok(format!("Skill '{}'（目录）已删除", name))
            } else {
                Err(format!("Skill '{}' 不存在", name))
            }
        }
        "codex" => {
            let skills_dir = codex_skills_dir().ok_or("无法找到 Codex skills 目录")?;
            let skill_dir = skills_dir.join(name);
            if !skill_dir.exists() && !skill_dir.is_symlink() {
                return Err(format!("Skill '{}' 不存在", name));
            }
            if skill_dir.is_symlink() || skill_dir.is_file() {
                fs::remove_file(&skill_dir).map_err(|e| format!("删除失败: {}", e))?;
            } else {
                fs::remove_dir_all(&skill_dir).map_err(|e| format!("删除失败: {}", e))?;
            }
            Ok(format!("Skill '{}' 已删除", name))
        }
        "qoder" => {
            let skills_dir = qoder_skills_dir().ok_or("无法找到 Qoder skills 目录")?;
            let skill_dir = skills_dir.join(name);
            if !skill_dir.exists() && !skill_dir.is_symlink() {
                return Err(format!("Skill '{}' 不存在", name));
            }
            if skill_dir.is_symlink() || skill_dir.is_file() {
                fs::remove_file(&skill_dir).map_err(|e| format!("删除失败: {}", e))?;
            } else {
                fs::remove_dir_all(&skill_dir).map_err(|e| format!("删除失败: {}", e))?;
            }
            Ok(format!("Skill '{}' 已删除", name))
        }
        _ => Err(format!("不支持的 CLI: {}", cli)),
    }
}

// ═══════════════════════════════════════════════════════════════
//  TAURI COMMANDS
// ═══════════════════════════════════════════════════════════════

#[tauri::command]
pub fn scan_skills(project_path: Option<String>) -> SkillManagerData {
    let project_root = project_path.as_ref().and_then(|p| {
        let path = Path::new(p);
        if path.exists() && path.is_dir() {
            Some(path)
        } else {
            None
        }
    });
    scan_all(project_root)
}

/// Strip the internal `cli:type:` prefix from an asset ID to get the raw name.
/// Handles both old format `"cli:type:name"` and new format `"cli:type:hash:name"`.
/// E.g. `"claude:plugin:superpowers@superpowers-marketplace"` → `"superpowers@superpowers-marketplace"`
///      `"claude:project-skill:a3f2b1c0:hello"` → `"hello"`
fn strip_id_prefix(id: &str) -> &str {
    // Take the last colon-delimited segment — the name is always the filename
    // (without extension), and filenames can't contain ':' on any platform.
    id.rsplit(':').next().unwrap_or(id)
}

#[tauri::command]
pub fn toggle_plugin(
    cli: String,
    plugin_id: String,
    enabled: bool,
    project_path: Option<String>,
) -> Result<(), String> {
    let name = strip_id_prefix(&plugin_id);
    match cli.as_str() {
        "claude" => {
            if let Some(ref project_path) = project_path {
                toggle_claude_project_plugin(name, enabled, project_path)
            } else {
                toggle_claude_plugin(name, enabled)
            }
        }
        "codex" => {
            if let Some(ref project_path) = project_path {
                toggle_codex_project_plugin(name, enabled, project_path)
            } else {
                toggle_codex_plugin(name, enabled)
            }
        }
        _ => Err(format!("不支持的 CLI: {}", cli)),
    }
}

#[tauri::command]
pub fn toggle_standalone_skill(
    cli: String,
    skill_id: String,
    enabled: bool,
    path: Option<String>,
) -> Result<(), String> {
    // If path is provided, use path-based toggle (works for both user and project level)
    if let Some(ref p) = path {
        return toggle_resource_by_path(p, enabled);
    }
    // Fallback: name-based lookup in user-level directory
    let name = strip_id_prefix(&skill_id);
    match cli.as_str() {
        "claude" => toggle_claude_standalone_skill(name, enabled),
        "codex" => toggle_codex_standalone_skill(name, enabled),
        "qoder" => toggle_qoder_standalone_skill(name, enabled),
        _ => Err(format!("不支持的 CLI: {}", cli)),
    }
}

/// Generic toggle by file path — handles files (.md, .toml) and directories (Codex/Qoder skills).
///
/// For files: `name.ext` ↔ `name.ext.disabled`
/// For directories: `SKILL.md` ↔ `SKILL.md.disabled` inside the dir
/// Generic toggle by file path — handles files (.md, .toml) and directories (Codex/Qoder skills).
///
/// For files: `name.ext` ↔ `name.ext.disabled`
/// For directories: `SKILL.md` ↔ `SKILL.md.disabled` inside the dir
pub(crate) fn toggle_resource_by_path(path: &str, enabled: bool) -> Result<(), String> {
    let p = std::path::Path::new(path);
    if p.is_dir() {
        // Codex/Qoder skill directory — toggle SKILL.md inside
        let active = p.join("SKILL.md");
        let disabled = p.join("SKILL.md.disabled");
        if enabled {
            if disabled.exists() {
                fs::rename(&disabled, &active).map_err(|e| format!("启用失败: {}", e))?;
            } else if !active.exists() {
                return Err(format!("Skill 不存在: {}", p.display()));
            }
        } else {
            if active.exists() {
                fs::rename(&active, &disabled).map_err(|e| format!("禁用失败: {}", e))?;
            } else if !disabled.exists() {
                return Err(format!("Skill 不存在: {}", p.display()));
            }
        }
    } else {
        // File-based (.md or .toml) — add/remove .disabled suffix
        if !p.exists() {
            return Err(format!("文件不存在: {}", path));
        }
        let path_str = p.to_string_lossy();
        if enabled {
            let new_str = path_str
                .strip_suffix(".disabled")
                .ok_or("资源未处于禁用状态，无法启用")?;
            let new_path = std::path::Path::new(new_str);
            if new_path.exists() {
                return Err("目标文件已存在".into());
            }
            fs::rename(p, new_path).map_err(|e| format!("启用失败: {}", e))?;
        } else {
            let new_str = format!("{}.disabled", path_str);
            let new_path = std::path::Path::new(&new_str);
            if new_path.exists() {
                return Err("禁用文件已存在".into());
            }
            fs::rename(p, new_path).map_err(|e| format!("禁用失败: {}", e))?;
        }
    }
    Ok(())
}

// ═══════════════════════════════════════════════════════════════
//  COMMAND TOGGLE / UNINSTALL
// ═══════════════════════════════════════════════════════════════

/// Toggle a Claude command: rename `xxx.md` ↔ `xxx.md.disabled`.
#[tauri::command]
pub fn toggle_command(
    cli: String,
    name: String,
    enabled: bool,
    path: Option<String>,
) -> Result<(), String> {
    // If path is provided, use path-based toggle (works for both user and project level)
    if let Some(ref p) = path {
        return toggle_resource_by_path(p, enabled);
    }
    // Fallback: name-based lookup in user-level directory
    validate_skill_name(&name)?;
    let commands_dir = match cli.as_str() {
        "claude" => claude_commands_dir().ok_or("无法找到 Claude commands 目录")?,
        _ => return Err(format!("不支持的 CLI: {}", cli)),
    };

    let active_path = commands_dir.join(format!("{}.md", name));
    let disabled_path = commands_dir.join(format!("{}.md.disabled", name));

    if enabled {
        if disabled_path.exists() {
            fs::rename(&disabled_path, &active_path).map_err(|e| format!("重命名失败: {}", e))?;
        } else if !active_path.exists() {
            return Err(format!("Command '{}' 不存在", name));
        }
    } else {
        if active_path.exists() {
            fs::rename(&active_path, &disabled_path).map_err(|e| format!("重命名失败: {}", e))?;
        } else if !disabled_path.exists() {
            return Err(format!("Command '{}' 不存在", name));
        }
    }

    Ok(())
}

/// Uninstall a command: delete the file or directory directly.
/// When `path` is provided, deletes the file at that path directly (works for both user and project level).
#[tauri::command]
pub fn uninstall_command(
    cli: String,
    name: String,
    path: Option<String>,
) -> Result<String, String> {
    // If path is provided, delete directly (handles both user and project level)
    if let Some(ref p) = path {
        let file_path = std::path::Path::new(p);
        if !file_path.exists() {
            return Err(format!("Command '{}' 不存在", name));
        }
        fs::remove_file(file_path).map_err(|e| format!("删除失败: {}", e))?;
        return Ok(format!("Command '{}' 已删除", name));
    }
    // Fallback: name-based lookup in user-level directory
    validate_skill_name(&name)?;
    let commands_dir = match cli.as_str() {
        "claude" => claude_commands_dir().ok_or("无法找到 Claude commands 目录")?,
        _ => return Err(format!("不支持的 CLI: {}", cli)),
    };

    let active = commands_dir.join(format!("{}.md", name));
    let disabled = commands_dir.join(format!("{}.md.disabled", name));

    if active.exists() {
        fs::remove_file(&active).map_err(|e| format!("删除失败: {}", e))?;
        Ok(format!("Command '{}' 已删除", name))
    } else if disabled.exists() {
        fs::remove_file(&disabled).map_err(|e| format!("删除失败: {}", e))?;
        Ok(format!("Command '{}'（已禁用）已删除", name))
    } else {
        Err(format!("Command '{}' 不存在", name))
    }
}

#[tauri::command]
pub fn check_updates(app_handle: tauri::AppHandle, state: tauri::State<Mutex<CancelState>>) {
    let cancel = {
        let cs = state.lock().unwrap();
        cs.cancelled.clone()
    };
    cancel.store(false, Ordering::SeqCst);

    // Run in background thread — returns immediately, results stream via events
    std::thread::spawn(move || {
        let results = check_claude_updates(cancel);
        let _ = app_handle.emit("update-check-complete", results);
    });
}

#[tauri::command]
pub fn cancel_check_updates(state: tauri::State<Mutex<CancelState>>) {
    if let Ok(cs) = state.lock() {
        cs.cancelled.store(true, Ordering::SeqCst);
    }
}

#[tauri::command]
pub fn update_plugin(
    app_handle: tauri::AppHandle,
    cli: String,
    plugin_id: String,
) -> Result<(), String> {
    match cli.as_str() {
        "claude" => {
            let id = plugin_id.clone();
            std::thread::spawn(move || {
                let result = exec_claude_plugin_update(&id);
                let _ = app_handle.emit(
                    "update-plugin-complete",
                    serde_json::json!({
                        "pluginId": id,
                        "success": result.is_ok(),
                        "message": result.unwrap_or_else(|e| e),
                    }),
                );
            });
            Ok(())
        }
        _ => Err(format!("不支持的 CLI: {}", cli)),
    }
}

#[tauri::command]
pub fn read_skill_content(path: String) -> Result<SkillContent, String> {
    content::read_skill_content(path)
}

#[tauri::command]
pub fn move_skill_file(
    source_path: String,
    dest_dir: String,
    resource_name: String,
    resource_type: String,
    from_scope: String,
    to_scope: String,
    overwrite: Option<bool>,
) -> Result<MoveUndoInfo, String> {
    transfer::move_skill_file(
        source_path,
        dest_dir,
        resource_name,
        resource_type,
        from_scope,
        to_scope,
        overwrite.unwrap_or(false),
    )
}

#[tauri::command]
pub fn copy_skill_file(
    source_path: String,
    dest_dir: String,
    resource_name: String,
    overwrite: Option<bool>,
) -> Result<(), String> {
    transfer::copy_skill_file(
        source_path,
        dest_dir,
        resource_name,
        overwrite.unwrap_or(false),
    )
}

#[tauri::command]
pub fn undo_move_skill(
    backup_path: String,
    original_path: String,
    dest_path: String,
    content_fingerprint: String,
) -> Result<(), String> {
    transfer::undo_move_skill(backup_path, original_path, dest_path, content_fingerprint)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codex_project_plugin_config_write_and_remove_use_full_plugin_id() {
        let temp = tempfile::tempdir().expect("create temp project");
        let project = temp.path().to_string_lossy().to_string();

        write_codex_project_plugin_enabled(&project, "browser", "openai-bundled", true)
            .expect("write project plugin");

        let config_path = temp.path().join(".codex").join("config.toml");
        let written = std::fs::read_to_string(&config_path).expect("read config");
        assert!(written.contains("[plugins.\"browser@openai-bundled\"]"));
        assert!(written.contains("enabled = true"));

        remove_codex_project_plugin(&project, "browser@openai-bundled")
            .expect("remove project plugin");

        let removed = std::fs::read_to_string(&config_path).expect("read config after remove");
        assert!(!removed.contains("browser@openai-bundled"));
    }

    #[test]
    fn codex_project_plugin_config_rejects_missing_project_dir() {
        let temp = tempfile::tempdir().expect("create temp parent");
        let missing = temp.path().join("missing").to_string_lossy().to_string();

        let err = write_codex_project_plugin_enabled(&missing, "browser", "openai-bundled", true)
            .expect_err("missing project should fail");

        assert!(err.contains("项目目录不存在"));
    }

    #[test]
    fn command_runner_times_out_and_returns_error() {
        let err = run_command_with_timeout(
            "sh",
            "/bin/sh",
            &["-c", "sleep 2"],
            None,
            Duration::from_millis(50),
        )
        .expect_err("sleep should time out");

        assert!(err.contains("执行超时"));
    }

    #[test]
    fn project_codex_plugin_toggle_updates_project_config() {
        let temp = tempfile::tempdir().expect("create temp project");
        let project = temp.path().to_string_lossy().to_string();
        write_codex_project_plugin_enabled(&project, "browser", "openai-bundled", true)
            .expect("write project plugin");

        toggle_codex_project_plugin("browser@openai-bundled", false, &project)
            .expect("disable project plugin");

        let config_path = temp.path().join(".codex").join("config.toml");
        let text = std::fs::read_to_string(&config_path).expect("read config");
        let config: toml::Value = toml::from_str(&text).expect("parse config");
        let enabled = config
            .get("plugins")
            .and_then(|v| v.get("browser@openai-bundled"))
            .and_then(|v| v.get("enabled"))
            .and_then(|v| v.as_bool());
        assert_eq!(enabled, Some(false));
    }

    #[test]
    fn project_claude_plugin_toggle_keeps_disabled_entry_visible() {
        let temp = tempfile::tempdir().expect("create temp project");
        let project = temp.path().to_string_lossy().to_string();
        let claude_dir = temp.path().join(".claude");
        std::fs::create_dir_all(&claude_dir).expect("create claude dir");
        std::fs::write(
            claude_dir.join("settings.json"),
            r#"{"enabledPlugins":{"superpowers@openai-curated":true}}"#,
        )
        .expect("write settings");

        toggle_claude_project_plugin("superpowers@openai-curated", false, &project)
            .expect("disable project plugin");

        let text = std::fs::read_to_string(claude_dir.join("settings.json")).expect("read settings");
        let root: serde_json::Value = serde_json::from_str(&text).expect("parse settings");
        let enabled = root
            .get("enabledPlugins")
            .and_then(|v| v.get("superpowers@openai-curated"))
            .and_then(|v| v.as_bool());
        assert_eq!(enabled, Some(false));
    }

    #[test]
    fn marketplace_project_annotation_distinguishes_user_and_project_installs() {
        let temp = tempfile::tempdir().expect("create temp project");
        let project = temp.path().to_string_lossy().to_string();
        let mut plugins = vec![MarketplacePluginEntry {
            name: "browser".to_string(),
            marketplace: "openai-bundled".to_string(),
            cli: "codex".to_string(),
            version: None,
            description: None,
            installed: true,
            user_installed: Some(true),
            project_installed: None,
            skill_count: 0,
        }];

        annotate_project_marketplace_installs(&mut plugins, &project);

        assert_eq!(plugins[0].user_installed, Some(true));
        assert_eq!(plugins[0].project_installed, Some(false));
        assert!(!plugins[0].installed);

        write_codex_project_plugin_enabled(&project, "browser", "openai-bundled", true)
            .expect("write project plugin");
        annotate_project_marketplace_installs(&mut plugins, &project);

        assert_eq!(plugins[0].project_installed, Some(true));
        assert!(plugins[0].installed);
    }
}
