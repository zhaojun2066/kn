use std::path::{Path, PathBuf};

/// Resolve the user home directory.
///
/// Reads `HOME` first, falling back to `echo $HOME` via `sh`,
/// and finally to temp dir as a last resort.
pub fn home_dir() -> PathBuf {
    if let Ok(home) = std::env::var("HOME") {
        return PathBuf::from(home);
    }
    // Fallback: try shell to resolve home directory
    if let Ok(output) = std::process::Command::new("sh")
        .args(["-c", "echo $HOME"])
        .output()
    {
        let s = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !s.is_empty() {
            return PathBuf::from(s);
        }
    }
    // Last resort: temp dir is always available and writable,
    // unlike CWD which may be "/" in macOS .app bundles
    std::env::temp_dir()
}

/// Shared config directory.
///
/// Respects `KN_HOME` env var first, then `CLAUDE_PROFILES_HOME` (legacy),
/// falling back to `~/.kn` on all platforms.
///
/// Environment variable values are validated: must be absolute and
/// must not contain `..` path-traversal components.
pub fn config_dir() -> PathBuf {
    for var in &["KN_HOME", "CLAUDE_PROFILES_HOME"] {
        if let Ok(dir) = std::env::var(var) {
            let p = PathBuf::from(&dir);
            if p.is_absolute() && !p.components().any(|c| c == std::path::Component::ParentDir) {
                return p;
            }
        }
    }
    let home = std::env::var("HOME").unwrap_or_else(|_| {
        std::env::temp_dir().to_string_lossy().to_string()
    });
    PathBuf::from(&home).join(".kn")
}

/// Agent-specific directory under the config dir.
pub fn agent_dir() -> PathBuf {
    config_dir().join("agent")
}

/// Resolve a binary by trying hardcoded macOS paths first,
/// then falling back to login-shell PATH lookup.
pub fn find_binary(names: &[&str]) -> Option<String> {
    for name in names {
        let paths: Vec<String> = vec![
            format!("/usr/bin/{}", name),
            format!("/opt/homebrew/bin/{}", name),
            format!("/usr/local/bin/{}", name),
        ];
        for p in &paths {
            if Path::new(p).exists() {
                return Some(p.clone());
            }
        }
    }
    // Fallback: check login-shell PATH
    for name in names {
        if let Some(path) = resolve_from_shell_path(name) {
            return Some(path);
        }
    }
    // Final fallback: bare command name (None if names is empty)
    names.first().map(|n| n.to_string())
}

/// Resolve a binary name via login-shell PATH lookup.
fn resolve_from_shell_path(name: &str) -> Option<String> {
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string());
    let cmd = format!(
        "command -v {} 2>/dev/null || (type {} 2>/dev/null | grep -v 'not found')",
        name, name
    );
    let output = std::process::Command::new(&shell)
        .args(["-lc", &cmd])
        .output()
        .ok()?;
    let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if path.is_empty() || path.contains("not found") {
        None
    } else {
        Some(path)
    }
}

/// Verify a file's SHA-256 checksum.
pub fn verify_sha256(path: &Path, expected: &str) -> Result<bool, String> {
    use sha2::Digest;
    let mut file = std::fs::File::open(path).map_err(|e| format!("无法打开文件: {}", e))?;
    let mut hasher = sha2::Sha256::new();
    std::io::copy(&mut file, &mut hasher).map_err(|e| format!("读取文件失败: {}", e))?;
    let actual = format!("{:x}", hasher.finalize());
    Ok(actual.to_lowercase() == expected.to_lowercase())
}

/// Atomically rename `src` to `dst`, overwriting `dst` if it exists (on Unix).
pub fn atomic_rename(src: &Path, dst: &Path) -> Result<(), String> {
    std::fs::rename(src, dst)
        .map_err(|e| format!("rename 失败: {} -> {}: {}", src.display(), dst.display(), e))
}

/// Generate a short (8-char hex) hash of a path string.
/// Used to create unique scope keys across projects.
///
/// Uses SHA-256 (truncated to 8 hex chars) for deterministic output
/// across Rust compiler versions, architectures, and runs.
pub fn hash_path(path: &str) -> String {
    use sha2::Digest;
    let mut hasher = sha2::Sha256::new();
    hasher.update(path.as_bytes());
    format!("{:08x}", hasher.finalize())
}

/// Derive a project name from a project root directory path.
pub fn project_name_from_root(root: &Path) -> Option<String> {
    root.file_name()
        .and_then(|n| n.to_str())
        .map(|s| s.to_string())
}
