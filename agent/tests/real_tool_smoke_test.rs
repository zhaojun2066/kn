//! Real-tool smoke tests.  Each test creates a tiny project locally and is
//! skipped when the corresponding tool is not installed on the host.

use kn_agent::session::terminal_parser::{
    CommandContext, OutputStream, ParseStatus, TerminalOutputParser,
};
use std::fs;
use std::path::Path;
use std::process::{Command, Stdio};

fn available(program: &str) -> bool {
    Command::new("sh")
        .args(["-lc", &format!("command -v {program}")])
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

fn run(cwd: &Path, argv: &[&str]) -> (TerminalOutputParser, Option<i32>) {
    let context = CommandContext::with_working_dir(
        argv.iter().map(|value| (*value).to_string()).collect(),
        cwd,
    );
    let mut parser = TerminalOutputParser::new(context);
    let output = Command::new(argv[0])
        .args(&argv[1..])
        .current_dir(cwd)
        .output()
        .expect("tool should launch");
    parser.on_bytes_from(OutputStream::Stdout, &output.stdout);
    parser.on_bytes_from(OutputStream::Stderr, &output.stderr);
    (parser, output.status.code())
}

#[test]
fn real_pytest_minimal_project() {
    if !available("python3")
        || !Command::new("python3")
            .args(["-c", "import pytest"])
            .stderr(Stdio::null())
            .status()
            .map(|status| status.success())
            .unwrap_or(false)
    {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("test_sample.py"),
        "def test_parser_smoke():\n    assert 1 == 1\n",
    )
    .unwrap();
    let (parser, exit) = run(dir.path(), &["python3", "-m", "pytest", "-q"]);
    let result = parser.finalize(exit);
    assert_eq!(result.parser, "pytest");
    assert_eq!(result.status, ParseStatus::Success, "{}", result.summary);
}

#[test]
fn real_go_minimal_project() {
    if !available("go") {
        return;
    }
    let Ok(goroot) = Command::new("go").args(["env", "GOROOT"]).output() else {
        return;
    };
    let goroot = String::from_utf8_lossy(&goroot.stdout).trim().to_string();
    if !Path::new(&goroot).join("src/testing").is_dir() {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("go.mod"), "module smoke\n\ngo 1.20\n").unwrap();
    fs::write(
        dir.path().join("smoke_test.go"),
        "package smoke\nimport \"testing\"\nfunc TestSmoke(t *testing.T) {}\n",
    )
    .unwrap();
    let (parser, exit) = run(dir.path(), &["go", "test", "./..."]);
    let result = parser.finalize(exit);
    assert_eq!(result.parser, "go");
    assert_eq!(result.status, ParseStatus::Success, "{}", result.summary);
}

#[test]
fn real_cargo_minimal_project() {
    if !available("cargo") {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir_all(dir.path().join("src")).unwrap();
    fs::write(
        dir.path().join("Cargo.toml"),
        "[package]\nname = \"parser-smoke\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .unwrap();
    fs::write(
        dir.path().join("src/lib.rs"),
        "#[test]\nfn parser_smoke() { assert_eq!(2 + 2, 4); }\n",
    )
    .unwrap();
    let (parser, exit) = run(dir.path(), &["cargo", "test", "--quiet"]);
    let result = parser.finalize(exit);
    assert_eq!(result.parser, "cargo");
    assert_eq!(result.status, ParseStatus::Success, "{}", result.summary);
}

#[test]
fn real_maven_minimal_project() {
    if !available("mvn") {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("pom.xml"), r#"<project xmlns="http://maven.apache.org/POM/4.0.0"><modelVersion>4.0.0</modelVersion><groupId>smoke</groupId><artifactId>parser-smoke</artifactId><version>1</version></project>"#).unwrap();
    let (parser, exit) = run(dir.path(), &["mvn", "-o", "-q", "validate"]);
    let result = parser.finalize(exit);
    assert_eq!(result.parser, "maven");
    assert_eq!(result.status, ParseStatus::Success, "{}", result.summary);
}

#[test]
fn real_typescript_minimal_project() {
    if !available("tsc") {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("tsconfig.json"),
        r#"{"compilerOptions":{"strict":true,"noEmit":true},"include":["index.ts"]}"#,
    )
    .unwrap();
    fs::write(dir.path().join("index.ts"), "const answer: number = 42;\n").unwrap();
    let (parser, exit) = run(dir.path(), &["tsc", "--project", "tsconfig.json"]);
    let result = parser.finalize(exit);
    assert_eq!(result.parser, "typescript");
    assert_eq!(result.status, ParseStatus::Success, "{}", result.summary);
}

#[test]
fn real_swiftpm_minimal_project() {
    if !available("swift") {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir_all(dir.path().join("Sources/Smoke")).unwrap();
    fs::write(dir.path().join("Package.swift"), "// swift-tools-version:5.7\nimport PackageDescription\nlet package = Package(name: \"Smoke\", targets: [.executableTarget(name: \"Smoke\")])\n").unwrap();
    fs::write(
        dir.path().join("Sources/Smoke/main.swift"),
        "print(\"smoke\")\n",
    )
    .unwrap();
    let (parser, exit) = run(dir.path(), &["swift", "build"]);
    let result = parser.finalize(exit);
    assert_eq!(result.parser, "swiftpm");
    assert_eq!(result.status, ParseStatus::Success, "{}", result.summary);
}
