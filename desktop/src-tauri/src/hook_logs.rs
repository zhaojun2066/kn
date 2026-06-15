//! Hook Execution Logs — wrapper script + log viewer for hook execution history.
//!
//! ## Architecture
//!
//! - `run-with-log.sh` is a wrapper script embedded at compile time. Users configure
//!   their hooks to run `run-with-log.sh <hook_id> -- <actual_command>` instead of
//!   the command directly. The wrapper measures duration, captures exit code and
//!   output previews, then writes a JSON log to `~/.kn/hook-logs/`.
//! - `get_hook_execution_logs` is a Tauri command that scans the log directory,
//!   parses JSON files, and returns filtered/sorted results for the UI.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

// ── Embedded wrapper script ────────────────────────────────────

/// The `run-with-log.sh` wrapper script, embedded at compile time.
/// Written to `~/.kn/hooks/run-with-log.sh` on app startup.
pub const RUN_WITH_LOG_SCRIPT: &str = r##"#!/usr/bin/env bash
# run-with-log.sh — Hook execution wrapper with logging
# Usage: run-with-log.sh <hook_id> <command...>
#
# Runs the given command, captures exit code / stdout preview / stderr preview /
# duration, and writes a JSON log to ~/.kn/hook-logs/.
# The command's stdout and stderr are passed through to the parent process
# so the hook's output is still available to Claude Code / Codex.
set -euo pipefail

if [ $# -lt 2 ]; then
    echo "Usage: run-with-log.sh <hook_id> <command...>" >&2
    exit 1
fi

HOOK_ID="$1"
shift

LOGS_DIR="${KN_HOME:-${HOME}/.kn}/hook-logs"
mkdir -p "${LOGS_DIR}"

# Find working python (python3 on Unix, python on Windows)
PYTHON=""
for py in python3 python; do
    if command -v "$py" >/dev/null 2>&1; then
        PYTHON="$py"
        break
    fi
done

# Cross-platform temp dir
TMPDIR="${TMPDIR:-${TEMP:-${TMP:-/tmp}}}"

# ISO 8601 UTC timestamp via python (avoids BSD vs GNU date differences)
TIMESTAMP=""
if [ -n "${PYTHON}" ]; then
    TIMESTAMP=$("${PYTHON}" -c "from datetime import datetime,timezone; print(datetime.now(timezone.utc).strftime('%Y-%m-%dT%H:%M:%SZ'))" 2>/dev/null || echo "")
fi
[ -z "${TIMESTAMP}" ] && TIMESTAMP=$(date -u +"%Y-%m-%dT%H:%M:%SZ" 2>/dev/null || echo "")
# Final fallback: epoch seconds (available even in minimal containers)
[ -z "${TIMESTAMP}" ] && TIMESTAMP="ts-$(date +%s 2>/dev/null || echo 0)"

# Start time in ms
START_MS=0
if [ -n "${PYTHON}" ]; then
    START_MS=$("${PYTHON}" -c "import time; print(int(time.time()*1000))" 2>/dev/null || echo 0)
fi

# Temp files (use TMPDIR instead of hardcoded /tmp)
TMP_OUT="${TMPDIR}/hlog-out-$$"
TMP_ERR="${TMPDIR}/hlog-err-$$"

# Run the command
EXIT_CODE=0
"$@" >"${TMP_OUT}" 2>"${TMP_ERR}" || EXIT_CODE=$?

# End time and duration
END_MS=0
if [ -n "${PYTHON}" ]; then
    END_MS=$("${PYTHON}" -c "import time; print(int(time.time()*1000))" 2>/dev/null || echo 0)
fi
DURATION_MS=$((END_MS - START_MS))
if [ "${DURATION_MS}" -lt 0 ]; then DURATION_MS=0; fi

# JSON-safe previews (first 500 chars)
OUT_PREVIEW='""'
ERR_PREVIEW='""'
if [ -n "${PYTHON}" ]; then
    OUT_PREVIEW=$(head -c 500 "${TMP_OUT}" 2>/dev/null | \
        "${PYTHON}" -c "import sys,json; print(json.dumps(sys.stdin.read()))" 2>/dev/null || echo '""')
    ERR_PREVIEW=$(head -c 500 "${TMP_ERR}" 2>/dev/null | \
        "${PYTHON}" -c "import sys,json; print(json.dumps(sys.stdin.read()))" 2>/dev/null || echo '""')
fi

# Sanitize hook_id for safe filename (replace / and : with _)
SAFE_ID=$(echo "${HOOK_ID}" | tr '/:' '__')

# Write JSON log via python (preferred) or shell fallback
LOG_FILE="${LOGS_DIR}/${SAFE_ID}-${TIMESTAMP}.json"

if [ -n "${PYTHON}" ]; then
    export _HL_HOOK_ID="${HOOK_ID}"
    export _HL_TIMESTAMP="${TIMESTAMP}"
    export _HL_EXIT_CODE="${EXIT_CODE}"
    export _HL_DURATION_MS="${DURATION_MS}"
    export _HL_OUT_PREVIEW="${OUT_PREVIEW}"
    export _HL_ERR_PREVIEW="${ERR_PREVIEW}"
    export _HL_LOG_FILE="${LOG_FILE}"

    set +e
    "${PYTHON}" << 'PYEOF' 2>/dev/null
import os, json

def safe_json(v, default=""):
    try:
        return json.loads(v)
    except (json.JSONDecodeError, TypeError):
        return v if v else default

log = {
    "hookId": os.environ.get("_HL_HOOK_ID", ""),
    "timestamp": os.environ.get("_HL_TIMESTAMP", ""),
    "exitCode": int(os.environ.get("_HL_EXIT_CODE", "0")),
    "durationMs": int(os.environ.get("_HL_DURATION_MS", "0")),
    "outputPreview": safe_json(os.environ.get("_HL_OUT_PREVIEW", '""')),
    "errorPreview": safe_json(os.environ.get("_HL_ERR_PREVIEW", '""')),
}
with open(os.environ["_HL_LOG_FILE"], "w") as f:
    json.dump(log, f, indent=2, ensure_ascii=False)
PYEOF
    PY_EXIT=$?
    set -e
else
    PY_EXIT=1
fi

if [ "${PY_EXIT:-1}" -ne 0 ] || [ ! -f "${LOG_FILE}" ]; then
    cat > "${LOG_FILE}" << JSONFALLBACK
{
  "hookId": "${HOOK_ID}",
  "timestamp": "${TIMESTAMP}",
  "exitCode": ${EXIT_CODE},
  "durationMs": ${DURATION_MS},
  "outputPreview": "",
  "errorPreview": ""
}
JSONFALLBACK
fi

# Pass through original stdout/stderr to parent
cat "${TMP_OUT}"
cat "${TMP_ERR}" >&2

rm -f "${TMP_OUT}" "${TMP_ERR}"
exit ${EXIT_CODE}
"##;

// ── Types ──────────────────────────────────────────────────────

/// A single hook execution log entry, deserialized from the JSON files
/// written by `run-with-log.sh`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HookExecutionLog {
    pub hook_id: String,
    pub timestamp: String,
    #[serde(default)]
    pub exit_code: Option<i32>,
    #[serde(default)]
    pub duration_ms: Option<u64>,
    #[serde(default)]
    pub output_preview: Option<String>,
    #[serde(default)]
    pub error_preview: Option<String>,
}

impl HookExecutionLog {
    /// Whether the hook execution was successful (exit code 0, or no exit code).
    #[allow(dead_code)]
    pub fn success(&self) -> bool {
        self.exit_code.unwrap_or(0) == 0
    }
}

// ── Helpers ────────────────────────────────────────────────────

fn hook_logs_dir() -> PathBuf {
    crate::config_dir().join("hook-logs")
}

fn hooks_dir() -> PathBuf {
    crate::config_dir().join("hooks")
}

// ── Write wrapper script on startup ───────────────────────────

/// Write `run-with-log.sh` to `~/.kn/hooks/run-with-log.sh`.
///
/// Only overwrites if the content has changed (preserves user customizations).
/// Call this from `ensure_shell_rc()` or on app startup.
pub fn write_run_with_log_script() -> Result<(), String> {
    let dir = hooks_dir();
    fs::create_dir_all(&dir).map_err(|e| format!("创建 hooks 目录失败: {}", e))?;

    let script_path = dir.join("run-with-log.sh");
    let needs_write = match fs::read_to_string(&script_path) {
        Ok(existing) => existing != RUN_WITH_LOG_SCRIPT,
        Err(_) => true,
    };

    if needs_write {
        fs::write(&script_path, RUN_WITH_LOG_SCRIPT)
            .map_err(|e| format!("写入 run-with-log.sh 失败: {}", e))?;
        // Make it executable
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&script_path)
                .map_err(|e| format!("读取 run-with-log.sh 权限失败: {}", e))?
                .permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&script_path, perms).ok();
        }
    }

    Ok(())
}

// ── Tauri command ──────────────────────────────────────────────

/// Get hook execution logs, optionally filtered by `hook_id`.
///
/// Scans `~/.kn/hook-logs/` for JSON log files, parses them,
/// filters by hook_id if provided, sorts by timestamp descending, and
/// returns up to `limit` entries.
#[tauri::command]
pub fn get_hook_execution_logs(
    hook_id: Option<String>,
    limit: Option<u32>,
) -> Result<Vec<HookExecutionLog>, String> {
    let dir = hook_logs_dir();
    let limit = limit.unwrap_or(50).min(200) as usize;

    if !dir.exists() {
        return Ok(Vec::new());
    }

    let mut logs: Vec<HookExecutionLog> = Vec::new();

    let entries = match fs::read_dir(&dir) {
        Ok(e) => e,
        Err(e) => return Err(format!("读取 hook-logs 目录失败: {}", e)),
    };

    for entry in entries {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };

        let path = entry.path();
        if path.extension().map_or(true, |ext| ext != "json") {
            continue;
        }

        let content = match fs::read_to_string(&path) {
            Ok(c) => c,
            Err(_) => continue,
        };

        let log: HookExecutionLog = match serde_json::from_str(&content) {
            Ok(l) => l,
            Err(_) => continue,
        };

        // Filter by hook_id if specified
        if let Some(ref filter_id) = hook_id {
            if log.hook_id != *filter_id {
                continue;
            }
        }

        logs.push(log);
    }

    // Sort by timestamp descending (newest first)
    logs.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));

    // Apply limit
    if logs.len() > limit {
        logs.truncate(limit);
    }

    Ok(logs)
}
