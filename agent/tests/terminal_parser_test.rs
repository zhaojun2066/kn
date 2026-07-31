use kn_agent::session::terminal_parser::{
    CommandContext, OutputStream, ParseStatus, TaskType, TerminalOutputParser,
};

fn parse(command: &[&str], output: &str, exit_code: Option<i32>) -> kn_agent::session::terminal_parser::TerminalParseResult {
    let context = CommandContext::new(command.iter().map(|part| (*part).to_string()).collect());
    let mut parser = TerminalOutputParser::new(context);
    for line in output.lines() {
        parser.on_line(line);
    }
    parser.finalize(exit_code)
}

#[test]
fn generic_parser_ignores_error_words_when_process_succeeds() {
    let result = parse(
        &["./scripts/check.sh"],
        "ExceptionTest\nassertThrows(RuntimeException.class)\nErrors: 0\nExpected exception",
        Some(0),
    );

    assert_eq!(result.status, ParseStatus::Success);
    assert_eq!(result.task_type, TaskType::Custom);
    assert!(result.errors.is_empty());
}

#[test]
fn maven_zero_failure_summary_is_success() {
    let result = parse(
        &["mvn", "test"],
        "[INFO] Tests run: 10, Failures: 0, Errors: 0, Skipped: 1\n[INFO] BUILD SUCCESS",
        Some(0),
    );

    assert_eq!(result.status, ParseStatus::Success);
    assert_eq!(result.task_type, TaskType::Test);
    assert_eq!(result.parser, "maven");
    assert!(result.errors.is_empty());
}

#[test]
fn pytest_summary_with_failures_is_failed_and_extracts_test_name() {
    let result = parse(
        &["python", "-m", "pytest"],
        "FAILED tests/test_math.py::test_addition - assert 1 == 2\n===== 1 failed, 4 passed in 0.10s =====",
        Some(1),
    );

    assert_eq!(result.status, ParseStatus::Failed);
    assert_eq!(result.task_type, TaskType::Test);
    assert_eq!(result.parser, "pytest");
    assert_eq!(result.errors[0].test_name.as_deref(), Some("tests/test_math.py::test_addition"));
}

#[test]
fn android_gradle_assemble_is_identified_separately_from_generic_gradle() {
    let result = parse(
        &["./gradlew", ":app:assembleDebug"],
        "> Task :app:mergeDebugResources\nBUILD SUCCESSFUL in 2s",
        Some(0),
    );

    assert_eq!(result.status, ParseStatus::Success);
    assert_eq!(result.task_type, TaskType::Package);
    assert_eq!(result.parser, "android-gradle");
}

#[test]
fn parser_keeps_stdout_and_stderr_partial_lines_separate() {
    let context = CommandContext::new(vec!["cargo".to_string(), "check".to_string()]);
    let mut parser = TerminalOutputParser::new(context);

    parser.on_bytes_from(OutputStream::Stdout, b"progress: ");
    parser.on_bytes_from(OutputStream::Stderr, b"error[E0308]: mismatched types\n");
    parser.on_bytes_from(OutputStream::Stdout, b"done\n");
    let result = parser.finalize(Some(1));

    assert_eq!(result.errors.len(), 1);
    assert_eq!(result.errors[0].message, "error[E0308]: mismatched types");
    assert_eq!(result.errors[0].code.as_deref(), Some("E0308"));
}

#[test]
fn unsupported_node_script_uses_generic_exit_parser_until_inner_tool_is_resolved() {
    let result = parse(&["npm", "run", "test"], "FAIL tests named Error", Some(0));

    assert_eq!(result.parser, "generic-exit");
    assert_eq!(result.status, ParseStatus::Success);
    assert!(result.errors.is_empty());
}
