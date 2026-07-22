use serde_json::json;
use std::collections::{HashMap, HashSet};
use std::path::{Component, Path, PathBuf};
use tokio::process::Command;
use tokio::time::{timeout, Duration};

const COMMAND_TIMEOUT: Duration = Duration::from_secs(5);
const MAX_FILES: usize = 300;
const MAX_DIFF_BYTES: usize = 256 * 1024;
const MAX_STAT_TEXT_BYTES: usize = 64 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
struct StatusEntry {
    raw_status: String,
    path: String,
}

pub async fn summary(session_id: &str, cwd: &str) -> serde_json::Value {
    let repo_root = match repo_root(cwd).await {
        Ok(root) => root,
        Err(GitPreviewError::NotGitRepo) => return summary_error(session_id, "notGitRepo"),
        Err(_) => return summary_error(session_id, "error"),
    };

    let status = match git_output(
        cwd,
        &[
            "-c",
            "core.quotePath=false",
            "status",
            "--porcelain=v1",
            "-z",
            "--untracked-files=all",
        ],
    )
    .await
    {
        Ok(out) => out,
        Err(_) => return summary_error(session_id, "error"),
    };
    let entries = parse_status_entries(&status);
    let numstat = git_output(
        cwd,
        &[
            "-c",
            "core.quotePath=false",
            "diff",
            "--numstat",
            "HEAD",
            "--",
        ],
    )
    .await
    .unwrap_or_default();
    let stats = parse_numstat(&numstat);

    let mut files = Vec::new();
    for entry in &entries {
        if entry.path.is_empty() {
            continue;
        }
        let (additions, deletions) = stats.get(&entry.path).copied().unwrap_or((None, None));
        files.push(json!({
            "path": entry.path,
            "changeType": change_type(&entry.raw_status),
            "rawStatus": entry.raw_status,
            "additions": additions,
            "deletions": deletions
        }));
        if files.len() >= MAX_FILES {
            break;
        }
    }

    let mut truncated = entries.len() > files.len();
    let stat_text = git_output(
        cwd,
        &["-c", "core.quotePath=false", "diff", "--stat", "HEAD", "--"],
    )
    .await
    .unwrap_or_default();
    let stat_text = stat_text.trim();
    let bounded_stat_text = crate::session::response_limits::truncate_utf8(stat_text, MAX_STAT_TEXT_BYTES);
    truncated |= bounded_stat_text.len() < stat_text.len();
    json!({
        "sessionId": session_id,
        "status": if files.is_empty() { "noChanges" } else { "ok" },
        "cwd": repo_root.to_string_lossy(),
        "files": files,
        "statText": bounded_stat_text,
        "truncated": truncated
    })
}

pub async fn file_diff(session_id: &str, cwd: &str, path: &str) -> serde_json::Value {
    let repo_root = match repo_root(cwd).await {
        Ok(root) => root,
        Err(GitPreviewError::NotGitRepo) => return diff_error(session_id, path, "notGitRepo"),
        Err(_) => return diff_error(session_id, path, "error"),
    };

    if !is_safe_relative_path(path) {
        return diff_error(session_id, path, "pathDenied");
    }

    let status = match git_output(
        cwd,
        &[
            "-c",
            "core.quotePath=false",
            "status",
            "--porcelain=v1",
            "-z",
            "--untracked-files=all",
        ],
    )
    .await
    {
        Ok(out) => out,
        Err(_) => return diff_error(session_id, path, "error"),
    };
    let entries = parse_status_entries(&status);
    let status_paths = parse_status_paths(&entries);
    if !status_paths.contains(path) {
        return diff_error(session_id, path, "pathDenied");
    }

    let full_path = repo_root.join(path);
    if !stays_under(&repo_root, &full_path) {
        return diff_error(session_id, path, "pathDenied");
    }

    let is_untracked = entries
        .iter()
        .any(|entry| entry.raw_status == "??" && entry.path == path);
    if is_untracked {
        return untracked_diff(session_id, path, &full_path).await;
    }

    let diff = git_output(
        cwd,
        &["-c", "core.quotePath=false", "diff", "HEAD", "--", path],
    )
    .await
    .unwrap_or_default();

    if diff.len() > MAX_DIFF_BYTES {
        return diff_error(session_id, path, "tooLarge");
    }
    if is_binary_diff(&diff) {
        return diff_error(session_id, path, "binary");
    }

    json!({
        "sessionId": session_id,
        "path": path,
        "status": "ok",
        "diffText": diff
    })
}

async fn untracked_diff(session_id: &str, path: &str, full_path: &Path) -> serde_json::Value {
    let metadata = match tokio::fs::symlink_metadata(full_path).await {
        Ok(metadata) => metadata,
        Err(_) => return diff_error(session_id, path, "pathDenied"),
    };
    if metadata.file_type().is_symlink() || metadata.is_dir() {
        return diff_error(session_id, path, "pathDenied");
    }
    if metadata.len() as usize > MAX_DIFF_BYTES {
        return diff_error(session_id, path, "tooLarge");
    }
    let bytes = match tokio::fs::read(full_path).await {
        Ok(bytes) => bytes,
        Err(_) => return diff_error(session_id, path, "error"),
    };
    if bytes.contains(&0) {
        return diff_error(session_id, path, "binary");
    }
    let content = match String::from_utf8(bytes) {
        Ok(content) => content,
        Err(_) => return diff_error(session_id, path, "binary"),
    };
    let line_count = content.lines().count();
    let mut diff = format!(
        "diff --git a/{0} b/{0}\nnew file mode 100644\n--- /dev/null\n+++ b/{0}\n@@ -0,0 +1,{1} @@\n",
        path, line_count
    );
    for line in content.lines() {
        diff.push('+');
        diff.push_str(line);
        diff.push('\n');
    }
    json!({
        "sessionId": session_id,
        "path": path,
        "status": "ok",
        "diffText": diff
    })
}

async fn repo_root(cwd: &str) -> Result<PathBuf, GitPreviewError> {
    match git_output(cwd, &["rev-parse", "--show-toplevel"]).await {
        Ok(out) => Ok(PathBuf::from(out.trim())),
        Err(GitPreviewError::CommandFailed(_)) => Err(GitPreviewError::NotGitRepo),
        Err(err) => Err(err),
    }
}

async fn git_output(cwd: &str, args: &[&str]) -> Result<String, GitPreviewError> {
    let output = timeout(
        COMMAND_TIMEOUT,
        Command::new("git").arg("-C").arg(cwd).args(args).output(),
    )
    .await
    .map_err(|_| GitPreviewError::Timeout)?
    .map_err(|_| GitPreviewError::Io)?;
    if !output.status.success() {
        return Err(GitPreviewError::CommandFailed(
            String::from_utf8_lossy(&output.stderr).into_owned(),
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn parse_numstat(text: &str) -> HashMap<String, (Option<i32>, Option<i32>)> {
    let mut stats = HashMap::new();
    for line in text.lines() {
        let mut parts = line.splitn(3, '\t');
        let additions = parts.next().and_then(|s| s.parse::<i32>().ok());
        let deletions = parts.next().and_then(|s| s.parse::<i32>().ok());
        if let Some(path) = parts.next() {
            stats.insert(display_path(path), (additions, deletions));
        }
    }
    stats
}

fn parse_status_entries(status: &str) -> Vec<StatusEntry> {
    let mut entries = Vec::new();
    let mut parts = status
        .split('\0')
        .filter(|part| !part.is_empty())
        .peekable();
    while let Some(part) = parts.next() {
        if part.len() < 4 {
            continue;
        }
        let raw_status = part[0..2].to_string();
        let path = part[3..].to_string();
        if raw_status.contains('R') || raw_status.contains('C') {
            let _old_path = parts.next();
        }
        entries.push(StatusEntry { raw_status, path });
    }
    entries
}

fn parse_status_paths(entries: &[StatusEntry]) -> HashSet<String> {
    entries.iter().map(|entry| entry.path.clone()).collect()
}

fn display_path(raw: &str) -> String {
    raw.rsplit_once(" -> ")
        .map(|(_, new_path)| new_path)
        .unwrap_or(raw)
        .trim_matches('"')
        .to_string()
}

fn is_binary_diff(diff: &str) -> bool {
    diff.as_bytes().contains(&0)
        || diff.contains("Binary files ")
        || diff.contains("GIT binary patch")
}

fn change_type(raw_status: &str) -> &'static str {
    if raw_status == "??" {
        return "untracked";
    }
    if raw_status.contains('R') {
        return "renamed";
    }
    if raw_status.contains('C') {
        return "copied";
    }
    if raw_status.contains('A') {
        return "added";
    }
    if raw_status.contains('D') {
        return "deleted";
    }
    if raw_status.contains('T') {
        return "typeChanged";
    }
    if raw_status.contains('M') {
        return "modified";
    }
    "unknown"
}

fn is_safe_relative_path(path: &str) -> bool {
    if path.is_empty() || Path::new(path).is_absolute() {
        return false;
    }
    Path::new(path)
        .components()
        .all(|component| matches!(component, Component::Normal(_)))
}

fn stays_under(root: &Path, candidate: &Path) -> bool {
    let mut depth = 0i32;
    for component in candidate
        .strip_prefix(root)
        .unwrap_or(candidate)
        .components()
    {
        match component {
            Component::Normal(_) => depth += 1,
            Component::CurDir => {}
            Component::ParentDir => depth -= 1,
            _ => return false,
        }
        if depth < 0 {
            return false;
        }
    }
    true
}

fn summary_error(session_id: &str, status: &str) -> serde_json::Value {
    json!({
        "sessionId": session_id,
        "status": status,
        "files": []
    })
}

fn diff_error(session_id: &str, path: &str, status: &str) -> serde_json::Value {
    json!({
        "sessionId": session_id,
        "path": path,
        "status": status,
        "diffText": ""
    })
}

#[derive(Debug)]
enum GitPreviewError {
    NotGitRepo,
    Timeout,
    Io,
    CommandFailed(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::process::Command as StdCommand;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn rejects_unsafe_paths() {
        assert!(is_safe_relative_path("Sources/App.swift"));
        assert!(!is_safe_relative_path("../secret"));
        assert!(!is_safe_relative_path("/tmp/secret"));
        assert!(!is_safe_relative_path(""));
    }

    #[test]
    fn parses_untracked_status_path() {
        let entries = parse_status_entries("?? Sources/NewFile.swift\0 M Sources/App.swift\0");
        let paths = parse_status_paths(&entries);
        assert!(paths.contains("Sources/NewFile.swift"));
        assert!(paths.contains("Sources/App.swift"));
    }

    #[test]
    fn parses_nul_status_paths_with_spaces_unicode_and_renames() {
        let entries = parse_status_entries(
            " M Sources/My File.swift\0R  Sources/新名字.swift\0Sources/Old Name.swift\0?? 你好.swift\0",
        );
        let paths = parse_status_paths(&entries);
        assert!(paths.contains("Sources/My File.swift"));
        assert!(paths.contains("Sources/新名字.swift"));
        assert!(paths.contains("你好.swift"));
        assert!(!paths.contains("Sources/Old Name.swift"));
    }

    #[test]
    fn detects_binary_diff_marker_after_header() {
        let diff = "diff --git a/image.png b/image.png\nindex 111..222\nBinary files a/image.png and b/image.png differ\n";
        assert!(is_binary_diff(diff));
    }

    #[test]
    fn stat_text_limit_preserves_utf8_boundary() {
        let source = format!("{}中文", "a".repeat(MAX_STAT_TEXT_BYTES));
        let bounded = crate::session::response_limits::truncate_utf8(&source, MAX_STAT_TEXT_BYTES);
        assert_eq!(bounded.len(), MAX_STAT_TEXT_BYTES);
        assert!(bounded.is_char_boundary(bounded.len()));
    }

    #[tokio::test]
    async fn file_diff_includes_staged_and_unstaged_changes() {
        let repo = unique_temp_repo();
        fs::create_dir_all(&repo).unwrap();
        run_git(&repo, &["init"]);
        run_git(&repo, &["config", "user.email", "test@example.com"]);
        run_git(&repo, &["config", "user.name", "Test"]);

        let file = repo.join("Sources").join("App.swift");
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(&file, "base\n").unwrap();
        run_git(&repo, &["add", "."]);
        run_git(&repo, &["commit", "-m", "initial"]);

        fs::write(&file, "base\nstaged\n").unwrap();
        run_git(&repo, &["add", "."]);
        fs::write(&file, "base\nstaged\nunstaged\n").unwrap();

        let result = file_diff("s_test", repo.to_str().unwrap(), "Sources/App.swift").await;
        assert_eq!(result["status"], "ok");
        let diff = result["diffText"].as_str().unwrap();
        assert!(diff.contains("+staged"));
        assert!(diff.contains("+unstaged"));

        fs::remove_dir_all(&repo).ok();
    }

    fn unique_temp_repo() -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("kn-agent-git-preview-{nanos}"))
    }

    fn run_git(repo: &Path, args: &[&str]) {
        let output = StdCommand::new("git")
            .arg("-C")
            .arg(repo)
            .args(args)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "git {:?} failed: {}",
            args,
            String::from_utf8_lossy(&output.stderr)
        );
    }
}
