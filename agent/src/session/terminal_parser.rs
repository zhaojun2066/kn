//! Tool-aware, per-command terminal output parsing.
//!
//! This module intentionally keeps command outcome separate from diagnostic
//! extraction: arbitrary output text never turns a successful process into a
//! failure.  A tool-specific final summary may do so only when it contains a
//! parsed, non-zero failure counter.

use serde::{Deserialize, Serialize};
use regex_lite::Regex;

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
    /// Directory from which the command is executed.  It is optional so
    /// callers that only have an argv (for example protocol fixtures) retain
    /// the generic resolver behaviour.
    pub working_dir: Option<std::path::PathBuf>,
    pub parser_hint: Option<String>,
    pub task_type_hint: Option<String>,
}

impl CommandContext {
    pub fn new(argv: Vec<String>) -> Self {
        Self { argv, working_dir: None, parser_hint: None, task_type_hint: None }
    }

    pub fn with_working_dir(argv: Vec<String>, working_dir: impl Into<std::path::PathBuf>) -> Self {
        Self { argv, working_dir: Some(working_dir.into()), parser_hint: None, task_type_hint: None }
    }

    pub fn with_parser_hint(mut self, parser_hint: impl Into<String>) -> Self {
        self.parser_hint = Some(parser_hint.into());
        self
    }

    pub fn with_task_type_hint(mut self, task_type_hint: impl Into<String>) -> Self {
        self.task_type_hint = Some(task_type_hint.into());
        self
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
    secondary_parsers: Vec<ParserKind>,
    task_type: TaskType,
    profile_rules: Option<ProfileRules>,
    summary_failure: bool,
    summary: Option<String>,
    errors: Vec<TerminalParseError>,
    emitted_error_count: usize,
    line_number: usize,
    pending_stdout: Vec<u8>,
    pending_stderr: Vec<u8>,
    web_build_error_context: bool,
}

#[derive(Debug, Clone)]
struct ProfileRules { summary: Vec<Regex>, success: Vec<Regex>, failure: Vec<Regex>, ignore: Vec<Regex> }

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
    Jest,
    Vitest,
    Playwright,
    Cmake,
    MakeNinja,
    CppCompiler,
    Ruby,
    Php,
    Sbt,
    Mix,
    Haskell,
    Bazel,
    WebBuild,
    SwiftPm,
    DartFlutter,
    Ctest,
    ToxNox,
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
            Self::Jest => "jest",
            Self::Vitest => "vitest",
            Self::Playwright => "playwright",
            Self::Cmake => "cmake",
            Self::MakeNinja => "make-ninja",
            Self::CppCompiler => "cpp-compiler",
            Self::Ruby => "ruby",
            Self::Php => "php",
            Self::Sbt => "sbt",
            Self::Mix => "mix",
            Self::Haskell => "haskell",
            Self::Bazel => "bazel",
            Self::WebBuild => "web-build",
            Self::SwiftPm => "swiftpm",
            Self::DartFlutter => "dart-flutter",
            Self::Ctest => "ctest",
            Self::ToxNox => "tox-nox",
        }
    }
}

impl TerminalOutputParser {
    pub fn new(context: CommandContext) -> Self {
        let (parser, detected_task) = identify(&context.argv, context.working_dir.as_deref(), context.parser_hint.as_deref());
        let task_type = context.task_type_hint.as_deref().and_then(task_type_hint).unwrap_or(detected_task);
        let profile_rules = active_profile_rules(&context.argv, parser);
        let secondary_parsers = node_script_secondary_parsers(&context.argv, context.working_dir.as_deref(), parser);
        Self {
            parser,
            secondary_parsers,
            task_type,
            profile_rules,
            summary_failure: false,
            summary: None,
            errors: Vec::new(),
            emitted_error_count: 0,
            line_number: 0,
            pending_stdout: Vec::new(),
            pending_stderr: Vec::new(),
            web_build_error_context: false,
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
        self.apply_profile_rules(&line);
        self.parse_kind(self.parser, &line);
        for parser in self.secondary_parsers.clone() { self.parse_kind(parser, &line); }
    }

    fn parse_kind(&mut self, parser: ParserKind, line: &str) {
        match parser {
            ParserKind::Maven => self.parse_maven(&line),
            ParserKind::Gradle => self.parse_gradle(&line),
            ParserKind::AndroidGradle => { self.parse_gradle(&line); self.parse_android_diagnostic(&line); }
            ParserKind::Pytest => self.parse_pytest(&line),
            ParserKind::Cargo => self.parse_cargo(&line),
            ParserKind::TypeScript => self.parse_typescript(&line),
            ParserKind::Go => self.parse_go(&line),
            ParserKind::Xcodebuild => self.parse_xcodebuild(&line),
            ParserKind::PythonUnittest => self.parse_python_unittest(&line),
            ParserKind::Dotnet => self.parse_dotnet(&line),
            ParserKind::Jest => self.parse_js_test(&line, "jest"),
            ParserKind::Vitest => self.parse_js_test(&line, "vitest"),
            ParserKind::Playwright => self.parse_js_test(&line, "playwright"),
            ParserKind::Cmake => self.parse_native_build(&line, "cmake"),
            ParserKind::MakeNinja => self.parse_native_build(&line, "make"),
            ParserKind::CppCompiler => self.parse_cpp_compiler(&line),
            ParserKind::Ruby => self.parse_language_summary(&line, "ruby"),
            ParserKind::Php => self.parse_language_summary(&line, "php"),
            ParserKind::Sbt => self.parse_language_summary(&line, "sbt"),
            ParserKind::Mix => self.parse_language_summary(&line, "mix"),
            ParserKind::Haskell => self.parse_language_summary(&line, "haskell"),
            ParserKind::Bazel => self.parse_language_summary(&line, "bazel"),
            ParserKind::WebBuild => self.parse_web_build(&line),
            ParserKind::SwiftPm => self.parse_swiftpm(&line),
            ParserKind::DartFlutter => self.parse_dart_flutter(&line),
            ParserKind::Ctest => self.parse_ctest(&line),
            ParserKind::ToxNox => self.parse_tox_nox(&line),
            _ => {}
        }
    }

    fn apply_profile_rules(&mut self, line: &str) {
        let Some(rules) = self.profile_rules.as_ref() else { return; };
        if rules.ignore.iter().any(|pattern| pattern.is_match(line)) { return; }
        if rules.summary.iter().any(|pattern| pattern.is_match(line)) { self.summary = Some(line.to_string()); }
        if rules.failure.iter().any(|pattern| pattern.is_match(line)) {
            self.summary_failure = true;
            self.summary = Some(line.to_string());
        } else if rules.success.iter().any(|pattern| pattern.is_match(line)) {
            self.summary_failure = false;
            self.summary = Some(line.to_string());
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

    /// Adds a diagnostic obtained from a report generated by this command
    /// (for example Surefire XML). Reports are treated as corroborating
    /// evidence and never read from a previous session by the caller.
    pub fn add_artifact_error(&mut self, error: TerminalParseError, summary: impl Into<String>) {
        self.summary_failure = true;
        self.summary = Some(summary.into());
        let mut error = error;
        if error.start_line == 0 { if let Some(line) = error.line { error.start_line = line; } }
        if error.end_line == 0 { error.end_line = error.start_line; }
        if !self.errors.iter().any(|existing| existing.message == error.message && existing.file == error.file && existing.line == error.line) {
            self.errors.push(error);
        }
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
            self.summary_failure = true;
            self.summary = Some(line.to_string());
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

    fn parse_android_diagnostic(&mut self, line: &str) {
        let lower = line.to_ascii_lowercase();
        let fixed = [
            ("aapt2", "Android AAPT2 resource error"),
            ("manifest merger failed", "Android Manifest merge failed"),
            ("r8: ", "Android R8/ProGuard error"),
            ("keystore was tampered", "Android signing/keystore error"),
            ("sdk location not found", "Android SDK environment error"),
            ("ndk not found", "Android NDK environment error"),
            ("unsupported class file major version", "Android JDK/AGP compatibility error"),
        ];
        let resource_missing = lower.contains("resource") && lower.contains("not found");
        let dex_failure = lower.contains("dex") && (lower.contains("error") || lower.contains("failed"));
        let match_item = fixed.iter().find(|(needle, _)| lower.contains(needle));
        let (needle, label) = match_item.map(|(needle, label)| (*needle, *label)).unwrap_or_else(|| {
            if resource_missing { ("resource missing", "Android resource missing") }
            else if dex_failure { ("dex failure", "Android DEX processing error") }
            else { ("", "") }
        });
        if !needle.is_empty() {
            // AAPT2 and the other tools emit stable prefixes; retain the full
            // line for the user while classifying it without global keywords.
            self.summary_failure = true;
            self.summary = Some(label.to_string());
            self.errors.push(TerminalParseError { message: line.to_string(), file: None, line: None, column: None, code: Some((*needle).to_string()), test_name: None, start_line: self.line_number, end_line: self.line_number });
        }
        if let Some(error) = android_location_diagnostic(line, self.line_number) {
            self.errors.push(error);
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
        let failed = counter(line, "failed").unwrap_or(0).max(counter_before(line, "failed").unwrap_or(0));
        let errors = counter(line, "errors").unwrap_or(0).max(counter_before(line, "errors").unwrap_or(0));
        if (line.contains(" passed") || line.contains(" failed") || line.contains(" errors"))
            && (failed > 0 || errors > 0)
        {
            self.summary_failure = true;
            self.summary = Some(line.to_string());
        }
    }

    fn parse_cargo(&mut self, line: &str) {
        if self.parse_cargo_json(line) { return; }
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
        if self.parse_go_json(line) { return; }
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

    fn parse_go_json(&mut self, line: &str) -> bool {
        let Ok(value) = serde_json::from_str::<serde_json::Value>(line) else { return false; };
        let Some(action) = value.get("Action").and_then(|v| v.as_str()) else { return false; };
        let package = value.get("Package").and_then(|v| v.as_str()).unwrap_or("");
        match action {
            "pass" => self.summary = Some(format!("go test passed: {package}")),
            "fail" => {
                self.summary_failure = true;
                self.summary = Some(format!("go test failed: {package}"));
                if let Some(test) = value.get("Test").and_then(|v| v.as_str()) {
                    self.errors.push(TerminalParseError { message: format!("failed test: {test}"), file: None, line: None, column: None, code: None, test_name: Some(test.to_string()), start_line: self.line_number, end_line: self.line_number });
                }
            }
            "build-fail" => {
                self.summary_failure = true;
                self.summary = Some(format!("go build failed: {package}"));
            }
            _ => {}
        }
        true
    }

    fn parse_cargo_json(&mut self, line: &str) -> bool {
        let Ok(value) = serde_json::from_str::<serde_json::Value>(line) else { return false; };
        let Some(reason) = value.get("reason").and_then(|v| v.as_str()) else { return false; };
        match reason {
            "compiler-message" => {
                let message = value.get("message").cloned().unwrap_or_default();
                let level = message.get("level").and_then(|v| v.as_str()).unwrap_or("");
                if level == "error" {
                    let rendered = message.get("rendered").and_then(|v| v.as_str()).unwrap_or("compiler error").trim().to_string();
                    let code = message.get("code").and_then(|v| v.get("code")).and_then(|v| v.as_str()).map(str::to_string);
                    self.summary_failure = true;
                    self.summary = Some(rendered.clone());
                    self.errors.push(TerminalParseError { message: rendered, file: None, line: None, column: None, code, test_name: None, start_line: self.line_number, end_line: self.line_number });
                }
            }
            "test" => {
                let event = value.get("event").and_then(|v| v.as_str()).unwrap_or("");
                if event == "failed" {
                    self.summary_failure = true;
                    let name = value.get("name").and_then(|v| v.as_str()).unwrap_or("unknown test");
                    self.summary = Some(format!("cargo test failed: {name}"));
                    self.errors.push(TerminalParseError { message: format!("failed test: {name}"), file: None, line: None, column: None, code: None, test_name: Some(name.to_string()), start_line: self.line_number, end_line: self.line_number });
                } else if event == "ok" {
                    self.summary = Some("cargo test passed".to_string());
                }
            }
            "build-finished" => {
                if value.get("success").and_then(|v| v.as_bool()) == Some(false) {
                    self.summary_failure = true;
                    self.summary = Some("cargo build failed".to_string());
                }
            }
            _ => {}
        }
        true
    }

    fn parse_xcodebuild(&mut self, line: &str) {
        if line.contains("BUILD FAILED") || line.contains("TEST FAILED") {
            self.summary_failure = true;
            self.summary = Some(line.to_string());
        } else if line.contains("BUILD SUCCEEDED") || line.contains("TEST SUCCEEDED") {
            self.summary = Some(line.to_string());
        }
        if let Some(error) = xcode_diagnostic(line, self.line_number) {
            self.summary_failure = true;
            self.errors.push(error);
        }
        if line.contains("Test Case '-[") && line.contains("failed") {
            self.summary_failure = true;
            self.errors.push(TerminalParseError { message: line.to_string(), file: None, line: None, column: None, code: None, test_name: Some(line.to_string()), start_line: self.line_number, end_line: self.line_number });
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

    fn parse_js_test(&mut self, line: &str, _tool: &str) {
        let failed = counter(line, "failed").unwrap_or(0);
        if line.contains("Tests:") || line.contains("Test Files") || line.contains("Test Suites") || line.contains("Tests ") || line.contains(" passed") {
            self.summary = Some(line.to_string());
            if failed > 0 { self.summary_failure = true; }
        }
    }

    fn parse_native_build(&mut self, line: &str, tool: &str) {
        if (tool == "cmake" && line.contains("CMake Error"))
            || (tool == "make" && line.contains("Error "))
            || (tool == "make" && line.contains("build stopped"))
        {
            self.summary_failure = true;
            self.summary = Some(line.to_string());
            self.errors.push(TerminalParseError {
                message: line.to_string(), file: None, line: None, column: None,
                code: None, test_name: None, start_line: self.line_number, end_line: self.line_number,
            });
        }
    }

    fn parse_cpp_compiler(&mut self, line: &str) {
        if let Some((location, message)) = line.split_once(": error:") {
            let mut parts = location.rsplitn(3, ':');
            let column = parts.next().and_then(|v| v.parse().ok());
            let source_line = parts.next().and_then(|v| v.parse().ok());
            let file = parts.next().map(str::to_string);
            self.errors.push(TerminalParseError {
                message: message.trim().to_string(), file, line: source_line, column,
                code: None, test_name: None, start_line: self.line_number, end_line: self.line_number,
            });
        }
    }

    fn parse_language_summary(&mut self, line: &str, tool: &str) {
        let failure = match tool {
            "ruby" => counter(line, "failure").unwrap_or(0).max(counter_before(line, "failure").unwrap_or(0)) > 0,
            "php" => counter(line, "failures:").unwrap_or(0) > 0 || counter(line, "errors:").unwrap_or(0) > 0,
            "sbt" => line.starts_with("[error] Failed tests"),
            "mix" => line.contains("failure") && counter(line, "failure").unwrap_or(0).max(counter_before(line, "failure").unwrap_or(0)) > 0,
            "haskell" => line.contains(": FAIL"),
            "bazel" => line.starts_with("FAILED:") || line.contains("unsuccessfully"),
            _ => false,
        };
        if failure {
            self.summary_failure = true;
            self.summary = Some(line.to_string());
        } else if (tool == "ruby" && line.contains("examples")) || (tool == "php" && line.contains("Tests:")) || (tool == "bazel" && line.contains("successfully")) {
            self.summary = Some(line.to_string());
        }
    }

    fn parse_web_build(&mut self, line: &str) {
        // Web bundlers vary in wording; only their explicit terminal markers
        // are trusted. Warnings and module names containing "error" are not.
        let lower = line.to_ascii_lowercase();
        if lower.contains("compiled successfully") || lower.contains("build complete") || lower.contains("built in ") || lower.contains("✓ built") || lower.contains("created ") {
            self.summary = Some(line.to_string());
        }
        if lower.contains("failed to load config") {
            self.summary_failure = true;
            self.summary = Some(line.to_string());
            self.errors.push(TerminalParseError { message: line.to_string(), file: None, line: None, column: None, code: None, test_name: None, start_line: self.line_number, end_line: self.line_number });
        }
        if lower.contains("error during build:") {
            self.summary_failure = true;
            self.summary = Some(line.to_string());
            self.web_build_error_context = true;
            self.errors.push(TerminalParseError { message: line.to_string(), file: None, line: None, column: None, code: None, test_name: None, start_line: self.line_number, end_line: self.line_number });
        } else if self.web_build_error_context {
            let trimmed = line.trim_start();
            if trimmed.starts_with("Error:") || trimmed.starts_with("error:") {
                self.summary_failure = true;
                self.errors.push(TerminalParseError { message: trimmed.to_string(), file: None, line: None, column: None, code: None, test_name: None, start_line: self.line_number, end_line: self.line_number });
            }
            self.web_build_error_context = false;
        }
        if lower.contains("failed to compile") || lower.contains("compilation failed") || lower.contains("build failed") || lower.starts_with("error: failed to") {
            self.summary_failure = true;
            self.summary = Some(line.to_string());
            self.errors.push(TerminalParseError { message: line.to_string(), file: None, line: None, column: None, code: None, test_name: None, start_line: self.line_number, end_line: self.line_number });
        }
    }

    fn parse_swiftpm(&mut self, line: &str) {
        if line.contains("Test Suite '") && line.contains("passed") {
            self.summary = Some(line.to_string());
        } else if line.contains("Test Suite '") && line.contains("failed") {
            self.summary_failure = true;
            self.summary = Some(line.to_string());
        } else if line.contains("Build complete!") || line.contains("Build complete") {
            self.summary = Some(line.to_string());
        } else if line.starts_with("error:") {
            self.summary_failure = true;
            self.summary = Some(line.to_string());
            self.errors.push(TerminalParseError { message: line.to_string(), file: None, line: None, column: None, code: None, test_name: None, start_line: self.line_number, end_line: self.line_number });
        }
    }

    fn parse_dart_flutter(&mut self, line: &str) {
        let lower = line.to_ascii_lowercase();
        if lower.contains("all tests passed") || lower.contains("no issues found") || lower.contains("built successfully") {
            self.summary = Some(line.to_string());
        } else if lower.contains("some tests failed") || lower.contains("issues found") || lower.contains("build failed") {
            self.summary_failure = true;
            self.summary = Some(line.to_string());
            self.errors.push(TerminalParseError { message: line.to_string(), file: None, line: None, column: None, code: None, test_name: None, start_line: self.line_number, end_line: self.line_number });
        }
    }

    fn parse_ctest(&mut self, line: &str) {
        if line.contains("tests passed") && counter(line, "tests failed").unwrap_or(0).max(counter_before(line, "tests failed").unwrap_or(0)) == 0 {
            self.summary = Some(line.to_string());
        } else if line.contains("tests failed") && counter(line, "tests failed").unwrap_or(0).max(counter_before(line, "tests failed").unwrap_or(0)) > 0 {
            self.summary_failure = true;
            self.summary = Some(line.to_string());
        } else if line.starts_with("The following tests FAILED") {
            self.summary_failure = true;
            self.summary = Some(line.to_string());
            self.errors.push(TerminalParseError { message: line.to_string(), file: None, line: None, column: None, code: None, test_name: None, start_line: self.line_number, end_line: self.line_number });
        }
    }

    fn parse_tox_nox(&mut self, line: &str) {
        if line.contains("congratulations :)" ) || line.contains("Sessions complete") {
            self.summary = Some(line.to_string());
        } else if line.contains(": FAIL") || line.contains("evaluation failed") || line.contains("Session ") && line.ends_with("failed") {
            self.summary_failure = true;
            self.summary = Some(line.to_string());
            self.errors.push(TerminalParseError { message: line.to_string(), file: None, line: None, column: None, code: None, test_name: None, start_line: self.line_number, end_line: self.line_number });
        }
    }
}

fn identify(argv: &[String], working_dir: Option<&std::path::Path>, parser_hint: Option<&str>) -> (ParserKind, TaskType) {
    let program = argv.first().map(|value| basename(value)).unwrap_or("");
    let arguments = argv.join(" ").to_ascii_lowercase();
    if let Some(parser) = parser_hint.and_then(parser_hint_kind) {
        return (parser, task_from_words(&arguments));
    }
    if matches!(program, "npm" | "pnpm" | "yarn" | "bun") {
        if let Some((inner, task)) = resolve_node_script(argv, working_dir) {
            return (inner, task);
        }
        // A package script is deliberately not guessed when package.json is
        // unavailable. Its exit code still provides an exact generic result.
        return (ParserKind::Generic, TaskType::Custom);
    }
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
            .filter(|profile| profile.command_matchers.iter().any(|pattern| profile_matches_command(pattern, &command)))
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
                "jest" => Some(ParserKind::Jest),
                "vitest" => Some(ParserKind::Vitest),
                "playwright" => Some(ParserKind::Playwright),
                "cmake" => Some(ParserKind::Cmake),
                "make" => Some(ParserKind::MakeNinja),
                "ninja" => Some(ParserKind::MakeNinja),
                "cpp-compiler" => Some(ParserKind::CppCompiler),
                "ruby" => Some(ParserKind::Ruby),
                "php" => Some(ParserKind::Php),
                "sbt" => Some(ParserKind::Sbt),
                "mix" => Some(ParserKind::Mix),
                "haskell" => Some(ParserKind::Haskell),
                "bazel" => Some(ParserKind::Bazel),
                "web-build" => Some(ParserKind::WebBuild),
                "swiftpm" => Some(ParserKind::SwiftPm),
                "dart-flutter" => Some(ParserKind::DartFlutter),
                "ctest" => Some(ParserKind::Ctest),
                "tox-nox" => Some(ParserKind::ToxNox),
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
    if matches!(program, "jest" | "vitest" | "playwright") {
        return (match program { "jest" => ParserKind::Jest, "vitest" => ParserKind::Vitest, _ => ParserKind::Playwright }, TaskType::Test);
    }
    if program == "cmake" { return (ParserKind::Cmake, task_from_words(&arguments)); }
    if matches!(program, "make" | "gmake" | "ninja") { return (ParserKind::MakeNinja, TaskType::Build); }
    if matches!(program, "gcc" | "g++" | "clang" | "clang++" | "cc" | "c++" | "cl") {
        return (ParserKind::CppCompiler, TaskType::Compile);
    }
    if matches!(program, "rspec" | "rake") || (program == "bundle" && arguments.contains("rspec")) { return (ParserKind::Ruby, TaskType::Test); }
    if matches!(program, "phpunit" | "pest") || (program == "composer" && arguments.contains("test")) { return (ParserKind::Php, TaskType::Test); }
    if program == "sbt" { return (ParserKind::Sbt, task_from_words(&arguments)); }
    if program == "mix" { return (ParserKind::Mix, task_from_words(&arguments)); }
    if matches!(program, "cabal" | "stack") { return (ParserKind::Haskell, task_from_words(&arguments)); }
    if program == "bazel" { return (ParserKind::Bazel, task_from_words(&arguments)); }
    if program == "go" {
        return (ParserKind::Go, task_from_words(&arguments));
    }
    if program == "cargo" || program == "cargo-nextest" {
        return (ParserKind::Cargo, task_from_words(&arguments));
    }
    if program == "xcodebuild" {
        return (ParserKind::Xcodebuild, task_from_words(&arguments));
    }
    if program == "swift" && (arguments.contains(" build") || arguments.contains(" test") || arguments.contains(" package")) {
        return (ParserKind::SwiftPm, task_from_words(&arguments));
    }
    if program == "swiftpm" { return (ParserKind::SwiftPm, task_from_words(&arguments)); }
    if program == "dart" && (arguments.contains(" test") || arguments.contains(" analyze") || arguments.contains(" compile")) {
        return (ParserKind::DartFlutter, task_from_words(&arguments));
    }
    if program == "flutter" { return (ParserKind::DartFlutter, task_from_words(&arguments)); }
    if matches!(program, "vite" | "webpack" | "rollup" | "next" | "nuxt") {
        return (ParserKind::WebBuild, TaskType::Build);
    }
    if program == "ctest" { return (ParserKind::Ctest, TaskType::Test); }
    if matches!(program, "tox" | "nox") { return (ParserKind::ToxNox, TaskType::Test); }
    (ParserKind::Generic, TaskType::Custom)
}

fn active_profile_rules(argv: &[String], parser: ParserKind) -> Option<ProfileRules> {
    let profiles = crate::session::terminal_profiles::active()?;
    let command = argv.join(" ");
    let profile = profiles.profiles.iter()
        .filter(|profile| profile.id == parser.id())
        .filter(|profile| profile.command_matchers.is_empty() || profile.command_matchers.iter().any(|pattern| profile_matches_command(pattern, &command)))
        .max_by_key(|profile| profile.priority)?;
    let compile = |patterns: &[String]| patterns.iter().filter_map(|pattern| Regex::new(pattern).ok()).collect();
    Some(ProfileRules { summary: compile(&profile.summary_patterns), success: compile(&profile.success_patterns), failure: compile(&profile.failure_patterns), ignore: compile(&profile.ignore_patterns) })
}

fn profile_matches_command(pattern: &str, command: &str) -> bool {
    let pattern = pattern.trim();
    let executable = command.split_whitespace().next().map(basename).unwrap_or("");
    if pattern.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '+' | '-' | '.')) { return executable == pattern; }
    Regex::new(pattern).is_ok_and(|regex| regex.is_match(command))
}

fn parser_hint_kind(value: &str) -> Option<ParserKind> {
    match value.trim().to_ascii_lowercase().as_str() {
        "generic" | "generic-exit" => Some(ParserKind::Generic),
        "maven" => Some(ParserKind::Maven), "gradle" => Some(ParserKind::Gradle), "android-gradle" => Some(ParserKind::AndroidGradle),
        "pytest" => Some(ParserKind::Pytest), "go" => Some(ParserKind::Go), "cargo" => Some(ParserKind::Cargo),
        "jest" => Some(ParserKind::Jest), "vitest" => Some(ParserKind::Vitest), "playwright" => Some(ParserKind::Playwright),
        "typescript" => Some(ParserKind::TypeScript), "xcodebuild" => Some(ParserKind::Xcodebuild), "swiftpm" => Some(ParserKind::SwiftPm),
        "dart-flutter" => Some(ParserKind::DartFlutter), "web-build" => Some(ParserKind::WebBuild), "ctest" => Some(ParserKind::Ctest),
        "tox-nox" => Some(ParserKind::ToxNox), "dotnet" => Some(ParserKind::Dotnet),
        "python-unittest" => Some(ParserKind::PythonUnittest), "cmake" => Some(ParserKind::Cmake),
        "make-ninja" | "make" | "ninja" => Some(ParserKind::MakeNinja), "cpp-compiler" => Some(ParserKind::CppCompiler),
        "ruby" => Some(ParserKind::Ruby), "php" => Some(ParserKind::Php), "sbt" => Some(ParserKind::Sbt),
        "mix" => Some(ParserKind::Mix), "haskell" => Some(ParserKind::Haskell), "bazel" => Some(ParserKind::Bazel), _ => None,
    }
}

fn task_type_hint(value: &str) -> Option<TaskType> {
    match value.trim().to_ascii_lowercase().as_str() {
        "compile" => Some(TaskType::Compile), "test" => Some(TaskType::Test), "package" => Some(TaskType::Package),
        "build" => Some(TaskType::Build), "run" => Some(TaskType::Run), "lint" => Some(TaskType::Lint), "custom" => Some(TaskType::Custom), _ => None,
    }
}

/// Resolve a package-manager script to the tool it actually invokes. This is
/// intentionally conservative: only a script declared in package.json is
/// considered, and wrappers are inspected for known executable tokens.
fn resolve_node_script(argv: &[String], working_dir: Option<&std::path::Path>) -> Option<(ParserKind, TaskType)> {
    let script = argv.iter().skip(1).find(|arg| !arg.starts_with('-') && *arg != "run")?;
    let cwd = working_dir?;
    let package = std::fs::read_to_string(cwd.join("package.json")).ok()?;
    let value: serde_json::Value = serde_json::from_str(&package).ok()?;
    let command = value.get("scripts")?.get(script)?.as_str()?.to_ascii_lowercase();
    let mut queue = vec![command.clone()];
    let mut visited = std::collections::HashSet::new();
    let mut best = None;
    for _ in 0..8 {
        let Some(current) = queue.pop() else { break };
        if !visited.insert(current.clone()) { continue; }
        let words: Vec<_> = current.split(|c: char| c.is_whitespace() || c == '&' || c == ';' || c == '|').filter(|w| !w.is_empty()).collect();
        for (index, word) in words.iter().enumerate() {
            let name = basename(word).trim_matches(['"', '\'']);
            let candidate = match name {
                "jest" | "react-scripts" => Some((ParserKind::Jest, TaskType::Test)),
                "vitest" => Some((ParserKind::Vitest, TaskType::Test)),
                "playwright" => Some((ParserKind::Playwright, TaskType::Test)),
                "tsc" | "vue-tsc" | "ngc" => Some((ParserKind::TypeScript, TaskType::Compile)),
                "vite" | "webpack" | "rollup" | "next" | "nuxt" => Some((ParserKind::WebBuild, TaskType::Build)),
                "npm" | "pnpm" | "yarn" | "bun" if words.get(index + 1) == Some(&"run") => {
                    if let Some(nested) = words.get(index + 2).and_then(|name| value.get("scripts")?.get(*name)?.as_str()) { queue.push(nested.to_ascii_lowercase()); }
                    None
                }
                _ => None,
            };
            if candidate.is_some() { best = candidate; }
        }
    }
    best.or_else(|| Some((ParserKind::Generic, task_from_words(&command))))
}

fn node_script_secondary_parsers(argv: &[String], working_dir: Option<&std::path::Path>, primary: ParserKind) -> Vec<ParserKind> {
    let Some(cwd) = working_dir else { return Vec::new(); };
    let Some(script) = argv.iter().skip(1).find(|arg| !arg.starts_with('-') && *arg != "run") else { return Vec::new(); };
    let Ok(package) = std::fs::read_to_string(cwd.join("package.json")) else { return Vec::new(); };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(&package) else { return Vec::new(); };
    let Some(command) = value.get("scripts").and_then(|scripts| scripts.get(script)).and_then(|value| value.as_str()) else { return Vec::new(); };
    let mut result = Vec::new();
    for word in command.split(|c: char| c.is_whitespace() || c == '&' || c == ';' || c == '|') {
        let kind = match basename(word).trim_matches(['"', '\'']) {
            "jest" | "react-scripts" => Some(ParserKind::Jest),
            "vitest" => Some(ParserKind::Vitest),
            "playwright" => Some(ParserKind::Playwright),
            "tsc" | "vue-tsc" | "ngc" => Some(ParserKind::TypeScript),
            "vite" | "webpack" | "rollup" | "next" | "nuxt" => Some(ParserKind::WebBuild),
            _ => None,
        };
        if let Some(kind) = kind.filter(|kind| *kind != primary) {
            if !result.contains(&kind) { result.push(kind); }
        }
    }
    result
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

fn counter_before(line: &str, label: &str) -> Option<u64> {
    let lower = line.to_ascii_lowercase();
    let label = label.to_ascii_lowercase();
    let index = lower.find(&label)?;
    let digits: String = lower[..index].trim_end_matches(|c: char| c == ',' || c == ':' || c.is_whitespace())
        .chars().rev().take_while(|c| c.is_ascii_digit()).collect::<String>().chars().rev().collect();
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

fn xcode_diagnostic(line: &str, line_number: usize) -> Option<TerminalParseError> {
    let marker = ": error:";
    let index = line.find(marker)?;
    let location = &line[..index];
    let mut parts = location.rsplitn(3, ':');
    let column = parts.next().and_then(|v| v.parse().ok());
    let source_line = parts.next().and_then(|v| v.parse().ok());
    let file = parts.next()?.to_string();
    Some(TerminalParseError { message: line[index + marker.len()..].trim().to_string(), file: Some(file), line: source_line, column, code: None, test_name: None, start_line: line_number, end_line: line_number })
}

fn android_location_diagnostic(line: &str, line_number: usize) -> Option<TerminalParseError> {
    let marker = ": error:";
    let index = line.find(marker)?;
    let location = &line[..index];
    let mut parts = location.rsplitn(3, ':');
    let column = parts.next().and_then(|v| v.parse().ok());
    let source_line = parts.next().and_then(|v| v.parse().ok());
    let file = parts.next()?.to_string();
    Some(TerminalParseError { message: line[index + marker.len()..].trim().to_string(), file: Some(file), line: source_line, column, code: Some("android-diagnostic".to_string()), test_name: None, start_line: line_number, end_line: line_number })
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
