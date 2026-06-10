use std::path::PathBuf;

use crate::commands::find_binary;

/// Resolve a CLI binary (claude, codex) to its full path.
/// Required because Tauri .app bundles have a minimal PATH that
/// doesn't include Homebrew or npm global install directories.
pub(super) fn cli_binary(name: &str) -> String {
    find_binary(&[name]).unwrap_or_else(|| name.to_string())
}

// ── Types (mirrors frontend `SkillManager.tsx` types) ────────────

// ── Paths ──────────────────────────────────────────────────────

/// Thin wrapper around [`crate::home_dir`] that returns `None` instead of `"."` fallback.
/// Preserves Option semantics used by the skill scanning call chains.
pub(super) fn home_dir() -> Option<PathBuf> {
    let h = crate::home_dir();
    if h.as_os_str() == "." { None } else { Some(h) }
}

pub(super) fn claude_skills_dir() -> Option<PathBuf> {
    home_dir().map(|h| h.join(".claude").join("skills"))
}

pub(super) fn claude_commands_dir() -> Option<PathBuf> {
    home_dir().map(|h| h.join(".claude").join("commands"))
}

pub(super) fn claude_plugins_json() -> Option<PathBuf> {
    home_dir().map(|h| {
        h.join(".claude")
            .join("plugins")
            .join("installed_plugins.json")
    })
}

pub(super) fn claude_settings_json() -> Option<PathBuf> {
    home_dir().map(|h| h.join(".claude").join("settings.json"))
}

pub(super) fn codex_skills_dir() -> Option<PathBuf> {
    home_dir().map(|h| h.join(".codex").join("skills"))
}

pub(super) fn codex_config_toml() -> Option<PathBuf> {
    home_dir().map(|h| h.join(".codex").join("config.toml"))
}

pub(super) fn codex_plugins_dir() -> Option<PathBuf> {
    home_dir().map(|h| h.join(".codex").join("plugins"))
}

pub(super) fn qoder_skills_dir() -> Option<PathBuf> {
    home_dir().map(|h| h.join(".qoder-cn").join("skills"))
}

pub(super) fn claude_known_marketplaces_json() -> Option<PathBuf> {
    home_dir().map(|h| {
        h.join(".claude")
            .join("plugins")
            .join("known_marketplaces.json")
    })
}
