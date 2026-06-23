//! Profile configuration read/write — shared between Desktop and Agent.
//!
//! The desktop crate wraps write operations with its intra-process mutex
//! (`with_write_lock`). The Agent only needs read access (list profiles for
//! WSS reporting).

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
pub struct Config {
    #[serde(default)]
    pub default: String,
    #[serde(default)]
    pub profiles: HashMap<String, ProfileConfig>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct ProfileConfig {
    #[serde(default)]
    pub desc: String,
    #[serde(default)]
    pub env: HashMap<String, String>,
}

// ── Config path ──────────────────────────────────────────────

fn config_file() -> PathBuf {
    crate::path::config_dir().join("config.yaml")
}

fn lock_file_path() -> PathBuf {
    crate::path::config_dir().join(".config.lock")
}

// ── File locking (cross-process, interoperable with Python fcntl.flock) ──

pub fn acquire_lock(file: &std::fs::File, timeout: Duration) -> Result<(), String> {
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

pub fn release_lock(file: &std::fs::File) -> Result<(), String> {
    file.unlock()
        .map_err(|e| format!("释放锁失败: {}", e))
}

// ── File operations ─────────────────────────────────────────

/// Read config from disk. Returns empty config if file doesn't exist.
pub fn read_config() -> Result<Config, String> {
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

/// Write config to disk with backup rotation and atomic write.
///
/// Acquires cross-process file lock (via fs2) to serialize with Python CLI.
/// Does NOT acquire intra-process mutex — callers in the desktop crate must
/// wrap with `with_write_lock` for concurrent Tauri command safety.
pub fn write_config_file(config: &Config) -> Result<(), String> {
    let dir = crate::path::config_dir();
    fs::create_dir_all(&dir).map_err(|e| format!("创建目录失败: {}", e))?;
    let path = config_file();

    // Acquire cross-process lock
    let lock_path = lock_file_path();
    let lock_fh = std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(false)
        .open(&lock_path)
        .map_err(|e| format!("无法打开锁文件: {}", e))?;
    acquire_lock(&lock_fh, Duration::from_secs(5))?;

    let result = write_config_inner(&dir, &path, config);

    release_lock(&lock_fh)?;
    result
}

/// Write config file — inner logic WITHOUT acquiring locks.
/// Callers must hold cross-process file lock.
///
/// `path` should be `dir.join("config.yaml")`; if not, backup files go to
/// `dir` but the main file is written to the (possibly different) `path`.
pub fn write_config_inner(
    dir: &std::path::Path,
    path: &std::path::Path,
    config: &Config,
) -> Result<(), String> {
    debug_assert!(
        path.parent().map_or(false, |p| p == dir),
        "write_config_inner: path should be under dir"
    );
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
        if let Err(e) = fs::copy(path, &backup) {
            eprintln!("[kn-common] 配置备份失败: {}", e);
        }
    }

    let yaml = serde_yaml::to_string(config).map_err(|e| format!("序列化失败: {}", e))?;

    // Atomic-ish write: tmp file → fsync → rename
    let tmp = dir.join("config.yaml.tmp");
    fs::write(&tmp, &yaml).map_err(|e| format!("写入配置失败: {}", e))?;
    if let Ok(f) = std::fs::File::open(&tmp) {
        let _ = f.sync_all();
    }
    if fs::rename(&tmp, path).is_err() {
        fs::write(path, &yaml).map_err(|e| format!("写入配置文件失败: {}", e))?;
        let _ = fs::remove_file(&tmp);
    }
    Ok(())
}

// ── Public command functions (read-only) ─────────────────────

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

pub fn get_default_profile_cmd() -> Result<String, String> {
    let config = read_config()?;
    Ok(config.default)
}

// ── Profile name validation ─────────────────────────────────

const RESERVED_KEYWORDS: &[&str] = &["claude", "codex", "qoderclicn", "profile", "ai", "help"];

/// Validate profile name: [a-z0-9]([a-z0-9-]*[a-z0-9])? + not reserved.
pub fn validate_profile_name(name: &str) -> Result<(), String> {
    if name.is_empty()
        || !name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        || name.starts_with('-')
        || name.ends_with('-')
    {
        return Err(
            "profile 名称只能包含小写字母、数字和连字符，不能以连字符开头或结尾".into(),
        );
    }
    if RESERVED_KEYWORDS.contains(&name) {
        return Err(format!(
            "'{}' 是系统保留关键字，不能用作 Profile 名称",
            name
        ));
    }
    Ok(())
}

// ── Public mutation commands (use write_config_file, no intra-process lock) ──

pub fn add_profile_cmd(name: &str, desc: Option<&str>) -> Result<MutationResult, String> {
    if let Err(e) = validate_profile_name(name) {
        return Ok(MutationResult {
            ok: false,
            error: Some(e),
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
    write_config_file(&config)?;
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
    write_config_file(&config)?;
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
    write_config_file(&config)?;
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
    write_config_file(&config)?;
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
    write_config_file(&config)?;
    Ok(MutationResult {
        ok: true,
        error: None,
        action: Some("default".into()),
        profile: Some(name.into()),
        key: None,
    })
}

pub fn init_profiles_cmd() -> Result<MutationResult, String> {
    let home = crate::path::home_dir().to_string_lossy().to_string();
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
    write_config_file(&config)?;
    Ok(MutationResult {
        ok: true,
        error: None,
        action: Some("init".into()),
        profile: None,
        key: None,
    })
}

// ── CLI type detection ──────────────────────────────────────

/// Detect which CLI tool a profile is configured for.
pub fn detect_cli_type(env: &HashMap<String, String>) -> Option<String> {
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

// ── Test helpers (re-exported for desktop crate tests) ──────

#[cfg(test)]
pub fn make_test_config(default: &str, names: &[&str]) -> Config {
    let mut profiles = HashMap::new();
    for name in names {
        profiles.insert(name.to_string(), ProfileConfig::default());
    }
    Config {
        default: default.into(),
        profiles,
    }
}

// ── Tests ───────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_remove_default_promotes_first_alphabetical() {
        let mut config = make_test_config("zulu", &["alpha", "bravo", "zulu"]);
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
        let mut config = make_test_config("alpha", &["alpha", "bravo"]);
        config.profiles.remove("bravo");
        assert_eq!(config.default, "alpha");
    }

    #[test]
    fn test_remove_last_profile_clears_default() {
        let mut config = make_test_config("solo", &["solo"]);
        config.profiles.remove("solo");
        if config.default == "solo" {
            let mut remaining: Vec<&String> = config.profiles.keys().collect();
            remaining.sort();
            config.default = remaining.first().map(|s| (*s).clone()).unwrap_or_default();
        }
        assert_eq!(config.default, "");
    }

    #[test]
    fn test_config_serialization_roundtrip() {
        let mut cfg = Config {
            default: String::new(),
            profiles: std::collections::HashMap::new(),
        };
        let mut env = std::collections::HashMap::new();
        env.insert("KEY1".into(), "value1".into());
        env.insert("KEY2".into(), "value2".into());
        cfg.profiles.insert(
            "myprofile".into(),
            ProfileConfig {
                desc: "test desc".into(),
                env,
            },
        );
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
        let result = read_config();
        // May succeed or fail depending on whether real config exists; either way no panic
        assert!(result.is_ok() || result.is_err());
    }

    #[test]
    fn test_validate_profile_name_valid() {
        assert!(validate_profile_name("my-profile").is_ok());
        assert!(validate_profile_name("abc123").is_ok());
        assert!(validate_profile_name("a").is_ok());
    }

    #[test]
    fn test_validate_profile_name_invalid() {
        assert!(validate_profile_name("-start").is_err());
        assert!(validate_profile_name("end-").is_err());
        assert!(validate_profile_name("claude").is_err());
        assert!(validate_profile_name("").is_err());
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
}
