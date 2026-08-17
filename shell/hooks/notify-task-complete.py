"""System hook: report completed AI turns to kn-agent.

Called from Claude/Codex/Qoder Stop hooks. The hook must never block or fail
the CLI, so every error path exits successfully.
"""

import json
import os
import re
import socket
import sys
import time
import uuid
from datetime import datetime, timezone


KN_HOME = os.environ.get("KN_HOME", os.path.expanduser("~/.kn"))
EVENTS_DIR = os.path.join(KN_HOME, "events")
QUEUE_FILE = os.path.join(EVENTS_DIR, "task-complete.jsonl")
IPC_SOCK = os.path.join(KN_HOME, "agent", "ipc.sock")
MAX_ASSISTANT_MESSAGE_BYTES = 256 * 1024


def main():
    try:
        payload = json.load(sys.stdin)
    except Exception:
        return

    event = build_event(payload)
    if not event:
        return

    had_pending = append_queue(event)
    if not had_pending:
        send_ipc(event)


def build_event(payload):
    message = first_text(
        payload,
        [
            "last_assistant_message",
            "lastAssistantMessage",
            "assistant_message",
            "assistantMessage",
            "message",
        ],
    )
    if not message:
        message = transcript_last_assistant(payload)

    now = datetime.now(timezone.utc).isoformat()
    turn_id = first_text(payload, ["turn_id", "turnId", "conversation_id", "session_id", "sessionId"])
    native_session_id = first_text(payload, ["session_id", "sessionId", "transcript_path", "transcriptPath"])
    usage = extract_usage(payload)

    return {
        "eventId": stable_event_id(payload, turn_id, message),
        "eventName": str(payload.get("hook_event_name") or payload.get("event") or "Stop"),
        "tool": os.environ.get("KN_CLI_TOOL", "") or infer_tool(payload),
        "profile": os.environ.get("KN_PROFILE", ""),
        "projectPath": resolve_project_path(payload),
        "nativeSessionId": native_session_id or "",
        "sessionId": os.environ.get("KN_SESSION_ID", ""),
        "turnId": turn_id or "",
        "model": first_text(payload, ["model"]) or usage.get("model", ""),
        "tokensIn": usage.get("tokensIn", 0),
        "tokensOut": usage.get("tokensOut", 0),
        "durationMs": first_int(payload, ["duration_ms", "durationMs", "elapsed_ms", "elapsedMs"]),
        "finishedAt": now,
        "lastAssistantMessage": truncate_utf8(sanitize_text(message), MAX_ASSISTANT_MESSAGE_BYTES),
        "summary": summarize(message),
    }


def send_ipc(event):
    if not os.path.exists(IPC_SOCK):
        return
    req = {
        "id": "hook-" + uuid.uuid4().hex,
        "method": "task_complete_event",
        "params": event,
    }
    try:
        with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as sock:
            sock.settimeout(0.5)
            sock.connect(IPC_SOCK)
            sock.sendall((json.dumps(req, ensure_ascii=False) + "\n").encode("utf-8"))
            try:
                sock.recv(4096)
            except Exception:
                pass
    except Exception:
        return


def append_queue(event):
    try:
        os.makedirs(EVENTS_DIR, exist_ok=True)
        pending_before = queue_has_pending_event()
        with open(QUEUE_FILE, "a", encoding="utf-8") as f:
            f.write(json.dumps(event, ensure_ascii=False) + "\n")
        return pending_before
    except OSError:
        return False


def queue_has_pending_event():
    try:
        size = os.path.getsize(QUEUE_FILE)
    except OSError:
        return False
    try:
        with open(QUEUE_FILE + ".offset", encoding="utf-8") as f:
            offset = int((f.read() or "0").strip() or "0")
    except Exception:
        offset = 0
    return offset < size


def transcript_last_assistant(payload):
    path = first_text(payload, ["transcript_path", "transcriptPath"])
    if not path:
        return ""
    try:
        with open(path, encoding="utf-8") as f:
            lines = f.readlines()[-200:]
    except OSError:
        return ""

    for line in reversed(lines):
        try:
            item = json.loads(line)
        except Exception:
            continue
        role = str(item.get("role") or item.get("type") or "")
        if "assistant" not in role.lower():
            continue
        text = extract_text_value(item.get("content")) or extract_text_value(item.get("message"))
        if text:
            return text
    return ""


def extract_usage(payload):
    raw = payload.get("token_usage") or payload.get("usage") or {}
    if not isinstance(raw, dict):
        raw = {}
    tokens_in = int_value(raw.get("input") or raw.get("input_tokens") or raw.get("tokens_in"))
    tokens_out = int_value(raw.get("output") or raw.get("output_tokens") or raw.get("tokens_out"))
    return {
        "model": str(raw.get("model") or ""),
        "tokensIn": tokens_in,
        "tokensOut": tokens_out,
    }


def first_text(obj, names):
    for name in names:
        value = obj.get(name) if isinstance(obj, dict) else None
        if isinstance(value, str) and value.strip():
            return value.strip()
    return ""


def first_int(obj, names):
    for name in names:
        if isinstance(obj, dict) and name in obj:
            return int_value(obj.get(name))
    return None


def int_value(value):
    try:
        return int(value or 0)
    except Exception:
        return 0


def extract_text_value(value):
    if isinstance(value, str):
        return value
    if isinstance(value, list):
        parts = []
        for item in value:
            if isinstance(item, str):
                parts.append(item)
            elif isinstance(item, dict):
                text = item.get("text") or item.get("content")
                if isinstance(text, str):
                    parts.append(text)
        return "\n".join(parts)
    if isinstance(value, dict):
        return extract_text_value(value.get("content")) or extract_text_value(value.get("text"))
    return ""


def sanitize_text(text):
    text = re.sub(r"[\x00-\x08\x0b\x0c\x0e-\x1f\x7f]", "", text or "")
    return text.strip()


def summarize(text):
    text = re.sub(r"\s+", " ", sanitize_text(text))
    return text[:252] + "..." if len(text) > 255 else text


def truncate_utf8(text, max_bytes):
    raw = (text or "").encode("utf-8")
    if len(raw) <= max_bytes:
        return text or ""
    return raw[:max_bytes].decode("utf-8", "ignore").rstrip() + "\n\n[内容过长，已截断]"


def resolve_project_path(payload):
    cwd = os.environ.get("KN_WORKING_DIR") or os.environ.get("KN_PROJECT_DIR")
    if cwd:
        return os.path.realpath(cwd)
    return first_text(payload, ["cwd", "projectPath", "project_path"])


def infer_tool(payload):
    path = first_text(payload, ["transcript_path", "transcriptPath"])
    if "/.codex/" in path:
        return "codex"
    if "/.qoder" in path:
        return "qoder"
    if "/.claude/" in path:
        return "claude"
    return ""


def stable_event_id(payload, turn_id, message):
    explicit = first_text(payload, ["event_id", "eventId"])
    if explicit:
        return explicit
    base = "|".join([
        turn_id or "",
        first_text(payload, ["session_id", "sessionId"]),
        first_text(payload, ["transcript_path", "transcriptPath"]),
        message[:200] if message else "",
        str(int(time.time() * 1000)),
    ])
    return uuid.uuid5(uuid.NAMESPACE_URL, base).hex


if __name__ == "__main__":
    try:
        main()
    finally:
        sys.exit(0)
