#!/usr/bin/env python3
"""System hook for kn remote approval.

It only forwards structured PermissionRequest and PreToolUse payloads from a
kn-owned session.  It never scans terminal output and never writes to a PTY.
"""

import json
import os
import socket
import sys
import uuid


KN_HOME = os.environ.get("KN_HOME", os.path.expanduser("~/.kn"))
IPC_SOCK = os.path.join(KN_HOME, "agent", "ipc.sock")
APPROVAL_CONFIG = os.path.join(KN_HOME, "agent", "approval-config.json")
TIMEOUT_SECONDS = 305
SUPPORTED_CLI = {"claude", "codex", "qoderclicn"}


def main():
    session_id = os.environ.get("KN_SESSION_ID", "").strip()
    cli_type = os.environ.get("KN_CLI_TOOL", "").strip().lower()
    if not session_id or cli_type not in SUPPORTED_CLI:
        return 0

    try:
        payload = json.load(sys.stdin)
    except Exception:
        return 0

    event_name = str(payload.get("hook_event_name") or payload.get("hookEventName") or payload.get("event") or "").strip()
    if event_name not in {"PermissionRequest", "PreToolUse"}:
        return 0

    # Hooks are installed permanently. When the feature is disabled, an
    # unavailable agent must not change the local CLI's ordinary behavior.
    if not remote_approval_enabled(cli_type, event_name):
        return 0

    response = request_decision({
        "requestKey": "approval-" + uuid.uuid4().hex,
        "sessionId": session_id,
        "cliType": cli_type,
        "eventName": event_name,
        "payload": payload,
    })
    if response.get("action") == "passthrough":
        return 0
    if response.get("decision") == "allowOnce":
        emit_allow(event_name)
        return 0

    # A missing agent / invalid IPC response is fail-closed for a kn session.
    emit_deny(event_name)
    return 2


def request_decision(params):
    request = {
        "id": "hook-" + uuid.uuid4().hex,
        "method": "approval_request",
        "params": params,
    }
    try:
        with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as sock:
            sock.settimeout(TIMEOUT_SECONDS)
            sock.connect(IPC_SOCK)
            sock.sendall((json.dumps(request, ensure_ascii=False) + "\n").encode("utf-8"))
            chunks = []
            while True:
                data = sock.recv(4096)
                if not data:
                    break
                chunks.append(data)
                if b"\n" in data:
                    break
        response = json.loads(b"".join(chunks).decode("utf-8", "replace").strip() or "{}")
        return response.get("result") or {}
    except Exception:
        return {"action": "decision", "decision": "deny", "reason": "agent_unavailable"}


def remote_approval_enabled(cli_type, event_name):
    try:
        with open(APPROVAL_CONFIG, encoding="utf-8") as config_file:
            config = json.load(config_file)
            if not config.get("enabled", False):
                return False
            mode_key = {
                "claude": "claudeMode",
                "codex": "codexMode",
                "qoderclicn": "qoderCnMode",
            }.get(cli_type)
            mode = config.get(mode_key) if mode_key else None
            return (
                (event_name == "PermissionRequest" and mode == "nativePermission")
                or (event_name == "PreToolUse" and mode == "preToolUse")
            )
    except Exception:
        return False


def emit_allow(event_name):
    # Claude Code and Codex CLI both accept hook-specific permission decisions.
    # Qoder CLI CN uses the same blockable command-hook contract for PreToolUse.
    print(json.dumps({
        "continue": True,
        "hookSpecificOutput": {
            "hookEventName": event_name,
            "permissionDecision": "allow",
        },
    }, ensure_ascii=False))


def emit_deny(event_name):
    print(json.dumps({
        "continue": False,
        "stopReason": "远程授权被拒绝或已超时",
        "hookSpecificOutput": {
            "hookEventName": event_name,
            "permissionDecision": "deny",
            "permissionDecisionReason": "远程授权被拒绝或已超时",
        },
    }, ensure_ascii=False))


if __name__ == "__main__":
    sys.exit(main())
