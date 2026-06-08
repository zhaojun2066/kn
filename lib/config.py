"""AI Profile Manager — shared config module.

Reads/writes ~/.claude-profiles/config.yaml.
Imported by both CLI (bin/profile) and future UI.
Uses fcntl.flock for concurrent write safety.
"""

import os
import time

# Cross-platform file locking: fcntl on Unix, msvcrt on Windows
try:
    import fcntl
    _HAS_FCNTL = True
except ImportError:
    _HAS_FCNTL = False
    import msvcrt

_default_config_dir = os.path.join(os.path.expanduser("~"), ".claude-profiles")
CONFIG_DIR = os.environ.get("CLAUDE_PROFILES_HOME", _default_config_dir)
CONFIG_FILE = os.path.join(CONFIG_DIR, "config.yaml")
BACKUP_FILE = os.path.join(CONFIG_DIR, "config.yaml.bak")
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
    """Serialize config dict back to config.yaml, with file locking.

    Automatically backs up existing config.yaml → config.yaml.bak before writing.
    """
    text = _format_yaml(config)

    os.makedirs(CONFIG_DIR, exist_ok=True)

    with open(LOCK_FILE, "w") as lf:
        _acquire_lock(lf)
        try:
            # Backup existing config before overwriting
            if os.path.exists(CONFIG_FILE):
                try:
                    import shutil
                    shutil.copy2(CONFIG_FILE, BACKUP_FILE)
                except Exception:
                    pass  # backup is best-effort; don't block the write

            # Atomic-ish: write to temp file then rename
            tmp_file = CONFIG_FILE + ".tmp"
            with open(tmp_file, "w") as f:
                f.write(text)
                f.flush()
                os.fsync(f.fileno())
            if os.name == 'nt':
                # Windows: os.replace() fails with PermissionError if the
                # target file is temporarily locked by antivirus/backup.
                for attempt in range(5):
                    try:
                        os.replace(tmp_file, CONFIG_FILE)
                        break
                    except PermissionError:
                        if attempt == 4:
                            raise
                        time.sleep(0.1 * (attempt + 1))
            else:
                os.replace(tmp_file, CONFIG_FILE)
        finally:
            _release_lock(lf)


def restore_backup():
    """Restore config.yaml from backup if it exists. Returns True on success."""
    if not os.path.exists(BACKUP_FILE):
        return False
    import shutil
    shutil.copy2(BACKUP_FILE, CONFIG_FILE)
    return True


def backup_exists():
    """Check whether a backup file exists."""
    return os.path.exists(BACKUP_FILE)


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
    if _HAS_FCNTL:
        deadline = time.time() + 5
        while True:
            try:
                fcntl.flock(lock_file, fcntl.LOCK_EX | fcntl.LOCK_NB)
                return
            except BlockingIOError:
                if time.time() > deadline:
                    raise TimeoutError("Could not acquire config lock after 5s")
                time.sleep(0.05)
    else:
        # Windows: use msvcrt.locking (C runtime byte-range lock)
        deadline = time.time() + 5
        while True:
            try:
                msvcrt.locking(lock_file.fileno(), msvcrt.LK_NBLCK, 0x7fffffff)
                return
            except (IOError, OSError):
                if time.time() > deadline:
                    raise TimeoutError("Could not acquire config lock after 5s")
                time.sleep(0.05)


def _release_lock(lock_file):
    """Release the file lock."""
    if _HAS_FCNTL:
        fcntl.flock(lock_file, fcntl.LOCK_UN)
    else:
        msvcrt.locking(lock_file.fileno(), msvcrt.LK_UNLCK, 0x7fffffff)


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
    # Strip inline comments BEFORE quote stripping, so " #" inside
    # quoted values is preserved as literal content (bug M22 fix).
    if " #" in v:
        # Only strip if the " #" is outside quotes
        in_quote = False
        quote_char = None
        comment_idx = None
        for i, ch in enumerate(v):
            if ch in ('"', "'") and (i == 0 or v[i-1] != '\\'):
                if not in_quote:
                    in_quote = True
                    quote_char = ch
                elif ch == quote_char:
                    in_quote = False
                    quote_char = None
            if ch == '#' and v[i-1:i+1] == ' #' and not in_quote:
                comment_idx = i - 1
                break
        if comment_idx is not None:
            v = v[:comment_idx].strip()
    if len(v) >= 2:
        if (v.startswith('"') and v.endswith('"')) or \
           (v.startswith("'") and v.endswith("'")):
            return v[1:-1]
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
            lines.append(f'    desc: {_quote_yaml(profile["desc"])}')
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
    """Quote value if it contains YAML-special characters or is a YAML reserved word."""
    if not val:
        return '""'
    # Quote YAML boolean/null/number-like values for compatibility with
    # Rust serde_yaml (which would otherwise interpret them as non-strings).
    yaml_reserved = {"true", "false", "yes", "no", "on", "off", "null", "~", "y", "n"}
    if val.lower() in yaml_reserved:
        return f'"{val}"'
    # Quote numeric-looking strings (including floats like "1.0" and negatives)
    if val.replace(".", "", 1).replace("-", "", 1).isdigit() and any(c.isdigit() for c in val):
        return f'"{val}"'
    special = set(" :#{}[]&*!|>\"'@`,")
    if any(c in special for c in val):
        escaped = val.replace("\\", "\\\\").replace('"', '\\"')
        return f'"{escaped}"'
    return val
