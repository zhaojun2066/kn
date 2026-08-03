//! Project types shared between Desktop and Agent.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProjectInfo {
    pub name: String,
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_profile: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    #[serde(default)]
    pub pinned: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub verify: Option<ProjectVerifyConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProjectVerifyConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub default_environment: Option<String>,
    #[serde(default)]
    pub environments: BTreeMap<String, ProjectVerifyEnvironment>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct ProjectVerifyEnvironment {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub build: Option<ProjectVerifyCommand>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub test: Option<ProjectVerifyCommand>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProjectVerifyCommand {
    pub command: String,
    #[serde(default = "default_verify_command_enabled")]
    pub enabled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub timeout_seconds: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub parser_hint: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub task_type_hint: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub report_hints: Option<Vec<String>>,
}

fn default_verify_command_enabled() -> bool {
    true
}
