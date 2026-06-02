# AI Profile Manager — Design Spec

## Overview

A CLI tool and shell wrapper system that lets users switch between different API keys and configurations (profiles) when launching `claude` (Claude Code) or `codex` (OpenAI Codex CLI). Each terminal session can use a different profile without cross-contamination.

## Core Principle

**Single source of truth**: `~/.claude-profiles/config.yaml` is the only data store. The management CLI, shell wrapper, and future UI all read/write this same file.

## Architecture

```
UI Layer (future) ────┐
                      ├── reads/writes config.yaml
CLI (profile) ────────┤
                      │
Shell Wrapper ────────┘ reads config.yaml, injects env vars into child process
```

## File Layout

```
~/.claude-profiles/
├── config.yaml           # Single source of truth, all profiles
├── profile               # Management CLI script (optional, in PATH)
└── shell-rc              # Shell functions, sourced from .zshrc
```

## config.yaml Schema

```yaml
default: <profile-name>     # Optional, defaults to first profile

profiles:
  <name>:
    desc: "<description>"   # Optional human-readable description
    env:                    # Free-form key-value, injected as env vars
      KEY: value
      # ...
```

**Design decisions:**
- `env` is a free-form map — no schema validation on keys. This supports any future tool's env vars without code changes.
- `desc` is optional, used in interactive selection UI.
- Profile names must be unique, lowercase alphanumeric + hyphens.

## Example Config

```yaml
default: deepseek

profiles:
  deepseek:
    desc: "DeepSeek 中转"
    env:
      ANTHROPIC_AUTH_TOKEN: sk-4bb88881db8b416e89114d7cdaaf079c
      ANTHROPIC_BASE_URL: https://api.deepseek.com/anthropic
      ANTHROPIC_MODEL: deepseek-v4-pro
      ANTHROPIC_DEFAULT_HAIKU_MODEL: deepseek-v4-flash
      ANTHROPIC_DEFAULT_SONNET_MODEL: deepseek-v4-pro[1M]
      ANTHROPIC_DEFAULT_OPUS_MODEL: deepseek-v4-pro[1M]
      DISABLE_AUTOUPDATER: "1"

  codex-default:
    desc: "OpenAI 官方 Codex"
    env:
      OPENAI_API_KEY: sk-proj-xxxxxxxx

  codex-custom:
    desc: "OpenAI 兼容中转"
    env:
      OPENAI_API_KEY: sk-zzz-xxxxxxxx
      OPENAI_BASE_URL: https://api.custom-provider.com/v1
      OPENAI_MODEL: gpt-5
```

## Shell Wrapper Mechanism

Shell functions in `~/.claude-profiles/shell-rc` shadow the real `claude` and `codex` commands:

```
User types:  claude deepseek
             │
             ▼
Shell function "claude" intercepts
             │
             ├─ "deepseek" is a known profile? → YES
             ├─ Read deepseek.profile.env from config.yaml
             ├─ Extract env vars
             ├─ Run: env KEY=VALUE... command claude
             │        (command = skip function, call real binary)
             │
             └─ "deepseek" is NOT a profile? → pass through:
                command claude deepseek  (real claude gets the arg)
```

**Key behaviors:**
- `claude <profile>` — inject profile env, launch
- `claude` (no args) — interactive selection via fzf/select, then launch
- `claude <not-a-profile>` — transparent pass-through to real claude
- `codex` — same logic, but reads `OPENAI_*` prefixed vars
- On process exit, all injected env vars disappear — zero cleanup needed

**Env injection rules:**
- All env vars from the profile are injected
- Overrides any existing env var of the same name
- Only affects the child process (claude/codex), not the parent shell

## Management CLI (`profile`)

| Command | Args | Description |
|---------|------|-------------|
| `profile add <name>` | `--desc "..."` | Create new empty profile |
| `profile set <name> <KEY>=<value>` | | Set/update one env var |
| `profile unset <name> <KEY>` | | Remove one env var |
| `profile remove <name>` | | Delete entire profile |
| `profile list` | | Show all profiles with desc |
| `profile show <name>` | | Show full profile detail |
| `profile default <name>` | | Set default profile |
| `profile init` | | Bootstrap from existing `~/.claude/settings.json` |

## Interactive Selection (no-arg launch)

When `claude` or `codex` is invoked without a profile name:

1. Read all profile names + descriptions from config.yaml
2. Present as a selection list (fzf if available, fallback to bash `select`)
3. User picks one → inject env and launch

If `default:` is set in config.yaml, skip selection and use default directly.

## Implementation Phases

### Phase 1: Core Shell Script
- `config.yaml` bootstrapping (migrate from existing settings.json)
- Shell wrapper functions (`claude`, `codex`)
- Interactive selection
- ~60 lines of shell

### Phase 2: Management CLI
- `profile` script with add/set/unset/remove/list/show/default
- YAML parsing via minimal approach (awk/sed or python one-liner)
- ~80 lines of shell

### Phase 3: Future UI
- Web or TUI interface
- Reads/writes the same `config.yaml`
- Profile CRUD + env var editor
- No new state introduced

## Non-Goals

- No encryption of API keys at rest (user responsibility to secure `~/.claude-profiles/`)
- No multi-machine sync (user can sync config.yaml with their own tools)
- No per-project profile binding (use direnv or .claude/settings.json for that)
