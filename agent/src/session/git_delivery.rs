use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::path::{Component, Path, PathBuf};
use tokio::process::Command;
use tokio::time::{timeout, Duration};

const COMMAND_TIMEOUT: Duration = Duration::from_secs(30);
const BRANCH_FETCH_TIMEOUT: Duration = Duration::from_secs(8);
const PUSH_TIMEOUT: Duration = Duration::from_secs(20 * 60);
const MAX_MESSAGE_BYTES: usize = 4 * 1024;
const MAX_SELECTED_FILES: usize = 100;
const MAX_OUTPUT_BYTES: usize = 8 * 1024;
pub const GIT_STATUS_PAGE_SIZE: usize = 100;

#[derive(Debug, Clone)]
struct StatusEntry {
    raw_status: String,
    path: String,
}

pub async fn status(project_key: &str, cwd: &str) -> Value {
    status_page(project_key, cwd, 0, GIT_STATUS_PAGE_SIZE as i64, None).await
}

/// Lists local and remote branches. Fetch failures are deliberately non-fatal so an
/// offline computer can still switch among the branches it already knows about.
pub async fn branches(project_key: &str, cwd: &str) -> Value {
    let current = status(project_key, cwd).await;
    if current["status"].as_str() != Some("ok") {
        return current;
    }
    let fetch_warning = if default_remote(cwd).await.is_some() {
        fetch_branches(cwd)
            .await
            .err()
            .map(|_| "无法同步远端分支，正在显示本地缓存".to_string())
    } else {
        None
    };
    let refs = match git_output(
        cwd,
        &[
            "for-each-ref",
            "--format=%(refname:short)|%(refname)",
            "refs/heads",
            "refs/remotes",
        ],
    )
    .await
    {
        Ok(refs) => refs,
        Err(_) => return branch_error(project_key, "error", "无法读取分支列表"),
    };
    let current_branch = current["branch"].as_str().unwrap_or_default();
    let branches: Vec<Value> = refs.lines().filter_map(|line| {
        let (name, reference) = line.split_once('|')?;
        if name.ends_with("/HEAD") || name.is_empty() { return None; }
        let is_remote = reference.starts_with("refs/remotes/");
        let display_name = if is_remote { name.split_once('/').map(|(_, value)| value).unwrap_or(name) } else { name };
        Some(json!({"name": display_name, "ref": name, "kind": if is_remote { "remote" } else { "local" }, "isCurrent": !is_remote && name == current_branch}))
    }).collect();
    json!({"projectKey": project_key, "status": "ok", "currentBranch": current_branch, "branches": branches, "fetchWarning": fetch_warning})
}

pub async fn checkout_branch(project_key: &str, cwd: &str, reference: &str) -> Value {
    if !is_safe_branch_ref(reference) {
        return branch_error(project_key, "invalidBranch", "分支名无效");
    }
    let before = status(project_key, cwd).await;
    if before["status"].as_str() != Some("ok") {
        return before;
    }
    if before["files"]
        .as_array()
        .is_some_and(|files| !files.is_empty())
    {
        return branch_error(
            project_key,
            "workingTreeDirty",
            "存在未提交变更，无法切换分支",
        );
    }
    let local_exists = git_output(
        cwd,
        &[
            "show-ref",
            "--verify",
            "--quiet",
            &format!("refs/heads/{reference}"),
        ],
    )
    .await
    .is_ok();
    let args = if local_exists {
        vec!["switch".to_string(), reference.to_string()]
    } else if reference.contains('/')
        && git_output(
            cwd,
            &[
                "show-ref",
                "--verify",
                "--quiet",
                &format!("refs/remotes/{reference}"),
            ],
        )
        .await
        .is_ok()
    {
        vec![
            "switch".to_string(),
            "--track".to_string(),
            reference.to_string(),
        ]
    } else {
        return branch_error(project_key, "branchNotFound", "分支不存在");
    };
    match git_output_owned(cwd, &args).await {
        Ok(_) => branch_success(project_key, cwd).await,
        Err(error) => branch_error(project_key, branch_error_code(error), "切换分支失败"),
    }
}

pub async fn create_and_checkout_branch(
    project_key: &str,
    cwd: &str,
    name: &str,
    base: &str,
) -> Value {
    if !is_safe_branch_ref(name) || !is_safe_branch_ref(base) {
        return branch_error(project_key, "invalidBranch", "分支名无效");
    }
    let before = status(project_key, cwd).await;
    if before["status"].as_str() != Some("ok") {
        return before;
    }
    if before["files"]
        .as_array()
        .is_some_and(|files| !files.is_empty())
    {
        return branch_error(
            project_key,
            "workingTreeDirty",
            "存在未提交变更，无法创建分支",
        );
    }
    if git_output(cwd, &["check-ref-format", "--branch", name])
        .await
        .is_err()
    {
        return branch_error(project_key, "invalidBranch", "分支名无效");
    }
    if git_output(
        cwd,
        &[
            "show-ref",
            "--verify",
            "--quiet",
            &format!("refs/heads/{name}"),
        ],
    )
    .await
    .is_ok()
    {
        return branch_error(project_key, "branchExists", "同名本地分支已存在");
    }
    if git_output(
        cwd,
        &["rev-parse", "--verify", &format!("{base}^{{commit}}")],
    )
    .await
    .is_err()
    {
        return branch_error(project_key, "branchNotFound", "基线分支不存在");
    }
    match git_output_owned(
        cwd,
        &[
            "switch".to_string(),
            "-c".to_string(),
            name.to_string(),
            base.to_string(),
        ],
    )
    .await
    {
        Ok(_) => branch_success(project_key, cwd).await,
        Err(error) => branch_error(project_key, branch_error_code(error), "创建分支失败"),
    }
}

async fn branch_success(project_key: &str, cwd: &str) -> Value {
    let git_status = status(project_key, cwd).await;
    json!({"projectKey": project_key, "status": "ok", "branch": git_status["branch"], "gitStatus": git_status})
}

fn branch_error(project_key: &str, status: &str, message: &str) -> Value {
    json!({"projectKey": project_key, "status": status, "message": message})
}

fn is_safe_branch_ref(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 255
        && !value.starts_with('-')
        && !value.contains('\0')
        && !value.contains("..")
}

fn branch_error_code(error: GitError) -> &'static str {
    match error {
        GitError::Timeout => "operationTimeout",
        GitError::Failed(message) if message.contains("would be overwritten") => "workingTreeDirty",
        _ => "error",
    }
}

pub async fn status_page(
    project_key: &str,
    cwd: &str,
    offset: i64,
    limit: i64,
    requested_snapshot_id: Option<&str>,
) -> Value {
    let branch = match git_output(cwd, &["rev-parse", "--abbrev-ref", "HEAD"]).await {
        Ok(branch) if branch.trim() != "HEAD" => branch.trim().to_string(),
        Ok(_) => {
            return status_error(
                project_key,
                "detachedHead",
                "当前仓库处于 detached HEAD 状态",
            )
        }
        // A freshly initialized repository has a symbolic HEAD but no commit yet.
        // `rev-parse --abbrev-ref HEAD` fails there, while `symbolic-ref` still
        // provides the initial branch name (for example, main).
        Err(GitError::Failed(_)) => {
            match git_output(cwd, &["symbolic-ref", "--quiet", "--short", "HEAD"]).await {
                Ok(branch) if !branch.trim().is_empty() => branch.trim().to_string(),
                _ => return status_error(project_key, "notGitRepo", "当前目录不是 Git 仓库"),
            }
        }
        Err(_) => return status_error(project_key, "error", "无法读取 Git 状态"),
    };
    let raw_status = match raw_status(cwd).await {
        Ok(value) => value,
        Err(_) => return status_error(project_key, "error", "无法读取 Git 状态"),
    };
    let snapshot_id = snapshot_id(&raw_status);
    if requested_snapshot_id.is_some_and(|expected| expected != snapshot_id) {
        return workspace_changed(project_key);
    }
    let (upstream, ahead, behind, entries) = parse_status(&raw_status);
    let upstream_remote = git_output(
        cwd,
        &["config", "--get", &format!("branch.{branch}.remote")],
    )
    .await
    .ok()
    .map(|value| value.trim().to_string())
    .filter(|value| is_safe_remote(value));
    let remote = match upstream_remote {
        Some(remote) => Some(remote),
        None => default_remote(cwd).await,
    };
    let default_branch = match remote.as_deref() {
        Some(remote) => git_output(
            cwd,
            &[
                "symbolic-ref",
                "--quiet",
                "--short",
                &format!("refs/remotes/{remote}/HEAD"),
            ],
        )
        .await
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty()),
        None => None,
    };
    let head = git_output(cwd, &["rev-parse", "--short", "HEAD"])
        .await
        .ok()
        .map(|value| value.trim().to_string());
    let latest_commit = git_output(cwd, &["log", "-1", "--pretty=%s"])
        .await
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    let has_index_changes = entries.iter().any(|entry| is_staged(&entry.raw_status));
    let total_files = entries.len();
    let offset = offset.max(0) as usize;
    let limit = limit.clamp(1, GIT_STATUS_PAGE_SIZE as i64) as usize;
    let page_end = offset.saturating_add(limit).min(total_files);
    let files: Vec<Value> = entries
        .iter()
        .skip(offset)
        .take(limit)
        .map(|entry| {
            json!({
                "path": &entry.path,
                "rawStatus": &entry.raw_status,
                "changeType": change_type(&entry.raw_status),
                "isStaged": is_staged(&entry.raw_status)
            })
        })
        .collect();
    let has_more = page_end < total_files;
    json!({
        "projectKey": project_key,
        "status": "ok",
        "branch": branch,
        "upstream": upstream,
        "remote": remote,
        "defaultBranch": default_branch,
        "ahead": ahead,
        "behind": behind,
        "head": head,
        "latestCommit": latest_commit,
        "hasIndexChanges": has_index_changes,
        "files": files,
        "totalFiles": total_files,
        "offset": offset,
        "nextOffset": page_end,
        "hasMore": has_more,
        "truncated": has_more,
        "snapshotId": snapshot_id
    })
}

pub async fn commit(project_key: &str, cwd: &str, message: &str, paths: &[String]) -> Value {
    if let Err(message) = validate_commit_request(message, paths) {
        return commit_error(project_key, &message);
    }
    let before = status(project_key, cwd).await;
    if before["status"].as_str() != Some("ok") {
        return before;
    }
    if before["hasIndexChanges"].as_bool() == Some(true) {
        return commit_error(project_key, "indexHasChanges");
    }
    let entries = match raw_status(cwd).await {
        Ok(raw) => parse_status(&raw).3,
        Err(_) => return commit_error(project_key, "error"),
    };
    let available: HashSet<String> = entries.iter().map(|entry| entry.path.clone()).collect();
    if paths.iter().any(|path| !available.contains(path)) {
        return commit_error(project_key, "pathDenied");
    }
    let untracked: Vec<String> = entries
        .iter()
        .filter(|entry| entry.raw_status == "??")
        .map(|entry| entry.path.clone())
        .filter(|path| paths.contains(path))
        .collect();
    if !untracked.is_empty() {
        let args = [vec!["add".to_string(), "--".to_string()], untracked.clone()].concat();
        if git_output_owned(cwd, &args).await.is_err() {
            return commit_error(project_key, "error");
        }
    }
    let message_file = std::env::temp_dir().join(format!("kn-git-{}.txt", uuid::Uuid::new_v4()));
    if tokio::fs::write(&message_file, message.trim())
        .await
        .is_err()
    {
        if !untracked.is_empty() {
            let restore = [
                vec![
                    "restore".to_string(),
                    "--staged".to_string(),
                    "--".to_string(),
                ],
                untracked,
            ]
            .concat();
            let _ = git_output_owned(cwd, &restore).await;
        }
        return commit_error(project_key, "error");
    }
    let mut args = vec!["commit".to_string(), "--only".to_string(), "-F".to_string()];
    args.push(message_file.to_string_lossy().into_owned());
    args.push("--".to_string());
    args.extend(paths.iter().cloned());
    let commit_result = git_output_owned(cwd, &args).await;
    let _ = tokio::fs::remove_file(&message_file).await;
    if let Err(error) = commit_result {
        if !untracked.is_empty() {
            let restore = [
                vec![
                    "restore".to_string(),
                    "--staged".to_string(),
                    "--".to_string(),
                ],
                untracked,
            ]
            .concat();
            let _ = git_output_owned(cwd, &restore).await;
        }
        return commit_error(project_key, commit_error_code(error));
    }
    let head = git_output(cwd, &["rev-parse", "--short", "HEAD"])
        .await
        .ok()
        .map(|value| value.trim().to_string());
    json!({
        "projectKey": project_key,
        "status": "ok",
        "commit": head,
        "message": message.trim(),
        "commitScope": "selected",
        "committedFileCount": paths.len(),
        "gitStatus": status(project_key, cwd).await
    })
}

/// Commits every Git-visible working-tree change through an isolated temporary index.
/// The repository's real staging area remains untouched throughout this operation.
pub async fn commit_all_working_tree(project_key: &str, cwd: &str, message: &str) -> Value {
    if message.trim().is_empty() || message.len() > MAX_MESSAGE_BYTES {
        return commit_error(project_key, "invalidMessage");
    }
    let index_lock = match acquire_real_index_lock(cwd).await {
        Ok(lock) => lock,
        Err(IndexLockError::Busy) => return commit_error(project_key, "indexHasChanges"),
        Err(IndexLockError::Git(error)) => {
            return commit_error(project_key, commit_error_code(error))
        }
        Err(IndexLockError::Io) => return commit_error(project_key, "error"),
    };
    let raw = match raw_status(cwd).await {
        Ok(raw) => raw,
        Err(_) => return commit_error(project_key, "error"),
    };
    let entries = parse_status(&raw).3;
    if entries.iter().any(|entry| is_staged(&entry.raw_status)) {
        return commit_error(project_key, "indexHasChanges");
    }
    if entries.is_empty() {
        return commit_error(project_key, "nothingToCommit");
    }

    let temp_dir = match create_private_temp_dir().await {
        Ok(dir) => dir,
        Err(_) => return commit_error(project_key, "error"),
    };
    let index_path = temp_dir.path().join("index");
    let message_file = temp_dir.path().join("message.txt");
    let result = async {
        tokio::fs::write(&message_file, message.trim())
            .await
            .map_err(|_| GitError::Io)?;
        let env = [("GIT_INDEX_FILE", index_path.to_string_lossy().into_owned())];
        git_output_with_env(cwd, &["read-tree".to_string(), "HEAD".to_string()], &env).await?;
        git_output_with_env(cwd, &["add".to_string(), "-A".to_string()], &env).await?;
        git_output_with_env(
            cwd,
            &[
                "commit".to_string(),
                "-F".to_string(),
                message_file.to_string_lossy().into_owned(),
            ],
            &env,
        )
        .await
    }
    .await;
    drop(index_lock);
    drop(temp_dir);
    if let Err(error) = result {
        return commit_error(project_key, commit_error_code(error));
    }
    let head = git_output(cwd, &["rev-parse", "--short", "HEAD"])
        .await
        .ok()
        .map(|value| value.trim().to_string());
    json!({
        "projectKey": project_key,
        "status": "ok",
        "commit": head,
        "message": message.trim(),
        "commitScope": "allWorkingTree",
        "committedFileCount": entries.len(),
        "gitStatus": status(project_key, cwd).await
    })
}

pub async fn push(project_key: &str, cwd: &str, report_progress: impl Fn(&str)) -> Value {
    report_progress("正在检查推送条件");
    let current = status(project_key, cwd).await;
    if current["status"].as_str() != Some("ok") {
        return current;
    }
    let Some(branch) = current["branch"].as_str() else {
        return push_error(project_key, "detachedHead");
    };
    let Some(remote) = current["remote"].as_str() else {
        return push_error(project_key, "noRemote");
    };
    let has_upstream = current["upstream"]
        .as_str()
        .is_some_and(|value| !value.is_empty());
    let args = if has_upstream {
        vec!["push".to_string(), remote.to_string(), branch.to_string()]
    } else {
        vec![
            "push".to_string(),
            "-u".to_string(),
            remote.to_string(),
            branch.to_string(),
        ]
    };
    report_progress("正在推送提交");
    match git_output_owned_with_timeout(cwd, &args, PUSH_TIMEOUT).await {
        Ok(_) => json!({
            "projectKey": project_key,
            "status": "ok",
            "branch": branch,
            "remote": remote,
            "gitStatus": status(project_key, cwd).await
        }),
        Err(GitError::Failed(message)) => push_error(project_key, push_error_code(&message)),
        Err(GitError::Timeout) => push_error(project_key, "operationTimeout"),
        Err(_) => push_error(project_key, "error"),
    }
}

fn validate_commit_request(message: &str, paths: &[String]) -> Result<(), String> {
    if message.trim().is_empty() || message.len() > MAX_MESSAGE_BYTES {
        return Err("invalidMessage".to_string());
    }
    if paths.is_empty()
        || paths.len() > MAX_SELECTED_FILES
        || paths.iter().any(|path| !is_safe_relative_path(path))
    {
        return Err("pathDenied".to_string());
    }
    Ok(())
}

async fn git_output(cwd: &str, args: &[&str]) -> Result<String, GitError> {
    git_output_owned(
        cwd,
        &args
            .iter()
            .map(|value| (*value).to_string())
            .collect::<Vec<_>>(),
    )
    .await
}

/// Refreshing remote refs is optional for the branch picker. It must never
/// wait for interactive credentials or hold the mobile request hostage.
async fn fetch_branches(cwd: &str) -> Result<String, GitError> {
    let mut command = Command::new("git");
    command
        .arg("-C")
        .arg(cwd)
        .args(["fetch", "--prune"])
        .env("GIT_TERMINAL_PROMPT", "0")
        .kill_on_drop(true);
    let output = timeout(BRANCH_FETCH_TIMEOUT, command.output())
        .await
        .map_err(|_| GitError::Timeout)?
        .map_err(|_| GitError::Io)?;
    if !output.status.success() {
        return Err(GitError::Failed(
            String::from_utf8_lossy(&output.stderr)
                .chars()
                .take(MAX_OUTPUT_BYTES)
                .collect(),
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

async fn raw_status(cwd: &str) -> Result<String, GitError> {
    git_output_with_env(
        cwd,
        &[
            "-c".to_string(),
            "core.quotePath=false".to_string(),
            "status".to_string(),
            "--porcelain=v1".to_string(),
            "-z".to_string(),
            "--branch".to_string(),
            "--untracked-files=all".to_string(),
        ],
        &[("GIT_OPTIONAL_LOCKS", "0".to_string())],
    )
    .await
}

async fn git_output_with_env(
    cwd: &str,
    args: &[String],
    env: &[(&'static str, String)],
) -> Result<String, GitError> {
    let mut command = Command::new("git");
    command.arg("-C").arg(cwd).args(args).kill_on_drop(true);
    for (key, value) in env {
        command.env(*key, value);
    }
    let output = timeout(COMMAND_TIMEOUT, command.output())
        .await
        .map_err(|_| GitError::Timeout)?
        .map_err(|_| GitError::Io)?;
    if !output.status.success() {
        return Err(GitError::Failed(
            String::from_utf8_lossy(&output.stderr)
                .chars()
                .take(MAX_OUTPUT_BYTES)
                .collect(),
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn snapshot_id(raw_status: &str) -> String {
    hex::encode(Sha256::digest(raw_status.as_bytes()))
}

async fn git_output_owned(cwd: &str, args: &[String]) -> Result<String, GitError> {
    git_output_owned_with_timeout(cwd, args, COMMAND_TIMEOUT).await
}

async fn git_output_owned_with_timeout(
    cwd: &str,
    args: &[String],
    command_timeout: Duration,
) -> Result<String, GitError> {
    let mut command = Command::new("git");
    command.arg("-C").arg(cwd).args(args).kill_on_drop(true);
    let output = timeout(command_timeout, command.output())
        .await
        .map_err(|_| GitError::Timeout)?
        .map_err(|_| GitError::Io)?;
    if !output.status.success() {
        return Err(GitError::Failed(
            String::from_utf8_lossy(&output.stderr)
                .chars()
                .take(MAX_OUTPUT_BYTES)
                .collect(),
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn parse_status(raw: &str) -> (Option<String>, i32, i32, Vec<StatusEntry>) {
    let mut parts = raw.split('\0').filter(|part| !part.is_empty());
    let branch_line = parts.next().unwrap_or_default();
    let (upstream, ahead, behind) = parse_branch(branch_line);
    let mut entries = Vec::new();
    while let Some(part) = parts.next() {
        if part.len() < 4 {
            continue;
        }
        let raw_status = part[0..2].to_string();
        let path = part[3..].to_string();
        if raw_status.contains('R') || raw_status.contains('C') {
            let _ = parts.next();
        }
        entries.push(StatusEntry { raw_status, path });
    }
    (upstream, ahead, behind, entries)
}

fn parse_branch(line: &str) -> (Option<String>, i32, i32) {
    let body = line.strip_prefix("## ").unwrap_or_default();
    let upstream = body
        .split("...")
        .nth(1)
        .and_then(|value| value.split_whitespace().next())
        .map(str::to_string);
    let ahead = body
        .split("ahead ")
        .nth(1)
        .and_then(|value| value.split(|c: char| !c.is_ascii_digit()).next())
        .and_then(|value| value.parse().ok())
        .unwrap_or(0);
    let behind = body
        .split("behind ")
        .nth(1)
        .and_then(|value| value.split(|c: char| !c.is_ascii_digit()).next())
        .and_then(|value| value.parse().ok())
        .unwrap_or(0);
    (upstream, ahead, behind)
}

async fn default_remote(cwd: &str) -> Option<String> {
    let remotes: Vec<String> = git_output(cwd, &["remote"])
        .await
        .ok()?
        .lines()
        .map(str::trim)
        .filter(|remote| !remote.is_empty())
        .filter(|remote| is_safe_remote(remote))
        .map(str::to_string)
        .collect();
    if remotes.iter().any(|remote| remote == "origin") {
        Some("origin".to_string())
    } else if remotes.len() == 1 {
        remotes.into_iter().next()
    } else {
        None
    }
}

fn is_staged(raw_status: &str) -> bool {
    raw_status
        .as_bytes()
        .first()
        .is_some_and(|value| *value != b' ' && *value != b'?')
}

fn change_type(raw_status: &str) -> &'static str {
    if raw_status == "??" {
        return "untracked";
    }
    if raw_status.contains('R') {
        return "renamed";
    }
    if raw_status.contains('A') {
        return "added";
    }
    if raw_status.contains('D') {
        return "deleted";
    }
    if raw_status.contains('M') {
        return "modified";
    }
    "unknown"
}

fn is_safe_relative_path(path: &str) -> bool {
    !path.is_empty()
        && !Path::new(path).is_absolute()
        && Path::new(path)
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
}
fn is_safe_remote(remote: &str) -> bool {
    !remote.is_empty() && !remote.starts_with('-') && !remote.chars().any(char::is_whitespace)
}

fn status_error(project_key: &str, status: &str, message: &str) -> Value {
    json!({
        "projectKey": project_key,
        "status": status,
        "files": [],
        "totalFiles": 0,
        "offset": 0,
        "nextOffset": 0,
        "hasMore": false,
        "truncated": false,
        "snapshotId": null,
        "message": message
    })
}
fn workspace_changed(project_key: &str) -> Value {
    json!({
        "projectKey": project_key,
        "status": "workspaceChanged",
        "files": [],
        "totalFiles": 0,
        "offset": 0,
        "nextOffset": 0,
        "hasMore": false,
        "truncated": false,
        "snapshotId": null,
        "message": "电脑上的变更已更新，请重新加载"
    })
}
fn commit_error(project_key: &str, status: &str) -> Value {
    json!({"projectKey": project_key, "status": status})
}
fn push_error(project_key: &str, status: &str) -> Value {
    json!({"projectKey": project_key, "status": status})
}
fn push_error_code(message: &str) -> &'static str {
    let value = message.to_ascii_lowercase();
    if value.contains("non-fast-forward") || value.contains("fetch first") {
        "nonFastForward"
    } else if value.contains("authentication") || value.contains("permission denied") {
        "authRequired"
    } else if value.contains("protected branch") || value.contains("remote rejected") {
        "remoteRejected"
    } else if value.contains("failed to connect")
        || value.contains("could not resolve host")
        || value.contains("network is unreachable")
        || value.contains("connection timed out")
        || value.contains("connection refused")
    {
        "networkUnavailable"
    } else {
        "error"
    }
}
fn commit_error_code(error: GitError) -> &'static str {
    match error {
        GitError::Timeout => "operationTimeout",
        GitError::Failed(message) if message.to_ascii_lowercase().contains("nothing to commit") => {
            "nothingToCommit"
        }
        _ => "commitFailed",
    }
}

#[derive(Debug)]
enum GitError {
    Timeout,
    Io,
    Failed(String),
}

#[derive(Debug)]
enum IndexLockError {
    Busy,
    Io,
    Git(GitError),
}

struct RealIndexLock {
    path: PathBuf,
    marker_path: PathBuf,
    _file: std::fs::File,
}

impl Drop for RealIndexLock {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
        let _ = std::fs::remove_file(&self.marker_path);
    }
}

struct PrivateTempDir {
    path: PathBuf,
}

impl PrivateTempDir {
    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for PrivateTempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

async fn acquire_real_index_lock(cwd: &str) -> Result<RealIndexLock, IndexLockError> {
    let index_path = git_output(cwd, &["rev-parse", "--git-path", "index"])
        .await
        .map_err(IndexLockError::Git)?;
    let index_path = PathBuf::from(index_path.trim());
    let index_path = if index_path.is_absolute() {
        index_path
    } else {
        Path::new(cwd).join(index_path)
    };
    let file_name = index_path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or(IndexLockError::Io)?;
    let lock_path = index_path.with_file_name(format!("{file_name}.lock"));
    let marker_path = index_path.with_file_name(format!("{file_name}.lock.kn-agent"));
    recover_stale_lock_files(&lock_path, &marker_path);

    // The sidecar is written first so a crash before the Git lock is created
    // leaves no ambiguous lock behind. A plain index.lock without this sidecar
    // is never removed automatically because it may belong to Git itself.
    let marker = match std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&marker_path)
    {
        Ok(mut file) => {
            use std::io::Write;
            let pid = std::process::id();
            if writeln!(file, "pid={pid}").is_err() {
                let _ = std::fs::remove_file(&marker_path);
                return Err(IndexLockError::Io);
            }
            file
        }
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            return Err(IndexLockError::Busy)
        }
        Err(_) => return Err(IndexLockError::Io),
    };
    drop(marker);
    let file = match std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&lock_path)
    {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            let _ = std::fs::remove_file(&marker_path);
            return Err(IndexLockError::Busy);
        }
        Err(_) => {
            let _ = std::fs::remove_file(&marker_path);
            return Err(IndexLockError::Io);
        }
    };
    Ok(RealIndexLock {
        path: lock_path,
        marker_path,
        _file: file,
    })
}

/// Startup recovery entry point. It is deliberately conservative: only an
/// index lock accompanied by a kn-agent marker for a dead PID is removed.
pub async fn recover_stale_agent_lock(cwd: &str) {
    let Ok(index_path) = git_output(cwd, &["rev-parse", "--git-path", "index"]).await else {
        return;
    };
    let index_path = PathBuf::from(index_path.trim());
    let index_path = if index_path.is_absolute() {
        index_path
    } else {
        Path::new(cwd).join(index_path)
    };
    let Some(file_name) = index_path.file_name().and_then(|name| name.to_str()) else {
        return;
    };
    let lock_path = index_path.with_file_name(format!("{file_name}.lock"));
    let marker_path = index_path.with_file_name(format!("{file_name}.lock.kn-agent"));
    recover_stale_lock_files(&lock_path, &marker_path);
}

/// Removes only a lock that carries our sidecar marker and whose owning
/// process is no longer alive. A normal Git `index.lock` is intentionally left
/// untouched for safety.
fn recover_stale_lock_files(lock_path: &Path, marker_path: &Path) {
    if !lock_path.exists() {
        let _ = std::fs::remove_file(marker_path);
        return;
    }
    let Ok(marker) = std::fs::read_to_string(marker_path) else {
        return;
    };
    let Some(pid) = marker
        .strip_prefix("pid=")
        .and_then(|value| value.trim().parse::<libc::pid_t>().ok())
    else {
        return;
    };
    if pid <= 0 || process_is_alive(pid) {
        return;
    }
    let _ = std::fs::remove_file(lock_path);
    let _ = std::fs::remove_file(marker_path);
}

#[cfg(unix)]
fn process_is_alive(pid: libc::pid_t) -> bool {
    // kill(pid, 0) performs existence/permission checking without signalling.
    // EPERM still means the process exists, so only ESRCH is considered dead.
    let result = unsafe { libc::kill(pid, 0) };
    result == 0 || std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
}

#[cfg(not(unix))]
fn process_is_alive(_pid: libc::pid_t) -> bool {
    true
}

async fn create_private_temp_dir() -> Result<PrivateTempDir, GitError> {
    let path = std::env::temp_dir().join(format!("kn-git-index-{}", uuid::Uuid::new_v4()));
    let mut builder = std::fs::DirBuilder::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(&path).map_err(|_| GitError::Io)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&path)
            .map_err(|_| GitError::Io)?
            .permissions()
            .mode()
            & 0o777;
        if mode != 0o700 {
            let _ = std::fs::remove_dir_all(&path);
            return Err(GitError::Io);
        }
    }
    Ok(PrivateTempDir { path })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command as StdCommand;

    #[test]
    fn push_classifies_proxy_and_dns_failures_as_network_unavailable() {
        assert_eq!(
            push_error_code("Failed to connect to localhost port 7890 after 0 ms"),
            "networkUnavailable"
        );
        assert_eq!(
            push_error_code("Could not resolve host: github.com"),
            "networkUnavailable"
        );
    }
    use tempfile::TempDir;

    fn run_git(cwd: &std::path::Path, args: &[&str]) {
        let output = StdCommand::new("git")
            .arg("-C")
            .arg(cwd)
            .args(args)
            .output()
            .expect("git should start");
        assert!(
            output.status.success(),
            "git {:?} failed: {}",
            args,
            String::from_utf8_lossy(&output.stderr)
        );
    }

    fn initialized_repo() -> TempDir {
        let repo = tempfile::tempdir().expect("temporary repo");
        run_git(repo.path(), &["init"]);
        run_git(repo.path(), &["config", "user.email", "test@example.com"]);
        run_git(repo.path(), &["config", "user.name", "kn test"]);
        std::fs::write(repo.path().join("README.md"), "initial\n").expect("write initial file");
        run_git(repo.path(), &["add", "README.md"]);
        run_git(repo.path(), &["commit", "-m", "initial"]);
        repo
    }

    #[tokio::test]
    async fn status_recognizes_initialized_repository_without_a_first_commit() {
        let repo = tempfile::tempdir().expect("temporary repo");
        run_git(repo.path(), &["init"]);
        let cwd = repo.path().to_str().expect("utf8 repo");

        let result = status("device:/repo", cwd).await;

        assert_eq!(result["status"], "ok");
        assert!(result["branch"]
            .as_str()
            .is_some_and(|branch| !branch.is_empty()));
        assert!(result["head"].is_null());

        let branch_result = branches("device:/repo", cwd).await;
        assert_eq!(branch_result["status"], "ok");
        assert!(branch_result["branches"]
            .as_array()
            .is_some_and(Vec::is_empty));
        assert!(branch_result["fetchWarning"].is_null());
    }

    #[tokio::test]
    async fn branches_lists_current_local_branch() {
        let repo = initialized_repo();
        let result = branches("device:/repo", repo.path().to_str().expect("utf8 repo")).await;
        assert_eq!(result["status"], "ok");
        assert!(result["branches"]
            .as_array()
            .is_some_and(|branches| branches.iter().any(|branch| branch["isCurrent"] == true)));
    }

    #[tokio::test]
    async fn checkout_rejects_dirty_working_tree() {
        let repo = initialized_repo();
        std::fs::write(repo.path().join("README.md"), "dirty\n").expect("write working change");
        let cwd = repo.path().to_str().expect("utf8 repo");
        let current = git_output(cwd, &["rev-parse", "--abbrev-ref", "HEAD"])
            .await
            .unwrap();
        let result = checkout_branch("device:/repo", cwd, current.trim()).await;
        assert_eq!(result["status"], "workingTreeDirty");
    }

    #[tokio::test]
    async fn create_branch_creates_and_checks_out_from_current_branch() {
        let repo = initialized_repo();
        let cwd = repo.path().to_str().expect("utf8 repo");
        let current = git_output(cwd, &["rev-parse", "--abbrev-ref", "HEAD"])
            .await
            .unwrap();
        let result =
            create_and_checkout_branch("device:/repo", cwd, "feat/remote", current.trim()).await;
        assert_eq!(result["status"], "ok");
        assert_eq!(result["branch"], "feat/remote");
        assert_eq!(
            git_output(cwd, &["rev-parse", "--abbrev-ref", "HEAD"])
                .await
                .unwrap()
                .trim(),
            "feat/remote"
        );
    }

    #[test]
    fn commit_request_rejects_empty_message_and_unsafe_paths() {
        assert!(validate_commit_request("", &["Sources/App.swift".to_string()]).is_err());
        assert!(
            validate_commit_request("feat: add delivery", &["../secrets".to_string()]).is_err()
        );
    }

    #[test]
    fn commit_request_accepts_a_bounded_relative_file_selection() {
        assert!(validate_commit_request(
            "feat: add delivery",
            &["Sources/App.swift".to_string(), "README.md".to_string()]
        )
        .is_ok());
    }

    #[tokio::test]
    async fn first_push_uses_origin_and_establishes_upstream() {
        let repo = initialized_repo();
        let remote = tempfile::tempdir().expect("bare remote");
        run_git(remote.path(), &["init", "--bare"]);
        run_git(
            repo.path(),
            &[
                "remote",
                "add",
                "origin",
                remote.path().to_str().expect("utf8 remote"),
            ],
        );

        let result = push(
            "device:/repo",
            repo.path().to_str().expect("utf8 repo"),
            |_| {},
        )
        .await;

        assert_eq!(result["status"], "ok");
        assert_eq!(result["remote"], "origin");
        assert!(result["gitStatus"]["upstream"]
            .as_str()
            .is_some_and(|upstream| upstream.starts_with("origin/")));
    }

    #[tokio::test]
    async fn commit_only_includes_selected_file_and_leaves_other_change_uncommitted() {
        let repo = initialized_repo();
        std::fs::write(repo.path().join("selected.txt"), "selected\n").expect("write selected");
        std::fs::write(repo.path().join("unselected.txt"), "unselected\n")
            .expect("write unselected");
        let result = commit(
            "device:/repo",
            repo.path().to_str().expect("utf8 repo"),
            "commit selected",
            &["selected.txt".to_string()],
        )
        .await;

        assert_eq!(result["status"], "ok");
        let names = StdCommand::new("git")
            .arg("-C")
            .arg(repo.path())
            .args(["show", "--format=", "--name-only", "HEAD"])
            .output()
            .expect("read commit names");
        assert_eq!(
            String::from_utf8_lossy(&names.stdout).trim(),
            "selected.txt"
        );
        let current = status("device:/repo", repo.path().to_str().expect("utf8 repo")).await;
        assert!(current["files"]
            .as_array()
            .is_some_and(|files| files.iter().any(|file| file["path"] == "unselected.txt")));
    }

    #[tokio::test]
    async fn commit_rejects_preexisting_staged_content_without_modifying_index() {
        let repo = initialized_repo();
        std::fs::write(repo.path().join("README.md"), "staged\n").expect("write staged file");
        run_git(repo.path(), &["add", "README.md"]);
        std::fs::write(repo.path().join("selected.txt"), "selected\n").expect("write selected");

        let result = commit(
            "device:/repo",
            repo.path().to_str().expect("utf8 repo"),
            "should not commit",
            &["selected.txt".to_string()],
        )
        .await;

        assert_eq!(result["status"], "indexHasChanges");
        let cached = StdCommand::new("git")
            .arg("-C")
            .arg(repo.path())
            .args(["diff", "--cached", "--name-only"])
            .output()
            .expect("read index");
        assert_eq!(String::from_utf8_lossy(&cached.stdout).trim(), "README.md");
    }

    #[tokio::test]
    async fn commit_can_include_a_deleted_file_without_including_other_changes() {
        let repo = initialized_repo();
        std::fs::remove_file(repo.path().join("README.md")).expect("delete tracked file");
        std::fs::write(repo.path().join("unselected.txt"), "keep\n").expect("write unselected");

        let result = commit(
            "device:/repo",
            repo.path().to_str().expect("utf8 repo"),
            "remove readme",
            &["README.md".to_string()],
        )
        .await;

        assert_eq!(result["status"], "ok");
        let names = StdCommand::new("git")
            .arg("-C")
            .arg(repo.path())
            .args(["show", "--format=", "--name-status", "HEAD"])
            .output()
            .expect("read commit names");
        assert_eq!(
            String::from_utf8_lossy(&names.stdout).trim(),
            "D\tREADME.md"
        );
        assert!(repo.path().join("unselected.txt").exists());
    }

    #[tokio::test]
    async fn status_reports_no_remote_and_push_rejects_it() {
        let repo = initialized_repo();
        let current = status("device:/repo", repo.path().to_str().expect("utf8 repo")).await;
        assert!(current["remote"].is_null());

        let result = push(
            "device:/repo",
            repo.path().to_str().expect("utf8 repo"),
            |_| {},
        )
        .await;
        assert_eq!(result["status"], "noRemote");
    }

    #[tokio::test]
    async fn status_reports_the_new_path_for_a_renamed_file() {
        let repo = initialized_repo();
        run_git(repo.path(), &["mv", "README.md", "RENAMED.md"]);

        let current = status("device:/repo", repo.path().to_str().expect("utf8 repo")).await;
        assert!(current["files"].as_array().is_some_and(|files| files
            .iter()
            .any(|file| { file["path"] == "RENAMED.md" && file["changeType"] == "renamed" })));
    }

    #[tokio::test]
    async fn status_page_returns_only_requested_page_and_snapshot() {
        let repo = initialized_repo();
        for index in 0..105 {
            std::fs::write(repo.path().join(format!("file-{index}.txt")), "new\n")
                .expect("write untracked file");
        }

        let first = status_page(
            "device:/repo",
            repo.path().to_str().expect("utf8 repo"),
            0,
            100,
            None,
        )
        .await;
        assert_eq!(first["status"], "ok");
        assert_eq!(first["totalFiles"], 105);
        assert_eq!(first["files"].as_array().map(Vec::len), Some(100));
        assert_eq!(first["offset"], 0);
        assert_eq!(first["nextOffset"], 100);
        assert_eq!(first["hasMore"], true);
        let snapshot = first["snapshotId"].as_str().expect("snapshot");

        let second = status_page(
            "device:/repo",
            repo.path().to_str().expect("utf8 repo"),
            100,
            100,
            Some(snapshot),
        )
        .await;
        assert_eq!(second["status"], "ok");
        assert_eq!(second["files"].as_array().map(Vec::len), Some(5));
        assert_eq!(second["offset"], 100);
        assert_eq!(second["hasMore"], false);
    }

    #[tokio::test]
    async fn status_page_rejects_mixed_workspace_snapshot() {
        let repo = initialized_repo();
        std::fs::write(repo.path().join("first.txt"), "first\n").expect("write file");
        let first = status_page(
            "device:/repo",
            repo.path().to_str().expect("utf8 repo"),
            0,
            100,
            None,
        )
        .await;
        let snapshot = first["snapshotId"].as_str().expect("snapshot").to_string();
        std::fs::write(repo.path().join("second.txt"), "second\n").expect("write file");

        let result = status_page(
            "device:/repo",
            repo.path().to_str().expect("utf8 repo"),
            100,
            100,
            Some(&snapshot),
        )
        .await;
        assert_eq!(result["status"], "workspaceChanged");
        assert_eq!(result["files"].as_array().map(Vec::len), Some(0));
        assert_eq!(result["hasMore"], false);
    }

    #[tokio::test]
    async fn all_working_tree_commit_uses_temporary_index_and_includes_untracked() {
        let repo = initialized_repo();
        std::fs::write(repo.path().join("README.md"), "updated\n").expect("write tracked");
        std::fs::write(repo.path().join("new.txt"), "new\n").expect("write untracked");
        std::fs::write(repo.path().join(".gitignore"), "ignored.txt\n").expect("write ignore");
        std::fs::write(repo.path().join("ignored.txt"), "ignored\n").expect("write ignored");
        let before_index = std::fs::read(repo.path().join(".git/index")).expect("read real index");

        let result = commit_all_working_tree(
            "device:/repo",
            repo.path().to_str().expect("utf8 repo"),
            "commit everything",
        )
        .await;

        assert_eq!(result["status"], "ok");
        assert_eq!(result["commitScope"], "allWorkingTree");
        assert_eq!(result["committedFileCount"], 3);
        assert_eq!(
            std::fs::read(repo.path().join(".git/index")).expect("read real index"),
            before_index
        );
        let names = StdCommand::new("git")
            .arg("-C")
            .arg(repo.path())
            .args(["show", "--format=", "--name-only", "HEAD"])
            .output()
            .expect("read commit names");
        let names = String::from_utf8_lossy(&names.stdout);
        assert!(names.contains("README.md"));
        assert!(names.contains("new.txt"));
        assert!(names.contains(".gitignore"));
        assert!(!names.contains("ignored.txt"));
    }

    #[tokio::test]
    async fn all_working_tree_commit_holds_the_real_index_lock_until_it_finishes() {
        let repo = initialized_repo();
        let index_lock = acquire_real_index_lock(repo.path().to_str().expect("utf8 repo"))
            .await
            .expect("acquire real index lock");
        std::fs::write(repo.path().join("README.md"), "staged\n").expect("write file");

        let output = StdCommand::new("git")
            .arg("-C")
            .arg(repo.path())
            .args(["add", "README.md"])
            .output()
            .expect("start git add");

        assert!(
            !output.status.success(),
            "git add must not race the temporary-index commit"
        );
        drop(index_lock);
    }

    #[tokio::test]
    async fn startup_recovery_removes_only_dead_agent_lock_with_marker() {
        let repo = initialized_repo();
        let index = repo.path().join(".git/index");
        let lock = index.with_file_name("index.lock");
        let marker = index.with_file_name("index.lock.kn-agent");
        std::fs::write(&lock, b"").expect("create stale lock");
        std::fs::write(&marker, b"pid=2147483647\n").expect("create agent marker");

        recover_stale_agent_lock(repo.path().to_str().expect("utf8 repo")).await;

        assert!(!lock.exists());
        assert!(!marker.exists());
    }

    #[tokio::test]
    async fn startup_recovery_keeps_unmarked_git_lock() {
        let repo = initialized_repo();
        let lock = repo.path().join(".git/index.lock");
        std::fs::write(&lock, b"").expect("create git lock");

        recover_stale_agent_lock(repo.path().to_str().expect("utf8 repo")).await;

        assert!(lock.exists());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn all_working_tree_temporary_directory_is_owner_only() {
        use std::os::unix::fs::PermissionsExt;

        let temp_dir = create_private_temp_dir()
            .await
            .expect("create private temp directory");
        let permissions = std::fs::metadata(temp_dir.path())
            .expect("temporary directory metadata")
            .permissions()
            .mode()
            & 0o777;

        assert_eq!(permissions, 0o700);
    }

    #[tokio::test]
    async fn push_reports_non_fast_forward_when_remote_advanced() {
        let repo = initialized_repo();
        let remote = tempfile::tempdir().expect("bare remote");
        run_git(remote.path(), &["init", "--bare"]);
        run_git(
            repo.path(),
            &[
                "remote",
                "add",
                "origin",
                remote.path().to_str().expect("utf8 remote"),
            ],
        );
        assert_eq!(
            push(
                "device:/repo",
                repo.path().to_str().expect("utf8 repo"),
                |_| {}
            )
            .await["status"],
            "ok"
        );

        let peer_parent = tempfile::tempdir().expect("peer parent");
        let peer_path = peer_parent.path().join("peer");
        let clone = StdCommand::new("git")
            .args([
                "clone",
                remote.path().to_str().expect("utf8 remote"),
                peer_path.to_str().expect("utf8 peer"),
            ])
            .output()
            .expect("clone peer");
        assert!(
            clone.status.success(),
            "{}",
            String::from_utf8_lossy(&clone.stderr)
        );
        run_git(&peer_path, &["config", "user.email", "peer@example.com"]);
        run_git(&peer_path, &["config", "user.name", "peer"]);
        std::fs::write(peer_path.join("peer.txt"), "peer\n").expect("write peer file");
        run_git(&peer_path, &["add", "peer.txt"]);
        run_git(&peer_path, &["commit", "-m", "peer advance"]);
        run_git(&peer_path, &["push"]);

        std::fs::write(repo.path().join("local.txt"), "local\n").expect("write local file");
        assert_eq!(
            commit(
                "device:/repo",
                repo.path().to_str().expect("utf8 repo"),
                "local advance",
                &["local.txt".to_string()]
            )
            .await["status"],
            "ok"
        );
        let result = push(
            "device:/repo",
            repo.path().to_str().expect("utf8 repo"),
            |_| {},
        )
        .await;
        assert_eq!(result["status"], "nonFastForward");
    }
}
