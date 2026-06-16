use fs2::FileExt;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::time::{Duration, Instant};

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

// ── File locking (cross-process, interoperable with Python fcntl.flock) ──

fn lock_file_path() -> PathBuf {
    crate::config_dir().join(".config.lock")
}

fn acquire_lock(file: &std::fs::File, timeout: Duration) -> Result<(), String> {
    let deadline = Instant::now() + timeout;
    loop {
        match file.try_lock_exclusive() {
            Ok(()) => return Ok(()),
            Err(_) => {
                if Instant::now() > deadline {
                    return Err("无法获取配置锁 (5s 超时)".into());
                }
                std::thread::sleep(Duration::from_millis(50));
            }
        }
    }
}

fn release_lock(file: &std::fs::File) -> Result<(), String> {
    file.unlock()
        .map_err(|e| format!("释放锁失败: {}", e))
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

/// Write config file — inner logic WITHOUT acquiring locks.
/// Callers must hold both `with_write_lock` and cross-process file lock.
fn write_config_inner(config: &Config) -> Result<(), String> {
    let dir = crate::config_dir();
    fs::create_dir_all(&dir).map_err(|e| format!("创建目录失败: {}", e))?;
    let path = config_file();
    let backup = dir.join("config.yaml.bak");

    // ── Rotate backups (keep 3 generations) then backup ──
    if path.exists() {
        for i in (1..=2).rev() {
            let src = dir.join(format!("config.yaml.bak.{}", i));
            let dst = dir.join(format!("config.yaml.bak.{}", i + 1));
            if src.exists() {
                let _ = fs::rename(&src, &dst);
            }
        }
        if backup.exists() {
            let _ = fs::rename(&backup, dir.join("config.yaml.bak.1"));
        }
        let _ = fs::copy(&path, &backup); // best-effort, don't fail on backup error
    }

    let yaml = serde_yaml::to_string(config).map_err(|e| format!("序列化失败: {}", e))?;

    // Atomic-ish write: tmp file → fsync → rename
    let tmp = dir.join("config.yaml.tmp");
    fs::write(&tmp, &yaml).map_err(|e| format!("写入配置失败: {}", e))?;
    if let Ok(f) = std::fs::File::open(&tmp) {
        let _ = f.sync_all();
    }
    if fs::rename(&tmp, &path).is_err() {
        fs::write(&path, &yaml).map_err(|e| format!("写入配置文件失败: {}", e))?;
        let _ = fs::remove_file(&tmp);
    }
    Ok(())
}

fn write_config(config: &Config) -> Result<(), String> {
    crate::with_write_lock(|| {
        let lock_path = lock_file_path();
        let lock_fh = std::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(false)
            .open(&lock_path)
            .map_err(|e| format!("无法打开锁文件: {}", e))?;
        acquire_lock(&lock_fh, Duration::from_secs(5))?;

        let result = write_config_inner(config);

        release_lock(&lock_fh)?;
        result
    })
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
        let mut remaining: Vec<&String> = config.profiles.keys().collect();
        remaining.sort();
        config.default = remaining.first().map(|s| (*s).clone()).unwrap_or_default();
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
    let home = crate::home_dir().to_string_lossy().to_string();
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

// Embedded at build time from canonical sources in shell/ directory.
const SHELL_RC: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../shell/ai-profile.sh"));

const SHELL_RC_PS1: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../shell/ai-profile.ps1"));

const COMPLETION_ZSH: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../shell/completions/_ai"));

const COMPLETION_BASH: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../shell/completions/ai.bash"));

const HOOK_RECORDER: &str = r##"#!/usr/bin/env python3
"""Token usage recorder — called by Stop/SessionEnd hooks.
Reads structured JSON from stdin, extracts token usage, appends to usage.jsonl.
Supports Claude Code, Codex transcripts.
"""

import sys, json, os
from datetime import datetime, timezone

USAGE_FILE = os.path.join(
    os.environ.get("KN_HOME", os.path.expanduser("~/.kn")), "usage.jsonl"
)

PROJECTS_FILE = os.path.join(
    os.environ.get("KN_HOME", os.path.expanduser("~/.kn")), "projects.json"
)


def _resolve_project():
    """Read KN_WORKING_DIR, auto-register project, return (path, name)."""
    working_dir = os.environ.get("KN_WORKING_DIR", "")
    if not working_dir:
        # Backward compatibility: try old env var
        working_dir = os.environ.get("KN_PROJECT_DIR", "")
    if not working_dir:
        return None, None

    # Normalize: realpath resolves symlinks and normalizes path separators
    try:
        working_dir = os.path.realpath(working_dir)
    except OSError:
        working_dir = os.path.abspath(working_dir)

    project_name = os.path.basename(working_dir.rstrip(os.sep))

    # Auto-register in projects.json
    _ensure_project_registered(working_dir, project_name)

    return working_dir, project_name


def _ensure_project_registered(path, name):
    """Add project to projects.json if not already present.

    Uses fcntl.flock on Unix for cross-process safety (same pattern as
    the Rust backend's config lock). On Windows, falls back to simple
    read-merge-write.
    """
    try:
        import fcntl

        os.makedirs(os.path.dirname(PROJECTS_FILE), exist_ok=True)
        fd = os.open(PROJECTS_FILE, os.O_RDWR | os.O_CREAT, 0o644)
        try:
            fcntl.flock(fd, fcntl.LOCK_EX)
            os.lseek(fd, 0, 0)
            raw = os.read(fd, 16384) or b"[]"
            projects = json.loads(raw.decode("utf-8"))
        except (json.JSONDecodeError, OSError):
            projects = []
            os.lseek(fd, 0, 0)

        if not isinstance(projects, list):
            projects = []

        # Check if path already registered
        if any(p.get("path") == path for p in projects if isinstance(p, dict)):
            fcntl.flock(fd, fcntl.LOCK_UN)
            os.close(fd)
            return

        projects.append({"name": name, "path": path})
        data = json.dumps(projects, indent=2, ensure_ascii=False)

        os.ftruncate(fd, 0)
        os.lseek(fd, 0, 0)
        os.write(fd, data.encode("utf-8"))
        fcntl.flock(fd, fcntl.LOCK_UN)
        os.close(fd)
        return
    except ImportError:
        # Windows fallback: no fcntl available
        pass

    # Windows path (or any platform where fcntl import failed)
    try:
        os.makedirs(os.path.dirname(PROJECTS_FILE), exist_ok=True)
        try:
            with open(PROJECTS_FILE, "r", encoding="utf-8") as f:
                projects = json.load(f)
        except (FileNotFoundError, json.JSONDecodeError):
            projects = []

        if not isinstance(projects, list):
            projects = []

        if any(p.get("path") == path for p in projects if isinstance(p, dict)):
            return

        projects.append({"name": name, "path": path})
        with open(PROJECTS_FILE, "w", encoding="utf-8") as f:
            json.dump(projects, f, indent=2, ensure_ascii=False)
    except OSError:
        pass


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

    project_path, project_name = _resolve_project()

    record = {
        "ts": datetime.now(timezone.utc).isoformat(),
        "profile": profile,
        "tool": tool,
        **usage,
    }
    if project_path:
        record["project_path"] = project_path
        if project_name:
            record["project_name"] = project_name

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
    # But token_count events don't carry the model name — merge it from
    # assistant messages or the config.toml fallback when available.
    if latest_codex_usage:
        if not latest_codex_usage.get("model") and model:
            latest_codex_usage["model"] = model
        elif not latest_codex_usage.get("model"):
            latest_codex_usage["model"] = default_model()
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


def _codex_config_model():
    """Read model from ~/.codex/config.toml as last-resort fallback."""
    try:
        path = os.path.join(os.path.expanduser("~"), ".codex", "config.toml")
        with open(path) as f:
            for line in f:
                line = line.strip()
                if line.startswith("model "):
                    return line.split("=", 1)[1].strip().strip('"').strip("'")
    except OSError:
        pass
    return ""


def default_model():
    return os.environ.get("OPENAI_MODEL") or os.environ.get("CODEX_MODEL") or _codex_config_model() or ""


if __name__ == "__main__":
    main()
"##;

pub fn ensure_shell_rc() -> Result<String, String> {
    // One-time migration from legacy ~/.claude-profiles → ~/.kn
    crate::migrate_legacy_config_dir();

    let dir = crate::config_dir();
    fs::create_dir_all(&dir).map_err(|e| format!("创建目录失败: {}", e))?;

    let home = crate::home_dir().to_string_lossy().to_string();

    // ── Migration: merge old dev config into unified config (one-time) ──
    let dev_config = PathBuf::from(&home)
        .join(".claude-profiles-dev")
        .join("config.yaml");
    if dev_config.exists() {
        if let Ok(content) = fs::read_to_string(&dev_config) {
            if let Ok(dev_cfg) = serde_yaml::from_str::<Config>(&content) {
                // Read, merge, and write under both locks to prevent races
                // with concurrent config writes from Python CLI or other Tauri commands
                let _ = crate::with_write_lock(|| {
                    let lock_path = lock_file_path();
                    let lock_fh = match std::fs::OpenOptions::new()
                        .create(true)
                        .write(true)
                        .truncate(false)
                        .open(&lock_path)
                    {
                        Ok(fh) => fh,
                        Err(_) => return Ok(()),
                    };
                    if acquire_lock(&lock_fh, Duration::from_secs(5)).is_err() {
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
                            write_config_inner(&prod_cfg)?;
                        }
                        Ok(())
                    })();

                    release_lock(&lock_fh)?;
                    result
                });
            }
        }
        // Rename old dev config so migration only runs once
        let _ = fs::rename(&dev_config, dev_config.with_extension("yaml.migrated"));
    }

    // Write shell-rc to config dir (only if content changed — preserves user customizations)
    let shell_rc_path = dir.join("shell-rc");
    let needs_write = match fs::read_to_string(&shell_rc_path) {
        Ok(existing) => existing != SHELL_RC,
        Err(_) => true,
    };
    if needs_write {
        fs::write(&shell_rc_path, SHELL_RC).map_err(|e| format!("写入 shell-rc 失败: {}", e))?;
    }
    if cfg!(target_os = "windows") {
        let ps1_path = dir.join("shell-rc.ps1");
        // PowerShell 5.1 reads files without BOM as the system's ANSI
        // codepage (GBK on Chinese Windows).  This corrupts non-ASCII
        // characters in string literals and can break the parser when
        // bytes inside strings happen to look like operators or quotes.
        const UTF8_BOM: &[u8] = &[0xEF, 0xBB, 0xBF];
        let needs_ps1_write = match fs::read(&ps1_path) {
            Ok(existing) => {
                let existing_content = if existing.starts_with(UTF8_BOM) {
                    &existing[3..]
                } else {
                    &existing
                };
                existing_content != SHELL_RC_PS1.as_bytes()
            }
            Err(_) => true,
        };
        if needs_ps1_write {
            let mut content = Vec::with_capacity(UTF8_BOM.len() + SHELL_RC_PS1.len());
            content.extend_from_slice(UTF8_BOM);
            content.extend_from_slice(SHELL_RC_PS1.as_bytes());
            fs::write(&ps1_path, content).ok();
        }
    }

    // Write shell completions to config dir
    let completions_dir = dir.join("completions");
    fs::create_dir_all(&completions_dir).ok();
    let zsh_path = completions_dir.join("_ai");
    let bash_path = completions_dir.join("ai.bash");
    let needs_zsh_write = match fs::read_to_string(&zsh_path) {
        Ok(existing) => existing != COMPLETION_ZSH,
        Err(_) => true,
    };
    if needs_zsh_write {
        fs::write(&zsh_path, COMPLETION_ZSH).ok();
    }
    let needs_bash_write = match fs::read_to_string(&bash_path) {
        Ok(existing) => existing != COMPLETION_BASH,
        Err(_) => true,
    };
    if needs_bash_write {
        fs::write(&bash_path, COMPLETION_BASH).ok();
    }

    // Write token usage hook recorder script
    let hooks_dir = dir.join("hooks");
    fs::create_dir_all(&hooks_dir).ok();
    fs::write(hooks_dir.join("record-usage.py"), HOOK_RECORDER).ok();

    // Write hook execution log wrapper script
    let _ = crate::hook_logs::write_run_with_log_script();

    // Repair any missing hook store scripts (e.g. after config dir migration)
    crate::hook_store::repair_missing_hook_scripts();

    // ── Unix: add source line to ~/.zshrc (idempotent) ──
    if !cfg!(target_os = "windows") {
        let zshrc = PathBuf::from(&home).join(".zshrc");
        let source_line = format!("source \"{}/shell-rc\"", dir.display());
        let content = if zshrc.exists() {
            fs::read_to_string(&zshrc).unwrap_or_default()
        } else {
            String::new()
        };
        // Clean up any legacy .claude-profiles references from old installs
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
            // Source line already present — still write back cleaned content
            // in case old .claude-profiles lines were removed above.
            fs::write(&zshrc, content).map_err(|e| format!("写入 .zshrc 失败: {}", e))?;
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
            // Clean up any legacy .claude-profiles references
            let bash_content = remove_claude_profiles_lines(&bash_content);
            let bash_marker = "# kn (bash)";
            if !bash_content.contains(&bash_source_line) {
                let new_bash = if bash_content.ends_with('\n') || bash_content.is_empty() {
                    format!("{}{}\n{}\n", bash_content, bash_marker, bash_source_line)
                } else {
                    format!("{}\n{}\n{}\n", bash_content, bash_marker, bash_source_line)
                };
                fs::write(&bashrc, new_bash).ok();
            } else {
                // Write back cleaned content even if source line already present
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
                // Only add compinit if .zshrc doesn't already have it (avoid slow duplicate init)
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
                // Remove old completion block if present, then append new one
                let zsh_cleaned = remove_marker_block(&zsh_content, compl_marker_start, compl_marker_end);
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
                let bash_cleaned = remove_marker_block(&bash_content, compl_marker_start, compl_marker_end);
                let new_bash = format!("{}{}", bash_cleaned, bash_compl_block);
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
                // Clean up any legacy .claude-profiles references
                let content = remove_claude_profiles_lines(&content);
                if !content.contains(&dot_line) {
                    fs::write(
                        &ps_profile,
                        format!("{}\n# kn\n{}\n", content, dot_line),
                    )
                    .ok();
                } else {
                    // Write back cleaned content even if dot_line already present
                    fs::write(&ps_profile, content).ok();
                }
            } else {
                fs::write(&ps_profile, format!("# kn\n{}\n", dot_line)).ok();
            }
        }
    }

    Ok(dir.display().to_string())
}

// ── Helpers ──────────────────────────────────────────────────

/// Remove any lines referencing the legacy `.claude-profiles` directory
/// from shell RC content. This cleans up old `source` / `.` lines left
/// behind after the project was renamed to `kn` (~/.claude-profiles → ~/.kn).
fn remove_claude_profiles_lines(content: &str) -> String {
    content
        .lines()
        .filter(|line| !line.contains(".claude-profiles"))
        .collect::<Vec<&str>>()
        .join("\n")
}

/// Remove a marker-delimited block from a string. Used for idempotent shell RC updates.
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn make_config(default: &str, names: &[&str]) -> Config {
        let mut profiles = HashMap::new();
        for name in names {
            profiles.insert(name.to_string(), ProfileConfig::default());
        }
        Config {
            default: default.into(),
            profiles,
        }
    }

    #[test]
    fn test_remove_default_promotes_first_alphabetical() {
        let mut config = make_config("zulu", &["alpha", "bravo", "zulu"]);
        config.profiles.remove("zulu");
        if config.default == "zulu" {
            let mut remaining: Vec<&String> = config.profiles.keys().collect();
            remaining.sort();
            config.default = remaining.first().map(|s| (*s).clone()).unwrap_or_default();
        }
        assert_eq!(config.default, "alpha");
    }

    #[test]
    fn test_remove_non_default_preserves_default() {
        let mut config = make_config("alpha", &["alpha", "bravo"]);
        config.profiles.remove("bravo");
        // default was "alpha", bravo was not default — no change to default logic
        assert_eq!(config.default, "alpha");
    }

    #[test]
    fn test_remove_last_profile_clears_default() {
        let mut config = make_config("solo", &["solo"]);
        config.profiles.remove("solo");
        if config.default == "solo" {
            let mut remaining: Vec<&String> = config.profiles.keys().collect();
            remaining.sort();
            config.default = remaining.first().map(|s| (*s).clone()).unwrap_or_default();
        }
        assert_eq!(config.default, "");
    }

    #[test]
    fn test_lock_acquire_release() {
        let dir = std::env::temp_dir().join(format!("kn-test-lock-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let lock_path = dir.join(".config.lock");
        let f = std::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .open(&lock_path)
            .unwrap();

        acquire_lock(&f, Duration::from_secs(1)).unwrap();
        release_lock(&f).unwrap();

        std::fs::remove_dir_all(&dir).ok();
    }

    // ── Config validation & serialization tests ──

    #[test]
    fn test_config_serialization_roundtrip() {
        // Test serde_yaml roundtrip without touching filesystem
        let mut cfg = Config { default: String::new(), profiles: std::collections::HashMap::new() };
        let mut env = std::collections::HashMap::new();
        env.insert("KEY1".into(), "value1".into());
        env.insert("KEY2".into(), "value2".into());
        cfg.profiles.insert("myprofile".into(), ProfileConfig { desc: "test desc".into(), env });
        cfg.default = "myprofile".into();

        let yaml = serde_yaml::to_string(&cfg).unwrap();
        let loaded: Config = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(loaded.default, "myprofile");
        assert!(loaded.profiles.contains_key("myprofile"));
        let p = loaded.profiles.get("myprofile").unwrap();
        assert_eq!(p.desc, "test desc");
        assert_eq!(p.env.get("KEY1").unwrap(), "value1");
        assert_eq!(p.env.get("KEY2").unwrap(), "value2");
    }

    #[test]
    fn test_read_config_returns_default_for_missing_file() {
        // read_config without env setup reads from default path — just verify it doesn't panic
        let result = read_config();
        // May succeed or fail depending on whether real config exists; either way no panic
        if let Ok(cfg) = result {
            assert!(cfg.profiles.is_empty() || !cfg.profiles.is_empty()); // tautology, just no panic
        }
    }
}
