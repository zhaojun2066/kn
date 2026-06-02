"""AI Profile Manager — shared config module.

Reads/writes ~/.claude-profiles/config.yaml.
Imported by both CLI (bin/profile) and future UI.
Uses fcntl.flock for concurrent write safety.
"""

import os
import fcntl
import time

CONFIG_DIR = os.environ.get("CLAUDE_PROFILES_HOME", os.path.expanduser("~/.claude-profiles"))
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

    Indent structure (standard YAML):
      - 0: top-level scalars (default:)
      - 2: profile names (keys in the profiles mapping)
      - 4: desc: and env: (keys inside a profile mapping)
      - 6: env var key: value entries (keys inside the env mapping)

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
            if ":" in stripped and not stripped.startswith("desc:") and not stripped.startswith("env:"):
                key = stripped.partition(":")[0].strip()
                current_profile = key
                config["profiles"][current_profile] = {"desc": "", "env": {}}
            continue

        if indent == 4 and current_profile:
            if ":" in stripped:
                key, _, val = stripped.partition(":")
                key = key.strip()
                val = _yaml_val(val)
                if key == "desc":
                    config["profiles"][current_profile]["desc"] = val
                elif key == "env":
                    in_env = (val != "{}")
                continue

        if indent == 6 and in_env and current_profile:
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
