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
fn parser_exposes_new_diagnostic_candidates_during_streaming() {
    let mut parser = TerminalOutputParser::new(CommandContext::new(vec!["pytest".into()]));
    parser.on_line("FAILED tests/test_api.py::test_login - AssertionError");
    let candidates = parser.take_candidates();
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].test_name.as_deref(), Some("tests/test_api.py::test_login"));
    assert!(parser.take_candidates().is_empty());
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
fn parser_bounds_long_utf8_partial_lines_without_splitting_character() {
    let context = CommandContext::new(vec!["mystery".into()]);
    let mut parser = TerminalOutputParser::new(context);
    let mut bytes = vec![b'x'; 16 * 1024 - 1];
    bytes.extend_from_slice("界\n".as_bytes());
    parser.on_bytes(&bytes);
    let result = parser.finalize(Some(0));
    assert_eq!(result.status, ParseStatus::Success);
}

#[test]
fn unsupported_node_script_uses_generic_exit_parser_until_inner_tool_is_resolved() {
    let result = parse(&["npm", "run", "test"], "FAIL tests named Error", Some(0));

    assert_eq!(result.parser, "generic-exit");
    assert_eq!(result.status, ParseStatus::Success);
    assert!(result.errors.is_empty());
}

#[test]
fn node_script_resolver_uses_declared_inner_jest() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("package.json"), r#"{"scripts":{"test":"jest --runInBand"}}"#).unwrap();
    let context = CommandContext::with_working_dir(
        vec!["npm".into(), "run".into(), "test".into()],
        dir.path(),
    );
    let mut parser = TerminalOutputParser::new(context);
    parser.on_line("Test Suites: 1 passed, 1 total");
    let result = parser.finalize(Some(0));
    assert_eq!(result.parser, "jest");
    assert_eq!(result.status, ParseStatus::Success);
}

#[test]
fn python_unittest_uses_final_failure_counters() {
    let result = parse(
        &["python", "-m", "unittest"],
        "Ran 3 tests in 0.01s\nFAILED (failures=1, errors=0)",
        Some(1),
    );
    assert_eq!(result.parser, "python-unittest");
    assert_eq!(result.status, ParseStatus::Failed);
}

#[test]
fn dotnet_test_zero_failures_is_success_even_with_error_word_in_test_name() {
    let result = parse(
        &["dotnet", "test"],
        "ErrorHandlingTests passed\nPassed! - Failed: 0, Passed: 4, Skipped: 0",
        Some(0),
    );
    assert_eq!(result.parser, "dotnet");
    assert_eq!(result.status, ParseStatus::Success);
}

#[test]
fn jest_final_tests_summary_controls_status() {
    let result = parse(&["jest", "--runInBand"], "Tests: 1 failed, 2 passed, 3 total", Some(1));
    assert_eq!(result.parser, "jest");
    assert_eq!(result.status, ParseStatus::Failed);
}

#[test]
fn vitest_zero_failed_files_is_success() {
    let result = parse(&["vitest", "run"], "Test Files 2 passed (2)\nTests 5 passed (5)", Some(0));
    assert_eq!(result.parser, "vitest");
    assert_eq!(result.status, ParseStatus::Success);
}

#[test]
fn playwright_flaky_retry_is_not_a_failed_final_summary() {
    let result = parse(&["playwright", "test"], "1 flaky, 2 passed, 0 failed", Some(0));
    assert_eq!(result.parser, "playwright");
    assert_eq!(result.status, ParseStatus::Success);
}

#[test]
fn cmake_error_summary_is_failed() {
    let result = parse(&["cmake", "--build", "build"], "CMake Error at CMakeLists.txt:4 (add_executable):\n  Cannot find source file", Some(1));
    assert_eq!(result.parser, "cmake");
    assert_eq!(result.status, ParseStatus::Failed);
}

#[test]
fn compiler_warning_does_not_fail_successful_compile() {
    let result = parse(&["clang++", "main.cpp"], "main.cpp:4:2: warning: unused variable", Some(0));
    assert_eq!(result.parser, "cpp-compiler");
    assert_eq!(result.status, ParseStatus::Success);
}

#[test]
fn ruby_rspec_summary_uses_failure_count() {
    let result = parse(&["bundle", "exec", "rspec"], "3 examples, 0 failures", Some(0));
    assert_eq!(result.parser, "ruby");
    assert_eq!(result.status, ParseStatus::Success);
}

#[test]
fn bazel_failed_target_is_failed() {
    let result = parse(&["bazel", "test", "//..."], "FAILED: //app:test\nINFO: Build completed unsuccessfully", Some(1));
    assert_eq!(result.parser, "bazel");
    assert_eq!(result.status, ParseStatus::Failed);
}
