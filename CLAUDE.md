# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

AI Profile Manager — a monorepo with three components sharing one data file (`~/.claude-profiles/config.yaml`):

| Component | Dir | Language | Role |
|-----------|-----|----------|------|
| CLI + Shell Wrapper | `bin/`, `lib/`, `shell/` | Python 3 + Bash | `profile` CLI + `ai()` shell function for env injection |
| Desktop App | `desktop/` | TypeScript + Rust (Tauri v2) | GUI with embedded PTY terminal |
| Product Site | `site/` | Vue 3 + TypeScript + Vite | Landing page + docs, deployed to GitHub Pages |

All three read/write the same `~/.claude-profiles/config.yaml`. The Python lib uses `fcntl.flock` (Unix) / `msvcrt.locking` (Windows) for concurrent-write safety. The Rust backend reads/writes directly via `serde_yaml`.

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
                    ~/.claude-profiles/config.yaml  (single source of truth)
                         ↑ read/write ↑
        ┌────────────────┼────────────┼────────────────┐
        ▼                ▼            ▼                ▼
   bin/profile     lib/config.py  desktop/src-tauri   shell/ai-profile.sh
   (Python CLI)    (YAML+lock)    (Rust serde_yaml)   (Bash sed reader)
```

### CLI (Python)
- `bin/profile` — thin CLI wrapper, delegates to `lib/config.py`
- `lib/config.py` — hand-rolled YAML parser/formatter (no PyYAML dependency), file locking via `fcntl` (Unix) or `msvcrt` (Windows), atomic write with tmp-file + fsync + rename
- `tests/test_config.py` — unit tests for YAML parsing, serialization, public API
- `tests/test_json_output.py` — integration tests for `--json` CLI flag

### Shell Wrapper
- `shell/ai-profile.sh` — installed to `~/.claude-profiles/shell-rc`, sourced from `.zshrc`/`.bashrc`
- Defines `ai()` function: `ai claude <profile>` / `ai codex <profile>`
- Reads config via `sed` (zero-dependency), injects env vars into subshell
- `install.sh` copies it to `~/.claude-profiles/shell-rc`

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

- `.github/workflows/build-desktop.yml` — manual trigger (`workflow_dispatch`), builds macOS (ARM + Intel) + Windows, creates GitHub Release
- `.github/workflows/deploy-site.yml` — auto-deploys `site/` to GitHub Pages on push to main

## Cross-Platform Design

| Operation | macOS | Linux | Windows |
|-----------|-------|-------|---------|
| HTTP fetch | `curl -sL` | `curl -sL` | `curl -sL` |
| SHA256 | `shasum -a 256` | `sha256sum` / `shasum` | `powershell Get-FileHash` |
| Open file | `/usr/bin/open` | `xdg-open` | `cmd /c start` |
| PTY shell | `/bin/zsh -i -l` | `/bin/bash -i -l` | `powershell.exe` |
| File lock | `fcntl.flock` | `fcntl.flock` | `msvcrt.locking` |
| Shell RC | `.zshrc` + `.bashrc` | `.zshrc` + `.bashrc` | PowerShell profile + `.bashrc` |

Key implementation details:
- `commands.rs::find_binary()` — searches platform-specific paths before falling back to bare command name
- `commands.rs::home_dir()` — checks `HOME` (Unix) then `USERPROFILE` (Windows)
- `install.sh` + `ensure_shell_rc()` — configures both `.zshrc` and `.bashrc` idempotently
- `pty.rs` — always spawns with `-i -l` (login + interactive), ensures `TERM=xterm-256color`

## Key Conventions

- **Profile names**: `[a-z0-9]([a-z0-9-]*[a-z0-9])?` — enforced by `add_profile_cmd` to prevent shell injection in `sed`/regex parsing
- **Config atomic write**: tmp file → `fsync` → `rename`, with backup to `.bak` before overwrite
- **No dev/prod config split**: single `~/.claude-profiles/config.yaml` for all modes
- **Shell wrapper `sed -i`**: tries macOS `sed -i ''` first, falls back to Linux `sed -i`
