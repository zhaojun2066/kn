use crate::profile_cmd;
use sha2::{Digest, Sha256};
use std::time::Duration;
use tauri::command;
use tauri::Emitter;

// ── Config backup paths ────────────────────────────────────
fn config_dir() -> std::path::PathBuf {
    if let Ok(dir) = std::env::var("CLAUDE_PROFILES_HOME") {
        return std::path::PathBuf::from(dir);
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
    // Resolve .. and symlinks before checking prefix
    let resolved = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let home = home_dir();
    let tmp = std::env::temp_dir();
    let home_resolved = home.canonicalize().unwrap_or_else(|_| home.clone());
    let tmp_resolved = tmp.canonicalize().unwrap_or_else(|_| tmp.clone());
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

/// Directories & files to skip when building the file tree
const SKIP_NAMES: &[&str] = &[
    ".git",
    "node_modules",
    "__pycache__",
    ".DS_Store",
    ".claude",
    ".codex",
    ".qoder-cn",
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
        if fname.starts_with('.') || SKIP_NAMES.contains(&fname.as_str()) {
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
        home_dir().join(".claude-profiles").join("update.json"),
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
            // Windows: check common system paths with .exe suffix
            let exe_name = if name.ends_with(".exe") {
                name.to_string()
            } else {
                format!("{}.exe", name)
            };
            let system32 = std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".into());
            let home = crate::home_dir();
            let home_str = home.to_string_lossy();
            vec![
                format!(r"{}\System32\{}", system32, exe_name),
                format!(r"{}\scoop\shims\{}", home_str, exe_name),
                format!(r"{}\AppData\Roaming\npm\{}", home_str, exe_name),
                format!(r"C:\Program Files\{}", exe_name),
                format!(r"C:\Program Files (x86)\{}", exe_name),
            ]
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

/// Resolve a binary name via login-shell PATH lookup.
/// Tauri GUI apps have a minimal PATH; the login shell has the user's full PATH.
fn resolve_from_shell_path(name: &str) -> Option<String> {
    let output = if cfg!(target_os = "windows") {
        std::process::Command::new("powershell.exe")
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
        let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".into());
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
    tauri::async_runtime::spawn_blocking(move || {
        let curl = find_binary(&["curl"]).unwrap_or_else(|| "curl".into());
        let mut child = std::process::Command::new(&curl)
            .args(["-L", "--max-time", "600", "-o", &path, &url])
            .stderr(std::process::Stdio::piped())
            .spawn()
            .map_err(|e| format!("curl 启动失败: {}", e))?;

        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| "无法获取 stderr".to_string())?;

        // Read curl stderr for progress.
        //
        // Curl's progress meter format (when stderr is piped / not a TTY):
        //   % Total    % Received % Xferd  ...
        //                                  Dload ...
        //   \r  PERCENT  bytes1  PERCENT2 bytes2  PERCENT3 bytes3 ...
        //
        // The first numeric field after whitespace is the *total* completion
        // percentage (0–100) — a bare integer, **without** a '%' suffix.
        // The '%' symbol only appears in the column headers.
        //
        // We split on \r and extract the leading integer from each fragment,
        // skipping the header and separator lines.
        use std::io::{BufRead, BufReader};
        let reader = BufReader::new(stderr);
        for line_result in reader.split(b'\r') {
            let chunk = line_result.unwrap_or_default();
            let trimmed = String::from_utf8_lossy(&chunk);
            let trimmed = trimmed.trim();
            // Skip headers, separators, and empty lines
            if trimmed.is_empty()
                || trimmed.contains('%')
                || trimmed.contains("Dload")
                || trimmed.contains("----")
            {
                continue;
            }
            // First whitespace-delimited token is the progress percentage
            if let Some(first) = trimmed.split_whitespace().next() {
                if let Ok(pct) = first.parse::<u8>() {
                    let _ = app.emit("download-progress", pct);
                }
            }
        }

        let status = child.wait().map_err(|e| format!("curl 等待失败: {}", e))?;
        if !status.success() {
            return Err("下载失败，请检查网络连接".into());
        }
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
    let shell = if cfg!(target_os = "windows") {
        "powershell.exe".to_string()
    } else {
        std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".to_string())
    };
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

fn cli_ok_item(name: &str, label: &str, path: String) -> EnvCheckItem {
    EnvCheckItem {
        name: name.into(),
        label: label.into(),
        status: "ok".into(),
        severity: "ok".into(),
        category: "cli".into(),
        detail: path.clone(),
        detected_path: Some(path),
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
        install_options: if options.is_empty() {
            None
        } else {
            Some(options)
        },
        install_cmd,
    }
}

#[tauri::command]
pub fn check_environment() -> EnvCheckResult {
    let home = home_dir();
    let mut items = Vec::new();

    // 1. Claude Code
    match check_binary_on_path("claude") {
        Some(path) => items.push(cli_ok_item("claude", "Claude Code", path)),
        None => items.push(cli_missing_item("claude", "Claude Code")),
    }

    // 2. Codex
    match check_binary_on_path("codex") {
        Some(path) => items.push(cli_ok_item("codex", "Codex", path)),
        None => items.push(cli_missing_item("codex", "Codex")),
    }

    // 4. Qoder
    match check_binary_on_path("qoderclicn") {
        Some(path) => items.push(cli_ok_item("qoderclicn", "Qoder", path)),
        None => items.push(cli_missing_item("qoderclicn", "Qoder")),
    }

    // 6. Shell wrapper
    let shell_rc = home.join(".claude-profiles").join("shell-rc");
    let shell_rc_ps1 = home.join(".claude-profiles").join("shell-rc.ps1");
    if shell_rc.exists() || shell_rc_ps1.exists() {
        let in_rc = if let Ok(zshrc) = std::fs::read_to_string(home.join(".zshrc")) {
            zshrc.contains("shell-rc") || zshrc.contains(".claude-profiles")
        } else {
            false
        };
        // On non-macOS also check .bashrc (Linux + Windows Git Bash)
        let in_bashrc = !cfg!(target_os = "macos") && {
            if let Ok(bashrc) = std::fs::read_to_string(home.join(".bashrc")) {
                bashrc.contains("shell-rc") || bashrc.contains(".claude-profiles")
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
                    .map(|c| c.contains("shell-rc") || c.contains(".claude-profiles"))
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
            install_options: None,
            install_cmd: None,
        });
    }

    // 7. Config directory
    let config_dir = home.join(".claude-profiles");
    let config_file = config_dir.join("config.yaml");
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
                install_options: None,
                install_cmd: None,
            });
        }
    } else {
        items.push(EnvCheckItem {
            name: "config".into(),
            label: "配置文件".into(),
            status: "missing".into(),
            severity: "warn".into(),
            category: "config".into(),
            detail: "目录不存在".into(),
            detected_path: None,
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

    static CRUD_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn temp_config_setup() -> (std::sync::MutexGuard<'static, ()>, tempfile::TempDir) {
        let guard = CRUD_LOCK.lock().unwrap();
        let dir = tempfile::tempdir().unwrap();
        std::env::set_var(
            "CLAUDE_PROFILES_HOME",
            dir.path().to_string_lossy().to_string(),
        );
        (guard, dir)
    }

    fn cleanup_config(dir: tempfile::TempDir) {
        std::env::remove_var("CLAUDE_PROFILES_HOME");
        drop(dir);
        // CRUD_LOCK guard is dropped by caller
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
