"""Token usage recorder — called by Stop/SessionEnd hooks.
Reads structured JSON from stdin, extracts token usage, appends to usage.jsonl.
Supports Claude Code, Codex transcripts.
"""

import sys, json, os
from datetime import datetime, timezone

USAGE_FILE = os.path.join(
    os.environ.get("KN_HOME", os.path.expanduser("~/.kn")), "usage.jsonl"
)

PROJECTS_FILE = os.path.join(
    os.environ.get("KN_HOME", os.path.expanduser("~/.kn")), "projects.json"
)


def _resolve_project():
    """Read KN_WORKING_DIR, auto-register project, return (path, name)."""
    working_dir = os.environ.get("KN_WORKING_DIR", "")
    if not working_dir:
        # Backward compatibility: try old env var
        working_dir = os.environ.get("KN_PROJECT_DIR", "")
    if not working_dir:
        return None, None

    # Normalize: realpath resolves symlinks and normalizes path separators
    try:
        working_dir = os.path.realpath(working_dir)
    except OSError:
        working_dir = os.path.abspath(working_dir)

    project_name = os.path.basename(working_dir.rstrip(os.sep))

    # Auto-register in projects.json
    _ensure_project_registered(working_dir, project_name)

    return working_dir, project_name


def _ensure_project_registered(path, name):
    """Add project to projects.json if not already present.

    Uses fcntl.flock for cross-process safety (same pattern as
    the Rust backend's config lock), with a simple read-merge-write
    fallback if fcntl is unavailable.
    """
    try:
        import fcntl

        os.makedirs(os.path.dirname(PROJECTS_FILE), exist_ok=True)
        fd = os.open(PROJECTS_FILE, os.O_RDWR | os.O_CREAT, 0o644)
        try:
            fcntl.flock(fd, fcntl.LOCK_EX)
            os.lseek(fd, 0, 0)
            raw = os.read(fd, 16384) or b"[]"
            projects = json.loads(raw.decode("utf-8"))
        except (json.JSONDecodeError, OSError):
            projects = []
            os.lseek(fd, 0, 0)

        if not isinstance(projects, list):
            projects = []

        # Check if path already registered
        if any(p.get("path") == path for p in projects if isinstance(p, dict)):
            fcntl.flock(fd, fcntl.LOCK_UN)
            os.close(fd)
            return

        projects.append({"name": name, "path": path})
        data = json.dumps(projects, indent=2, ensure_ascii=False)

        os.ftruncate(fd, 0)
        os.lseek(fd, 0, 0)
        os.write(fd, data.encode("utf-8"))
        fcntl.flock(fd, fcntl.LOCK_UN)
        os.close(fd)
        return
    except ImportError:
        # fcntl not available; fall back to simple read-merge-write
        pass

    # Fallback path (fcntl import failed)
    try:
        os.makedirs(os.path.dirname(PROJECTS_FILE), exist_ok=True)
        try:
            with open(PROJECTS_FILE, "r", encoding="utf-8") as f:
                projects = json.load(f)
        except (FileNotFoundError, json.JSONDecodeError):
            projects = []

        if not isinstance(projects, list):
            projects = []

        if any(p.get("path") == path for p in projects if isinstance(p, dict)):
            return

        projects.append({"name": name, "path": path})
        with open(PROJECTS_FILE, "w", encoding="utf-8") as f:
            json.dump(projects, f, indent=2, ensure_ascii=False)
    except OSError:
        pass


def main():
    try:
        data = json.load(sys.stdin)
    except (json.JSONDecodeError, OSError):
        sys.exit(0)

    profile = os.environ.get("KN_PROFILE", "")
    tool = os.environ.get("KN_CLI_TOOL", "")

    usage = extract(data)
    if not usage:
        sys.exit(0)

    project_path, project_name = _resolve_project()

    record = {
        "ts": datetime.now(timezone.utc).isoformat(),
        "profile": profile,
        "tool": tool,
        **usage,
    }
    if project_path:
        record["project_path"] = project_path
        if project_name:
            record["project_name"] = project_name

    try:
        os.makedirs(os.path.dirname(USAGE_FILE), exist_ok=True)
        with open(USAGE_FILE, "a", encoding="utf-8") as f:
            f.write(json.dumps(record, ensure_ascii=False) + "\n")
    except OSError:
        pass

    sys.exit(0)


def extract(data):
    """Extract token usage from hook payload. Returns dict or None."""
    # Codex session/event payload: event_msg token_count carries cumulative usage.
    codex_usage = extract_codex_token_count(data)
    if codex_usage:
        return codex_usage
    # Codex: TurnComplete carries token_usage
    if "token_usage" in data:
        u = data["token_usage"]
        return {
            "model": str(u.get("model", "")),
            "tokens_in": int(u.get("input", u.get("input_tokens", 0))),
            "tokens_out": int(u.get("output", u.get("output_tokens", 0))),
        }
    # Claude Code: usage field in Stop/SessionEnd
    if "usage" in data:
        u = data["usage"]
        return {
            "model": str(u.get("model", "")),
            "tokens_in": int(u.get("input_tokens", u.get("input", 0))),
            "tokens_out": int(u.get("output_tokens", u.get("output", 0))),
        }
    # Claude Code / Codex: no usage inline, read transcript file
    if "transcript_path" in data:
        return extract_from_transcript(data["transcript_path"])
    if "transcriptPath" in data:
        return extract_from_transcript(data["transcriptPath"])
    # Generic fallback: top-level tokens_in / tokens_out
    if "tokens_in" in data or "tokens_out" in data:
        return {
            "model": str(data.get("model", "")),
            "tokens_in": int(data.get("tokens_in", 0)),
            "tokens_out": int(data.get("tokens_out", 0)),
        }
    return None


def extract_codex_token_count(entry):
    """Extract cumulative Codex token usage from an event_msg/token_count entry."""
    payload = entry.get("payload", {})
    if not isinstance(payload, dict) or payload.get("type") != "token_count":
        return None

    info = payload.get("info", {})
    if not isinstance(info, dict):
        return None

    u = info.get("total_token_usage") or info.get("last_token_usage")
    if not isinstance(u, dict):
        return None

    tokens_in = _in(u)
    tokens_out = _out(u)
    if tokens_in == 0 and tokens_out == 0:
        return None

    return {
        "model": str(u.get("model", entry.get("model", default_model()))),
        "tokens_in": tokens_in,
        "tokens_out": tokens_out,
    }


def extract_from_transcript(path):
    """Read transcript, sum usage from all turns.
    Handles Claude Code/Codex (JSONL).
    """
    total_in = 0
    total_out = 0
    model = ""
    latest_codex_usage = None

    try:
        with open(path, encoding="utf-8") as f:
            raw = f.read()
    except OSError:
        return None

    # ── JSONL: Claude Code / Codex transcript, one JSON per line ──
    for line in raw.splitlines():
        line = line.strip()
        if not line:
            continue
        try:
            entry = json.loads(line)
        except json.JSONDecodeError:
            continue

        codex_usage = extract_codex_token_count(entry)
        if codex_usage:
            latest_codex_usage = codex_usage
            continue

        # Claude Code / Codex: message.role == "assistant" with usage
        msg = entry.get("message", {})
        if isinstance(msg, dict) and msg.get("role") == "assistant":
            u = msg.get("usage")
            if u:
                total_in += _in(u)
                total_out += _out(u)
                if not model:
                    model = msg.get("model", "")
            continue

        # Generic: usage at entry level
        u = entry.get("usage")
        if isinstance(u, dict):
            total_in += _in(u)
            total_out += _out(u)
            if not model:
                model = entry.get("model", u.get("model", ""))
            continue

    # Codex token_count entries are cumulative; use the last one, do not sum.
    # But token_count events don't carry the model name — merge it from
    # assistant messages or the config.toml fallback when available.
    if latest_codex_usage:
        if not latest_codex_usage.get("model") and model:
            latest_codex_usage["model"] = model
        elif not latest_codex_usage.get("model"):
            latest_codex_usage["model"] = default_model()
        return latest_codex_usage

    if total_in == 0 and total_out == 0:
        return None

    return {
        "model": str(model),
        "tokens_in": total_in,
        "tokens_out": total_out,
    }


def _in(u):
    """Extract input tokens from a usage dict (multiple field name fallbacks)."""
    return int(u.get("input_tokens", u.get("prompt_tokens",
           u.get("input", u.get("promptTokenCount", 0)))))


def _out(u):
    """Extract output tokens from a usage dict (multiple field name fallbacks)."""
    return int(u.get("output_tokens", u.get("completion_tokens",
           u.get("output", u.get("candidatesTokenCount", 0)))))


def _codex_config_model():
    """Read model from ~/.codex/config.toml as last-resort fallback."""
    try:
        path = os.path.join(os.path.expanduser("~"), ".codex", "config.toml")
        with open(path) as f:
            for line in f:
                line = line.strip()
                if line.startswith("model "):
                    return line.split("=", 1)[1].strip().strip('"').strip("'")
    except OSError:
        pass
    return ""


def default_model():
    return os.environ.get("OPENAI_MODEL") or os.environ.get("CODEX_MODEL") or _codex_config_model() or ""


if __name__ == "__main__":
    main()
