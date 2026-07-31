//! Tool-aware, per-command terminal output parsing.
//!
//! This module intentionally keeps command outcome separate from diagnostic
//! extraction: arbitrary output text never turns a successful process into a
//! failure.  A tool-specific final summary may do so only when it contains a
//! parsed, non-zero failure counter.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ParseStatus {
    Success,
    Failed,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ParseReason {
    None,
    ExitNonZero,
    SummaryFailure,
    Timeout,
    Cancelled,
    LaunchFailed,
    LostExitStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TaskType {
    Compile,
    Test,
    Package,
    Build,
    Run,
    Lint,
    Custom,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandContext {
    pub argv: Vec<String>,
}

impl CommandContext {
    pub fn new(argv: Vec<String>) -> Self {
        Self { argv }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalParseError {
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub column: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub test_name: Option<String>,
    pub start_line: usize,
    pub end_line: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalParseResult {
    pub status: ParseStatus,
    pub reason: ParseReason,
    pub task_type: TaskType,
    pub parser: String,
    pub exit_code: Option<i32>,
    pub summary: String,
    pub errors: Vec<TerminalParseError>,
}

pub struct TerminalOutputParser {
    parser: ParserKind,
    task_type: TaskType,
    summary_failure: bool,
    summary: Option<String>,
    errors: Vec<TerminalParseError>,
    emitted_error_count: usize,
    line_number: usize,
    pending_stdout: Vec<u8>,
    pending_stderr: Vec<u8>,
}

const MAX_PENDING_LINE_BYTES: usize = 16 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputStream {
    Stdout,
    Stderr,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ParserKind {
    Generic,
    Maven,
    Gradle,
    AndroidGradle,
    TypeScript,
    Pytest,
    Go,
    Cargo,
    Xcodebuild,
    PythonUnittest,
    Dotnet,
}

impl ParserKind {
    fn id(self) -> &'static str {
        match self {
            Self::Generic => "generic-exit",
            Self::Maven => "maven",
            Self::Gradle => "gradle",
            Self::AndroidGradle => "android-gradle",
            Self::TypeScript => "typescript",
            Self::Pytest => "pytest",
            Self::Go => "go",
            Self::Cargo => "cargo",
            Self::Xcodebuild => "xcodebuild",
            Self::PythonUnittest => "python-unittest",
            Self::Dotnet => "dotnet",
        }
    }
}

impl TerminalOutputParser {
    pub fn new(context: CommandContext) -> Self {
        let (parser, task_type) = identify(&context.argv);
        Self {
            parser,
            task_type,
            summary_failure: false,
            summary: None,
            errors: Vec::new(),
            emitted_error_count: 0,
            line_number: 0,
            pending_stdout: Vec::new(),
            pending_stderr: Vec::new(),
        }
    }

    /// Consumes process output without corrupting a UTF-8 character split
    /// across two reads. Only complete newline-delimited records are parsed
    /// while the command is running.
    pub fn on_bytes(&mut self, bytes: &[u8]) {
        self.on_bytes_from(OutputStream::Stdout, bytes);
    }

    pub fn on_bytes_from(&mut self, stream: OutputStream, bytes: &[u8]) {
        let pending = match stream {
            OutputStream::Stdout => &mut self.pending_stdout,
            OutputStream::Stderr => &mut self.pending_stderr,
        };
        pending.extend_from_slice(bytes);
        let lines = complete_lines(pending);
        for line in lines {
            let text = String::from_utf8_lossy(&line);
            self.on_line(text.trim_end_matches(['\r', '\n']));
        }
    }

    pub fn on_line(&mut self, line: &str) {
        self.line_number += 1;
        let line = strip_ansi(line);
        match self.parser {
            ParserKind::Maven => self.parse_maven(&line),
            ParserKind::Gradle | ParserKind::AndroidGradle => self.parse_gradle(&line),
            ParserKind::Pytest => self.parse_pytest(&line),
            ParserKind::Cargo => self.parse_cargo(&line),
            ParserKind::TypeScript => self.parse_typescript(&line),
            ParserKind::Go => self.parse_go(&line),
            ParserKind::Xcodebuild => self.parse_xcodebuild(&line),
            ParserKind::PythonUnittest => self.parse_python_unittest(&line),
            ParserKind::Dotnet => self.parse_dotnet(&line),
            _ => {}
        }
    }

    /// Returns diagnostics discovered since the previous call. Candidates are
    /// advisory while a command is running; the final result remains the
    /// authoritative status and deduplicated error set.
    pub fn take_candidates(&mut self) -> Vec<TerminalParseError> {
        let candidates = self.errors[self.emitted_error_count..].to_vec();
        self.emitted_error_count = self.errors.len();
        candidates
    }

    pub fn finalize(mut self, exit_code: Option<i32>) -> TerminalParseResult {
        self.flush_pending_line();
        let (status, reason) = match exit_code {
            None => (ParseStatus::Unknown, ParseReason::LostExitStatus),
            Some(code) if code != 0 => (ParseStatus::Failed, ParseReason::ExitNonZero),
            Some(_) if self.summary_failure => (ParseStatus::Failed, ParseReason::SummaryFailure),
            Some(_) => (ParseStatus::Success, ParseReason::None),
        };
        TerminalParseResult {
            status,
            reason,
            task_type: self.task_type,
            parser: self.parser.id().to_string(),
            exit_code,
            summary: self.summary.unwrap_or_default(),
            errors: self.errors,
        }
    }

    pub fn finalize_timeout(mut self) -> TerminalParseResult {
        self.flush_pending_line();
        self.result_with_terminal_reason(ParseStatus::Failed, ParseReason::Timeout)
    }

    pub fn finalize_cancelled(mut self) -> TerminalParseResult {
        self.flush_pending_line();
        self.result_with_terminal_reason(ParseStatus::Unknown, ParseReason::Cancelled)
    }

    pub fn finalize_launch_failed(mut self) -> TerminalParseResult {
        self.flush_pending_line();
        self.result_with_terminal_reason(ParseStatus::Failed, ParseReason::LaunchFailed)
    }

    fn flush_pending_line(&mut self) {
        let stdout = std::mem::take(&mut self.pending_stdout);
        let stderr = std::mem::take(&mut self.pending_stderr);
        for pending in [stdout, stderr] {
            if pending.is_empty() {
                continue;
            }
            self.on_line(String::from_utf8_lossy(&pending).trim_end_matches('\r'));
        }
    }

    fn result_with_terminal_reason(&self, status: ParseStatus, reason: ParseReason) -> TerminalParseResult {
        TerminalParseResult {
            status,
            reason,
            task_type: self.task_type.clone(),
            parser: self.parser.id().to_string(),
            exit_code: None,
            summary: self.summary.clone().unwrap_or_default(),
            errors: self.errors.clone(),
        }
    }

    fn parse_maven(&mut self, line: &str) {
        if line.contains("BUILD FAILURE") || line.contains("COMPILATION ERROR") {
            self.summary_failure = true;
            self.summary = Some(line.to_string());
        } else if line.contains("BUILD SUCCESS") {
            self.summary = Some(line.to_string());
        }
        let failures = counter(line, "Failures:").unwrap_or(0);
        let errors = counter(line, "Errors:").unwrap_or(0);
        if line.contains("Tests run:") && (failures > 0 || errors > 0) {
            self.summary_failure = true;
            self.summary = Some(line.to_string());
        }
        if let Some(error) = maven_diagnostic(line, self.line_number) {
            self.errors.push(error);
        }
    }

    fn parse_gradle(&mut self, line: &str) {
        if line.contains("BUILD FAILED") {
            self.summary_failure = true;
            self.summary = Some(line.to_string());
        } else if line.contains("BUILD SUCCESSFUL") {
            self.summary = Some(line.to_string());
        }
        if let Some(task) = line.strip_prefix("> Task ").and_then(|value| value.strip_suffix(" FAILED")) {
            self.errors.push(TerminalParseError {
                message: format!("Gradle task failed: {task}"),
                file: None,
                line: None,
                column: None,
                code: None,
                test_name: None,
                start_line: self.line_number,
                end_line: self.line_number,
            });
        }
    }

    fn parse_pytest(&mut self, line: &str) {
        if let Some(rest) = line.strip_prefix("FAILED ") {
            let test_name = rest.split(" - ").next().unwrap_or(rest).trim();
            self.errors.push(TerminalParseError {
                message: rest.to_string(),
                file: None,
                line: None,
                column: None,
                code: None,
                test_name: Some(test_name.to_string()),
                start_line: self.line_number,
                end_line: self.line_number,
            });
        }
        let failed = counter(line, "failed").unwrap_or(0);
        let errors = counter(line, "errors").unwrap_or(0);
        if (line.contains(" passed") || line.contains(" failed") || line.contains(" errors"))
            && (failed > 0 || errors > 0)
        {
            self.summary_failure = true;
            self.summary = Some(line.to_string());
        }
    }

    fn parse_cargo(&mut self, line: &str) {
        if line.contains("test result: FAILED") || line.contains("could not compile") {
            self.summary_failure = true;
            self.summary = Some(line.to_string());
        } else if line.contains("test result: ok") {
            self.summary = Some(line.to_string());
        }
        if line.starts_with("error[") || line.starts_with("error:") {
            self.errors.push(TerminalParseError {
                message: line.to_string(),
                file: None,
                line: None,
                column: None,
                code: rust_error_code(line),
                test_name: None,
                start_line: self.line_number,
                end_line: self.line_number,
            });
        }
    }

    fn parse_typescript(&mut self, line: &str) {
        if let Some(code) = typescript_code(line) {
            self.errors.push(TerminalParseError {
                message: line.to_string(),
                file: None,
                line: None,
                column: None,
                code: Some(code),
                test_name: None,
                start_line: self.line_number,
                end_line: self.line_number,
            });
        }
    }

    fn parse_go(&mut self, line: &str) {
        if line == "FAIL" || line.starts_with("FAIL\t") {
            self.summary_failure = true;
            self.summary = Some(line.to_string());
        }
        if let Some(test_name) = line.strip_prefix("--- FAIL: ") {
            self.errors.push(TerminalParseError {
                message: line.to_string(),
                file: None,
                line: None,
                column: None,
                code: None,
                test_name: Some(test_name.split_whitespace().next().unwrap_or(test_name).to_string()),
                start_line: self.line_number,
                end_line: self.line_number,
            });
        }
    }

    fn parse_xcodebuild(&mut self, line: &str) {
        if line.contains("BUILD FAILED") || line.contains("TEST FAILED") {
            self.summary_failure = true;
            self.summary = Some(line.to_string());
        } else if line.contains("BUILD SUCCEEDED") || line.contains("TEST SUCCEEDED") {
            self.summary = Some(line.to_string());
        }
    }

    fn parse_python_unittest(&mut self, line: &str) {
        if line == "OK" {
            self.summary = Some(line.to_string());
        } else if line.starts_with("FAILED (") {
            let failures = counter(line, "failures=").unwrap_or(0);
            let errors = counter(line, "errors=").unwrap_or(0);
            self.summary = Some(line.to_string());
            self.summary_failure = failures > 0 || errors > 0;
        }
    }

    fn parse_dotnet(&mut self, line: &str) {
        if line.starts_with("Passed! -") || line.starts_with("Failed! -") {
            let failed = counter(line, "Failed:").unwrap_or(0);
            self.summary = Some(line.to_string());
            self.summary_failure = failed > 0 || line.starts_with("Failed!");
        } else if line.contains("Build FAILED") {
            self.summary_failure = true;
            self.summary = Some(line.to_string());
        } else if line.contains("Build succeeded") {
            self.summary = Some(line.to_string());
        }
    }
}

fn identify(argv: &[String]) -> (ParserKind, TaskType) {
    let program = argv.first().map(|value| basename(value)).unwrap_or("");
    let arguments = argv.join(" ").to_ascii_lowercase();
    // AGP task semantics are more specific than the generic Gradle profile;
    // preserve Android parser identity regardless of profile refresh timing.
    if matches!(program, "gradle" | "gradlew")
        && (arguments.contains("assemble")
            || arguments.contains("bundle")
            || arguments.contains("connected")
            || arguments.contains("androidtest")
            || arguments.contains("lint")
            || arguments.contains(" debug")
            || arguments.contains(" release"))
    {
        return (ParserKind::AndroidGradle, gradle_task_type(&arguments));
    }
    if let Some(profiles) = crate::session::terminal_profiles::active() {
        let command = argv.join(" ");
        if let Some(profile) = profiles.profiles.iter()
            .filter(|profile| profile.command_matchers.iter().any(|pattern| regex_lite::Regex::new(pattern).is_ok_and(|regex| regex.is_match(&command))))
            .max_by_key(|profile| profile.priority)
        {
            let known = match profile.id.as_str() {
                "maven" => Some(ParserKind::Maven),
                "gradle" => Some(ParserKind::Gradle),
                "android-gradle" => Some(ParserKind::AndroidGradle),
                "typescript" => Some(ParserKind::TypeScript),
                "pytest" => Some(ParserKind::Pytest),
                "go" => Some(ParserKind::Go),
                "cargo" => Some(ParserKind::Cargo),
                "xcodebuild" => Some(ParserKind::Xcodebuild),
                "python-unittest" => Some(ParserKind::PythonUnittest),
                "dotnet" => Some(ParserKind::Dotnet),
                _ => None,
            };
            if let Some(parser) = known {
                return (parser, task_from_words(&command));
            }
        }
    }
    if program == "mvn" || program == "mvnw" {
        return (ParserKind::Maven, maven_task_type(&arguments));
    }
    if program == "gradle" || program == "gradlew" {
        let android = arguments.contains("assemble")
            || arguments.contains("bundle")
            || arguments.contains("connected")
            || arguments.contains("androidtest")
            || arguments.contains("lint")
            || arguments.contains("debug")
            || arguments.contains("release");
        return (
            if android { ParserKind::AndroidGradle } else { ParserKind::Gradle },
            gradle_task_type(&arguments),
        );
    }
    if matches!(program, "tsc" | "vue-tsc" | "ngc") {
        return (ParserKind::TypeScript, TaskType::Compile);
    }
    if program == "pytest" || arguments.contains(" -m pytest") {
        return (ParserKind::Pytest, TaskType::Test);
    }
    if program == "python" && arguments.contains(" -m unittest") {
        return (ParserKind::PythonUnittest, TaskType::Test);
    }
    if program == "dotnet" {
        return (ParserKind::Dotnet, task_from_words(&arguments));
    }
    if program == "go" {
        return (ParserKind::Go, task_from_words(&arguments));
    }
    if program == "cargo" || program == "cargo-nextest" {
        return (ParserKind::Cargo, task_from_words(&arguments));
    }
    if program == "xcodebuild" {
        return (ParserKind::Xcodebuild, task_from_words(&arguments));
    }
    (ParserKind::Generic, TaskType::Custom)
}

fn complete_lines(pending: &mut Vec<u8>) -> Vec<Vec<u8>> {
    let mut lines = Vec::new();
    while let Some(newline) = pending.iter().position(|byte| *byte == b'\n') {
        lines.push(pending.drain(..=newline).collect());
    }
    if pending.len() > MAX_PENDING_LINE_BYTES {
        let mut cutoff = MAX_PENDING_LINE_BYTES;
        while cutoff > 0 && cutoff < pending.len() && (pending[cutoff] & 0b1100_0000) == 0b1000_0000 {
            cutoff -= 1;
        }
        lines.push(pending.drain(..cutoff).collect());
    }
    lines
}

fn basename(value: &str) -> &str {
    value.rsplit('/').next().unwrap_or(value)
}

fn maven_task_type(arguments: &str) -> TaskType {
    if arguments.contains(" test") || arguments.contains("surefire:test") || arguments.contains("failsafe:") {
        TaskType::Test
    } else if arguments.contains(" compile") || arguments.contains("testcompile") {
        TaskType::Compile
    } else if arguments.contains(" package") || arguments.contains(" install") || arguments.contains(" deploy") {
        TaskType::Package
    } else {
        TaskType::Build
    }
}

fn gradle_task_type(arguments: &str) -> TaskType {
    if arguments.contains("test") || arguments.contains("connected") {
        TaskType::Test
    } else if arguments.contains("assemble") || arguments.contains("bundle") {
        TaskType::Package
    } else if arguments.contains("compile") || arguments.contains("classes") {
        TaskType::Compile
    } else if arguments.contains("lint") {
        TaskType::Lint
    } else {
        TaskType::Build
    }
}

fn task_from_words(arguments: &str) -> TaskType {
    if arguments.contains(" test") {
        TaskType::Test
    } else if arguments.contains(" lint") || arguments.contains(" analyze") || arguments.contains("clippy") || arguments.contains(" vet") {
        TaskType::Lint
    } else if arguments.contains(" package") || arguments.contains(" publish") || arguments.contains(" archive") || arguments.contains(" bundle") {
        TaskType::Package
    } else if arguments.contains(" compile") || arguments.contains(" check") || arguments.contains(" build") {
        TaskType::Compile
    } else if arguments.contains(" run") {
        TaskType::Run
    } else {
        TaskType::Build
    }
}

fn counter(line: &str, label: &str) -> Option<u64> {
    let lower = line.to_ascii_lowercase();
    let label = label.to_ascii_lowercase();
    let index = lower.find(&label)? + label.len();
    let digits: String = lower[index..]
        .trim_start()
        .chars()
        .take_while(|character| character.is_ascii_digit())
        .collect();
    (!digits.is_empty()).then(|| digits.parse().ok()).flatten()
}

fn maven_diagnostic(line: &str, line_number: usize) -> Option<TerminalParseError> {
    let rest = line.strip_prefix("[ERROR] ")?;
    let marker = rest.find(":[")?;
    let file = rest[..marker].to_string();
    let location_end = rest[marker + 2..].find(']')? + marker + 2;
    let location = &rest[marker + 2..location_end];
    let mut values = location.split(',');
    let source_line = values.next()?.parse().ok();
    let column = values.next().and_then(|value| value.parse().ok());
    Some(TerminalParseError {
        message: rest[location_end + 1..].trim().to_string(),
        file: Some(file),
        line: source_line,
        column,
        code: None,
        test_name: None,
        start_line: line_number,
        end_line: line_number,
    })
}

fn rust_error_code(line: &str) -> Option<String> {
    let start = line.find("error[")? + "error[".len();
    let end = line[start..].find(']')? + start;
    Some(line[start..end].to_string())
}

fn typescript_code(line: &str) -> Option<String> {
    line.split_whitespace()
        .find(|part| part.len() == 6 && part.starts_with("TS") && part[2..].chars().all(|character| character.is_ascii_digit()))
        .map(str::to_string)
}

fn strip_ansi(line: &str) -> String {
    let mut result = String::with_capacity(line.len());
    let mut characters = line.chars().peekable();
    while let Some(character) = characters.next() {
        if character == '\u{1b}' && characters.peek() == Some(&'[') {
            characters.next();
            while let Some(next) = characters.next() {
                if ('@'..='~').contains(&next) {
                    break;
                }
            }
        } else {
            result.push(character);
        }
    }
    result
}
