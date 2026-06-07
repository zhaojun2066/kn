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

fn config_file() -> PathBuf {
    crate::config_dir().join("config.yaml")
}

// ── File operations ─────────────────────────────────────────

fn read_config() -> Result<Config, String> {
    let path = config_file();
    if !path.exists() {
        return Ok(Config {
            default: String::new(),
            profiles: HashMap::new(),
        });
    }
    let content = fs::read_to_string(&path).map_err(|e| format!("读取配置失败: {}", e))?;
    serde_yaml::from_str(&content).map_err(|e| format!("解析配置失败: {}", e))
}

fn write_config(config: &Config) -> Result<(), String> {
    let dir = crate::config_dir();
    fs::create_dir_all(&dir).map_err(|e| format!("创建目录失败: {}", e))?;
    let path = config_file();
    let backup = dir.join("config.yaml.bak");

    // ── Backup existing config before overwriting ──
    if path.exists() {
        let _ = fs::copy(&path, &backup); // best-effort, don't fail on backup error
    }

    let yaml = serde_yaml::to_string(config).map_err(|e| format!("序列化失败: {}", e))?;

    // Atomic-ish write: tmp file → fsync → rename
    // On Windows, rename() may fail if the target exists and is locked by AV/backup.
    // Use write-then-rename for best-effort atomicity, fall back to direct write on failure.
    let tmp = dir.join("config.yaml.tmp");
    fs::write(&tmp, &yaml).map_err(|e| format!("写入配置失败: {}", e))?;
    // fsync the tmp file before renaming
    if let Ok(f) = std::fs::File::open(&tmp) {
        let _ = f.sync_all();
    }
    if fs::rename(&tmp, &path).is_err() {
        // Rename failed (e.g. Windows file lock) — fall back to direct write
        fs::write(&path, &yaml).map_err(|e| format!("写入配置文件失败: {}", e))?;
        let _ = fs::remove_file(&tmp);
    }

    Ok(())
}

// ── Public command functions ────────────────────────────────

pub fn list_profiles_cmd() -> Result<ProfileList, String> {
    let config = read_config()?;
    let mut profiles: Vec<ProfileSummary> = config
        .profiles
        .iter()
        .map(|(name, p)| {
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
                let tags: Vec<String> = tags_str
                    .split(',')
                    .map(|t| t.trim().to_string())
                    .filter(|t| !t.is_empty())
                    .collect();
                if !tags.is_empty() {
                    summary.tags = Some(tags);
                }
            }
            summary
        })
        .collect();
    profiles.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(ProfileList {
        default: config.default,
        profiles,
    })
}

pub fn show_profile_cmd(name: &str) -> Result<ProfileDetail, String> {
    let config = read_config()?;
    let p = config
        .profiles
        .get(name)
        .ok_or_else(|| format!("profile '{}' 不存在", name))?;
    Ok(ProfileDetail {
        name: name.to_string(),
        desc: p.desc.clone(),
        env: p.env.clone(),
        is_default: config.default == name,
    })
}

pub fn get_env_cmd(name: &str) -> Result<EnvOutput, String> {
    let config = read_config()?;
    let p = config
        .profiles
        .get(name)
        .ok_or_else(|| format!("profile '{}' 不存在", name))?;
    Ok(EnvOutput {
        name: name.to_string(),
        env: p.env.clone(),
    })
}

pub fn add_profile_cmd(name: &str, desc: Option<&str>) -> Result<MutationResult, String> {
    // Validate name: must match [a-z0-9]([a-z0-9-]*[a-z0-9])?
    // This prevents shell injection in sed / regex-based YAML parsing in shell-rc.
    if name.is_empty()
        || !name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        || name.starts_with('-')
        || name.ends_with('-')
    {
        return Ok(MutationResult {
            ok: false,
            error: Some(
                "profile 名称只能包含小写字母、数字和连字符，不能以连字符开头或结尾".into(),
            ),
            action: None,
            profile: None,
            key: None,
        });
    }
    // Reserved keywords — conflicts with shell wrapper tool routing
    const RESERVED: &[&str] = &["claude", "codex", "qoderclicn", "profile", "ai", "help"];
    if RESERVED.contains(&name) {
        return Ok(MutationResult {
            ok: false,
            error: Some(format!(
                "'{}' 是系统保留关键字，不能用作 Profile 名称",
                name
            )),
            action: None,
            profile: None,
            key: None,
        });
    }
    let mut config = read_config()?;
    if config.profiles.contains_key(name) {
        return Ok(MutationResult {
            ok: false,
            error: Some(format!("profile '{}' 已存在", name)),
            action: None,
            profile: None,
            key: None,
        });
    }
    let desc = desc.unwrap_or("").to_string();
    config.profiles.insert(
        name.to_string(),
        ProfileConfig {
            desc,
            env: HashMap::new(),
        },
    );
    if config.default.is_empty() {
        config.default = name.to_string();
    }
    write_config(&config)?;
    Ok(MutationResult {
        ok: true,
        error: None,
        action: Some("add".into()),
        profile: Some(name.into()),
        key: None,
    })
}

pub fn remove_profile_cmd(name: &str) -> Result<MutationResult, String> {
    let mut config = read_config()?;
    if !config.profiles.contains_key(name) {
        return Ok(MutationResult {
            ok: false,
            error: Some(format!("profile '{}' 不存在", name)),
            action: None,
            profile: None,
            key: None,
        });
    }
    config.profiles.remove(name);
    if config.default == name {
        config.default = config.profiles.keys().next().cloned().unwrap_or_default();
    }
    write_config(&config)?;
    Ok(MutationResult {
        ok: true,
        error: None,
        action: Some("remove".into()),
        profile: Some(name.into()),
        key: None,
    })
}

pub fn set_env_var_cmd(name: &str, key: &str, value: &str) -> Result<MutationResult, String> {
    let mut config = read_config()?;
    let p = config
        .profiles
        .get_mut(name)
        .ok_or_else(|| format!("profile '{}' 不存在", name))?;
    p.env.insert(key.to_string(), value.to_string());
    write_config(&config)?;
    Ok(MutationResult {
        ok: true,
        error: None,
        action: Some("set".into()),
        profile: Some(name.into()),
        key: Some(key.into()),
    })
}

pub fn unset_env_var_cmd(name: &str, key: &str) -> Result<MutationResult, String> {
    let mut config = read_config()?;
    let p = config
        .profiles
        .get_mut(name)
        .ok_or_else(|| format!("profile '{}' 不存在", name))?;
    p.env.remove(key);
    write_config(&config)?;
    Ok(MutationResult {
        ok: true,
        error: None,
        action: Some("unset".into()),
        profile: Some(name.into()),
        key: Some(key.into()),
    })
}

pub fn set_default_profile_cmd(name: &str) -> Result<MutationResult, String> {
    let mut config = read_config()?;
    if !config.profiles.contains_key(name) {
        return Ok(MutationResult {
            ok: false,
            error: Some(format!("profile '{}' 不存在", name)),
            action: None,
            profile: None,
            key: None,
        });
    }
    config.default = name.to_string();
    write_config(&config)?;
    Ok(MutationResult {
        ok: true,
        error: None,
        action: Some("default".into()),
        profile: Some(name.into()),
        key: None,
    })
}

pub fn get_default_profile_cmd() -> Result<String, String> {
    let config = read_config()?;
    Ok(config.default)
}

pub fn init_profiles_cmd() -> Result<MutationResult, String> {
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_default();
    let settings_path = PathBuf::from(&home).join(".claude").join("settings.json");

    if !settings_path.exists() {
        return Ok(MutationResult {
            ok: false,
            error: Some("未找到 ~/.claude/settings.json".into()),
            action: None,
            profile: None,
            key: None,
        });
    }

    let content = fs::read_to_string(&settings_path).map_err(|e| format!("读取失败: {}", e))?;
    let json: serde_json::Value =
        serde_json::from_str(&content).map_err(|e| format!("解析失败: {}", e))?;

    let mut config = read_config()?;
    let mut imported = 0;

    // Direct env at root level → create "claude" profile
    if let Some(env_obj) = json.get("env").and_then(|v| v.as_object()) {
        let mut env = HashMap::new();
        for (k, v) in env_obj {
            if let Some(s) = v.as_str() {
                env.insert(k.clone(), s.to_string());
            }
        }
        if !env.is_empty() && !config.profiles.contains_key("claude") {
            config.profiles.insert(
                "claude".into(),
                ProfileConfig {
                    desc: "从 settings.json 导入".into(),
                    env,
                },
            );
            imported += 1;
        }
    }

    // Profiles section
    if let Some(profiles_obj) = json.get("profiles").and_then(|v| v.as_object()) {
        for (name, val) in profiles_obj {
            if config.profiles.contains_key(name) {
                continue;
            }
            let mut env = HashMap::new();
            if let Some(e) = val.get("env").and_then(|v| v.as_object()) {
                for (k, v) in e {
                    if let Some(s) = v.as_str() {
                        env.insert(k.clone(), s.to_string());
                    }
                }
            }
            let desc = val
                .get("desc")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            if !env.is_empty() {
                config
                    .profiles
                    .insert(name.clone(), ProfileConfig { desc, env });
                imported += 1;
            }
        }
    }

    if imported == 0 {
        return Ok(MutationResult {
            ok: false,
            error: Some("未找到可导入的配置".into()),
            action: None,
            profile: None,
            key: None,
        });
    }
    if config.default.is_empty() {
        config.default = "claude".into();
    }
    write_config(&config)?;
    Ok(MutationResult {
        ok: true,
        error: None,
        action: Some("init".into()),
        profile: None,
        key: None,
    })
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
        awk -F': ' '/^      [A-Za-z_][A-Za-z0-9_]*:/ {
            k=$1; sub(/^      /,"",k)
            v=substr($0,length(k)+9)
            if (v ~ /^".*"$/) v=substr(v,2,length(v)-2)
            else if (v ~ /^'"'"'.*'"'"'$/) v=substr(v,2,length(v)-2)
            print "export " k "='"'"'" v "'"'"'"
        }'
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
        echo "    ai qoderclicn <profile>   Run Qoder with profile env"
        echo ""
        echo "  Manage profiles:"
        echo "    ai profile list           List all profiles"
        echo "    ai profile env <name>     Show env vars for a profile"
        echo "    ai profile switch <name>  Set default profile"
        return
    fi
    case "$cmd" in
        claude|codex|qoderclicn)
            local tool="$1"; shift
            if [ $# -gt 0 ]; then
                local env_output=$(_profile_env "$1")
                if [ -n "$env_output" ]; then
                    local profile_name="$1"; shift
                    echo "-> Using profile: $profile_name"
                    case "$tool" in
                        claude)
                            # Claude Code v2.0.1+ bug: settings.json env overrides shell env.
                            # Generate temp settings file from profile, pass via --settings flag.
                            if command -v python3 >/dev/null 2>&1; then
                                local tmp_settings
                                tmp_settings=$(mktemp "${TMPDIR:-/tmp}/kn-claude.XXXXXX")
                                echo "$env_output" | python3 -c "
import sys, json
env = {}
for line in sys.stdin:
    line = line.strip()
    if not line.startswith('export '):
        continue
    rest = line[7:]
    eq = rest.index('=')
    key = rest[:eq]
    val = rest[eq+1:]
    # Strip matching surrounding quotes if present
    if len(val) >= 2 and val[0] == val[-1] and val[0] in ('\"', \"'\"):
        val = val[1:-1]
    env[key] = val
print(json.dumps({'env': env}))
" > "$tmp_settings"
                                (eval "$env_output" && export KN_PROFILE="$profile_name" && export KN_CLI_TOOL="$tool" && command "$tool" --settings "$tmp_settings" "$@")
                                local rc=$?
                                rm -f "$tmp_settings"
                                return $rc
                            else
                                # python3 not available — fall back to env vars only
                                (eval "$env_output" && export KN_PROFILE="$profile_name" && export KN_CLI_TOOL="$tool" && command "$tool" "$@")
                                return
                            fi
                            ;;
                        codex)
                            # Codex ignores OPENAI_API_KEY env var; reads only ~/.codex/auth.json.
                            # Write the profile's key to auth.json, pass base_url/model via -c.
                            # Use set -- (not string var) for arg accumulation — zsh does NOT
                            # word-split unquoted variables, so $_kn_extra would become one arg.
                            local _kn_apikey=$(echo "$env_output" | sed -n "s/^export OPENAI_API_KEY='\\(.*\\)'/\\1/p")
                            local _kn_base=$(echo "$env_output" | sed -n "s/^export OPENAI_BASE_URL='\\(.*\\)'/\\1/p")
                            local _kn_model=$(echo "$env_output" | sed -n "s/^export OPENAI_MODEL='\\(.*\\)'/\\1/p")
                            local _kn_auth="$HOME/.codex/auth.json"
                            [ -n "$_kn_model" ] && set -- -c "model=$_kn_model" "$@"
                            [ -n "$_kn_base" ] && set -- -c "model_providers.custom.base_url=$_kn_base" "$@"
                            if [ -n "$_kn_apikey" ]; then
                                [ -d "$HOME/.codex" ] || mkdir -p "$HOME/.codex"
                                [ -f "$_kn_auth" ] && cp "$_kn_auth" "$_kn_auth.kn-bak"
                                printf '{"auth_mode":"apikey","OPENAI_API_KEY":"%s"}\n' "$_kn_apikey" > "$_kn_auth"
                                (eval "$env_output" && export KN_PROFILE="$profile_name" && export KN_CLI_TOOL="$tool" && command "$tool" "$@")
                                local _kn_rc=$?
                                [ -f "$_kn_auth.kn-bak" ] && mv "$_kn_auth.kn-bak" "$_kn_auth"
                                return $_kn_rc
                            fi
                            (eval "$env_output" && export KN_PROFILE="$profile_name" && export KN_CLI_TOOL="$tool" && command "$tool" "$@")
                            return
                            ;;
                        *)
                            (eval "$env_output" && export KN_PROFILE="$profile_name" && export KN_CLI_TOOL="$tool" && command "$tool" "$@")
                            return
                            ;;
                    esac
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
            echo "  ai qoderclicn <profile>   Run Qoder with profile"
            echo "  ai profile list           List all profiles"
            echo "  ai profile env <name>     Show env vars for profile"
            echo "  ai profile switch <name>  Set default profile"
            ;;
        *)
            echo "Unknown command: $cmd" >&2
            echo "Supported: claude, codex, qoderclicn, profile" >&2
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
    if (-not (Test-Path $cfg)) { return @{} }
    $env_vars = @{}
    $inProfile = $false; $inEnv = $false
    $escaped = [regex]::Escape($name)
    Get-Content $cfg | ForEach-Object {
        if ($_ -match "^  ${escaped}:") { $inProfile = $true; return }
        if ($inProfile -and $_ -match "^    env:") { $inEnv = $true; return }
        if ($inEnv -and $_ -match '^      ([A-Za-z_][A-Za-z0-9_]*):\s*"(.*)"\s*$') {
            $env_vars[$Matches[1]] = $Matches[2]
        } elseif ($inEnv -and $_ -match "^      ([A-Za-z_][A-Za-z0-9_]*):\s*'(.*)'\s*$") {
            $env_vars[$Matches[1]] = $Matches[2]
        } elseif ($inEnv -and $_ -match '^      ([A-Za-z_][A-Za-z0-9_]*):\s*([^\s].*[^\s]|[^\s])\s*$') {
            $env_vars[$Matches[1]] = $Matches[2]
        }
        if ($inEnv -and $_ -match "^  [a-z]") { $inProfile = $false; $inEnv = $false }
    }
    return $env_vars
}

function _profile_list {
    $cfg = Join-Path $script:_kn_config_dir "config.yaml"
    if (-not (Test-Path $cfg)) { Write-Host "No config found at $cfg"; return }
    $defaultMatch = Get-Content $cfg | Select-String '^default:\s*"?(.+?)"?\s*$'
    $default = if ($defaultMatch) { $defaultMatch.Matches.Groups[1].Value } else { "" }
    Get-Content $cfg | Select-String '^  ([a-zA-Z0-9_-]+):' | ForEach-Object {
        $n = $_.Matches.Groups[1].Value
        if ($n -eq $default) { Write-Host "  $n (*)" } else { Write-Host "  $n" }
    }
}

function _profile_show {
    param([string]$name)
    $vars = _profile_env $name
    if (-not $vars -or $vars.Count -eq 0) { Write-Host "Profile '$name' not found"; return }
    Write-Host "Profile: $name"
    foreach ($key in ($vars.Keys | Sort-Object)) {
        Write-Host "  $key=$($vars[$key])"
    }
}

function ai {
    $cmd = $args[0]

    if (-not $cmd) {
        Write-Host "Usage: ai <tool> [args...]"
        Write-Host ""
        Write-Host "  Run with profile:"
        Write-Host "    ai claude <profile>       Run Claude Code with profile env"
        Write-Host "    ai codex <profile>        Run Codex CLI with profile env"
        Write-Host "    ai qoderclicn <profile>   Run Qoder with profile env"
        Write-Host ""
        Write-Host "  Manage profiles:"
        Write-Host "    ai profile list           List all profiles"
        Write-Host "    ai profile env <name>     Show env vars for a profile"
        return
    }

    switch ($cmd) {
        'claude' {
            $tool = 'claude'
            $rest = @($args | Select-Object -Skip 1)
            if ($rest.Count -gt 0) {
                $envs = _profile_env $rest[0]
                if ($envs -and $envs.Count -gt 0) {
                    $profileName = $rest[0]
                    $toolArgs = @($rest | Select-Object -Skip 1)
                    Write-Host "-> Using profile: $profileName"
                    # Claude Code v2.0.1+ bug: settings.json env overrides shell env.
                    # Generate temp settings file via --settings (CLI flag > settings.json).
                    $tmpSettings = New-TemporaryFile
                    @{ env = $envs } | ConvertTo-Json -Compress | Set-Content $tmpSettings
                    foreach ($key in $envs.Keys) {
                        Set-Item -Path "env:$key" -Value $envs[$key]
                    }
                    $env:KN_PROFILE = $profileName
                    $env:KN_CLI_TOOL = $tool
                    & $tool --settings $tmpSettings @toolArgs
                    Remove-Item $tmpSettings -Force -ErrorAction SilentlyContinue
                    return
                }
            }
            & $tool @rest
        }
        'codex' {
            $tool = 'codex'
            $rest = @($args | Select-Object -Skip 1)
            if ($rest.Count -gt 0) {
                $envs = _profile_env $rest[0]
                if ($envs -and $envs.Count -gt 0) {
                    $profileName = $rest[0]
                    $toolArgs = @($rest | Select-Object -Skip 1)
                    Write-Host "-> Using profile: $profileName"
                    # Codex ignores OPENAI_API_KEY env var; reads only ~/.codex/auth.json.
                    # Write the profile's key to auth.json, pass base_url/model via -c.
                    $apiKey = $envs['OPENAI_API_KEY']
                    $baseUrl = $envs['OPENAI_BASE_URL']
                    $model = $envs['OPENAI_MODEL']
                    $authFile = "$env:USERPROFILE\.codex\auth.json"
                    $extraArgs = @()
                    if ($model) { $extraArgs += '-c', "model=$model" }
                    if ($baseUrl) { $extraArgs += '-c', "model_providers.custom.base_url=$baseUrl" }
                    if ($apiKey) {
                        $codexDir = Split-Path $authFile -Parent
                        if (-not (Test-Path $codexDir)) { New-Item -ItemType Directory -Force $codexDir | Out-Null }
                        $oldAuth = $null
                        if (Test-Path $authFile) { $oldAuth = Get-Content $authFile -Raw }
                        @{ auth_mode = "apikey"; OPENAI_API_KEY = $apiKey } | ConvertTo-Json -Compress | Set-Content $authFile
                        try {
                            foreach ($key in $envs.Keys) {
                                Set-Item -Path "env:$key" -Value $envs[$key]
                            }
                            $env:KN_PROFILE = $profileName
                            $env:KN_CLI_TOOL = $tool
                            & $tool @extraArgs @toolArgs
                        } finally {
                            if ($oldAuth) { Set-Content $authFile $oldAuth }
                        }
                        return
                    }
                    foreach ($key in $envs.Keys) {
                        Set-Item -Path "env:$key" -Value $envs[$key]
                    }
                    $env:KN_PROFILE = $profileName
                    $env:KN_CLI_TOOL = $tool
                    & $tool @extraArgs @toolArgs
                    return
                }
            }
            & $tool @rest
        }
        'qoderclicn' {
            $tool = 'qoderclicn'
            $rest = @($args | Select-Object -Skip 1)
            if ($rest.Count -gt 0) {
                $envs = _profile_env $rest[0]
                if ($envs -and $envs.Count -gt 0) {
                    $profileName = $rest[0]
                    $toolArgs = @($rest | Select-Object -Skip 1)
                    Write-Host "-> Using profile: $profileName"
                    foreach ($key in $envs.Keys) {
                        Set-Item -Path "env:$key" -Value $envs[$key]
                    }
                    $env:KN_PROFILE = $profileName
                    $env:KN_CLI_TOOL = $tool
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
            Write-Host "  ai qoderclicn <profile>   Run Qoder with profile"
            Write-Host "  ai profile list           List all profiles"
            Write-Host "  ai profile env <name>     Show env vars for profile"
        }
        default {
            Write-Host "Unknown command: $cmd"
            Write-Host "Supported: claude, codex, qoderclicn, profile"
        }
    }
}
"#;

const HOOK_RECORDER: &str = r##"#!/usr/bin/env python3
"""Token usage recorder — called by Stop/SessionEnd hooks.
Reads structured JSON from stdin, extracts token usage, appends to usage.jsonl.
Supports Claude Code, Codex transcripts.
"""

import sys, json, os
from datetime import datetime, timezone

USAGE_FILE = os.path.join(
    os.path.expanduser("~"), ".claude-profiles", "usage.jsonl"
)


def main():
    try:
        data = json.load(sys.stdin)
    except (json.JSONDecodeError, OSError):
        sys.exit(0)

    profile = os.environ.get("KN_PROFILE", "")
    tool = os.environ.get("KN_CLI_TOOL", "")

    usage = extract(data)
    if not usage:
        sys.exit(0)

    record = {
        "ts": datetime.now(timezone.utc).isoformat(),
        "profile": profile,
        "tool": tool,
        **usage,
    }

    try:
        os.makedirs(os.path.dirname(USAGE_FILE), exist_ok=True)
        with open(USAGE_FILE, "a", encoding="utf-8") as f:
            f.write(json.dumps(record, ensure_ascii=False) + "\n")
    except OSError:
        pass

    sys.exit(0)


def extract(data):
    """Extract token usage from hook payload. Returns dict or None."""
    # Codex session/event payload: event_msg token_count carries cumulative usage.
    codex_usage = extract_codex_token_count(data)
    if codex_usage:
        return codex_usage
    # Codex: TurnComplete carries token_usage
    if "token_usage" in data:
        u = data["token_usage"]
        return {
            "model": str(u.get("model", "")),
            "tokens_in": int(u.get("input", u.get("input_tokens", 0))),
            "tokens_out": int(u.get("output", u.get("output_tokens", 0))),
        }
    # Claude Code: usage field in Stop/SessionEnd
    if "usage" in data:
        u = data["usage"]
        return {
            "model": str(u.get("model", "")),
            "tokens_in": int(u.get("input_tokens", u.get("input", 0))),
            "tokens_out": int(u.get("output_tokens", u.get("output", 0))),
        }
    # Claude Code / Codex: no usage inline, read transcript file
    if "transcript_path" in data:
        return extract_from_transcript(data["transcript_path"])
    if "transcriptPath" in data:
        return extract_from_transcript(data["transcriptPath"])
    # Generic fallback: top-level tokens_in / tokens_out
    if "tokens_in" in data or "tokens_out" in data:
        return {
            "model": str(data.get("model", "")),
            "tokens_in": int(data.get("tokens_in", 0)),
            "tokens_out": int(data.get("tokens_out", 0)),
        }
    return None


def extract_codex_token_count(entry):
    """Extract cumulative Codex token usage from an event_msg/token_count entry."""
    payload = entry.get("payload", {})
    if not isinstance(payload, dict) or payload.get("type") != "token_count":
        return None

    info = payload.get("info", {})
    if not isinstance(info, dict):
        return None

    u = info.get("total_token_usage") or info.get("last_token_usage")
    if not isinstance(u, dict):
        return None

    tokens_in = _in(u)
    tokens_out = _out(u)
    if tokens_in == 0 and tokens_out == 0:
        return None

    return {
        "model": str(u.get("model", entry.get("model", default_model()))),
        "tokens_in": tokens_in,
        "tokens_out": tokens_out,
    }


def extract_from_transcript(path):
    """Read transcript, sum usage from all turns.
    Handles Claude Code/Codex (JSONL).
    """
    total_in = 0
    total_out = 0
    model = ""
    latest_codex_usage = None

    try:
        with open(path, encoding="utf-8") as f:
            raw = f.read()
    except OSError:
        return None

    # ── JSONL: Claude Code / Codex transcript, one JSON per line ──
    for line in raw.splitlines():
        line = line.strip()
        if not line:
            continue
        try:
            entry = json.loads(line)
        except json.JSONDecodeError:
            continue

        codex_usage = extract_codex_token_count(entry)
        if codex_usage:
            latest_codex_usage = codex_usage
            continue

        # Claude Code / Codex: message.role == "assistant" with usage
        msg = entry.get("message", {})
        if isinstance(msg, dict) and msg.get("role") == "assistant":
            u = msg.get("usage")
            if u:
                total_in += _in(u)
                total_out += _out(u)
                if not model:
                    model = msg.get("model", "")
            continue

        # Generic: usage at entry level
        u = entry.get("usage")
        if isinstance(u, dict):
            total_in += _in(u)
            total_out += _out(u)
            if not model:
                model = entry.get("model", u.get("model", ""))
            continue

    # Codex token_count entries are cumulative; use the last one, do not sum.
    if latest_codex_usage:
        return latest_codex_usage

    if total_in == 0 and total_out == 0:
        return None

    return {
        "model": str(model),
        "tokens_in": total_in,
        "tokens_out": total_out,
    }


def _in(u):
    """Extract input tokens from a usage dict (multiple field name fallbacks)."""
    return int(u.get("input_tokens", u.get("prompt_tokens",
           u.get("input", u.get("promptTokenCount", 0)))))


def _out(u):
    """Extract output tokens from a usage dict (multiple field name fallbacks)."""
    return int(u.get("output_tokens", u.get("completion_tokens",
           u.get("output", u.get("candidatesTokenCount", 0)))))


def default_model():
    return os.environ.get("OPENAI_MODEL") or os.environ.get("CODEX_MODEL") or ""


if __name__ == "__main__":
    main()
"##;

pub fn ensure_shell_rc() -> Result<String, String> {
    let dir = crate::config_dir();
    fs::create_dir_all(&dir).map_err(|e| format!("创建目录失败: {}", e))?;

    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_default();

    // ── Migration: merge old dev config into unified config (one-time) ──
    let dev_config = PathBuf::from(&home)
        .join(".claude-profiles-dev")
        .join("config.yaml");
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

    // Write token usage hook recorder script
    let hooks_dir = dir.join("hooks");
    fs::create_dir_all(&hooks_dir).ok();
    fs::write(hooks_dir.join("record-usage.py"), HOOK_RECORDER).ok();

    // ── Unix: add source line to ~/.zshrc (idempotent) ──
    if !cfg!(target_os = "windows") {
        let zshrc = PathBuf::from(&home).join(".zshrc");
        let source_line = format!("source \"{}/shell-rc\"", dir.display());
        let content = if zshrc.exists() {
            fs::read_to_string(&zshrc).unwrap_or_default()
        } else {
            String::new()
        };
        let marker = "# AI Profile Manager";
        if !content.contains(&source_line) {
            let new_content = if content.ends_with('\n') || content.is_empty() {
                format!("{}{}\n{}\n", content, marker, source_line)
            } else {
                format!("{}\n{}\n{}\n", content, marker, source_line)
            };
            fs::write(&zshrc, new_content).map_err(|e| format!("写入 .zshrc 失败: {}", e))?;
        }

        // ── Unix: also add to ~/.bashrc (Linux default, also harmless on macOS) ──
        {
            let bashrc = PathBuf::from(&home).join(".bashrc");
            let bash_source_line = format!("source \"{}/shell-rc\"", dir.display());
            let bash_content = if bashrc.exists() {
                fs::read_to_string(&bashrc).unwrap_or_default()
            } else {
                String::new()
            };
            let bash_marker = "# AI Profile Manager (bash)";
            if !bash_content.contains(&bash_source_line) {
                let new_bash = if bash_content.ends_with('\n') || bash_content.is_empty() {
                    format!("{}{}\n{}\n", bash_content, bash_marker, bash_source_line)
                } else {
                    format!("{}\n{}\n{}\n", bash_content, bash_marker, bash_source_line)
                };
                fs::write(&bashrc, new_bash).ok();
            }
        }
    } // !windows: end Unix shell RC setup

    // ── Windows only: PowerShell profile (PS5 + PS7) ──
    if cfg!(target_os = "windows") {
        let dir_str = dir.display().to_string().replace('\\', "/");
        let dot_line = format!(". \"{}/shell-rc.ps1\"", dir_str);

        let docs_dir = crate::windows_documents_dir();
        // PowerShell 7 profile
        let ps7_profile = docs_dir
            .join("PowerShell")
            .join("Microsoft.PowerShell_profile.ps1");
        // PowerShell 5.1 profile
        let ps5_profile = docs_dir
            .join("WindowsPowerShell")
            .join("Microsoft.PowerShell_profile.ps1");

        for ps_profile in &[ps7_profile, ps5_profile] {
            if let Some(parent) = ps_profile.parent() {
                fs::create_dir_all(parent).ok();
            }
            if ps_profile.exists() {
                let content = fs::read_to_string(&ps_profile).unwrap_or_default();
                if !content.contains(&dot_line) {
                    fs::write(
                        &ps_profile,
                        format!("{}\n# AI Profile Manager\n{}\n", content, dot_line),
                    )
                    .ok();
                }
            } else {
                fs::write(&ps_profile, format!("# AI Profile Manager\n{}\n", dot_line)).ok();
            }
        }
    }

    Ok(dir.display().to_string())
}

// ── Helpers ──────────────────────────────────────────────────

fn detect_cli_type(env: &HashMap<String, String>) -> Option<String> {
    if let Some(t) = env.get("_KN_CLI_TYPE") {
        if t == "both" {
            return Some("claude".into());
        }
        return Some(t.clone());
    }
    // Qoder uses OPENAI_API_KEY + OPENAI_BASE_URL, same as Codex.
    // Distinguish by checking if base_url points to dashscope (Qwen official endpoint).
    if let Some(base_url) = env.get("OPENAI_BASE_URL") {
        if base_url.contains("dashscope") {
            return Some("qoderclicn".into());
        }
    }
    if env.keys().any(|k| k.starts_with("ANTHROPIC_")) {
        return Some("claude".into());
    }
    if env.keys().any(|k| k.starts_with("OPENAI_")) {
        return Some("codex".into());
    }
    None
}
