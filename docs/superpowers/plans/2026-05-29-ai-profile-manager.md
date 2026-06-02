# AI Profile Manager — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a multi-profile management system that lets users switch API keys/configurations when launching `claude` or `codex`, using `claude <profile>` or `codex <profile>` syntax.

**Architecture:** Three-layer design sharing a single `config.yaml` file. A shared Python module (`lib/config.py`) handles all YAML read/write with file locking — the CLI (`bin/profile`) and future UI both import it. Shell wrapper functions (`claude`, `codex`) intercept launch commands, call `profile env <name>` to get env vars as `eval`-safe output, and inject them into the real CLI process. Environment injection is per-process only — exits clean.

**Tech Stack:** Python 3 (stdlib only, no pip deps), Zsh/Bash, YAML (minimal subset, hand-rolled parser), `fcntl.flock` for concurrent write safety

---

## File Structure

```
kn/
├── lib/
│   └── config.py             # Shared config module — YAML read/write + file lock
├── bin/
│   └── profile               # Python CLI — thin wrapper, imports lib.config
├── shell/
│   └── ai-profile.sh         # Shell wrapper functions (claude + codex)
├── templates/
│   └── config.yaml           # Reference config with comments
├── tests/
│   └── test_config.py        # Unit tests for lib/config.py
├── install.sh                # Installer: copies files to ~/.local/bin, ~/.claude-profiles/
└── docs/superpowers/
    ├── specs/2026-05-29-ai-profile-manager-design.md
    └── plans/2026-05-29-ai-profile-manager.md
```

**After installation:**
```
~/.claude-profiles/
├── config.yaml              # User's profile data (shared between CLI and future UI)
├── lib/
│   └── config.py            # Shared config module (UI also imports this)
└── shell-rc                 # Shell functions, sourced from .zshrc

~/.local/bin/
└── profile                  # Management CLI
```

**Future UI import path:**
```python
# UI will import the same module:
from lib.config import read_config, write_config, ProfileManager
# Or shell out:
# subprocess.run(["profile", "list"], capture_output=True)
```

---

### Task 1: Project scaffolding

**Files:**
- Create: `lib/config.py` (empty placeholder)
- Create: `bin/profile` (empty placeholder)
- Create: `shell/ai-profile.sh` (empty placeholder)
- Create: `templates/config.yaml`
- Create: `tests/test_config.py` (empty placeholder)
- Create: `install.sh` (empty placeholder)

- [ ] **Step 1: Create directory structure**

Run:
```bash
mkdir -p /Users/zhaojun/workspace/me/shark/kn/lib
mkdir -p /Users/zhaojun/workspace/me/shark/kn/bin
mkdir -p /Users/zhaojun/workspace/me/shark/kn/shell
mkdir -p /Users/zhaojun/workspace/me/shark/kn/templates
mkdir -p /Users/zhaojun/workspace/me/shark/kn/tests
```

- [ ] **Step 2: Create reference config template**

Write `templates/config.yaml`:

```yaml
# AI Profile Manager — config
# Edit directly or use: profile <command>
# https://api.deepseek.com/anthropic

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

- [ ] **Step 3: Create empty placeholder files**

Run:
```bash
touch /Users/zhaojun/workspace/me/shark/kn/lib/config.py
touch /Users/zhaojun/workspace/me/shark/kn/bin/profile
touch /Users/zhaojun/workspace/me/shark/kn/shell/ai-profile.sh
touch /Users/zhaojun/workspace/me/shark/kn/tests/test_config.py
touch /Users/zhaojun/workspace/me/shark/kn/install.sh
chmod +x /Users/zhaojun/workspace/me/shark/kn/bin/profile
chmod +x /Users/zhaojun/workspace/me/shark/kn/install.sh
```

- [ ] **Step 4: Verify structure**

Run:
```bash
find /Users/zhaojun/workspace/me/shark/kn -type f | sort
```

Expected output shows all created files.

---

### Task 2: Write lib/config.py — shared config module with file locking

**Files:**
- Create: `lib/config.py` (complete config module)
- Create: `bin/profile` (thin CLI, imports lib.config)

**Design note:** `lib/config.py` is the single source of truth for all config operations. Both the CLI and the future UI import it. File locking (`fcntl.flock`) prevents concurrent write corruption when CLI and UI run simultaneously.

- [ ] **Step 1: Write lib/config.py**

Write `lib/config.py`:

```python
"""AI Profile Manager — shared config module.

Reads/writes ~/.claude-profiles/config.yaml.
Imported by both CLI (bin/profile) and future UI.
Uses fcntl.flock for concurrent write safety.
"""

import os
import fcntl
import time

CONFIG_DIR = os.path.expanduser("~/.claude-profiles")
CONFIG_FILE = os.path.join(CONFIG_DIR, "config.yaml")
LOCK_FILE = os.path.join(CONFIG_DIR, ".config.lock")

# ── Public API ─────────────────────────────────────────────────

def read_config():
    """Parse config.yaml into a dict. Returns empty profiles dict if no file."""
    if not os.path.exists(CONFIG_FILE):
        return {"profiles": {}}
    with open(CONFIG_FILE, "r") as f:
        text = f.read()
    return _parse_yaml(text)


def write_config(config):
    """Serialize config dict back to config.yaml, with file locking."""
    text = _format_yaml(config)

    os.makedirs(CONFIG_DIR, exist_ok=True)

    with open(LOCK_FILE, "w") as lf:
        _acquire_lock(lf)
        try:
            with open(CONFIG_FILE, "w") as f:
                f.write(text)
        finally:
            _release_lock(lf)


def get_profile(config, name):
    """Return a single profile dict or None."""
    return config.get("profiles", {}).get(name)


def list_profiles(config):
    """Return sorted list of profile names."""
    return sorted(config.get("profiles", {}).keys())


def get_default(config):
    """Return the default profile name or empty string."""
    return config.get("default", "")


# ── File locking ───────────────────────────────────────────────

def _acquire_lock(lock_file):
    """Block until exclusive lock is acquired (5s timeout)."""
    deadline = time.time() + 5
    while True:
        try:
            fcntl.flock(lock_file, fcntl.LOCK_EX | fcntl.LOCK_NB)
            return
        except BlockingIOError:
            if time.time() > deadline:
                raise TimeoutError("Could not acquire config lock after 5s")
            time.sleep(0.05)


def _release_lock(lock_file):
    """Release the file lock."""
    fcntl.flock(lock_file, fcntl.LOCK_UN)


# ── YAML Parser (hand-rolled, zero-dependency) ─────────────────

def _parse_yaml(text):
    """Parse our constrained YAML subset.

    Only handles the schema we produce:
      - top-level scalar key: value
      - 2-space indented profile blocks with desc: and env:
      - 4-space indented KEY: value inside env:
    No support for lists, anchors, multiline, tags.
    """
    config = {"profiles": {}}
    current_profile = None
    in_env = False

    for raw in text.split("\n"):
        line = raw.rstrip()
        stripped = line.strip()

        if not stripped or stripped.startswith("#"):
            continue

        indent = len(line) - len(line.lstrip(" "))

        if indent == 0:
            in_env = False
            if ":" in stripped:
                key, _, val = stripped.partition(":")
                key = key.strip()
                val = _yaml_val(val)
                if key == "default":
                    config["default"] = val
            continue

        if indent == 2:
            in_env = False
            if ":" in stripped:
                key, _, val = stripped.partition(":")
                key = key.strip()
                val = _yaml_val(val)
                if key == "desc":
                    if current_profile:
                        config["profiles"][current_profile]["desc"] = val
                elif key == "env":
                    in_env = True
                else:
                    current_profile = key
                    config["profiles"][current_profile] = {"desc": "", "env": {}}
            continue

        if indent == 4 and in_env and current_profile:
            if ":" in stripped:
                key, _, val = stripped.partition(":")
                key = key.strip()
                val = _yaml_val(val)
                config["profiles"][current_profile]["env"][key] = val
            continue

    return config


def _yaml_val(raw):
    """Strip quotes and whitespace from a YAML scalar value."""
    v = raw.strip()
    if len(v) >= 2:
        if (v.startswith('"') and v.endswith('"')) or \
           (v.startswith("'") and v.endswith("'")):
            return v[1:-1]
    if " #" in v:
        v = v[: v.index(" #")]
    return v.strip()


# ── YAML Formatter ─────────────────────────────────────────────

def _format_yaml(config):
    """Serialize config dict to YAML string."""
    lines = []
    if config.get("default"):
        lines.append(f"default: {config['default']}")
        lines.append("")

    lines.append("profiles:")
    for name in sorted(config.get("profiles", {}).keys()):
        profile = config["profiles"][name]
        lines.append(f"  {name}:")
        if profile.get("desc"):
            lines.append(f'    desc: "{profile["desc"]}"')
        lines.append("    env:")
        env = profile.get("env", {})
        if env:
            for k, v in sorted(env.items()):
                lines.append(f"      {k}: {_quote_yaml(v)}")
        else:
            lines.append("      {}")
        lines.append("")

    return "\n".join(lines) + "\n"


def _quote_yaml(val):
    """Quote value if it contains YAML-special characters."""
    if not val:
        return '""'
    special = set(" :#{}[]&*!|>\"'@`,")
    if any(c in special for c in val):
        escaped = val.replace("\\", "\\\\").replace('"', '\\"')
        return f'"{escaped}"'
    return val
```

- [ ] **Step 2: Write bin/profile as thin CLI**

Write `bin/profile`:

```python
#!/usr/bin/env python3
"""AI Profile Manager CLI — thin wrapper around lib.config."""

import os
import sys
import json
import re

# Import shared config module (works both in dev and after install)
_script_dir = os.path.dirname(os.path.abspath(__file__))
_lib_paths = [
    os.path.join(_script_dir, "..", "lib"),                     # dev layout
    os.path.join(os.path.expanduser("~"), ".claude-profiles"),  # installed layout
]
for _p in _lib_paths:
    if _p not in sys.path:
        sys.path.insert(0, _p)
import config as cfg

USAGE = """Usage: profile <command> [args...]

Commands:
  list              List all profiles
  show   <name>     Show profile details
  env    <name>     Output env vars for shell eval
  names             Output profile list for fzf

  add    <name> [desc]   Create new profile
  remove <name>           Delete a profile
  set    <name> <K=V>     Set env var in profile
  unset  <name> <K>       Remove env var from profile
  default [name]          Get/set default profile

  init              Bootstrap from ~/.claude/settings.json
"""
```

- [ ] **Step 3: Verify imports work**

Run:
```bash
cd /Users/zhaojun/workspace/me/shark/kn && python3 -c "
import sys; sys.path.insert(0, 'lib')
import config as cfg

# Test with the template
cfg.CONFIG_FILE = 'templates/config.yaml'
config = cfg.read_config()
print('Profiles found:', list(config.get('profiles', {}).keys()))
print('Default:', config.get('default', 'none'))

# Round-trip
import tempfile
with tempfile.NamedTemporaryFile(mode='w', suffix='.yaml', delete=False) as f:
    tmp = f.name
cfg.LOCK_FILE = tmp + '.lock'
cfg.CONFIG_FILE = tmp
cfg.write_config(config)
config2 = cfg.read_config()
assert config == config2, 'Round-trip failed!'
print('Round-trip OK')
import os; os.unlink(tmp); os.unlink(tmp + '.lock')
"
```

Expected: Profiles found and round-trip passes.

---

### Task 3: Write profile CLI — query commands (list, show, env, names, init)

**Files:**
- Modify: `bin/profile` (append CLI logic at bottom)
- Modify: `lib/config.py` (add missing helper if needed)

- [ ] **Step 1: Write subcommands for read-only operations**

Append to `bin/profile`:

```python
# ── Query commands ────────────────────────────────────────────

def cmd_list(config):
    """List all profiles with descriptions."""
    profiles = config.get("profiles", {})
    default = config.get("default", "")
    if not profiles:
        print("No profiles configured. Run: profile init")
        return
    width = max(len(n) for n in profiles) if profiles else 0
    for name in sorted(profiles):
        marker = " *" if name == default else "  "
        desc = profiles[name].get("desc", "")
        env_count = len(profiles[name].get("env", {}))
        print(f" {marker} {name:<{width}}  {desc} ({env_count} vars)")


def cmd_show(config, name):
    """Print full detail of one profile."""
    if not name:
        print("Usage: profile show <name>", file=sys.stderr)
        sys.exit(1)
    if name not in config.get("profiles", {}):
        print(f"Profile '{name}' not found.", file=sys.stderr)
        sys.exit(1)
    p = config["profiles"][name]
    print(f"[{name}]")
    if p.get("desc"):
        print(f"  desc: {p['desc']}")
    print(f"  env:")
    env = p.get("env", {})
    if env:
        for k, v in sorted(env.items()):
            display = _mask_secret(k, v)
            print(f"    {k}={display}")
    else:
        print("    (empty)")


def _mask_secret(key, val):
    """Mask API keys/tokens in display output."""
    secret_keys = {"KEY", "TOKEN", "SECRET", "PASSWORD", "AUTH_TOKEN", "API_KEY"}
    is_secret = any(s in key.upper() for s in secret_keys)
    if is_secret and len(val) > 8:
        return val[:4] + "****" + val[-4:]
    return val


def cmd_env(config, name):
    """Output env vars as shell eval-safe statements."""
    if not name:
        print("Usage: profile env <name>", file=sys.stderr)
        sys.exit(1)
    if name not in config.get("profiles", {}):
        print(f"Profile '{name}' not found.", file=sys.stderr)
        sys.exit(1)
    for k, v in sorted(config["profiles"][name].get("env", {}).items()):
        safe = v.replace("'", "'\\''")
        print(f"export {k}='{safe}'")


def cmd_names(config):
    """Output profile names for fzf/select. Tab-separated name + desc."""
    for name in sorted(config.get("profiles", {})):
        desc = config["profiles"][name].get("desc", "")
        print(f"{name}\t{desc}")


def cmd_init(config):
    """Bootstrap from existing ~/.claude/settings.json."""
    settings_file = os.path.expanduser("~/.claude/settings.json")
    if not os.path.exists(settings_file):
        print("No existing ~/.claude/settings.json found. Creating empty config.")
        if not config.get("profiles"):
            config["profiles"] = {}
        cfg.write_config(config)
        return

    with open(settings_file, "r") as f:
        settings = json.load(f)

    env = settings.get("env", {})
    if not env:
        print("No env vars found in settings.json. Creating empty config.")
        if not config.get("profiles"):
            config["profiles"] = {}
        cfg.write_config(config)
        return

    base_url = env.get("ANTHROPIC_BASE_URL", "")
    name = "deepseek" if "deepseek" in base_url else "imported"

    config["profiles"][name] = {
        "desc": f"Imported from ~/.claude/settings.json",
        "env": dict(env),
    }
    config["default"] = name
    cfg.write_config(config)
    print(f"Imported {len(env)} env vars from settings.json → profile '{name}'")
    print(f"Config saved to {cfg.CONFIG_FILE}")
```

- [ ] **Step 2: Wire up query commands in main()**

Append to `bin/profile`:

```python
# ── CLI entry point ───────────────────────────────────────────

def main():
    if len(sys.argv) < 2:
        print(USAGE, file=sys.stderr)
        sys.exit(1)

    cmd = sys.argv[1]
    config = cfg.read_config()

    # Query commands
    if cmd == "list":
        cmd_list(config)
    elif cmd == "show":
        cmd_show(config, sys.argv[2] if len(sys.argv) > 2 else "")
    elif cmd == "env":
        cmd_env(config, sys.argv[2] if len(sys.argv) > 2 else "")
    elif cmd == "names":
        cmd_names(config)
    elif cmd == "init":
        cmd_init(config)
    else:
        print(f"Unknown command: {cmd}", file=sys.stderr)
        print(USAGE, file=sys.stderr)
        sys.exit(1)


if __name__ == "__main__":
    main()
```

- [ ] **Step 3: Test query commands against template config**

Run:
```bash
cd /Users/zhaojun/workspace/me/shark/kn
# Symlink config to test location temporarily
mkdir -p ~/.claude-profiles
cp templates/config.yaml ~/.claude-profiles/config.yaml
python3 bin/profile list
python3 bin/profile show deepseek
python3 bin/profile env deepseek
python3 bin/profile names
```

Expected:
- `list` shows 3 profiles with descriptions
- `show deepseek` shows all env vars (token masked)
- `env deepseek` prints `export ANTHROPIC_AUTH_TOKEN='...'` etc.
- `names` prints tab-separated name + desc

---

### Task 4: Write profile CLI — mutation commands (add, remove, set, unset, default)

**Files:**
- Modify: `bin/profile` (append mutation commands and wire up in main)

- [ ] **Step 1: Write mutation subcommands**

Append to `bin/profile` (before the main function):

```python
# ── Mutation commands ─────────────────────────────────────────

def cmd_add(config, name, desc):
    """Create a new empty profile."""
    if not name:
        print("Usage: profile add <name> [description]", file=sys.stderr)
        sys.exit(1)
    if not re.match(r"^[a-z0-9]([a-z0-9-]*[a-z0-9])?$", name):
        print("Profile name must be lowercase alphanumeric + hyphens.", file=sys.stderr)
        sys.exit(1)
    if name in config.get("profiles", {}):
        print(f"Profile '{name}' already exists.", file=sys.stderr)
        sys.exit(1)
    config.setdefault("profiles", {})[name] = {"desc": desc, "env": {}}
    cfg.write_config(config)
    print(f"Profile '{name}' added.")


def cmd_remove(config, name):
    """Delete a profile."""
    if not name:
        print("Usage: profile remove <name>", file=sys.stderr)
        sys.exit(1)
    if name not in config.get("profiles", {}):
        print(f"Profile '{name}' not found.", file=sys.stderr)
        sys.exit(1)
    del config["profiles"][name]
    if config.get("default") == name:
        config["default"] = ""
    cfg.write_config(config)
    print(f"Profile '{name}' removed.")


def cmd_set(config, name, raw):
    """Set an env var in a profile. raw is KEY=VALUE."""
    if not name or not raw:
        print("Usage: profile set <name> <KEY>=<value>", file=sys.stderr)
        sys.exit(1)
    if name not in config.get("profiles", {}):
        print(f"Profile '{name}' not found.", file=sys.stderr)
        sys.exit(1)
    if "=" not in raw:
        print("Format: KEY=VALUE", file=sys.stderr)
        sys.exit(1)
    key, _, val = raw.partition("=")
    if not key:
        print("KEY cannot be empty.", file=sys.stderr)
        sys.exit(1)
    config["profiles"][name].setdefault("env", {})[key] = val
    cfg.write_config(config)
    print(f"{key} = {val}")


def cmd_unset(config, name, key):
    """Remove an env var from a profile."""
    if not name or not key:
        print("Usage: profile unset <name> <KEY>", file=sys.stderr)
        sys.exit(1)
    if name not in config.get("profiles", {}):
        print(f"Profile '{name}' not found.", file=sys.stderr)
        sys.exit(1)
    env = config["profiles"][name].get("env", {})
    if key in env:
        del env[key]
        cfg.write_config(config)
        print(f"Removed {key} from '{name}'.")
    else:
        print(f"Key '{key}' not found in '{name}'.", file=sys.stderr)


def cmd_default(config, name):
    """Get or set the default profile."""
    if not name:
        d = config.get("default", "")
        if d:
            print(d)
        else:
            print("No default profile set.", file=sys.stderr)
        return
    if name not in config.get("profiles", {}):
        print(f"Profile '{name}' not found.", file=sys.stderr)
        sys.exit(1)
    config["default"] = name
    cfg.write_config(config)
    print(f"Default profile → '{name}'.")
```

- [ ] **Step 2: Wire mutation commands into main()**

Replace the `else` block in `main()` with:

```python
    # Mutation commands
    elif cmd == "add":
        cmd_add(config, sys.argv[2] if len(sys.argv) > 2 else "",
                sys.argv[3] if len(sys.argv) > 3 else "")
    elif cmd == "remove":
        cmd_remove(config, sys.argv[2] if len(sys.argv) > 2 else "")
    elif cmd == "set":
        cmd_set(config, sys.argv[2] if len(sys.argv) > 2 else "",
                sys.argv[3] if len(sys.argv) > 3 else "")
    elif cmd == "unset":
        cmd_unset(config, sys.argv[2] if len(sys.argv) > 2 else "",
                  sys.argv[3] if len(sys.argv) > 3 else "")
    elif cmd == "default":
        cmd_default(config, sys.argv[2] if len(sys.argv) > 2 else "")
    else:
        print(f"Unknown command: {cmd}", file=sys.stderr)
        print(USAGE, file=sys.stderr)
        sys.exit(1)
```

- [ ] **Step 3: Test mutation commands**

Run:
```bash
cd /Users/zhaojun/workspace/me/shark/kn

# Add a new profile
python3 bin/profile add test-profile "Test profile"
python3 bin/profile list

# Set env vars
python3 bin/profile set test-profile ANTHROPIC_AUTH_TOKEN=sk-test-12345
python3 bin/profile set test-profile ANTHROPIC_BASE_URL=https://api.test.com

# Show it
python3 bin/profile show test-profile

# Set as default
python3 bin/profile default test-profile
python3 bin/profile default   # should print test-profile

# Unset a var
python3 bin/profile unset test-profile ANTHROPIC_BASE_URL
python3 bin/profile show test-profile

# Remove profile
python3 bin/profile remove test-profile
python3 bin/profile list

# Reset default
python3 bin/profile default deepseek
```

Expected: All commands succeed, output makes sense.

---

### Task 5: Write shell wrapper functions

**Files:**
- Create: `shell/ai-profile.sh`

- [ ] **Step 1: Write wrapper functions**

Write `shell/ai-profile.sh`:

```bash
# AI Profile Manager — Shell Wrapper
# Source this from ~/.zshrc:  source ~/.claude-profiles/shell-rc
#
# Usage:
#   claude <profile>     Launch claude with profile's env
#   claude               Interactive profile picker → launch claude
#   claude <anything>    Pass through to real claude
#   codex <profile>      Same for codex

# Ensure profile CLI is on PATH
if [ -x "$HOME/.local/bin/profile" ]; then
    PROFILE_CMD="$HOME/.local/bin/profile"
elif [ -x "$HOME/.claude-profiles/profile" ]; then
    PROFILE_CMD="$HOME/.claude-profiles/profile"
else
    PROFILE_CMD="profile"
fi

# ── claude ────────────────────────────────────────────────────

claude() {
    _ai_launch "claude" "$@"
}

# ── codex ─────────────────────────────────────────────────────

codex() {
    _ai_launch "codex" "$@"
}

# ── Internal: dispatch logic ───────────────────────────────────

_ai_launch() {
    local tool="$1"
    shift

    # Check if first arg is a known profile
    if [ $# -gt 0 ]; then
        if "$PROFILE_CMD" names 2>/dev/null | cut -f1 | grep -qx "$1"; then
            local profile_name="$1"
            shift
            _ai_launch_with_profile "$tool" "$profile_name" "$@"
            return
        fi
    fi

    # No profile arg → interactive picker or default
    local default_profile
    default_profile=$("$PROFILE_CMD" default 2>/dev/null)

    if [ -n "$default_profile" ]; then
        # Default set → use it directly
        _ai_launch_with_profile "$tool" "$default_profile" "$@"
        return
    fi

    # No default → interactive selection
    local selected
    if command -v fzf >/dev/null 2>&1; then
        selected=$("$PROFILE_CMD" names 2>/dev/null | fzf --prompt="Select profile for $tool: " --height=10 | cut -f1)
    else
        echo "Profiles:"
        "$PROFILE_CMD" list 2>/dev/null
        echo -n "Enter profile name (or Enter to skip): "
        read -r selected
    fi

    if [ -n "$selected" ]; then
        _ai_launch_with_profile "$tool" "$selected" "$@"
    else
        # No selection → launch bare
        command "$tool" "$@"
    fi
}

# ── Internal: inject env and launch ───────────────────────────

_ai_launch_with_profile() {
    local tool="$1"
    local profile_name="$2"
    shift 2

    local env_output
    env_output=$("$PROFILE_CMD" env "$profile_name" 2>/dev/null)
    if [ -z "$env_output" ]; then
        echo "Profile '$profile_name' not found or has no env vars." >&2
        command "$tool" "$@"
        return 1
    fi

    echo "→ Using profile: $profile_name"

    # eval the export statements, then launch the real command
    # The exports only affect THIS subshell, so parent shell is clean
    (
        eval "$env_output"
        command "$tool" "$@"
    )
}
```

- [ ] **Step 2: Verify syntax**

Run:
```bash
zsh -n /Users/zhaojun/workspace/me/shark/kn/shell/ai-profile.sh
```

Expected: no output (no syntax errors).

- [ ] **Step 3: Test wrapper with real claude (dry-run)**

Run:
```bash
cd /Users/zhaojun/workspace/me/shark/kn

# Source wrapper, test with a profile
source shell/ai-profile.sh

# Test that profile names are detected
profile names | cut -f1

# Test env output
profile env deepseek | head -3

# Test direct profile launch (will actually start claude — Ctrl+C to exit)
# claude deepseek --version
```

Expected: `profile names` outputs profile names, `profile env` outputs exports.

---

### Task 6: Write install script

**Files:**
- Create: `install.sh`

- [ ] **Step 1: Write installer**

Write `install.sh`:

```bash
#!/usr/bin/env bash
# Install AI Profile Manager
# Copies: lib/config.py → ~/.claude-profiles/lib/config.py (shared module)
#         bin/profile → ~/.local/bin/profile (CLI)
#         shell/ai-profile.sh → ~/.claude-profiles/shell-rc
#         templates/config.yaml → ~/.claude-profiles/config.yaml (if not exists)

set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"

echo "==> Installing AI Profile Manager"

# Create target directories
mkdir -p "$HOME/.local/bin"
mkdir -p "$HOME/.claude-profiles/lib"

# Install shared config module (used by CLI now, UI in future)
cp "$SCRIPT_DIR/lib/config.py" "$HOME/.claude-profiles/lib/config.py"
echo "  ✓ Installed config module → ~/.claude-profiles/lib/config.py"

# Install profile CLI
cp "$SCRIPT_DIR/bin/profile" "$HOME/.local/bin/profile"
chmod +x "$HOME/.local/bin/profile"
echo "  ✓ Installed profile → ~/.local/bin/profile"

# Install shell wrapper
cp "$SCRIPT_DIR/shell/ai-profile.sh" "$HOME/.claude-profiles/shell-rc"
echo "  ✓ Installed shell-rc → ~/.claude-profiles/shell-rc"

# Install config template (only if not exists)
if [ ! -f "$HOME/.claude-profiles/config.yaml" ]; then
    cp "$SCRIPT_DIR/templates/config.yaml" "$HOME/.claude-profiles/config.yaml"
    echo "  ✓ Installed config template → ~/.claude-profiles/config.yaml"
else
    echo "  - Config already exists, skipping: ~/.claude-profiles/config.yaml"
fi

# Check if ~/.local/bin is in PATH
if ! echo "$PATH" | tr ':' '\n' | grep -qx "$HOME/.local/bin"; then
    echo ""
    echo "  ⚠  ~/.local/bin is not in your PATH."
    echo "     Add this to your ~/.zshrc:"
    echo '     export PATH="$HOME/.local/bin:$PATH"'
fi

# Check if shell-rc is sourced
if ! grep -q "shell-rc" "$HOME/.zshrc" 2>/dev/null; then
    echo ""
    echo "  Add this line to your ~/.zshrc to activate the wrapper:"
    echo '  source "$HOME/.claude-profiles/shell-rc"'
fi

echo ""
echo "==> Done! Run this to activate now:"
echo "    source ~/.claude-profiles/shell-rc"
echo ""
echo "    Then:  profile init       # Import existing settings"
echo "           profile list       # See all profiles"
echo "           claude deepseek    # Launch with profile"
```

- [ ] **Step 2: Run installer**

Run:
```bash
cd /Users/zhaojun/workspace/me/shark/kn
bash install.sh
```

Expected: All ✓ lines, no errors.

- [ ] **Step 3: Verify installed files**

Run:
```bash
ls -la ~/.local/bin/profile
ls -la ~/.claude-profiles/shell-rc
ls -la ~/.claude-profiles/config.yaml
ls -la ~/.claude-profiles/lib/config.py
```

Expected: All four files exist and are readable.

---

### Task 7: Write unit tests for config module

**Files:**
- Create: `tests/test_config.py`

- [ ] **Step 1: Write tests**

Write `tests/test_config.py`:

```python
#!/usr/bin/env python3
"""Tests for lib/config.py — YAML parsing, serialization, file locking."""

import os
import sys
import tempfile
import unittest

# Load the config module
sys.path.insert(0, os.path.join(os.path.dirname(__file__), "..", "lib"))
import config as cfg


class TestYamlParsing(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.NamedTemporaryFile(mode="w", suffix=".yaml", delete=False)
        self.lock = tempfile.NamedTemporaryFile(mode="w", suffix=".lock", delete=False)
        cfg.CONFIG_FILE = self.tmp.name
        cfg.LOCK_FILE = self.lock.name

    def tearDown(self):
        self.tmp.close()
        self.lock.close()
        os.unlink(self.tmp.name)
        os.unlink(self.lock.name)

    def _write(self, text):
        self.tmp.write(text)
        self.tmp.flush()

    def test_empty_config(self):
        self._write("profiles:\n")
        config = cfg.read_config()
        self.assertEqual(config["profiles"], {})

    def test_single_profile(self):
        self._write("""profiles:
  test:
    desc: "A test profile"
    env:
      KEY1: value1
      KEY2: value2
""")
        config = cfg.read_config()
        self.assertIn("test", config["profiles"])
        self.assertEqual(config["profiles"]["test"]["desc"], "A test profile")
        self.assertEqual(config["profiles"]["test"]["env"]["KEY1"], "value1")
        self.assertEqual(config["profiles"]["test"]["env"]["KEY2"], "value2")

    def test_default(self):
        self._write("""default: myprof
profiles:
  myprof:
    desc: "Default one"
    env:
      X: "1"
""")
        config = cfg.read_config()
        self.assertEqual(config["default"], "myprof")

    def test_env_with_special_chars(self):
        self._write("""profiles:
  test:
    desc: "Special"
    env:
      URL: "https://api.example.com/v1?foo=bar"
      TOKEN: sk-test-1234567890
""")
        config = cfg.read_config()
        env = config["profiles"]["test"]["env"]
        self.assertEqual(env["URL"], "https://api.example.com/v1?foo=bar")
        self.assertEqual(env["TOKEN"], "sk-test-1234567890")

    def test_multiple_profiles(self):
        self._write("""profiles:
  a:
    desc: "Profile A"
    env:
      K: aval
  b:
    desc: "Profile B"
    env:
      K: bval
""")
        config = cfg.read_config()
        self.assertEqual(len(config["profiles"]), 2)
        self.assertEqual(config["profiles"]["a"]["env"]["K"], "aval")
        self.assertEqual(config["profiles"]["b"]["env"]["K"], "bval")

    def test_round_trip(self):
        data = {
            "default": "p1",
            "profiles": {
                "p1": {
                    "desc": "First",
                    "env": {"KEY1": "val1", "URL": "https://x.com/path"},
                },
                "p2": {
                    "desc": "Second",
                    "env": {},
                },
            },
        }
        cfg.write_config(data)
        config = cfg.read_config()
        self.assertEqual(config["default"], "p1")
        self.assertEqual(config["profiles"]["p1"]["desc"], "First")
        self.assertEqual(config["profiles"]["p1"]["env"]["KEY1"], "val1")
        self.assertEqual(config["profiles"]["p1"]["env"]["URL"], "https://x.com/path")
        self.assertEqual(config["profiles"]["p2"]["desc"], "Second")
        self.assertEqual(config["profiles"]["p2"]["env"], {})

    def test_comments_ignored(self):
        self._write("""# Top comment
default: p1
# Another comment
profiles:
  p1:
    desc: "Test"  # inline comment
    env:
      KEY: value
""")
        config = cfg.read_config()
        self.assertEqual(config["default"], "p1")
        self.assertEqual(config["profiles"]["p1"]["env"]["KEY"], "value")

    def test_empty_env(self):
        self._write("""profiles:
  bare:
    desc: "No env vars"
    env: {}
""")
        config = cfg.read_config()
        self.assertEqual(config["profiles"]["bare"]["env"], {})

    def test_missing_file(self):
        cfg.CONFIG_FILE = "/tmp/nonexistent_config_test.yaml"
        config = cfg.read_config()
        self.assertEqual(config, {"profiles": {}})


class TestQuoteYaml(unittest.TestCase):
    def test_plain_value(self):
        self.assertEqual(cfg._quote_yaml("hello"), "hello")

    def test_url_value(self):
        result = cfg._quote_yaml("https://api.example.com/v1")
        self.assertTrue(result.startswith('"'))

    def test_empty_value(self):
        self.assertEqual(cfg._quote_yaml(""), '""')

    def test_value_with_space(self):
        result = cfg._quote_yaml("hello world")
        self.assertTrue(result.startswith('"'))


class TestPublicAPI(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.NamedTemporaryFile(mode="w", suffix=".yaml", delete=False)
        self.lock = tempfile.NamedTemporaryFile(mode="w", suffix=".lock", delete=False)
        cfg.CONFIG_FILE = self.tmp.name
        cfg.LOCK_FILE = self.lock.name
        data = {
            "default": "a",
            "profiles": {
                "a": {"desc": "First", "env": {"X": "1"}},
                "b": {"desc": "Second", "env": {"Y": "2"}},
            },
        }
        cfg.write_config(data)

    def tearDown(self):
        self.tmp.close()
        self.lock.close()
        os.unlink(self.tmp.name)
        os.unlink(self.lock.name)

    def test_list_profiles(self):
        config = cfg.read_config()
        self.assertEqual(cfg.list_profiles(config), ["a", "b"])

    def test_get_profile(self):
        config = cfg.read_config()
        p = cfg.get_profile(config, "a")
        self.assertEqual(p["desc"], "First")
        self.assertEqual(p["env"]["X"], "1")

    def test_get_profile_missing(self):
        config = cfg.read_config()
        self.assertIsNone(cfg.get_profile(config, "nonexistent"))

    def test_get_default(self):
        config = cfg.read_config()
        self.assertEqual(cfg.get_default(config), "a")


if __name__ == "__main__":
    unittest.main()
```

- [ ] **Step 2: Run tests**

Run:
```bash
cd /Users/zhaojun/workspace/me/shark/kn && python3 tests/test_config.py -v
```

Expected: All tests pass (13 tests across 3 test classes).

- [ ] **Step 3: Add install checker step to verify PATH and .zshrc**

Run the install script's checks manually:

```bash
# Verify ~/.local/bin in PATH
echo "$PATH" | tr ':' '\n' | grep -q "$HOME/.local/bin" && echo "OK: PATH has ~/.local/bin" || echo "MISSING: ~/.local/bin not in PATH"

# Verify shell-rc sourced
grep -q "shell-rc" "$HOME/.zshrc" 2>/dev/null && echo "OK: shell-rc sourced" || echo "MISSING: add source line to ~/.zshrc"
```

---

### Task 8: End-to-end integration test

**Files:** No new files.

- [ ] **Step 1: Test full flow with fake claude**

Run:
```bash
cd /Users/zhaojun/workspace/me/shark/kn

# Create a test profile
~/.local/bin/profile add e2e-test "E2E test profile"
~/.local/bin/profile set e2e-test ANTHROPIC_AUTH_TOKEN=sk-e2e-12345678
~/.local/bin/profile set e2e-test ANTHROPIC_BASE_URL=https://api.e2e-test.com

# Source wrapper and test with a fake "claude" that just prints env
source ~/.claude-profiles/shell-rc

# Override claude temporarily for testing
claude() {
    echo "--- Env vars injected: ---"
    env | grep -E "ANTHROPIC_|OPENAI_" || echo "(none)"
}

# Test profile launch
claude e2e-test

# Verify output shows the env vars
```

Expected: Output shows `ANTHROPIC_AUTH_TOKEN=sk-e2e-12345678` and `ANTHROPIC_BASE_URL=https://api.e2e-test.com`.

- [ ] **Step 2: Clean up test profile**

```bash
~/.local/bin/profile remove e2e-test
```

- [ ] **Step 3: Restore user's real config**

Run:
```bash
# If ~/.claude-profiles/config.yaml was overwritten by template,
# re-run init to import actual settings
~/.local/bin/profile init
~/.local/bin/profile list
```

---

## Self-Review Checklist

### Spec coverage
- [x] config.yaml schema → Task 2 (`lib/config.py` — read/write), Task 1 (template)
- [x] Shell wrapper mechanism (claude + codex) → Task 5
- [x] Interactive selection (fzf/select) → Task 5 (`_ai_launch` function)
- [x] Management CLI (list/show/env/init) → Task 3
- [x] Management CLI (add/remove/set/unset/default) → Task 4
- [x] Bootstrap from settings.json → Task 3 (`cmd_init`)
- [x] Example config with deepseek + codex → Task 1 (template)
- [x] File locking for concurrent write safety → Task 2 (`_acquire_lock`, `_release_lock`)
- [x] Shared module for CLI + future UI → Task 2 (`lib/config.py`, imported by `bin/profile`)
- [x] Non-goals addressed: no encryption, no sync, no project binding

### Placeholder scan
- [x] No TBDs or TODOs
- [x] Every code step has complete, runnable code
- [x] All commands have expected output

### Type consistency
- [x] `read_config()` returns `{"profiles": {...}, "default": "..."}` → in `lib/config.py`
- [x] `write_config()` accepts same shape + acquires `fcntl.flock` before writing → in `lib/config.py`
- [x] `cfg.get_profile()`, `cfg.list_profiles()`, `cfg.get_default()` are the public API for UI consumption
- [x] `bin/profile` imports `config as cfg` — calls `cfg.read_config()`, `cfg.write_config()`
- [x] Shell wrapper calls `profile env <name>` — outputs `export KEY='VAL'`
- [x] `profile names` outputs tab-separated — `cut -f1` gets name

### Future UI readiness
- [x] UI can `import config` from `~/.claude-profiles/lib/config.py` — same module, no duplication
- [x] UI can call `read_config()` / `write_config()` directly — file lock prevents races
- [x] UI can shell out to `profile <command>` as alternative — both paths work
- [x] No new state or schema changes needed when UI is added
