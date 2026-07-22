use crate::session::verification_history::{LastVerification, VerificationHistory};
use kn_common::project::{ProjectInfo, ProjectVerifyCommand, ProjectVerifyConfig};
use regex_lite::Regex;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::{BTreeMap, HashMap};
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, ErrorKind, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Arc, Mutex, OnceLock};
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
const VERIFY_RUN_LOG_TTL_SECS: u64 = 24 * 60 * 60;
const VERIFY_RUN_LOG_MAX_BYTES: u64 = 200 * 1024 * 1024;
const VERIFY_LOG_WINDOW_MAX_CONTEXT: usize = 300;
const VERIFY_LOG_ISSUE_MAX_MATCHERS: usize = 80;
const VERIFY_LOG_ISSUE_MAX_PATTERN_LEN: usize = 300;
const VERIFY_LOG_ISSUE_MAX_LIMIT: usize = 300;

static RUNNING_SESSIONS: OnceLock<Mutex<HashMap<String, RunningRun>>> = OnceLock::new();
static VERIFY_RUN_LOGS: OnceLock<Mutex<HashMap<String, Arc<Mutex<VerifyRunLog>>>>> =
    OnceLock::new();
static VERIFY_RUN_DISK_CLEANUP: OnceLock<()> = OnceLock::new();
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
pub enum StageName {
    Build,
    Test,
}

impl StageName {
    pub fn as_str(self) -> &'static str {
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

struct ResolvedVerifyPlan {
    plan: VerifyPlan,
    environment: String,
    available_environments: Vec<String>,
    detected_languages: Vec<String>,
    manual_config: Option<ProjectVerifyConfig>,
}

#[derive(Clone)]
struct VerifyRunLogStageSnapshot {
    stage: StageName,
    command: String,
    line_count: usize,
    byte_count: u64,
    tail_text: String,
    tail_start_line: usize,
    tail_end_line: usize,
    truncated: bool,
}

#[derive(Clone)]
struct VerifyLogIssueScanTarget {
    stage: StageName,
    path: PathBuf,
    line_count: usize,
}

struct VerifyRunLog {
    session_id: String,
    run_id: String,
    dir: PathBuf,
    created_at: Instant,
    expires_at: Option<Instant>,
    build: Option<StageLogFile>,
    test: Option<StageLogFile>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct VerifyRunLogMeta {
    session_id: String,
    run_id: String,
    created_at: u64,
    #[serde(default)]
    finished_at: Option<u64>,
}

impl VerifyRunLog {
    fn create(session_id: &str, run_id: &str) -> Self {
        ensure_verify_run_disk_cleanup_once();
        let dir = verify_runs_dir().join(run_id);
        let _ = write_verify_run_meta(
            &dir,
            session_id,
            run_id,
            unix_millis_at(SystemTime::now()),
            None,
        );
        Self {
            session_id: session_id.to_string(),
            run_id: run_id.to_string(),
            dir,
            created_at: Instant::now(),
            expires_at: None,
            build: None,
            test: None,
        }
    }

    fn append(&mut self, stage: StageName, command: &str, text: &str) {
        if text.is_empty() {
            return;
        }
        let dir = self.dir.clone();
        let log = self.stage_mut(stage, command, &dir);
        log.append(text);
    }

    fn mark_finished(&mut self) {
        self.expires_at = Some(Instant::now() + Duration::from_secs(VERIFY_RUN_LOG_TTL_SECS));
        let created_at = read_verify_run_meta(&self.dir)
            .map(|meta| meta.created_at)
            .unwrap_or_else(|| unix_millis_at(SystemTime::now()));
        let _ = write_verify_run_meta(
            &self.dir,
            &self.session_id,
            &self.run_id,
            created_at,
            Some(unix_millis_at(SystemTime::now())),
        );
    }

    fn restore_from_dir(
        dir: &Path,
        session_id: &str,
        run_id: &str,
        now: SystemTime,
    ) -> Option<Self> {
        let meta = read_verify_run_meta(dir)?;
        if meta.session_id != session_id || meta.run_id != run_id {
            return None;
        }
        let finished_at = meta.finished_at?;
        let expires_at = system_time_from_millis(finished_at)?
            .checked_add(Duration::from_secs(VERIFY_RUN_LOG_TTL_SECS))?;
        let remaining = expires_at.duration_since(now).ok()?;
        let build = StageLogFile::restore(StageName::Build, dir);
        let test = StageLogFile::restore(StageName::Test, dir);
        if build.is_none() && test.is_none() {
            return None;
        }
        Some(Self {
            session_id: session_id.to_string(),
            run_id: run_id.to_string(),
            dir: dir.to_path_buf(),
            created_at: Instant::now(),
            expires_at: Some(Instant::now() + remaining),
            build,
            test,
        })
    }

    fn is_expired(&self) -> bool {
        self.expires_at
            .map(|expires_at| Instant::now() >= expires_at)
            .unwrap_or(false)
    }

    fn snapshot(&self) -> Vec<VerifyRunLogStageSnapshot> {
        [self.build.as_ref(), self.test.as_ref()]
            .into_iter()
            .flatten()
            .map(StageLogFile::snapshot)
            .collect()
    }

    fn window(
        &self,
        stage: StageName,
        center_line: usize,
        before: usize,
        after: usize,
    ) -> serde_json::Value {
        let Some(log) = self.stage_ref(stage) else {
            return verify_log_window_error(
                &self.session_id,
                &self.run_id,
                stage,
                "stageNotFound",
                "阶段日志不存在",
            );
        };
        log.window(&self.session_id, &self.run_id, center_line, before, after)
    }

    fn stage_ref(&self, stage: StageName) -> Option<&StageLogFile> {
        match stage {
            StageName::Build => self.build.as_ref(),
            StageName::Test => self.test.as_ref(),
        }
    }

    fn stage_mut(&mut self, stage: StageName, command: &str, dir: &Path) -> &mut StageLogFile {
        let slot = match stage {
            StageName::Build => &mut self.build,
            StageName::Test => &mut self.test,
        };
        slot.get_or_insert_with(|| StageLogFile::create(stage, command, dir))
    }

    fn issue_scan_targets(&self, stages: &[StageName]) -> Vec<VerifyLogIssueScanTarget> {
        stages
            .iter()
            .filter_map(|stage| {
                let log = self.stage_ref(*stage)?;
                Some(VerifyLogIssueScanTarget {
                    stage: *stage,
                    path: log.path.clone(),
                    line_count: log.line_count(),
                })
            })
            .collect()
    }
}

struct StageLogFile {
    stage: StageName,
    command: String,
    path: PathBuf,
    file: Option<File>,
    line_starts: Vec<u64>,
    byte_count: u64,
    ends_with_newline: bool,
    truncated: bool,
}

impl StageLogFile {
    fn create(stage: StageName, command: &str, dir: &Path) -> Self {
        let path = dir.join(format!("{}.log", stage.as_str()));
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .read(true)
            .open(&path)
            .ok();
        Self {
            stage,
            command: command.to_string(),
            path,
            file,
            line_starts: vec![0],
            byte_count: 0,
            ends_with_newline: false,
            truncated: false,
        }
    }

    fn restore(stage: StageName, dir: &Path) -> Option<Self> {
        let path = dir.join(format!("{}.log", stage.as_str()));
        let byte_count = fs::metadata(&path).ok()?.len();
        if byte_count > VERIFY_RUN_LOG_MAX_BYTES || File::open(&path).is_err() {
            return None;
        }
        let mut reader = BufReader::new(File::open(&path).ok()?);
        let mut line_starts = vec![0];
        let mut offset = 0_u64;
        let mut ends_with_newline = false;
        let mut buffer = [0_u8; 8192];
        loop {
            let read = reader.read(&mut buffer).ok()?;
            if read == 0 {
                break;
            }
            for (index, byte) in buffer[..read].iter().enumerate() {
                if *byte == b'\n' {
                    line_starts.push(offset + index as u64 + 1);
                }
            }
            ends_with_newline = buffer[read - 1] == b'\n';
            offset += read as u64;
        }
        Some(Self {
            stage,
            command: "已完成验证".to_string(),
            path,
            file: None,
            line_starts,
            byte_count,
            ends_with_newline,
            truncated: byte_count >= VERIFY_RUN_LOG_MAX_BYTES,
        })
    }

    fn append(&mut self, text: &str) {
        if self.truncated || text.is_empty() {
            return;
        }
        let remaining = VERIFY_RUN_LOG_MAX_BYTES.saturating_sub(self.byte_count);
        if remaining == 0 {
            self.truncated = true;
            return;
        }
        let bytes = text.as_bytes();
        let write_len = if bytes.len() as u64 > remaining {
            self.truncated = true;
            utf8_boundary_len(text, remaining as usize)
        } else {
            bytes.len()
        };
        if write_len == 0 {
            return;
        }
        let slice = &text[..write_len];
        if let Some(file) = self.file.as_mut() {
            let _ = file.write_all(slice.as_bytes());
            let _ = file.flush();
        }
        for (index, byte) in slice.as_bytes().iter().enumerate() {
            if *byte == b'\n' {
                self.line_starts.push(self.byte_count + index as u64 + 1);
            }
        }
        self.byte_count += write_len as u64;
        self.ends_with_newline = slice.as_bytes().last() == Some(&b'\n');
    }

    fn line_count(&self) -> usize {
        if self.byte_count == 0 {
            return 0;
        }
        let possible = self.line_starts.len();
        if self.ends_with_newline {
            possible.saturating_sub(1)
        } else {
            possible
        }
    }

    fn snapshot(&self) -> VerifyRunLogStageSnapshot {
        let line_count = self.line_count();
        let tail_start = line_count.saturating_sub(MAX_OUTPUT_LINES).max(1);
        let tail = self.read_lines(tail_start, line_count);
        VerifyRunLogStageSnapshot {
            stage: self.stage,
            command: self.command.clone(),
            line_count,
            byte_count: self.byte_count,
            tail_text: tail,
            tail_start_line: if line_count == 0 { 0 } else { tail_start },
            tail_end_line: line_count,
            truncated: self.truncated,
        }
    }

    fn window(
        &self,
        session_id: &str,
        run_id: &str,
        center_line: usize,
        before: usize,
        after: usize,
    ) -> serde_json::Value {
        let line_count = self.line_count();
        if line_count == 0 {
            return json!({
                "sessionId": session_id,
                "runId": run_id,
                "stage": self.stage.as_str(),
                "status": "ok",
                "startLine": 0,
                "endLine": 0,
                "centerLine": center_line,
                "lines": [],
                "hasEarlier": false,
                "hasLater": false,
                "contentTruncated": false,
            });
        }
        let before = before.min(VERIFY_LOG_WINDOW_MAX_CONTEXT);
        let after = after.min(VERIFY_LOG_WINDOW_MAX_CONTEXT);
        let center = center_line.clamp(1, line_count);
        let start = center.saturating_sub(before).max(1);
        let end = (center + after).min(line_count);
        let (lines, content_truncated) = self.read_line_entries(start, end, center);
        let returned_start = lines
            .first()
            .and_then(|line| line["lineNumber"].as_u64())
            .map(|line| line as usize)
            .unwrap_or(start);
        let returned_end = lines
            .last()
            .and_then(|line| line["lineNumber"].as_u64())
            .map(|line| line as usize)
            .unwrap_or(end);
        json!({
            "sessionId": session_id,
            "runId": run_id,
            "stage": self.stage.as_str(),
            "status": "ok",
            "startLine": returned_start,
            "endLine": returned_end,
            "centerLine": center,
            "lines": lines,
            "hasEarlier": returned_start > 1,
            "hasLater": returned_end < line_count,
            "contentTruncated": content_truncated,
        })
    }

    fn read_line_entries(
        &self,
        start_line: usize,
        end_line: usize,
        center_line: usize,
    ) -> (Vec<serde_json::Value>, bool) {
        let mut lines = Vec::with_capacity(end_line.saturating_sub(start_line).saturating_add(1));
        let mut line_truncated = false;
        for line_number in start_line..=end_line {
            let (text, truncated) = self.read_line_text(line_number);
            line_truncated |= truncated;
            lines.push((line_number, text));
        }
        let (lines, budget_truncated) =
            crate::session::response_limits::bounded_log_lines_around_center(lines, center_line);
        (lines, line_truncated || budget_truncated)
    }

    fn read_line_text(&self, line_number: usize) -> (String, bool) {
        let Some(start_offset) = self.line_starts.get(line_number - 1).copied() else {
            return (String::new(), false);
        };
        let end_offset = self
            .line_starts
            .get(line_number)
            .copied()
            .unwrap_or(self.byte_count);
        if end_offset <= start_offset {
            return (String::new(), false);
        }
        let available = (end_offset - start_offset) as usize;
        let read_len = available.min(crate::session::response_limits::LOG_LINE_MAX_BYTES + 1);
        let mut file = match File::open(&self.path) {
            Ok(file) => file,
            Err(_) => return (String::new(), false),
        };
        if file.seek(SeekFrom::Start(start_offset)).is_err() {
            return (String::new(), false);
        }
        let mut buf = vec![0; read_len];
        if file.read_exact(&mut buf).is_err() {
            return (String::new(), false);
        }
        let mut valid_len = buf.len();
        while valid_len > 0 && std::str::from_utf8(&buf[..valid_len]).is_err() {
            valid_len -= 1;
        }
        let text = std::str::from_utf8(&buf[..valid_len]).unwrap_or_default();
        let text = text.strip_suffix('\n').unwrap_or(text);
        let text = crate::session::response_limits::truncate_utf8(
            text,
            crate::session::response_limits::LOG_LINE_MAX_BYTES,
        );
        (text, available > read_len || valid_len < buf.len())
    }

    fn read_lines(&self, start_line: usize, end_line: usize) -> String {
        if start_line == 0 || end_line < start_line {
            return String::new();
        }
        let Some(start_offset) = self.line_starts.get(start_line - 1).copied() else {
            return String::new();
        };
        let end_offset = self
            .line_starts
            .get(end_line)
            .copied()
            .unwrap_or(self.byte_count);
        if end_offset <= start_offset {
            return String::new();
        }
        let mut file = match File::open(&self.path) {
            Ok(file) => file,
            Err(_) => return String::new(),
        };
        if file.seek(SeekFrom::Start(start_offset)).is_err() {
            return String::new();
        }
        let mut buf = vec![0; (end_offset - start_offset) as usize];
        if file.read_exact(&mut buf).is_err() {
            return String::new();
        }
        String::from_utf8_lossy(&buf).to_string()
    }
}

struct VerifyLogMatcher {
    rule_id: String,
    level: String,
    regex: Regex,
    context_before: usize,
    context_after: usize,
}

#[derive(Clone)]
struct RunningRun {
    run_id: String,
    environment: String,
    target: VerifyTarget,
    command_source: String,
    request_id: Option<String>,
    cancel: CancellationToken,
    started: Instant,
    current_stage: Option<StageName>,
    current_status: String,
    current_command: String,
    log: Arc<Mutex<VerifyRunLog>>,
}

struct RunningGuard {
    session_id: String,
    run_id: String,
    cancel: CancellationToken,
    target: VerifyTarget,
    completed: bool,
}

impl RunningGuard {
    fn try_acquire(
        session_id: &str,
        run_id: &str,
        environment: &str,
        target: VerifyTarget,
        request_id: Option<&str>,
    ) -> Option<Self> {
        let set = RUNNING_SESSIONS.get_or_init(|| Mutex::new(HashMap::new()));
        let mut guard = set.lock().unwrap_or_else(|e| e.into_inner());
        if guard.contains_key(session_id) {
            return None;
        }
        let cancel = CancellationToken::new();
        let started = Instant::now();
        let log = Arc::new(Mutex::new(VerifyRunLog::create(session_id, run_id)));
        register_run_log(log.clone());
        let started_at_ms = unix_millis() as u64;
        let initial_summary = LastVerification {
            run_id: run_id.to_string(),
            state: running_state_for_target(target).to_string(),
            started_at_ms,
            finished_at_ms: None,
            duration_ms: 0,
            target: target.as_str().to_string(),
            environment: environment.to_string(),
            command_source: "auto".to_string(),
            build_state: None,
            test_state: None,
            log_available: true,
            is_running: true,
        };
        if let Err(error) =
            VerificationHistory::default_at_config_dir().save(session_id, &initial_summary)
        {
            tracing::warn!(%error, session_id, "保存验证运行摘要失败");
        }
        guard.insert(
            session_id.to_string(),
            RunningRun {
                run_id: run_id.to_string(),
                environment: environment.to_string(),
                target,
                command_source: "auto".to_string(),
                request_id: request_id.map(str::to_owned),
                cancel: cancel.clone(),
                started,
                current_stage: None,
                current_status: "started".to_string(),
                current_command: String::new(),
                log,
            },
        );
        Some(Self {
            session_id: session_id.to_string(),
            run_id: run_id.to_string(),
            cancel,
            target,
            completed: false,
        })
    }

    fn update_plan_context(&self, environment: &str, command_source: &str) {
        let set = RUNNING_SESSIONS.get_or_init(|| Mutex::new(HashMap::new()));
        let mut guard = set.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(run) = guard.get_mut(&self.session_id) {
            if run.run_id == self.run_id {
                run.environment = environment.to_string();
                run.command_source = command_source.to_string();
            }
        }
    }

    fn log(&self) -> Option<Arc<Mutex<VerifyRunLog>>> {
        let set = RUNNING_SESSIONS.get_or_init(|| Mutex::new(HashMap::new()));
        let guard = set.lock().unwrap_or_else(|e| e.into_inner());
        guard
            .get(&self.session_id)
            .and_then(|run| (run.run_id == self.run_id).then(|| run.log.clone()))
    }

    fn finish(&mut self, result: &serde_json::Value) {
        let now = unix_millis() as u64;
        let state = summary_state(result, self.target);
        let summary = LastVerification {
            run_id: self.run_id.clone(),
            state,
            started_at_ms: now.saturating_sub(result["durationMs"].as_u64().unwrap_or_default()),
            finished_at_ms: Some(now),
            duration_ms: result["durationMs"].as_u64().unwrap_or_default(),
            target: result["target"]
                .as_str()
                .unwrap_or(self.target.as_str())
                .to_string(),
            environment: result["environment"]
                .as_str()
                .unwrap_or("default")
                .to_string(),
            command_source: result["commandSource"]
                .as_str()
                .unwrap_or("auto")
                .to_string(),
            build_state: stage_state(result, StageName::Build),
            test_state: stage_state(result, StageName::Test),
            log_available: is_verify_log_available(&self.run_id),
            is_running: false,
        };
        if let Err(error) =
            VerificationHistory::default_at_config_dir().save(&self.session_id, &summary)
        {
            tracing::warn!(%error, session_id = %self.session_id, "保存验证结果摘要失败");
        }
        self.completed = true;
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
    project_device_id: Option<u64>,
    project_path: Option<String>,
    request_id: Option<String>,
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

    pub fn new_project(
        project_key: &str,
        device_id: u64,
        project_path: &str,
        run_id: &str,
        environment: &str,
        target: VerifyTarget,
        command_source: &str,
        tx: Option<mpsc::UnboundedSender<String>>,
        request_id: Option<&str>,
    ) -> Self {
        let mut reporter = Self::new(project_key, run_id, environment, target, command_source, tx);
        reporter.project_device_id = Some(device_id);
        reporter.project_path = Some(project_path.to_string());
        reporter.request_id = request_id.map(str::to_owned);
        reporter
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
            project_device_id: None,
            project_path: None,
            request_id: None,
        }
    }

    pub fn new_project_with_started(
        project_key: &str,
        device_id: u64,
        project_path: &str,
        run_id: &str,
        environment: &str,
        target: VerifyTarget,
        command_source: &str,
        tx: Option<mpsc::UnboundedSender<String>>,
        started: Instant,
        request_id: Option<&str>,
    ) -> Self {
        let mut reporter = Self::new_with_started(
            project_key,
            run_id,
            environment,
            target,
            command_source,
            tx,
            started,
        );
        reporter.project_device_id = Some(device_id);
        reporter.project_path = Some(project_path.to_string());
        reporter.request_id = request_id.map(str::to_owned);
        reporter
    }

    pub fn send_cancelling(&self) {
        self.send("cancelling", None, "", "");
    }

    fn send_chunk(
        &self,
        status: &str,
        stage: StageName,
        command: &str,
        chunk: &ProgressOutputChunk,
    ) {
        update_running_status(&self.session_id, &self.run_id, status, Some(stage), command);
        let Some(tx) = self.tx.as_ref() else {
            return;
        };
        if chunk.text.is_empty() {
            return;
        }
        let mut data = self.base_data(status);
        data["stage"] = serde_json::Value::String(stage.as_str().to_string());
        if !command.is_empty() {
            data["command"] = serde_json::Value::String(command.to_string());
        }
        data["outputTail"] = serde_json::Value::String(chunk.text.clone());
        data["outputStartLine"] = serde_json::Value::Number(chunk.start_line.into());
        data["outputEndLine"] = serde_json::Value::Number(chunk.end_line.into());
        data["outputTruncated"] = serde_json::Value::Bool(chunk.truncated);
        let msg = self.progress_message(data);
        let _ = tx.send(msg);
    }

    fn base_data(&self, status: &str) -> serde_json::Value {
        let mut data = json!({
            "sessionId": self.session_id,
            "runId": self.run_id,
            "environment": self.environment,
            "target": self.target.as_str(),
            "commandSource": self.command_source,
            "status": status,
            "elapsedMs": self.started.elapsed().as_millis() as u64,
        });
        if let Some(request_id) = &self.request_id {
            data["requestId"] = serde_json::Value::String(request_id.clone());
        }
        data
    }

    fn send(&self, status: &str, stage: Option<StageName>, command: &str, output_tail: &str) {
        update_running_status(&self.session_id, &self.run_id, status, stage, command);
        let Some(tx) = self.tx.as_ref() else {
            return;
        };
        let mut data = self.base_data(status);
        if let Some(stage) = stage {
            data["stage"] = serde_json::Value::String(stage.as_str().to_string());
        }
        if !command.is_empty() {
            data["command"] = serde_json::Value::String(command.to_string());
        }
        if !output_tail.is_empty() {
            data["outputTail"] = serde_json::Value::String(tail_string(output_tail));
        }
        let msg = self.progress_message(data);
        let _ = tx.send(msg);
    }

    fn progress_message(&self, data: serde_json::Value) -> String {
        let device_id = self.project_device_id.unwrap_or(0);
        let project_path = self.project_path.as_deref().unwrap_or("");
        crate::proto::WsMessageBuilder::project_result(
            "project_verify_changes_progress",
            &self.session_id,
            device_id,
            project_path,
            data,
        )
    }
}

pub fn cancel(
    session_id: &str,
    run_id: &str,
) -> Option<(String, VerifyTarget, String, Instant, Option<String>)> {
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
        run.request_id.clone(),
    ))
}

fn update_running_status(
    session_id: &str,
    run_id: &str,
    status: &str,
    stage: Option<StageName>,
    command: &str,
) {
    let set = RUNNING_SESSIONS.get_or_init(|| Mutex::new(HashMap::new()));
    let mut guard = set.lock().unwrap_or_else(|e| e.into_inner());
    let Some(run) = guard.get_mut(session_id) else {
        return;
    };
    if run.run_id != run_id {
        return;
    }
    run.current_status = status.to_string();
    if let Some(stage) = stage {
        run.current_stage = Some(stage);
    }
    if !command.is_empty() {
        run.current_command = command.to_string();
    }
}

impl Drop for RunningGuard {
    fn drop(&mut self) {
        if !self.completed {
            tracing::debug!(session_id = %self.session_id, run_id = %self.run_id, "验证未完成，保留记录以便重启后恢复为中断");
        }
        let set = RUNNING_SESSIONS.get_or_init(|| Mutex::new(HashMap::new()));
        let mut guard = set.lock().unwrap_or_else(|e| e.into_inner());
        if guard.get(&self.session_id).map(|run| run.run_id.as_str()) == Some(self.run_id.as_str())
        {
            if let Some(run) = guard.get(&self.session_id) {
                run.log
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .mark_finished();
            }
            guard.remove(&self.session_id);
        }
    }
}

fn running_state_for_target(target: VerifyTarget) -> &'static str {
    match target {
        VerifyTarget::Test => "runningTest",
        VerifyTarget::All | VerifyTarget::Build => "runningBuild",
    }
}

fn stage_state(result: &serde_json::Value, stage: StageName) -> Option<String> {
    result["stages"]
        .as_array()?
        .iter()
        .find(|value| value["name"].as_str() == Some(stage.as_str()))?
        .get("status")?
        .as_str()
        .map(str::to_string)
}

fn summary_state(result: &serde_json::Value, target: VerifyTarget) -> String {
    match result["status"].as_str().unwrap_or("error") {
        "passed" => "passed".to_string(),
        "cancelled" => "cancelled".to_string(),
        "error" if result["message"].as_str() == Some("未找到可用命令，请在桌面端配置") => {
            "commandNotFound".to_string()
        }
        "failed" => {
            if stage_state(result, StageName::Test).as_deref() == Some("failed")
                || target == VerifyTarget::Test
            {
                "testFailed".to_string()
            } else {
                "buildFailed".to_string()
            }
        }
        other => other.to_string(),
    }
}

pub fn last_verification(session_id: &str) -> Option<serde_json::Value> {
    let set = RUNNING_SESSIONS.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some(run) = set
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(session_id)
        .cloned()
    {
        let state = match run.current_stage {
            Some(StageName::Test) => "runningTest",
            Some(StageName::Build) | None => running_state_for_target(run.target),
        };
        return Some(
            LastVerification {
                run_id: run.run_id,
                state: state.to_string(),
                started_at_ms: unix_millis() as u64 - run.started.elapsed().as_millis() as u64,
                finished_at_ms: None,
                duration_ms: run.started.elapsed().as_millis() as u64,
                target: run.target.as_str().to_string(),
                environment: run.environment,
                command_source: run.command_source,
                build_state: None,
                test_state: None,
                log_available: true,
                is_running: true,
            }
            .as_json(),
        );
    }
    let mut summary =
        VerificationHistory::default_at_config_dir().load(session_id, SystemTime::now())?;
    if summary.log_available && !is_verify_log_available(&summary.run_id) {
        summary.log_available = false;
    }
    Some(summary.as_json())
}

fn is_verify_log_available(run_id: &str) -> bool {
    let path = verify_runs_dir().join(run_id);
    is_verify_log_available_at(&path, SystemTime::now())
}

fn completed_result(running: &mut RunningGuard, result: serde_json::Value) -> serde_json::Value {
    running.finish(&result);
    result
}

pub async fn verify(
    session_id: &str,
    cwd: &str,
    environment: &str,
    target: VerifyTarget,
    tx: Option<mpsc::UnboundedSender<String>>,
    project_scope: (u64, String),
    request_id: Option<&str>,
) -> serde_json::Value {
    let run_id = new_run_id();
    let Some(mut running) =
        RunningGuard::try_acquire(session_id, &run_id, environment, target, request_id)
    else {
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
            return completed_result(
                &mut running,
                error_result(
                    session_id,
                    environment,
                    target,
                    "notGitRepo",
                    "当前目录不是 Git 仓库",
                ),
            );
        }
        Err(_) => {
            return completed_result(
                &mut running,
                error_result(
                    session_id,
                    environment,
                    target,
                    "error",
                    "无法检查 Git 仓库",
                ),
            );
        }
    };

    let verify_root = nearest_project_root(cwd, &repo_root);
    let started = Instant::now();
    let resolved = resolve_verify_plan(&verify_root, environment);
    let plan = resolved.plan;
    running.update_plan_context(&resolved.environment, plan.command_source);
    let (device_id, project_path) = project_scope;
    let reporter = ProgressReporter::new_project(
        session_id,
        device_id,
        &project_path,
        &run_id,
        &resolved.environment,
        target,
        plan.command_source,
        tx,
        request_id,
    );
    reporter.send("started", None, "", "");
    let mut stages = Vec::new();

    let status = match target {
        VerifyTarget::Build => match plan.build.as_ref() {
            Some(build) => {
                let stage = run_stage(
                    &verify_root,
                    StageName::Build,
                    build,
                    &running.cancel,
                    &reporter,
                    running.log(),
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
            None => {
                return completed_result(
                    &mut running,
                    command_not_found(
                        session_id,
                        &resolved.environment,
                        target,
                        plan.command_source,
                    ),
                );
            }
        },
        VerifyTarget::Test => match plan.test.as_ref() {
            Some(test) => {
                let stage = run_stage(
                    &verify_root,
                    StageName::Test,
                    test,
                    &running.cancel,
                    &reporter,
                    running.log(),
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
            None => {
                return completed_result(
                    &mut running,
                    command_not_found(
                        session_id,
                        &resolved.environment,
                        target,
                        plan.command_source,
                    ),
                );
            }
        },
        VerifyTarget::All => {
            let Some(build) = plan.build.as_ref() else {
                return completed_result(
                    &mut running,
                    command_not_found(
                        session_id,
                        &resolved.environment,
                        target,
                        plan.command_source,
                    ),
                );
            };
            let build_stage = run_stage(
                &verify_root,
                StageName::Build,
                build,
                &running.cancel,
                &reporter,
                running.log(),
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
                    &verify_root,
                    StageName::Test,
                    test,
                    &running.cancel,
                    &reporter,
                    running.log(),
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
    if let Some(log) = running.log() {
        log.lock()
            .unwrap_or_else(|e| e.into_inner())
            .mark_finished();
    }

    let result = json!({
        "sessionId": session_id,
        "runId": run_id,
        "status": status,
        "environment": resolved.environment,
        "target": target.as_str(),
        "commandSource": plan.command_source,
        "durationMs": started.elapsed().as_millis() as u64,
        "stages": stages
    });
    completed_result(&mut running, result)
}

pub async fn preview(session_id: &str, cwd: &str, environment: &str) -> serde_json::Value {
    let repo_root = match repo_root(cwd).await {
        Ok(root) => root,
        Err(VerifyError::NotGitRepo) => {
            return json!({
                "sessionId": session_id,
                "status": "notGitRepo",
                "cwd": cwd,
                "environment": environment,
                "availableEnvironments": [],
                "commandSource": "auto",
                "detectedLanguages": [],
                "message": "当前目录不是 Git 仓库"
            });
        }
        Err(_) => {
            return json!({
                "sessionId": session_id,
                "status": "error",
                "cwd": cwd,
                "environment": environment,
                "availableEnvironments": [],
                "commandSource": "auto",
                "detectedLanguages": [],
                "message": "无法检查 Git 仓库"
            });
        }
    };

    let verify_root = nearest_project_root(cwd, &repo_root);
    let resolved = resolve_verify_plan(&verify_root, environment);
    let build = preview_stage(
        StageName::Build,
        resolved.plan.build.as_ref(),
        resolved
            .manual_config
            .as_ref()
            .and_then(|config| manual_environment(config, &resolved.environment))
            .and_then(|env| env.build.as_ref()),
        resolved.plan.command_source,
    );
    let test = preview_stage(
        StageName::Test,
        resolved.plan.test.as_ref(),
        resolved
            .manual_config
            .as_ref()
            .and_then(|config| manual_environment(config, &resolved.environment))
            .and_then(|env| env.test.as_ref()),
        resolved.plan.command_source,
    );

    json!({
        "sessionId": session_id,
        "status": "ok",
        "cwd": cwd,
        "repoRoot": repo_root.to_string_lossy(),
        "projectRoot": verify_root.to_string_lossy(),
        "environment": resolved.environment,
        "availableEnvironments": resolved.available_environments,
        "commandSource": resolved.plan.command_source,
        "detectedLanguages": resolved.detected_languages,
        "build": build,
        "test": test
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

pub fn status(session_id: &str) -> serde_json::Value {
    ensure_verify_run_disk_cleanup_once();
    cleanup_expired_run_logs();
    let last_verification = last_verification(session_id);
    let set = RUNNING_SESSIONS.get_or_init(|| Mutex::new(HashMap::new()));
    let guard = set.lock().unwrap_or_else(|e| e.into_inner());
    let Some(run) = guard.get(session_id) else {
        return json!({
            "sessionId": session_id,
            "status": "idle",
            "lastVerification": last_verification,
        });
    };
    let stages = run
        .log
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .snapshot()
        .into_iter()
        .map(|stage| {
            json!({
                "stage": stage.stage.as_str(),
                "command": stage.command,
                "lineCount": stage.line_count,
                "byteCount": stage.byte_count,
                "tailText": stage.tail_text,
                "tailStartLine": stage.tail_start_line,
                "tailEndLine": stage.tail_end_line,
                "truncated": stage.truncated,
            })
        })
        .collect::<Vec<_>>();

    json!({
        "sessionId": session_id,
        "status": "running",
        "runId": run.run_id,
        "environment": run.environment,
        "target": run.target.as_str(),
        "commandSource": run.command_source,
        "elapsedMs": run.started.elapsed().as_millis() as u64,
        "currentStage": run.current_stage.map(StageName::as_str),
        "currentStatus": run.current_status,
        "currentCommand": run.current_command,
        "stages": stages,
        "lastVerification": last_verification,
    })
}

pub fn log_window(
    session_id: &str,
    run_id: &str,
    stage: StageName,
    center_line: usize,
    before: usize,
    after: usize,
) -> serde_json::Value {
    let Some(log) = get_run_log(session_id, run_id) else {
        return verify_log_window_error(
            session_id,
            run_id,
            stage,
            "runNotFound",
            "验证记录不存在或日志已过期",
        );
    };
    let result =
        log.lock()
            .unwrap_or_else(|e| e.into_inner())
            .window(stage, center_line, before, after);
    result
}

pub fn log_issues(
    session_id: &str,
    run_id: &str,
    stages: Vec<StageName>,
    rules_version: &str,
    matcher_values: &[serde_json::Value],
    limit: usize,
) -> serde_json::Value {
    let Some(log) = get_run_log(session_id, run_id) else {
        return json!({
            "sessionId": session_id,
            "runId": run_id,
            "status": "runNotFound",
            "rulesVersion": rules_version,
            "issues": [],
            "truncated": false,
            "message": "验证记录不存在或日志已过期",
        });
    };
    let matchers = match parse_issue_matchers(matcher_values) {
        Ok(matchers) => matchers,
        Err(message) => {
            return json!({
                "sessionId": session_id,
                "runId": run_id,
                "status": "invalidMatcher",
                "rulesVersion": rules_version,
                "issues": [],
                "truncated": false,
                "message": message,
            });
        }
    };
    let limit = limit.clamp(1, VERIFY_LOG_ISSUE_MAX_LIMIT);
    let targets = log
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .issue_scan_targets(&stages);

    let mut issues = Vec::new();
    let mut truncated = false;
    'targets: for target in targets {
        let found = scan_log_issues(&target, &matchers, limit.saturating_sub(issues.len()));
        for issue in found {
            let (added, item_truncated) =
                crate::session::response_limits::append_bounded_issue(&mut issues, issue);
            truncated |= item_truncated;
            if !added {
                break 'targets;
            }
        }
        if issues.len() >= limit {
            truncated = true;
            break;
        }
    }

    crate::session::response_limits::bounded_issue_response(
        session_id,
        run_id,
        rules_version,
        issues,
        truncated,
    )
}

fn scan_log_issues(
    target: &VerifyLogIssueScanTarget,
    matchers: &[VerifyLogMatcher],
    limit: usize,
) -> Vec<serde_json::Value> {
    if limit == 0 {
        return Vec::new();
    }
    let file = match File::open(&target.path) {
        Ok(file) => file,
        Err(_) => return Vec::new(),
    };
    let mut issues = Vec::new();
    for (index, line_result) in BufReader::new(file).lines().enumerate() {
        if index >= target.line_count {
            break;
        }
        let Ok(line) = line_result else { break };
        for matcher in matchers {
            if matcher.regex.is_match(&line) {
                let line_number = index + 1;
                issues.push(json!({
                    "issueId": format!("{}-{}-{}", target.stage.as_str(), line_number, matcher.rule_id),
                    "stage": target.stage.as_str(),
                    "lineNumber": line_number,
                    "level": matcher.level,
                    "ruleId": matcher.rule_id,
                    "preview": line,
                    "contextStartLine": line_number.saturating_sub(matcher.context_before).max(1),
                    "contextEndLine": (line_number + matcher.context_after).min(target.line_count),
                }));
                break;
            }
        }
        if issues.len() >= limit {
            break;
        }
    }
    issues
}

fn parse_issue_matchers(values: &[serde_json::Value]) -> Result<Vec<VerifyLogMatcher>, String> {
    if values.len() > VERIFY_LOG_ISSUE_MAX_MATCHERS {
        return Err("日志规则数量过多".to_string());
    }
    let mut matchers = Vec::new();
    for value in values {
        let rule_id = value["ruleId"].as_str().unwrap_or("").trim();
        let level = value["level"].as_str().unwrap_or("error").trim();
        let pattern = value["pattern"].as_str().unwrap_or("");
        if rule_id.is_empty()
            || pattern.is_empty()
            || pattern.len() > VERIFY_LOG_ISSUE_MAX_PATTERN_LEN
        {
            return Err("日志规则不可用".to_string());
        }
        let regex = Regex::new(pattern).map_err(|_| "日志规则不可用".to_string())?;
        matchers.push(VerifyLogMatcher {
            rule_id: rule_id.to_string(),
            level: level.to_string(),
            regex,
            context_before: value["contextBefore"].as_u64().unwrap_or(0).min(300) as usize,
            context_after: value["contextAfter"].as_u64().unwrap_or(0).min(300) as usize,
        });
    }
    Ok(matchers)
}

fn resolve_verify_plan(repo_root: &Path, environment: &str) -> ResolvedVerifyPlan {
    let manual_config = load_project_verify_config(repo_root);
    if let Some(config) = manual_config.clone() {
        if let Some((environment, plan)) =
            manual_plan_from_config_with_environment(&config, environment)
        {
            return ResolvedVerifyPlan {
                plan,
                environment,
                available_environments: config.environments.keys().cloned().collect(),
                detected_languages: detected_languages(repo_root),
                manual_config: Some(config),
            };
        }
    }

    ResolvedVerifyPlan {
        plan: auto_plan(repo_root),
        environment: environment.to_string(),
        available_environments: vec!["default".to_string()],
        detected_languages: detected_languages(repo_root),
        manual_config: None,
    }
}

fn manual_plan_from_config(config: &ProjectVerifyConfig, environment: &str) -> Option<VerifyPlan> {
    manual_plan_from_config_with_environment(config, environment).map(|(_, plan)| plan)
}

fn manual_plan_from_config_with_environment(
    config: &ProjectVerifyConfig,
    environment: &str,
) -> Option<(String, VerifyPlan)> {
    let env_name = if config.environments.contains_key(environment) {
        environment
    } else if let Some(default_environment) = config.default_environment.as_deref() {
        if config.environments.contains_key(default_environment) {
            default_environment
        } else {
            "default"
        }
    } else {
        "default"
    };
    let env = config.environments.get(env_name)?;
    Some((
        env_name.to_string(),
        VerifyPlan {
            command_source: "manual",
            build: manual_stage(StageName::Build, env.build.as_ref()),
            test: manual_stage(StageName::Test, env.test.as_ref()),
        },
    ))
}

fn manual_environment<'a>(
    config: &'a ProjectVerifyConfig,
    environment: &str,
) -> Option<&'a kn_common::project::ProjectVerifyEnvironment> {
    config.environments.get(environment)
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

fn preview_stage(
    stage: StageName,
    resolved: Option<&StagePlan>,
    manual_command: Option<&ProjectVerifyCommand>,
    command_source: &str,
) -> serde_json::Value {
    if command_source == "manual" {
        let Some(command) = manual_command else {
            return json!({
                "available": false,
                "enabled": false,
                "source": "manual",
                "timeoutSeconds": stage.default_timeout_secs(),
                "message": if stage == StageName::Build { "未配置构建命令" } else { "未配置测试命令" }
            });
        };
        if !command.enabled {
            return json!({
                "available": true,
                "enabled": false,
                "command": command.command,
                "timeoutSeconds": clamp_timeout(command.timeout_seconds, stage.default_timeout_secs()),
                "source": "manual",
                "message": if stage == StageName::Build { "构建已在桌面端配置为禁用" } else { "测试已在桌面端配置为禁用" }
            });
        }
        return match parse_manual_command(&command.command) {
            Ok(_) => json!({
                "available": true,
                "enabled": true,
                "command": command.command,
                "timeoutSeconds": clamp_timeout(command.timeout_seconds, stage.default_timeout_secs()),
                "source": "manual"
            }),
            Err(err) => json!({
                "available": false,
                "enabled": true,
                "command": command.command,
                "timeoutSeconds": clamp_timeout(command.timeout_seconds, stage.default_timeout_secs()),
                "source": "manual",
                "message": format!("命令配置不可用：{err}")
            }),
        };
    }

    let Some(stage_plan) = resolved else {
        return json!({
            "available": false,
            "enabled": false,
            "source": "auto",
            "timeoutSeconds": stage.default_timeout_secs(),
            "message": if stage == StageName::Build { "未自动识别到构建命令" } else { "未自动识别到测试命令" }
        });
    };

    json!({
        "available": true,
        "enabled": true,
        "command": display_commands(&stage_plan.commands),
        "timeoutSeconds": stage_plan
            .commands
            .iter()
            .map(|command| command.timeout_secs)
            .max()
            .unwrap_or_else(|| stage.default_timeout_secs()),
        "source": "auto"
    })
}

fn load_project_verify_config(repo_root: &Path) -> Option<ProjectVerifyConfig> {
    let path = kn_common::path::config_dir().join("projects.json");
    let text = std::fs::read_to_string(path).ok()?;
    let projects: Vec<ProjectInfo> = serde_json::from_str(&text).ok()?;
    find_project_verify_config(projects, repo_root)
}

fn find_project_verify_config(
    projects: Vec<ProjectInfo>,
    context_root: &Path,
) -> Option<ProjectVerifyConfig> {
    let context = canonical_path(context_root);
    projects
        .into_iter()
        .filter_map(|project| {
            let verify = project.verify?;
            let project_path = PathBuf::from(project.path);
            let canonical = canonical_path(&project_path);
            if context == canonical || context.starts_with(&canonical) {
                Some((canonical.components().count(), verify))
            } else {
                None
            }
        })
        .max_by_key(|(depth, _)| *depth)
        .map(|(_, verify)| verify)
}

fn canonical_path(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

fn nearest_project_root(cwd: &str, repo_root: &Path) -> PathBuf {
    let repo = canonical_path(repo_root);
    let mut current = canonical_path(Path::new(cwd));
    if !current.starts_with(&repo) {
        return repo;
    }
    loop {
        if has_project_marker(&current) {
            return current;
        }
        if current == repo {
            return repo;
        }
        if !current.pop() {
            return repo;
        }
    }
}

fn has_project_marker(path: &Path) -> bool {
    [
        "package.json",
        "pom.xml",
        "build.gradle",
        "build.gradle.kts",
        "go.mod",
        "Cargo.toml",
        "pyproject.toml",
        "pytest.ini",
        "requirements.txt",
    ]
    .iter()
    .any(|marker| path.join(marker).exists())
}

fn detected_languages(repo_root: &Path) -> Vec<String> {
    let mut languages = Vec::new();
    if repo_root.join("package.json").exists() {
        languages.push("node".to_string());
    }
    if repo_root.join("pom.xml").exists()
        || repo_root.join("build.gradle").exists()
        || repo_root.join("build.gradle.kts").exists()
    {
        languages.push("java".to_string());
    }
    if repo_root.join("go.mod").exists() {
        languages.push("go".to_string());
    }
    if repo_root.join("Cargo.toml").exists() {
        languages.push("rust".to_string());
    }
    if repo_root.join("pyproject.toml").exists()
        || repo_root.join("pytest.ini").exists()
        || repo_root.join("requirements.txt").exists()
    {
        languages.push("python".to_string());
    }
    languages
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
    run_log: Option<Arc<Mutex<VerifyRunLog>>>,
) -> serde_json::Value {
    let started = Instant::now();
    let mut output = OutputTailBuffer::new();
    let mut progress_output = ProgressOutputBuffer::new();
    let command_display = display_commands(&plan.commands);
    reporter.send("stageStarted", Some(name), &command_display, "");
    for command in &plan.commands {
        match run_command(
            repo_root,
            command,
            cancel,
            reporter,
            name,
            &mut progress_output,
            run_log.as_ref(),
        )
        .await
        {
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
                reporter.send("stageFinished", Some(name), &command_display, "");
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
                reporter.send("stageFinished", Some(name), &command_display, "");
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
                reporter.send("stageFinished", Some(name), &command_display, "");
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
                reporter.send("stageFinished", Some(name), &command_display, "");
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
    reporter.send("stageFinished", Some(name), &command_display, "");
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
    progress_output: &mut ProgressOutputBuffer,
    run_log: Option<&Arc<Mutex<VerifyRunLog>>>,
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
            let message = if err.kind() == ErrorKind::NotFound {
                format!(
                    "无法找到可执行命令 `{}`。Agent PATH={}",
                    program, execution_path
                )
            } else {
                format!("无法执行命令 `{}`: {}", spec.display, err)
            };
            progress_output.push_str(&message);
            append_run_log(run_log, stage, &spec.display, &message);
            emit_progress_output(reporter, stage, &spec.display, progress_output);
            return CommandOutcome::Io {
                output_tail: message,
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
                drain_output(&mut output_rx, &mut output, progress_output, run_log, stage, &spec.display).await;
                output.push_str("\n验证已取消");
                progress_output.push_str("\n验证已取消");
                append_run_log(run_log, stage, &spec.display, "\n验证已取消");
                emit_progress_output(reporter, stage, &spec.display, progress_output);
                return CommandOutcome::Cancelled {
                    output_tail: output.into_string(),
                };
            }
            _ = &mut deadline => {
                let _ = child.kill().await;
                let _ = child.wait().await;
                drain_output(&mut output_rx, &mut output, progress_output, run_log, stage, &spec.display).await;
                let timeout_message = format!("\n命令超时：{}", spec.display);
                output.push_str(&timeout_message);
                progress_output.push_str(&timeout_message);
                append_run_log(run_log, stage, &spec.display, &timeout_message);
                emit_progress_output(reporter, stage, &spec.display, progress_output);
                return CommandOutcome::Timeout {
                    output_tail: output.into_string(),
                };
            }
            maybe_chunk = output_rx.recv(), if !output_closed => {
                if let Some(chunk) = maybe_chunk {
                    let text = String::from_utf8_lossy(&chunk);
                    output.push_str(&text);
                    progress_output.push_str(&text);
                    append_run_log(run_log, stage, &spec.display, &text);
                    pending_bytes += chunk.len();
                    if pending_bytes >= PROGRESS_OUTPUT_BYTES || last_emit.elapsed() >= PROGRESS_INTERVAL {
                        emit_progress_output(reporter, stage, &spec.display, progress_output);
                        pending_bytes = 0;
                        last_emit = Instant::now();
                    }
                } else {
                    output_closed = true;
                }
            }
            _ = progress_tick.tick() => {
                if pending_bytes > 0 {
                    emit_progress_output(reporter, stage, &spec.display, progress_output);
                    pending_bytes = 0;
                    last_emit = Instant::now();
                }
            }
            status = child.wait() => {
                drain_output(&mut output_rx, &mut output, progress_output, run_log, stage, &spec.display).await;
                if pending_bytes > 0 || started.elapsed() >= PROGRESS_INTERVAL || progress_output.has_pending() {
                    emit_progress_output(reporter, stage, &spec.display, progress_output);
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

async fn drain_output(
    output_rx: &mut mpsc::Receiver<Vec<u8>>,
    output: &mut OutputTailBuffer,
    progress_output: &mut ProgressOutputBuffer,
    run_log: Option<&Arc<Mutex<VerifyRunLog>>>,
    stage: StageName,
    command: &str,
) {
    while let Some(chunk) = output_rx.recv().await {
        let text = String::from_utf8_lossy(&chunk);
        output.push_str(&text);
        progress_output.push_str(&text);
        append_run_log(run_log, stage, command, &text);
    }
}

fn append_run_log(
    run_log: Option<&Arc<Mutex<VerifyRunLog>>>,
    stage: StageName,
    command: &str,
    text: &str,
) {
    let Some(run_log) = run_log else {
        return;
    };
    let mut guard = run_log.lock().unwrap_or_else(|e| e.into_inner());
    guard.append(stage, command, text);
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

fn emit_progress_output(
    reporter: &ProgressReporter,
    stage: StageName,
    command: &str,
    output: &mut ProgressOutputBuffer,
) {
    if !output.has_pending() {
        return;
    }
    let chunk = output.take_pending_chunk();
    reporter.send_chunk("stageOutput", stage, command, &chunk);
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

struct ProgressOutputChunk {
    text: String,
    start_line: u64,
    end_line: u64,
    truncated: bool,
}

struct ProgressOutputBuffer {
    pending_lines: BTreeMap<u64, String>,
    current_line: u64,
    current_line_text: String,
    pending_truncated: bool,
}

impl ProgressOutputBuffer {
    fn new() -> Self {
        Self {
            pending_lines: BTreeMap::new(),
            current_line: 1,
            current_line_text: String::new(),
            pending_truncated: false,
        }
    }

    fn push_str(&mut self, text: &str) {
        for ch in text.chars() {
            self.current_line_text.push(ch);
            if trim_to_last_bytes(&mut self.current_line_text, PROGRESS_OUTPUT_BYTES) {
                self.pending_truncated = true;
            }
            if ch == '\n' {
                self.pending_lines
                    .insert(self.current_line, self.current_line_text.clone());
                self.current_line += 1;
                self.current_line_text.clear();
            }
        }
        if !self.current_line_text.is_empty() {
            self.pending_lines
                .insert(self.current_line, self.current_line_text.clone());
        }
    }

    fn has_pending(&self) -> bool {
        !self.pending_lines.is_empty()
    }

    fn take_pending_chunk(&mut self) -> ProgressOutputChunk {
        if self.pending_lines.is_empty() {
            return ProgressOutputChunk {
                text: String::new(),
                start_line: 0,
                end_line: 0,
                truncated: false,
            };
        }

        let mut selected = Vec::new();
        let mut bytes = 0usize;
        let mut truncated = self.pending_truncated;
        for (line, text) in self.pending_lines.iter().rev() {
            let would_exceed = bytes + text.len() > PROGRESS_OUTPUT_BYTES;
            if !selected.is_empty() && (would_exceed || selected.len() >= MAX_OUTPUT_LINES) {
                truncated = true;
                break;
            }
            selected.push((*line, text.clone()));
            bytes += text.len();
            if would_exceed {
                truncated = true;
                break;
            }
        }
        selected.reverse();

        let start_line = selected.first().map(|(line, _)| *line).unwrap_or(0);
        let end_line = selected.last().map(|(line, _)| *line).unwrap_or(0);
        let mut text = selected
            .into_iter()
            .map(|(_, text)| text)
            .collect::<String>();
        if text.len() > PROGRESS_OUTPUT_BYTES {
            let mut start = text.len() - PROGRESS_OUTPUT_BYTES;
            while !text.is_char_boundary(start) {
                start += 1;
            }
            text = text[start..].to_string();
            truncated = true;
        }

        self.pending_lines.clear();
        self.pending_truncated = false;
        ProgressOutputChunk {
            text,
            start_line,
            end_line,
            truncated,
        }
    }
}

fn trim_to_last_bytes(text: &mut String, max_bytes: usize) -> bool {
    if text.len() <= max_bytes {
        return false;
    }
    let mut start = text.len() - max_bytes;
    while !text.is_char_boundary(start) {
        start += 1;
    }
    text.drain(..start);
    true
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

fn unix_millis() -> u128 {
    unix_millis_at(SystemTime::now()) as u128
}

fn unix_millis_at(time: SystemTime) -> u64 {
    time.duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn verify_runs_dir() -> PathBuf {
    kn_common::path::config_dir().join("verify-runs")
}

fn write_verify_run_meta(
    dir: &Path,
    session_id: &str,
    run_id: &str,
    created_at: u64,
    finished_at: Option<u64>,
) -> std::io::Result<()> {
    fs::create_dir_all(dir)?;
    let destination = dir.join("meta.json");
    let temporary = dir.join(format!(".meta-{}.tmp", std::process::id()));
    let meta = VerifyRunLogMeta {
        session_id: session_id.to_string(),
        run_id: run_id.to_string(),
        created_at,
        finished_at,
    };
    let bytes = serde_json::to_vec(&meta)
        .map_err(|error| std::io::Error::new(ErrorKind::InvalidData, error))?;
    let result = (|| {
        let mut file = OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(&temporary)?;
        file.write_all(&bytes)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&temporary, &destination)?;
        File::open(dir)?.sync_all()?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

fn read_verify_run_meta(dir: &Path) -> Option<VerifyRunLogMeta> {
    serde_json::from_slice(&fs::read(dir.join("meta.json")).ok()?).ok()
}

fn system_time_from_millis(value: u64) -> Option<SystemTime> {
    UNIX_EPOCH.checked_add(Duration::from_millis(value))
}

fn is_verify_log_available_at(dir: &Path, now: SystemTime) -> bool {
    let Some(meta) = read_verify_run_meta(dir) else {
        return false;
    };
    let Some(finished_at) = meta.finished_at.and_then(system_time_from_millis) else {
        return false;
    };
    if now.duration_since(finished_at).map_or(true, |age| {
        age > Duration::from_secs(VERIFY_RUN_LOG_TTL_SECS)
    }) {
        return false;
    }
    [StageName::Build, StageName::Test]
        .into_iter()
        .any(|stage| {
            let path = dir.join(format!("{}.log", stage.as_str()));
            fs::metadata(&path)
                .ok()
                .filter(|metadata| metadata.is_file() && metadata.len() <= VERIFY_RUN_LOG_MAX_BYTES)
                .is_some_and(|_| File::open(path).is_ok())
        })
}

fn ensure_verify_run_disk_cleanup_once() {
    VERIFY_RUN_DISK_CLEANUP.get_or_init(cleanup_expired_verify_run_dirs);
}

fn cleanup_expired_verify_run_dirs() {
    let dir = verify_runs_dir();
    let Some(cutoff) = SystemTime::now().checked_sub(Duration::from_secs(VERIFY_RUN_LOG_TTL_SECS))
    else {
        return;
    };
    cleanup_expired_verify_run_dirs_in(&dir, &active_verify_run_dirs(), cutoff);
}

fn active_verify_run_dirs() -> Vec<PathBuf> {
    VERIFY_RUN_LOGS
        .get()
        .map(|logs| {
            logs.lock()
                .unwrap_or_else(|e| e.into_inner())
                .values()
                .map(|log| log.lock().unwrap_or_else(|e| e.into_inner()).dir.clone())
                .collect()
        })
        .unwrap_or_default()
}

fn cleanup_expired_verify_run_dirs_in(dir: &Path, active_dirs: &[PathBuf], cutoff: SystemTime) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        if active_dirs.iter().any(|active| active == &path) {
            continue;
        }
        let completed_at = read_verify_run_meta(&path)
            .and_then(|meta| meta.finished_at)
            .and_then(system_time_from_millis)
            .or_else(|| {
                entry
                    .metadata()
                    .and_then(|metadata| metadata.modified())
                    .ok()
            });
        if completed_at.is_some_and(|completed_at| completed_at < cutoff) {
            let _ = std::fs::remove_dir_all(path);
        }
    }
}

fn utf8_boundary_len(text: &str, max_bytes: usize) -> usize {
    let mut len = max_bytes.min(text.len());
    while len > 0 && !text.is_char_boundary(len) {
        len -= 1;
    }
    len
}

fn register_run_log(log: Arc<Mutex<VerifyRunLog>>) {
    let run_id = log.lock().unwrap_or_else(|e| e.into_inner()).run_id.clone();
    let map = VERIFY_RUN_LOGS.get_or_init(|| Mutex::new(HashMap::new()));
    map.lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert(run_id, log);
}

fn get_run_log(session_id: &str, run_id: &str) -> Option<Arc<Mutex<VerifyRunLog>>> {
    ensure_verify_run_disk_cleanup_once();
    cleanup_expired_run_logs();
    let map = VERIFY_RUN_LOGS.get_or_init(|| Mutex::new(HashMap::new()));
    let existing = map
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(run_id)
        .cloned();
    let log = if let Some(log) = existing {
        log
    } else {
        let restored = is_safe_verify_run_id(run_id)
            .then(|| {
                VerifyRunLog::restore_from_dir(
                    &verify_runs_dir().join(run_id),
                    session_id,
                    run_id,
                    SystemTime::now(),
                )
            })
            .flatten()?;
        let log = Arc::new(Mutex::new(restored));
        map.lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(run_id.to_string(), log.clone());
        log
    };
    let guard = log.lock().unwrap_or_else(|e| e.into_inner());
    if guard.session_id == session_id && !guard.is_expired() {
        drop(guard);
        Some(log)
    } else {
        None
    }
}

fn is_safe_verify_run_id(run_id: &str) -> bool {
    !run_id.is_empty()
        && run_id.len() <= 128
        && run_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
}

fn cleanup_expired_run_logs() {
    let map = VERIFY_RUN_LOGS.get_or_init(|| Mutex::new(HashMap::new()));
    let mut guard = map.lock().unwrap_or_else(|e| e.into_inner());
    let expired = guard
        .iter()
        .filter_map(|(run_id, log)| {
            let log_guard = log.lock().unwrap_or_else(|e| e.into_inner());
            log_guard
                .is_expired()
                .then(|| (run_id.clone(), log_guard.dir.clone()))
        })
        .collect::<Vec<_>>();
    for (run_id, dir) in expired {
        guard.remove(&run_id);
        let _ = std::fs::remove_dir_all(dir);
    }
}

pub fn parse_stage_name(value: &str) -> Option<StageName> {
    match value {
        "build" => Some(StageName::Build),
        "test" => Some(StageName::Test),
        _ => None,
    }
}

fn verify_log_window_error(
    session_id: &str,
    run_id: &str,
    stage: StageName,
    status: &str,
    message: &str,
) -> serde_json::Value {
    json!({
        "sessionId": session_id,
        "runId": run_id,
        "stage": stage.as_str(),
        "status": status,
        "startLine": 0,
        "endLine": 0,
        "centerLine": 0,
        "lines": [],
        "hasEarlier": false,
        "hasLater": false,
        "contentTruncated": false,
        "message": message,
    })
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

        let plan = manual_plan_from_config_with_environment(&config, "default")
            .unwrap()
            .1;

        assert_eq!(plan.command_source, "manual");
        assert_eq!(
            plan.build.unwrap().commands[0].display,
            "mvn -q -DskipTests compile"
        );
        assert!(plan.test.is_none());
    }

    #[test]
    fn manual_config_falls_back_to_default_environment() {
        let mut environments = BTreeMap::new();
        environments.insert(
            "ci".to_string(),
            kn_common::project::ProjectVerifyEnvironment {
                build: Some(ProjectVerifyCommand {
                    command: "cargo check".to_string(),
                    enabled: true,
                    timeout_seconds: Some(300),
                }),
                test: None,
            },
        );
        let config = ProjectVerifyConfig {
            default_environment: Some("ci".to_string()),
            environments,
        };

        let (environment, plan) =
            manual_plan_from_config_with_environment(&config, "missing").unwrap();

        assert_eq!(environment, "ci");
        assert_eq!(plan.command_source, "manual");
        assert_eq!(plan.build.unwrap().commands[0].display, "cargo check");
    }

    #[test]
    fn manual_config_falls_back_to_default_when_default_environment_is_missing() {
        let mut environments = BTreeMap::new();
        environments.insert(
            "default".to_string(),
            kn_common::project::ProjectVerifyEnvironment {
                build: Some(ProjectVerifyCommand {
                    command: "mvn -q -DskipTests compile".to_string(),
                    enabled: true,
                    timeout_seconds: None,
                }),
                test: None,
            },
        );
        let config = ProjectVerifyConfig {
            default_environment: Some("missing".to_string()),
            environments,
        };

        let (environment, plan) = manual_plan_from_config_with_environment(&config, "ci").unwrap();

        assert_eq!(environment, "default");
        assert_eq!(
            plan.build.unwrap().commands[0].display,
            "mvn -q -DskipTests compile"
        );
    }

    #[test]
    fn project_verify_config_uses_longest_ancestor_and_never_child_for_parent() {
        let dir = unique_temp_dir("project-config-match");
        let repo = dir.join("repo");
        let java = repo.join("java");
        let agent = repo.join("agent");
        fs::create_dir_all(&java).unwrap();
        fs::create_dir_all(&agent).unwrap();

        let parent_config = verify_config("cargo check", "cargo test");
        let java_config = verify_config("mvn -q -DskipTests compile", "mvn -q test");
        let projects = vec![
            project_info("repo", &repo, Some(parent_config.clone())),
            project_info("java", &java, Some(java_config.clone())),
        ];

        let parent_match = find_project_verify_config(projects.clone(), &repo).unwrap();
        assert_eq!(parent_match, parent_config);

        let child_match = find_project_verify_config(projects.clone(), &java).unwrap();
        assert_eq!(child_match, java_config);

        let sibling_match = find_project_verify_config(projects, &agent).unwrap();
        assert_eq!(sibling_match, parent_config);

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn nearest_project_root_prefers_cwd_project_inside_monorepo() {
        let dir = unique_temp_dir("nearest-project-root");
        let repo = dir.join("repo");
        let site = repo.join("site");
        let nested = site.join("src");
        fs::create_dir_all(&nested).unwrap();
        fs::write(repo.join("Cargo.toml"), "[workspace]\nmembers=[]\n").unwrap();
        fs::write(
            site.join("package.json"),
            r#"{"scripts":{"build":"vite build"}}"#,
        )
        .unwrap();

        let root = nearest_project_root(&nested.to_string_lossy(), &repo);
        assert_eq!(root, site.canonicalize().unwrap());
        let plan = auto_plan(&root);

        assert_eq!(plan.build.unwrap().commands[0].display, "npm run build");

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn preview_stage_reports_disabled_and_invalid_manual_commands() {
        let disabled = ProjectVerifyCommand {
            command: "cargo test".to_string(),
            enabled: false,
            timeout_seconds: None,
        };
        let disabled_preview = preview_stage(StageName::Test, None, Some(&disabled), "manual");

        assert_eq!(disabled_preview["available"], true);
        assert_eq!(disabled_preview["enabled"], false);
        assert_eq!(disabled_preview["message"], "测试已在桌面端配置为禁用");

        let invalid = ProjectVerifyCommand {
            command: "cargo test && rm -rf target".to_string(),
            enabled: true,
            timeout_seconds: Some(600),
        };
        let invalid_preview = preview_stage(StageName::Test, None, Some(&invalid), "manual");

        assert_eq!(invalid_preview["available"], false);
        assert_eq!(invalid_preview["enabled"], true);
        assert!(invalid_preview["message"]
            .as_str()
            .unwrap()
            .contains("命令配置不可用"));
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
    fn disk_cleanup_keeps_active_verify_run_dirs() {
        let dir = unique_temp_dir("verify-run-cleanup");
        let active = dir.join("active-run");
        let inactive = dir.join("inactive-run");
        fs::create_dir_all(&active).unwrap();
        fs::create_dir_all(&inactive).unwrap();

        cleanup_expired_verify_run_dirs_in(
            &dir,
            std::slice::from_ref(&active),
            SystemTime::now() + Duration::from_secs(1),
        );

        assert!(active.exists());
        assert!(!inactive.exists());
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn restores_finished_verify_log_from_disk_after_restart() {
        let dir = unique_temp_dir("verify-log-restore");
        fs::create_dir_all(&dir).unwrap();
        let now = SystemTime::now();
        write_verify_run_meta(
            &dir,
            "17:/repo",
            "v_restore",
            unix_millis_at(now),
            Some(unix_millis_at(now)),
        )
        .unwrap();
        fs::write(
            dir.join("build.log"),
            "first line\nerror: restored failure\n",
        )
        .unwrap();

        let log = VerifyRunLog::restore_from_dir(&dir, "17:/repo", "v_restore", now)
            .expect("finished disk log should be restored");
        let window = log.window(StageName::Build, 2, 1, 1);

        assert_eq!(window["status"], "ok");
        assert_eq!(window["lines"][1]["text"], "error: restored failure");
        assert_eq!(log.issue_scan_targets(&[StageName::Build]).len(), 1);
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn log_window_keeps_the_requested_center_line_when_earlier_lines_exhaust_the_budget() {
        let dir = unique_temp_dir("verify-log-window-center");
        fs::create_dir_all(&dir).unwrap();
        let mut stage = StageLogFile::create(StageName::Build, "test", &dir);
        for line in 1..=201 {
            stage.append(&format!("{line}:{}\n", "x".repeat(4 * 1024)));
        }

        let window = stage.window("17:/repo", "v_center", 101, 100, 100);

        assert!(window["contentTruncated"].as_bool().unwrap_or(false));
        assert!(window["lines"].as_array().is_some_and(|lines| lines
            .iter()
            .any(|line| line["lineNumber"] == 101)));
        assert!(serde_json::to_vec(&window).unwrap().len() <= 128 * 1024);
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn log_availability_uses_finished_time_not_creation_time() {
        let dir = unique_temp_dir("verify-log-finished-ttl");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("build.log"), "ok\n").unwrap();
        let now = SystemTime::now();
        let old = now - Duration::from_secs(VERIFY_RUN_LOG_TTL_SECS + 60);

        write_verify_run_meta(
            &dir,
            "17:/repo",
            "v_ttl",
            unix_millis_at(old),
            Some(unix_millis_at(now)),
        )
        .unwrap();
        assert!(is_verify_log_available_at(&dir, now));

        write_verify_run_meta(
            &dir,
            "17:/repo",
            "v_ttl",
            unix_millis_at(old),
            Some(unix_millis_at(old)),
        )
        .unwrap();
        assert!(!is_verify_log_available_at(&dir, now));
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn progress_output_buffer_emits_positioned_incremental_lines() {
        let mut buffer = ProgressOutputBuffer::new();
        buffer.push_str("line 1\nline 2\n");
        let first = buffer.take_pending_chunk();

        assert_eq!(first.text, "line 1\nline 2\n");
        assert_eq!(first.start_line, 1);
        assert_eq!(first.end_line, 2);
        assert!(!first.truncated);

        buffer.push_str("line 3\nline 4\n");
        let second = buffer.take_pending_chunk();

        assert_eq!(second.text, "line 3\nline 4\n");
        assert_eq!(second.start_line, 3);
        assert_eq!(second.end_line, 4);
        assert!(!second.truncated);
    }

    #[test]
    fn progress_output_buffer_replaces_growing_unfinished_line() {
        let mut buffer = ProgressOutputBuffer::new();
        buffer.push_str("compiling");
        let first = buffer.take_pending_chunk();

        assert_eq!(first.text, "compiling");
        assert_eq!(first.start_line, 1);
        assert_eq!(first.end_line, 1);

        buffer.push_str(" crate");
        let second = buffer.take_pending_chunk();

        assert_eq!(second.text, "compiling crate");
        assert_eq!(second.start_line, 1);
        assert_eq!(second.end_line, 1);
    }

    #[test]
    fn progress_output_buffer_marks_large_chunk_truncated() {
        let mut buffer = ProgressOutputBuffer::new();
        for index in 0..500 {
            buffer.push_str(&format!("line-{index:03}-{}\n", "x".repeat(512)));
        }
        let chunk = buffer.take_pending_chunk();

        assert!(chunk.truncated);
        assert!(chunk.start_line > 1);
        assert_eq!(chunk.end_line, 500);
        assert!(chunk.text.contains("line-499"));
        assert!(!chunk.text.contains("line-000"));
    }

    #[test]
    fn progress_output_buffer_bounds_unfinished_long_line() {
        let mut buffer = ProgressOutputBuffer::new();
        buffer.push_str(&"x".repeat(PROGRESS_OUTPUT_BYTES * 3));

        assert!(buffer.current_line_text.len() <= PROGRESS_OUTPUT_BYTES);
        assert!(buffer
            .pending_lines
            .get(&1)
            .map(|line| line.len() <= PROGRESS_OUTPUT_BYTES)
            .unwrap_or(false));
        let chunk = buffer.take_pending_chunk();

        assert!(chunk.truncated);
        assert_eq!(chunk.start_line, 1);
        assert_eq!(chunk.end_line, 1);
        assert!(chunk.text.len() <= PROGRESS_OUTPUT_BYTES);

        buffer.push_str("done");
        let updated = buffer.take_pending_chunk();

        assert!(updated.truncated);
        assert_eq!(updated.start_line, 1);
        assert_eq!(updated.end_line, 1);
        assert!(updated.text.ends_with("done"));
        assert!(updated.text.len() <= PROGRESS_OUTPUT_BYTES);
    }

    #[tokio::test]
    async fn run_command_flushes_short_output_before_stage_finished() {
        let dir = unique_temp_dir("short-output-progress");
        fs::create_dir_all(&dir).unwrap();
        let (tx, mut rx) = mpsc::unbounded_channel();
        let reporter = ProgressReporter::new_project(
            "42:/repo",
            42,
            "/repo",
            "v_1",
            "default",
            VerifyTarget::Build,
            "auto",
            Some(tx),
            None,
        );
        let spec = cmd(&["/bin/sh", "-c", "printf short-output"], 5);
        let mut progress_output = ProgressOutputBuffer::new();

        let outcome = run_command(
            &dir,
            &spec,
            &CancellationToken::new(),
            &reporter,
            StageName::Build,
            &mut progress_output,
            None,
        )
        .await;

        match outcome {
            CommandOutcome::Passed { output_tail } => assert_eq!(output_tail, "short-output"),
            _ => panic!("expected command to pass"),
        }
        let mut saw_output = false;
        while let Ok(message) = rx.try_recv() {
            if message.contains("\"type\":\"project_verify_changes_progress\"")
                && message.contains("\"status\":\"stageOutput\"")
                && message.contains("short-output")
                && message.contains("\"outputStartLine\":1")
                && message.contains("\"outputEndLine\":1")
            {
                saw_output = true;
                break;
            }
        }

        assert!(saw_output);
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn invalid_target_result_does_not_echo_invalid_target_to_ios() {
        let result = invalid_target_result("s_1", "default");

        assert_eq!(result["status"], "error");
        assert_eq!(result["target"], "all");
        assert_eq!(result["message"], "验证目标不支持");
    }

    #[test]
    fn cancelling_run_retains_original_request_id() {
        let session_id = "cancel-request-id-test";
        let run_id = "v_cancel_request_id";
        let running = RunningGuard::try_acquire(
            session_id,
            run_id,
            "default",
            VerifyTarget::Build,
            Some("verify-request-1"),
        )
        .expect("test run should acquire its isolated session");

        let (_, _, _, _, request_id) =
            cancel(session_id, run_id).expect("run should be cancellable");

        assert_eq!(request_id.as_deref(), Some("verify-request-1"));
        drop(running);
    }

    fn project_info(name: &str, path: &Path, verify: Option<ProjectVerifyConfig>) -> ProjectInfo {
        ProjectInfo {
            name: name.to_string(),
            path: path.to_string_lossy().to_string(),
            default_profile: None,
            description: None,
            pinned: false,
            verify,
        }
    }

    fn verify_config(build: &str, test: &str) -> ProjectVerifyConfig {
        let mut environments = BTreeMap::new();
        environments.insert(
            "default".to_string(),
            kn_common::project::ProjectVerifyEnvironment {
                build: Some(ProjectVerifyCommand {
                    command: build.to_string(),
                    enabled: true,
                    timeout_seconds: None,
                }),
                test: Some(ProjectVerifyCommand {
                    command: test.to_string(),
                    enabled: true,
                    timeout_seconds: None,
                }),
            },
        );
        ProjectVerifyConfig {
            default_environment: Some("default".to_string()),
            environments,
        }
    }

    fn unique_temp_dir(label: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("kn-verify-{label}-{nanos}"))
    }
}
