use serde::{Deserialize, Serialize};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use crate::agent_manager::AgentEntry;

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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillContent {
    pub description: String,
    pub body: String,
}

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
