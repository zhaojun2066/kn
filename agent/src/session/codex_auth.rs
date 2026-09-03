use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs::{self, File, OpenOptions};
use std::io::{ErrorKind, Write};
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

static AUTH_TMP_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CodexAuthKind {
    ApiKey,
    Account,
    Unknown,
}

#[derive(Debug)]
pub(crate) struct CodexAuthRestore {
    auth_path: PathBuf,
    state_root: PathBuf,
    lock_dir: PathBuf,
    account_slot_path: PathBuf,
    written_path: PathBuf,
    original_path: PathBuf,
    no_original_path: PathBuf,
    restored: bool,
}

impl CodexAuthRestore {
    pub(crate) fn restore_after_start(&mut self) -> Result<(), String> {
        self.restore("after_start")
    }

    pub(crate) fn restore_on_exit(&mut self) -> Result<(), String> {
        self.restore("on_exit")
    }

    fn restore(&mut self, _phase: &str) -> Result<(), String> {
        if self.restored {
            return Ok(());
        }

        if file_matches(&self.auth_path, &self.written_path)? {
            if self.original_path.exists() {
                copy_auth_file(&self.original_path, &self.auth_path)?;
            } else if self.no_original_path.exists() && self.auth_path.exists() {
                fs::remove_file(&self.auth_path)
                    .map_err(|e| format!("failed to remove temporary Codex auth: {e}"))?;
            }
        } else if classify_auth_file(&self.auth_path)? == CodexAuthKind::Account {
            ensure_dir_private(&self.state_root)?;
            ensure_parent_private(&self.account_slot_path)?;
            copy_auth_file(&self.auth_path, &self.account_slot_path)?;
        }

        let _ = fs::remove_dir_all(&self.lock_dir);
        self.restored = true;
        Ok(())
    }
}

pub(crate) fn codex_auth_args_and_restore(
    env: &HashMap<String, String>,
    profile_name: Option<&str>,
) -> Result<(Vec<String>, Option<CodexAuthRestore>), String> {
    let paths = CodexAuthPaths::resolve(profile_name.unwrap_or("default"))?;
    codex_auth_args_and_restore_with_paths(env, paths)
}

fn codex_auth_args_and_restore_with_paths(
    env: &HashMap<String, String>,
    paths: CodexAuthPaths,
) -> Result<(Vec<String>, Option<CodexAuthRestore>), String> {
    ensure_file_auth_storage_at(&paths.codex_home)?;

    let mut extra_args = Vec::new();
    if let Some(model) = env.get("OPENAI_MODEL").filter(|v| !v.is_empty()) {
        extra_args.extend([
            "-c".into(),
            format!("model={}", super::env::toml_string(model)),
        ]);
    }

    let auth_mode = env.get("_KN_AUTH_MODE").map(String::as_str);
    let auth_mode_normalized = auth_mode.map(str::to_ascii_lowercase);
    let api_key = env.get("OPENAI_API_KEY").filter(|v| !v.is_empty());
    let explicit_local_login = matches!(
        auth_mode_normalized.as_deref(),
        Some("local_login" | "chatgpt")
    );
    let explicit_api_key = matches!(auth_mode_normalized.as_deref(), Some("api_key" | "apikey"));
    let is_api_key = !explicit_local_login && (api_key.is_some() || explicit_api_key);

    if is_api_key {
        if let Some(base_url) = env.get("OPENAI_BASE_URL").filter(|v| !v.is_empty()) {
            extra_args.extend([
                "-c".into(),
                "model_provider=\"custom\"".into(),
                "-c".into(),
                "model_providers.custom.name=\"Custom\"".into(),
                "-c".into(),
                "model_providers.custom.env_key=\"OPENAI_API_KEY\"".into(),
                "-c".into(),
                format!(
                    "model_providers.custom.base_url={}",
                    super::env::toml_string(base_url)
                ),
                "-c".into(),
                "model_providers.custom.requires_openai_auth=true".into(),
                "-c".into(),
                "model_providers.custom.wire_api=\"responses\"".into(),
            ]);
        } else {
            extra_args.extend(["-c".into(), "model_provider=\"openai\"".into()]);
        }

        let key =
            api_key.ok_or_else(|| "Codex API key profile is missing OPENAI_API_KEY".to_string())?;
        let restore = prepare_api_key_auth(key, &paths)?;
        return Ok((extra_args, Some(restore)));
    }

    extra_args.extend(["-c".into(), "model_provider=\"openai\"".into()]);
    let account_paths = paths.for_profile("local-login");
    let restore = prepare_account_auth_if_needed(&account_paths)?;
    Ok((extra_args, restore))
}

pub(crate) fn restore_grace_period() -> Duration {
    restore_grace_period_from_value(std::env::var("KN_CODEX_AUTH_RESTORE_DELAY_MS").ok())
}

fn restore_grace_period_from_value(value: Option<String>) -> Duration {
    let millis = value
        .as_deref()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(500)
        .min(5000);
    Duration::from_millis(millis)
}

fn prepare_api_key_auth(api_key: &str, paths: &CodexAuthPaths) -> Result<CodexAuthRestore, String> {
    let mut restore = acquire_auth_lock(paths)?;

    let result = (|| {
        if classify_auth_file(&paths.auth_path)? == CodexAuthKind::Account {
            copy_auth_file(&paths.auth_path, &paths.account_slot_path)?;
        }

        let api_key_json = serde_json::to_string(api_key)
            .map_err(|e| format!("failed to encode API key auth: {e}"))?;
        let auth_content =
            format!("{{\"auth_mode\":\"apikey\",\"OPENAI_API_KEY\":{api_key_json}}}\n");
        write_auth_file(&paths.profile_api_key_slot_path, auth_content.as_bytes())?;
        write_auth_file(&paths.written_path, auth_content.as_bytes())?;
        write_auth_file(&paths.auth_path, auth_content.as_bytes())?;
        Ok::<(), String>(())
    })();
    if let Err(error) = result {
        let _ = restore.restore_after_start();
        return Err(error);
    }
    Ok(restore)
}

fn prepare_account_auth_if_needed(
    paths: &CodexAuthPaths,
) -> Result<Option<CodexAuthRestore>, String> {
    if classify_auth_file(&paths.auth_path)? == CodexAuthKind::Account {
        ensure_dir_private(&paths.state_root)?;
        ensure_dir_private(&paths.scope_dir)?;
        copy_auth_file(&paths.auth_path, &paths.account_slot_path)?;
        return Ok(None);
    }
    if paths.lock_dir.exists() {
        recover_stale_lock(paths)?;
    }
    if !paths.account_slot_path.exists() {
        return Ok(None);
    }

    let mut restore = acquire_auth_lock(paths)?;
    let result = (|| {
        copy_auth_file(&paths.account_slot_path, &paths.written_path)?;
        copy_auth_file(&paths.account_slot_path, &paths.auth_path)?;
        Ok::<(), String>(())
    })();
    if let Err(error) = result {
        let _ = restore.restore_after_start();
        return Err(error);
    }
    Ok(Some(restore))
}

fn acquire_auth_lock(paths: &CodexAuthPaths) -> Result<CodexAuthRestore, String> {
    ensure_dir_private(&paths.state_root)?;
    ensure_dir_private(&paths.scope_dir)?;
    match fs::create_dir(&paths.lock_dir) {
        Ok(()) => {}
        Err(_) => {
            recover_stale_lock(paths)?;
            fs::create_dir(&paths.lock_dir)
                .map_err(|e| format!("failed to acquire Codex auth lock: {e}"))?;
        }
    }
    ensure_dir_private(&paths.lock_dir)?;

    if paths.auth_path.exists() {
        if let Err(error) = copy_auth_file(&paths.auth_path, &paths.original_path) {
            let _ = fs::remove_dir_all(&paths.lock_dir);
            return Err(error);
        }
    } else {
        if let Err(error) = File::create(&paths.no_original_path)
            .map_err(|e| format!("failed to record missing original Codex auth: {e}"))
            .and_then(|_| set_private_file_permissions(&paths.no_original_path))
        {
            let _ = fs::remove_dir_all(&paths.lock_dir);
            return Err(error);
        }
    }

    let meta = format!(
        "pid={}\nprofile={}\nauth={}\n",
        std::process::id(),
        paths.profile_name,
        paths.auth_path.display()
    );
    if let Err(error) = write_private_file(&paths.lock_dir.join("meta"), meta.as_bytes()) {
        let _ = fs::remove_dir_all(&paths.lock_dir);
        return Err(error);
    }

    Ok(CodexAuthRestore {
        auth_path: paths.auth_path.clone(),
        state_root: paths.state_root.clone(),
        lock_dir: paths.lock_dir.clone(),
        account_slot_path: paths.account_slot_path.clone(),
        written_path: paths.written_path.clone(),
        original_path: paths.original_path.clone(),
        no_original_path: paths.no_original_path.clone(),
        restored: false,
    })
}

fn recover_stale_lock(paths: &CodexAuthPaths) -> Result<(), String> {
    let meta = fs::read_to_string(paths.lock_dir.join("meta")).unwrap_or_default();
    let pid = meta
        .lines()
        .find_map(|line| line.strip_prefix("pid="))
        .and_then(|value| value.trim().parse::<u32>().ok());
    if pid.is_some_and(process_alive) {
        return Err("Codex auth is being prepared by another kn session".into());
    }

    let written = paths.lock_dir.join("written.auth.json");
    let original = paths.lock_dir.join("original.auth.json");
    let no_original = paths.lock_dir.join("no-original");
    if file_matches(&paths.auth_path, &written)? {
        if original.exists() {
            copy_auth_file(&original, &paths.auth_path)?;
        } else if no_original.exists() && paths.auth_path.exists() {
            fs::remove_file(&paths.auth_path)
                .map_err(|e| format!("failed to remove stale temporary Codex auth: {e}"))?;
        }
    } else if classify_auth_file(&paths.auth_path)? == CodexAuthKind::Account {
        ensure_dir_private(&paths.state_root)?;
        ensure_dir_private(&paths.scope_dir)?;
        copy_auth_file(&paths.auth_path, &paths.account_slot_path)?;
    }
    let _ = fs::remove_dir_all(&paths.lock_dir);
    Ok(())
}

fn process_alive(pid: u32) -> bool {
    #[cfg(unix)]
    {
        unsafe { libc::kill(pid as libc::pid_t, 0) == 0 }
    }
    #[cfg(not(unix))]
    {
        let _ = pid;
        false
    }
}

struct CodexAuthPaths {
    codex_home: PathBuf,
    auth_path: PathBuf,
    state_root: PathBuf,
    scope_dir: PathBuf,
    lock_dir: PathBuf,
    account_slot_path: PathBuf,
    profile_api_key_slot_path: PathBuf,
    original_path: PathBuf,
    written_path: PathBuf,
    no_original_path: PathBuf,
    profile_name: String,
}

impl CodexAuthPaths {
    fn resolve(profile_name: &str) -> Result<Self, String> {
        let codex_home = codex_home();
        let state_root = codex_auth_state_root();
        let kn_home = kn_common::path::config_dir();
        Ok(Self::from_roots(
            profile_name,
            codex_home,
            state_root,
            kn_home,
        ))
    }

    fn from_roots(
        profile_name: &str,
        codex_home: PathBuf,
        state_root: PathBuf,
        kn_home: PathBuf,
    ) -> Self {
        let codex_home = normalize_codex_home(codex_home);
        let auth_path = codex_home.join("auth.json");
        let scope_id = hash_path(&auth_path);
        let scope_dir = state_root.join(scope_id);
        let lock_dir = scope_dir.join("codex-auth.lock");
        let profile_slot_name = format!("{}.auth.json", safe_name(profile_name));
        let profile_api_key_slot_path = kn_home
            .join("codex-auth")
            .join("api-key")
            .join(profile_slot_name);
        Self {
            codex_home: codex_home.clone(),
            auth_path,
            state_root,
            account_slot_path: scope_dir.join("account.auth.json"),
            original_path: lock_dir.join("original.auth.json"),
            written_path: lock_dir.join("written.auth.json"),
            no_original_path: lock_dir.join("no-original"),
            scope_dir,
            lock_dir,
            profile_api_key_slot_path,
            profile_name: profile_name.to_string(),
        }
    }

    fn for_profile(&self, profile_name: &str) -> Self {
        Self::from_roots(
            profile_name,
            self.codex_home.clone(),
            self.scope_dir
                .parent()
                .map(Path::to_path_buf)
                .unwrap_or_else(|| codex_auth_state_root()),
            self.profile_api_key_slot_path
                .parent()
                .and_then(Path::parent)
                .and_then(Path::parent)
                .map(Path::to_path_buf)
                .unwrap_or_else(kn_common::path::config_dir),
        )
    }
}

fn codex_home() -> PathBuf {
    std::env::var_os("CODEX_HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| kn_common::path::home_dir().join(".codex"))
}

fn normalize_codex_home(path: PathBuf) -> PathBuf {
    if let Ok(canonical) = path.canonicalize() {
        return canonical;
    }
    if let Some(parent) = path.parent() {
        if let Ok(canonical_parent) = parent.canonicalize() {
            if let Some(file_name) = path.file_name() {
                return canonical_parent.join(file_name);
            }
        }
    }
    if path.is_absolute() {
        path
    } else {
        std::env::current_dir()
            .map(|cwd| cwd.join(&path))
            .unwrap_or(path)
    }
}

fn codex_auth_state_root() -> PathBuf {
    std::env::var_os("KN_CODEX_AUTH_STATE_DIR")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| kn_common::path::home_dir().join(".kn-codex-auth"))
}

fn ensure_file_auth_storage_at(codex_home: &Path) -> Result<(), String> {
    let config = codex_home.join("config.toml");
    let Ok(text) = fs::read_to_string(config) else {
        return Ok(());
    };
    let explicit_keyring = text
        .parse::<toml::Value>()
        .ok()
        .and_then(|value| {
            value
                .get("cli_auth_credentials_store")
                .and_then(toml::Value::as_str)
                .map(|value| value.eq_ignore_ascii_case("keyring"))
        })
        .unwrap_or_else(|| {
            text.lines().any(|line| {
                let without_comment = line.split('#').next().unwrap_or_default().trim();
                let Some((key, value)) = without_comment.split_once('=') else {
                    return false;
                };
                key.trim() == "cli_auth_credentials_store"
                    && value.trim().trim_matches('"').trim_matches('\'') == "keyring"
            })
        });
    if explicit_keyring {
        return Err(
            "Codex keyring auth storage is configured; kn only manages file auth storage".into(),
        );
    }
    Ok(())
}

pub(crate) fn classify_auth_file(path: &Path) -> Result<CodexAuthKind, String> {
    let Ok(text) = fs::read_to_string(path) else {
        return Ok(CodexAuthKind::Unknown);
    };
    classify_auth_text(&text)
}

pub(crate) fn classify_auth_text(text: &str) -> Result<CodexAuthKind, String> {
    let Ok(value) = serde_json::from_str::<Value>(text) else {
        return Ok(CodexAuthKind::Unknown);
    };
    let has_api_key = value.get("OPENAI_API_KEY").is_some();
    let auth_mode = value
        .get("auth_mode")
        .and_then(Value::as_str)
        .map(|s| s.to_ascii_lowercase());
    if has_api_key || matches!(auth_mode.as_deref(), Some("apikey" | "api_key")) {
        return Ok(CodexAuthKind::ApiKey);
    }
    if !has_api_key
        && (auth_mode.as_deref() == Some("chatgpt")
            || value.get("tokens").is_some()
            || value.get("ChatgptAuthTokens").is_some())
    {
        return Ok(CodexAuthKind::Account);
    }
    Ok(CodexAuthKind::Unknown)
}

fn file_matches(left: &Path, right: &Path) -> Result<bool, String> {
    if !left.exists() || !right.exists() {
        return Ok(false);
    }
    Ok(file_hash(left)? == file_hash(right)?)
}

fn file_hash(path: &Path) -> Result<String, String> {
    let bytes = fs::read(path).map_err(|e| format!("failed to read Codex auth file: {e}"))?;
    Ok(hex::encode(Sha256::digest(bytes)))
}

fn hash_path(path: &Path) -> String {
    hex::encode(Sha256::digest(path.to_string_lossy().as_bytes()))[..16].to_string()
}

fn safe_name(value: &str) -> String {
    let mut out = String::new();
    for ch in value.chars() {
        if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.') {
            out.push(ch);
        } else {
            out.push('_');
        }
    }
    if out.is_empty() {
        "profile".into()
    } else {
        out
    }
}

fn copy_auth_file(src: &Path, dst: &Path) -> Result<(), String> {
    let bytes = fs::read(src).map_err(|e| format!("failed to read Codex auth file: {e}"))?;
    write_auth_file(dst, &bytes)
}

fn write_auth_file(path: &Path, bytes: &[u8]) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("failed to create Codex auth file dir: {e}"))?;
    }
    write_private_file(path, bytes)
}

fn write_private_file(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let (tmp, mut file) = create_unique_temp_file(path)?;
    {
        if let Err(error) = set_private_file_permissions(&tmp) {
            let _ = fs::remove_file(&tmp);
            return Err(error);
        }
        file.write_all(bytes).map_err(|e| {
            let _ = fs::remove_file(&tmp);
            format!("failed to write temporary Codex auth file: {e}")
        })?;
        file.sync_all().map_err(|e| {
            let _ = fs::remove_file(&tmp);
            format!("failed to sync temporary Codex auth file: {e}")
        })?;
    }
    fs::rename(&tmp, path).map_err(|e| {
        let _ = fs::remove_file(&tmp);
        format!("failed to install Codex auth file: {e}")
    })?;
    if let Some(parent) = path.parent() {
        if let Ok(dir) = File::open(parent) {
            let _ = dir.sync_all();
        }
    }
    Ok(())
}

fn create_unique_temp_file(path: &Path) -> Result<(PathBuf, File), String> {
    let mut last_exists_error = None;
    for _ in 0..32 {
        let tmp = unique_temp_path(path);
        match OpenOptions::new().write(true).create_new(true).open(&tmp) {
            Ok(file) => return Ok((tmp, file)),
            Err(error) if error.kind() == ErrorKind::AlreadyExists => {
                last_exists_error = Some(error);
            }
            Err(error) => {
                return Err(format!(
                    "failed to create temporary Codex auth file: {error}"
                ));
            }
        }
    }
    Err(format!(
        "failed to create unique temporary Codex auth file: {}",
        last_exists_error
            .map(|error| error.to_string())
            .unwrap_or_else(|| "too many collisions".into())
    ))
}

fn unique_temp_path(path: &Path) -> PathBuf {
    let counter = AUTH_TMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let suffix = format!("tmp.{}.{}", std::process::id(), counter);
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("auth");
    path.with_file_name(format!("{file_name}.{suffix}"))
}

fn ensure_parent_private(path: &Path) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        ensure_dir_private(parent)?;
    }
    Ok(())
}

fn ensure_dir_private(path: &Path) -> Result<(), String> {
    fs::create_dir_all(path).map_err(|e| format!("failed to create Codex auth state dir: {e}"))?;
    #[cfg(unix)]
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))
        .map_err(|e| format!("failed to chmod Codex auth state dir: {e}"))?;
    Ok(())
}

fn set_private_file_permissions(path: &Path) -> Result<(), String> {
    #[cfg(unix)]
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))
        .map_err(|e| format!("failed to chmod Codex auth file: {e}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env_map(values: &[(&str, &str)]) -> HashMap<String, String> {
        values
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    fn test_paths(
        profile_name: &str,
        codex_home: &Path,
        state_dir: &Path,
        kn_home: &Path,
    ) -> CodexAuthPaths {
        CodexAuthPaths::from_roots(
            profile_name,
            codex_home.to_path_buf(),
            state_dir.to_path_buf(),
            kn_home.to_path_buf(),
        )
    }

    fn test_scope_dir(codex_home: &Path, state_dir: &Path, kn_home: &Path) -> PathBuf {
        test_paths("scope", codex_home, state_dir, kn_home).scope_dir
    }

    #[test]
    fn classifies_codex_auth_shapes() {
        assert_eq!(
            classify_auth_text(r#"{"auth_mode":"chatgpt","tokens":{"id_token":"x"}}"#).unwrap(),
            CodexAuthKind::Account
        );
        assert_eq!(
            classify_auth_text(r#"{"tokens":{"id_token":"x"}}"#).unwrap(),
            CodexAuthKind::Account
        );
        assert_eq!(
            classify_auth_text(r#"{"auth_mode":"apikey","OPENAI_API_KEY":"sk"}"#).unwrap(),
            CodexAuthKind::ApiKey
        );
        assert_eq!(
            classify_auth_text(r#"{"auth_mode":"api_key"}"#).unwrap(),
            CodexAuthKind::ApiKey
        );
        assert_eq!(
            classify_auth_text(r#"{"hello":"world"}"#).unwrap(),
            CodexAuthKind::Unknown
        );
    }

    #[test]
    fn path_layers_keep_kn_state_outside_codex_home_and_split_scopes() {
        let tmp = tempfile::tempdir().unwrap();
        let kn_home = tmp.path().join("kn");
        let state_dir = tmp.path().join("state");
        let codex_home_one = tmp.path().join("codex-one");
        let codex_home_two = tmp.path().join("codex-two");

        let paths_one = test_paths("codex-key", &codex_home_one, &state_dir, &kn_home);
        let paths_two = test_paths("codex-key", &codex_home_two, &state_dir, &kn_home);

        assert_eq!(
            paths_one.auth_path,
            normalize_codex_home(codex_home_one.clone()).join("auth.json")
        );
        assert!(paths_one.account_slot_path.starts_with(&state_dir));
        assert!(paths_one.lock_dir.starts_with(&state_dir));
        assert!(paths_one.profile_api_key_slot_path.starts_with(&kn_home));
        assert_ne!(paths_one.scope_dir, paths_two.scope_dir);
        assert!(!paths_one.scope_dir.starts_with(&codex_home_one));
    }

    #[test]
    fn codex_home_trailing_slash_uses_the_same_scope_lock() {
        let tmp = tempfile::tempdir().unwrap();
        let kn_home = tmp.path().join("kn");
        let state_dir = tmp.path().join("state");
        let codex_home = tmp.path().join("codex");
        fs::create_dir_all(&codex_home).unwrap();
        let codex_home_with_slash = PathBuf::from(format!("{}/", codex_home.display()));

        let paths_one = test_paths("codex-key", &codex_home, &state_dir, &kn_home);
        let paths_two = test_paths("codex-key", &codex_home_with_slash, &state_dir, &kn_home);

        assert_eq!(paths_one.auth_path, paths_two.auth_path);
        assert_eq!(paths_one.lock_dir, paths_two.lock_dir);
    }

    #[test]
    fn auth_temp_paths_are_unique_per_write() {
        let target = PathBuf::from("/tmp/auth.json");

        assert_ne!(unique_temp_path(&target), unique_temp_path(&target));
    }

    #[test]
    fn restore_grace_period_defaults_and_caps() {
        assert_eq!(
            restore_grace_period_from_value(None),
            Duration::from_millis(500)
        );
        assert_eq!(
            restore_grace_period_from_value(Some("25".into())),
            Duration::from_millis(25)
        );
        assert_eq!(
            restore_grace_period_from_value(Some("not-a-number".into())),
            Duration::from_millis(500)
        );
        assert_eq!(
            restore_grace_period_from_value(Some("6000".into())),
            Duration::from_millis(5000)
        );
    }

    #[test]
    fn api_key_profile_uses_state_outside_codex_home_and_restores_after_start() {
        let tmp = tempfile::tempdir().unwrap();
        let kn_home = tmp.path().join("kn-dev");
        let codex_home = tmp.path().join("codex");
        let state_dir = tmp.path().join("state");
        fs::create_dir_all(&codex_home).unwrap();
        fs::write(
            codex_home.join("auth.json"),
            r#"{"auth_mode":"chatgpt","tokens":{"id_token":"original"}}"#,
        )
        .unwrap();
        let original = fs::read_to_string(codex_home.join("auth.json")).unwrap();
        let env = env_map(&[
            ("OPENAI_API_KEY", "sk-test"),
            ("OPENAI_BASE_URL", "https://proxy.example.com/v1"),
            ("OPENAI_MODEL", "gpt-test"),
            ("_KN_AUTH_MODE", "api_key"),
        ]);
        let paths = test_paths("codex-key", &codex_home, &state_dir, &kn_home);
        let (args, mut restore) = codex_auth_args_and_restore_with_paths(&env, paths).unwrap();
        assert!(args.iter().any(|arg| arg == "model_provider=\"custom\""));
        assert!(args
            .iter()
            .any(|arg| arg == "model_providers.custom.env_key=\"OPENAI_API_KEY\""));
        assert!(fs::read_to_string(codex_home.join("auth.json"))
            .unwrap()
            .contains("OPENAI_API_KEY"));
        assert!(!codex_home.join("kn-auth").exists());
        assert!(state_dir.read_dir().unwrap().next().is_some());
        #[cfg(unix)]
        assert_eq!(
            fs::metadata(&state_dir).unwrap().permissions().mode() & 0o777,
            0o700
        );
        assert!(kn_home.join("codex-auth/api-key").exists());

        restore.as_mut().unwrap().restore_after_start().unwrap();
        assert_eq!(
            fs::read_to_string(codex_home.join("auth.json")).unwrap(),
            original
        );
    }

    #[test]
    #[cfg(unix)]
    fn api_key_profile_does_not_chmod_codex_home() {
        let tmp = tempfile::tempdir().unwrap();
        let kn_home = tmp.path().join("kn");
        let codex_home = tmp.path().join("codex");
        let state_dir = tmp.path().join("state");
        fs::create_dir_all(&codex_home).unwrap();
        fs::set_permissions(&codex_home, fs::Permissions::from_mode(0o755)).unwrap();
        let env = env_map(&[("OPENAI_API_KEY", "sk-test")]);
        let paths = test_paths("openai-key", &codex_home, &state_dir, &kn_home);

        let (_, mut restore) = codex_auth_args_and_restore_with_paths(&env, paths).unwrap();

        assert_eq!(
            fs::metadata(&codex_home).unwrap().permissions().mode() & 0o777,
            0o755
        );
        restore.as_mut().unwrap().restore_after_start().unwrap();
    }

    #[test]
    fn api_key_profile_without_base_url_uses_openai_provider() {
        let tmp = tempfile::tempdir().unwrap();
        let kn_home = tmp.path().join("kn");
        let codex_home = tmp.path().join("codex");
        let state_dir = tmp.path().join("state");
        let env = env_map(&[("OPENAI_API_KEY", "sk-test")]);

        let paths = test_paths("openai-key", &codex_home, &state_dir, &kn_home);
        let (args, mut restore) = codex_auth_args_and_restore_with_paths(&env, paths).unwrap();
        assert!(args.iter().any(|arg| arg == "model_provider=\"openai\""));
        restore.as_mut().unwrap().restore_after_start().unwrap();
        assert!(!codex_home.join("auth.json").exists());
    }

    #[test]
    fn local_login_ignores_base_url_and_uses_account_slot_temporarily() {
        let tmp = tempfile::tempdir().unwrap();
        let kn_home = tmp.path().join("kn");
        let codex_home = tmp.path().join("codex");
        let state_dir = tmp.path().join("state");
        fs::create_dir_all(&codex_home).unwrap();
        fs::write(codex_home.join("auth.json"), r#"{"hello":"original"}"#).unwrap();
        let scope = test_scope_dir(&codex_home, &state_dir, &kn_home);
        fs::create_dir_all(&scope).unwrap();
        fs::write(
            scope.join("account.auth.json"),
            r#"{"auth_mode":"chatgpt","tokens":{"id_token":"slot"}}"#,
        )
        .unwrap();
        let env = env_map(&[
            ("_KN_AUTH_MODE", "local_login"),
            ("OPENAI_BASE_URL", "https://should-be-ignored.example/v1"),
        ]);

        let paths = test_paths("codex-login", &codex_home, &state_dir, &kn_home);
        let (args, mut restore) = codex_auth_args_and_restore_with_paths(&env, paths).unwrap();
        assert!(args.iter().any(|arg| arg == "model_provider=\"openai\""));
        assert!(!args.iter().any(|arg| arg.contains("should-be-ignored")));
        assert!(fs::read_to_string(codex_home.join("auth.json"))
            .unwrap()
            .contains("chatgpt"));

        restore.as_mut().unwrap().restore_after_start().unwrap();
        assert_eq!(
            fs::read_to_string(codex_home.join("auth.json")).unwrap(),
            r#"{"hello":"original"}"#
        );
    }

    #[test]
    fn local_login_current_account_refreshes_slot_without_swapping_auth() {
        let tmp = tempfile::tempdir().unwrap();
        let kn_home = tmp.path().join("kn");
        let codex_home = tmp.path().join("codex");
        let state_dir = tmp.path().join("state");
        let account = r#"{"auth_mode":"chatgpt","tokens":{"id_token":"current"}}"#;
        fs::create_dir_all(&codex_home).unwrap();
        fs::write(codex_home.join("auth.json"), account).unwrap();
        let env = env_map(&[
            ("_KN_AUTH_MODE", "local_login"),
            ("OPENAI_BASE_URL", "https://should-be-ignored.example/v1"),
        ]);

        let paths = test_paths("codex-login", &codex_home, &state_dir, &kn_home);
        let (args, restore) = codex_auth_args_and_restore_with_paths(&env, paths).unwrap();

        assert!(restore.is_none());
        assert!(args.iter().any(|arg| arg == "model_provider=\"openai\""));
        assert!(!args.iter().any(|arg| arg.contains("should-be-ignored")));
        assert_eq!(
            fs::read_to_string(codex_home.join("auth.json")).unwrap(),
            account
        );
        assert_eq!(
            fs::read_to_string(
                test_scope_dir(&codex_home, &state_dir, &kn_home).join("account.auth.json")
            )
            .unwrap(),
            account
        );
    }

    #[test]
    fn local_login_auth_mode_wins_over_residual_api_key_env() {
        let tmp = tempfile::tempdir().unwrap();
        let kn_home = tmp.path().join("kn");
        let codex_home = tmp.path().join("codex");
        let state_dir = tmp.path().join("state");
        fs::create_dir_all(&codex_home).unwrap();
        fs::write(codex_home.join("auth.json"), r#"{"hello":"original"}"#).unwrap();
        let scope = test_scope_dir(&codex_home, &state_dir, &kn_home);
        fs::create_dir_all(&scope).unwrap();
        fs::write(
            scope.join("account.auth.json"),
            r#"{"auth_mode":"chatgpt","tokens":{"id_token":"slot"}}"#,
        )
        .unwrap();
        let env = env_map(&[
            ("_KN_AUTH_MODE", "local_login"),
            ("OPENAI_API_KEY", "sk-residual"),
            ("OPENAI_BASE_URL", "https://should-be-ignored.example/v1"),
        ]);

        let paths = test_paths("codex-login", &codex_home, &state_dir, &kn_home);
        let (args, mut restore) = codex_auth_args_and_restore_with_paths(&env, paths).unwrap();

        assert!(args.iter().any(|arg| arg == "model_provider=\"openai\""));
        assert!(!args.iter().any(|arg| arg.contains("custom")));
        assert!(!kn_home
            .join("codex-auth/api-key/codex-login.auth.json")
            .exists());
        assert!(fs::read_to_string(codex_home.join("auth.json"))
            .unwrap()
            .contains("chatgpt"));
        restore.as_mut().unwrap().restore_after_start().unwrap();
    }

    #[test]
    fn local_login_rejects_live_lock_when_auth_is_not_account() {
        let tmp = tempfile::tempdir().unwrap();
        let kn_home = tmp.path().join("kn");
        let codex_home = tmp.path().join("codex");
        let state_dir = tmp.path().join("state");
        let paths = test_paths("codex-login", &codex_home, &state_dir, &kn_home);
        fs::create_dir_all(&paths.lock_dir).unwrap();
        fs::write(
            paths.lock_dir.join("meta"),
            format!("pid={}\n", std::process::id()),
        )
        .unwrap();
        fs::create_dir_all(&codex_home).unwrap();
        fs::write(
            codex_home.join("auth.json"),
            r#"{"auth_mode":"apikey","OPENAI_API_KEY":"temporary"}"#,
        )
        .unwrap();
        let env = env_map(&[("_KN_AUTH_MODE", "local_login")]);

        let error = codex_auth_args_and_restore_with_paths(&env, paths).unwrap_err();

        assert!(error.contains("being prepared"));
    }

    #[test]
    fn restore_keeps_external_changes_and_refreshes_only_account_slot() {
        let tmp = tempfile::tempdir().unwrap();
        let kn_home = tmp.path().join("kn");
        let codex_home = tmp.path().join("codex");
        let state_dir = tmp.path().join("state");
        let env = env_map(&[("OPENAI_API_KEY", "sk-test")]);
        let paths = test_paths("key", &codex_home, &state_dir, &kn_home);
        let (_, mut restore) = codex_auth_args_and_restore_with_paths(&env, paths).unwrap();
        fs::write(
            codex_home.join("auth.json"),
            r#"{"auth_mode":"chatgpt","tokens":{"id_token":"external"}}"#,
        )
        .unwrap();

        restore.as_mut().unwrap().restore_after_start().unwrap();
        let current = fs::read_to_string(codex_home.join("auth.json")).unwrap();
        assert!(current.contains("external"));
        let scope = test_scope_dir(&codex_home, &state_dir, &kn_home);
        assert!(fs::read_to_string(scope.join("account.auth.json"))
            .unwrap()
            .contains("external"));
    }

    #[test]
    fn restore_keeps_external_api_key_without_refreshing_account_slot() {
        let tmp = tempfile::tempdir().unwrap();
        let kn_home = tmp.path().join("kn");
        let codex_home = tmp.path().join("codex");
        let state_dir = tmp.path().join("state");
        let env = env_map(&[("OPENAI_API_KEY", "sk-test")]);
        let paths = test_paths("key", &codex_home, &state_dir, &kn_home);
        let (_, mut restore) = codex_auth_args_and_restore_with_paths(&env, paths).unwrap();
        fs::write(
            codex_home.join("auth.json"),
            r#"{"auth_mode":"apikey","OPENAI_API_KEY":"external"}"#,
        )
        .unwrap();

        restore.as_mut().unwrap().restore_after_start().unwrap();
        assert!(fs::read_to_string(codex_home.join("auth.json"))
            .unwrap()
            .contains("external"));
        let scope = test_scope_dir(&codex_home, &state_dir, &kn_home);
        assert!(!scope.join("account.auth.json").exists());
    }

    #[test]
    fn restore_keeps_external_unknown_without_refreshing_account_slot() {
        let tmp = tempfile::tempdir().unwrap();
        let kn_home = tmp.path().join("kn");
        let codex_home = tmp.path().join("codex");
        let state_dir = tmp.path().join("state");
        let env = env_map(&[("OPENAI_API_KEY", "sk-test")]);
        let paths = test_paths("key", &codex_home, &state_dir, &kn_home);
        let (_, mut restore) = codex_auth_args_and_restore_with_paths(&env, paths).unwrap();
        fs::write(codex_home.join("auth.json"), r#"{"hello":"external"}"#).unwrap();

        restore.as_mut().unwrap().restore_after_start().unwrap();

        assert_eq!(
            fs::read_to_string(codex_home.join("auth.json")).unwrap(),
            r#"{"hello":"external"}"#
        );
        let scope = test_scope_dir(&codex_home, &state_dir, &kn_home);
        assert!(!scope.join("account.auth.json").exists());
    }

    #[test]
    fn keyring_detection_ignores_inline_comment_when_store_is_file() {
        let tmp = tempfile::tempdir().unwrap();
        let codex_home = tmp.path().join("codex");
        fs::create_dir_all(&codex_home).unwrap();
        fs::write(
            codex_home.join("config.toml"),
            r#"cli_auth_credentials_store = "file" # keyring is disabled"#,
        )
        .unwrap();

        ensure_file_auth_storage_at(&codex_home).unwrap();
    }

    #[test]
    fn keyring_detection_rejects_explicit_keyring() {
        let tmp = tempfile::tempdir().unwrap();
        let codex_home = tmp.path().join("codex");
        fs::create_dir_all(&codex_home).unwrap();
        fs::write(
            codex_home.join("config.toml"),
            r#"cli_auth_credentials_store = "keyring""#,
        )
        .unwrap();

        assert!(ensure_file_auth_storage_at(&codex_home).is_err());
    }
}
