use crate::profile_cmd;
use sha2::{Digest, Sha256};
use std::time::Duration;
use tauri::command;
use tauri::Emitter;

// ── Config backup paths ────────────────────────────────────
fn config_dir() -> std::path::PathBuf {
    for var in &["KN_HOME", "CLAUDE_PROFILES_HOME"] {
        if let Ok(dir) = std::env::var(var) {
            return std::path::PathBuf::from(dir);
        }
    }
    crate::config_dir()
}
fn config_file() -> std::path::PathBuf {
    config_dir().join("config.yaml")
}
fn backup_file() -> std::path::PathBuf {
    config_dir().join("config.yaml.bak")
}

#[command]
pub fn config_backup_exists() -> bool {
    backup_file().exists()
}

#[command]
pub fn backup_config() -> Result<String, String> {
    let cfg = config_file();
    let bak = backup_file();
    if !cfg.exists() {
        return Err("配置文件不存在".into());
    }
    std::fs::copy(&cfg, &bak).map_err(|e| format!("备份失败: {}", e))?;
    Ok("配置已备份".into())
}

#[command]
pub fn restore_config_backup() -> Result<String, String> {
    let bak = backup_file();
    let cfg = config_file();
    if !bak.exists() {
        return Err("备份文件不存在".into());
    }
    // Create a backup of current config before restoring (safety net)
    if cfg.exists() {
        let pre_restore = config_dir().join("config.yaml.pre-restore");
        std::fs::copy(&cfg, &pre_restore).map_err(|e| format!("无法创建恢复前备份: {}", e))?;
    }
    std::fs::copy(&bak, &cfg).map_err(|e| format!("恢复失败: {}", e))?;
    Ok("配置已从备份恢复".into())
}

#[command]
pub fn batch_export_profiles(names: Vec<String>) -> Result<String, String> {
    let mut results: Vec<serde_json::Value> = Vec::new();
    for name in &names {
        let detail = profile_cmd::show_profile_cmd(name)?;
        results.push(serde_json::json!({
            "name": detail.name,
            "desc": detail.desc,
            "env": detail.env,
        }));
    }
    serde_json::to_string_pretty(&results).map_err(|e| format!("JSON 序列化失败: {}", e))
}

#[command]
pub fn batch_delete_profiles(names: Vec<String>) -> Result<Vec<String>, String> {
    let mut deleted: Vec<String> = Vec::new();
    for name in &names {
        match profile_cmd::remove_profile_cmd(name) {
            Ok(r) if r.ok => {
                deleted.push(name.clone());
            }
            Ok(_) => { /* profile didn't exist, skip */ }
            Err(e) => return Err(format!("删除 '{}' 失败: {}", name, e)),
        }
    }
    Ok(deleted)
}

#[command]
pub fn list_profiles() -> Result<profile_cmd::ProfileList, String> {
    profile_cmd::list_profiles_cmd()
}

#[command]
pub fn show_profile(name: String) -> Result<profile_cmd::ProfileDetail, String> {
    profile_cmd::show_profile_cmd(&name)
}

#[command]
pub fn get_env(name: String) -> Result<profile_cmd::EnvOutput, String> {
    profile_cmd::get_env_cmd(&name)
}

#[command]
pub fn add_profile(
    name: String,
    desc: Option<String>,
) -> Result<profile_cmd::MutationResult, String> {
    profile_cmd::add_profile_cmd(&name, desc.as_deref())
}

#[command]
pub fn remove_profile(name: String) -> Result<profile_cmd::MutationResult, String> {
    profile_cmd::remove_profile_cmd(&name)
}

#[command]
pub fn set_env_var(
    name: String,
    key: String,
    value: String,
) -> Result<profile_cmd::MutationResult, String> {
    profile_cmd::set_env_var_cmd(&name, &key, &value)
}

#[command]
pub fn unset_env_var(name: String, key: String) -> Result<profile_cmd::MutationResult, String> {
    profile_cmd::unset_env_var_cmd(&name, &key)
}

#[command]
pub fn set_default_profile(name: String) -> Result<profile_cmd::MutationResult, String> {
    profile_cmd::set_default_profile_cmd(&name)
}

#[command]
pub fn get_default_profile() -> Result<String, String> {
    profile_cmd::get_default_profile_cmd()
}

#[command]
pub fn init_profiles() -> Result<profile_cmd::MutationResult, String> {
    profile_cmd::init_profiles_cmd()
}

#[command]
pub fn ensure_shell_rc() -> Result<String, String> {
    profile_cmd::ensure_shell_rc()
}

#[tauri::command]
pub fn write_file(path: String, content: String) -> Result<(), String> {
    if !is_safe_path(std::path::Path::new(&path)) {
        return Err("不允许访问此路径".into());
    }
    std::fs::write(&path, &content).map_err(|e| format!("写入文件失败: {}", e))
}

fn is_safe_path(path: &std::path::Path) -> bool {
    // Resolve .. and symlinks before checking prefix.
    // If canonicalize fails (path doesn't exist or can't be resolved),
    // reject the operation — don't fall back to un-resolved path which
    // could bypass the safety check (TOCTOU race).
    let resolved = match path.canonicalize() {
        Ok(r) => r,
        Err(_) => return false,
    };
    let home = home_dir();
    let tmp = std::env::temp_dir();
    let home_resolved = match home.canonicalize() {
        Ok(r) => r,
        Err(_) => return false,
    };
    let tmp_resolved = match tmp.canonicalize() {
        Ok(r) => r,
        Err(_) => return false,
    };
    resolved.starts_with(&home_resolved) || resolved.starts_with(&tmp_resolved)
}

#[tauri::command]
pub fn read_file(path: String) -> Result<String, String> {
    if !is_safe_path(std::path::Path::new(&path)) {
        return Err("不允许访问此路径".into());
    }
    std::fs::read_to_string(&path).map_err(|e| format!("读取文件失败: {}", e))
}

#[tauri::command]
pub fn read_file_base64(path: String) -> Result<String, String> {
    if !is_safe_path(std::path::Path::new(&path)) {
        return Err("不允许访问此路径".into());
    }
    use base64::{engine::general_purpose::STANDARD, Engine as _};
    let bytes = std::fs::read(&path).map_err(|e| format!("读取文件失败: {}", e))?;
    Ok(STANDARD.encode(&bytes))
}

#[derive(Debug, serde::Serialize, Clone)]
pub struct FileTreeNode {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub children: Option<Vec<FileTreeNode>>,
}

/// Directories & files to skip when building the file tree.
/// Hidden files (dot-prefixed) are now shown; only massive/noisy dirs are skipped.
const SKIP_NAMES: &[&str] = &[
    ".git",
    "node_modules",
    "__pycache__",
    ".DS_Store",
];

/// Maximum recursion depth to prevent runaway on deeply nested structures
const MAX_TREE_DEPTH: u32 = 20;

fn build_tree(root: &std::path::Path) -> Result<FileTreeNode, String> {
    let mut visited = std::collections::HashSet::new();
    build_tree_inner(root, &mut visited, 0)
}

fn build_tree_inner(
    root: &std::path::Path,
    visited: &mut std::collections::HashSet<std::path::PathBuf>,
    depth: u32,
) -> Result<FileTreeNode, String> {
    let name = root
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
    let path = root.display().to_string();
    let is_dir = root.is_dir();

    if !is_dir {
        return Ok(FileTreeNode {
            name,
            path,
            is_dir: false,
            children: None,
        });
    }

    // Symlink cycle guard: detect already-visited directories
    let canonical = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    if !visited.insert(canonical) {
        return Ok(FileTreeNode {
            name,
            path,
            is_dir: true,
            children: Some(vec![]),
        });
    }

    // Depth limit
    if depth >= MAX_TREE_DEPTH {
        return Ok(FileTreeNode {
            name,
            path,
            is_dir: true,
            children: Some(vec![]),
        });
    }

    let mut children: Vec<FileTreeNode> = Vec::new();
    let entries = std::fs::read_dir(root).map_err(|e| format!("读取目录失败: {}", e))?;

    let mut dirs: Vec<(String, std::path::PathBuf)> = Vec::new();
    let mut files: Vec<(String, std::path::PathBuf)> = Vec::new();

    for entry in entries.flatten() {
        let fname = entry.file_name().to_string_lossy().to_string();
        if SKIP_NAMES.contains(&fname.as_str()) {
            continue;
        }
        let fpath = entry.path();
        if fpath.is_dir() {
            dirs.push((fname, fpath));
        } else {
            files.push((fname, fpath));
        }
    }

    dirs.sort_by(|a, b| a.0.to_lowercase().cmp(&b.0.to_lowercase()));
    files.sort_by(|a, b| a.0.to_lowercase().cmp(&b.0.to_lowercase()));

    for (_name, dir_path) in &dirs {
        match build_tree_inner(dir_path, visited, depth + 1) {
            Ok(node) => children.push(node),
            Err(_) => continue,
        }
    }

    for (fname, fpath) in &files {
        children.push(FileTreeNode {
            name: fname.clone(),
            path: fpath.display().to_string(),
            is_dir: false,
            children: None,
        });
    }

    Ok(FileTreeNode {
        name,
        path,
        is_dir: true,
        children: Some(children),
    })
}

#[tauri::command]
pub fn list_directory_tree(path: String) -> Result<FileTreeNode, String> {
    let p = std::path::Path::new(&path);

    // If the path is a single file, return a tree with just that file
    if p.is_file() {
        let name = p
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
            .to_string();
        if !is_safe_path(p) {
            return Err("不允许访问此路径".into());
        }
        return Ok(FileTreeNode {
            name: name.clone(),
            path: p.to_string_lossy().to_string(),
            is_dir: false,
            children: None,
        });
    }

    // Directory — build full tree
    let root = p.to_path_buf();
    if !root.exists() {
        return Err(format!("路径不存在: {}", root.display()));
    }
    if !is_safe_path(&root) {
        return Err("不允许访问此路径".into());
    }

    build_tree(&root)
}

/// Read a single directory level (no recursion).
/// Directories are returned with `children: None` to indicate "not yet expanded".
/// The frontend lazily loads children when the user expands a directory node.
#[tauri::command]
pub fn list_directory_children(path: String) -> Result<Vec<FileTreeNode>, String> {
    let p = std::path::Path::new(&path);
    if !p.is_dir() {
        return Err("路径不是目录".into());
    }
    if !p.exists() {
        return Err(format!("路径不存在: {}", p.display()));
    }
    if !is_safe_path(p) {
        return Err("不允许访问此路径".into());
    }

    let mut dirs: Vec<(String, std::path::PathBuf)> = Vec::new();
    let mut files: Vec<(String, std::path::PathBuf)> = Vec::new();

    let entries = std::fs::read_dir(p).map_err(|e| format!("读取目录失败: {}", e))?;
    for entry in entries.flatten() {
        let fname = entry.file_name().to_string_lossy().to_string();
        if SKIP_NAMES.contains(&fname.as_str()) {
            continue;
        }
        let fpath = entry.path();
        if fpath.is_dir() {
            dirs.push((fname, fpath));
        } else {
            files.push((fname, fpath));
        }
    }

    dirs.sort_by(|a, b| a.0.to_lowercase().cmp(&b.0.to_lowercase()));
    files.sort_by(|a, b| a.0.to_lowercase().cmp(&b.0.to_lowercase()));

    let mut children: Vec<FileTreeNode> = Vec::with_capacity(dirs.len() + files.len());
    for (name, fpath) in &dirs {
        children.push(FileTreeNode {
            name: name.clone(),
            path: fpath.display().to_string(),
            is_dir: true,
            children: None, // not yet expanded
        });
    }
    for (fname, fpath) in &files {
        children.push(FileTreeNode {
            name: fname.clone(),
            path: fpath.display().to_string(),
            is_dir: false,
            children: None,
        });
    }

    Ok(children)
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct AppConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub update_url: Option<String>,
}

#[tauri::command]
pub fn read_app_config() -> Result<AppConfig, String> {
    let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    // macOS .app bundle: Resources/update/update.json (relative to executable)
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()))
        .unwrap_or_else(|| cwd.clone());
    let paths = vec![
        // Bundled resource (production):
        //   macOS .app:  Contents/Resources/ (relative to exe: ../Resources/)
        //   Windows/MSI: next to .exe
        //   Linux:        next to binary
        exe_dir.join("../Resources/update.json"), // macOS
        exe_dir.join("update.json"),              // Windows / Linux
        // Development fallback
        cwd.join("update.json"),
        // Global fallback
        crate::config_dir().join("update.json"),
    ];
    for path in &paths {
        if path.exists() {
            let content = std::fs::read_to_string(path).map_err(|e| format!("读取失败: {}", e))?;
            return serde_json::from_str(&content).map_err(|e| format!("解析失败: {}", e));
        }
    }
    Ok(AppConfig { update_url: None })
}

#[tauri::command]
#[allow(dead_code)]
pub fn write_app_config(config: AppConfig) -> Result<(), String> {
    let dir = std::env::current_dir()
        .unwrap_or_else(|_| std::path::PathBuf::from("."))
        .join("update");
    std::fs::create_dir_all(&dir).map_err(|e| format!("创建目录失败: {}", e))?;
    let path = dir.join("update.json");
    let content =
        serde_json::to_string_pretty(&config).map_err(|e| format!("序列化失败: {}", e))?;
    std::fs::write(&path, &content).map_err(|e| format!("写入失败: {}", e))
}

#[derive(Debug, serde::Serialize)]
pub struct ScanResult {
    pub profiles: Vec<ScanProfile>,
}

#[derive(Debug, serde::Serialize, Clone)]
pub struct ScanProfile {
    pub name: String,
    pub cli_type: String,
    pub env: std::collections::HashMap<String, String>,
    pub source: String,
}

pub(crate) fn home_dir() -> std::path::PathBuf {
    crate::home_dir()
}

/// Returns the user's home directory as a string, for use by the frontend
/// when computing user-level config paths.
#[tauri::command]
pub fn get_home_dir() -> String {
    home_dir().to_string_lossy().to_string()
}

fn read_json_file(path: &std::path::Path) -> Result<serde_json::Value, String> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| format!("读取 {} 失败: {}", path.display(), e))?;
    serde_json::from_str(&content).map_err(|e| format!("解析 {} 失败: {}", path.display(), e))
}

/// Profile names that conflict with reserved keywords in `profile_cmd::add_profile_cmd`.
/// Scan results with these names get a "-config" suffix appended so they can be imported.
const RESERVED_PROFILE_NAMES: &[&str] = &["claude", "codex", "qoderclicn", "profile", "ai", "help"];

/// Return a valid profile name for a scan result.
/// If the name is a reserved keyword, append "-config".
fn sanitize_scan_name(name: &str) -> String {
    if RESERVED_PROFILE_NAMES.contains(&name) {
        format!("{}-config", name)
    } else {
        name.to_string()
    }
}

#[tauri::command]
pub fn scan_system_configs() -> Result<ScanResult, String> {
    let home = home_dir();
    let mut profiles = Vec::new();
    let mut checked = Vec::new();

    // Scan ~/.claude/settings.json → has { "env": { "ANTHROPIC_...": "..." } }
    let claude_path = home.join(".claude").join("settings.json");
    let claude_str = claude_path.display().to_string();
    checked.push(claude_str.clone());
    if let Ok(json) = read_json_file(&claude_path) {
        let mut env = std::collections::HashMap::new();
        if let Some(env_obj) = json.get("env").and_then(|e| e.as_object()) {
            for (k, v) in env_obj {
                if let Some(s) = v.as_str() {
                    env.insert(k.clone(), s.to_string());
                }
            }
        }
        if !env.is_empty() {
            profiles.push(ScanProfile {
                name: sanitize_scan_name("claude"),
                cli_type: "claude".into(),
                env,
                source: claude_str,
            });
        }
    }

    // Scan ~/.codex/auth.json → has { "OPENAI_API_KEY": "sk-..." }
    let codex_auth = home.join(".codex").join("auth.json");
    let codex_auth_str = codex_auth.display().to_string();
    checked.push(codex_auth_str.clone());
    let mut codex_env = std::collections::HashMap::new();
    if let Ok(json) = read_json_file(&codex_auth) {
        if let Some(key) = json.get("OPENAI_API_KEY").and_then(|v| v.as_str()) {
            codex_env.insert("OPENAI_API_KEY".into(), key.to_string());
        }
    }

    // Scan ~/.codex/config.toml for model + base_url
    let codex_config = home.join(".codex").join("config.toml");
    let codex_config_str = codex_config.display().to_string();
    checked.push(codex_config_str.clone());
    if let Ok(content) = std::fs::read_to_string(&codex_config) {
        // Parse TOML manually for key fields
        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("model ") || trimmed.starts_with("model=") {
                if let Some(v) = trimmed.splitn(2, '=').nth(1) {
                    let val = v.trim().trim_matches('\'').trim_matches('"');
                    if !val.is_empty() {
                        codex_env.insert("OPENAI_MODEL".into(), val.to_string());
                    }
                }
            }
            if trimmed.starts_with("base_url ") || trimmed.starts_with("base_url=") {
                if let Some(v) = trimmed.splitn(2, '=').nth(1) {
                    let val = v.trim().trim_matches('\'').trim_matches('"');
                    if !val.is_empty() {
                        codex_env.insert("OPENAI_BASE_URL".into(), val.to_string());
                    }
                }
            }
        }
    }

    if !codex_env.is_empty() {
        profiles.push(ScanProfile {
            name: sanitize_scan_name("codex"),
            cli_type: "codex".into(),
            env: codex_env,
            source: format!("{}, {}", codex_auth_str, codex_config_str),
        });
    }

    // Scan ~/.qoder-cn/ (Qoder CN CLI) — PAT token set via env var, not in config files
    let qoder_dir = home.join(".qoder-cn");
    let qoder_str = qoder_dir.display().to_string();
    checked.push(qoder_str.clone());
    if qoder_dir.exists() {
        let mut qoder_env = std::collections::HashMap::new();
        // Qoder CN uses QODERCN_PERSONAL_ACCESS_TOKEN env var (not stored in config)
        // Try settings.json for any hints, but primarily just mark as installed
        let settings_path = qoder_dir.join("settings.json");
        if let Ok(json) = read_json_file(&settings_path) {
            if let Some(token) = json.get("personalAccessToken").and_then(|v| v.as_str()) {
                qoder_env.insert("QODERCN_PERSONAL_ACCESS_TOKEN".into(), token.to_string());
            }
        }
        // Always include Qoder CN if the directory exists (even without extracted env vars)
        profiles.push(ScanProfile {
            name: sanitize_scan_name("qoder-cn"),
            cli_type: "qoderclicn".into(),
            env: qoder_env,
            source: qoder_str,
        });
    }

    if profiles.is_empty() {
        return Err(format!("未找到配置。\n已检查:\n{}", checked.join("\n")));
    }
    Ok(ScanResult { profiles })
}

#[tauri::command]
pub fn temp_dir() -> String {
    std::env::temp_dir().display().to_string()
}

#[derive(Debug, serde::Serialize)]
pub struct PlatformInfo {
    pub os: String,   // "macos", "windows", "linux"
    pub arch: String, // "aarch64", "x86_64"
}

#[tauri::command]
pub fn is_debug_build() -> bool {
    cfg!(debug_assertions)
}

#[tauri::command]
pub fn get_platform_info() -> PlatformInfo {
    PlatformInfo {
        os: if cfg!(target_os = "macos") {
            "macos".into()
        } else if cfg!(target_os = "windows") {
            "windows".into()
        } else {
            "linux".into()
        },
        arch: if cfg!(target_arch = "aarch64") {
            "aarch64".into()
        } else {
            "x86_64".into()
        },
    }
}

#[tauri::command]
pub fn get_app_version(app: tauri::AppHandle) -> String {
    app.config()
        .version
        .clone()
        .unwrap_or_else(|| "0.0.0".into())
}

/// Find a system binary across common platform paths
pub(crate) fn find_binary(names: &[&str]) -> Option<String> {
    for name in names {
        // Try full paths first
        let paths: Vec<String> = if cfg!(target_os = "macos") {
            vec![
                format!("/usr/bin/{}", name),
                format!("/opt/homebrew/bin/{}", name),
                format!("/usr/local/bin/{}", name),
            ]
        } else if cfg!(target_os = "linux") {
            vec![
                format!("/usr/bin/{}", name),
                format!("/bin/{}", name),
                format!("/usr/local/bin/{}", name),
            ]
        } else {
            let home = crate::home_dir();
            let local_appdata = std::env::var("LOCALAPPDATA")
                .unwrap_or_else(|_| home.join("AppData").join("Local").to_string_lossy().to_string());
            windows_binary_candidates(&local_appdata, name)
        };
        for p in &paths {
            if std::path::Path::new(p).exists() {
                return Some(p.clone());
            }
        }
    }
    // Fallback 2: check login-shell PATH (catches ~/.local/bin, Homebrew, etc.)
    for name in names {
        if let Some(path) = resolve_from_shell_path(name) {
            return Some(path);
        }
    }
    // Final fallback: bare command name (relies on system PATH)
    names.first().map(|n| n.to_string())
}

fn windows_binary_candidates(local_appdata: &str, name: &str) -> Vec<String> {
    let system32 = std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".into());
    let home = crate::home_dir();
    let home_str = home.to_string_lossy();
    let bases = [
        format!(r"{}\System32", system32),
        format!(r"{}\scoop\shims", home_str),
        format!(r"{}\AppData\Roaming\npm", home_str),
        format!(r"{}\Programs", local_appdata),
        r"C:\Program Files\PowerShell\7".to_string(),
        r"C:\ProgramData\chocolatey\bin".to_string(),
        r"C:\msys64\usr\bin".to_string(),
        r"C:\cygwin64\bin".to_string(),
        r"C:\Program Files".to_string(),
        r"C:\Program Files (x86)".to_string(),
    ];
    let exts: &[&str] = if name.ends_with(".exe")
        || name.ends_with(".cmd")
        || name.ends_with(".bat")
    {
        &[""]
    } else {
        &[".exe", ".cmd", ""]
    };
    let mut paths = Vec::new();
    for base in &bases {
        for ext in exts {
            paths.push(if ext.is_empty() {
                format!(r"{}\{}", base, name)
            } else {
                format!(r"{}\{}{}", base, name, ext)
            });
        }
    }
    paths
}

fn login_shell_for_path_lookup() -> String {
    if cfg!(target_os = "windows") {
        powershell_exe_path()
    } else {
        std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string())
    }
}

/// Resolve a binary name via login-shell PATH lookup.
/// Tauri GUI apps have a minimal PATH; the login shell has the user's full PATH.
fn resolve_from_shell_path(name: &str) -> Option<String> {
    let output = if cfg!(target_os = "windows") {
        std::process::Command::new(login_shell_for_path_lookup())
            .args([
                "-NoProfile",
                "-Command",
                &format!(
                    "Get-Command {} -CommandType Application -ErrorAction SilentlyContinue | ForEach-Object Source",
                    name
                ),
            ])
            .output()
            .ok()?
    } else {
        let shell = login_shell_for_path_lookup();
        // Use `command -v` first (POSIX standard, clean output).
        // Fall back to `type` (bash/zsh builtin) but filter error messages.
        let cmd = format!(
            "command -v {} 2>/dev/null || (type {} 2>/dev/null | grep -v 'not found')",
            name, name
        );
        std::process::Command::new(&shell)
            .args(["-lc", &cmd])
            .output()
            .ok()?
    };
    let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
    // Filter out shell error messages that leak to stdout (e.g. zsh `type` output)
    if path.is_empty() || path.contains("not found") {
        None
    } else {
        Some(path)
    }
}

#[tauri::command]
pub async fn fetch_url(url: String) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        reqwest::blocking::Client::new()
            .get(&url)
            .timeout(Duration::from_secs(30))
            .send()
            .map_err(|e| format!("请求失败: {}", e))?
            .error_for_status()
            .map_err(|e| format!("HTTP 错误: {}", e))?
            .text()
            .map_err(|e| format!("读取响应失败: {}", e))
    })
    .await
    .map_err(|e| format!("后台任务失败: {}", e))?
}

#[tauri::command]
pub async fn download_file(url: String, path: String, app: tauri::AppHandle) -> Result<(), String> {
    // Safety: restrict write destination to home dir or temp dir
    if !is_safe_path(std::path::Path::new(&path)) {
        return Err("不允许下载到此路径".into());
    }
    tauri::async_runtime::spawn_blocking(move || {
        use std::io::{Read, Write};

        let mut response = reqwest::blocking::Client::builder()
            .connect_timeout(Duration::from_secs(30))
            .timeout(Duration::from_secs(3600))
            .build()
            .map_err(|e| format!("创建 HTTP 客户端失败: {}", e))?
            .get(&url)
            .send()
            .map_err(|e| format!("请求失败: {}", e))?
            .error_for_status()
            .map_err(|e| format!("HTTP 错误: {}", e))?;

        let total = response.content_length();

        let mut file =
            std::fs::File::create(&path).map_err(|e| format!("创建文件失败: {}", e))?;

        let mut downloaded: u64 = 0;
        let mut last_pct: u8 = 0;
        let mut buf = [0u8; 8192];

        // Stream response body, writing to file and emitting
        // progress events based on Content-Length when available.
        loop {
            let n = response
                .read(&mut buf)
                .map_err(|e| format!("下载失败: {}", e))?;
            if n == 0 {
                break;
            }
            file.write_all(&buf[..n])
                .map_err(|e| format!("写入文件失败: {}", e))?;
            downloaded += n as u64;

            if let Some(total) = total {
                if total > 0 {
                    let pct =
                        ((downloaded as f64 / total as f64) * 100.0).min(99.0) as u8;
                    if pct != last_pct {
                        last_pct = pct;
                        let _ = app.emit("download-progress", pct);
                    }
                }
            }
        }

        file.flush().map_err(|e| format!("刷新文件失败: {}", e))?;
        file.sync_all().map_err(|e| format!("同步文件失败: {}", e))?;

        // Emit 100% at completion to ensure the UI shows done
        let _ = app.emit("download-progress", 100u8);
        Ok(())
    })
    .await
    .map_err(|e| format!("后台任务失败: {}", e))?
}

#[tauri::command]
pub fn verify_sha256(path: String, expected: String) -> Result<bool, String> {
    let mut file = std::fs::File::open(&path).map_err(|e| format!("无法打开文件: {}", e))?;
    let mut hasher = Sha256::new();
    std::io::copy(&mut file, &mut hasher).map_err(|e| format!("读取文件失败: {}", e))?;
    let actual = format!("{:x}", hasher.finalize());
    Ok(actual == expected.to_lowercase())
}

#[tauri::command]
pub fn open_in_terminal(path: String) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        // Try common macOS terminals: iTerm2, Warp, then default Terminal.app
        let mut spawned = false;
        for app in &["iTerm", "Warp", "Terminal"] {
            if std::process::Command::new("open")
                .args(["-a", app, &path])
                .spawn()
                .is_ok()
            {
                spawned = true;
                break;
            }
        }
        if !spawned {
            return Err("未找到可用的终端应用 (iTerm/Warp/Terminal)".into());
        }
    }
    #[cfg(target_os = "linux")]
    {
        // Common modern terminals ordered by popularity,
        // each with appropriate working-directory flag
        let terminals: &[(&str, &[&str])] = &[
            ("gnome-terminal", &["--working-directory"]),
            ("konsole", &["--workdir"]),
            ("alacritty", &["--working-directory"]),
            ("kitty", &["--directory"]),
            ("wezterm", &["start", "--cwd"]),
            ("foot", &["--working-directory"]),
            ("tilix", &["--working-directory"]),
            ("xfce4-terminal", &["--working-directory"]),
            ("lxterminal", &["--working-directory"]),
            ("terminator", &["--working-directory"]),
            ("xterm", &["-e"]), // xterm uses -e + cd combo
        ];
        let mut spawned = false;
        for (term, workdir_flag) in terminals {
            let mut cmd = std::process::Command::new(term);
            for flag in *workdir_flag {
                cmd.arg(flag);
            }
            // xterm needs `cd <path> ; exec $SHELL` via -e
            if *term == "xterm" {
                cmd.arg(format!("cd '{}' ; exec $SHELL", path));
            } else {
                cmd.arg(&path);
            }
            if cmd.spawn().is_ok() {
                spawned = true;
                break;
            }
        }
        if !spawned {
            return Err("未找到可用的终端模拟器".into());
        }
    }
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("cmd")
            .args(["/c", "start", "cmd", "/k", &format!("cd /d \"{}\"", path)])
            .spawn()
            .map_err(|e| format!("打开终端失败: {}", e))?;
    }
    Ok(())
}

#[tauri::command]
pub fn open_file(path: String) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("/usr/bin/open")
            .arg(&path)
            .spawn()
            .map_err(|e| format!("{}", e))?;
    }
    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open")
            .arg(&path)
            .spawn()
            .map_err(|e| format!("{}", e))?;
    }
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("cmd")
            .args(["/c", "start", "", &path])
            .spawn()
            .map_err(|e| format!("{}", e))?;
    }
    Ok(())
}

/// Open a path in an external editor or file manager.
/// `editor`: "code" (VS Code), "cursor" (Cursor), "idea" (IntelliJ IDEA), "terminal", "finder" (file manager)
#[tauri::command]
pub fn open_in_editor(path: String, editor: String) -> Result<(), String> {
    match editor.as_str() {
        "code" => open_with_editor(&path, "code", &["code", "code.cmd"]),
        "cursor" => open_with_editor(&path, "cursor", &["cursor", "cursor.cmd"]),
        "idea" => {
            #[cfg(target_os = "macos")]
            {
                // Try `idea` CLI first, fall back to `open -a`
                if let Some(bin) = crate::commands::find_binary(&["idea"]) {
                    std::process::Command::new(&bin).arg(&path).spawn()
                        .map_err(|e| format!("启动 IntelliJ IDEA 失败: {}", e))?;
                } else {
                    std::process::Command::new("open")
                        .args(["-a", "IntelliJ IDEA", &path])
                        .spawn()
                        .map_err(|e| format!("启动 IntelliJ IDEA 失败: {}", e))?;
                }
            }
            #[cfg(not(target_os = "macos"))]
            {
                open_with_editor(&path, "IntelliJ IDEA", &["idea", "idea64"])?;
            }
            Ok(())
        }
        "terminal" => crate::commands::open_in_terminal(path),
        "finder" => crate::commands::open_file(path),
        _ => Err(format!("不支持的编辑器: {}", editor)),
    }
}

/// Try to open a path with a given editor binary.
fn open_with_editor(path: &str, name: &str, binaries: &[&str]) -> Result<(), String> {
    let binary = crate::commands::find_binary(binaries)
        .ok_or_else(|| format!("未找到 {}，请先安装", name))?;
    std::process::Command::new(&binary)
        .arg(path)
        .spawn()
        .map_err(|e| format!("启动 {} 失败: {}", name, e))?;
    Ok(())
}

/// Shared list of Git Bash installation paths used by both PTY shell
/// resolution and environment check (Claude Code runtime dependency).
pub(crate) fn git_bash_candidates(home: &str, local_appdata: &str) -> Vec<String> {
    let mut candidates = vec![
        r"C:\Program Files\Git\bin\bash.exe".into(),
        r"C:\Program Files (x86)\Git\bin\bash.exe".into(),
        format!(r"{}\AppData\Local\Programs\Git\bin\bash.exe", home),
        r"C:\scoop\apps\git\current\bin\bash.exe".into(),
        r"C:\ProgramData\Git\bin\bash.exe".into(),
    ];
    if !local_appdata.is_empty() {
        candidates.push(format!(
            r"{}\Microsoft\WinGet\Links\bash.exe",
            local_appdata
        ));
    }
    candidates
}

/// Check whether a bash.exe or pwsh.exe (PowerShell 7) binary can be found.
/// Returns the found path if any. Used to verify Claude Code runtime
/// dependencies on Windows.
pub(crate) fn find_bash_or_pwsh() -> Option<String> {
    let home = crate::home_dir().to_string_lossy().to_string();
    let local_appdata = std::env::var("LOCALAPPDATA").unwrap_or_default();
    // Check Git Bash candidates first, then pwsh
    for p in git_bash_candidates(&home, &local_appdata) {
        if std::path::Path::new(&p).exists() {
            return Some(p);
        }
    }
    // Try pwsh (PowerShell 7) via find_binary
    find_binary(&["pwsh", "pwsh.exe"])
}

/// Full path to the system PowerShell binary (Windows PowerShell 5.1).
/// Uses `SystemRoot` so it works even when Windows is not on C:.
///
/// Always prefer this over a bare `"powershell.exe"` name — Tauri GUI apps
/// may have a limited PATH that excludes the PowerShell subdirectory.
pub(crate) fn powershell_exe_path() -> String {
    let sysroot = std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".into());
    format!(
        r"{}\System32\WindowsPowerShell\v1.0\powershell.exe",
        sysroot
    )
}

// ── Environment check (for onboarding) ───────────────────────

#[derive(Debug, serde::Serialize)]
pub struct InstallOption {
    pub id: String,
    pub label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    pub description: String,
    pub recommended: bool,
    pub platforms: Vec<String>,
}

#[derive(Debug, serde::Serialize)]
pub struct EnvCheckItem {
    pub name: String,
    pub label: String,
    pub status: String,   // "ok" | "warn" | "missing"
    pub severity: String, // "ok" | "info" | "warn" | "error"
    pub category: String, // "cli" | "shell" | "config"
    pub detail: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detected_path: Option<String>,
    /// CLI version detected from package.json or --version
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub install_options: Option<Vec<InstallOption>>,
    /// Executable install command (only populated when status == "missing")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub install_cmd: Option<String>,
}

#[derive(Debug, serde::Serialize)]
pub struct EnvCheckResult {
    pub items: Vec<EnvCheckItem>,
    pub all_ok: bool,
}

fn check_binary_on_path(name: &str) -> Option<String> {
    // Try full paths first (fast, no shell overhead)
    if let Some(path) = find_binary(&[name]) {
        // Only accept paths that are absolute (start with /) or at least
        // contain a path separator — bare names from the final fallback
        // (e.g. "codex") are not real paths and must be resolved via shell.
        let p = std::path::Path::new(&path);
        if (p.is_absolute() || path.contains('/') || path.contains('\\')) && p.exists() {
            return Some(path);
        }
    }
    // Use login shell to resolve user PATH (brew, npx, etc.)
    let shell = login_shell_for_path_lookup();
    let shell_args: &[&str] = if cfg!(target_os = "windows") {
        &["-Command", &format!("Get-Command {} -CommandType Application -ErrorAction SilentlyContinue | ForEach-Object Source", name)]
    } else {
        &[
            "-lc",
            &format!(
                "command -v {} 2>/dev/null || (type {} 2>/dev/null | grep -v 'not found')",
                name, name
            ),
        ]
    };
    if let Ok(output) = std::process::Command::new(&shell).args(shell_args).output() {
        let s = String::from_utf8_lossy(&output.stdout).trim().to_string();
        // Filter out shell error messages that leak to stdout (e.g. zsh `type` output)
        if !s.is_empty() && !s.contains("not found") {
            return Some(s);
        }
    }
    None
}

/// Try to detect a CLI's version without running it.
///
/// Strategy 1 (fast, no subprocess): canonicalize the binary path, walk up
/// the directory tree to find `package.json`, and read the `version` field.
/// This covers all npm-based installs (Homebrew, nvm, fnm, Volta, pnpm, etc.).
///
/// Strategy 2 (fallback): run `<binary> --version` with a 3-second timeout.
/// Catches standalone binaries and non-npm installs.
fn get_cli_version(binary_path: &str) -> Option<String> {
    // Strategy 1 — resolve symlink → find a sibling package.json
    if let Ok(real) = std::fs::canonicalize(binary_path) {
        let mut dir = real.parent().map(|p| p.to_path_buf());
        while let Some(current) = dir {
            let pkg = current.join("package.json");
            if pkg.exists() {
                if let Ok(contents) = std::fs::read_to_string(&pkg) {
                    // Quick extraction: find "version" key without pulling in a
                    // full JSON parser. package.json is small and this avoids
                    // another dependency.
                    if let Some(ver) = extract_version_from_json(&contents) {
                        return Some(ver);
                    }
                }
            }
            // Stop walking at filesystem root — don't cross into /
            if current.parent().is_none() || current.as_os_str().is_empty() {
                break;
            }
            dir = current.parent().map(|p| p.to_path_buf());
        }
    }

    // Strategy 2 — run --version (3 second timeout)
    if let Ok(output) = std::process::Command::new(binary_path)
        .arg("--version")
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .output()
    {
        if output.status.success() {
            let s = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !s.is_empty() {
                return Some(s);
            }
        }
    }

    None
}

/// Extract the "version" field value from a package.json string slice.
/// Uses a simple scan to avoid a full JSON parse dependency.
fn extract_version_from_json(json: &str) -> Option<String> {
    for line in json.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("\"version\"") {
            // Expect: "version": "1.2.3"  or  "version":"1.2.3"
            let after_key = rest.trim_start();
            if let Some(val_part) = after_key.strip_prefix(':') {
                let val = val_part.trim();
                // Strip trailing comma BEFORE unwrapping quotes
                // ("version": "1.2.3", → val = "\"1.2.3\"," → strip comma → "\"1.2.3\"")
                let val = val.strip_suffix(',').unwrap_or(val);
                // Unwrap surrounding quotes
                if val.len() >= 2 && val.starts_with('"') && val.ends_with('"') {
                    let inner = &val[1..val.len() - 1];
                    if !inner.is_empty() {
                        return Some(inner.to_string());
                    }
                }
            }
        }
    }
    None
}

fn current_platform_id() -> String {
    if cfg!(target_os = "macos") {
        "macos".into()
    } else if cfg!(target_os = "windows") {
        "windows".into()
    } else {
        "linux".into()
    }
}

fn install_option(
    id: &str,
    label: &str,
    command: Option<&str>,
    description: &str,
    recommended: bool,
    platforms: &[&str],
) -> InstallOption {
    InstallOption {
        id: id.into(),
        label: label.into(),
        command: command.map(|c| c.into()),
        description: description.into(),
        recommended,
        platforms: platforms.iter().map(|p| (*p).into()).collect(),
    }
}

fn cli_install_options(name: &str) -> Vec<InstallOption> {
    let platform = current_platform_id();
    match name {
        "claude" => {
            let script_supported = platform != "windows";
            vec![
                install_option(
                    if script_supported {
                        "official-script"
                    } else {
                        "npm"
                    },
                    if script_supported {
                        "官方脚本"
                    } else {
                        "npm 全局安装"
                    },
                    Some(if script_supported {
                        "curl -fsSL https://claude.ai/install.sh | bash"
                    } else {
                        "npm i -g @anthropic-ai/claude-code"
                    }),
                    if script_supported {
                        "推荐用于 macOS/Linux，不假设 npm 全局目录。"
                    } else {
                        "推荐用于 Windows。"
                    },
                    true,
                    &[&platform],
                ),
                install_option(
                    if script_supported {
                        "npm"
                    } else {
                        "official-docs"
                    },
                    if script_supported {
                        "npm 全局安装"
                    } else {
                        "官方安装说明"
                    },
                    if script_supported {
                        Some("npm i -g @anthropic-ai/claude-code")
                    } else {
                        None
                    },
                    if script_supported {
                        "适合已经使用 Node/npm 管理 CLI 的用户。"
                    } else {
                        "如不使用 npm，请按 Claude Code 官方文档安装。"
                    },
                    false,
                    &[&platform],
                ),
            ]
        }
        "codex" => {
            let mut options = vec![install_option(
                "npm",
                "npm 全局安装",
                Some("npm i -g @openai/codex"),
                "适合已有 Node/npm 环境的用户。",
                true,
                &[&platform],
            )];
            if platform != "windows" {
                options.push(install_option(
                    "homebrew",
                    "Homebrew 安装",
                    Some("brew install codex"),
                    "适合通过 Homebrew 管理 CLI 的 macOS/Linux 用户。",
                    false,
                    &[&platform],
                ));
            }
            options.push(install_option(
                "manual",
                "手动安装",
                None,
                "如使用 pnpm、公司镜像或其他包管理器，请按你的环境安装并确保 codex 在 PATH 中。",
                false,
                &[&platform],
            ));
            options
        }
        "qoderclicn" => vec![
            install_option(
                "npm",
                "npm 全局安装",
                Some("npm i -g @qodercn-ai/qoderclicn"),
                "沿用当前应用支持的 qoderclicn 命令名。",
                true,
                &[&platform],
            ),
            install_option(
                "manual",
                "官方/手动安装",
                None,
                "如你通过 Qoder 官方安装器或其他渠道安装，请确保 qoderclicn 在 PATH 中。",
                false,
                &[&platform],
            ),
        ],
        _ => Vec::new(),
    }
}

fn recommended_install_cmd(options: &[InstallOption]) -> Option<String> {
    options
        .iter()
        .find(|o| o.recommended)
        .and_then(|o| o.command.clone())
        .or_else(|| options.iter().find_map(|o| o.command.clone()))
}

fn cli_ok_item(name: &str, label: &str, path: String, version: Option<String>) -> EnvCheckItem {
    let detail = version.as_deref().unwrap_or(&path);
    EnvCheckItem {
        name: name.into(),
        label: label.into(),
        status: "ok".into(),
        severity: "ok".into(),
        category: "cli".into(),
        detail: detail.to_string(),
        detected_path: Some(path),
        version,
        install_options: None,
        install_cmd: None,
    }
}

fn cli_missing_item(name: &str, label: &str) -> EnvCheckItem {
    let options = cli_install_options(name);
    let install_cmd = recommended_install_cmd(&options);
    let detail = match install_cmd.as_deref() {
        Some(cmd) => format!("未安装，推荐: {}", cmd),
        None => "未安装，请按官方说明安装并加入 PATH".into(),
    };

    EnvCheckItem {
        name: name.into(),
        label: label.into(),
        status: "missing".into(),
        severity: "warn".into(),
        category: "cli".into(),
        detail,
        detected_path: None,
        version: None,
        install_options: if options.is_empty() {
            None
        } else {
            Some(options)
        },
        install_cmd,
    }
}

/// Build a warn item when a CLI binary is found but its Windows runtime
/// dependency (bash.exe / pwsh.exe) is missing.
fn cli_no_runtime_item(
    name: &str,
    label: &str,
    path: String,
    version: Option<String>,
) -> EnvCheckItem {
    let detail = format!(
        "已安装但缺少运行时依赖。{} 在 Windows 上需要 Git Bash 或 PowerShell 7。\n\
         安装后请重启 KN。\n\
         Git for Windows: https://git-scm.com/downloads/win\n\
         PowerShell 7: https://aka.ms/powershell",
        label
    );
    EnvCheckItem {
        name: name.into(),
        label: label.into(),
        status: "warn".into(),
        severity: "warn".into(),
        category: "cli".into(),
        detail,
        detected_path: Some(path),
        version,
        install_options: Some(vec![
            install_option(
                "git-bash",
                "安装 Git for Windows",
                None,
                "提供 bash.exe，同时解决 KN 终端和 CLI 运行时依赖。",
                true,
                &["windows"],
            ),
            install_option(
                "powershell-7",
                "安装 PowerShell 7",
                None,
                "提供 pwsh.exe，Node.js CLI 原生支持。",
                false,
                &["windows"],
            ),
        ]),
        install_cmd: None,
    }
}

/// Check if a CLI binary is installed, and on Windows verify runtime
/// dependencies (bash.exe / pwsh.exe) are also present.
fn push_cli_item(items: &mut Vec<EnvCheckItem>, binary: &str, label: &str) {
    match check_binary_on_path(binary) {
        Some(path) => {
            let version = get_cli_version(&path);
            if cfg!(target_os = "windows") && find_bash_or_pwsh().is_none() {
                items.push(cli_no_runtime_item(binary, label, path, version));
            } else {
                items.push(cli_ok_item(binary, label, path, version));
            }
        }
        None => items.push(cli_missing_item(binary, label)),
    }
}

#[tauri::command]
pub fn check_environment() -> EnvCheckResult {
    let home = home_dir();
    let mut items = Vec::new();

    // 1. Claude Code
    push_cli_item(&mut items, "claude", "Claude Code");

    // 2. Codex
    push_cli_item(&mut items, "codex", "Codex");

    // 3. Qoder
    push_cli_item(&mut items, "qoderclicn", "Qoder");

    // 6. Shell wrapper
    let kn_dir = home.join(".kn");
    let shell_rc = kn_dir.join("shell-rc");
    let shell_rc_ps1 = kn_dir.join("shell-rc.ps1");
    if shell_rc.exists() || shell_rc_ps1.exists() {
        let in_rc = if let Ok(zshrc) = std::fs::read_to_string(home.join(".zshrc")) {
            zshrc.contains(".kn/shell-rc") || zshrc.contains(".claude-profiles")
        } else {
            false
        };
        // On non-macOS also check .bashrc (Linux + Windows Git Bash)
        let in_bashrc = !cfg!(target_os = "macos") && {
            if let Ok(bashrc) = std::fs::read_to_string(home.join(".bashrc")) {
                bashrc.contains(".kn/shell-rc") || bashrc.contains(".claude-profiles")
            } else {
                false
            }
        };
        // On Windows also check PowerShell profile for shell-rc.ps1
        let in_ps_profile = cfg!(target_os = "windows") && {
            let docs = home.join("Documents");
            let ps7 = docs
                .join("PowerShell")
                .join("Microsoft.PowerShell_profile.ps1");
            let ps5 = docs
                .join("WindowsPowerShell")
                .join("Microsoft.PowerShell_profile.ps1");
            let check = |p: &std::path::Path| -> bool {
                std::fs::read_to_string(p)
                    .map(|c| c.contains(".kn/shell-rc") || c.contains(".claude-profiles"))
                    .unwrap_or(false)
            };
            check(&ps7) || check(&ps5)
        };
        let activated = in_rc || in_bashrc || in_ps_profile;
        items.push(EnvCheckItem {
            name: "shell-wrapper".into(),
            label: "Shell 集成".into(),
            status: if activated {
                "ok".into()
            } else {
                "warn".into()
            },
            severity: if activated {
                "ok".into()
            } else {
                "warn".into()
            },
            category: "shell".into(),
            detail: if activated {
                "已激活".into()
            } else {
                "已安装但未激活".into()
            },
            detected_path: Some(if shell_rc.exists() {
                shell_rc.display().to_string()
            } else {
                shell_rc_ps1.display().to_string()
            }),
            version: None,
            install_options: None,
            install_cmd: None,
        });
    } else {
        // Fallback: also check legacy ~/.claude-profiles/ for users who haven't migrated
        let legacy_dir = home.join(".claude-profiles");
        let legacy_rc = legacy_dir.join("shell-rc");
        let legacy_rc_ps1 = legacy_dir.join("shell-rc.ps1");
        if legacy_rc.exists() || legacy_rc_ps1.exists() {
            let in_rc = if let Ok(zshrc) = std::fs::read_to_string(home.join(".zshrc")) {
                zshrc.contains("shell-rc")
            } else {
                false
            };
            let in_bashrc = !cfg!(target_os = "macos") && {
                if let Ok(bashrc) = std::fs::read_to_string(home.join(".bashrc")) {
                    bashrc.contains("shell-rc")
                } else {
                    false
                }
            };
            let activated = in_rc || in_bashrc;
            items.push(EnvCheckItem {
                name: "shell-wrapper".into(),
                label: "Shell 集成".into(),
                status: if activated { "ok".into() } else { "warn".into() },
                severity: if activated { "ok".into() } else { "warn".into() },
                category: "shell".into(),
                detail: if activated {
                    "已激活（旧目录 ~/.claude-profiles/，建议迁移）".into()
                } else {
                    "已安装但未激活（旧目录 ~/.claude-profiles/）".into()
                },
                detected_path: Some(if legacy_rc.exists() {
                    legacy_rc.display().to_string()
                } else {
                    legacy_rc_ps1.display().to_string()
                }),
                version: None,
                install_options: None,
                install_cmd: None,
            });
        } else {
            items.push(EnvCheckItem {
                name: "shell-wrapper".into(),
                label: "Shell 集成".into(),
                status: "missing".into(),
                severity: "warn".into(),
                category: "shell".into(),
                detail: if cfg!(target_os = "windows") {
                    "未安装，应用启动时会尝试自动写入 PowerShell 集成".into()
                } else {
                    "未安装，应用启动时会尝试自动写入 shell 集成".into()
                },
                detected_path: None,
                version: None,
                install_options: None,
                install_cmd: None,
            });
        }
    }

    // 7. Config directory
    let config_dir = home.join(".kn");
    let config_file = config_dir.join("config.yaml");
    // Also check legacy location for migration hint
    let legacy_config_dir = home.join(".claude-profiles");
    let legacy_config_file = legacy_config_dir.join("config.yaml");
    if config_dir.exists() {
        if config_file.exists() {
            items.push(EnvCheckItem {
                name: "config".into(),
                label: "配置文件".into(),
                status: "ok".into(),
                severity: "ok".into(),
                category: "config".into(),
                detail: config_file.display().to_string(),
                detected_path: Some(config_file.display().to_string()),
                version: None,
                install_options: None,
                install_cmd: None,
            });
        } else {
            items.push(EnvCheckItem {
                name: "config".into(),
                label: "配置文件".into(),
                status: "warn".into(),
                severity: "warn".into(),
                category: "config".into(),
                detail: "目录存在但无配置文件".into(),
                detected_path: Some(config_dir.display().to_string()),
                version: None,
                install_options: None,
                install_cmd: None,
            });
        }
        // If legacy dir still exists and has config, suggest cleanup
        if legacy_config_file.exists() {
            items.push(EnvCheckItem {
                name: "config-legacy".into(),
                label: "旧配置文件".into(),
                status: "info".into(),
                severity: "info".into(),
                category: "config".into(),
                detail: format!("旧目录仍存在: {}，建议迁移后清理", legacy_config_dir.display()),
                detected_path: Some(legacy_config_file.display().to_string()),
                version: None,
                install_options: None,
                install_cmd: None,
            });
        }
    } else if legacy_config_dir.exists() && legacy_config_file.exists() {
        // Legacy config still present, hasn't been migrated
        items.push(EnvCheckItem {
            name: "config".into(),
            label: "配置文件".into(),
            status: "warn".into(),
            severity: "warn".into(),
            category: "config".into(),
            detail: format!("旧目录: {} → 重启应用将自动迁移到 ~/.kn/", legacy_config_dir.display()),
            detected_path: Some(legacy_config_file.display().to_string()),
            version: None,
            install_options: None,
            install_cmd: None,
        });
    } else {
        items.push(EnvCheckItem {
            name: "config".into(),
            label: "配置文件".into(),
            status: "missing".into(),
            severity: "warn".into(),
            category: "config".into(),
            detail: "目录不存在".into(),
            detected_path: None,
            version: None,
            install_options: None,
            install_cmd: None,
        });
    }

    let all_ok = items.iter().all(|i| i.status == "ok");

    EnvCheckResult { items, all_ok }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── find_binary ────────────────────────────────────────

    #[test]
    fn test_find_binary_known_command() {
        // "sh" should exist on all Unix-like systems
        let path = find_binary(&["sh"]);
        assert!(path.is_some(), "find_binary should find 'sh'");
        let path = path.unwrap();
        assert!(
            std::path::Path::new(&path).exists(),
            "returned path should exist"
        );
    }

    #[test]
    fn test_find_binary_nonexistent() {
        // Should return None for clearly nonexistent binary
        // (may still resolve via bare name fallback, but the path won't exist)
        let path = find_binary(&["nonexistent_binary_xyz_12345"]);
        // The bare-name fallback may still return a string, but it shouldn't
        // resolve to an actual file on disk
        if let Some(p) = &path {
            assert!(
                !std::path::Path::new(p).exists(),
                "bare-name fallback for nonexistent binary should not resolve to real file"
            );
        }
    }

    #[test]
    fn test_cli_missing_item_has_recommended_install_option() {
        let item = cli_missing_item("codex", "Codex");
        assert_eq!(item.status, "missing");
        assert_eq!(item.severity, "warn");
        assert_eq!(item.category, "cli");
        assert!(item.install_cmd.is_some());
        assert!(item
            .install_options
            .as_ref()
            .unwrap()
            .iter()
            .any(|o| o.recommended && o.command.is_some()));
    }

    #[test]
    fn test_qoder_install_command_is_consistent() {
        let item = cli_missing_item("qoderclicn", "Qoder");
        assert_eq!(
            item.install_cmd.as_deref(),
            Some("npm i -g @qodercn-ai/qoderclicn")
        );
    }

    #[test]
    fn test_login_shell_for_path_lookup_prefers_sh_on_unix() {
        let _guard = crate::TEST_ENV_LOCK.lock().unwrap();
        let old_shell = std::env::var_os("SHELL");
        std::env::remove_var("SHELL");
        assert_eq!(login_shell_for_path_lookup(), "/bin/sh");
        if let Some(value) = old_shell {
            std::env::set_var("SHELL", value);
        } else {
            std::env::remove_var("SHELL");
        }
    }

    #[test]
    fn test_windows_binary_candidates_include_powershell_7_path() {
        let home = r"C:\Users\Alice";
        let candidates = super::windows_binary_candidates(
            &format!(r"{}\AppData\Local", home),
            "pwsh.exe",
        );
        assert!(candidates.iter().any(|p| p == r"C:\Program Files\PowerShell\7\pwsh.exe"));
    }

    #[test]
    fn test_git_bash_candidates_include_user_localappdata_install() {
        let candidates = git_bash_candidates(
            r"C:\Users\Alice",
            r"C:\Users\Alice\AppData\Local",
        );
        assert!(candidates.iter().any(|p| {
            p == r"C:\Users\Alice\AppData\Local\Programs\Git\bin\bash.exe"
        }));
    }

    #[test]
    fn test_powershell_exe_path_uses_systemroot() {
        let _guard = crate::TEST_ENV_LOCK.lock().unwrap();
        let old = std::env::var_os("SystemRoot");
        std::env::set_var("SystemRoot", r"D:\Windows");
        assert_eq!(
            powershell_exe_path(),
            r"D:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe"
        );
        if let Some(value) = old {
            std::env::set_var("SystemRoot", value);
        } else {
            std::env::remove_var("SystemRoot");
        }
    }

    // ── verify_sha256 ──────────────────────────────────────

    #[test]
    fn test_verify_sha256_correct_hash() {
        let dir = std::env::temp_dir().join(format!("kn-test-sha256-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file_path = dir.join("test.bin");
        std::fs::write(&file_path, b"hello world\n").unwrap();

        // sha256 of "hello world\n"
        let expected = "a948904f2f0f479b8f8197694b30184b0d2ed1c1cd2a1ec0fb85d299a192a447";
        let result = verify_sha256(
            file_path.to_string_lossy().to_string(),
            expected.to_string(),
        )
        .unwrap();
        assert!(result, "verify_sha256 should return true for correct hash");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_verify_sha256_wrong_hash() {
        let dir = std::env::temp_dir().join(format!("kn-test-sha256-wrong-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file_path = dir.join("test.bin");
        std::fs::write(&file_path, b"hello world\n").unwrap();

        let result = verify_sha256(
            file_path.to_string_lossy().to_string(),
            "0000000000000000000000000000000000000000000000000000000000000000".to_string(),
        )
        .unwrap();
        assert!(!result, "verify_sha256 should return false for wrong hash");

        std::fs::remove_dir_all(&dir).ok();
    }

    // ── home_dir ───────────────────────────────────────────

    #[test]
    fn test_home_dir_returns_existing_path() {
        let home = crate::home_dir();
        assert!(
            home.exists(),
            "home_dir should return an existing path: {:?}",
            home
        );
        assert!(home.is_absolute(), "home_dir should return absolute path");
    }

    // ── config backup ──────────────────────────────────────

    #[test]
    fn test_backup_file_path() {
        let bak = backup_file();
        assert!(
            bak.ends_with("config.yaml.bak"),
            "backup_file should end with config.yaml.bak, got: {:?}",
            bak
        );
    }

    #[test]
    fn test_config_backup_exists_non_panic() {
        // Should not panic even when config doesn't exist
        let _ = config_backup_exists();
    }

    // ── Profile CRUD ────────────────────────────────────────

    fn temp_config_setup() -> (std::sync::MutexGuard<'static, ()>, tempfile::TempDir) {
        let guard = crate::TEST_ENV_LOCK.lock().unwrap();
        let dir = tempfile::tempdir().unwrap();
        std::env::set_var(
            "CLAUDE_PROFILES_HOME",
            dir.path().to_string_lossy().to_string(),
        );
        (guard, dir)
    }

    fn cleanup_config(dir: tempfile::TempDir) {
        std::env::remove_var("KN_HOME");
        std::env::remove_var("CLAUDE_PROFILES_HOME");
        drop(dir);
        // CRUD_LOCK guard is dropped by caller
    }

    // ── is_safe_path tests ──

    #[test]
    fn test_is_safe_path_rejects_parent_dir_traversal() {
        let bad = std::path::Path::new("../etc/passwd");
        assert!(!is_safe_path(bad), "parent-dir traversal should be rejected");
    }

    #[test]
    fn test_is_safe_path_rejects_non_existent_path() {
        // canonicalize fails → reject (don't fall back to un-resolved path)
        let bad = std::path::Path::new("/nonexistent-xyz-kn-test-file");
        assert!(!is_safe_path(bad), "non-existent path should be rejected");
    }

    #[test]
    fn test_is_safe_path_allows_home_subdir() {
        let _guard = crate::TEST_ENV_LOCK.lock().unwrap();
        let old_home = std::env::var_os("HOME");
        let tmp_home = tempfile::tempdir().unwrap();
        std::env::set_var("HOME", tmp_home.path());
        let home = home_dir();
        let safe = home.join(".kn-test-safe-path");
        std::fs::create_dir_all(&safe).ok();
        assert!(is_safe_path(&safe), "path under home should be allowed");
        std::fs::remove_dir_all(&safe).ok();
        if let Some(value) = old_home {
            std::env::set_var("HOME", value);
        } else {
            std::env::remove_var("HOME");
        }
    }

    #[test]
    fn test_is_safe_path_allows_temp_dir() {
        let tmp = std::env::temp_dir().join("kn-test-temp-safe");
        std::fs::create_dir_all(&tmp).ok();
        assert!(is_safe_path(&tmp), "path under temp dir should be allowed");
        std::fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn test_crud_flow_full_lifecycle() {
        let (_guard, dir) = temp_config_setup();

        // Add profiles
        assert!(
            add_profile("alpha".into(), Some("first".into()))
                .unwrap()
                .ok
        );
        assert!(add_profile("beta".into(), None).unwrap().ok);
        assert!(
            add_profile("gamma".into(), Some("third".into()))
                .unwrap()
                .ok
        );

        // List
        let list = list_profiles().unwrap();
        assert_eq!(list.profiles.len(), 3);

        // Set env var
        assert!(
            set_env_var("alpha".into(), "KEY1".into(), "val1".into())
                .unwrap()
                .ok
        );
        let detail = show_profile("alpha".into()).unwrap();
        assert_eq!(detail.env.get("KEY1"), Some(&"val1".to_string()));

        // Set default
        assert!(set_default_profile("beta".into()).unwrap().ok);
        assert_eq!(get_default_profile().unwrap(), "beta");

        // Remove non-default
        assert!(remove_profile("gamma".into()).unwrap().ok);
        let list2 = list_profiles().unwrap();
        assert_eq!(list2.profiles.len(), 2);

        // Remove default → alpha should be promoted (alphabetically first)
        assert!(remove_profile("beta".into()).unwrap().ok);
        assert_eq!(get_default_profile().unwrap(), "alpha");

        // Invalid name rejected
        assert!(!add_profile("BAD NAME".into(), None).unwrap().ok);

        cleanup_config(dir);
    }
}
