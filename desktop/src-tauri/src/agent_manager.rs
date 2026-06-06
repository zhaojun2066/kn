// agent_manager.rs — Agent scanning, toggling, and dependency graph construction
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

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
