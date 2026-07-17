//! 项目列表的轻量本地 Git 摘要。
//!
//! 此模块刻意只运行一次 `git status --porcelain=v2 --branch`，不访问网络、
//! 不读取远端配置，也不读取 diff 或提交历史。

use serde_json::{json, Value};
use tokio::process::Command;
use tokio::time::{timeout, Duration};

const GIT_STATUS_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GitListState {
    Changed,
    Clean,
    NotGitRepo,
    Unavailable,
}

impl GitListState {
    fn as_str(self) -> &'static str {
        match self {
            Self::Changed => "changed",
            Self::Clean => "clean",
            Self::NotGitRepo => "notGitRepo",
            Self::Unavailable => "unavailable",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitListStatus {
    pub state: GitListState,
    pub branch: Option<String>,
    pub has_upstream: bool,
    pub ahead: i64,
    pub behind: i64,
}

impl GitListStatus {
    fn unavailable(state: GitListState) -> Self {
        Self {
            state,
            branch: None,
            has_upstream: false,
            ahead: 0,
            behind: 0,
        }
    }

    fn as_json(&self) -> Value {
        json!({
            "state": self.state.as_str(),
            "branch": self.branch,
            "hasUpstream": self.has_upstream,
            "ahead": self.ahead,
            "behind": self.behind,
        })
    }
}

/// 解析 `git status --porcelain=v2 --branch -z` 输出。
/// 只有 porcelain 文件记录代表工作区存在变更；任意 `#` 开头记录均为 header。
pub fn parse_porcelain_v2(output: &str) -> GitListStatus {
    let mut branch = None;
    let mut has_upstream = false;
    let mut ahead = 0;
    let mut behind = 0;
    let mut changed = false;

    for record in output.split('\0').filter(|record| !record.is_empty()) {
        if let Some(value) = record.strip_prefix("# branch.head ") {
            if value != "(detached)" && value != "(unknown)" {
                branch = Some(value.to_string());
            }
        } else if record.strip_prefix("# branch.upstream ").is_some() {
            has_upstream = true;
        } else if let Some(value) = record.strip_prefix("# branch.ab ") {
            for count in value.split_whitespace() {
                if let Some(number) = count.strip_prefix('+') {
                    ahead = number.parse().unwrap_or(0);
                } else if let Some(number) = count.strip_prefix('-') {
                    behind = number.parse().unwrap_or(0);
                }
            }
        } else if !record.starts_with("# ") {
            changed |= matches!(
                record.as_bytes().first(),
                Some(b'1' | b'2' | b'u' | b'?' | b'!')
            ) && record.as_bytes().get(1) == Some(&b' ');
        }
    }

    GitListStatus {
        state: if changed {
            GitListState::Changed
        } else {
            GitListState::Clean
        },
        branch,
        has_upstream,
        ahead,
        behind,
    }
}

pub async fn read(project_key: &str, cwd: &str, last_verification: Option<Value>) -> Value {
    let git = match timeout(
        GIT_STATUS_TIMEOUT,
        Command::new("git")
            .arg("-C")
            .arg(cwd)
            .args([
                "status",
                "--porcelain=v2",
                "--branch",
                "-z",
                "--untracked-files=normal",
            ])
            .output(),
    )
    .await
    {
        Ok(Ok(output)) if output.status.success() => {
            let output = String::from_utf8_lossy(&output.stdout);
            parse_porcelain_v2(&output)
        }
        Ok(Ok(output)) if is_not_git_repo(&output.stderr) => {
            GitListStatus::unavailable(GitListState::NotGitRepo)
        }
        Ok(Ok(_)) | Ok(Err(_)) | Err(_) => GitListStatus::unavailable(GitListState::Unavailable),
    };

    json!({
        "projectKey": project_key,
        "status": "ok",
        "git": git.as_json(),
        "lastVerification": last_verification,
    })
}

fn is_not_git_repo(stderr: &[u8]) -> bool {
    String::from_utf8_lossy(stderr)
        .to_ascii_lowercase()
        .contains("not a git repository")
}
