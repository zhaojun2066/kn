# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

kn — a monorepo with three components sharing one data file (`~/.kn/config.yaml`):

| Component | Dir | Language | Role |
|-----------|-----|----------|------|
| CLI + Shell Wrapper | `bin/`, `lib/`, `shell/` | Python 3 + Bash | `profile` CLI + `ai()` shell function for env injection |
| Desktop App | `desktop/` | TypeScript + Rust (Tauri v2) | GUI with embedded PTY terminal |
| Product Site | `site/` | Vue 3 + TypeScript + Vite | Landing page + docs, deployed to GitHub Pages |

All three read/write the same `~/.kn/config.yaml`. The Python lib uses `fcntl.flock` for concurrent-write safety. The Rust backend reads/writes directly via `serde_yaml`.

## Build & Test Commands

```bash
# ── CLI (Python) ──
python3 bin/profile list                          # run CLI directly
PYTHONPATH=lib python3 -m pytest tests/ -v       # run all unit tests
bash -n install.sh                                # validate shell script syntax

# ── Desktop (Tauri) ──
cd desktop
npm run tauri dev                                 # dev mode (hot-reload frontend, watch Rust)
npx tsc --noEmit                                  # TypeScript type check
npx vite build                                    # build frontend
cd src-tauri && cargo check                       # check Rust
npx tsc --noEmit && npx vite build && cd src-tauri && cargo check  # full check

# ── Site ──
cd site
npm run dev                                       # dev server
npm run build                                     # production build
```

## Architecture

### Data Flow
```
                    ~/.kn/config.yaml  (single source of truth)
                         ↑ read/write ↑
        ┌────────────────┼────────────┼────────────────┐
        ▼                ▼            ▼                ▼
   bin/profile     lib/config.py  desktop/src-tauri   shell/ai-profile.sh
   (Python CLI)    (YAML+lock)    (Rust serde_yaml)   (Bash sed reader)
```

### CLI (Python)
- `bin/profile` — thin CLI wrapper, delegates to `lib/config.py`
- `lib/config.py` — hand-rolled YAML parser/formatter (no PyYAML dependency), file locking via `fcntl`, atomic write with tmp-file + fsync + rename
- `tests/test_config.py` — unit tests for YAML parsing, serialization, public API
- `tests/test_json_output.py` — integration tests for `--json` CLI flag

### Shell Wrapper
- `shell/ai-profile.sh` — installed to `~/.kn/shell-rc`, sourced from `.zshrc`/`.bashrc`
- Defines `ai()` function: `ai claude <profile>` / `ai codex <profile>`
- Reads config via `sed` (zero-dependency), injects env vars into subshell
- `install.sh` copies it to `~/.kn/shell-rc`

### Desktop App
See `desktop/CLAUDE.md` for full desktop architecture, PTY data flow, terminal panel system, and known pitfalls. Key pointers:

- **Two PTY terminals** coexist independently: Right (profile launch) and Bottom (VS Code-style toggle)
- **PTY writes**: RAF-batched via `useTerminal.ts` — data accumulated within a frame, flushed once via `requestAnimationFrame` to prevent parser overload
- **Renderer**: Canvas (not WebGL) — WebGL caused rendering glitches with CJK + box-drawing characters Claude Code uses
- **Font change**: XTerm remounts but no longer replays raw ANSI text (that corrupted the display)
- **Shell scope**: commands in `capabilities/default.json` must be declared in both `shell:allow-execute` and `shell:allow-spawn`

### Product Site
- `site/` — Vue 3 + Vite + Tailwind, hash-router
- `site/src/data/docs.ts` — documentation content as structured data
- Deployed via `.github/workflows/deploy-site.yml` → GitHub Pages

## CI/CD

- `.github/workflows/build-desktop.yml` — **tag-push trigger** (`v1.0.7` → auto build + release) with `workflow_dispatch` fallback; validates version, builds macOS (ARM + Intel), auto-generates release notes via git-cliff
- `.github/workflows/deploy-site.yml` — auto-deploys `site/` to GitHub Pages on push to main

## Release Process

发布由 Git tag 触发，构建和 release notes 完全自动化。**必须在 main 分支上操作，不要在功能分支上打 release tag。**

```bash
# 0. 确保在 main 分支且代码已合并
git checkout main
git pull origin main

# 1. 更新版本号（两个文件同步修改）
#    - desktop/src-tauri/tauri.conf.json  →  "version": "1.0.7"
#    - desktop/src-tauri/Cargo.toml       →  version = "1.0.7"

# 2. 提交版本升级
git add desktop/src-tauri/tauri.conf.json desktop/src-tauri/Cargo.toml
git commit -m "release: v1.0.7"

# 3. 打 annotated tag 并推送（推送 tag 即触发 CI 发布）
git tag -a v1.0.7 -m "v1.0.7"
git push origin main
git push origin v1.0.7

# 4. 在 https://github.com/zhaojun2066/kn/actions 查看构建进度
```

Release notes 由 [git-cliff](https://git-cliff.org) 根据 conventional commits 自动生成，按 Features / Bug Fixes / Refactoring / Miscellaneous 分组，每条带 commit 链接。
本地预览 release notes: `git cliff --unreleased`
完整发布指南（含功能分支开发 → merge → 发布全流程）见 `RELEASE.md`。

## Platform

kn targets macOS exclusively. Key platform assumptions:

- **PTY shell**: `/bin/zsh -i -l` (login + interactive). `pty.rs` ensures macOS GUI app PATH limitations are handled via login shell init.
- **Open file**: `/usr/bin/open`
- **File lock**: `fcntl.flock` (Python) + `fs2::lock_exclusive` (Rust)
- **Shell RC**: `.zshrc` + `.bashrc`
- **Config path**: `~/.kn/` (resolved via `HOME`)

Key implementation details:
- `commands.rs::find_binary()` — searches hardcoded macOS paths `/usr/bin`, `/opt/homebrew/bin`, `/usr/local/bin` → shell PATH fallback (`shell -lc "command -v"`) → bare command name
- `commands.rs::home_dir()` — checks `HOME`. **Always use this function — never inline `std::env::var("HOME")...`**
- `install.sh` + `ensure_shell_rc()` — configures both `.zshrc` and `.bashrc` idempotently
- `pty.rs` — spawns with `-i -l` (login + interactive); always ensures `TERM=xterm-256color`

## Sibling Repositories

Three repositories under `~/workspace/me/shark/` form the complete kn product:

| Repo | Dir | Language | Role |
|------|-----|----------|------|
| **kn** | `./` | Python + TypeScript + Rust | Desktop app + CLI + Agent (this repo) |
| **kn-cloud** | `../kn-cloud/` | Java 21 + Spring Boot 3.x | Cloud backend — REST API + WebSocket server |
| **kn-ios** | `../kn-ios/` | Swift | iOS mobile app — remote control client |

### kn-cloud (`../kn-cloud/`)

Maven multi-module project providing the server-side backend:

| Module | Port | Role |
|--------|------|------|
| `kn-cloud-common` | — | Shared infrastructure: JwtService, RedisKeys, ErrorCode, ApiResponse, BizException, LoginContext |
| `kn-cloud-api` | `8080` | REST API — user auth, device binding, redeem, session queries |
| `kn-cloud-ws` | `8081` | WebSocket server — agent/iOS connection orchestration, message relay |

**Database**: MySQL 8.0 via MyBatis-Plus, Redis 7 for cache/session state.

**Key API endpoints**:
| Endpoint | Auth | Purpose |
|----------|------|---------|
| `POST /api/v1/device/bind-init` | Public | Request bind code (returns 6-digit code, 300s TTL) |
| `GET /api/v1/device/bind-result?code=xxx` | Public | Poll bind confirmation status |
| `POST /api/v1/device/redeem` | Bearer device_token | Redeem card code |
| `GET /api/v1/config/purchase-url` | Public | Get purchase page URL |
| `GET /api/v1/auth/*` | Mixed | Register, login, refresh, captcha, Apple login |

**WebSocket protocol**: Agent connects to `wss://api.shark.kim/v1/ws` with headers `Authorization: Bearer <device_token>`, `X-KN-Role: kn-agent`, `X-KN-Machine-Id`, `X-KN-Protocol-Version: 1`. iOS connects with JWT access token.

**Key files**:
- `kn-cloud-api/src/main/java/dev/kn/cloud/api/controller/` — REST controllers
- `kn-cloud-api/src/main/java/dev/kn/cloud/api/config/AuthFilter.java` — JWT auth filter with public path whitelist
- `kn-cloud-api/src/main/resources/application-dev.yml` / `application-prod.yml` — environment configs
- `kn-cloud-ws/src/main/java/dev/kn/cloud/ws/handler/KnWsHandler.java` — WebSocket message dispatch
- `kn-cloud-ws/src/main/java/dev/kn/cloud/ws/service/ConnectionService.java` — Agent/iOS connection auth
- `kn-cloud/deploy/init.sql` — Database schema

### kn-ios (`../kn-ios/`)

Swift iOS app for remote Mac control. Communicates with kn-cloud via REST + WebSocket, scans QR codes from desktop BindDialog to complete device binding.

**Note**: When changes are made to kn (desktop/agent) that affect the binding protocol, QR code data format, or WebSocket message types, the kn-ios repo must be checked for compatibility.

## Key Conventions

- **Profile names**: `[a-z0-9]([a-z0-9-]*[a-z0-9])?` — enforced by `add_profile_cmd` to prevent shell injection in `sed`/regex parsing
- **Config atomic write**: tmp file → `fsync` → `rename`, with 3-generation rotating backup (`.bak` → `.bak.1` → `.bak.2` → `.bak.3`) before overwrite
- **No dev/prod config split**: single `~/.kn/config.yaml` for all modes
- **Shell wrapper `sed -i`**: uses macOS `sed -i ''` syntax

## Hard-Won Lessons

Rules discovered through bug fixes and code review. Violating any of these will re-introduce known bugs.

### 1. Config Write Safety (lock interoperability)

**Never write `config.yaml` without acquiring BOTH locks:**

| Lock | Scope | File |
|------|-------|------|
| `with_write_lock(|| ...)` | Intra-process (Rust↔Rust Tauri commands) | `lib.rs:23-33` |
| `fs2::lock_exclusive()` on `.config.lock` | Cross-process (Rust↔Python CLI) | `profile_cmd.rs` |

- Python side acquires `fcntl.flock(LOCK_EX)` on `~/.kn/.config.lock` before every write (`lib/config.py:45-76`). 5-second busy-wait timeout.
- Rust side MUST use `fs2::FileExt::try_lock_exclusive()` on the same `.config.lock` file, wrapped inside `crate::with_write_lock()`. See `profile_cmd.rs::write_config()`.

### 2. Shell Wrapper — Single Source of Truth

- **Canonical source**: `shell/ai-profile.sh` (bash).
- **Rust embeds at compile time**: `include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../shell/ai-profile.sh"))` — NOT a duplicate embedded string constant.
- **NEVER** maintain a separate `const SHELL_RC: &str = r#"..."#;` in Rust. Always use `include_str!`.
- **Checksum gating**: `ensure_shell_rc()` must compare content before overwriting `~/.kn/shell-rc`. Only write if content differs. Protects user customizations.
- The script includes all features: fzf picker, default fallback, `ai profile` subcommands, Codex auth.json handling, zero-dependency sed/awk fallback.

### 3. Delete Profile — Default Promotion

**Both Python CLI and Rust desktop must behave identically**: when deleting the default profile, promote the **alphabetically-first** remaining profile. If no profiles remain, set default to `""`.

- Python (`bin/profile:412-413`): `remaining = sorted(config.get("profiles", {}).keys()); config["default"] = remaining[0] if remaining else ""`
- Rust (`profile_cmd.rs`): sort `HashMap` keys with `remaining.sort()` before `.first()`. **Never use `.keys().next()` on HashMap** — iteration order is non-deterministic.

### 4. Binary Resolution — Always Check Shell PATH

`find_binary()` in `commands.rs` has a 3-tier fallback:
1. Hardcoded platform paths (fast, no shell overhead)
2. **Shell PATH** via `$SHELL -lc "command -v <name>"` (catches user-installed tools: Homebrew, `~/.local/bin`, npm global, etc.)
3. Bare command name (relies on system PATH — unreliable in GUI apps)

Never skip tier 2. Tauri GUI processes have a minimal PATH (`/usr/bin:/bin`). The login shell has the user's full PATH.

### 5. No Spawning System Binaries for HTTP or Hashing

| Don't | Do |
|-------|-----|
| `std::process::Command::new("curl")` | `reqwest::blocking::Client::new().get(url)` |
| `shasum` / `sha256sum` | `sha2::Sha256::new()` + `std::io::copy` |

`reqwest` and `sha2` are already dependencies. Using them eliminates:
- PATH dependency (no need to find `curl`/`shasum` on disk)
- Process spawn overhead
- Platform-specific argument differences

### 6. Config Backup — Rotate, Never Overwrite

Every write to `config.yaml` must rotate through 3 backup generations:
```
config.yaml.bak.2 → config.yaml.bak.3
config.yaml.bak.1 → config.yaml.bak.2
config.yaml.bak   → config.yaml.bak.1
config.yaml       → config.yaml.bak (new backup)
```
- Python: `lib/config.py:48-63` — `shutil.move` chain before `shutil.copy2`
- Rust: `profile_cmd.rs` — `fs::rename` chain before `fs::copy`
- Never overwrite the sole `.bak` — a bad write followed by another write destroys the only recovery path.

### 7. Python YAML — Block Scalars and Numbers

- **Block scalars**: `_parse_yaml` supports `|` and `>` indicators. `_format_yaml` emits `|` style when values contain `\n`.
- **Number detection**: Use `try: float(val)` NOT regex replace-chains. `val.replace(".", "", 1).replace("-", "", 1).isdigit()` misses `+5`, `1e5`, `-1e5`, `.inf`, `.nan`.

### 8. React State — Never Read Ref After setState

`useRef` synced to `useState` (like `sessionsRef.current = tabs`) introduces a race:
1. `setTabs(...)` is called → React batches the update
2. Code reads `sessionsRef.current` BEFORE re-render → gets **stale** (pre-update) data
3. Decisions based on stale data produce wrong UI state

**Fix**: Compute side-effects inside `setState` updater callback:
```typescript
setTabs((prev) => {
  const next = prev.filter(...);
  if (next.length === 0) setIsOpen(false);  // based on LATEST state
  return next;
});
```

### 9. TypeScript — Narrow Types, Use `unknown` for Catches

- **Discriminated unions narrow automatically**: after `if (item.type === "plugin")`, `item.data` IS `PluginEntry`. No `as any` needed. Extract to local variable for cleaner code.
- **Never `catch (e: any)`**: use `catch (e: unknown)`. `String(e)` works on `unknown` too — no behavioral change, just type safety.
- **Add missing fields to interfaces**: `PluginEntry` has `agents: AgentEntry[]` and `commands: CommandEntry[]` — if they exist at runtime, add them to the type.

### 10. Use `home_dir()` Consistently

`crate::home_dir()` exists at `lib.rs` with proper fallback chain. **Always use it.** Never inline `std::env::var("HOME")...` — the function handles edge cases correctly.
