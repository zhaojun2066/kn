use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

// ── Types ────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ProfileSummary {
    pub name: String,
    pub desc: String,
    pub env_count: usize,
    pub is_default: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cli_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ProfileList {
    pub default: String,
    pub profiles: Vec<ProfileSummary>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ProfileDetail {
    pub name: String,
    pub desc: String,
    pub env: HashMap<String, String>,
    pub is_default: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct EnvOutput {
    pub name: String,
    pub env: HashMap<String, String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct MutationResult {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub action: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct Config {
    #[serde(default)]
    default: String,
    #[serde(default)]
    profiles: HashMap<String, ProfileConfig>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
struct ProfileConfig {
    #[serde(default)]
    desc: String,
    #[serde(default)]
    env: HashMap<String, String>,
}

// ── Config path ──────────────────────────────────────────────

fn config_dir() -> PathBuf {
    let base = ".claude-profiles";
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_else(|_| "~".into());
    PathBuf::from(&home).join(base)
}

fn config_file() -> PathBuf {
    config_dir().join("config.yaml")
}

pub fn lock_file_path() -> PathBuf {
    config_dir().join(".config.lock")
}

// ── File operations ─────────────────────────────────────────

fn read_config() -> Result<Config, String> {
    let path = config_file();
    if !path.exists() {
        return Ok(Config { default: String::new(), profiles: HashMap::new() });
    }
    let content = fs::read_to_string(&path)
        .map_err(|e| format!("读取配置失败: {}", e))?;
    serde_yaml::from_str(&content)
        .map_err(|e| format!("解析配置失败: {}", e))
}

fn write_config(config: &Config) -> Result<(), String> {
    let dir = config_dir();
    fs::create_dir_all(&dir).map_err(|e| format!("创建目录失败: {}", e))?;
    let path = config_file();
    let backup = dir.join("config.yaml.bak");

    // File lock with staleness detection
    let lock = lock_file_path();
    if lock.exists() {
        // Check if lock is stale (> 3 seconds old)
        if let Ok(meta) = fs::metadata(&lock) {
            if let Ok(modified) = meta.modified() {
                let age = std::time::SystemTime::now().duration_since(modified).unwrap_or_default();
                if age.as_secs() > 3 {
                    let _ = fs::remove_file(&lock); // stale lock, remove it
                }
            }
        }
        // Wait briefly for lock to clear
        for _ in 0..20 {
            if !lock.exists() { break; }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        if lock.exists() {
            // Last resort: force remove stale lock
            let _ = fs::remove_file(&lock);
        }
    }
    fs::write(&lock, "").map_err(|e| format!("锁定失败: {}", e))?;

    // ── Backup existing config before overwriting ──
    if path.exists() {
        let _ = fs::copy(&path, &backup); // best-effort, don't fail on backup error
    }

    let yaml = serde_yaml::to_string(config).map_err(|e| format!("序列化失败: {}", e))?;

    // Atomic-ish write: tmp file → fsync → rename
    let tmp = dir.join("config.yaml.tmp");
    fs::write(&tmp, &yaml).map_err(|e| format!("写入配置失败: {}", e))?;
    // fsync the tmp file before renaming
    if let Ok(f) = std::fs::File::open(&tmp) {
        let _ = f.sync_all();
    }
    fs::rename(&tmp, &path).map_err(|e| format!("替换配置文件失败: {}", e))?;

    let _ = fs::remove_file(&lock);
    Ok(())
}

// ── Public command functions ────────────────────────────────

pub fn list_profiles_cmd() -> Result<ProfileList, String> {
    let config = read_config()?;
    let mut profiles: Vec<ProfileSummary> = config.profiles.iter().map(|(name, p)| {
        let cli_type = detect_cli_type(&p.env);
        let mut summary = ProfileSummary {
            name: name.clone(),
            desc: p.desc.clone(),
            env_count: p.env.len(),
            is_default: config.default == *name,
            cli_type,
            tags: None,
        };
        if let Some(tags_str) = p.env.get("_KN_TAGS") {
            let tags: Vec<String> = tags_str.split(',').map(|t| t.trim().to_string()).filter(|t| !t.is_empty()).collect();
            if !tags.is_empty() { summary.tags = Some(tags); }
        }
        summary
    }).collect();
    profiles.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(ProfileList { default: config.default, profiles })
}

pub fn show_profile_cmd(name: &str) -> Result<ProfileDetail, String> {
    let config = read_config()?;
    let p = config.profiles.get(name).ok_or_else(|| format!("profile '{}' 不存在", name))?;
    Ok(ProfileDetail {
        name: name.to_string(),
        desc: p.desc.clone(),
        env: p.env.clone(),
        is_default: config.default == name,
    })
}

pub fn get_env_cmd(name: &str) -> Result<EnvOutput, String> {
    let config = read_config()?;
    let p = config.profiles.get(name).ok_or_else(|| format!("profile '{}' 不存在", name))?;
    Ok(EnvOutput { name: name.to_string(), env: p.env.clone() })
}

pub fn add_profile_cmd(name: &str, desc: Option<&str>) -> Result<MutationResult, String> {
    let mut config = read_config()?;
    if config.profiles.contains_key(name) {
        return Ok(MutationResult { ok: false, error: Some(format!("profile '{}' 已存在", name)), action: None, profile: None, key: None });
    }
    let desc = desc.unwrap_or("").to_string();
    config.profiles.insert(name.to_string(), ProfileConfig { desc, env: HashMap::new() });
    if config.default.is_empty() { config.default = name.to_string(); }
    write_config(&config)?;
    Ok(MutationResult { ok: true, error: None, action: Some("add".into()), profile: Some(name.into()), key: None })
}

pub fn remove_profile_cmd(name: &str) -> Result<MutationResult, String> {
    let mut config = read_config()?;
    if !config.profiles.contains_key(name) {
        return Ok(MutationResult { ok: false, error: Some(format!("profile '{}' 不存在", name)), action: None, profile: None, key: None });
    }
    config.profiles.remove(name);
    if config.default == name {
        config.default = config.profiles.keys().next().cloned().unwrap_or_default();
    }
    write_config(&config)?;
    Ok(MutationResult { ok: true, error: None, action: Some("remove".into()), profile: Some(name.into()), key: None })
}

pub fn set_env_var_cmd(name: &str, key: &str, value: &str) -> Result<MutationResult, String> {
    let mut config = read_config()?;
    let p = config.profiles.get_mut(name).ok_or_else(|| format!("profile '{}' 不存在", name))?;
    p.env.insert(key.to_string(), value.to_string());
    write_config(&config)?;
    Ok(MutationResult { ok: true, error: None, action: Some("set".into()), profile: Some(name.into()), key: Some(key.into()) })
}

pub fn unset_env_var_cmd(name: &str, key: &str) -> Result<MutationResult, String> {
    let mut config = read_config()?;
    let p = config.profiles.get_mut(name).ok_or_else(|| format!("profile '{}' 不存在", name))?;
    p.env.remove(key);
    write_config(&config)?;
    Ok(MutationResult { ok: true, error: None, action: Some("unset".into()), profile: Some(name.into()), key: Some(key.into()) })
}

pub fn set_default_profile_cmd(name: &str) -> Result<MutationResult, String> {
    let mut config = read_config()?;
    if !config.profiles.contains_key(name) {
        return Ok(MutationResult { ok: false, error: Some(format!("profile '{}' 不存在", name)), action: None, profile: None, key: None });
    }
    config.default = name.to_string();
    write_config(&config)?;
    Ok(MutationResult { ok: true, error: None, action: Some("default".into()), profile: Some(name.into()), key: None })
}

pub fn get_default_profile_cmd() -> Result<String, String> {
    let config = read_config()?;
    Ok(config.default)
}

pub fn init_profiles_cmd() -> Result<MutationResult, String> {
    let home = std::env::var("HOME").or_else(|_| std::env::var("USERPROFILE")).unwrap_or_default();
    let settings_path = PathBuf::from(&home).join(".claude").join("settings.json");

    if !settings_path.exists() {
        return Ok(MutationResult { ok: false, error: Some("未找到 ~/.claude/settings.json".into()), action: None, profile: None, key: None });
    }

    let content = fs::read_to_string(&settings_path).map_err(|e| format!("读取失败: {}", e))?;
    let json: serde_json::Value = serde_json::from_str(&content).map_err(|e| format!("解析失败: {}", e))?;

    let mut config = read_config()?;
    let mut imported = 0;

    // Direct env at root level → create "claude" profile
    if let Some(env_obj) = json.get("env").and_then(|v| v.as_object()) {
        let mut env = HashMap::new();
        for (k, v) in env_obj {
            if let Some(s) = v.as_str() { env.insert(k.clone(), s.to_string()); }
        }
        if !env.is_empty() && !config.profiles.contains_key("claude") {
            config.profiles.insert("claude".into(), ProfileConfig { desc: "从 settings.json 导入".into(), env });
            imported += 1;
        }
    }

    // Profiles section
    if let Some(profiles_obj) = json.get("profiles").and_then(|v| v.as_object()) {
        for (name, val) in profiles_obj {
            if config.profiles.contains_key(name) { continue; }
            let mut env = HashMap::new();
            if let Some(e) = val.get("env").and_then(|v| v.as_object()) {
                for (k, v) in e { if let Some(s) = v.as_str() { env.insert(k.clone(), s.to_string()); } }
            }
            let desc = val.get("desc").and_then(|v| v.as_str()).unwrap_or("").to_string();
            if !env.is_empty() {
                config.profiles.insert(name.clone(), ProfileConfig { desc, env });
                imported += 1;
            }
        }
    }

    if imported == 0 {
        return Ok(MutationResult { ok: false, error: Some("未找到可导入的配置".into()), action: None, profile: None, key: None });
    }
    if config.default.is_empty() { config.default = "claude".into(); }
    write_config(&config)?;
    Ok(MutationResult { ok: true, error: None, action: Some("init".into()), profile: None, key: None })
}

// ── Shell wrapper setup ─────────────────────────────────────

const SHELL_RC: &str = r#"# AI Profile Manager — Shell Wrapper
# Usage: ai <tool> <profile>
#        ai profile <command>

CONFIG="$HOME/.claude-profiles/config.yaml"

_profile_env() {
    local name="$1"
    [ ! -f "$CONFIG" ] && return 1
    sed -n "/^  ${name}:/,/^  [a-z]/p" "$CONFIG" | \
        sed -n 's/^      \([A-Z][A-Z_]*\): *\(.*\)/export \1=\2/p'
}

_profile_list() {
    [ ! -f "$CONFIG" ] && { echo "No config found at $CONFIG" >&2; return 1; }
    grep '^  [a-zA-Z]' "$CONFIG" | sed 's/^  \([a-zA-Z0-9_-]*\):.*/\1/' | while read -r name; do
        local is_default=""
        grep -q "^default: \"$name\"" "$CONFIG" && is_default=" (*)"
        echo "  $name$is_default"
    done
}

_profile_show() {
    local name="$1"
    [ ! -f "$CONFIG" ] && { echo "No config found" >&2; return 1; }
    local output
    output=$(_profile_env "$name")
    if [ -z "$output" ]; then
        echo "Profile '$name' not found" >&2
        return 1
    fi
    echo "Profile: $name"
    echo "$output" | sed 's/^export //'
}

ai() {
    local cmd="${1:-}"
    if [ -z "$cmd" ]; then
        echo "Usage: ai <tool> [args...]"
        echo ""
        echo "  Run with profile:"
        echo "    ai claude <profile>       Run Claude Code with profile env"
        echo "    ai codex <profile>        Run Codex CLI with profile env"
        echo ""
        echo "  Manage profiles:"
        echo "    ai profile list           List all profiles"
        echo "    ai profile env <name>     Show env vars for a profile"
        echo "    ai profile switch <name>  Set default profile"
        return
    fi
    case "$cmd" in
        claude|codex)
            local tool="$1"; shift
            if [ $# -gt 0 ]; then
                local env_output=$(_profile_env "$1")
                if [ -n "$env_output" ]; then
                    local profile_name="$1"; shift
                    echo "-> Using profile: $profile_name"
                    (eval "$env_output" && command "$tool" "$@")
                    return
                fi
            fi
            command "$tool" "$@"
            ;;
        profile)
            shift
            case "${1:-}" in
                list)
                    _profile_list
                    ;;
                env)
                    shift
                    _profile_show "${1:-}"
                    ;;
                switch)
                    shift
                    local name="${1:-}"
                    if [ -z "$name" ]; then
                        echo "Usage: ai profile switch <name>" >&2
                        return 1
                    fi
                    if grep -q "^  ${name}:" "$CONFIG" 2>/dev/null; then
                        sed -i '' "s/^default:.*/default: \"$name\"/" "$CONFIG" 2>/dev/null || \
                        sed -i "s/^default:.*/default: \"$name\"/" "$CONFIG"
                        echo "Default profile set to '$name'"
                    else
                        echo "Profile '$name' not found" >&2
                        return 1
                    fi
                    ;;
                *)
                    echo "Usage: ai profile {list|env <name>|switch <name>}" >&2
                    return 1
                    ;;
            esac
            ;;
        -h|--help|help)
            echo "AI Profile Manager"
            echo "  ai claude <profile>       Run Claude Code with profile"
            echo "  ai codex <profile>        Run Codex CLI with profile"
            echo "  ai profile list           List all profiles"
            echo "  ai profile env <name>     Show env vars for profile"
            echo "  ai profile switch <name>  Set default profile"
            ;;
        *)
            echo "Unknown command: $cmd" >&2
            echo "Supported: claude, codex, profile" >&2
            return 1
            ;;
    esac
}
"#;

const SHELL_RC_PS1: &str = r#"# AI Profile Manager — PowerShell Wrapper
# Usage: ai <tool> <profile>
# Sourced by PTY on startup and/or via PowerShell profile.

$script:_kn_config_dir = if ($env:HOME) { "$env:HOME\.claude-profiles" } else { "$env:USERPROFILE\.claude-profiles" }

function _profile_env {
    param([string]$name)
    $cfg = Join-Path $script:_kn_config_dir "config.yaml"
    if (-not (Test-Path $cfg)) { return @() }
    $results = @()
    $inProfile = $false; $inEnv = $false
    $escaped = [regex]::Escape($name)
    Get-Content $cfg | ForEach-Object {
        if ($_ -match "^  ${escaped}:") { $inProfile = $true; return }
        if ($inProfile -and $_ -match "^    env:") { $inEnv = $true; return }
        if ($inEnv -and $_ -match '^      ([A-Za-z_][A-Za-z0-9_]*):\s*"?([^"]*)"?\s*$') {
            $results += "export $($Matches[1])=$($Matches[2])"
        }
        if ($inEnv -and $_ -match "^    [a-z]") { $inProfile = $false; $inEnv = $false }
    }
    return $results
}

function _profile_list {
    $cfg = Join-Path $script:_kn_config_dir "config.yaml"
    if (-not (Test-Path $cfg)) { Write-Host "No config found at $cfg"; return }
    $default = (Get-Content $cfg | Select-String '^default:\s*"?(.+?)"?\s*$').Matches.Groups[1].Value
    Get-Content $cfg | Select-String '^  ([a-zA-Z0-9_-]+):' | ForEach-Object {
        $n = $_.Matches.Groups[1].Value
        if ($n -eq $default) { Write-Host "  $n (*)" } else { Write-Host "  $n" }
    }
}

function _profile_show {
    param([string]$name)
    $vars = _profile_env $name
    if (-not $vars) { Write-Host "Profile '$name' not found"; return }
    Write-Host "Profile: $name"
    $vars | ForEach-Object { Write-Host ($_ -replace '^export ','') }
}

function ai {
    $cmd = $args[0]

    if (-not $cmd) {
        Write-Host "Usage: ai <tool> [args...]"
        Write-Host ""
        Write-Host "  Run with profile:"
        Write-Host "    ai claude <profile>       Run Claude Code with profile env"
        Write-Host "    ai codex <profile>        Run Codex CLI with profile env"
        Write-Host ""
        Write-Host "  Manage profiles:"
        Write-Host "    ai profile list           List all profiles"
        Write-Host "    ai profile env <name>     Show env vars for a profile"
        return
    }

    switch ($cmd) {
        'claude' {
            $tool = 'claude'
            $rest = $args[1..$args.Length]
            if ($rest.Count -gt 0) {
                $envs = _profile_env $rest[0]
                if ($envs) {
                    $profileName = $rest[0]
                    $toolArgs = $rest[1..$rest.Length]
                    Write-Host "-> Using profile: $profileName"
                    $envs | ForEach-Object { Invoke-Expression $_ }
                    & $tool @toolArgs
                    return
                }
            }
            & $tool @rest
        }
        'codex' {
            $tool = 'codex'
            $rest = $args[1..$args.Length]
            if ($rest.Count -gt 0) {
                $envs = _profile_env $rest[0]
                if ($envs) {
                    $profileName = $rest[0]
                    $toolArgs = $rest[1..$rest.Length]
                    Write-Host "-> Using profile: $profileName"
                    $envs | ForEach-Object { Invoke-Expression $_ }
                    & $tool @toolArgs
                    return
                }
            }
            & $tool @rest
        }
        'profile' {
            $subcmd = $args[1]
            switch ($subcmd) {
                'list' { _profile_list }
                'env' { _profile_show $args[2] }
                default {
                    Write-Host "Usage: ai profile {list|env <name>}"
                }
            }
        }
        { $_ -in @('-h','--help','help') } {
            Write-Host "AI Profile Manager"
            Write-Host "  ai claude <profile>       Run Claude Code with profile"
            Write-Host "  ai codex <profile>        Run Codex CLI with profile"
            Write-Host "  ai profile list           List all profiles"
            Write-Host "  ai profile env <name>     Show env vars for profile"
        }
        default {
            Write-Host "Unknown command: $cmd"
            Write-Host "Supported: claude, codex, profile"
        }
    }
}
"#;

pub fn ensure_shell_rc() -> Result<String, String> {
    let dir = config_dir();
    fs::create_dir_all(&dir).map_err(|e| format!("创建目录失败: {}", e))?;

    let home = std::env::var("HOME").or_else(|_| std::env::var("USERPROFILE")).unwrap_or_default();

    // ── Migration: merge old dev config into unified config (one-time) ──
    let dev_config = PathBuf::from(&home).join(".claude-profiles-dev").join("config.yaml");
    if dev_config.exists() {
        if let Ok(content) = fs::read_to_string(&dev_config) {
            if let Ok(dev_cfg) = serde_yaml::from_str::<Config>(&content) {
                if let Ok(mut prod_cfg) = read_config() {
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
                        let _ = write_config(&prod_cfg);
                    }
                }
            }
        }
        // Rename old dev config so migration only runs once
        let _ = fs::rename(&dev_config, dev_config.with_extension("yaml.migrated"));
    }

    // Write shell-rc to config dir
    fs::write(dir.join("shell-rc"), SHELL_RC).map_err(|e| format!("写入 shell-rc 失败: {}", e))?;
    if cfg!(target_os = "windows") {
        fs::write(dir.join("shell-rc.ps1"), SHELL_RC_PS1).ok();
    }

    // Unix: add source line to ~/.zshrc (idempotent)
    let zshrc = PathBuf::from(&home).join(".zshrc");
    let source_line = format!("source \"{}/shell-rc\"", dir.display());
    let content = if zshrc.exists() { fs::read_to_string(&zshrc).unwrap_or_default() } else { String::new() };
    let marker = "# AI Profile Manager";
    if !content.contains(&source_line) {
        let new_content = if content.ends_with('\n') || content.is_empty() {
            format!("{}{}\n{}\n", content, marker, source_line)
        } else {
            format!("{}\n{}\n{}\n", content, marker, source_line)
        };
        fs::write(&zshrc, new_content).map_err(|e| format!("写入 .zshrc 失败: {}", e))?;
    }

    // Windows only: add dot-source to PowerShell profile AND .bashrc (for Git Bash)
    if cfg!(target_os = "windows") {
        let ps_profile = PathBuf::from(&home).join("Documents").join("PowerShell").join("Microsoft.PowerShell_profile.ps1");
        if let Some(parent) = ps_profile.parent() {
            fs::create_dir_all(parent).ok();
        }
        let dir_str = dir.display().to_string().replace('\\', "/");
        let dot_line = format!(". \"{}/shell-rc.ps1\"", dir_str);
        if ps_profile.exists() {
            let content = fs::read_to_string(&ps_profile).unwrap_or_default();
            if !content.contains(&dot_line) {
                fs::write(&ps_profile, format!("{}\n# AI Profile Manager\n{}\n", content, dot_line)).ok();
            }
        } else {
            fs::write(&ps_profile, format!("# AI Profile Manager\n{}\n", dot_line)).ok();
        }

        // Also add to .bashrc for Git Bash users (the PTY uses Git Bash on Windows)
        let bashrc = PathBuf::from(&home).join(".bashrc");
        let bash_source_line = format!("source \"{}/shell-rc\"", dir_str);
        let bash_content = if bashrc.exists() { fs::read_to_string(&bashrc).unwrap_or_default() } else { String::new() };
        if !bash_content.contains(&bash_source_line) {
            let new_bash = if bash_content.ends_with('\n') || bash_content.is_empty() {
                format!("{}# AI Profile Manager (bash)\n{}\n", bash_content, bash_source_line)
            } else {
                format!("{}\n# AI Profile Manager (bash)\n{}\n", bash_content, bash_source_line)
            };
            fs::write(&bashrc, new_bash).ok();
        }
    }

    Ok(dir.display().to_string())
}

// ── Helpers ──────────────────────────────────────────────────

fn detect_cli_type(env: &HashMap<String, String>) -> Option<String> {
    if let Some(t) = env.get("_KN_CLI_TYPE") { return Some(t.clone()); }
    let has_a = env.keys().any(|k| k.starts_with("ANTHROPIC_"));
    let has_o = env.keys().any(|k| k.starts_with("OPENAI_"));
    if has_a && has_o { Some("both".into()) }
    else if has_a { Some("claude".into()) }
    else if has_o { Some("codex".into()) }
    else { None }
}
