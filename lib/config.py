"""kn — shared config module.

Reads/writes ~/.kn/config.yaml.
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

_default_config_dir = os.path.join(os.path.expanduser("~"), ".kn")
CONFIG_DIR = os.environ.get("KN_HOME", _default_config_dir)
# Legacy env var support
if CONFIG_DIR == _default_config_dir:
    CONFIG_DIR = os.environ.get("CLAUDE_PROFILES_HOME", CONFIG_DIR)
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
            # Rotate backups before overwriting (keep 3 generations)
            if os.path.exists(CONFIG_FILE):
                try:
                    import shutil
                    # Rotate .bak.2 → .bak.3, .bak.1 → .bak.2, .bak → .bak.1
                    for i in range(2, 0, -1):
                        older = f"{BACKUP_FILE}.{i}"
                        newer = f"{BACKUP_FILE}.{i + 1}"
                        if os.path.exists(older):
                            shutil.move(older, newer)
                    if os.path.exists(BACKUP_FILE):
                        shutil.move(BACKUP_FILE, f"{BACKUP_FILE}.1")
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

def _detect_indent_unit(text):
    """Detect the indent unit from the first indented line. Defaults to 2."""
    for line in text.split("\n"):
        stripped = line.strip()
        if stripped and not stripped.startswith("#"):
            indent = len(line) - len(line.lstrip(" "))
            if indent > 0:
                return indent
    return 2

def _parse_yaml(text):
    """Parse our constrained YAML subset.

    Indent structure (levels = multiples of detected unit):
      - 0: top-level scalars (default:)
      - 1 unit: profile names (keys in the profiles mapping)
      - 2 units: desc: and env: (keys inside a profile mapping)
      - 3 units: env var key: value entries (keys inside the env mapping)

    The indent unit is auto-detected from the first indented line;
    defaults to 2 spaces if only top-level lines exist.
    Supports block scalars (| / >) for multi-line desc and env var values.
    """
    # Auto-detect indent unit from first non-zero indent line
    indent_unit = _detect_indent_unit(text)

    config = {"profiles": {}}
    current_profile = None
    in_env = False
    block_scalar_key = None     # key name being filled by block scalar
    block_scalar_indent = 0      # minimum indent for continuation lines
    block_scalar_lines = []      # collected lines

    def _flush_block_scalar():
        """Commit any in-progress block scalar to the config."""
        nonlocal block_scalar_key, block_scalar_indent, block_scalar_lines
        if block_scalar_key is None:
            return
        val = "\n".join(block_scalar_lines)
        # Preserve trailing newline for literal style (|)
        if block_scalar_lines and block_scalar_lines[-1] == "":
            val += "\n"
        if current_profile and in_env:
            config["profiles"][current_profile]["env"][block_scalar_key] = val
        elif current_profile and block_scalar_key == "desc":
            config["profiles"][current_profile]["desc"] = val
        block_scalar_key = None
        block_scalar_indent = 0
        block_scalar_lines = []

    for raw in text.split("\n"):
        line = raw.rstrip()
        stripped = line.strip()

        if not stripped or stripped.startswith("#"):
            if block_scalar_key is not None:
                # Empty lines within block scalar are preserved content
                block_scalar_lines.append("")
            continue

        indent = len(line) - len(line.lstrip(" "))

        # Check for block scalar indicator (| or >) in a key: value line
        if ":" in stripped:
            key, _, val = stripped.partition(":")
            val_stripped = val.strip()
            if val_stripped in ("|", ">"):
                # Flush any previous block scalar first
                _flush_block_scalar()
                block_scalar_key = key.strip()
                block_scalar_indent = indent + indent_unit
                continue

        # Collect continuation lines for current block scalar
        if block_scalar_key is not None and indent >= block_scalar_indent:
            content = line[block_scalar_indent:] if len(line) >= block_scalar_indent else ""
            block_scalar_lines.append(content)
            continue
        else:
            _flush_block_scalar()

        if indent == 0:
            in_env = False
            if ":" in stripped:
                key, _, val = stripped.partition(":")
                key = key.strip()
                val = _yaml_val(val)
                if key == "default":
                    config["default"] = val
            continue

        if indent == indent_unit:
            in_env = False
            if ":" in stripped and not stripped.startswith("desc:") and not stripped.startswith("env:"):
                key = stripped.partition(":")[0].strip()
                current_profile = key
                config["profiles"][current_profile] = {"desc": "", "env": {}}
            continue

        if indent == indent_unit * 2 and current_profile:
            if ":" in stripped:
                key, _, val = stripped.partition(":")
                key = key.strip()
                val = _yaml_val(val)
                if key == "desc":
                    config["profiles"][current_profile]["desc"] = val
                elif key == "env":
                    in_env = (val != "{}")
                continue

        if indent == indent_unit * 3 and in_env and current_profile:
            if ":" in stripped:
                key, _, val = stripped.partition(":")
                key = key.strip()
                val = _yaml_val(val)
                config["profiles"][current_profile]["env"][key] = val
            continue

    _flush_block_scalar()
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
        _emit_yaml_scalar(lines, "desc", profile.get("desc", ""), 4)
        lines.append("    env:")
        env = profile.get("env", {})
        if env:
            for k, v in sorted(env.items()):
                _emit_yaml_scalar(lines, k, v, 6)
        else:
            lines.append("      {}")
        lines.append("")

    return "\n".join(lines) + "\n"


def _emit_yaml_scalar(lines, key, val, indent):
    """Emit a YAML key: value line, using block scalar (|) if value has newlines."""
    prefix = " " * indent
    if "\n" in val:
        lines.append(f"{prefix}{key}: |")
        for line in val.split("\n"):
            lines.append(f"{prefix}  {line}")
    else:
        lines.append(f"{prefix}{key}: {_quote_yaml(val)}")


def _quote_yaml(val):
    """Quote value if it contains YAML-special characters or is a YAML reserved word."""
    if not val:
        return '""'
    # Quote YAML boolean/null/number-like values for compatibility with
    # Rust serde_yaml (which would otherwise interpret them as non-strings).
    yaml_reserved = {"true", "false", "yes", "no", "on", "off", "null", "~", "y", "n"}
    if val.lower() in yaml_reserved:
        return f'"{val}"'
    # Quote numeric-looking strings (including floats like "1.0", negatives,
    # scientific notation like "1e5", and signed numbers like "+5").
    # Use float() instead of regex to catch all YAML number formats.
    try:
        float(val)
        return f'"{val}"'
    except ValueError:
        pass
    special = set(" :#{}[]&*!|>\"'@`,")
    if any(c in special for c in val):
        escaped = val.replace("\\", "\\\\").replace('"', '\\"')
        return f'"{escaped}"'
    return val
