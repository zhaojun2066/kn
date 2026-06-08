# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Build & Run Commands

```bash
# Development (hot-reload frontend, watch Rust)
npm run tauri dev

# Type check only
npx tsc --noEmit

# Build frontend only
npx vite build

# Build Rust only
cd src-tauri && cargo check

# Full build check (all three)
cd /Users/zhaojun/workspace/me/shark/kn/desktop && npx tsc --noEmit && npx vite build && cd src-tauri && cargo check
```

**Important**: `tauri dev` requires a full restart (Ctrl+C then re-run) to pick up changes to `tauri.conf.json`, `capabilities/default.json`, or Rust source files. Hot reload only covers frontend TSX/CSS changes.

## Architecture

### App Overview
AI Profile Manager — a Tauri v2 desktop app (React + Tailwind + TypeScript frontend, Rust backend). Manages environment variable profiles for Claude Code and Codex CLI tools. Each profile is a named set of `KEY=VALUE` pairs (API keys, base URLs, model names). Users create profiles, then launch `ai claude <profile>` or `ai codex <profile>` directly from the embedded xterm.js terminal.

### Key Technology Stack
- **Desktop shell**: Tauri v2 with `tauri-plugin-shell` (for `Command.create` from frontend), `tauri-plugin-dialog` (file open/save dialogs), `tauri-plugin-fs`, `tauri-plugin-updater`
- **Terminal**: xterm.js (`@xterm/xterm` + `@xterm/addon-fit` + `@xterm/addon-search`) with **canvas renderer** (WebGL disabled — caused rendering glitches with CJK + box-drawing), `portable-pty` crate in Rust for real PTY-backed shell sessions. **Two independent terminal panels** coexist:
  - **Right terminal** (`panelId="right"`): opens on the right side when user clicks「运行」on a profile. Width-based sizing.
  - **Bottom terminal** (`panelId="bottom"`): VS Code-style bottom panel, toggled via toolbar button or Ctrl+`. Height-based sizing.
  - Each has its own tabs, history, PTY sessions — fully independent.
- **PTY data flow**: Rust spawns `zsh -i -l` (login + interactive) via `portable-pty`, uses Tauri `Channel<PtyEvent>` to stream stdout/stderr to the frontend, frontend `invoke("write_pty", {sessionId, data})` to send keystrokes
- **Profile storage**: Single YAML config at `~/.claude-profiles/config.yaml`. The Tauri backend reads/writes it directly via `serde_yaml`. The shell wrapper (`~/.claude-profiles/shell-rc`) reads it with `sed` for zero-dependency profile injection in the terminal. **No separate dev/prod config — one file for all modes.**

### Directory Structure
```
desktop/
├── src/                    # React frontend
│   ├── App.tsx             # Top-level layout: Toolbar | (Sidebar + MainPanel + BottomTerm) vs RightTerm | StatusBar
│   ├── components/
│   │   ├── Toolbar.tsx     # Add/delete/copy/scan/refresh/export/import profile, theme, terminal toggle (toggles bottom terminal)
│   │   ├── Sidebar.tsx     # Profile list with search, sort, right-click menu, CLI type icons
│   │   ├── MainPanel.tsx   # Profile detail: env var table + command reference block + session history
│   │   ├── TerminalPanel.tsx # Tab bar + xterm terminals + work dir + history dropdown. mode="right"|"bottom" adapts sizing
│   │   ├── XTerm.tsx       # xterm.js + FitAddon + SearchAddon wrapper (forwardRef, canvas renderer)
│   │   ├── ProfileDialog.tsx # 4-step create wizard: name → CLI type → env vars → done
│   │   ├── ImportPreview.tsx  # JSON import preview with CLI detection
│   │   ├── ScanPreview.tsx    # System scan results with checkboxes + editable names
│   │   ├── EnvVarTable.tsx / EnvVarRow.tsx  # Editable env var table with secret masking
│   │   ├── ConfirmDialog.tsx / NameDialog.tsx / ShortcutsPanel.tsx / ErrorBoundary.tsx
│   │   └── common/         # Button, Badge, SearchInput, CLIIcon
│   ├── hooks/
│   │   ├── useTerminal.ts  # Multi-instance PTY session management: useTerminal("right") | useTerminal("bottom")
│   │   ├── useProfiles.ts  # Profile CRUD via Tauri IPC
│   │   └── useTheme.ts     # light/dark/system theme toggle
│   ├── lib/
│   │   ├── types.ts        # ProfileSummary, ProfileDetail, TerminalSession, SessionRecord
│   │   └── tauri-api.ts    # Tauri invoke wrappers for profile commands
│   └── styles/globals.css  # CSS variables for light/dark themes
├── src-tauri/
│   ├── src/
│   │   ├── lib.rs          # Tauri builder, plugin registration, all command handlers
│   │   ├── commands.rs     # Profile CRUD commands, file I/O, system scan, config, update helpers
│   │   ├── profile_cmd.rs  # Wrapper around the Python `profile` CLI — calls profile via subprocess
│   │   └── pty.rs          # PTY spawn/write/resize/kill using portable-pty + Channel streaming
│   ├── Cargo.toml          # portable-pty, tauri-plugin-shell/dialog/fs/updater, libc
│   ├── tauri.conf.json     # Window config, shell scope (allowed commands), updater config
│   └── capabilities/default.json  # Tauri v2 permissions (shell:allow-spawn/execute with cmd scoping)
├── update/
│   ├── update.json         # User's update URL config
│   └── demo.json           # Full-platform update manifest example
└── package.json
```

### Shell Plugin Scope (Critical for Debugging)
When adding new shell commands (`Command.create` from frontend or `std::process::Command` from Rust), the command binary MUST be allowed in either:
1. `capabilities/default.json` — under `shell:allow-execute` and `shell:allow-spawn` permission objects with `{ "name": "...", "cmd": "/full/path", "args": true }`
2. Custom Rust commands (`#[tauri::command]`) have full system access and don't need shell scope

### Terminal Data Flow
```
User types → xterm.onData → invoke("write_pty", {sessionId, data})
  → Rust write_pty → PTY stdin
  → Shell output → PTY stdout
  → Rust reader thread → Channel.send(PtyEvent::Data)
  → Frontend Channel.onmessage → RAF-batched write (accumulate within frame)
  → term.write(data)  ← single flush per animation frame
```

PTY output is **RAF-batched** in `useTerminal.ts`: data from multiple IPC messages within the same animation frame is accumulated into a buffer and written to xterm.js in a single `term.write()` call. This prevents parser overload when the PTY produces many small chunks rapidly (e.g. Claude Code TUI streaming with ANSI escape sequences).

### Dual Terminal Instances
`App.tsx` creates two independent `useTerminal` instances:
```ts
const rightTerminal = useTerminal("right");   // profile「运行」→ 右侧面板
const bottomTerminal = useTerminal("bottom"); // 工具栏按钮 → 底部面板 (VS Code 风格)
```

| Aspect | Right Terminal | Bottom Terminal |
|--------|---------------|-----------------|
| Trigger | Profile「运行」button | Toolbar terminal toggle / Ctrl+` |
| Layout | Right side of MainPanel | Below MainPanel (VS Code panel) |
| Sizing | Width-based (min 480px) | Height-based (min 120px) |
| Resize | Horizontal drag (cursor-col-resize) | Vertical drag (cursor-row-resize) |
| Maximize | Hides Sidebar+MainPanel+Bottom | Hides Sidebar+MainPanel+Right |
| localStorage | `kn-terminal-right-*` | `kn-terminal-bottom-*` |

Each instance has its own tabs, history, PTY sessions — fully independent.
`TerminalPanel` adapts via `mode="right"|"bottom"` prop (border direction, sizing, etc.).

### PTY Resize Chain
```
Container resize → ResizeObserver → RAF coalescing → fit()
  → fitAddon.fit() adjusts xterm.js cols/rows
  → term.refresh(0, rows-1) force re-renders viewport (prevents cursor drift after shrink+expand)
  → onResize callback → handleTerminalResize
  → invoke("resize_pty", {sessionId, cols, rows})
  → Rust: ioctl(TIOCSWINSZ) on PTY master fd → kernel sends SIGWINCH to child
```
**Important**: `term.onResize` is intentionally NOT used for PTY resize — it fires internally during `fitAddon.fit()` and would cause a duplicate `resize_pty` call.

### Font Rendering & Resize
Uses **canvas renderer** (default) for maximum stability — WebGL was disabled because it caused rendering glitches with CJK fonts, box-drawing characters (`╭─│├`), and Braille spinners (`⠋⠙⠹`) heavily used by Claude Code's TUI. The canvas renderer supports the same Unicode characters.

Font stack: `ui-monospace, SF Mono, Cascadia Code, Menlo, Monaco, JetBrains Mono, PingFang SC, Microsoft YaHei, Noto Sans CJK SC, Consolas, Courier New, monospace`.

Font size changes force an XTerm remount (xterm.js doesn't support hot-reloading `fontSize`). The PTY session stays alive; `resize_pty` (via fit → onResize) sends SIGWINCH to the child, triggering TUI redraw. **No ANSI text replay** — raw escape sequences replayed into a fresh terminal would corrupt the display.

`FitAddon` handles automatic terminal resizing with RAF (requestAnimationFrame) coalescing — no artificial delay. PTY resize signal is sent immediately in the same frame.

### Profile Config & Shell Wrapper
- Config file: `~/.claude-profiles/config.yaml`. Single source of truth — no separate dev/prod split.
- Shell wrapper: `~/.claude-profiles/shell-rc`, sourced from `.zshrc`. Defines `ai()` function for `ai claude <profile>` / `ai codex <profile>`.
- `_profile_env()` in shell-rc reads config via `sed` (zero dependencies, works even with limited PATH).
- `ensure_shell_rc` (Rust) writes the embedded shell-rc to disk on app startup, ensuring it's always up to date.

### Version & Updates
- App version is in `tauri.conf.json` (both `version` field and `bundle.version`)
- Update check: reads `update/update.json` → fetches manifest from `update_url` → compares versions → downloads via curl → verifies SHA256 via shasum → opens installer
- All update HTTP operations use absolute paths for system binaries: `/usr/bin/curl`, `/usr/bin/shasum`, `/usr/bin/open`

## CLI Config Directory Conventions

Each supported CLI tool uses different config directory names for user-level vs project-level:

| CLI | User-level | Project-level | Notes |
|-----|-----------|---------------|-------|
| **Claude Code** | `~/.claude/` | `<project>/.claude/` | Same name for both scopes |
| **Codex** | `~/.codex/` | `<project>/.codex/` | Same name for both scopes |
| **Qoder (国产版)** | `~/.qoder-cn/` | `<project>/.qoder/` | **Different names!** User: `.qoder-cn`, Project: `.qoder` |

### Qoder path details

- **User-level**: `~/.qoder-cn/` — all user config (settings.json, agents/, skills/, commands/)
- **Project-level**: `<project>/.qoder/` — all project config (settings.json, agents/, skills/)
- **Key rule**: 用户级全是 `.qoder-cn`，项目级全是 `.qoder`，没有任何例外。
- **Frontend path logic**:
  - `getHookTargetPath`: project scope + qoder → `project.path/.qoder/settings.json`; user scope → `~/.qoder-cn/settings.json`
  - `toScope === "project" && cli === "qoder"` → `${project.path}/.qoder/${subdir}`
  - Detecting CLI from source path: check both `.qoder-cn/` (user) and `.qoder/` (project)

## Cross-Platform Compatibility

The app targets **macOS Intel**, **macOS Apple Silicon**, **Windows**, and **Linux**. Platform differences are handled in these key areas:

### System Binary Resolution (`commands.rs`)
The `find_binary()` function tries multiple paths per OS before falling back to the bare command name:
- macOS: `/usr/bin/<name>` → `/opt/homebrew/bin/<name>` → `/usr/local/bin/<name>`
- Linux: `/usr/bin/<name>` → `/bin/<name>`
- Windows: bare command name only

### Platform-Specific Commands
| Operation | macOS | Linux | Windows |
|-----------|-------|-------|---------|
| HTTP fetch | `curl -sL` | `curl -sL` | `curl -sL` |
| SHA256 verify | `/usr/bin/shasum -a 256` | `sha256sum` or `shasum` | `certutil -hashfile SHA256` |
| Open file/folder | `/usr/bin/open` | `xdg-open` | `cmd /c start` |
| PTY default shell | `/bin/zsh` | `/bin/bash` | `powershell.exe` |

### Shell Scope (`capabilities/default.json`)
Shell plugin commands need entries in both `shell:allow-execute` and `shell:allow-spawn` permission objects. Each entry specifies `cmd` (full path) and `args: true`. Platform-specific entries coexist — the first matching one on the current OS is used. Key entries include: `curl`, `shasum`, `sha256sum` (Linux), `certutil` (Windows), `open` (macOS), `xdg-open` (Linux), `brew` (macOS, both Apple Silicon `/opt/homebrew/bin/brew` and Intel `/usr/local/bin/brew`), `apt-get` (Linux), `winget` (Windows).

### Font Stack (`XTerm.tsx`)
Starts with system UI monospace, then falls through platform-specific fonts:
```
ui-monospace → SF Mono (macOS) → Cascadia Code (Windows) → Menlo → Monaco → JetBrains Mono → Fira Code → Consolas → Courier New → monospace
```

### Home Directory (`commands.rs`)
`home_dir()` checks `HOME` env var first (Unix), then `USERPROFILE` (Windows), falls back to `"~"`. Used for scanning `~/.claude/settings.json` and `~/.codex/` config files.

### PTY Resize (`pty.rs`)
- Unix (macOS/Linux): `ioctl(TIOCSWINSZ)` on the PTY master fd, kernel automatically sends SIGWINCH to child process
- Windows: `MasterPty::resize()` from `portable-pty` crate handles ConPTY resize

### Profile CLI Paths
The `profile` CLI is installed at `~/.claude-profiles/bin/profile`. Rust `find_profile_cli()` checks `PROFILE_CLI_PATH` env var first, then the default install location, then falls back to bare `profile` command on PATH.

### Environment Detection (`App.tsx`)
Startup checks use `bash -lc "command -v <name>"` to ensure the user's full shell PATH is available (Tauri GUI apps have a limited PATH by default). The `kn-env-checked` localStorage flag prevents repeated prompts after user dismissal.

## Known Pitfalls

### PTY Shell PATH 陷阱 — 生产环境 `command not found`

**现象**：dev 模式下终端里 `ai claude <profile>` 正常工作，但生产包（macOS `.app` / Linux `.AppImage`）里同样的命令报 `command not found: claude`。

**根因**：

1. **Tauri GUI 进程**（`.app` 启动）的 PATH 极简：macOS 为 `/usr/bin:/bin:/usr/sbin:/sbin`，不含 Homebrew 等用户安装的工具路径
2. `pty.rs` 中 `for (k, v) in std::env::vars()` 会把这份受限 PATH 复制给 PTY shell
3. **dev 模式为什么没问题**：`npm run tauri dev` 从终端启动，进程继承了完整的用户 PATH，PTY 自然也拿到完整 PATH
4. 用户的 PATH 扩展（如 Homebrew `brew shellenv`）通常写在 `~/.zprofile`（macOS）或 `~/.profile`（Linux）中，这些文件**只有 login shell 才会 source**

**修复**：PTY 启动 shell 时必须同时传 `-i`（interactive）和 `-l`（login）两个标志：

```rust
// pty.rs — 正确做法
cmd.args(["-i", "-l"]);  // login + interactive
// 不要只传 -i：
// cmd.args(["-i"]);     // ❌ 不加载 .zprofile / .profile，生产环境 PATH 缺失
```

各平台 shell 初始化文件加载顺序：

| Shell 类型 | macOS (zsh) | Linux (bash) |
|-----------|-------------|-------------|
| `-i` only（interactive） | `.zshenv` → `.zshrc` | `.bashrc` |
| `-i -l`（login+interactive） | `.zshenv` → `.zprofile` → `.zshrc` → `.zlogin` | `.profile` → `.bashrc` |

**教训**：
- 不要假设 Tauri 进程的 `std::env::vars()` 包含完整用户环境——GUI 应用的 PATH 是不可靠的
- **永远用 `-i -l` 启动 PTY shell**，让 shell 按标准流程初始化用户环境
- 这与 macOS Terminal.app、iTerm2、GNOME Terminal 的行为一致——它们都默认启动 login shell
- 凡是调用外部命令（`claude`、`codex`、`brew`、`node` 等），如果这些命令的路径是用户级安装的，必须确保 login shell 已初始化

### PTY 终端无法输入 — 缺少 `TERM` 环境变量

**现象**：生产包（macOS `.app`）中打开内置终端后，键盘完全无法输入，终端像"死"了一样。dev 模式正常。

**根因**：

1. macOS `.app` 启动时，进程环境变量中 **没有 `TERM`**（这是终端模拟器特有的变量，GUI 应用不会有）
2. `pty.rs` 中 `for (k, v) in std::env::vars()` 复制了父进程的所有环境变量，但其中没有 `TERM`
3. **Shell 依赖 `TERM` 来确定终端能力**：zsh 用 `TERM` 来决定是否启用 ZLE（Zsh Line Editor），bash 用 `TERM` 来决定是否启用 readline
4. 没有 `TERM` 的 shell 可能退化到 "dumb" 模式，行编辑功能不可用，甚至完全不回显输入
5. **dev 模式为什么没问题**：`npm run tauri dev` 从终端启动，父进程有 `TERM=xterm-256color`，PTY 继承了这个值

**修复**：在复制父进程环境变量之后，**强制覆盖**终端必需的环境变量：

```rust
// pty.rs — 在 std::env::vars() 循环之后
// 父进程（GUI app）没有 TERM，我们明确知道运行在 xterm.js 兼容终端里
cmd.env("TERM", "xterm-256color");
cmd.env("COLORTERM", "truecolor");
```

**关键环境变量对比**：

| 变量 | Terminal.app | macOS .app | 作用 |
|------|-------------|------------|------|
| `TERM` | `xterm-256color` | **未设置** | 终端类型，决定行编辑/颜色/光标能力 |
| `COLORTERM` | `truecolor` | **未设置** | 24-bit 真彩色支持 |
| `SHELL` | `/bin/zsh` | 可能未设置 | 默认 shell（代码有 fallback 到 `/bin/zsh`） |
| `LANG` | `zh_CN.UTF-8` | 可能未设置 | 语言/编码（影响字符显示和排序） |

**教训**：
- GUI 应用进程的环境变量是"残缺的"——它只包含系统级变量，缺少终端特有的变量
- **永远不要假设 `std::env::vars()` 包含完整的终端环境**
- `TERM` 是终端模拟器的"身份证"，必须在 PTY 中显式设置
- 这个 **TERM 值必须与实际终端匹配**：xterm.js 兼容 xterm-256color，不要设为 `xterm-kitty` 或 `alacritty` 等不兼容的值
- 同样的问题也存在于 Linux（AppImage/flatpak）和 Windows（MSIX/nsis）

### Profile 名泄漏到 Claude/Codex 交互输入 — dev/prod config 分离的陷阱（已修复并合并）

**历史问题**：之前存在 `~/.claude-profiles/` 和 `~/.claude-profiles-dev/` 两套 config 目录。生产 App 写入 prod config，但 `_profile_env` 优先读 dev config。当 profile 只存在于 prod config 时找不到，profile 名泄漏为 claude CLI 参数。

**最终方案**：**合并为单一 config 目录** `~/.claude-profiles/config.yaml`。`config_dir()` 不再区分 debug/release，shell-rc 也只读这一个文件。从根本上消除了"该读哪个 config"的歧义。

```rust
// profile_cmd.rs — 统一路径，无分支
fn config_dir() -> PathBuf {
    let base = ".claude-profiles";  // 不再判断 cfg!(debug_assertions)
    ...
}
```
