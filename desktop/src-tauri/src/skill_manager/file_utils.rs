use std::fs;
use std::path::{Path, PathBuf};

// ── Symlink Resolution ─────────────────────────────────────────

/// Resolve a symlink to its target path. Returns the original path if not a symlink.
pub(super) fn resolve_symlink(path: &Path) -> PathBuf {
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
pub(super) fn extract_description(md_path: &Path) -> Option<String> {
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
pub(super) fn read_skill_description(skill_path: &Path) -> Option<String> {
    if skill_path.is_dir() {
        extract_description(&skill_path.join("SKILL.md"))
    } else if skill_path.extension().is_some_and(|e| e == "md") {
        extract_description(skill_path)
    } else {
        None
    }
}

// ── Entry type detection ───────────────────────────────────────

pub(super) fn classify_entry(path: &Path) -> &'static str {
    if path.is_symlink() {
        "symlink"
    } else if path.is_dir() {
        "directory"
    } else {
        "file"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_description_from_crlf_frontmatter() {
        let temp = tempfile::tempdir().expect("tempdir");
        let path = temp.path().join("SKILL.md");
        std::fs::write(&path, "---\r\ndescription: \"CRLF skill\"\r\n---\r\nBody\r\n")
            .expect("write skill");

        assert_eq!(extract_description(&path), Some("CRLF skill".to_string()));
    }

    #[test]
    fn ignores_markdown_without_frontmatter() {
        let temp = tempfile::tempdir().expect("tempdir");
        let path = temp.path().join("SKILL.md");
        std::fs::write(&path, "# Plain skill\n").expect("write skill");

        assert_eq!(extract_description(&path), None);
    }
}
