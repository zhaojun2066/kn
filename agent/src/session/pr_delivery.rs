use serde_json::{json, Value};
use std::path::Path;
use tokio::process::Command;
use tokio::time::{timeout, Duration};

const COMMAND_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_TITLE_BYTES: usize = 256;
const MAX_BODY_BYTES: usize = 64 * 1024;
const MAX_BRANCHES: usize = 100;

pub async fn status(project_key: &str, cwd: &str) -> Value {
    let git = crate::session::git_delivery::status(project_key, cwd).await;
    if git["status"].as_str() != Some("ok") { return git; }
    let Some(branch) = git["branch"].as_str().filter(|value| !value.is_empty()) else { return error(project_key, "detachedHead"); };
    let Some(remote) = git["remote"].as_str().filter(|value| !value.is_empty()) else { return error(project_key, "noRemote"); };
    if !is_github_remote(cwd, remote).await { return error(project_key, "notGitHubRemote"); }
    if !gh_available(cwd).await { return error(project_key, "ghUnavailable"); }
    if !gh_authenticated(cwd).await { return error(project_key, "ghNotAuthenticated"); }
    let bases = known_bases(cwd, remote).await;
    let suggested_base = suggested_base(cwd, branch, remote, &bases).await;
    let Some(suggested_base) = suggested_base else { return error(project_key, "noBaseBranch"); };
    let is_pushed = git["upstream"].as_str().is_some_and(|upstream| upstream == format!("{remote}/{branch}"))
        && head_matches_upstream(cwd).await;
    let existing = if is_pushed { existing_pr(cwd, branch).await } else { None };
    json!({
        "projectKey": project_key,
        "status": "ok",
        "branch": branch,
        "remote": remote,
        "suggestedBase": suggested_base,
        "baseBranches": bases,
        "isPushed": is_pushed,
        "existingPullRequest": existing,
        "canCreate": is_pushed && branch != suggested_base && existing.is_none()
    })
}

pub async fn create(project_key: &str, cwd: &str, base: &str, title: &str, body: &str) -> Value {
    if !valid_branch(base) || title.trim().is_empty() || title.len() > MAX_TITLE_BYTES || body.len() > MAX_BODY_BYTES {
        return error(project_key, "invalidRequest");
    }
    let current = status(project_key, cwd).await;
    if current["status"].as_str() != Some("ok") { return current; }
    let Some(branch) = current["branch"].as_str() else { return error(project_key, "detachedHead"); };
    if branch == base || !current["baseBranches"].as_array().is_some_and(|items| items.iter().any(|item| item.as_str() == Some(base))) {
        return error(project_key, "noBaseBranch");
    }
    if !current["isPushed"].as_bool().unwrap_or(false) { return error(project_key, "branchNotPushed"); }
    if current["existingPullRequest"].is_object() { return error(project_key, "prAlreadyExists"); }

    let body_file = std::env::temp_dir().join(format!("kn-pr-{}.md", uuid::Uuid::new_v4()));
    if tokio::fs::write(&body_file, body).await.is_err() { return error(project_key, "prCreateFailed"); }
    let output = gh_output(cwd, &["pr", "create", "--base", base, "--title", title.trim(), "--body-file", &body_file.to_string_lossy()]).await;
    let _ = tokio::fs::remove_file(&body_file).await;
    match output {
        Ok(url) => json!({
            "projectKey": project_key,
            "status": "ok",
            "url": url.trim(),
            "number": pr_number(url.trim()),
            "base": base,
            "head": branch
        }),
        Err(_) => error(project_key, "prCreateFailed"),
    }
}

async fn is_github_remote(cwd: &str, remote: &str) -> bool {
    git_output(cwd, &["remote", "get-url", remote]).await.is_ok_and(|url| {
        let value = url.trim().to_ascii_lowercase();
        value.contains("github.com:") || value.contains("github.com/")
    })
}

async fn gh_available(cwd: &str) -> bool { gh_output(cwd, &["--version"]).await.is_ok() }
async fn gh_authenticated(cwd: &str) -> bool { gh_output(cwd, &["auth", "status", "--hostname", "github.com"]).await.is_ok() }

async fn known_bases(cwd: &str, remote: &str) -> Vec<String> {
    git_output(cwd, &["for-each-ref", "--format=%(refname:strip=3)", &format!("refs/remotes/{remote}")]).await
        .unwrap_or_default()
        .lines()
        .map(str::trim)
        .filter(|branch| !branch.is_empty() && *branch != "HEAD" && valid_branch(branch))
        .take(MAX_BRANCHES)
        .map(str::to_string)
        .collect()
}

async fn suggested_base(cwd: &str, branch: &str, remote: &str, bases: &[String]) -> Option<String> {
    let configured = git_output(cwd, &["config", "--get", &format!("branch.{branch}.gh-merge-base")]).await.ok().map(|value| value.trim().to_string());
    if let Some(base) = configured.filter(|base| bases.contains(base)) { return Some(base); }
    let default = git_output(cwd, &["symbolic-ref", "--quiet", "--short", &format!("refs/remotes/{remote}/HEAD")]).await.ok()?;
    let base = default.trim().strip_prefix(&format!("{remote}/"))?.to_string();
    bases.contains(&base).then_some(base)
}

async fn existing_pr(cwd: &str, branch: &str) -> Option<Value> {
    let output = gh_output(cwd, &["pr", "list", "--head", branch, "--state", "open", "--json", "number,url,baseRefName"]).await.ok()?;
    let mut prs: Vec<Value> = serde_json::from_str(&output).ok()?;
    prs.pop().map(|pr| json!({"number": pr["number"], "url": pr["url"], "base": pr["baseRefName"]}))
}

async fn git_output(cwd: &str, args: &[&str]) -> Result<String, ()> { command_output("git", cwd, args).await }
async fn gh_output(cwd: &str, args: &[&str]) -> Result<String, ()> { command_output("gh", cwd, args).await }
async fn command_output(program: &str, cwd: &str, args: &[&str]) -> Result<String, ()> {
    let mut command = Command::new(program);
    command.current_dir(Path::new(cwd)).args(args).kill_on_drop(true);
    let output = timeout(COMMAND_TIMEOUT, command.output()).await.map_err(|_| ())?.map_err(|_| ())?;
    output.status.success().then(|| String::from_utf8_lossy(&output.stdout).into_owned()).ok_or(())
}

async fn head_matches_upstream(cwd: &str) -> bool {
    let head = git_output(cwd, &["rev-parse", "HEAD"]).await.ok();
    let upstream = git_output(cwd, &["rev-parse", "@{upstream}"]).await.ok();
    matches!((head, upstream), (Some(head), Some(upstream)) if head.trim() == upstream.trim())
}

fn valid_branch(branch: &str) -> bool {
    !branch.is_empty() && branch.len() <= 255 && !branch.starts_with('-') && !branch.contains("..") && !branch.contains([' ', '~', '^', ':', '?', '*', '[', '\\'])
}
fn pr_number(url: &str) -> Option<u64> { url.rsplit('/').next()?.parse().ok() }
fn error(project_key: &str, status: &str) -> Value { json!({"projectKey": project_key, "status": status}) }

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn branch_validation_rejects_git_revision_syntax() {
        assert!(valid_branch("release/1.2"));
        assert!(!valid_branch("main..other"));
        assert!(!valid_branch("--upload-pack=evil"));
        assert!(!valid_branch("feature name"));
    }

    #[test]
    fn pr_number_uses_the_last_url_component() {
        assert_eq!(pr_number("https://github.com/owner/repo/pull/42"), Some(42));
    }
}
