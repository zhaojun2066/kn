//! Skill & Plugin Manager — scan and toggle skills/plugins across Claude Code & Codex.
//!
//! # Data Model
//!
//! - **Plugin**: enable/disable unit. Contains zero or more skills (read-only children).
//! - **Standalone Skill**: individually toggleable skill not owned by any plugin.
//! - **System Skill**: built-in, read-only (Codex `.system/` directory).

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::sync::Mutex;
use tauri::Emitter;

use crate::agent_manager::AgentEntry;
use crate::commands::find_binary;

/// Resolve a CLI binary (claude, codex) to its full path.
/// Required because Tauri .app bundles have a minimal PATH that
/// doesn't include Homebrew or npm global install directories.
fn cli_binary(name: &str) -> String {
    find_binary(&[name]).unwrap_or_else(|| name.to_string())
}

// ── Types (mirrors frontend `SkillManager.tsx` types) ────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillEntry {
    pub name: String,
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginEntry {
    pub id: String,
    pub cli: String,
    pub name: String,
    pub marketplace: String,
    pub enabled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    pub source: String,
    pub skills: Vec<SkillEntry>,
    pub agents: Vec<AgentEntry>,
    pub commands: Vec<CommandEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StandaloneSkill {
    pub id: String,
    pub cli: String,
    pub name: String,
    pub enabled: bool,
    #[serde(rename = "linkType")]
    pub link_type: String,
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandEntry {
    pub id: String,
    pub cli: String,
    pub name: String,
    pub path: String,
    pub description: String,
    pub enabled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillManagerData {
    pub plugins: Vec<PluginEntry>,
    #[serde(rename = "standaloneSkills")]
    pub standalone_skills: Vec<StandaloneSkill>,
    #[serde(rename = "systemSkills")]
    pub system_skills: Vec<StandaloneSkill>,
    pub commands: Vec<CommandEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginUpdateInfo {
    pub plugin_id: String,
    pub current_version: String,
    pub current_sha: String,
    pub latest_sha: String,
    pub has_update: bool,
}

/// Shared cancel flag for aborting long-running update checks.
pub struct CancelState {
    pub cancelled: Arc<AtomicBool>,
}

// ── Marketplace types ──────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MarketplacePluginEntry {
    pub name: String,
    pub marketplace: String,
    pub cli: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub installed: bool,
    /// Number of skills this plugin contains (0 if unknown)
    pub skill_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MarketplaceData {
    pub plugins: Vec<MarketplacePluginEntry>,
    pub marketplaces: Vec<String>,
}

// ── Paths ──────────────────────────────────────────────────────

/// Thin wrapper around [`crate::home_dir`] that returns `None` instead of `"."` fallback.
/// Preserves Option semantics used by the skill scanning call chains.
fn home_dir() -> Option<PathBuf> {
    let h = crate::home_dir();
    if h.as_os_str() == "." { None } else { Some(h) }
}

fn claude_skills_dir() -> Option<PathBuf> {
    home_dir().map(|h| h.join(".claude").join("skills"))
}

fn claude_commands_dir() -> Option<PathBuf> {
    home_dir().map(|h| h.join(".claude").join("commands"))
}

fn claude_plugins_json() -> Option<PathBuf> {
    home_dir().map(|h| {
        h.join(".claude")
            .join("plugins")
            .join("installed_plugins.json")
    })
}

fn claude_settings_json() -> Option<PathBuf> {
    home_dir().map(|h| h.join(".claude").join("settings.json"))
}

fn codex_skills_dir() -> Option<PathBuf> {
    home_dir().map(|h| h.join(".codex").join("skills"))
}

fn codex_config_toml() -> Option<PathBuf> {
    home_dir().map(|h| h.join(".codex").join("config.toml"))
}

fn codex_plugins_dir() -> Option<PathBuf> {
    home_dir().map(|h| h.join(".codex").join("plugins"))
}

fn qoder_skills_dir() -> Option<PathBuf> {
    home_dir().map(|h| h.join(".qoder-cn").join("skills"))
}

fn claude_known_marketplaces_json() -> Option<PathBuf> {
    home_dir().map(|h| {
        h.join(".claude")
            .join("plugins")
            .join("known_marketplaces.json")
    })
}

// ── Symlink Resolution ─────────────────────────────────────────

/// Resolve a symlink to its target path. Returns the original path if not a symlink.
fn resolve_symlink(path: &Path) -> PathBuf {
    match fs::read_link(path) {
        Ok(target) => {
            if target.is_absolute() {
                target
            } else {
                // Relative symlink: resolve relative to the symlink's parent directory
                path.parent().unwrap_or(Path::new(".")).join(target)
            }
        }
        Err(_) => path.to_path_buf(),
    }
}

/// Extract `description` from YAML frontmatter of a markdown file.
/// Looks for `description:` or `description: >` between `---` delimiters.
/// Handles both Unix (`\n`) and Windows (`\r\n`) line endings.
fn extract_description(md_path: &Path) -> Option<String> {
    let raw = fs::read_to_string(md_path).ok()?;
    // Normalize Windows CRLF → LF for consistent parsing
    let text = raw.replace("\r\n", "\n");
    let content = text.trim_start();
    // Frontmatter must start with ---
    if !content.starts_with("---") {
        return None;
    }
    // Find the closing ---
    let after_first = &content[3..];
    let end = after_first.find("\n---")?;
    let frontmatter = &after_first[..end];

    // Look for description line
    for line in frontmatter.lines() {
        let trimmed = line.trim();
        if let Some(val) = trimmed.strip_prefix("description:") {
            let desc = val.trim().trim_matches('"').trim_matches('\'');
            if !desc.is_empty() {
                return Some(desc.to_string());
            }
        }
    }
    None
}

/// Read description from a Claude skill (.md file) or Codex skill (dir/SKILL.md).
#[allow(dead_code)]
fn read_skill_description(skill_path: &Path) -> Option<String> {
    if skill_path.is_dir() {
        extract_description(&skill_path.join("SKILL.md"))
    } else if skill_path.extension().map_or(false, |e| e == "md") {
        extract_description(skill_path)
    } else {
        None
    }
}

// ── Entry type detection ───────────────────────────────────────

fn classify_entry(path: &Path) -> &'static str {
    if path.is_symlink() {
        "symlink"
    } else if path.is_dir() {
        "directory"
    } else {
        "file"
    }
}

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
}

#[derive(Debug, Deserialize, Default)]
struct CodexPluginConfig {
    enabled: Option<bool>,
}

/// Parse Codex config.toml to get plugin enabled states.
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
            states.insert(full_name, cfg.enabled.unwrap_or(false));
        }
    }
    states
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
    }

    // Source 2: Marketplace plugins (bundled + runtime)
    if let Some(home) = home_dir() {
        // Bundled marketplace
        let bundled = home
            .join(".codex")
            .join(".tmp")
            .join("bundled-marketplaces");
        scan_codex_marketplace_dir(&bundled, &plugin_states, "bundled", &mut plugins);

        // Runtime marketplace
        let runtime = home
            .join(".cache")
            .join("codex-runtimes")
            .join("codex-primary-runtime")
            .join("plugins");
        scan_codex_marketplace_dir(&runtime, &plugin_states, "bundled", &mut plugins);
    }

    plugins
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
    // Walk marketplace dirs: root/marketplace-name/plugins/plugin-name/
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
        let plugins_dir = mkt_path.join("plugins");
        if !plugins_dir.exists() {
            continue;
        }
        if let Ok(plugin_entries) = fs::read_dir(&plugins_dir) {
            for p in plugin_entries.flatten() {
                let plugin_path = p.path();
                if !plugin_path.is_dir() {
                    continue;
                }
                let source = format!("{}@{}", default_source, mkt_name);
                if let Some(plugin) = read_codex_plugin_manifest(&plugin_path, states, &source) {
                    // Avoid duplicates
                    if !plugins.iter().any(|existing| existing.id == plugin.id) {
                        plugins.push(plugin);
                    }
                }
            }
        }
    }
}

fn read_codex_plugin_manifest(
    path: &Path,
    states: &HashMap<String, bool>,
    source: &str,
) -> Option<PluginEntry> {
    let manifest_path = path.join(".codex-plugin").join("plugin.json");
    let text = fs::read_to_string(&manifest_path).ok()?;
    let manifest: serde_json::Value = serde_json::from_str(&text).ok()?;

    let name = manifest
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");
    let version = manifest
        .get("version")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let marketplace = source.split('@').last().unwrap_or(source);

    let full_id = format!("{}@{}", name, marketplace);
    let enabled = states.get(&full_id).copied().unwrap_or(false);

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
                if resolved.is_file() && resolved.extension().map_or(false, |e| e == "md") {
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
    scan_standalone_skills_in_dir("claude", &skills_dir, plugin_skill_names, pn, Some(project_root))
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
            format!("{}:project-skill:{}:{}", cli, crate::hash_path(&root.to_string_lossy()), name)
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
            format!("{}:project-skill:{}:{}", cli, crate::hash_path(&root.to_string_lossy()), file_name)
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
fn enumerate_commands_in_dir(cli: &str, commands_dir: &Path, project_name: Option<String>, project_root: Option<&Path>) -> Vec<CommandEntry> {
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
            format!("{}:project-command:{}:{}", cli, crate::hash_path(&root.to_string_lossy()), name)
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
        // Project skills
        let project_claude_skills =
            scan_claude_project_skills(root, &claude_plugin_skill_names);
        standalone.extend(project_claude_skills);

        let project_codex_skills = scan_codex_project_skills(root);
        standalone.extend(project_codex_skills);

        let project_qoder_skills = scan_qoder_project_skills(root);
        standalone.extend(project_qoder_skills);

        // Project commands
        let project_claude_commands = scan_claude_project_commands(root);
        claude_commands.extend(project_claude_commands);
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
pub fn toggle_codex_plugin(plugin_id: &str, enabled: bool) -> Result<(), String> {
    let path = codex_config_toml().ok_or("无法找到 Codex config.toml")?;
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
            .map_or(false, |latest| is_version_greater(latest, current_version));

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
            let source = p
                .get("source")
                .and_then(|v| v.as_str())
                .unwrap_or("");
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
                                let n =
                                    path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                                !n.starts_with('.')
                                    && (n.ends_with(".md")
                                        || (path.is_dir()
                                            && path.join("SKILL.md").exists()))
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
        let search_dirs = vec![
            (
                home.join(".codex")
                    .join(".tmp")
                    .join("bundled-marketplaces"),
                "bundled",
            ),
            (
                home.join(".cache")
                    .join("codex-runtimes")
                    .join("codex-primary-runtime")
                    .join("plugins"),
                "bundled",
            ),
        ];

        for (root, _source) in &search_dirs {
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
                let mkt_name = mkt_path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("unknown");
                let plugins_dir = mkt_path.join("plugins");
                if !plugins_dir.exists() {
                    continue;
                }
                if let Ok(plugin_entries) = fs::read_dir(&plugins_dir) {
                    for p in plugin_entries.flatten() {
                        let plugin_path = p.path();
                        if !plugin_path.is_dir() {
                            continue;
                        }
                        let manifest_path = plugin_path.join(".codex-plugin").join("plugin.json");
                        let text = match fs::read_to_string(&manifest_path) {
                            Ok(t) => t,
                            Err(_) => continue,
                        };
                        let manifest: serde_json::Value = match serde_json::from_str(&text) {
                            Ok(v) => v,
                            Err(_) => continue,
                        };

                        let name = manifest
                            .get("name")
                            .and_then(|v| v.as_str())
                            .unwrap_or("unknown")
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
                                                && !e
                                                    .file_name()
                                                    .to_str()
                                                    .map_or(true, |n| n.starts_with('.'))
                                                && e.path().join("SKILL.md").exists()
                                        })
                                        .count()
                                })
                                .unwrap_or(0)
                        } else {
                            0
                        };

                        let full_id = format!("{}@{}", name, mkt_name);
                        let installed = installed_states.get(&full_id).copied().unwrap_or(false);

                        // Avoid duplicates
                        if !plugins.iter().any(|existing: &MarketplacePluginEntry| {
                            existing.name == name && existing.marketplace == mkt_name
                        }) {
                            plugins.push(MarketplacePluginEntry {
                                name,
                                marketplace: mkt_name.to_string(),
                                cli: "codex".into(),
                                version,
                                description,
                                installed,
                                skill_count,
                            });
                        }
                    }
                }
            }
        }
    }

    plugins
}

#[tauri::command]
pub fn list_marketplace_plugins(cli: String) -> MarketplaceData {
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

    MarketplaceData {
        plugins,
        marketplaces,
    }
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
) -> Result<String, String> {
    let cli_clone = cli.clone();
    let n = name.clone();

    let result: Result<String, String> = match cli.as_str() {
        "claude" => {
            let full = format!("{}@{}", name, marketplace);
            let n = name;
            tauri::async_runtime::spawn_blocking(move || {
                run_cli_plugin_action(
                    "claude",
                    &["plugin", "install", &full, "--scope", "user"],
                    &format!("{} 安装成功", n),
                    "安装失败",
                )
            })
            .await
            .map_err(|e| format!("执行失败: {}", e))?
        }
        "codex" => {
            let full = format!("{}@{}", name, marketplace);
            let n = name;
            tauri::async_runtime::spawn_blocking(move || {
                run_cli_plugin_action(
                    "codex",
                    &["plugin", "add", &full],
                    &format!("{} 安装成功", n),
                    "安装失败",
                )
            })
            .await
            .map_err(|e| format!("执行失败: {}", e))?
        }
        _ => return Err(format!("不支持的 CLI: {}", cli)),
    };

    let _ = app_handle.emit(
        "plugin-install-complete",
        serde_json::json!({
            "name": n,
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
) -> Result<String, String> {
    let cli_clone = cli.clone();
    let name = strip_id_prefix(&plugin_id).to_string();

    let result: Result<String, String> = match cli.as_str() {
        "claude" => {
            let n = name.clone();
            tauri::async_runtime::spawn_blocking(move || {
                run_cli_plugin_action(
                    "claude",
                    &["plugin", "uninstall", &n, "-y"],
                    &format!("{} 删除成功", n),
                    "删除失败",
                )
            })
            .await
            .map_err(|e| format!("执行失败: {}", e))?
        }
        "codex" => {
            let n = name.clone();
            tauri::async_runtime::spawn_blocking(move || {
                run_cli_plugin_action(
                    "codex",
                    &["plugin", "remove", &n],
                    &format!("{} 删除成功", n),
                    "删除失败",
                )
            })
            .await
            .map_err(|e| format!("执行失败: {}", e))?
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
    success_msg: &str,
    error_prefix: &str,
) -> Result<String, String> {
    let binary = cli_binary(cli_name);
    let output = std::process::Command::new(&binary)
        .args(args)
        .output()
        .map_err(|e| format!("无法执行 {} ({}): {}", cli_name, binary, e))?;

    if output.status.success() {
        Ok(success_msg.to_string())
    } else {
        let err = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let out = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let msg = if err.is_empty() { out } else { err };
        Err(format!("{}: {}", error_prefix, msg))
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
fn install_claude_standalone_skill(source: &Path) -> Result<String, String> {
    let skills_dir = claude_skills_dir().ok_or("无法找到 Claude skills 目录")?;

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
            return Err(format!(
                "Skill '{}' 已存在于 {}",
                name,
                skills_dir.display()
            ));
        }

        fs::create_dir_all(&dest_dir)
            .map_err(|e| format!("创建目录失败: {}", e))?;
        copy_dir_contents(source, &dest_dir)?;

        Ok(format!("Skill '{}' 安装成功", name))
    } else {
        // Backward compat: single .md file mode
        let file_name = source.file_name().and_then(|n| n.to_str()).unwrap_or("");
        let name = file_name.strip_suffix(".md").unwrap_or(file_name);

        validate_skill_name(name)?;

        let dest = skills_dir.join(format!("{}.md", name));

        if dest.exists() {
            return Err(format!(
                "Skill '{}' 已存在于 {}",
                name,
                skills_dir.display()
            ));
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
) -> Result<String, String> {
    let name = if source.is_dir() {
        source
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_string()
    } else if source.extension().map_or(false, |e| e == "md") {
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
        return Err(format!("Skill '{}' 已存在", name));
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
        return Err(format!(
            "目录 '{}' 不包含 SKILL.md 文件",
            dir.display()
        ));
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
pub fn install_standalone_skill(cli: String, source_path: String) -> Result<String, String> {
    let src = Path::new(&source_path);
    if !src.exists() {
        return Err(format!("文件不存在: {}", source_path));
    }

    match cli.as_str() {
        "claude" => install_claude_standalone_skill(src),
        "codex" => {
            let skills_dir = codex_skills_dir().ok_or("无法找到 Codex skills 目录")?;
            install_codex_style_standalone_skill(src, &skills_dir)
        }
        "qoder" => {
            let skills_dir = qoder_skills_dir().ok_or("无法找到 Qoder skills 目录")?;
            install_codex_style_standalone_skill(src, &skills_dir)
        }
        _ => Err(format!("不支持的 CLI: {}", cli)),
    }
}

#[tauri::command]
pub fn uninstall_standalone_skill(cli: String, skill_id: String, skill_path: Option<String>, skill_name: Option<String>) -> Result<String, String> {
    // If path is provided, delete directly (handles both user and project level)
    if let Some(ref p) = skill_path {
        let file_path = std::path::Path::new(p);
        if !file_path.exists() {
            let fallback_name = skill_name.as_deref().unwrap_or("unknown");
            return Err(format!("Skill '{}' 不存在", fallback_name));
        }
        let name = skill_name.as_deref().unwrap_or(
            file_path.file_name().and_then(|n| n.to_str()).unwrap_or("unknown")
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
                fs::remove_dir_all(&skill_md_dir)
                    .map_err(|e| format!("删除失败: {}", e))?;
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
        if path.exists() && path.is_dir() { Some(path) } else { None }
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
pub fn toggle_plugin(cli: String, plugin_id: String, enabled: bool) -> Result<(), String> {
    let name = strip_id_prefix(&plugin_id);
    match cli.as_str() {
        "claude" => toggle_claude_plugin(name, enabled),
        "codex" => toggle_codex_plugin(name, enabled),
        _ => Err(format!("不支持的 CLI: {}", cli)),
    }
}

#[tauri::command]
pub fn toggle_standalone_skill(cli: String, skill_id: String, enabled: bool, path: Option<String>) -> Result<(), String> {
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
pub fn toggle_command(cli: String, name: String, enabled: bool, path: Option<String>) -> Result<(), String> {
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
            fs::rename(&disabled_path, &active_path)
                .map_err(|e| format!("重命名失败: {}", e))?;
        } else if !active_path.exists() {
            return Err(format!("Command '{}' 不存在", name));
        }
    } else {
        if active_path.exists() {
            fs::rename(&active_path, &disabled_path)
                .map_err(|e| format!("重命名失败: {}", e))?;
        } else if !disabled_path.exists() {
            return Err(format!("Command '{}' 不存在", name));
        }
    }

    Ok(())
}

/// Uninstall a command: delete the file or directory directly.
/// When `path` is provided, deletes the file at that path directly (works for both user and project level).
#[tauri::command]
pub fn uninstall_command(cli: String, name: String, path: Option<String>) -> Result<String, String> {
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

// ── Skill content reader ──

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillContent {
    pub description: String,
    pub body: String,
}

/// Read a skill file and extract its description + body content.
#[tauri::command]
pub fn read_skill_content(path: String) -> Result<SkillContent, String> {
    if path.is_empty() {
        return Ok(SkillContent {
            description: String::new(),
            body: String::new(),
        });
    }

    let file_path = std::path::Path::new(&path);
    let actual_path = if file_path.is_dir() {
        let md = file_path.join("SKILL.md");
        if md.exists() {
            md
        } else {
            // Check for disabled skill (SKILL.md.disabled) before falling back to skill.md
            let md_disabled = file_path.join("SKILL.md.disabled");
            if md_disabled.exists() {
                md_disabled
            } else {
                file_path.join("skill.md")
            }
        }
    } else {
        file_path.to_path_buf()
    };

    let content = std::fs::read_to_string(&actual_path).map_err(|e| format!("读取失败: {}", e))?;

    let mut lines = content.lines();
    let mut description = String::new();
    let mut body = String::new();
    let mut in_frontmatter = false;
    let mut frontmatter_done = false;
    let mut frontmatter_count = 0;

    for line in &mut lines {
        if !frontmatter_done {
            if line.trim() == "---" {
                frontmatter_count += 1;
                if frontmatter_count == 1 {
                    in_frontmatter = true;
                    continue;
                } else if frontmatter_count == 2 {
                    frontmatter_done = true;
                    continue;
                }
            }
            if in_frontmatter {
                if let Some(val) = line.strip_prefix("description:") {
                    description = val.trim().to_string();
                }
                continue;
            }
            if frontmatter_count == 0 {
                frontmatter_done = true;
                body.push_str(line);
                body.push('\n');
            }
        } else {
            body.push_str(line);
            body.push('\n');
        }
    }

    Ok(SkillContent {
        description,
        body: body.trim().to_string(),
    })
}

// ═══════════════════════════════════════════════════════════════
//  MOVE / COPY OPERATIONS
// ═══════════════════════════════════════════════════════════════

/// Info returned after a move so the frontend can undo it.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MoveUndoInfo {
    pub resource_name: String,
    pub resource_type: String, // "skill" | "agent" | "command"
    pub from_scope: String,    // "user" | "project"
    pub to_scope: String,
    pub backup_path: String,
    pub original_path: String,
    pub dest_path: String,
    /// Lightweight content fingerprint of the source file at move time.
    /// Used by undo to verify the destination hasn't been modified before deleting it.
    /// Format: "size:first_256_bytes_as_lossy_utf8"
    pub content_fingerprint: String,
}

/// Determine the file name (last component) from a path.
/// For directories, returns the directory name; for files, returns the file name.
fn file_name_from_path(path: &Path) -> Option<String> {
    path.file_name()
        .and_then(|n| n.to_str())
        .map(|s| s.to_string())
}

/// Compute a lightweight content fingerprint for a file or directory.
/// For files: first 256 bytes as lossy UTF-8 + file size.
/// For directories: entry count + total size.
fn compute_content_fingerprint(path: &Path) -> String {
    if path.is_dir() {
        match std::fs::read_dir(path) {
            Ok(entries) => {
                let count = entries.count();
                format!("dir:{}", count)
            }
            Err(_) => "dir:err".to_string(),
        }
    } else {
        let size = path.metadata().map(|m| m.len()).unwrap_or(0);
        match std::fs::read(path) {
            Ok(bytes) => {
                let preview = String::from_utf8_lossy(&bytes[..bytes.len().min(256)]);
                format!("{}:{}", size, preview)
            }
            Err(_) => format!("{}:read_err", size),
        }
    }
}

/// Check whether a path is inside a project directory (has `.claude/` structure).
#[allow(dead_code)]
fn is_project_scope(path: &str) -> bool {
    // Project-level resources have ":project-" in their ID
    // But for paths, we check if the path contains "/.claude/" or "/.codex/" or
    // "/.qoder-cn/" (user) or "/.qoder/" (project, for agents/skills)
    path.contains("/.claude/") || path.contains("/.codex/") || path.contains("/.qoder-cn/") || path.contains("/.qoder/")
}

/// Move a file-based resource (skill, agent, command) from source to destination directory.
///
/// Creates a backup (.bak) at the source location for undo support.
/// Returns undo info so the frontend can reverse the operation.
#[tauri::command]
pub fn move_skill_file(
    source_path: String,
    dest_dir: String,
    resource_name: String,
    resource_type: String,
    from_scope: String,
    to_scope: String,
) -> Result<MoveUndoInfo, String> {
    let src = Path::new(&source_path);
    if !src.exists() {
        return Err(format!("源文件不存在: {}", source_path));
    }

    let dest_dir_path = Path::new(&dest_dir);
    // Ensure destination directory exists
    fs::create_dir_all(dest_dir_path)
        .map_err(|e| format!("无法创建目标目录: {}", e))?;

    let file_name = file_name_from_path(src)
        .ok_or_else(|| format!("无法解析文件名: {}", source_path))?;

    // Determine dest path based on resource type
    let dest_path = if src.is_dir() {
        // Codex/Qoder skills are directories
        dest_dir_path.join(&file_name)
    } else if src.extension().map_or(false, |e| e == "md") {
        // Claude skills/agents/commands are .md files
        dest_dir_path.join(format!("{}.md", file_name.trim_end_matches(".md")))
    } else {
        // .toml files (Codex agents) or other
        dest_dir_path.join(&file_name)
    };

    // Check for conflicts
    if dest_path.exists() {
        return Err(format!("目标已存在同名资源: {}", file_name));
    }

    // Compute fingerprint BEFORE any mutation (for undo verification)
    let content_fingerprint = compute_content_fingerprint(src);

    // Copy to destination
    if src.is_dir() {
        copy_dir_recursive(src, &dest_path)?;
    } else {
        fs::copy(src, &dest_path)
            .map_err(|e| format!("复制文件失败: {}", e))?;
    }

    // Build timestamped backup path to avoid overwriting previous .bak files
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let backup_path = if src.is_dir() {
        let mut bak_name = src
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
            .to_string();
        bak_name.push_str(&format!(".bak.{}", ts));
        src.parent().unwrap_or(Path::new(".")).join(&bak_name)
    } else {
        let stem = src.file_stem()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown");
        let ext = src.extension()
            .and_then(|n| n.to_str())
            .map(|e| format!(".{}", e))
            .unwrap_or_default();
        let bak_name = format!("{}{}.bak.{}", stem, ext, ts);
        src.parent().unwrap_or(Path::new(".")).join(&bak_name)
    };

    // Rename source to backup (soft delete)
    if let Err(e) = fs::rename(src, &backup_path) {
        // Transactional rollback: delete the copy we just created at destination
        let _ = if dest_path.is_dir() {
            fs::remove_dir_all(&dest_path)
        } else {
            fs::remove_file(&dest_path)
        };
        return Err(format!("备份源文件失败: {}", e));
    }

    Ok(MoveUndoInfo {
        resource_name,
        resource_type,
        from_scope,
        to_scope,
        backup_path: backup_path.to_string_lossy().to_string(),
        original_path: source_path,
        dest_path: dest_path.to_string_lossy().to_string(),
        content_fingerprint,
    })
}

/// Copy a file-based resource without deleting the source.
#[tauri::command]
pub fn copy_skill_file(
    source_path: String,
    dest_dir: String,
    resource_name: String,
) -> Result<(), String> {
    let src = Path::new(&source_path);
    if !src.exists() {
        return Err(format!("源文件不存在: {}", source_path));
    }

    let dest_dir_path = Path::new(&dest_dir);
    fs::create_dir_all(dest_dir_path)
        .map_err(|e| format!("无法创建目标目录: {}", e))?;

    let file_name = file_name_from_path(src)
        .ok_or_else(|| format!("无法解析文件名: {}", source_path))?;

    let dest_path = if src.is_dir() {
        dest_dir_path.join(&file_name)
    } else if src.extension().map_or(false, |e| e == "md") {
        dest_dir_path.join(format!("{}.md", file_name.trim_end_matches(".md")))
    } else {
        dest_dir_path.join(&file_name)
    };

    if dest_path.exists() {
        return Err(format!("目标已存在同名资源: {}", resource_name));
    }

    if src.is_dir() {
        copy_dir_recursive(src, &dest_path)?;
    } else {
        fs::copy(src, &dest_path)
            .map_err(|e| format!("复制文件失败: {}", e))?;
    }

    Ok(())
}

/// Undo a move operation: delete the destination and restore the backup to original location.
///
/// Verifies the destination content hasn't been modified since the move (using a lightweight
/// fingerprint) before deleting, to avoid destroying user changes.
#[tauri::command]
pub fn undo_move_skill(backup_path: String, original_path: String, dest_path: String, content_fingerprint: String) -> Result<(), String> {
    let bak = Path::new(&backup_path);
    let orig = Path::new(&original_path);
    let dest = Path::new(&dest_path);

    // Verify destination content matches the fingerprint saved at move time.
    // If the user modified the destination after the move, refuse to delete it.
    if dest.exists() {
        let current_fp = compute_content_fingerprint(dest);
        if current_fp != content_fingerprint {
            return Err("目标文件已被修改，撤销取消。请手动处理".into());
        }
        if dest.is_dir() {
            fs::remove_dir_all(dest).map_err(|e| format!("删除目标失败: {}", e))?;
        } else {
            fs::remove_file(dest).map_err(|e| format!("删除目标失败: {}", e))?;
        }
    }

    // Restore backup to original location
    if bak.exists() {
        fs::rename(bak, orig).map_err(|e| format!("恢复备份失败: {}", e))?;
    } else {
        return Err("备份文件不存在，无法撤销".into());
    }

    Ok(())
}

/// Recursively copy a directory (used for Codex/Qoder skill directories).
fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<(), String> {
    fs::create_dir_all(dst).map_err(|e| format!("创建目录失败: {}", e))?;
    for entry in fs::read_dir(src).map_err(|e| format!("读取目录失败: {}", e))? {
        let entry = entry.map_err(|e| format!("读取条目失败: {}", e))?;
        let path = entry.path();
        if path.is_symlink() {
            continue; // Skip symlinks for safety
        }
        let dest_path = dst.join(entry.file_name());
        if path.is_dir() {
            copy_dir_recursive(&path, &dest_path)?;
        } else {
            fs::copy(&path, &dest_path).map_err(|e| format!("复制文件失败: {}", e))?;
        }
    }
    Ok(())
}
