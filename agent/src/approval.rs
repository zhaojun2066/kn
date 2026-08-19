//! Remote approval coordination for structured CLI hook events.
//!
//! This module deliberately never inspects PTY output.  Only `PermissionRequest`
//! and `PreToolUse` hook payloads are considered, so prompts printed by shell
//! subprocesses (for example `Proceed? [y/N]`) remain ordinary terminal input.

use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::Path;

use serde::{Deserialize, Serialize};
use tokio::sync::{oneshot, Mutex};

const PREVIEW_LIMIT: usize = 1_800;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ApprovalMode {
    Disabled,
    NativePermission,
    PreToolUse,
}

impl Default for ApprovalMode {
    fn default() -> Self {
        Self::Disabled
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RiskRules {
    pub destructive_filesystem: bool,
    pub force_git: bool,
    pub deploy_publish: bool,
    pub project_external_write: bool,
    pub credentials_security: bool,
}

impl Default for RiskRules {
    fn default() -> Self {
        Self {
            destructive_filesystem: true,
            force_git: true,
            deploy_publish: true,
            project_external_write: true,
            credentials_security: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApprovalConfig {
    pub enabled: bool,
    pub claude_mode: ApprovalMode,
    pub codex_mode: ApprovalMode,
    pub qoder_cn_mode: ApprovalMode,
    #[serde(default)]
    pub rules: RiskRules,
}

impl Default for ApprovalConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            claude_mode: ApprovalMode::NativePermission,
            codex_mode: ApprovalMode::NativePermission,
            qoder_cn_mode: ApprovalMode::PreToolUse,
            rules: RiskRules::default(),
        }
    }
}

impl ApprovalConfig {
    pub fn mode_for(&self, cli_type: &str) -> ApprovalMode {
        match cli_type {
            "claude" => self.claude_mode.clone(),
            "codex" => self.codex_mode.clone(),
            "qoderclicn" => self.qoder_cn_mode.clone(),
            _ => ApprovalMode::Disabled,
        }
    }
}

pub fn config_path(config_dir: &Path) -> std::path::PathBuf {
    config_dir.join("agent").join("approval-config.json")
}

pub fn load_config(config_dir: &Path) -> ApprovalConfig {
    let path = config_path(config_dir);
    fs::read_to_string(path)
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default()
}

pub fn save_config(config_dir: &Path, config: &ApprovalConfig) -> Result<(), String> {
    let path = config_path(config_dir);
    let parent = path.parent().ok_or("审批配置目录无效")?;
    fs::create_dir_all(parent).map_err(|e| format!("创建审批配置目录失败: {e}"))?;
    let content =
        serde_json::to_string_pretty(config).map_err(|e| format!("序列化审批配置失败: {e}"))?;
    let temporary = parent.join(format!(
        ".approval-config-{}-{}.tmp",
        std::process::id(),
        nanoid::nanoid!(8)
    ));
    let write_result = (|| -> Result<(), String> {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(|e| format!("创建审批临时配置失败: {e}"))?;
        file.write_all(content.as_bytes())
            .map_err(|e| format!("写入审批临时配置失败: {e}"))?;
        file.sync_all()
            .map_err(|e| format!("同步审批临时配置失败: {e}"))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&temporary, fs::Permissions::from_mode(0o600))
                .map_err(|e| format!("设置审批配置权限失败: {e}"))?;
        }
        fs::rename(&temporary, &path).map_err(|e| format!("替换审批配置失败: {e}"))?;
        Ok(())
    })();
    if write_result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    write_result
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HookApprovalRequest {
    pub request_key: String,
    pub session_id: String,
    pub cli_type: String,
    pub event_name: String,
    #[serde(default)]
    pub payload: serde_json::Value,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ApprovalDecision {
    AllowOnce,
    Deny,
}

impl ApprovalDecision {
    pub fn from_wire(value: &str) -> Option<Self> {
        match value {
            "allowOnce" | "approved" | "allow" => Some(Self::AllowOnce),
            "deny" | "denied" => Some(Self::Deny),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApprovalRequestedEvent {
    pub request_key: String,
    pub session_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_id: Option<u64>,
    pub cli_type: String,
    pub source: String,
    pub tool_name: String,
    pub canonical_tool_type: String,
    pub risk_category: Option<String>,
    pub title: String,
    pub summary: String,
    pub instruction_preview: String,
    pub target_preview: Option<String>,
    pub detail_snapshot: serde_json::Value,
    pub expires_at: i64,
}

#[derive(Debug, Clone)]
pub struct PreparedApproval {
    pub event: ApprovalRequestedEvent,
}

/// In-memory waiters are deliberately ephemeral.  If the agent restarts, a
/// hook cannot safely continue and must receive a deny result.
#[derive(Default)]
pub struct ApprovalCoordinator {
    waiters: Mutex<HashMap<String, PendingApproval>>,
}

struct PendingApproval {
    session_id: String,
    sender: oneshot::Sender<ApprovalDecision>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AbandonedApproval {
    pub request_key: String,
    pub session_id: String,
}

impl ApprovalCoordinator {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn register(
        &self,
        request_key: &str,
        session_id: &str,
    ) -> Result<oneshot::Receiver<ApprovalDecision>, String> {
        let mut waiters = self.waiters.lock().await;
        if waiters.contains_key(request_key) {
            return Err("相同审批请求仍在等待中".to_string());
        }
        let (tx, rx) = oneshot::channel();
        waiters.insert(
            request_key.to_string(),
            PendingApproval {
                session_id: session_id.to_string(),
                sender: tx,
            },
        );
        Ok(rx)
    }

    pub async fn remove(&self, request_key: &str) {
        self.waiters.lock().await.remove(request_key);
    }

    pub async fn decide(&self, request_key: &str, decision: ApprovalDecision) -> bool {
        let sender = self.waiters.lock().await.remove(request_key);
        sender.is_some_and(|pending| pending.sender.send(decision).is_ok())
    }

    pub async fn pending_keys(&self) -> Vec<String> {
        self.waiters.lock().await.keys().cloned().collect()
    }

    /// Drops all local hook waiters so shutdown cannot leave a Cloud request
    /// pending after the agent is gone. Callers must notify Cloud first.
    pub async fn drain_pending(&self) -> Vec<AbandonedApproval> {
        let mut waiters = self.waiters.lock().await;
        waiters
            .drain()
            .map(|(request_key, pending)| AbandonedApproval {
                request_key,
                session_id: pending.session_id,
            })
            .collect()
    }

    pub async fn drain_session(&self, session_id: &str) -> Vec<AbandonedApproval> {
        let mut waiters = self.waiters.lock().await;
        let mut abandoned = Vec::new();
        waiters.retain(|request_key, pending| {
            if pending.session_id == session_id {
                abandoned.push(AbandonedApproval {
                    request_key: request_key.clone(),
                    session_id: pending.session_id.clone(),
                });
                false
            } else {
                true
            }
        });
        abandoned
    }
}

pub fn prepare_request(
    request: &HookApprovalRequest,
    config: &ApprovalConfig,
    cwd: &str,
    project_id: Option<u64>,
    now_ms: i64,
) -> Option<PreparedApproval> {
    if !config.enabled
        || request.request_key.trim().is_empty()
        || request.session_id.trim().is_empty()
    {
        return None;
    }
    let cli_type = cloud_cli_type(&request.cli_type)?;
    let mode = config.mode_for(cli_type);
    let event_name = request.event_name.trim();
    let native = event_name == "PermissionRequest" && mode == ApprovalMode::NativePermission;
    let pre_tool = event_name == "PreToolUse" && mode == ApprovalMode::PreToolUse;
    if !native && !pre_tool {
        return None;
    }

    let tool_name = json_string(&request.payload, &["tool_name", "toolName", "name"])
        .unwrap_or_else(|| "未知工具".to_string());
    let input = request
        .payload
        .get("tool_input")
        .or_else(|| request.payload.get("toolInput"))
        .cloned()
        .unwrap_or_else(|| serde_json::json!({}));
    let canonical_tool_type = canonical_tool_type(&tool_name);
    let instruction = instruction_from(&tool_name, &input);
    let target = target_from(&input).map(|value| {
        if matches!(canonical_tool_type.as_str(), "fileWrite" | "fileEdit") {
            resolve_target_path(&value, cwd)
        } else {
            value
        }
    });
    let risk_category = if pre_tool {
        match_risk(
            &canonical_tool_type,
            &instruction,
            target.as_deref(),
            cwd,
            &config.rules,
        )?
    } else {
        "nativePermission".to_string()
    };
    let safe_instruction = sanitize_preview(&instruction);
    let safe_target = target.map(|value| sanitize_preview(&value));
    let title = if native {
        format!("{} 请求授权", display_cli(&request.cli_type))
    } else {
        risk_title(&risk_category).to_string()
    };
    let summary = if native {
        format!("{} 即将调用 {}", display_cli(&request.cli_type), tool_name)
    } else {
        format!(
            "{} 即将调用 {}，命中 {} 规则",
            display_cli(&request.cli_type),
            tool_name,
            risk_title(&risk_category)
        )
    };
    let detail_snapshot = serde_json::json!({
        "toolName": tool_name,
        "canonicalToolType": canonical_tool_type,
        "instruction": safe_instruction,
        "target": safe_target,
        "riskCategory": risk_category,
    });
    Some(PreparedApproval {
        event: ApprovalRequestedEvent {
            request_key: request.request_key.trim().to_string(),
            session_id: request.session_id.trim().to_string(),
            cli_type: cli_type.to_string(),
            source: if native {
                "nativePermission".to_string()
            } else {
                "preToolUse".to_string()
            },
            project_id,
            tool_name,
            canonical_tool_type,
            risk_category: if native { None } else { Some(risk_category) },
            title,
            summary,
            instruction_preview: safe_instruction,
            target_preview: safe_target,
            detail_snapshot,
            expires_at: now_ms.saturating_add(300_000),
        },
    })
}

fn json_string(value: &serde_json::Value, keys: &[&str]) -> Option<String> {
    keys.iter()
        .find_map(|key| value.get(*key)?.as_str().map(str::to_string))
}

fn canonical_tool_type(tool_name: &str) -> String {
    match tool_name.to_ascii_lowercase().as_str() {
        "bash" | "shell" | "command" | "terminal" => "shell".to_string(),
        "write" | "createfile" => "fileWrite".to_string(),
        "edit" | "apply_patch" | "applypatch" => "fileEdit".to_string(),
        "git" => "git".to_string(),
        "deploy" | "publish" => "deploy".to_string(),
        _ => "unknown".to_string(),
    }
}

fn instruction_from(tool_name: &str, input: &serde_json::Value) -> String {
    for key in ["command", "cmd", "script", "description"] {
        if let Some(value) = input.get(key).and_then(|item| item.as_str()) {
            return value.to_string();
        }
    }
    // File content, patches and arbitrary tool JSON may contain secrets. The
    // separately whitelisted target preview is enough context for those tools.
    tool_name.to_string()
}

fn target_from(input: &serde_json::Value) -> Option<String> {
    ["file_path", "filePath", "path", "target", "environment"]
        .iter()
        .find_map(|key| input.get(*key)?.as_str().map(str::to_string))
}

fn resolve_target_path(target: &str, cwd: &str) -> String {
    let path = std::path::Path::new(target);
    let combined = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::path::Path::new(cwd).join(path)
    };
    normalize_lexical_path(&combined)
        .to_string_lossy()
        .to_string()
}

fn normalize_lexical_path(path: &std::path::Path) -> std::path::PathBuf {
    use std::path::Component;

    let mut result = std::path::PathBuf::new();
    for component in path.components() {
        match component {
            Component::Prefix(prefix) => result.push(prefix.as_os_str()),
            Component::RootDir => result.push(component.as_os_str()),
            Component::CurDir => {}
            Component::ParentDir => {
                let _ = result.pop();
            }
            Component::Normal(segment) => result.push(segment),
        }
    }
    result
}

fn match_risk(
    tool_type: &str,
    instruction: &str,
    target: Option<&str>,
    cwd: &str,
    rules: &RiskRules,
) -> Option<String> {
    let tokens = command_tokens(instruction);
    let target_lower = target.unwrap_or_default().to_ascii_lowercase();
    if rules.force_git && is_force_git(&tokens) {
        return Some("forceGit".to_string());
    }
    if rules.destructive_filesystem && is_destructive_filesystem(&tokens) {
        return Some("destructiveFilesystem".to_string());
    }
    if rules.deploy_publish && is_publish_deploy(&tokens) {
        return Some("publishDeploy".to_string());
    }
    if rules.credentials_security
        && (target_lower.ends_with(".env")
            || target_lower.contains("/.env.")
            || target_lower.contains(".ssh/")
            || target_lower.ends_with(".pem")
            || target_lower.ends_with(".key")
            || target_lower.ends_with("id_rsa")
            || target_lower.ends_with("authorized_keys")
            || target_lower.ends_with("credentials")
            || target_lower.ends_with("secrets")
            || contains_token(&tokens, "authorization:")
            || has_sequence(&tokens, &["private", "key"])
            || tokens
                .iter()
                .any(|token| looks_like_secret_assignment(token)))
    {
        return Some("credentialSecurity".to_string());
    }
    if rules.project_external_write && matches!(tool_type, "fileWrite" | "fileEdit") {
        if let Some(path) = target.filter(|value| !value.is_empty()) {
            if std::path::Path::new(path).is_absolute() && !path_within(path, cwd) {
                return Some("projectExternalWrite".to_string());
            }
        }
    }
    None
}

fn command_tokens(instruction: &str) -> Vec<String> {
    instruction
        .split(|character: char| {
            character.is_whitespace() || matches!(character, ';' | '|' | '&' | '(' | ')')
        })
        .map(|part| part.trim_matches(|character| matches!(character, '\'' | '"' | '`' | ',')))
        .filter(|part| !part.is_empty())
        .map(|part| part.to_ascii_lowercase())
        .collect()
}

fn contains_token(tokens: &[String], expected: &str) -> bool {
    tokens.iter().any(|token| token == expected)
}

fn has_sequence(tokens: &[String], expected: &[&str]) -> bool {
    tokens.windows(expected.len()).any(|window| {
        window
            .iter()
            .zip(expected)
            .all(|(actual, wanted)| actual == wanted)
    })
}

fn is_force_git(tokens: &[String]) -> bool {
    (has_sequence(tokens, &["git", "push"])
        && (contains_token(tokens, "--force") || contains_token(tokens, "-f")))
        || has_sequence(tokens, &["git", "reset", "--hard"])
        || has_sequence(tokens, &["git", "clean", "-f"])
}

fn is_destructive_filesystem(tokens: &[String]) -> bool {
    tokens.windows(2).any(|window| {
        window[0] == "rm"
            && window[1].starts_with('-')
            && window[1].contains('r')
            && window[1].contains('f')
    }) || contains_token(tokens, "truncate")
        || tokens
            .iter()
            .any(|token| token == "mkfs" || token.starts_with("mkfs."))
        || (contains_token(tokens, "dd") && tokens.iter().any(|token| token.starts_with("if=")))
}

fn is_publish_deploy(tokens: &[String]) -> bool {
    [
        &["terraform", "apply"][..],
        &["pulumi", "up"][..],
        &["kubectl", "apply"][..],
        &["npm", "publish"][..],
        &["pnpm", "publish"][..],
        &["yarn", "publish"][..],
        &["cargo", "publish"][..],
        &["gh", "release", "create"][..],
        &["netlify", "deploy"][..],
        &["serverless", "deploy"][..],
    ]
    .iter()
    .any(|sequence| has_sequence(tokens, sequence))
        || (contains_token(tokens, "vercel") && contains_token(tokens, "--prod"))
}

fn looks_like_secret_assignment(token: &str) -> bool {
    let Some((name, _)) = token.split_once('=') else {
        return false;
    };
    looks_like_sensitive_name(name)
}

fn path_within(path: &str, cwd: &str) -> bool {
    let candidate = std::path::Path::new(path);
    let root = std::path::Path::new(cwd);
    let Some(root) = root.canonicalize().ok() else {
        return candidate.starts_with(root);
    };
    let resolved_candidate = candidate.canonicalize().ok().or_else(|| {
        candidate
            .parent()
            .and_then(|parent| parent.canonicalize().ok())
    });
    resolved_candidate
        .as_deref()
        .map(|resolved| resolved.starts_with(&root))
        .unwrap_or_else(|| candidate.starts_with(&root))
}

fn display_cli(cli: &str) -> &str {
    match cli {
        "claude" => "Claude Code",
        "codex" => "Codex CLI",
        "qoderclicn" => "Qoder CLI CN",
        _ => "CLI",
    }
}

/// Remote approval has one unambiguous Qoder identity. Do not silently map
/// local aliases: doing so could grant a capability to an unsupported CLI.
fn cloud_cli_type(cli: &str) -> Option<&str> {
    match cli.trim().to_ascii_lowercase().as_str() {
        "qoderclicn" => Some("qoderclicn"),
        "claude" => Some("claude"),
        "codex" => Some("codex"),
        _ => None,
    }
}

fn risk_title(category: &str) -> &str {
    match category {
        "forceGit" => "强制 Git 操作需要授权",
        "destructiveFilesystem" => "破坏性文件操作需要授权",
        "publishDeploy" => "发布或部署需要授权",
        "projectExternalWrite" => "项目外写入需要授权",
        "credentialSecurity" => "凭证或安全配置需要授权",
        _ => "高风险操作需要授权",
    }
}

pub fn sanitize_preview(input: &str) -> String {
    let mut value = input.to_string();
    for prefix in [
        "github_pat_",
        "ghp_",
        "gho_",
        "ghu_",
        "ghs_",
        "ghr_",
        "glpat-",
        "sk-proj-",
        "sk-",
        "xoxb-",
        "xoxp-",
        "akia",
        "aiza",
    ] {
        value = redact_prefixed_secret(&value, prefix);
    }
    value = redact_url_query_secrets(&value);
    value = redact_json_secret_fields(&value);
    value = redact_env_assignments(&value);
    for marker in [
        "authorization:",
        "authorization=",
        "bearer ",
        "--token=",
        "--token ",
        "--api-key=",
        "--api-key ",
        "token=",
        "token:",
        "access_token=",
        "access_token:",
        "password=",
        "password:",
        "client_secret=",
        "client_secret:",
        "api_key=",
        "api-key:",
        "private_key=",
        "private_key:",
        "secret=",
        "secret:",
    ] {
        value = redact_value_after(&value, marker);
    }
    if value.contains("-----BEGIN") && value.contains("PRIVATE KEY-----") {
        value = "[已隐藏私钥内容]".to_string();
    }
    truncate_chars(&value, PREVIEW_LIMIT)
}

fn redact_prefixed_secret(input: &str, prefix: &str) -> String {
    let mut rest = input;
    let mut output = String::with_capacity(input.len());
    let prefix_lower = prefix.to_ascii_lowercase();
    loop {
        let lower = rest.to_ascii_lowercase();
        let Some(index) = lower.find(&prefix_lower) else {
            output.push_str(rest);
            return output;
        };
        output.push_str(&rest[..index]);
        output.push_str("[已隐藏]");
        let end = rest[index..]
            .find(|character: char| !matches!(character, 'A'..='Z' | 'a'..='z' | '0'..='9' | '_' | '-'))
            .map(|offset| index + offset)
            .unwrap_or(rest.len());
        rest = &rest[end..];
    }
}

fn redact_url_query_secrets(input: &str) -> String {
    let mut value = input.to_string();
    for marker in [
        "token=",
        "access_token=",
        "api_key=",
        "apikey=",
        "secret=",
        "signature=",
    ] {
        value = redact_query_value(&value, marker);
    }
    value
}

fn redact_query_value(input: &str, marker: &str) -> String {
    let mut rest = input;
    let mut output = String::with_capacity(input.len());
    let marker_lower = marker.to_ascii_lowercase();
    loop {
        let lower = rest.to_ascii_lowercase();
        let Some(index) = lower.find(&marker_lower) else {
            output.push_str(rest);
            return output;
        };
        let valid_query_prefix = index > 0 && matches!(rest.as_bytes()[index - 1], b'?' | b'&');
        if !valid_query_prefix {
            let split = index + marker.len();
            output.push_str(&rest[..split]);
            rest = &rest[split..];
            continue;
        }
        let end = index + marker.len();
        output.push_str(&rest[..end]);
        output.push_str("[已隐藏]");
        let value_end = rest[end..]
            .find(|character: char| {
                matches!(character, '&' | '#' | ' ' | '\n' | '\r' | '\t' | '"' | '\'')
            })
            .map(|offset| end + offset)
            .unwrap_or(rest.len());
        rest = &rest[value_end..];
    }
}

fn redact_json_secret_fields(input: &str) -> String {
    let mut value = input.to_string();
    for field in [
        "token",
        "secret",
        "password",
        "api_key",
        "apikey",
        "access_token",
        "accesstoken",
        "client_secret",
        "clientsecret",
        "private_key",
        "privatekey",
    ] {
        value = redact_json_field(&value, field);
    }
    value
}

fn redact_env_assignments(input: &str) -> String {
    let mut rest = input;
    let mut output = String::with_capacity(input.len());
    loop {
        let Some(index) = rest.find('=') else {
            output.push_str(rest);
            return output;
        };
        let key_start = rest[..index]
            .rfind(|character: char| {
                character.is_whitespace() || matches!(character, ';' | '|' | '&')
            })
            .map(|offset| offset + 1)
            .unwrap_or(0);
        let key = rest[key_start..index].trim_matches(|character| matches!(character, '\'' | '"'));
        if !looks_like_sensitive_name(key) {
            let next = index + 1;
            output.push_str(&rest[..next]);
            rest = &rest[next..];
            continue;
        }
        let value_start = index + 1;
        let value_end = rest[value_start..]
            .find(|character: char| {
                character.is_whitespace() || matches!(character, ';' | '|' | '&' | ',' | '"' | '\'')
            })
            .map(|offset| value_start + offset)
            .unwrap_or(rest.len());
        output.push_str(&rest[..value_start]);
        output.push_str("[已隐藏]");
        rest = &rest[value_end..];
    }
}

fn looks_like_sensitive_name(name: &str) -> bool {
    let normalized = name.to_ascii_lowercase();
    [
        "token",
        "secret",
        "password",
        "api_key",
        "apikey",
        "access_key",
        "private_key",
        "credential",
        "auth",
    ]
    .iter()
    .any(|marker| normalized.contains(marker))
}

fn redact_json_field(input: &str, field: &str) -> String {
    let mut rest = input;
    let mut output = String::with_capacity(input.len());
    let needle = format!("\"{}\"", field);
    loop {
        let lower = rest.to_ascii_lowercase();
        let Some(index) = lower.find(&needle) else {
            output.push_str(rest);
            return output;
        };
        let after_field = index + needle.len();
        let Some(colon_offset) = rest[after_field..].find(':') else {
            output.push_str(rest);
            return output;
        };
        let value_start = after_field + colon_offset + 1;
        let whitespace_end = value_start
            + rest[value_start..]
                .chars()
                .take_while(|character| character.is_whitespace())
                .map(char::len_utf8)
                .sum::<usize>();
        if rest[whitespace_end..].starts_with('"') {
            let quoted_start = whitespace_end + 1;
            if let Some(quoted_end_offset) = rest[quoted_start..].find('"') {
                let quoted_end = quoted_start + quoted_end_offset + 1;
                output.push_str(&rest[..whitespace_end]);
                output.push_str("\"[已隐藏]\"");
                rest = &rest[quoted_end..];
                continue;
            }
        }
        output.push_str(&rest[..after_field]);
        rest = &rest[after_field..];
    }
}

fn redact_value_after(input: &str, marker: &str) -> String {
    let mut rest = input;
    let mut output = String::with_capacity(input.len());
    let marker_lower = marker.to_ascii_lowercase();
    loop {
        let lower = rest.to_ascii_lowercase();
        let Some(index) = lower.find(&marker_lower) else {
            output.push_str(rest);
            return output;
        };
        let end = index + marker.len();
        output.push_str(&rest[..end]);
        output.push_str("[已隐藏]");
        let mut value_start = end;
        while rest[value_start..]
            .chars()
            .next()
            .is_some_and(char::is_whitespace)
        {
            value_start += rest[value_start..].chars().next().unwrap().len_utf8();
        }
        if rest[value_start..]
            .get(..6)
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case("bearer"))
        {
            value_start += 6;
            while rest[value_start..]
                .chars()
                .next()
                .is_some_and(char::is_whitespace)
            {
                value_start += rest[value_start..].chars().next().unwrap().len_utf8();
            }
        }
        let value_end = rest[value_start..]
            .find(|character: char| {
                character.is_whitespace() || matches!(character, '&' | ',' | '"' | '\'')
            })
            .map(|offset| value_start + offset)
            .unwrap_or(rest.len());
        rest = &rest[value_end..];
    }
}

fn truncate_chars(value: &str, max: usize) -> String {
    let mut result = value.chars().take(max).collect::<String>();
    if value.chars().count() > max {
        result.push_str("…");
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(event_name: &str, tool: &str, input: serde_json::Value) -> HookApprovalRequest {
        HookApprovalRequest {
            request_key: "req-1".to_string(),
            session_id: "s_1".to_string(),
            cli_type: "qoderclicn".to_string(),
            event_name: event_name.to_string(),
            payload: serde_json::json!({"tool_name": tool, "tool_input": input}),
        }
    }

    #[test]
    fn only_explicit_pretool_rule_is_intercepted() {
        let mut config = ApprovalConfig {
            enabled: true,
            ..ApprovalConfig::default()
        };
        config.qoder_cn_mode = ApprovalMode::PreToolUse;
        let blocked = prepare_request(
            &request(
                "PreToolUse",
                "Bash",
                serde_json::json!({"command": "git push --force origin main"}),
            ),
            &config,
            "/work/app",
            Some(42),
            0,
        );
        let blocked = blocked.unwrap();
        assert_eq!(blocked.event.risk_category.as_deref(), Some("forceGit"));
        assert_eq!(blocked.event.project_id, Some(42));
        let normal = prepare_request(
            &request(
                "PreToolUse",
                "Bash",
                serde_json::json!({"command": "npm test"}),
            ),
            &config,
            "/work/app",
            None,
            0,
        );
        assert!(normal.is_none());
    }

    #[test]
    fn native_permission_does_not_double_intercept_pretool() {
        let config = ApprovalConfig {
            enabled: true,
            ..ApprovalConfig::default()
        };
        let native = HookApprovalRequest {
            cli_type: "claude".to_string(),
            ..request(
                "PermissionRequest",
                "Bash",
                serde_json::json!({"command": "rm -rf tmp"}),
            )
        };
        assert!(prepare_request(&native, &config, "/work/app", None, 0).is_some());
        let pre = HookApprovalRequest {
            cli_type: "claude".to_string(),
            ..request(
                "PreToolUse",
                "Bash",
                serde_json::json!({"command": "rm -rf tmp"}),
            )
        };
        assert!(prepare_request(&pre, &config, "/work/app", None, 0).is_none());
    }

    #[test]
    fn sanitization_hides_credentials() {
        let value = sanitize_preview(
            "export AWS_SECRET_ACCESS_KEY=abc ghp_1234567890 https://example.test?access_token=xyz \\
             --token sk-proj-secret {\"clientSecret\":\"json-secret\"}",
        );
        for secret in [
            "abc",
            "ghp_1234567890",
            "xyz",
            "sk-proj-secret",
            "json-secret",
        ] {
            assert!(!value.contains(secret), "leaked {secret}: {value}");
        }
    }

    #[test]
    fn risks_use_cloud_wire_values_and_command_boundaries() {
        let mut config = ApprovalConfig {
            enabled: true,
            ..ApprovalConfig::default()
        };
        config.qoder_cn_mode = ApprovalMode::PreToolUse;
        let publish = prepare_request(
            &request(
                "PreToolUse",
                "Bash",
                serde_json::json!({"command": "npm publish"}),
            ),
            &config,
            "/project",
            None,
            0,
        )
        .unwrap();
        assert_eq!(
            publish.event.risk_category.as_deref(),
            Some("publishDeploy")
        );
        let credential = prepare_request(
            &request(
                "PreToolUse",
                "Write",
                serde_json::json!({"file_path": ".env", "content": "ignored"}),
            ),
            &config,
            "/project",
            None,
            0,
        )
        .unwrap();
        assert_eq!(
            credential.event.risk_category.as_deref(),
            Some("credentialSecurity")
        );
        assert!(prepare_request(
            &request(
                "PreToolUse",
                "Bash",
                serde_json::json!({"command": "git-push --force"}),
            ),
            &config,
            "/project",
            None,
            0,
        )
        .is_none());
    }

    #[test]
    fn relative_writes_are_resolved_before_project_scope_check() {
        let mut config = ApprovalConfig {
            enabled: true,
            ..ApprovalConfig::default()
        };
        config.qoder_cn_mode = ApprovalMode::PreToolUse;
        let prepared = prepare_request(
            &request(
                "PreToolUse",
                "Write",
                serde_json::json!({"file_path": "../outside/file.txt"}),
            ),
            &config,
            "/workspace/project",
            None,
            0,
        )
        .unwrap();
        assert_eq!(
            prepared.event.risk_category.as_deref(),
            Some("projectExternalWrite")
        );
        assert_eq!(
            prepared.event.target_preview.as_deref(),
            Some("/workspace/outside/file.txt")
        );
    }

    #[test]
    fn config_writes_are_atomic_and_readable_by_hooks() {
        let directory = tempfile::tempdir().unwrap();
        let config = ApprovalConfig {
            enabled: true,
            ..ApprovalConfig::default()
        };
        save_config(directory.path(), &config).unwrap();
        assert!(load_config(directory.path()).enabled);
        let entries = fs::read_dir(directory.path().join("agent"))
            .unwrap()
            .count();
        assert_eq!(entries, 1);
    }

    #[test]
    fn only_qoderclicn_is_supported_for_remote_approval() {
        assert_eq!(cloud_cli_type("qoder"), None);
        assert_eq!(cloud_cli_type("qoder-cn"), None);
        assert_eq!(cloud_cli_type("qoderclicn"), Some("qoderclicn"));
        let config = ApprovalConfig {
            enabled: true,
            ..ApprovalConfig::default()
        };
        let mut alias = request(
            "PreToolUse",
            "Bash",
            serde_json::json!({"command": "rm -rf tmp"}),
        );
        alias.cli_type = "qoder".to_string();
        assert!(prepare_request(&alias, &config, "/project", None, 0).is_none());
    }

    #[test]
    fn canonical_tool_types_match_the_cloud_contract() {
        assert_eq!(canonical_tool_type("Bash"), "shell");
        assert_eq!(canonical_tool_type("Write"), "fileWrite");
        assert_eq!(canonical_tool_type("Apply_Patch"), "fileEdit");
        assert_eq!(canonical_tool_type("Git"), "git");
        assert_eq!(canonical_tool_type("Deploy"), "deploy");
        assert_eq!(canonical_tool_type("unmapped"), "unknown");
    }

    #[tokio::test]
    async fn draining_waiters_preserves_session_for_abandonment() {
        let coordinator = ApprovalCoordinator::new();
        let receiver = coordinator.register("request-1", "s_1").await.unwrap();
        assert_eq!(
            coordinator.drain_pending().await,
            vec![AbandonedApproval {
                request_key: "request-1".to_string(),
                session_id: "s_1".to_string()
            }]
        );
        assert!(receiver.await.is_err());
    }

    #[tokio::test]
    async fn draining_one_session_keeps_other_waiters() {
        let coordinator = ApprovalCoordinator::new();
        let first = coordinator.register("request-1", "s_1").await.unwrap();
        let second = coordinator.register("request-2", "s_2").await.unwrap();
        assert_eq!(coordinator.drain_session("s_1").await.len(), 1);
        assert!(first.await.is_err());
        assert!(
            coordinator
                .decide("request-2", ApprovalDecision::AllowOnce)
                .await
        );
        assert_eq!(second.await.unwrap(), ApprovalDecision::AllowOnce);
    }
}
