//! Local Hook Store — pre-built, practical hooks for Claude Code / Qoder / Codex CLI.
//!
//! Users browse hooks and install them to their chosen CLI with one click.
//! Each hook includes a bash/Python script and its configuration manifest.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

use crate::hook_manager;
use crate::hook_meta;

// ── Types ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoreHook {
    pub id: String,
    pub name: String,
    pub description: String,
    pub category: String,
    pub event_type: String,
    pub matcher: String,
    pub hook_type: String,
    pub script_ext: String,
    pub compatible_clis: Vec<String>,
    /// Platforms this hook can run on: "unix" (macOS/Linux), "windows"
    pub platforms: Vec<String>,
    /// Tags for filtering (e.g. "java", "python", "frontend")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
    /// Whether this hook is currently installed for each CLI
    #[serde(skip_serializing_if = "Option::is_none")]
    pub installed: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HookStoreData {
    pub hooks: Vec<StoreHook>,
}

// ── Helpers ──────────────────────────────────────────────────────

fn hooks_dir() -> PathBuf {
    crate::config_dir().join("hooks")
}


// ── Hook Definitions ─────────────────────────────────────────────

fn all_hooks() -> Vec<StoreHook> {
    vec![
        // ── Security ──────────────────────────────────────────
        StoreHook {
            id: "block-dangerous-commands".into(),
            name: "危险命令拦截".into(),
            description: "拦截 rm -rf /、git push --force main、curl|sh、DROP TABLE 等危险操作。被拦截时 stderr 注入 Claude，AI 会用安全方式重试。".into(),
            category: "security".into(),
            event_type: "PreToolUse".into(),
            matcher: "Bash".into(),
            hook_type: "command".into(),
            script_ext: "sh".into(),
            compatible_clis: vec!["claude".into(), "qoder".into(), "codex".into()],
            platforms: vec!["unix".into()],
            tags: None,
            installed: None,
        },
        StoreHook {
            id: "protect-sensitive-files".into(),
            name: "敏感文件保护".into(),
            description: "阻止 AI 修改 .env、.git/、*.pem、*.key、credentials.* 等敏感文件。".into(),
            category: "security".into(),
            event_type: "PreToolUse".into(),
            matcher: "Write|Edit".into(),
            hook_type: "command".into(),
            script_ext: "sh".into(),
            compatible_clis: vec!["claude".into(), "qoder".into(), "codex".into()],
            platforms: vec!["unix".into()],
            tags: None,
            installed: None,
        },
        StoreHook {
            id: "secret-scan".into(),
            name: "API 密钥扫描".into(),
            description: "写入文件前扫描是否包含 sk-ant-api03-、AKIA(AWS)、ghp_(GitHub)、sk- 等密钥模式。命中则阻断并提示用环境变量。".into(),
            category: "security".into(),
            event_type: "PreToolUse".into(),
            matcher: "Write|Edit".into(),
            hook_type: "command".into(),
            script_ext: "py".into(),
            compatible_clis: vec!["claude".into(), "qoder".into(), "codex".into()],
            platforms: vec!["unix".into()],
            tags: None,
            installed: None,
        },
        StoreHook {
            id: "block-main-branch-edits".into(),
            name: "阻断 main/master 直接编辑".into(),
            description: "禁止在 main 或 master 分支上直接编辑文件，强制 AI 先创建功能分支。防止 AI 误操作污染主分支。".into(),
            category: "security".into(),
            event_type: "PreToolUse".into(),
            matcher: "Edit|Write".into(),
            hook_type: "command".into(),
            script_ext: "sh".into(),
            compatible_clis: vec!["claude".into(), "qoder".into(), "codex".into()],
            platforms: vec!["unix".into()],
            tags: None,
            installed: None,
        },
        // ── Code Quality ──────────────────────────────────────
        StoreHook {
            id: "auto-format".into(),
            name: "自动格式化".into(),
            description: "AI 写文件后自动调用格式化工具：.ts/.js→prettier，.py→ruff，.sh→shfmt。Claude 看到格式化结果后代码更规范。".into(),
            category: "quality".into(),
            event_type: "PostToolUse".into(),
            matcher: "Write|Edit".into(),
            hook_type: "command".into(),
            script_ext: "sh".into(),
            compatible_clis: vec!["claude".into(), "qoder".into(), "codex".into()],
            platforms: vec!["unix".into()],
            tags: None,
            installed: None,
        },
        StoreHook {
            id: "auto-lint".into(),
            name: "自动 Lint 检查".into(),
            description: "写文件后自动运行 lint，stderr 注入 Claude 上下文，AI 自动修复 lint 问题。".into(),
            category: "quality".into(),
            event_type: "PostToolUse".into(),
            matcher: "Write|Edit".into(),
            hook_type: "command".into(),
            script_ext: "sh".into(),
            compatible_clis: vec!["claude".into(), "qoder".into(), "codex".into()],
            platforms: vec!["unix".into()],
            tags: None,
            installed: None,
        },
        StoreHook {
            id: "todo-fixme-collector".into(),
            name: "TODO/FIXME 收集器".into(),
            description: "AI 写代码文件后自动扫描新增的 TODO/FIXME/HACK/XXX 标记，汇总到上下文让你始终知道哪些是临时方案。".into(),
            category: "quality".into(),
            event_type: "PostToolUse".into(),
            matcher: "Write|Edit".into(),
            hook_type: "command".into(),
            script_ext: "sh".into(),
            compatible_clis: vec!["claude".into(), "qoder".into(), "codex".into()],
            platforms: vec!["unix".into()],
            tags: None,
            installed: None,
        },
        StoreHook {
            id: "block-bypass-comments".into(),
            name: "阻断绕过检查的注释".into(),
            description: "阻止 AI 使用 # noqa、// @ts-ignore、eslint-disable、@pytest.mark.skip 等绕过检查的注释。强制修复底层问题而非绕过。".into(),
            category: "quality".into(),
            event_type: "PreToolUse".into(),
            matcher: "Write|Edit".into(),
            hook_type: "command".into(),
            script_ext: "py".into(),
            compatible_clis: vec!["claude".into(), "qoder".into(), "codex".into()],
            platforms: vec!["unix".into()],
            tags: None,
            installed: None,
        },
        StoreHook {
            id: "stop-quality-gate".into(),
            name: "回合结束质量门禁".into(),
            description: "每回合结束时运行 tsc/lint/test，有错误则 exit 2 注入 Claude 形成自愈循环（AI 自动修复→再检查→直到通过）。".into(),
            category: "quality".into(),
            event_type: "Stop".into(),
            matcher: "".into(),
            hook_type: "command".into(),
            script_ext: "sh".into(),
            compatible_clis: vec!["claude".into(), "qoder".into(), "codex".into()],
            platforms: vec!["unix".into()],
            tags: None,
            installed: None,
        },
        // ── Automation ────────────────────────────────────────
        StoreHook {
            id: "auto-commit".into(),
            name: "自动 Git 提交".into(),
            description: "回合结束时自动 git add -A && git commit，适合无人值守长任务。commit 消息含时间戳。".into(),
            category: "automation".into(),
            event_type: "Stop".into(),
            matcher: "".into(),
            hook_type: "command".into(),
            script_ext: "sh".into(),
            compatible_clis: vec!["claude".into(), "qoder".into(), "codex".into()],
            platforms: vec!["unix".into()],
            tags: None,
            installed: None,
        },
        StoreHook {
            id: "auto-stage".into(),
            name: "自动 Git Add".into(),
            description: "AI 编辑文件后自动 git add 该文件。比 auto-commit 更温和——只暂存不提交，让你在提交前人工 review。适合与 auto-commit 搭配使用。".into(),
            category: "automation".into(),
            event_type: "PostToolUse".into(),
            matcher: "Edit|Write".into(),
            hook_type: "command".into(),
            script_ext: "sh".into(),
            compatible_clis: vec!["claude".into(), "qoder".into(), "codex".into()],
            platforms: vec!["unix".into()],
            tags: None,
            installed: None,
        },
        // ── Session ───────────────────────────────────────────
        StoreHook {
            id: "context-monitor".into(),
            name: "上下文用量监控".into(),
            description: "跟踪工具调用次数，接近上限时警告并建议 /compact 或保存 checkpoint，防止上下文溢出。".into(),
            category: "session".into(),
            event_type: "PostToolUse".into(),
            matcher: "".into(),
            hook_type: "command".into(),
            script_ext: "sh".into(),
            compatible_clis: vec!["claude".into(), "qoder".into(), "codex".into()],
            platforms: vec!["unix".into()],
            tags: None,
            installed: None,
        },
        StoreHook {
            id: "session-context".into(),
            name: "会话注入项目规范".into(),
            description: "会话启动时自动读取 AGENTS.md/CONTRIBUTING.md，通过 additionalContext 注入给 AI，确保 AI 遵循项目规范。".into(),
            category: "session".into(),
            event_type: "SessionStart".into(),
            matcher: "startup|resume".into(),
            hook_type: "command".into(),
            script_ext: "sh".into(),
            compatible_clis: vec!["claude".into(), "qoder".into(), "codex".into()],
            platforms: vec!["unix".into()],
            tags: None,
            installed: None,
        },
        StoreHook {
            id: "git-status-inject".into(),
            name: "Git 状态注入".into(),
            description: "会话启动时注入当前分支、变更文件、最近 commit 历史给 AI，让 AI 在动手前就了解项目最新状态和上下文。".into(),
            category: "session".into(),
            event_type: "SessionStart".into(),
            matcher: "startup|resume".into(),
            hook_type: "command".into(),
            script_ext: "sh".into(),
            compatible_clis: vec!["claude".into(), "qoder".into(), "codex".into()],
            platforms: vec!["unix".into()],
            tags: None,
            installed: None,
        },
        // ── Notification ──────────────────────────────────────
        StoreHook {
            id: "notify-on-stop".into(),
            name: "任务完成桌面通知".into(),
            description: "回合结束时发送桌面通知。适合长任务—AI 在工作，你去喝咖啡，完成时通知你回来。macOS/Linux 自适应。".into(),
            category: "notification".into(),
            event_type: "Stop".into(),
            matcher: "".into(),
            hook_type: "command".into(),
            script_ext: "sh".into(),
            compatible_clis: vec!["claude".into(), "qoder".into(), "codex".into()],
            platforms: vec!["unix".into()],
            tags: None,
            installed: None,
        },
        // ── Java ──────────────────────────────────────────────────
        StoreHook {
            id: "java-spotless-format".into(),
            name: "Java 自动格式化 (Spotless)".into(),
            description: "写 .java 文件后自动 mvn spotless:apply 或 gradlew spotlessApply。自动检测模块目录，仅格式化变更所在的 Maven/Gradle 模块。".into(),
            category: "quality".into(),
            event_type: "PostToolUse".into(),
            matcher: "Write|Edit".into(),
            hook_type: "command".into(),
            script_ext: "sh".into(),
            compatible_clis: vec!["claude".into(), "qoder".into(), "codex".into()],
            platforms: vec!["unix".into()],
            tags: Some(vec!["java".into()]),
            installed: None,
        },
        StoreHook {
            id: "java-compile-check".into(),
            name: "Java 编译检查".into(),
            description: "写 .java 文件后自动 mvn compile -q，编译错误注入 Claude 上下文。Maven/Gradle 双支持。".into(),
            category: "quality".into(),
            event_type: "PostToolUse".into(),
            matcher: "Write|Edit".into(),
            hook_type: "command".into(),
            script_ext: "sh".into(),
            compatible_clis: vec!["claude".into(), "qoder".into(), "codex".into()],
            platforms: vec!["unix".into()],
            tags: Some(vec!["java".into()]),
            installed: None,
        },
        StoreHook {
            id: "java-checkstyle-lint".into(),
            name: "Java Checkstyle 检查".into(),
            description: "写 .java 后自动 mvn checkstyle:check，命名规范/Javadoc/设计规范问题注入 Claude。".into(),
            category: "quality".into(),
            event_type: "PostToolUse".into(),
            matcher: "Write|Edit".into(),
            hook_type: "command".into(),
            script_ext: "sh".into(),
            compatible_clis: vec!["claude".into(), "qoder".into(), "codex".into()],
            platforms: vec!["unix".into()],
            tags: Some(vec!["java".into()]),
            installed: None,
        },
        StoreHook {
            id: "java-quality-gate".into(),
            name: "Java 回合结束质量门禁".into(),
            description: "每回合依次执行 spotless:check → compile → test。有错误则 exit 2 注入 Claude 自愈。与前端版 stop-quality-gate 类似，但用 Maven/Gradle 命令。".into(),
            category: "quality".into(),
            event_type: "Stop".into(),
            matcher: "".into(),
            hook_type: "command".into(),
            script_ext: "sh".into(),
            compatible_clis: vec!["claude".into(), "qoder".into(), "codex".into()],
            platforms: vec!["unix".into()],
            tags: Some(vec!["java".into()]),
            installed: None,
        },
        StoreHook {
            id: "java-spring-context".into(),
            name: "Java Spring Boot 上下文注入".into(),
            description: "会话启动时自动检测 Java/Spring Boot 版本、构建工具，通过 additionalContext 注入给 AI。提醒用 ./mvnw、编辑后 compile、提交前 test。".into(),
            category: "session".into(),
            event_type: "SessionStart".into(),
            matcher: "startup|compact".into(),
            hook_type: "command".into(),
            script_ext: "sh".into(),
            compatible_clis: vec!["claude".into(), "qoder".into(), "codex".into()],
            platforms: vec!["unix".into()],
            tags: Some(vec!["java".into()]),
            installed: None,
        },
        StoreHook {
            id: "java-enforce-wrapper".into(),
            name: "Java 强制 Maven/Gradle Wrapper".into(),
            description: "项目有 mvnw/gradlew 但 AI 直接用了 mvn/gradle，阻断并提示使用 ./mvnw 或 ./gradlew。防止环境不一致。".into(),
            category: "security".into(),
            event_type: "PreToolUse".into(),
            matcher: "Bash".into(),
            hook_type: "command".into(),
            script_ext: "sh".into(),
            compatible_clis: vec!["claude".into(), "qoder".into(), "codex".into()],
            platforms: vec!["unix".into()],
            tags: Some(vec!["java".into()]),
            installed: None,
        },
        // ── Python ─────────────────────────────────────────────
        StoreHook {
            id: "python-typecheck".into(),
            name: "Python 类型检查 (mypy)".into(),
            description: "写 .py 文件后自动 mypy 检查，类型错误注入 Claude 上下文供自动修复。优先使用 pyproject.toml 中的 mypy 配置。".into(),
            category: "quality".into(),
            event_type: "PostToolUse".into(),
            matcher: "Write|Edit".into(),
            hook_type: "command".into(),
            script_ext: "sh".into(),
            compatible_clis: vec!["claude".into(), "qoder".into(), "codex".into()],
            platforms: vec!["unix".into()],
            tags: Some(vec!["python".into()]),
            installed: None,
        },
        // ── TypeScript ─────────────────────────────────────────
        StoreHook {
            id: "typescript-typecheck".into(),
            name: "TypeScript 类型检查 (tsc)".into(),
            description: "写 .ts/.tsx 后自动 tsc --noEmit，大型项目自动启用 --incremental 加速。类型错误注入 Claude 上下文。".into(),
            category: "quality".into(),
            event_type: "PostToolUse".into(),
            matcher: "Write|Edit".into(),
            hook_type: "command".into(),
            script_ext: "sh".into(),
            compatible_clis: vec!["claude".into(), "qoder".into(), "codex".into()],
            platforms: vec!["unix".into()],
            tags: Some(vec!["typescript".into(), "frontend".into()]),
            installed: None,
        },
        // ── Rust ───────────────────────────────────────────────
        StoreHook {
            id: "rust-clippy-check".into(),
            name: "Rust Clippy 检查".into(),
            description: "写 .rs 文件后自动 cargo clippy，lint 建议注入 Claude 上下文。帮助 AI 遵循 Rust 最佳实践和惯用写法。".into(),
            category: "quality".into(),
            event_type: "PostToolUse".into(),
            matcher: "Write|Edit".into(),
            hook_type: "command".into(),
            script_ext: "sh".into(),
            compatible_clis: vec!["claude".into(), "qoder".into(), "codex".into()],
            platforms: vec!["unix".into()],
            tags: Some(vec!["rust".into()]),
            installed: None,
        },
        // ── Go ─────────────────────────────────────────────────
        StoreHook {
            id: "go-vet-check".into(),
            name: "Go 静态检查 (go vet)".into(),
            description: "写 .go 文件后自动 go vet 检查文件所在包，检查结果注入 Claude 上下文。".into(),
            category: "quality".into(),
            event_type: "PostToolUse".into(),
            matcher: "Write|Edit".into(),
            hook_type: "command".into(),
            script_ext: "sh".into(),
            compatible_clis: vec!["claude".into(), "qoder".into(), "codex".into()],
            platforms: vec!["unix".into()],
            tags: Some(vec!["go".into()]),
            installed: None,
        },
    ]
}

// ── Hook Scripts ─────────────────────────────────────────────────

fn get_script(hook_id: &str) -> Option<&'static str> {
    match hook_id {
        // Generic
        "block-dangerous-commands" => {
            Some(include_str!("../hook_scripts/block-dangerous-commands.sh"))
        }
        "protect-sensitive-files" => {
            Some(include_str!("../hook_scripts/protect-sensitive-files.sh"))
        }
        "secret-scan" => Some(include_str!("../hook_scripts/secret-scan.py")),
        "auto-format" => Some(include_str!("../hook_scripts/auto-format.sh")),
        "auto-lint" => Some(include_str!("../hook_scripts/auto-lint.sh")),
        "stop-quality-gate" => Some(include_str!("../hook_scripts/stop-quality-gate.sh")),
        "auto-commit" => Some(include_str!("../hook_scripts/auto-commit.sh")),
        "context-monitor" => Some(include_str!("../hook_scripts/context-monitor.sh")),
        "session-context" => Some(include_str!("../hook_scripts/session-context.sh")),
        "notify-on-stop" => Some(include_str!("../hook_scripts/notify-on-stop.sh")),
        // New general hooks
        "block-main-branch-edits" => {
            Some(include_str!("../hook_scripts/block-main-branch-edits.sh"))
        }
        "auto-stage" => Some(include_str!("../hook_scripts/auto-stage.sh")),
        "git-status-inject" => Some(include_str!("../hook_scripts/git-status-inject.sh")),
        "todo-fixme-collector" => Some(include_str!("../hook_scripts/todo-fixme-collector.sh")),
        "block-bypass-comments" => Some(include_str!("../hook_scripts/block-bypass-comments.py")),
        // Language-specific
        "python-typecheck" => Some(include_str!("../hook_scripts/python-typecheck.sh")),
        "typescript-typecheck" => Some(include_str!("../hook_scripts/typescript-typecheck.sh")),
        "rust-clippy-check" => Some(include_str!("../hook_scripts/rust-clippy-check.sh")),
        "go-vet-check" => Some(include_str!("../hook_scripts/go-vet-check.sh")),
        // Java
        "java-spotless-format" => {
            Some(include_str!("../hook_scripts/java/java-spotless-format.sh"))
        }
        "java-compile-check" => Some(include_str!("../hook_scripts/java/java-compile-check.sh")),
        "java-quality-gate" => Some(include_str!("../hook_scripts/java/java-quality-gate.sh")),
        "java-spring-context" => Some(include_str!("../hook_scripts/java/java-spring-context.sh")),
        "java-enforce-wrapper" => {
            Some(include_str!("../hook_scripts/java/java-enforce-wrapper.sh"))
        }
        "java-checkstyle-lint" => {
            Some(include_str!("../hook_scripts/java/java-checkstyle-lint.sh"))
        }
        _ => None,
    }
}

/// Repair missing hook scripts referenced by installed hooks.
///
/// Scans all installed hooks (user + every project), extracts script filenames
/// from hook commands, and writes any missing scripts from the embedded copies.
pub fn repair_missing_hook_scripts() {
    let hooks_dir = hooks_dir();
    let data = hook_manager::scan_hooks(None);

    for hook in &data.hooks {
        // Extract the script filename from the hook command
        // e.g. "/Users/xxx/.kn/hooks/context-monitor.sh" → "context-monitor"
        let cmd = hook.command.trim();
        let script_name = std::path::Path::new(cmd)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("");

        if script_name.is_empty() {
            continue;
        }

        let ext = std::path::Path::new(cmd)
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("sh");

        let script_path = hooks_dir.join(format!("{}.{}", script_name, ext));

        if script_path.exists() {
            continue;
        }

        // Try to restore from embedded copy
        if let Some(content) = get_script(script_name) {
            let _ = std::fs::create_dir_all(&hooks_dir);
            if std::fs::write(&script_path, content).is_ok() {
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    if let Ok(meta) = std::fs::metadata(&script_path) {
                        let mut perms = meta.permissions();
                        perms.set_mode(0o755);
                        let _ = std::fs::set_permissions(&script_path, perms);
                    }
                }
                eprintln!("[kn] Repaired missing hook script: {}", script_path.display());
            }
        }
    }
}

// ── List command ────────────────────────────────────────────────

#[tauri::command]
pub fn list_store_hooks() -> HookStoreData {
    // Check which hooks are already installed for each CLI
    let existing = hook_manager::scan_hooks(None);
    let mut hooks = all_hooks();

    for hook in &mut hooks {
        let mut installed = Vec::new();
        for cli in &hook.compatible_clis {
            let has = existing.hooks.iter().any(|h| {
                h.cli == *cli
                    && h.event_type == hook.event_type
                    && h.command
                        .contains(&format!(".kn/hooks/{}", hook.id))
            });
            if has {
                installed.push(cli.clone());
            }
        }
        if !installed.is_empty() {
            hook.installed = Some(installed);
        }
    }

    HookStoreData { hooks }
}

// ── Install command ─────────────────────────────────────────────

#[tauri::command]
pub fn install_store_hook(hook_id: String, cli: String) -> Result<(), String> {
    let hooks = all_hooks();
    let hook = hooks
        .iter()
        .find(|h| h.id == hook_id)
        .ok_or_else(|| format!("未找到 hook: {}", hook_id))?;

    if !hook.compatible_clis.contains(&cli) {
        return Err(format!("Hook '{}' 不兼容 CLI '{}'", hook_id, cli));
    }

    let current_os = "unix";
    if !hook.platforms.iter().any(|p| p == current_os) {
        return Err(format!(
            "Hook '{}' 不支持当前平台（仅支持 {}）",
            hook_id,
            hook.platforms.join(", ")
        ));
    }

    let script = get_script(&hook_id).ok_or_else(|| format!("hook 脚本不存在: {}", hook_id))?;

    // Ensure hooks directory exists
    let dir = hooks_dir();
    fs::create_dir_all(&dir).map_err(|e| format!("创建目录失败: {}", e))?;

    // Write script to ~/.kn/hooks/<hook_id>.<ext>
    let script_path = dir.join(format!("{}.{}", hook_id, hook.script_ext));
    fs::write(&script_path, script).map_err(|e| format!("写入脚本失败: {}", e))?;

    // Make executable on Unix
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(meta) = fs::metadata(&script_path) {
            let mut perms = meta.permissions();
            perms.set_mode(0o755);
            let _ = fs::set_permissions(&script_path, perms);
        }
    }

    // Build the command string that the hook config will reference.
    let cmd = script_path.to_string_lossy().to_string();

    // Check if already installed — prevent duplicate hook entries.
    // Scan existing hooks and look for a match on event_type + matcher + same script path.
    let existing = hook_manager::scan_hooks(None);
    let cmd_pattern = format!(".kn/hooks/{}", hook_id);
    let already_installed = existing.hooks.iter().any(|h| {
        h.cli == cli
            && h.event_type == hook.event_type
            && h.command.contains(&cmd_pattern)
    });
    if already_installed {
        return Ok(()); // idempotent — already installed, nothing to do
    }

    // Determine config path and write hook entry
    let home_path = crate::home_dir();

    match cli.as_str() {
        "claude" => {
            let path = home_path.join(".claude").join("settings.json");
            hook_manager::create_json_hook(
                &path,
                &hook.event_type,
                &hook.matcher,
                &cmd,
                &hook.hook_type,
            )?;
        }
        "qoder" => {
            let path = home_path.join(".qoder-cn").join("settings.json");
            hook_manager::create_json_hook(
                &path,
                &hook.event_type,
                &hook.matcher,
                &cmd,
                &hook.hook_type,
            )?;
        }
        "codex" => {
            hook_manager::create_codex_hook_new(
                &hook.event_type,
                &hook.matcher,
                &cmd,
                &hook.hook_type,
            )?;
        }
        _ => return Err(format!("不支持的 CLI: {}", cli)),
    }

    // Save metadata (name + description) for the newly installed hook.
    // Find the hook we just created by matching the command path, then persist.
    let scan_data = hook_manager::scan_hooks(None);
    if let Some(entry) = scan_data
        .hooks
        .iter()
        .find(|h| h.cli == cli && h.command == cmd)
    {
        let _ = hook_meta::set_hook_meta(
            cli,
            entry.event_type.clone(),
            entry.group_idx,
            entry.hook_idx,
            hook.name.clone(),
            Some(hook.description.clone()),
        );
    }

    Ok(())
}

// ── Uninstall command ────────────────────────────────────────────

#[tauri::command]
pub fn uninstall_store_hook(hook_id: String, cli: String) -> Result<(), String> {
    let hooks = all_hooks();
    let _hook = hooks
        .iter()
        .find(|h| h.id == hook_id)
        .ok_or_else(|| format!("未找到 hook: {}", hook_id))?;

    // Find and delete the hook entry from the CLI config
    let scan_data = hook_manager::scan_hooks(None);
    let cmd_pattern = format!(".kn/hooks/{}", hook_id);

    let matching: Vec<_> = scan_data
        .hooks
        .iter()
        .filter(|h| h.cli == cli && h.command.contains(&cmd_pattern))
        .collect();

    if matching.is_empty() {
        return Err(format!("Hook '{}' 未在 {} 中安装", hook_id, cli));
    }

    let mut errors = Vec::new();
    for entry in matching {
        if let Err(e) = hook_manager::delete_hook(
            cli.clone(),
            entry.event_type.clone(),
            entry.group_idx,
            entry.hook_idx,
            entry.path.clone(),
        ) {
            errors.push(e);
        }
    }

    if !errors.is_empty() {
        return Err(format!("删除失败: {}", errors.join("; ")));
    }

    // Remove the script file
    let dir = hooks_dir();
    for ext in &["sh", "py"] {
        let script_path = dir.join(format!("{}.{}", hook_id, ext));
        let _ = fs::remove_file(&script_path);
    }

    Ok(())
}

