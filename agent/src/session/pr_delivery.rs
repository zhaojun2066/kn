use serde_json::{json, Value};
use std::path::Path;
use tokio::process::Command;
use tokio::time::{timeout, Duration};
use url::Url;

const COMMAND_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_TITLE_BYTES: usize = 256;
const MAX_BODY_BYTES: usize = 64 * 1024;
const MAX_BRANCHES: usize = 100;
const MAX_PR_DETAILS: usize = 20;
const MAX_REVIEW_BODY_CHARS: usize = 2_000;
const PR_SUMMARY_FIELDS: &str = "number,url,baseRefName,headRefName,title,isDraft,reviewDecision,mergeStateStatus,statusCheckRollup,updatedAt";
const PR_DETAIL_FIELDS: &str = "number,url,baseRefName,headRefName,title,isDraft,reviewDecision,mergeStateStatus,statusCheckRollup,reviews,updatedAt";

pub async fn status(project_key: &str, cwd: &str) -> Value {
    let git = crate::session::git_delivery::status(project_key, cwd).await;
    if git["status"].as_str() != Some("ok") {
        return git;
    }
    let Some(branch) = git["branch"].as_str().filter(|value| !value.is_empty()) else {
        return error(project_key, "detachedHead");
    };
    let Some(remote) = git["remote"].as_str().filter(|value| !value.is_empty()) else {
        return error(project_key, "noRemote");
    };
    if !is_github_remote(cwd, remote).await {
        return error(project_key, "notGitHubRemote");
    }
    if !gh_available(cwd).await {
        return error(project_key, "ghUnavailable");
    }
    if !gh_authenticated(cwd).await {
        return error(project_key, "ghNotAuthenticated");
    }
    let bases = known_bases(cwd, remote).await;
    let suggested_base = suggested_base(cwd, branch, remote, &bases).await;
    let Some(suggested_base) = suggested_base else {
        return error(project_key, "noBaseBranch");
    };
    let is_pushed = git["upstream"]
        .as_str()
        .is_some_and(|upstream| upstream == format!("{remote}/{branch}"))
        && head_matches_upstream(cwd).await;
    let existing = existing_pr(cwd, branch).await;
    json!({
        "projectKey": project_key,
        "status": "ok",
        "branch": branch,
        "remote": remote,
        "suggestedBase": suggested_base,
        "baseBranches": bases,
        "isPushed": is_pushed,
        "existingPullRequest": existing,
        "canCreate": branch != suggested_base && existing.is_none()
    })
}

pub async fn create(project_key: &str, cwd: &str, base: &str, title: &str, body: &str) -> Value {
    if !valid_branch(base)
        || title.trim().is_empty()
        || title.len() > MAX_TITLE_BYTES
        || body.len() > MAX_BODY_BYTES
    {
        return error(project_key, "invalidRequest");
    }
    let current = status(project_key, cwd).await;
    if current["status"].as_str() != Some("ok") {
        return current;
    }
    let Some(branch) = current["branch"].as_str() else {
        return error(project_key, "detachedHead");
    };
    if branch == base
        || !current["baseBranches"]
            .as_array()
            .is_some_and(|items| items.iter().any(|item| item.as_str() == Some(base)))
    {
        return error(project_key, "noBaseBranch");
    }
    if current["existingPullRequest"].is_object() {
        return error(project_key, "prAlreadyExists");
    }

    let pushed_before_create =
        should_push_before_creating(current["isPushed"].as_bool().unwrap_or(false));
    if pushed_before_create {
        let push = crate::session::git_delivery::push(project_key, cwd, |_| {}).await;
        if push["status"].as_str() != Some("ok") {
            return push;
        }
    }

    let body_file = std::env::temp_dir().join(format!("kn-pr-{}.md", uuid::Uuid::new_v4()));
    if tokio::fs::write(&body_file, body).await.is_err() {
        return error(project_key, "prCreateFailed");
    }
    let output = gh_output(
        cwd,
        &[
            "pr",
            "create",
            "--base",
            base,
            "--title",
            title.trim(),
            "--body-file",
            &body_file.to_string_lossy(),
        ],
    )
    .await;
    let _ = tokio::fs::remove_file(&body_file).await;
    match output {
        Ok(url) => json!({
            "projectKey": project_key,
            "status": "ok",
            "url": url.trim(),
            "number": pr_number(url.trim()),
            "base": base,
            "head": branch,
            "pushedBeforeCreate": pushed_before_create
        }),
        Err(_) => error(project_key, "prCreateFailed"),
    }
}

/// Read the open PR for the current branch. This is intentionally separate
/// from `status`: list screens only need a small aggregate, while the detail
/// view is allowed to request bounded check and review data on demand.
pub async fn details(project_key: &str, cwd: &str) -> Value {
    let git = crate::session::git_delivery::status(project_key, cwd).await;
    if git["status"].as_str() != Some("ok") {
        return git;
    }
    let Some(branch) = git["branch"].as_str().filter(|value| !value.is_empty()) else {
        return error(project_key, "detachedHead");
    };
    let Some(remote) = git["remote"].as_str().filter(|value| !value.is_empty()) else {
        return error(project_key, "noRemote");
    };
    if !is_github_remote(cwd, remote).await {
        return error(project_key, "notGitHubRemote");
    }
    if !gh_available(cwd).await {
        return error(project_key, "ghUnavailable");
    }
    if !gh_authenticated(cwd).await {
        return error(project_key, "ghNotAuthenticated");
    }

    match open_pr_details(cwd, branch).await {
        Some(pr) => json!({
            "projectKey": project_key,
            "status": "ok",
            "pullRequest": pull_request_details(&pr)
        }),
        None => error(project_key, "noPullRequest"),
    }
}

async fn is_github_remote(cwd: &str, remote: &str) -> bool {
    git_output(cwd, &["remote", "get-url", remote])
        .await
        .is_ok_and(|url| is_github_remote_url(url.trim()))
}

fn is_github_remote_url(remote_url: &str) -> bool {
    if let Ok(url) = Url::parse(remote_url) {
        if let Some(host) = url.host_str() {
            return host.eq_ignore_ascii_case("github.com");
        }
    }

    // Git also accepts SCP-style remotes such as git@github.com:owner/repo.git.
    // Only the exact host is accepted; a path containing github.com is not enough.
    remote_url.rsplit_once(':').is_some_and(|(host, path)| {
        !path.is_empty()
            && host
                .rsplit('@')
                .next()
                .is_some_and(|value| value.eq_ignore_ascii_case("github.com"))
    })
}

async fn gh_available(cwd: &str) -> bool {
    gh_output(cwd, &["--version"]).await.is_ok()
}
async fn gh_authenticated(cwd: &str) -> bool {
    gh_output(cwd, &["auth", "status", "--hostname", "github.com"])
        .await
        .is_ok()
}

async fn known_bases(cwd: &str, remote: &str) -> Vec<String> {
    git_output(
        cwd,
        &[
            "for-each-ref",
            "--format=%(refname:strip=3)",
            &format!("refs/remotes/{remote}"),
        ],
    )
    .await
    .unwrap_or_default()
    .lines()
    .map(str::trim)
    .filter(|branch| !branch.is_empty() && *branch != "HEAD" && valid_branch(branch))
    .take(MAX_BRANCHES)
    .map(str::to_string)
    .collect()
}

async fn suggested_base(cwd: &str, branch: &str, remote: &str, bases: &[String]) -> Option<String> {
    let configured = git_output(
        cwd,
        &["config", "--get", &format!("branch.{branch}.gh-merge-base")],
    )
    .await
    .ok()
    .map(|value| value.trim().to_string());
    let local_default = git_output(
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
    .and_then(|value| {
        value
            .trim()
            .strip_prefix(&format!("{remote}/"))
            .map(str::to_string)
    });
    let github_default = if local_default.is_none() {
        github_default_base(cwd).await
    } else {
        None
    };
    select_suggested_base(
        configured.as_deref(),
        local_default.as_deref(),
        github_default.as_deref(),
        bases,
    )
}

async fn github_default_base(cwd: &str) -> Option<String> {
    gh_output(
        cwd,
        &[
            "repo",
            "view",
            "--json",
            "defaultBranchRef",
            "--jq",
            ".defaultBranchRef.name",
        ],
    )
    .await
    .ok()
    .map(|value| value.trim().to_string())
    .filter(|branch| valid_branch(branch))
}

fn select_suggested_base(
    configured: Option<&str>,
    local_default: Option<&str>,
    github_default: Option<&str>,
    bases: &[String],
) -> Option<String> {
    [configured, local_default, github_default]
        .into_iter()
        .flatten()
        .find(|base| bases.iter().any(|known| known == base))
        .map(str::to_string)
}

async fn existing_pr(cwd: &str, branch: &str) -> Option<Value> {
    open_pr_summary(cwd, branch).await.map(|pr| {
        json!({
            "number": pr["number"],
            "url": pr["url"],
            "base": pr["baseRefName"],
            "title": pr["title"],
            "isDraft": pr["isDraft"],
            "reviewDecision": pr["reviewDecision"],
            "mergeStateStatus": pr["mergeStateStatus"],
            "checkSummary": check_summary(&pr["statusCheckRollup"])
        })
    })
}

async fn open_pr_summary(cwd: &str, branch: &str) -> Option<Value> {
    open_pr(cwd, branch, PR_SUMMARY_FIELDS).await
}

async fn open_pr_details(cwd: &str, branch: &str) -> Option<Value> {
    open_pr(cwd, branch, PR_DETAIL_FIELDS).await
}

async fn open_pr(cwd: &str, branch: &str, fields: &str) -> Option<Value> {
    let output = gh_output(
        cwd,
        &[
            "pr", "list", "--head", branch, "--state", "open", "--limit", "20", "--json", fields,
        ],
    )
    .await
    .ok()?;
    select_latest_open_pr(serde_json::from_str::<Vec<Value>>(&output).ok()?)
}

/// A branch can have open PRs for different targets. Present the most recently
/// updated one consistently until the product supports choosing among them.
fn select_latest_open_pr(prs: Vec<Value>) -> Option<Value> {
    prs.into_iter().max_by(|left, right| {
        let left_updated = left["updatedAt"].as_str().unwrap_or_default();
        let right_updated = right["updatedAt"].as_str().unwrap_or_default();
        left_updated.cmp(right_updated).then_with(|| {
            left["number"]
                .as_u64()
                .unwrap_or_default()
                .cmp(&right["number"].as_u64().unwrap_or_default())
        })
    })
}

fn pull_request_details(pr: &Value) -> Value {
    json!({
        "number": pr["number"],
        "url": pr["url"],
        "base": pr["baseRefName"],
        "head": pr["headRefName"],
        "title": pr["title"],
        "isDraft": pr["isDraft"],
        "reviewDecision": pr["reviewDecision"],
        "mergeStateStatus": pr["mergeStateStatus"],
        "checkSummary": check_summary(&pr["statusCheckRollup"]),
        "checks": check_details(&pr["statusCheckRollup"]),
        "reviews": review_details(&pr["reviews"])
    })
}

fn check_details(checks: &Value) -> Vec<Value> {
    checks.as_array().cloned().unwrap_or_default().into_iter().filter_map(|check| {
        let name = check["name"].as_str()?.to_string();
        Some(json!({"name": name, "status": check["status"], "conclusion": check["conclusion"], "detailsUrl": check["detailsUrl"]}))
    }).take(MAX_PR_DETAILS).collect()
}

fn review_details(reviews: &Value) -> Vec<Value> {
    reviews
        .as_array()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|review| {
            let author = review["author"]["login"].as_str()?.to_string();
            let state = review["state"].as_str()?.to_string();
            Some(json!({
                "author": author,
                "state": state,
                "body": review["body"].as_str().map(truncate_review_body),
                "submittedAt": review["submittedAt"]
            }))
        })
        .take(MAX_PR_DETAILS)
        .collect()
}

fn truncate_review_body(body: &str) -> String {
    let mut value: String = body.chars().take(MAX_REVIEW_BODY_CHARS).collect();
    if body.chars().count() > MAX_REVIEW_BODY_CHARS {
        value.push('…');
    }
    value
}

fn check_summary(checks: &Value) -> Value {
    let items = checks.as_array().cloned().unwrap_or_default();
    let failed = items
        .iter()
        .filter(|check| {
            matches!(
                check["conclusion"].as_str(),
                Some("FAILURE" | "CANCELLED" | "TIMED_OUT" | "ACTION_REQUIRED")
            )
        })
        .count();
    let pending = items
        .iter()
        .filter(|check| {
            matches!(
                check["status"].as_str(),
                Some("IN_PROGRESS" | "QUEUED" | "PENDING" | "WAITING")
            ) || check["conclusion"].is_null()
        })
        .count();
    let passed = items
        .iter()
        .filter(|check| check["conclusion"].as_str() == Some("SUCCESS"))
        .count();
    let failed_names: Vec<String> = items
        .iter()
        .filter(|check| {
            matches!(
                check["conclusion"].as_str(),
                Some("FAILURE" | "CANCELLED" | "TIMED_OUT" | "ACTION_REQUIRED")
            )
        })
        .filter_map(|check| check["name"].as_str().map(str::to_string))
        .take(3)
        .collect();
    let state = if failed > 0 {
        "failed"
    } else if pending > 0 {
        "running"
    } else if items.is_empty() {
        "none"
    } else {
        "passed"
    };
    json!({"state": state, "total": items.len(), "passed": passed, "failed": failed, "pending": pending, "failedNames": failed_names})
}

async fn git_output(cwd: &str, args: &[&str]) -> Result<String, ()> {
    command_output("git", cwd, args).await
}
async fn gh_output(cwd: &str, args: &[&str]) -> Result<String, ()> {
    command_output("gh", cwd, args).await
}
async fn command_output(program: &str, cwd: &str, args: &[&str]) -> Result<String, ()> {
    let mut command = Command::new(program);
    command
        .current_dir(Path::new(cwd))
        .args(args)
        .kill_on_drop(true);
    let output = timeout(COMMAND_TIMEOUT, command.output())
        .await
        .map_err(|_| ())?
        .map_err(|_| ())?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).into_owned())
        .ok_or(())
}

async fn head_matches_upstream(cwd: &str) -> bool {
    let head = git_output(cwd, &["rev-parse", "HEAD"]).await.ok();
    let upstream = git_output(cwd, &["rev-parse", "@{upstream}"]).await.ok();
    matches!((head, upstream), (Some(head), Some(upstream)) if head.trim() == upstream.trim())
}

fn should_push_before_creating(is_pushed: bool) -> bool {
    !is_pushed
}

fn valid_branch(branch: &str) -> bool {
    !branch.is_empty()
        && branch.len() <= 255
        && !branch.starts_with('-')
        && !branch.contains("..")
        && !branch.contains([' ', '~', '^', ':', '?', '*', '[', '\\'])
}
fn pr_number(url: &str) -> Option<u64> {
    url.rsplit('/').next()?.parse().ok()
}
fn error(project_key: &str, status: &str) -> Value {
    json!({"projectKey": project_key, "status": status})
}

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

    #[test]
    fn suggested_base_falls_back_to_github_default_when_local_head_is_missing() {
        let bases = vec!["codex-action-center-v1".to_string(), "main".to_string()];

        assert_eq!(
            select_suggested_base(None, None, Some("main"), &bases),
            Some("main".to_string())
        );
    }

    #[test]
    fn pr_creation_requires_a_push_only_when_head_is_not_on_its_upstream() {
        assert!(should_push_before_creating(false));
        assert!(!should_push_before_creating(true));
    }

    #[test]
    fn check_summary_marks_failed_checks_before_pending_or_success() {
        let checks = json!([
            {"conclusion": "SUCCESS"},
            {"status": "IN_PROGRESS"},
            {"conclusion": "FAILURE"}
        ]);
        assert_eq!(check_summary(&checks)["state"], "failed");
        assert_eq!(check_summary(&checks)["failed"], 1);
    }

    #[test]
    fn pr_details_payload_limits_items_and_excludes_unrelated_fields() {
        let pr = json!({
            "number": 42,
            "url": "https://github.com/example/kn/pull/42",
            "baseRefName": "main",
            "headRefName": "feature/delivery",
            "title": "Deliver PR details",
            "isDraft": false,
            "reviewDecision": "CHANGES_REQUESTED",
            "mergeStateStatus": "BLOCKED",
            "statusCheckRollup": [{"name": "unit", "status": "COMPLETED", "conclusion": "FAILURE", "detailsUrl": "https://github.com/example/check/1"}],
            "reviews": [{"author": {"login": "reviewer"}, "state": "CHANGES_REQUESTED", "body": "Please add a test", "submittedAt": "2026-07-14T00:00:00Z"}],
            "sensitiveRemoteUrl": "git@github.com:example/kn.git"
        });

        let details = pull_request_details(&pr);

        assert_eq!(details["number"], 42);
        assert_eq!(details["checks"][0]["name"], "unit");
        assert_eq!(details["reviews"][0]["author"], "reviewer");
        assert!(details.get("sensitiveRemoteUrl").is_none());
    }

    #[test]
    fn github_remote_detection_requires_the_exact_github_host() {
        assert!(is_github_remote_url("https://github.com/example/kn.git"));
        assert!(is_github_remote_url("git@github.com:example/kn.git"));
        assert!(is_github_remote_url("github.com:example/kn.git"));
        assert!(is_github_remote_url("ssh://git@github.com/example/kn.git"));
        assert!(!is_github_remote_url(
            "https://evil.example/github.com/example/kn.git"
        ));
        assert!(!is_github_remote_url("git@notgithub.com:example/kn.git"));
    }

    #[test]
    fn pr_detail_reviews_skip_deleted_authors() {
        let reviews = json!([
            {"author": null, "state": "COMMENTED", "body": "orphan"},
            {"author": {"login": "reviewer"}, "state": "APPROVED", "body": "looks good"}
        ]);

        assert_eq!(
            review_details(&reviews),
            vec![json!({
                "author": "reviewer",
                "state": "APPROVED",
                "body": "looks good",
                "submittedAt": null
            })]
        );
    }

    #[test]
    fn latest_updated_pr_is_selected_deterministically() {
        let prs = vec![
            json!({"number": 3, "updatedAt": "2026-07-13T00:00:00Z"}),
            json!({"number": 2, "updatedAt": "2026-07-14T00:00:00Z"}),
        ];

        assert_eq!(select_latest_open_pr(prs).unwrap()["number"], 2);
    }

    #[test]
    fn status_query_does_not_request_review_bodies() {
        assert!(!PR_SUMMARY_FIELDS.split(',').any(|field| field == "reviews"));
        assert!(PR_DETAIL_FIELDS.split(',').any(|field| field == "reviews"));
    }
}
