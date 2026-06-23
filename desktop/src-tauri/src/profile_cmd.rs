//! Profile configuration — thin wrapper around kn_common::profile.
//!
//! Re-exports all types and read/write operations from the shared library.
//! Desktop-specific code (shell RC management, embedded resources) lives here.

use std::fs;
use std::path::PathBuf;

// Re-export everything from the common library
pub use kn_common::profile::*;
// Explicit imports for functions used in ensure_shell_rc (already covered by glob, kept for clarity)
use kn_common::profile::{acquire_lock, release_lock, read_config, write_config_inner};

// ── Desktop-specific: intra-process lock wrapper ─────────────

/// Write config with intra-process mutex (in addition to cross-process file lock).
/// Use this for desktop write operations that may race with other Tauri commands.
#[allow(dead_code)]
pub fn write_config(config: &Config) -> Result<(), String> {
    crate::with_write_lock(|| kn_common::profile::write_config_file(config))
}

// ── Desktop-specific: shell RC management ────────────────────

// Embedded at build time from canonical sources in shell/ directory.
const SHELL_RC: &str =
    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../shell/ai-profile.sh"));
const COMPLETION_ZSH: &str =
    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../shell/completions/_ai"));
const COMPLETION_BASH: &str =
    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../shell/completions/ai.bash"));
const HOOK_RECORDER: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../shell/hooks/record-usage.py"
));

pub fn ensure_shell_rc() -> Result<String, String> {
    // One-time migration from legacy ~/.claude-profiles → ~/.kn
    crate::migrate_legacy_config_dir();

    let dir = kn_common::path::config_dir();
    fs::create_dir_all(&dir).map_err(|e| format!("创建目录失败: {}", e))?;

    let home = kn_common::path::home_dir().to_string_lossy().to_string();

    // ── Migration: merge old dev config into unified config (one-time) ──
    let dev_config = PathBuf::from(&home)
        .join(".claude-profiles-dev")
        .join("config.yaml");
    if dev_config.exists() {
        if let Ok(content) = fs::read_to_string(&dev_config) {
            if let Ok(dev_cfg) = serde_yaml::from_str::<Config>(&content) {
                let _ = crate::with_write_lock(|| {
                    let lock_path =
                        kn_common::path::config_dir().join(".config.lock");
                    let lock_fh = match std::fs::OpenOptions::new()
                        .create(true)
                        .write(true)
                        .truncate(false)
                        .open(&lock_path)
                    {
                        Ok(fh) => fh,
                        Err(_) => return Ok(()),
                    };
                    if acquire_lock(&lock_fh, std::time::Duration::from_secs(5)).is_err() {
                        return Ok(());
                    }

                    let result = (|| {
                        let mut prod_cfg = read_config()?;
                        let mut merged = false;
                        for (name, pc) in &dev_cfg.profiles {
                            if !prod_cfg.profiles.contains_key(name) {
                                prod_cfg.profiles.insert(name.clone(), pc.clone());
                                merged = true;
                            }
                        }
                        if merged {
                            if prod_cfg.default.is_empty() && !dev_cfg.default.is_empty() {
                                prod_cfg.default = dev_cfg.default.clone();
                            }
                            write_config_inner(&dir, &dir.join("config.yaml"), &prod_cfg)?;
                        }
                        Ok(())
                    })();

                    release_lock(&lock_fh)?;
                    result
                });
            }
        }
        let _ = fs::rename(&dev_config, dev_config.with_extension("yaml.migrated"));
    }

    // Write shell-rc to config dir (only if content changed — preserves user customizations)
    let shell_rc_path = dir.join("shell-rc");
    let needs_write = match fs::read_to_string(&shell_rc_path) {
        Ok(existing) => existing != SHELL_RC,
        Err(_) => true,
    };
    if needs_write {
        fs::write(&shell_rc_path, SHELL_RC)
            .map_err(|e| format!("写入 shell-rc 失败: {}", e))?;
    }

    // Write shell completions to config dir
    let completions_dir = dir.join("completions");
    fs::create_dir_all(&completions_dir).ok();
    let zsh_path = completions_dir.join("_ai");
    let bash_path = completions_dir.join("ai.bash");
    if fs::read_to_string(&zsh_path).map_or(true, |e| e != COMPLETION_ZSH) {
        fs::write(&zsh_path, COMPLETION_ZSH).ok();
    }
    if fs::read_to_string(&bash_path).map_or(true, |e| e != COMPLETION_BASH) {
        fs::write(&bash_path, COMPLETION_BASH).ok();
    }

    // Write token usage hook recorder script
    let hooks_dir = dir.join("hooks");
    fs::create_dir_all(&hooks_dir).ok();
    fs::write(hooks_dir.join("record-usage.py"), HOOK_RECORDER).ok();

    // Write hook execution log wrapper script
    let _ = crate::hook_logs::write_run_with_log_script();

    // Repair any missing hook store scripts
    crate::hook_store::repair_missing_hook_scripts();

    // ── add source line to ~/.zshrc (idempotent) ──
    let zshrc = PathBuf::from(&home).join(".zshrc");
    let source_line = format!("source \"{}/shell-rc\"", dir.display());
    let content = if zshrc.exists() {
        fs::read_to_string(&zshrc).unwrap_or_default()
    } else {
        String::new()
    };
    let content = remove_claude_profiles_lines(&content);
    let marker = "# kn";
    if !content.contains(&source_line) {
        let new_content = if content.ends_with('\n') || content.is_empty() {
            format!("{}{}\n{}\n", content, marker, source_line)
        } else {
            format!("{}\n{}\n{}\n", content, marker, source_line)
        };
        fs::write(&zshrc, new_content).map_err(|e| format!("写入 .zshrc 失败: {}", e))?;
    } else {
        fs::write(&zshrc, content).map_err(|e| format!("写入 .zshrc 失败: {}", e))?;
    }

    // ── Also add to ~/.bashrc (harmless on macOS) ──
    {
        let bashrc = PathBuf::from(&home).join(".bashrc");
        let bash_source_line = format!("source \"{}/shell-rc\"", dir.display());
        let bash_content = if bashrc.exists() {
            fs::read_to_string(&bashrc).unwrap_or_default()
        } else {
            String::new()
        };
        let bash_content = remove_claude_profiles_lines(&bash_content);
        let bash_marker = "# kn (bash)";
        if !bash_content.contains(&bash_source_line) {
            let new_bash = if bash_content.ends_with('\n') || bash_content.is_empty() {
                format!("{}{}\n{}\n", bash_content, bash_marker, bash_source_line)
            } else {
                format!(
                    "{}\n{}\n{}\n",
                    bash_content, bash_marker, bash_source_line
                )
            };
            fs::write(&bashrc, new_bash).ok();
        } else {
            fs::write(&bashrc, bash_content).ok();
        }

        // ── Inject shell completion config ──
        let completions_dir_str = completions_dir.display().to_string();
        let compl_marker_start = "# >>> AI Profile Completions >>>";
        let compl_marker_end = "# <<< AI Profile Completions <<<";

        // Zsh: fpath + compinit
        {
            let zshrc = PathBuf::from(&home).join(".zshrc");
            let zsh_content = if zshrc.exists() {
                fs::read_to_string(&zshrc).unwrap_or_default()
            } else {
                String::new()
            };
            let has_compinit = zsh_content.contains("compinit");
            let zsh_compl_block = if has_compinit {
                format!(
                    "\n{}\nfpath=(\"{}\" $fpath)\n{}\n",
                    compl_marker_start, completions_dir_str, compl_marker_end
                )
            } else {
                format!(
                    "\n{}\nfpath=(\"{}\" $fpath)\nautoload -Uz compinit && compinit\n{}\n",
                    compl_marker_start, completions_dir_str, compl_marker_end
                )
            };
            let zsh_cleaned =
                remove_marker_block(&zsh_content, compl_marker_start, compl_marker_end);
            let new_zsh = format!("{}{}", zsh_cleaned, zsh_compl_block);
            fs::write(&zshrc, new_zsh).ok();
        }

        // Bash: source completion script
        {
            let bashrc = PathBuf::from(&home).join(".bashrc");
            let bash_content = if bashrc.exists() {
                fs::read_to_string(&bashrc).unwrap_or_default()
            } else {
                String::new()
            };
            let bash_compl_block = format!(
                "\n{}\nsource \"{}/ai.bash\"\n{}\n",
                compl_marker_start, completions_dir_str, compl_marker_end
            );
            let bash_cleaned =
                remove_marker_block(&bash_content, compl_marker_start, compl_marker_end);
            let new_bash = format!("{}{}", bash_cleaned, bash_compl_block);
            fs::write(&bashrc, new_bash).ok();
        }
    }

    Ok(dir.display().to_string())
}

// ── Helpers ──────────────────────────────────────────────────

fn remove_claude_profiles_lines(content: &str) -> String {
    content
        .lines()
        .filter(|line| !line.contains(".claude-profiles"))
        .collect::<Vec<&str>>()
        .join("\n")
}

fn remove_marker_block(content: &str, marker_start: &str, marker_end: &str) -> String {
    let mut result = String::with_capacity(content.len());
    let mut skip = false;
    for line in content.lines() {
        if line.trim() == marker_start {
            skip = true;
            continue;
        }
        if line.trim() == marker_end {
            skip = false;
            continue;
        }
        if !skip {
            result.push_str(line);
            result.push('\n');
        }
    }
    result
}
