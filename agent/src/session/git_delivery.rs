use serde_json::{json, Value};
use std::collections::HashSet;
use std::path::{Component, Path};
use tokio::process::Command;
use tokio::time::{timeout, Duration};

const COMMAND_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_MESSAGE_BYTES: usize = 4 * 1024;
const MAX_SELECTED_FILES: usize = 100;
const MAX_OUTPUT_BYTES: usize = 8 * 1024;

#[derive(Debug, Clone)]
struct StatusEntry {
    raw_status: String,
    path: String,
}

pub async fn status(project_key: &str, cwd: &str) -> Value {
    let branch = match git_output(cwd, &["rev-parse", "--abbrev-ref", "HEAD"]).await {
        Ok(branch) if branch.trim() != "HEAD" => branch.trim().to_string(),
        Ok(_) => {
            return status_error(
                project_key,
                "detachedHead",
                "当前仓库处于 detached HEAD 状态",
            )
        }
        Err(GitError::Failed(_)) => {
            return status_error(project_key, "notGitRepo", "当前目录不是 Git 仓库")
        }
        Err(_) => return status_error(project_key, "error", "无法读取 Git 状态"),
    };
    let raw_status = match git_output(
        cwd,
        &[
            "-c",
            "core.quotePath=false",
            "status",
            "--porcelain=v1",
            "-z",
            "--branch",
            "--untracked-files=all",
        ],
    )
    .await
    {
        Ok(value) => value,
        Err(_) => return status_error(project_key, "error", "无法读取 Git 状态"),
    };
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
    let files: Vec<Value> = entries
        .into_iter()
        .map(|entry| {
            json!({
                "path": entry.path,
                "rawStatus": entry.raw_status,
                "changeType": change_type(&entry.raw_status),
                "isStaged": is_staged(&entry.raw_status)
            })
        })
        .collect();
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
        "files": files
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
    let available: HashSet<String> = before["files"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|file| file["path"].as_str().map(str::to_string))
        .collect();
    if paths.iter().any(|path| !available.contains(path)) {
        return commit_error(project_key, "pathDenied");
    }
    let untracked: Vec<String> = before["files"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|file| file["rawStatus"].as_str() == Some("??"))
        .filter_map(|file| file["path"].as_str().map(str::to_string))
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
    match git_output_owned(cwd, &args).await {
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

async fn git_output_owned(cwd: &str, args: &[String]) -> Result<String, GitError> {
    let mut command = Command::new("git");
    command.arg("-C").arg(cwd).args(args).kill_on_drop(true);
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
    json!({"projectKey": project_key, "status": status, "files": [], "message": message})
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command as StdCommand;
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
