use crate::profile_cmd;
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
    use base64::{Engine as _, engine::general_purpose::STANDARD};
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
        return Ok(FileTreeNode { name, path, is_dir: false, children: None });
    }

    // Symlink cycle guard: detect already-visited directories
    let canonical = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    if !visited.insert(canonical) {
        return Ok(FileTreeNode { name, path, is_dir: true, children: Some(vec![]) });
    }

    // Depth limit
    if depth >= MAX_TREE_DEPTH {
        return Ok(FileTreeNode { name, path, is_dir: true, children: Some(vec![]) });
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

    Ok(FileTreeNode { name, path, is_dir: true, children: Some(children) })
}

#[tauri::command]
pub fn list_directory_tree(path: String) -> Result<FileTreeNode, String> {
    let p = std::path::Path::new(&path);

    // If the path is a single file, return a tree with just that file
    if p.is_file() {
        let name = p.file_name()
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
                name: "claude".into(),
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
            name: "codex".into(),
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
            name: "qoder-cn".into(),
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
            vec![
                format!(r"{}\System32\{}", system32, exe_name),
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
    // Fallback: just use the first name (might work via PATH)
    names.first().map(|n| n.to_string())
}

#[tauri::command]
pub async fn fetch_url(url: String) -> Result<String, String> {
    // 后台线程执行，不阻塞主异步运行时
    tauri::async_runtime::spawn_blocking(move || {
        let curl = find_binary(&["curl"]).unwrap_or_else(|| "curl".into());
        let output = std::process::Command::new(&curl)
            .args(["-sL", "--max-time", "30", &url])
            .output()
            .map_err(|e| format!("curl 执行失败: {}", e))?;
        if !output.status.success() {
            return Err(String::from_utf8_lossy(&output.stderr).to_string());
        }
        String::from_utf8(output.stdout).map_err(|e| format!("编码错误: {}", e))
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
    let actual = if cfg!(target_os = "macos") {
        let shasum = find_binary(&["shasum"]).unwrap_or_else(|| "shasum".into());
        let output = std::process::Command::new(&shasum)
            .args(["-a", "256", &path])
            .output()
            .map_err(|e| format!("shasum 执行失败: {}", e))?;
        String::from_utf8_lossy(&output.stdout)
            .split_whitespace()
            .next()
            .unwrap_or("")
            .to_string()
    } else if cfg!(target_os = "windows") {
        // Use PowerShell Get-FileHash (locale-independent, unlike certutil).
        // Escape single quotes in path: PowerShell single-quoted strings use '' for literal '
        let escaped_path = path.replace('\'', "''");
        let output = std::process::Command::new("powershell")
            .args([
                "-NoProfile",
                "-Command",
                &format!(
                    "(Get-FileHash -Path '{}' -Algorithm SHA256).Hash",
                    escaped_path
                ),
            ])
            .output()
            .map_err(|e| format!("powershell 执行失败: {}", e))?;
        String::from_utf8_lossy(&output.stdout)
            .trim()
            .to_lowercase()
    } else {
        let sha = find_binary(&["sha256sum", "shasum"]).unwrap_or_else(|| "sha256sum".into());
        let args: Vec<&str> = if sha.contains("shasum") {
            vec!["-a", "256", path.as_str()]
        } else {
            vec![path.as_str()]
        };
        let output = std::process::Command::new(&sha)
            .args(&args)
            .output()
            .map_err(|e| format!("sha256 执行失败: {}", e))?;
        String::from_utf8_lossy(&output.stdout)
            .split_whitespace()
            .next()
            .unwrap_or("")
            .to_string()
    };
    Ok(actual.to_lowercase() == expected.to_lowercase())
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
pub struct EnvCheckItem {
    pub name: String,
    pub label: String,
    pub status: String, // "ok" | "warn" | "missing"
    pub detail: String,
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
        if std::path::Path::new(&path).exists() {
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
                "command -v {} 2>/dev/null || type {} 2>/dev/null",
                name, name
            ),
        ]
    };
    if let Ok(output) = std::process::Command::new(&shell).args(shell_args).output() {
        let s = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !s.is_empty() {
            return Some(s);
        }
    }
    None
}

#[tauri::command]
pub fn check_environment() -> EnvCheckResult {
    let home = home_dir();
    let mut items = Vec::new();

    // 1. Claude Code
    match check_binary_on_path("claude") {
        Some(path) => items.push(EnvCheckItem {
            name: "claude".into(),
            label: "Claude Code".into(),
            status: "ok".into(),
            detail: path,
            install_cmd: None,
        }),
        None => {
            let (hint, install_cmd) = if cfg!(target_os = "windows") {
                (
                    "未安装 (npm i -g @anthropic-ai/claude-code)",
                    "npm i -g @anthropic-ai/claude-code",
                )
            } else {
                (
                    "未安装 (curl -fsSL https://claude.ai/install.sh | bash)",
                    "curl -fsSL https://claude.ai/install.sh | bash",
                )
            };
            items.push(EnvCheckItem {
                name: "claude".into(),
                label: "Claude Code".into(),
                status: "missing".into(),
                detail: hint.into(),
                install_cmd: Some(install_cmd.into()),
            })
        }
    }

    // 2. Codex
    match check_binary_on_path("codex") {
        Some(path) => items.push(EnvCheckItem {
            name: "codex".into(),
            label: "Codex".into(),
            status: "ok".into(),
            detail: path,
            install_cmd: None,
        }),
        None => items.push(EnvCheckItem {
            name: "codex".into(),
            label: "Codex".into(),
            status: "missing".into(),
            detail: "未安装 (npm i -g @openai/codex)".into(),
            install_cmd: Some("npm i -g @openai/codex".into()),
        }),
    }

    // 4. Qoder
    match check_binary_on_path("qoderclicn") {
        Some(path) => items.push(EnvCheckItem {
            name: "qoderclicn".into(),
            label: "Qoder".into(),
            status: "ok".into(),
            detail: path,
            install_cmd: None,
        }),
        None => items.push(EnvCheckItem {
            name: "qoderclicn".into(),
            label: "Qoder".into(),
            status: "missing".into(),
            detail: "未安装 (npm i -g @qodercn-ai/qoderclicn)".into(),
            install_cmd: Some("npm i -g @qodercn-ai/qoderclicn".into()),
        }),
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
            detail: if activated {
                "已激活".into()
            } else {
                "已安装但未激活".into()
            },
            install_cmd: None,
        });
    } else {
        items.push(EnvCheckItem {
            name: "shell-wrapper".into(),
            label: "Shell 集成".into(),
            status: "missing".into(),
            detail: if cfg!(target_os = "windows") {
                "未安装，请在终端运行 install.ps1".into()
            } else {
                "未安装，请在终端运行 install.sh".into()
            },
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
                detail: config_file.display().to_string(),
                install_cmd: None,
            });
        } else {
            items.push(EnvCheckItem {
                name: "config".into(),
                label: "配置文件".into(),
                status: "warn".into(),
                detail: "目录存在但无配置文件".into(),
                install_cmd: None,
            });
        }
    } else {
        items.push(EnvCheckItem {
            name: "config".into(),
            label: "配置文件".into(),
            status: "missing".into(),
            detail: "目录不存在".into(),
            install_cmd: None,
        });
    }

    let all_ok = items.iter().all(|i| i.status == "ok");

    EnvCheckResult { items, all_ok }
}
