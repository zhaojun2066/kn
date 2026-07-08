use kn_common::project::{ProjectInfo, ProjectVerifyCommand, ProjectVerifyConfig};
use serde_json::json;
use std::collections::HashMap;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::io::AsyncReadExt;
use tokio::process::Command;
use tokio::sync::mpsc;
use tokio::time::timeout;
use tokio_util::sync::CancellationToken;

const DEFAULT_BUILD_TIMEOUT_SECS: u64 = 300;
const DEFAULT_TEST_TIMEOUT_SECS: u64 = 600;
const MAX_TIMEOUT_SECS: u64 = 900;
const MAX_OUTPUT_BYTES: usize = 64 * 1024;
const MAX_OUTPUT_LINES: usize = 200;
const PROGRESS_OUTPUT_BYTES: usize = 8 * 1024;
const PROGRESS_INTERVAL: Duration = Duration::from_millis(800);

static RUNNING_SESSIONS: OnceLock<Mutex<HashMap<String, RunningRun>>> = OnceLock::new();
static LOGIN_SHELL_PATH: OnceLock<Option<String>> = OnceLock::new();

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VerifyTarget {
    All,
    Build,
    Test,
}

impl VerifyTarget {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "all" => Some(Self::All),
            "build" => Some(Self::Build),
            "test" => Some(Self::Test),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::Build => "build",
            Self::Test => "test",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StageName {
    Build,
    Test,
}

impl StageName {
    fn as_str(self) -> &'static str {
        match self {
            Self::Build => "build",
            Self::Test => "test",
        }
    }

    fn default_timeout_secs(self) -> u64 {
        match self {
            Self::Build => DEFAULT_BUILD_TIMEOUT_SECS,
            Self::Test => DEFAULT_TEST_TIMEOUT_SECS,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CommandSpec {
    argv: Vec<String>,
    display: String,
    timeout_secs: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct StagePlan {
    commands: Vec<CommandSpec>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct VerifyPlan {
    command_source: &'static str,
    build: Option<StagePlan>,
    test: Option<StagePlan>,
}

#[derive(Clone)]
struct RunningRun {
    run_id: String,
    environment: String,
    target: VerifyTarget,
    command_source: String,
    cancel: CancellationToken,
    started: Instant,
}

struct RunningGuard {
    session_id: String,
    run_id: String,
    cancel: CancellationToken,
}

impl RunningGuard {
    fn try_acquire(
        session_id: &str,
        run_id: &str,
        environment: &str,
        target: VerifyTarget,
    ) -> Option<Self> {
        let set = RUNNING_SESSIONS.get_or_init(|| Mutex::new(HashMap::new()));
        let mut guard = set.lock().unwrap_or_else(|e| e.into_inner());
        if guard.contains_key(session_id) {
            return None;
        }
        let cancel = CancellationToken::new();
        let started = Instant::now();
        guard.insert(
            session_id.to_string(),
            RunningRun {
                run_id: run_id.to_string(),
                environment: environment.to_string(),
                target,
                command_source: "auto".to_string(),
                cancel: cancel.clone(),
                started,
            },
        );
        Some(Self {
            session_id: session_id.to_string(),
            run_id: run_id.to_string(),
            cancel,
        })
    }

    fn update_command_source(&self, command_source: &str) {
        let set = RUNNING_SESSIONS.get_or_init(|| Mutex::new(HashMap::new()));
        let mut guard = set.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(run) = guard.get_mut(&self.session_id) {
            if run.run_id == self.run_id {
                run.command_source = command_source.to_string();
            }
        }
    }
}

#[derive(Clone)]
pub struct ProgressReporter {
    session_id: String,
    run_id: String,
    environment: String,
    target: VerifyTarget,
    command_source: String,
    started: Instant,
    tx: Option<mpsc::UnboundedSender<String>>,
}

impl ProgressReporter {
    pub fn new(
        session_id: &str,
        run_id: &str,
        environment: &str,
        target: VerifyTarget,
        command_source: &str,
        tx: Option<mpsc::UnboundedSender<String>>,
    ) -> Self {
        Self::new_with_started(
            session_id,
            run_id,
            environment,
            target,
            command_source,
            tx,
            Instant::now(),
        )
    }

    pub fn new_with_started(
        session_id: &str,
        run_id: &str,
        environment: &str,
        target: VerifyTarget,
        command_source: &str,
        tx: Option<mpsc::UnboundedSender<String>>,
        started: Instant,
    ) -> Self {
        Self {
            session_id: session_id.to_string(),
            run_id: run_id.to_string(),
            environment: environment.to_string(),
            target,
            command_source: command_source.to_string(),
            started,
            tx,
        }
    }

    pub fn send_cancelling(&self) {
        self.send("cancelling", None, "", "");
    }

    fn send(&self, status: &str, stage: Option<StageName>, command: &str, output_tail: &str) {
        let Some(tx) = self.tx.as_ref() else {
            return;
        };
        let mut data = json!({
            "sessionId": self.session_id,
            "runId": self.run_id,
            "environment": self.environment,
            "target": self.target.as_str(),
            "commandSource": self.command_source,
            "status": status,
            "elapsedMs": self.started.elapsed().as_millis() as u64,
        });
        if let Some(stage) = stage {
            data["stage"] = serde_json::Value::String(stage.as_str().to_string());
        }
        if !command.is_empty() {
            data["command"] = serde_json::Value::String(command.to_string());
        }
        if !output_tail.is_empty() {
            data["outputTail"] = serde_json::Value::String(tail_string(output_tail));
        }
        let msg = crate::proto::WsMessageBuilder::verify_changes_progress(&self.session_id, data);
        let _ = tx.send(msg);
    }
}

pub fn cancel(session_id: &str, run_id: &str) -> Option<(String, VerifyTarget, String, Instant)> {
    let set = RUNNING_SESSIONS.get_or_init(|| Mutex::new(HashMap::new()));
    let guard = set.lock().unwrap_or_else(|e| e.into_inner());
    let Some(run) = guard.get(session_id) else {
        return None;
    };
    if run.run_id != run_id {
        return None;
    }
    run.cancel.cancel();
    Some((
        run.environment.clone(),
        run.target,
        run.command_source.clone(),
        run.started,
    ))
}

impl Drop for RunningGuard {
    fn drop(&mut self) {
        let set = RUNNING_SESSIONS.get_or_init(|| Mutex::new(HashMap::new()));
        let mut guard = set.lock().unwrap_or_else(|e| e.into_inner());
        if guard.get(&self.session_id).map(|run| run.run_id.as_str()) == Some(self.run_id.as_str())
        {
            guard.remove(&self.session_id);
        }
    }
}

pub async fn verify(
    session_id: &str,
    cwd: &str,
    environment: &str,
    target: VerifyTarget,
    tx: Option<mpsc::UnboundedSender<String>>,
) -> serde_json::Value {
    let run_id = new_run_id();
    let Some(running) = RunningGuard::try_acquire(session_id, &run_id, environment, target) else {
        return error_result(
            session_id,
            environment,
            target,
            "alreadyRunning",
            "已有验证在执行",
        );
    };

    let repo_root = match repo_root(cwd).await {
        Ok(root) => root,
        Err(VerifyError::NotGitRepo) => {
            return error_result(
                session_id,
                environment,
                target,
                "notGitRepo",
                "当前目录不是 Git 仓库",
            )
        }
        Err(_) => {
            return error_result(
                session_id,
                environment,
                target,
                "error",
                "无法检查 Git 仓库",
            )
        }
    };

    let started = Instant::now();
    let plan = load_manual_plan(&repo_root, environment).unwrap_or_else(|| auto_plan(&repo_root));
    running.update_command_source(plan.command_source);
    let reporter = ProgressReporter::new(
        session_id,
        &run_id,
        environment,
        target,
        plan.command_source,
        tx,
    );
    reporter.send("started", None, "", "");
    let mut stages = Vec::new();

    let status = match target {
        VerifyTarget::Build => match plan.build.as_ref() {
            Some(build) => {
                let stage = run_stage(
                    &repo_root,
                    StageName::Build,
                    build,
                    &running.cancel,
                    &reporter,
                )
                .await;
                let stage_status = stage_status(&stage).to_string();
                stages.push(stage);
                match stage_status.as_str() {
                    "passed" => "passed",
                    "cancelled" => "cancelled",
                    _ => "failed",
                }
            }
            None => return command_not_found(session_id, environment, target, plan.command_source),
        },
        VerifyTarget::Test => match plan.test.as_ref() {
            Some(test) => {
                let stage = run_stage(
                    &repo_root,
                    StageName::Test,
                    test,
                    &running.cancel,
                    &reporter,
                )
                .await;
                let stage_status = stage_status(&stage).to_string();
                stages.push(stage);
                match stage_status.as_str() {
                    "passed" => "passed",
                    "cancelled" => "cancelled",
                    _ => "failed",
                }
            }
            None => return command_not_found(session_id, environment, target, plan.command_source),
        },
        VerifyTarget::All => {
            let Some(build) = plan.build.as_ref() else {
                return command_not_found(session_id, environment, target, plan.command_source);
            };
            let build_stage = run_stage(
                &repo_root,
                StageName::Build,
                build,
                &running.cancel,
                &reporter,
            )
            .await;
            let build_status = stage_status(&build_stage).to_string();
            stages.push(build_stage);
            if build_status == "cancelled" {
                stages.push(skipped_stage(StageName::Test, "验证已取消，已跳过测试"));
                "cancelled"
            } else if build_status != "passed" {
                stages.push(skipped_stage(StageName::Test, "构建失败，已跳过测试"));
                "failed"
            } else if let Some(test) = plan.test.as_ref() {
                let test_stage = run_stage(
                    &repo_root,
                    StageName::Test,
                    test,
                    &running.cancel,
                    &reporter,
                )
                .await;
                let test_status = stage_status(&test_stage).to_string();
                stages.push(test_stage);
                match test_status.as_str() {
                    "passed" => "passed",
                    "cancelled" => "cancelled",
                    _ => "failed",
                }
            } else {
                stages.push(skipped_stage(StageName::Test, "未配置测试命令，已跳过"));
                "passed"
            }
        }
    };
    if status == "cancelled" {
        reporter.send("cancelled", None, "", "");
    }

    json!({
        "sessionId": session_id,
        "runId": run_id,
        "status": status,
        "environment": environment,
        "target": target.as_str(),
        "commandSource": plan.command_source,
        "durationMs": started.elapsed().as_millis() as u64,
        "stages": stages
    })
}

pub fn invalid_target_result(session_id: &str, environment: &str) -> serde_json::Value {
    json!({
        "sessionId": session_id,
        "runId": new_run_id(),
        "status": "error",
        "environment": environment,
        "target": "all",
        "commandSource": "auto",
        "durationMs": 0,
        "message": "验证目标不支持",
        "stages": []
    })
}

fn load_manual_plan(repo_root: &Path, environment: &str) -> Option<VerifyPlan> {
    let config = load_project_verify_config(repo_root)?;
    manual_plan_from_config(&config, environment)
}

fn manual_plan_from_config(config: &ProjectVerifyConfig, environment: &str) -> Option<VerifyPlan> {
    let env_name = if config.environments.contains_key(environment) {
        environment
    } else {
        config.default_environment.as_deref().unwrap_or("default")
    };
    let env = config.environments.get(env_name)?;
    Some(VerifyPlan {
        command_source: "manual",
        build: manual_stage(StageName::Build, env.build.as_ref()),
        test: manual_stage(StageName::Test, env.test.as_ref()),
    })
}

fn manual_stage(stage: StageName, command: Option<&ProjectVerifyCommand>) -> Option<StagePlan> {
    let command = command?;
    if !command.enabled {
        return None;
    }
    let argv = parse_manual_command(&command.command).ok()?;
    Some(StagePlan {
        commands: vec![CommandSpec {
            argv,
            display: command.command.clone(),
            timeout_secs: clamp_timeout(command.timeout_seconds, stage.default_timeout_secs()),
        }],
    })
}

fn load_project_verify_config(repo_root: &Path) -> Option<ProjectVerifyConfig> {
    let path = kn_common::path::config_dir().join("projects.json");
    let text = std::fs::read_to_string(path).ok()?;
    let projects: Vec<ProjectInfo> = serde_json::from_str(&text).ok()?;
    let repo = repo_root
        .canonicalize()
        .unwrap_or_else(|_| repo_root.to_path_buf());
    projects.into_iter().find_map(|project| {
        let project_path = PathBuf::from(project.path);
        let canonical = project_path.canonicalize().unwrap_or(project_path);
        if repo == canonical || repo.starts_with(&canonical) || canonical.starts_with(&repo) {
            project.verify
        } else {
            None
        }
    })
}

fn auto_plan(repo_root: &Path) -> VerifyPlan {
    if repo_root.join("package.json").exists() {
        return node_plan(repo_root);
    }
    if repo_root.join("pom.xml").exists() {
        return plan(
            "auto",
            Some(stage(vec![cmd(
                &["mvn", "-q", "-DskipTests", "compile"],
                DEFAULT_BUILD_TIMEOUT_SECS,
            )])),
            Some(stage(vec![cmd(
                &["mvn", "-q", "test"],
                DEFAULT_TEST_TIMEOUT_SECS,
            )])),
        );
    }
    if repo_root.join("build.gradle").exists() || repo_root.join("build.gradle.kts").exists() {
        let gradle = if repo_root.join("gradlew").exists() {
            "./gradlew"
        } else {
            "gradle"
        };
        return plan(
            "auto",
            Some(stage(vec![cmd(
                &[gradle, "classes", "testClasses", "-x", "test"],
                DEFAULT_BUILD_TIMEOUT_SECS,
            )])),
            Some(stage(vec![cmd(
                &[gradle, "test"],
                DEFAULT_TEST_TIMEOUT_SECS,
            )])),
        );
    }
    if repo_root.join("go.mod").exists() {
        return plan(
            "auto",
            Some(stage(vec![cmd(
                &["go", "test", "./...", "-run", "^$"],
                DEFAULT_BUILD_TIMEOUT_SECS,
            )])),
            Some(stage(vec![cmd(
                &["go", "test", "./..."],
                DEFAULT_TEST_TIMEOUT_SECS,
            )])),
        );
    }
    if repo_root.join("Cargo.toml").exists() {
        return plan(
            "auto",
            Some(stage(vec![cmd(
                &["cargo", "check"],
                DEFAULT_BUILD_TIMEOUT_SECS,
            )])),
            Some(stage(vec![cmd(
                &["cargo", "test"],
                DEFAULT_TEST_TIMEOUT_SECS,
            )])),
        );
    }
    if repo_root.join("pyproject.toml").exists()
        || repo_root.join("pytest.ini").exists()
        || repo_root.join("requirements.txt").exists()
    {
        return plan(
            "auto",
            Some(stage(vec![cmd(
                &["python", "-m", "compileall", "."],
                DEFAULT_BUILD_TIMEOUT_SECS,
            )])),
            Some(stage(vec![cmd(
                &["python", "-m", "pytest"],
                DEFAULT_TEST_TIMEOUT_SECS,
            )])),
        );
    }
    plan("auto", None, None)
}

fn node_plan(repo_root: &Path) -> VerifyPlan {
    let text = std::fs::read_to_string(repo_root.join("package.json")).unwrap_or_default();
    let package: serde_json::Value = serde_json::from_str(&text).unwrap_or_default();
    let scripts = package.get("scripts").and_then(|v| v.as_object());
    let manager = package_manager(repo_root);
    let mut build_commands = Vec::new();
    if scripts
        .and_then(|s| s.get("typecheck"))
        .and_then(|v| v.as_str())
        .is_some()
    {
        build_commands.push(package_run_cmd(
            &manager,
            "typecheck",
            DEFAULT_BUILD_TIMEOUT_SECS,
        ));
    }
    if scripts
        .and_then(|s| s.get("build"))
        .and_then(|v| v.as_str())
        .is_some()
    {
        build_commands.push(package_run_cmd(
            &manager,
            "build",
            DEFAULT_BUILD_TIMEOUT_SECS,
        ));
    }
    let test_script = scripts
        .and_then(|s| s.get("test"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let test = if is_valid_test_script(test_script) {
        Some(stage(vec![package_run_cmd(
            &manager,
            "test",
            DEFAULT_TEST_TIMEOUT_SECS,
        )]))
    } else {
        None
    };
    plan(
        "auto",
        if build_commands.is_empty() {
            None
        } else {
            Some(stage(build_commands))
        },
        test,
    )
}

fn package_manager(repo_root: &Path) -> String {
    if repo_root.join("pnpm-lock.yaml").exists() {
        "pnpm".to_string()
    } else if repo_root.join("yarn.lock").exists() {
        "yarn".to_string()
    } else if repo_root.join("package-lock.json").exists() {
        "npm".to_string()
    } else {
        "npm".to_string()
    }
}

fn package_run_cmd(manager: &str, script: &str, timeout_secs: u64) -> CommandSpec {
    match manager {
        "yarn" => cmd(&["yarn", script], timeout_secs),
        _ => cmd(&[manager, "run", script], timeout_secs),
    }
}

fn is_valid_test_script(script: &str) -> bool {
    let normalized = script.trim().to_ascii_lowercase();
    !normalized.is_empty() && !normalized.contains("no test specified")
}

fn plan(source: &'static str, build: Option<StagePlan>, test: Option<StagePlan>) -> VerifyPlan {
    VerifyPlan {
        command_source: source,
        build,
        test,
    }
}

fn stage(commands: Vec<CommandSpec>) -> StagePlan {
    StagePlan { commands }
}

fn cmd(parts: &[&str], timeout_secs: u64) -> CommandSpec {
    let argv = parts.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    CommandSpec {
        display: parts.join(" "),
        argv,
        timeout_secs,
    }
}

async fn run_stage(
    repo_root: &Path,
    name: StageName,
    plan: &StagePlan,
    cancel: &CancellationToken,
    reporter: &ProgressReporter,
) -> serde_json::Value {
    let started = Instant::now();
    let mut output = OutputTailBuffer::new();
    let command_display = display_commands(&plan.commands);
    reporter.send("stageStarted", Some(name), &command_display, "");
    for command in &plan.commands {
        match run_command(repo_root, command, cancel, reporter, name).await {
            CommandOutcome::Passed { output_tail } => {
                output.push_str(&output_tail);
            }
            CommandOutcome::Failed {
                exit_code,
                output_tail,
            } => {
                output.push_str(&output_tail);
                let result = stage_result(
                    name,
                    "failed",
                    command_display.clone(),
                    exit_code,
                    started.elapsed(),
                    output.as_str(),
                );
                reporter.send(
                    "stageFinished",
                    Some(name),
                    &command_display,
                    output.as_str(),
                );
                return result;
            }
            CommandOutcome::Timeout { output_tail } => {
                output.push_str(&output_tail);
                let result = stage_result(
                    name,
                    "timeout",
                    command_display.clone(),
                    None,
                    started.elapsed(),
                    output.as_str(),
                );
                reporter.send(
                    "stageFinished",
                    Some(name),
                    &command_display,
                    output.as_str(),
                );
                return result;
            }
            CommandOutcome::Io { output_tail } => {
                output.push_str(&output_tail);
                let result = stage_result(
                    name,
                    "commandNotFound",
                    command_display.clone(),
                    None,
                    started.elapsed(),
                    output.as_str(),
                );
                reporter.send(
                    "stageFinished",
                    Some(name),
                    &command_display,
                    output.as_str(),
                );
                return result;
            }
            CommandOutcome::Cancelled { output_tail } => {
                output.push_str(&output_tail);
                let result = stage_result(
                    name,
                    "cancelled",
                    command_display.clone(),
                    None,
                    started.elapsed(),
                    output.as_str(),
                );
                reporter.send(
                    "stageFinished",
                    Some(name),
                    &command_display,
                    output.as_str(),
                );
                return result;
            }
        }
    }
    let result = stage_result(
        name,
        "passed",
        command_display.clone(),
        Some(0),
        started.elapsed(),
        output.as_str(),
    );
    reporter.send(
        "stageFinished",
        Some(name),
        &command_display,
        output.as_str(),
    );
    result
}

enum CommandOutcome {
    Passed {
        output_tail: String,
    },
    Failed {
        exit_code: Option<i32>,
        output_tail: String,
    },
    Timeout {
        output_tail: String,
    },
    Io {
        output_tail: String,
    },
    Cancelled {
        output_tail: String,
    },
}

async fn run_command(
    repo_root: &Path,
    spec: &CommandSpec,
    cancel: &CancellationToken,
    reporter: &ProgressReporter,
    stage: StageName,
) -> CommandOutcome {
    let Some(program) = spec.argv.first() else {
        return CommandOutcome::Io {
            output_tail: "空命令".to_string(),
        };
    };

    let execution_path = execution_path();
    let resolved_program = resolve_program(program, &execution_path);
    let mut command = Command::new(&resolved_program);
    command
        .current_dir(repo_root)
        .args(spec.argv.iter().skip(1))
        .env("PATH", &execution_path)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);

    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(err) => {
            return CommandOutcome::Io {
                output_tail: if err.kind() == ErrorKind::NotFound {
                    format!(
                        "无法找到可执行命令 `{}`。Agent PATH={}",
                        program, execution_path
                    )
                } else {
                    format!("无法执行命令 `{}`: {}", spec.display, err)
                },
            };
        }
    };

    let (output_tx, mut output_rx) = mpsc::channel::<Vec<u8>>(64);
    if let Some(stdout) = child.stdout.take() {
        spawn_output_reader(stdout, output_tx.clone());
    }
    if let Some(stderr) = child.stderr.take() {
        spawn_output_reader(stderr, output_tx);
    }

    let started = Instant::now();
    let deadline = tokio::time::sleep(Duration::from_secs(spec.timeout_secs));
    tokio::pin!(deadline);
    let mut progress_tick = tokio::time::interval(PROGRESS_INTERVAL);
    let mut output = OutputTailBuffer::new();
    let mut pending_bytes = 0usize;
    let mut last_emit = Instant::now();
    let mut output_closed = false;

    loop {
        tokio::select! {
            _ = cancel.cancelled() => {
                let _ = child.kill().await;
                let _ = child.wait().await;
                output.push_str("\n验证已取消");
                reporter.send("stageOutput", Some(stage), &spec.display, output.as_str());
                return CommandOutcome::Cancelled {
                    output_tail: output.into_string(),
                };
            }
            _ = &mut deadline => {
                let _ = child.kill().await;
                let _ = child.wait().await;
                output.push_str(&format!("\n命令超时：{}", spec.display));
                reporter.send("stageOutput", Some(stage), &spec.display, output.as_str());
                return CommandOutcome::Timeout {
                    output_tail: output.into_string(),
                };
            }
            maybe_chunk = output_rx.recv(), if !output_closed => {
                if let Some(chunk) = maybe_chunk {
                    let text = String::from_utf8_lossy(&chunk);
                    output.push_str(&text);
                    pending_bytes += chunk.len();
                    if pending_bytes >= PROGRESS_OUTPUT_BYTES || last_emit.elapsed() >= PROGRESS_INTERVAL {
                        reporter.send("stageOutput", Some(stage), &spec.display, output.as_str());
                        pending_bytes = 0;
                        last_emit = Instant::now();
                    }
                } else {
                    output_closed = true;
                }
            }
            _ = progress_tick.tick() => {
                if pending_bytes > 0 {
                    reporter.send("stageOutput", Some(stage), &spec.display, output.as_str());
                    pending_bytes = 0;
                    last_emit = Instant::now();
                }
            }
            status = child.wait() => {
                while let Ok(chunk) = output_rx.try_recv() {
                    let text = String::from_utf8_lossy(&chunk);
                    output.push_str(&text);
                }
                if pending_bytes > 0 || started.elapsed() >= PROGRESS_INTERVAL {
                    reporter.send("stageOutput", Some(stage), &spec.display, output.as_str());
                }
                let output_tail = output.into_string();
                return match status {
                    Ok(status) if status.success() => CommandOutcome::Passed {
                        output_tail,
                    },
                    Ok(status) => CommandOutcome::Failed {
                        exit_code: status.code(),
                        output_tail,
                    },
                    Err(err) => CommandOutcome::Io {
                        output_tail: format!("无法等待命令 `{}`: {}", spec.display, err),
                    },
                };
            }
        }
    }
}

fn spawn_output_reader<T>(mut reader: T, tx: mpsc::Sender<Vec<u8>>)
where
    T: tokio::io::AsyncRead + Unpin + Send + 'static,
{
    tokio::spawn(async move {
        let mut buf = vec![0u8; 4096];
        loop {
            match reader.read(&mut buf).await {
                Ok(0) => break,
                Ok(n) => {
                    if tx.send(buf[..n].to_vec()).await.is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    });
}

fn execution_path() -> String {
    let mut paths = std::env::var_os("PATH")
        .map(|value| std::env::split_paths(&value).collect::<Vec<_>>())
        .unwrap_or_default();
    if let Some(login_path) = login_shell_path() {
        paths.extend(std::env::split_paths(login_path));
    }
    for dir in common_tool_dirs() {
        if !paths.iter().any(|existing| existing == &dir) {
            paths.push(dir);
        }
    }
    std::env::join_paths(paths)
        .unwrap_or_default()
        .to_string_lossy()
        .to_string()
}

fn login_shell_path() -> Option<&'static str> {
    LOGIN_SHELL_PATH
        .get_or_init(|| read_login_shell_path().ok())
        .as_deref()
}

fn read_login_shell_path() -> Result<String, String> {
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".to_string());
    let output = std::process::Command::new(&shell)
        .arg("-lic")
        .arg("printf '__KN_PATH_START__%s__KN_PATH_END__\\n' \"$PATH\"")
        .output()
        .map_err(|err| err.to_string())?;
    if !output.status.success() {
        return Err(format!("login shell exited with {}", output.status));
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let Some(start) = stdout.find("__KN_PATH_START__") else {
        return Err("missing path start marker".to_string());
    };
    let path_start = start + "__KN_PATH_START__".len();
    let Some(end) = stdout[path_start..].find("__KN_PATH_END__") else {
        return Err("missing path end marker".to_string());
    };
    let path = stdout[path_start..path_start + end].trim();
    if path.is_empty() {
        return Err("empty login shell PATH".to_string());
    }
    Ok(path.to_string())
}

fn common_tool_dirs() -> Vec<PathBuf> {
    let mut dirs = vec![
        PathBuf::from("/opt/homebrew/bin"),
        PathBuf::from("/usr/local/bin"),
        PathBuf::from("/usr/bin"),
        PathBuf::from("/bin"),
        PathBuf::from("/usr/sbin"),
        PathBuf::from("/sbin"),
    ];
    if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
        dirs.extend([
            home.join(".cargo/bin"),
            home.join(".local/bin"),
            home.join(".bun/bin"),
            home.join("Library/pnpm"),
            home.join(".asdf/shims"),
            home.join(".sdkman/candidates/maven/current/bin"),
            home.join(".sdkman/candidates/gradle/current/bin"),
        ]);
    }
    dirs
}

fn resolve_program(program: &str, execution_path: &str) -> String {
    if program.contains('/') {
        return program.to_string();
    }
    for dir in std::env::split_paths(execution_path) {
        let candidate = dir.join(program);
        if is_executable_file(&candidate) {
            return candidate.to_string_lossy().to_string();
        }
    }
    program.to_string()
}

fn is_executable_file(path: &Path) -> bool {
    let Ok(metadata) = path.metadata() else {
        return false;
    };
    if !metadata.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        metadata.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        true
    }
}

fn stage_result(
    name: StageName,
    status: &str,
    command: String,
    exit_code: Option<i32>,
    duration: Duration,
    output: &str,
) -> serde_json::Value {
    json!({
        "name": name.as_str(),
        "status": status,
        "command": command,
        "exitCode": exit_code,
        "durationMs": duration.as_millis() as u64,
        "outputTail": tail_string(output)
    })
}

fn skipped_stage(name: StageName, message: &str) -> serde_json::Value {
    json!({
        "name": name.as_str(),
        "status": "skipped",
        "command": "",
        "exitCode": null,
        "durationMs": 0,
        "outputTail": message
    })
}

fn display_commands(commands: &[CommandSpec]) -> String {
    commands
        .iter()
        .map(|c| c.display.as_str())
        .collect::<Vec<_>>()
        .join(" && ")
}

fn stage_status(stage: &serde_json::Value) -> &str {
    stage["status"].as_str().unwrap_or("error")
}

fn command_not_found(
    session_id: &str,
    environment: &str,
    target: VerifyTarget,
    command_source: &str,
) -> serde_json::Value {
    json!({
        "sessionId": session_id,
        "runId": new_run_id(),
        "status": "commandNotFound",
        "environment": environment,
        "target": target.as_str(),
        "commandSource": command_source,
        "durationMs": 0,
        "message": "未找到可用命令，请在桌面端配置",
        "stages": []
    })
}

fn error_result(
    session_id: &str,
    environment: &str,
    target: VerifyTarget,
    status: &str,
    message: &str,
) -> serde_json::Value {
    json!({
        "sessionId": session_id,
        "runId": new_run_id(),
        "status": status,
        "environment": environment,
        "target": target.as_str(),
        "commandSource": "auto",
        "durationMs": 0,
        "message": message,
        "stages": []
    })
}

async fn repo_root(cwd: &str) -> Result<PathBuf, VerifyError> {
    let output = timeout(
        Duration::from_secs(5),
        Command::new("git")
            .arg("-C")
            .arg(cwd)
            .arg("rev-parse")
            .arg("--show-toplevel")
            .output(),
    )
    .await
    .map_err(|_| VerifyError::Timeout)?
    .map_err(|_| VerifyError::Io)?;
    if !output.status.success() {
        return Err(VerifyError::NotGitRepo);
    }
    Ok(PathBuf::from(
        String::from_utf8_lossy(&output.stdout).trim().to_string(),
    ))
}

fn parse_manual_command(command: &str) -> Result<Vec<String>, String> {
    reject_shell_syntax(command)?;
    let argv = split_command(command)?;
    reject_dangerous_command(&argv)?;
    Ok(argv)
}

fn reject_shell_syntax(command: &str) -> Result<(), String> {
    let dangerous = [";", "|", "&&", "||", "<", ">", "`", "$(", "\n"];
    if dangerous.iter().any(|token| command.contains(token)) {
        return Err("命令包含不支持的 shell 语法".to_string());
    }
    Ok(())
}

fn split_command(command: &str) -> Result<Vec<String>, String> {
    let mut result = Vec::new();
    let mut current = String::new();
    let mut quote: Option<char> = None;
    for ch in command.chars() {
        match (quote, ch) {
            (Some(q), c) if c == q => quote = None,
            (Some(_), c) => current.push(c),
            (None, '\'' | '"') => quote = Some(ch),
            (None, c) if c.is_whitespace() => {
                if !current.is_empty() {
                    result.push(std::mem::take(&mut current));
                }
            }
            (None, c) => current.push(c),
        }
    }
    if quote.is_some() {
        return Err("命令引号未闭合".to_string());
    }
    if !current.is_empty() {
        result.push(current);
    }
    if result.is_empty() {
        return Err("命令为空".to_string());
    }
    Ok(result)
}

fn reject_dangerous_command(argv: &[String]) -> Result<(), String> {
    let Some(program) = argv.first().map(|s| s.as_str()) else {
        return Err("命令为空".to_string());
    };
    let program_name = Path::new(program)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(program);
    match program_name {
        "rm" | "rmdir" | "mv" | "kill" | "pkill" | "shutdown" | "reboot" | "curl" | "wget"
        | "bash" | "sh" | "zsh" => return Err("命令不允许执行".to_string()),
        "git" => {
            let sub = argv.get(1).map(String::as_str).unwrap_or("");
            if matches!(
                sub,
                "push" | "reset" | "clean" | "checkout" | "rm" | "commit"
            ) {
                return Err("危险 git 命令不允许执行".to_string());
            }
        }
        "npm" | "pnpm" | "yarn" => {
            let sub = argv.get(1).map(String::as_str).unwrap_or("");
            if matches!(sub, "install" | "add" | "remove" | "upgrade" | "update") {
                return Err("依赖安装命令不允许执行".to_string());
            }
        }
        _ => {}
    }
    Ok(())
}

fn clamp_timeout(configured: Option<u64>, default_secs: u64) -> u64 {
    configured
        .unwrap_or(default_secs)
        .clamp(1, MAX_TIMEOUT_SECS)
}

struct OutputTailBuffer {
    text: String,
}

impl OutputTailBuffer {
    fn new() -> Self {
        Self {
            text: String::new(),
        }
    }

    fn push_str(&mut self, text: &str) {
        self.text.push_str(text);
        self.trim();
    }

    fn as_str(&self) -> &str {
        &self.text
    }

    fn into_string(self) -> String {
        self.text
    }

    fn trim(&mut self) {
        if self.text.len() <= MAX_OUTPUT_BYTES && self.text.lines().count() <= MAX_OUTPUT_LINES {
            return;
        }
        self.text = tail_string(&self.text);
    }
}

fn tail_string(text: &str) -> String {
    let mut lines = text
        .lines()
        .rev()
        .take(MAX_OUTPUT_LINES)
        .collect::<Vec<_>>();
    lines.reverse();
    let mut out = lines.join("\n");
    if out.len() > MAX_OUTPUT_BYTES {
        let mut start = out.len() - MAX_OUTPUT_BYTES;
        while !out.is_char_boundary(start) {
            start += 1;
        }
        out = out[start..].to_string();
    }
    out
}

fn new_run_id() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!("v_{nanos}")
}

#[derive(Debug)]
enum VerifyError {
    NotGitRepo,
    Timeout,
    Io,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use std::fs;

    #[test]
    fn detects_rust_build_and_test_commands() {
        let dir = unique_temp_dir("rust");
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join("Cargo.toml"),
            "[package]\nname='demo'\nversion='0.1.0'\n",
        )
        .unwrap();

        let plan = auto_plan(&dir);

        assert_eq!(plan.command_source, "auto");
        assert_eq!(plan.build.unwrap().commands[0].display, "cargo check");
        assert_eq!(plan.test.unwrap().commands[0].display, "cargo test");

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn detects_node_typecheck_then_build_and_ignores_placeholder_test() {
        let dir = unique_temp_dir("node");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("pnpm-lock.yaml"), "").unwrap();
        fs::write(
            dir.join("package.json"),
            r#"{"scripts":{"typecheck":"vue-tsc --noEmit","build":"vite build","test":"echo \"Error: no test specified\" && exit 1"}}"#,
        )
        .unwrap();

        let plan = auto_plan(&dir);
        let build = plan.build.unwrap();

        assert_eq!(build.commands[0].display, "pnpm run typecheck");
        assert_eq!(build.commands[1].display, "pnpm run build");
        assert!(plan.test.is_none());

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn manual_config_overrides_auto_detection_and_can_disable_test() {
        let mut environments = BTreeMap::new();
        environments.insert(
            "default".to_string(),
            kn_common::project::ProjectVerifyEnvironment {
                build: Some(ProjectVerifyCommand {
                    command: "mvn -q -DskipTests compile".to_string(),
                    enabled: true,
                    timeout_seconds: Some(400),
                }),
                test: Some(ProjectVerifyCommand {
                    command: "mvn -q test".to_string(),
                    enabled: false,
                    timeout_seconds: None,
                }),
            },
        );
        let config = ProjectVerifyConfig {
            default_environment: Some("default".to_string()),
            environments,
        };

        let plan = manual_plan_from_config(&config, "default").unwrap();

        assert_eq!(plan.command_source, "manual");
        assert_eq!(
            plan.build.unwrap().commands[0].display,
            "mvn -q -DskipTests compile"
        );
        assert!(plan.test.is_none());
    }

    #[test]
    fn manual_command_parser_keeps_quoted_arguments_and_rejects_shell() {
        let argv = parse_manual_command(
            "xcodebuild -destination 'generic/platform=iOS Simulator' build-for-testing",
        )
        .unwrap();

        assert_eq!(argv[2], "generic/platform=iOS Simulator");
        assert!(parse_manual_command("mvn test && rm -rf .").is_err());
        assert!(parse_manual_command("rm -rf target").is_err());
        assert!(parse_manual_command("git push").is_err());
    }

    #[test]
    fn resolve_program_finds_executable_in_augmented_path() {
        let dir = unique_temp_dir("path");
        fs::create_dir_all(&dir).unwrap();
        let tool = dir.join("mvn");
        fs::write(&tool, "#!/bin/sh\nexit 0\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut permissions = fs::metadata(&tool).unwrap().permissions();
            permissions.set_mode(0o755);
            fs::set_permissions(&tool, permissions).unwrap();
        }

        let path = std::env::join_paths([dir.as_path()]).unwrap();
        let resolved = resolve_program("mvn", &path.to_string_lossy());

        assert_eq!(resolved, tool.to_string_lossy());
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn execution_path_includes_login_shell_path_entries() {
        let path = execution_path();
        assert!(path.contains("/usr/bin") || path.contains("/bin"));
    }

    #[test]
    fn tail_string_truncates_on_utf8_boundary() {
        let text = "好".repeat(MAX_OUTPUT_BYTES);
        let tail = tail_string(&text);

        assert!(tail.len() <= MAX_OUTPUT_BYTES);
        assert!(tail.chars().all(|ch| ch == '好'));
    }

    #[test]
    fn output_tail_buffer_keeps_only_bounded_tail() {
        let mut buffer = OutputTailBuffer::new();
        for index in 0..500 {
            buffer.push_str(&format!("line-{index:03}-{}\n", "x".repeat(512)));
        }

        assert!(buffer.as_str().len() <= MAX_OUTPUT_BYTES);
        assert!(buffer.as_str().lines().count() <= MAX_OUTPUT_LINES);
        assert!(buffer.as_str().contains("line-499"));
        assert!(!buffer.as_str().contains("line-000"));
    }

    #[test]
    fn invalid_target_result_does_not_echo_invalid_target_to_ios() {
        let result = invalid_target_result("s_1", "default");

        assert_eq!(result["status"], "error");
        assert_eq!(result["target"], "all");
        assert_eq!(result["message"], "验证目标不支持");
    }

    fn unique_temp_dir(label: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("kn-verify-{label}-{nanos}"))
    }
}
