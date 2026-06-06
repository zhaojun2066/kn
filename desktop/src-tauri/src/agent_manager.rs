// agent_manager.rs — Agent scanning, toggling, and dependency graph construction
use serde::{Deserialize, Serialize};

// ── Agent data types ──

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentEntry {
    pub id: String,           // "claude:agent:security-auditor"
    pub cli: String,          // "claude" | "codex" | "qoder"
    pub name: String,
    pub description: String,
    pub enabled: bool,
    pub source: String,       // "builtin" | "user" | "project" | "plugin"
    pub model: Option<String>,
    pub tools: Vec<String>,
    pub color: Option<String>,
    pub path: String,
    pub skills: Vec<String>,  // Agent → Skill references from frontmatter
    pub sandbox_mode: Option<String>, // Codex only
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentManagerData {
    pub agents: Vec<AgentEntry>,
}

// ── Dependency graph types (used in Phase 2) ──

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DependencyGraph {
    pub nodes: Vec<DepNode>,
    pub edges: Vec<DepEdge>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DepNode {
    pub id: String,           // "claude:agent:security-auditor"
    pub kind: String,         // "plugin" | "agent" | "skill" | "tool" | "mcp"
    pub label: String,
    pub cli: String,
    pub source: String,
    pub locked: bool,         // source == "builtin" → true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DepEdge {
    pub from: String,
    pub to: String,
    pub kind: String,         // "contains" | "references" | "spawns" | "needsTool" | "needsModel"
    pub label: String,
}

// ── Builtin agent definitions ──

/// Returns builtin agents for a given CLI. These are never on disk —
/// we synthesize them from known lists so the user can see what's available.
fn builtin_agents(cli: &str) -> Vec<AgentEntry> {
    match cli {
        "claude" => vec![
            builtin("claude", "Explore", "Fast read-only code exploration agent", &["Read", "Grep", "WebSearch", "WebFetch"], Some("haiku"), "#10B981"),
            builtin("claude", "Plan", "Software architect for designing implementation plans", &["Read", "Grep", "Glob", "WebSearch", "WebFetch"], None, "#3B82F6"),
            builtin("claude", "general-purpose", "Catch-all agent for complex multi-step tasks", &["*"], None, "#6B7280"),
            builtin("claude", "statusline-setup", "Configures the Claude Code status line", &["Read", "Edit"], None, "#8B5CF6"),
            builtin("claude", "claude-code-guide", "Answers questions about Claude Code features", &["Read", "Bash", "WebFetch", "WebSearch"], None, "#F59E0B"),
        ],
        "qoder" => vec![
            builtin("qoder", "Explore", "Fast read-only code exploration agent", &["Read", "Grep", "WebSearch", "WebFetch"], Some("haiku"), "#10B981"),
            builtin("qoder", "Plan", "Software architect for designing implementation plans", &["Read", "Grep", "Glob", "WebSearch", "WebFetch"], None, "#3B82F6"),
            builtin("qoder", "general-purpose", "Catch-all agent for complex multi-step tasks", &["*"], None, "#6B7280"),
            builtin("qoder", "qoder-guide", "Answers questions about Qoder CLI features", &["Read", "Bash", "WebFetch", "WebSearch"], None, "#F59E0B"),
            builtin("qoder", "statusline-setup", "Configures the Qoder status line", &["Read", "Edit"], None, "#8B5CF6"),
        ],
        "codex" => vec![
            builtin("codex", "default", "Default general-purpose agent", &["*"], None, "#6B7280"),
            builtin("codex", "worker", "Worker agent for parallel task execution", &["*"], None, "#6B7280"),
            builtin("codex", "explorer", "Read-only code exploration agent", &["Read", "Grep"], Some("haiku"), "#10B981"),
        ],
        _ => vec![],
    }
}

fn builtin(cli: &str, name: &str, desc: &str, tools: &[&str], model: Option<&str>, color: &str) -> AgentEntry {
    AgentEntry {
        id: format!("{}:agent:{}", cli, name),
        cli: cli.to_string(),
        name: name.to_string(),
        description: desc.to_string(),
        enabled: true,
        source: "builtin".to_string(),
        model: model.map(|m| m.to_string()),
        tools: tools.iter().map(|t| t.to_string()).collect(),
        color: Some(color.to_string()),
        path: String::new(), // builtin agents have no file path
        skills: vec![],
        sandbox_mode: None,
    }
}

// ── YAML frontmatter parsing ──

/// Parse YAML frontmatter from a markdown file.
/// Returns a serde_yaml::Value if frontmatter is found between --- delimiters.
fn parse_frontmatter(content: &str) -> Option<serde_yaml::Value> {
    let mut lines = content.lines();
    // First line must be exactly "---"
    if lines.next()?.trim() != "---" {
        return None;
    }
    let mut yaml_str = String::new();
    for line in &mut lines {
        if line.trim() == "---" {
            break;
        }
        yaml_str.push_str(line);
        yaml_str.push('\n');
    }
    serde_yaml::from_str(&yaml_str).ok()
}

/// Create an AgentEntry from a frontmatter Value + file path metadata.
fn agent_from_frontmatter(
    cli: &str,
    name: &str,
    path: std::path::PathBuf,
    source: &str,     // "user" | "project"
    frontmatter: &serde_yaml::Value,
) -> AgentEntry {
    let description = frontmatter.get("description")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let model = frontmatter.get("model")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let tools: Vec<String> = frontmatter.get("tools")
        .and_then(|v| v.as_sequence())
        .map(|seq| seq.iter()
            .filter_map(|t| t.as_str().map(String::from))
            .collect())
        .unwrap_or_default();

    let color = frontmatter.get("color")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let skills: Vec<String> = frontmatter.get("skills")
        .and_then(|v| v.as_sequence())
        .map(|seq| seq.iter()
            .filter_map(|s| s.as_str().map(String::from))
            .collect())
        .unwrap_or_default();

    let sandbox_mode = frontmatter.get("sandbox_mode")
        .or_else(|| frontmatter.get("sandboxMode"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let enabled = !path.to_string_lossy().ends_with(".disabled");

    AgentEntry {
        id: format!("{}:agent:{}", cli, name),
        cli: cli.to_string(),
        name: name.to_string(),
        description,
        enabled,
        source: source.to_string(),
        model,
        tools,
        color,
        path: path.to_string_lossy().to_string(),
        skills,
        sandbox_mode,
    }
}

// ── Claude agent scanning ──

fn claude_agents_dir() -> std::path::PathBuf {
    let home = crate::commands::home_dir();
    home.join(".claude").join("agents")
}

fn scan_claude_user_agents() -> Vec<AgentEntry> {
    let dir = claude_agents_dir();
    scan_md_agents_in_dir("claude", &dir, "user")
}

fn scan_claude_project_agents() -> Vec<AgentEntry> {
    // Scan .claude/agents/ in current directory
    let dir = std::env::current_dir()
        .unwrap_or_default()
        .join(".claude")
        .join("agents");
    scan_md_agents_in_dir("claude", &dir, "project")
}

/// Scan a directory for .md agent files (Claude/Qoder format).
fn scan_md_agents_in_dir(cli: &str, dir: &std::path::PathBuf, source: &str) -> Vec<AgentEntry> {
    let mut agents = Vec::new();

    let read_dir = match std::fs::read_dir(dir) {
        Ok(rd) => rd,
        Err(_) => return agents,
    };

    for entry in read_dir.flatten() {
        let path = entry.path();
        let file_name = path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_string(); // owned String — ends borrow on path

        // Check if it's an .md file (both active and .disabled)
        let is_agent_file = file_name.ends_with(".md")
            && !file_name.starts_with('.');

        if !is_agent_file {
            continue;
        }

        let content = match std::fs::read_to_string(&path) {
            Ok(c) => c,
            Err(_) => continue,
        };

        let frontmatter = match parse_frontmatter(&content) {
            Some(fm) => fm,
            None => continue,
        };

        // Get agent name: prefer frontmatter 'name', fall back to filename
        let name = frontmatter.get("name")
            .and_then(|v| v.as_str())
            .unwrap_or(
                // Strip .md or .md.disabled suffix
                file_name
                    .strip_suffix(".disabled")
                    .unwrap_or(&file_name)
                    .strip_suffix(".md")
                    .unwrap_or(&file_name)
            );

        agents.push(agent_from_frontmatter(cli, name, path, source, &frontmatter));
    }

    agents
}

// ── Qoder agent scanning ──

fn qoder_agents_dir() -> std::path::PathBuf {
    let home = crate::commands::home_dir();
    home.join(".qoder-cn").join("agents")
}

fn scan_qoder_user_agents() -> Vec<AgentEntry> {
    let dir = qoder_agents_dir();
    scan_md_agents_in_dir("qoder", &dir, "user")
}

fn scan_qoder_project_agents() -> Vec<AgentEntry> {
    let dir = std::env::current_dir()
        .unwrap_or_default()
        .join(".qoder")
        .join("agents");
    scan_md_agents_in_dir("qoder", &dir, "project")
}

// ── Main scan entry point ──

#[tauri::command]
pub fn scan_agents() -> AgentManagerData {
    let mut agents = Vec::new();

    // 1. Builtin agents (always present, read-only)
    agents.extend(builtin_agents("claude"));
    agents.extend(builtin_agents("qoder"));
    agents.extend(builtin_agents("codex"));

    // 2. Claude user + project agents
    agents.extend(scan_claude_user_agents());
    agents.extend(scan_claude_project_agents());

    // 3. Qoder user + project agents
    agents.extend(scan_qoder_user_agents());
    agents.extend(scan_qoder_project_agents());

    // NOTE: Codex .toml agent scanning deferred to Phase 4

    // Deduplicate: file-based agents override builtins with same id
    let mut seen = std::collections::HashMap::new();
    for agent in agents {
        seen.entry(agent.id.clone())
            .and_modify(|existing: &mut AgentEntry| {
                // Non-builtin overrides builtin
                if existing.source == "builtin" && agent.source != "builtin" {
                    *existing = agent.clone();
                }
            })
            .or_insert(agent);
    }

    let mut agents: Vec<AgentEntry> = seen.into_values().collect();
    // Sort: builtins first, then by name
    agents.sort_by(|a, b| {
        a.source.cmp(&b.source)
            .then(a.name.cmp(&b.name))
    });

    AgentManagerData { agents }
}

// ── Dependency graph builder ──

use crate::skill_manager::{SkillManagerData, PluginEntry, StandaloneSkill};

/// Build a complete dependency graph from skills and agents data.
/// Returns nodes (plugins, skills, agents, tools, mcp servers) and
/// edges (contains, references, spawns, needsTool, needsModel).
#[tauri::command]
pub fn build_dependency_graph(
    skills_data: SkillManagerData,
    agents_data: AgentManagerData,
) -> DependencyGraph {
    let mut nodes: Vec<DepNode> = Vec::new();
    let mut edges: Vec<DepEdge> = Vec::new();

    // ── Add Plugin nodes + contains → Skill edges (edge type 1) ──
    for plugin in &skills_data.plugins {
        let plugin_id = format!("{}:plugin:{}", plugin.cli, plugin.name);
        nodes.push(DepNode {
            id: plugin_id.clone(),
            kind: "plugin".into(),
            label: plugin.name.clone(),
            cli: plugin.cli.clone(),
            source: plugin.source.clone(),
            locked: false,
        });
        for skill in &plugin.skills {
            let skill_id = format!("{}:skill:{}", plugin.cli, skill.name);
            edges.push(DepEdge {
                from: plugin_id.clone(),
                to: skill_id,
                kind: "contains".into(),
                label: "contains".into(),
            });
        }
    }

    // ── Add Skill nodes (standalone + system) ──
    let mut add_skills = |skills: &[StandaloneSkill], source: &str| {
        for skill in skills {
            let skill_id = format!("{}:skill:{}", skill.cli, skill.name);
            nodes.push(DepNode {
                id: skill_id.clone(),
                kind: "skill".into(),
                label: skill.name.clone(),
                cli: skill.cli.clone(),
                source: source.into(),
                locked: source == "system",
            });
        }
    };
    add_skills(&skills_data.standalone_skills, "user");
    add_skills(&skills_data.system_skills, "system");

    // ── Add Agent nodes + edges ──
    for agent in &agents_data.agents {
        let agent_id = agent.id.clone();
        nodes.push(DepNode {
            id: agent_id.clone(),
            kind: "agent".into(),
            label: agent.name.clone(),
            cli: agent.cli.clone(),
            source: agent.source.clone(),
            locked: agent.source == "builtin",
        });

        // Edge type 2: Agent → Skill references
        for skill_name in &agent.skills {
            let skill_id = format!("{}:skill:{}", agent.cli, skill_name);
            edges.push(DepEdge {
                from: agent_id.clone(),
                to: skill_id,
                kind: "references".into(),
                label: format!("references skill:{}", skill_name),
            });
        }

        // Edge type 4: Agent → Tool needs
        for tool in &agent.tools {
            if tool == "*" {
                continue; // wildcard — skip individual edges
            }
            let tool_id = format!("tool:{}", tool);
            if !nodes.iter().any(|n| n.id == tool_id) {
                nodes.push(DepNode {
                    id: tool_id.clone(),
                    kind: "tool".into(),
                    label: tool.clone(),
                    cli: agent.cli.clone(),
                    source: "builtin".into(),
                    locked: true,
                });
            }
            edges.push(DepEdge {
                from: agent_id.clone(),
                to: tool_id,
                kind: "needsTool".into(),
                label: format!("needs {}", tool),
            });
        }

        // Edge type 5: Agent → Model
        if let Some(ref model) = agent.model {
            let model_id = format!("model:{}", model);
            if !nodes.iter().any(|n| n.id == model_id) {
                nodes.push(DepNode {
                    id: model_id.clone(),
                    kind: "tool".into(),
                    label: model.clone(),
                    cli: agent.cli.clone(),
                    source: "builtin".into(),
                    locked: true,
                });
            }
            edges.push(DepEdge {
                from: agent_id.clone(),
                to: model_id,
                kind: "needsModel".into(),
                label: format!("model:{}", model),
            });
        }
    }

    DependencyGraph { nodes, edges }
}

/// Given a node ID, find all nodes that depend on it (reverse BFS).
#[tauri::command]
pub fn analyze_impact(
    target_id: String,
    graph: DependencyGraph,
) -> Vec<String> {
    let mut impacted: Vec<String> = Vec::new();
    let mut visited = std::collections::HashSet::new();
    let mut queue: Vec<&str> = vec![&target_id];

    while let Some(current) = queue.pop() {
        if !visited.insert(current.to_string()) {
            continue;
        }
        for edge in &graph.edges {
            if edge.to == current && !visited.contains(edge.from.as_str()) {
                impacted.push(edge.from.clone());
                queue.push(&edge.from);
            }
        }
    }

    impacted
}
