# kn — Shell Wrapper (canonical source)
# Installed by install.sh and desktop app to ~/.kn/shell-rc
# Usage: ai <tool> [profile] [args...]
#        ai profile <command>

# Config directory: explicit KN_HOME wins. Otherwise, when this file is sourced
# from an installed shell-rc, use that shell-rc's own directory so dev
# (~/.kn-dev) and production (~/.kn) stay separated.
_KN_SHELL_RC_PATH=""
if [ -n "${BASH_SOURCE:-}" ]; then
    _KN_SHELL_RC_PATH="${BASH_SOURCE[0]}"
elif [ -n "${ZSH_VERSION:-}" ]; then
    _KN_SHELL_RC_PATH="$(eval 'printf "%s" "${(%):-%x}"' 2>/dev/null || true)"
fi

if [ -n "${KN_HOME:-}" ]; then
    KN_DIR="$KN_HOME"
elif [ -n "$_KN_SHELL_RC_PATH" ] && [ "$(basename "$_KN_SHELL_RC_PATH")" = "shell-rc" ]; then
    KN_DIR="$(cd "$(dirname "$_KN_SHELL_RC_PATH")" && pwd)"
elif [ -d "$HOME/.kn" ]; then
    KN_DIR="$HOME/.kn"
elif [ -d "$HOME/.claude-profiles" ]; then
    KN_DIR="$HOME/.claude-profiles"
else
    KN_DIR="$HOME/.kn"
fi
unset _KN_SHELL_RC_PATH
CONFIG="$KN_DIR/config.yaml"

# ── Resolve profile CLI (preferred but optional) ──
if [ -x "$KN_DIR/bin/profile" ]; then
    PROFILE_CMD="$KN_DIR/bin/profile"
elif [ -x "$HOME/.local/bin/profile" ]; then
    PROFILE_CMD="$HOME/.local/bin/profile"
elif command -v profile >/dev/null 2>&1; then
    PROFILE_CMD="profile"
else
    PROFILE_CMD=""
fi

# ── Profile env extraction ──
# Tries profile CLI first, falls back to pure-shell sed/awk (zero deps)
_profile_env() {
    local name="$1"
    if [ -n "$PROFILE_CMD" ]; then
        local output
        output=$("$PROFILE_CMD" env "$name" 2>/dev/null)
        if [ -n "$output" ]; then
            echo "$output"
            return 0
        fi
    fi
    # Pure-shell fallback
    [ ! -f "$CONFIG" ] && return 1
    sed -n "/^  ${name}:/,/^  [a-z]/p" "$CONFIG" | \
        awk -F': ' '/^      [A-Za-z_][A-Za-z0-9_]*:/ {
            k=$1; sub(/^      /,"",k)
            v=substr($0,length(k)+9)
            if (v ~ /^".*"$/) v=substr(v,2,length(v)-2)
            else if (v ~ /^'"'"'.*'"'"'$/) v=substr(v,2,length(v)-2)
            print "export " k "='"'"'" v "'"'"'"
        }'
}

_profile_exists() {
    local name="$1"
    [ -z "$name" ] && return 1
    if [ -n "$(_profile_env "$name" 2>/dev/null)" ]; then
        return 0
    fi
    [ -f "$CONFIG" ] && grep -q "^  ${name}:" "$CONFIG" 2>/dev/null
}

# ── List profiles ──
_profile_list() {
    if [ -n "$PROFILE_CMD" ]; then
        "$PROFILE_CMD" list 2>/dev/null && return
    fi
    [ ! -f "$CONFIG" ] && { echo "No config found at $CONFIG" >&2; return 1; }
    grep '^  [a-zA-Z]' "$CONFIG" | sed 's/^  \([a-zA-Z0-9_-]*\):.*/\1/' | while read -r name; do
        local is_default=""
        grep -q "^default: \"$name\"" "$CONFIG" && is_default=" (*)"
        echo "  $name$is_default"
    done
}

# ── Show profile env vars ──
_profile_show() {
    local name="$1"
    local output
    output=$(_profile_env "$name")
    if [ -z "$output" ]; then
        echo "Profile '$name' not found" >&2
        return 1
    fi
    echo "Profile: $name"
    echo "$output" | sed 's/^export //'
}

# ── Switch default profile ──
_profile_switch() {
    local name="$1"
    if [ -z "$name" ]; then
        echo "Usage: ai profile switch <name>" >&2
        return 1
    fi
    if grep -q "^  ${name}:" "$CONFIG" 2>/dev/null; then
        sed -i '' "s/^default:.*/default: \"$name\"/" "$CONFIG"
        echo "Default profile set to '$name'"
    else
        echo "Profile '$name' not found" >&2
        return 1
    fi
}

# ── Get default profile name ──
_default_profile() {
    if [ -n "$PROFILE_CMD" ]; then
        local d
        d=$("$PROFILE_CMD" default 2>/dev/null)
        if [ -n "$d" ]; then echo "$d"; return; fi
    fi
    grep '^default:' "$CONFIG" 2>/dev/null | sed 's/^default: *"*\([^"]*\)"*/\1/'
}

# ── Find project-level .ai-profile by traversing up from $PWD ──
_find_project_profile() {
    local dir="$PWD"
    while [ "$dir" != "/" ]; do
        if [ -f "$dir/.ai-profile" ]; then
            local proj_name=$(head -n1 "$dir/.ai-profile" | sed 's/^default_profile:[[:space:]]*//' | tr -d '[:space:]')
            if [ -n "$proj_name" ] && [ -n "$(_profile_env "$proj_name" 2>/dev/null)" ]; then
                export KN_PROJECT_DIR="$dir"
                export KN_PROFILE_SOURCE="project"
                echo "$proj_name"; return 0
            fi
        fi
        dir=$(dirname "$dir")
    done; return 1
}

_toml_string() {
    printf '%s' "$1" | sed 's/\\/\\\\/g; s/"/\\"/g; s/^/"/; s/$/"/'
}

# ── Interactive profile picker ──
_interactive_pick() {
    local tool="$1"
    if command -v fzf >/dev/null 2>&1; then
        if [ -n "$PROFILE_CMD" ]; then
            "$PROFILE_CMD" names 2>/dev/null | fzf --prompt="Select profile for $tool: " --height=10 | cut -f1
        else
            grep '^  [a-zA-Z]' "$CONFIG" 2>/dev/null | sed 's/^  \([a-zA-Z0-9_-]*\):.*/\1/' | fzf --prompt="Select profile for $tool: " --height=10
        fi
    else
        echo "Profiles:"
        _profile_list
        echo -n "Enter profile name (or Enter to skip): "
        read -r selected
        echo "$selected"
    fi
}

# ── Tool install hints ────────────────────────────────────────
_tool_install_hint() {
    case "$1" in
        claude) echo "npm i -g @anthropic-ai/claude-code 或 curl -fsSL https://claude.ai/install.sh | bash" ;;
        codex)  echo "npm i -g @openai/codex" ;;
        qoderclicn) echo "请参考 Qoder 官方安装文档" ;;
        *)      echo "请确认 $1 已正确安装并在 PATH 中" ;;
    esac
}

# ── Check if a CLI tool is available ──────────────────────────
_check_tool() {
    local tool="$1"
    if ! command -v "$tool" >/dev/null 2>&1; then
        echo "" >&2
        echo "╔══════════════════════════════════════════════════════════════╗" >&2
        echo "║  ERROR: $tool 未找到                                      ║" >&2
        echo "╠══════════════════════════════════════════════════════════════╣" >&2
        echo "║                                                              ║" >&2
        printf "║  安装: %-52s ║\n" "$(_tool_install_hint "$tool")" >&2
        echo "║                                                              ║" >&2
        echo "╚══════════════════════════════════════════════════════════════╝" >&2
        echo "" >&2
        return 1
    fi
    return 0
}

_kn_codex_home_dir() {
    echo "${CODEX_HOME:-$HOME/.codex}"
}

_kn_codex_canonical_auth_path() {
    local codex_home="$1"
    if command -v python3 >/dev/null 2>&1; then
        python3 - "$codex_home" <<'PY'
import os, sys

codex_home = os.path.abspath(os.path.expanduser(sys.argv[1]))
auth = os.path.join(codex_home, "auth.json")
if os.path.exists(auth):
    print(os.path.realpath(auth))
elif os.path.exists(codex_home):
    print(os.path.join(os.path.realpath(codex_home), "auth.json"))
else:
    parent = os.path.dirname(codex_home)
    if parent and os.path.exists(parent):
        print(os.path.join(os.path.realpath(parent), os.path.basename(codex_home), "auth.json"))
    else:
        print(os.path.normpath(auth))
PY
    else
        case "$codex_home" in
            /*) echo "${codex_home%/}/auth.json" ;;
            *) echo "$PWD/${codex_home%/}/auth.json" ;;
        esac
    fi
}

_kn_codex_auth_path() {
    _kn_codex_canonical_auth_path "$(_kn_codex_home_dir)"
}

_kn_codex_auth_state_root() {
    echo "${KN_CODEX_AUTH_STATE_DIR:-$HOME/.kn-codex-auth}"
}

_kn_codex_scope_id() {
    if command -v python3 >/dev/null 2>&1; then
        python3 -c 'import hashlib,sys; print(hashlib.sha256(sys.argv[1].encode()).hexdigest()[:16])' "$1"
    else
        printf '%s' "$1" | cksum | awk '{print $1}'
    fi
}

_kn_codex_auth_scope_dir() {
    local auth_path="$(_kn_codex_auth_path)"
    echo "$(_kn_codex_auth_state_root)/$(_kn_codex_scope_id "$auth_path")"
}

_kn_codex_auth_lock_dir() {
    echo "$(_kn_codex_auth_scope_dir)/codex-auth.lock"
}

_kn_codex_account_auth_path() {
    echo "$(_kn_codex_auth_scope_dir)/account.auth.json"
}

_kn_codex_ensure_auth_state_dirs() {
    local state_root="$(_kn_codex_auth_state_root)"
    local scope_dir="$(_kn_codex_auth_scope_dir)"
    mkdir -p "$scope_dir" || return 1
    chmod 700 "$state_root" "$scope_dir" 2>/dev/null || true
}

_kn_codex_profile_auth_path() {
    local profile_name="$1"
    local safe
    safe="$(printf '%s' "$profile_name" | sed 's/[^A-Za-z0-9._-]/_/g')"
    [ -n "$safe" ] || safe="profile"
    echo "${KN_HOME:-$HOME/.kn}/codex-auth/api-key/${safe}.auth.json"
}

_kn_codex_auth_kind() {
    local auth="$1"
    [ -f "$auth" ] || { echo "unknown"; return; }
    if command -v python3 >/dev/null 2>&1; then
        python3 - "$auth" <<'PY'
import json, sys
try:
    with open(sys.argv[1], "r", encoding="utf-8") as f:
        data = json.load(f)
except Exception:
    print("unknown")
    raise SystemExit
mode = str(data.get("auth_mode", "")).lower()
has_api = "OPENAI_API_KEY" in data
if has_api or mode in ("apikey", "api_key"):
    print("api_key")
elif not has_api and (mode == "chatgpt" or "tokens" in data or "ChatgptAuthTokens" in data):
    print("account")
else:
    print("unknown")
PY
    else
        grep -q '"OPENAI_API_KEY"' "$auth" && { echo "api_key"; return; }
        grep -q '"auth_mode"[[:space:]]*:[[:space:]]*"chatgpt"' "$auth" && { echo "account"; return; }
        grep -q '"tokens"' "$auth" && { echo "account"; return; }
        echo "unknown"
    fi
}

_kn_codex_copy_auth_file() {
    local src="$1"
    local dst="$2"
    case "$dst" in
        "$(_kn_codex_auth_state_root)"/*) _kn_codex_ensure_auth_state_dirs || return 1 ;;
    esac
    if ! command -v python3 >/dev/null 2>&1; then
        echo "kn: Codex auth 安全写入需要 python3。" >&2
        return 1
    fi
    python3 - "$src" "$dst" <<'PY'
import os, shutil, sys

src, dst = sys.argv[1], sys.argv[2]
os.makedirs(os.path.dirname(dst), exist_ok=True)
tmp = f"{dst}.tmp.{os.getpid()}"
shutil.copyfile(src, tmp)
os.chmod(tmp, 0o600)
with open(tmp, "rb") as f:
    os.fsync(f.fileno())
os.replace(tmp, dst)
try:
    dir_fd = os.open(os.path.dirname(dst), os.O_RDONLY)
    try:
        os.fsync(dir_fd)
    finally:
        os.close(dir_fd)
except OSError:
    pass
PY
}

_kn_codex_write_api_auth() {
    local dst="$1"
    local apikey="$2"
    case "$dst" in
        "$(_kn_codex_auth_state_root)"/*) _kn_codex_ensure_auth_state_dirs || return 1 ;;
    esac
    mkdir -p "$(dirname "$dst")" || return 1
    if ! command -v python3 >/dev/null 2>&1; then
        echo "kn: Codex API Key auth 切换需要 python3 来安全写入临时 auth.json。" >&2
        return 1
    fi
    printf '%s' "$apikey" | python3 -c '
import json, os, sys

path = sys.argv[1]
key = sys.stdin.read()
tmp = f"{path}.tmp.{os.getpid()}"
with open(tmp, "w", encoding="utf-8") as f:
    json.dump({"auth_mode": "apikey", "OPENAI_API_KEY": key}, f, separators=(",", ":"))
    f.write("\n")
    f.flush()
    os.fsync(f.fileno())
os.chmod(tmp, 0o600)
os.replace(tmp, path)
try:
    dir_fd = os.open(os.path.dirname(path), os.O_RDONLY)
    try:
        os.fsync(dir_fd)
    finally:
        os.close(dir_fd)
except OSError:
    pass
' "$dst"
}

_kn_codex_keyring_auth_configured() {
    local cfg="$(_kn_codex_home_dir)/config.toml"
    [ -f "$cfg" ] || return 1
    if command -v python3 >/dev/null 2>&1; then
        python3 - "$cfg" <<'PY'
import re, sys
try:
    text = open(sys.argv[1], encoding="utf-8").read()
except OSError:
    raise SystemExit(1)
try:
    import tomllib
    if str(tomllib.loads(text).get("cli_auth_credentials_store", "")).lower() == "keyring":
        raise SystemExit(0)
    raise SystemExit(1)
except Exception:
    pass
for line in text.splitlines():
    line = line.split("#", 1)[0].strip()
    if not line or not line.startswith("cli_auth_credentials_store"):
        continue
    match = re.match(r"cli_auth_credentials_store\s*=\s*['\"]?([^'\"]+)['\"]?\s*$", line)
    if match and match.group(1).strip().lower() == "keyring":
        raise SystemExit(0)
raise SystemExit(1)
PY
        return $?
    fi
    grep -E '^[[:space:]]*cli_auth_credentials_store[[:space:]]*=' "$cfg" 2>/dev/null \
        | sed 's/[[:space:]]*#.*$//' \
        | grep -Eq '=[[:space:]]*["'\'']?keyring["'\'']?[[:space:]]*$'
}

_kn_pid_alive() {
    local pid="$1"
    [ -n "$pid" ] && kill -0 "$pid" >/dev/null 2>&1
}

_kn_codex_recover_stale_lock() {
    local lock_dir="$1"
    local auth="$2"
    local written="$lock_dir/written.auth.json"
    local backup="$lock_dir/original.auth.json"
    local no_original="$lock_dir/no-original"
    local account="$(_kn_codex_account_auth_path)"

    if [ -f "$written" ] && [ -f "$auth" ] && cmp -s "$auth" "$written"; then
        if [ -f "$backup" ]; then
            _kn_codex_copy_auth_file "$backup" "$auth"
            echo "kn: 已从上次异常退出中恢复 Codex auth.json" >&2
        elif [ -f "$no_original" ]; then
            rm -f "$auth"
            echo "kn: 已清理上次异常退出留下的 Codex 临时 auth.json" >&2
        fi
    else
        if [ "$(_kn_codex_auth_kind "$auth")" = "account" ]; then
            _kn_codex_copy_auth_file "$auth" "$account"
        fi
        echo "kn: 检测到上次 Codex API Key 会话异常退出，但 auth.json 已变化，已保留当前内容" >&2
    fi
    rm -rf "$lock_dir"
}

_kn_codex_acquire_auth_lock() {
    local profile_name="$1"
    local auth="$2"
    local lock_dir="$(_kn_codex_auth_lock_dir)"
    local lock_parent
    lock_parent="$(dirname "$lock_dir")"
    _kn_codex_ensure_auth_state_dirs || return 1
    mkdir -p "$lock_parent" || return 1

    if mkdir "$lock_dir" 2>/dev/null; then
        chmod 700 "$lock_dir" 2>/dev/null || true
        {
            echo "pid=$$"
            echo "profile=$profile_name"
            echo "started_at=$(date +%s)"
            echo "auth=$auth"
        } > "$lock_dir/meta"
        echo "$lock_dir"
        return 0
    fi

    local old_pid
    old_pid="$(sed -n 's/^pid=//p' "$lock_dir/meta" 2>/dev/null | head -1)"
    if _kn_pid_alive "$old_pid"; then
        echo "kn: Codex API Key 配置正在运行，暂时不能同时修改 auth.json。" >&2
        echo "kn: 请先关闭当前 Codex 会话，或使用账号登录配置。" >&2
        return 1
    fi

    _kn_codex_recover_stale_lock "$lock_dir" "$auth"
    if mkdir "$lock_dir" 2>/dev/null; then
        chmod 700 "$lock_dir" 2>/dev/null || true
        {
            echo "pid=$$"
            echo "profile=$profile_name"
            echo "started_at=$(date +%s)"
            echo "auth=$auth"
        } > "$lock_dir/meta"
        echo "$lock_dir"
        return 0
    fi

    echo "kn: 无法获取 Codex auth 锁，请稍后重试。" >&2
    return 1
}

_kn_codex_recover_or_reject_existing_lock() {
    local auth="$1"
    local lock_dir="$(_kn_codex_auth_lock_dir)"
    [ -d "$lock_dir" ] || return 0

    local old_pid
    old_pid="$(sed -n 's/^pid=//p' "$lock_dir/meta" 2>/dev/null | head -1)"
    if _kn_pid_alive "$old_pid"; then
        echo "kn: Codex auth 正在由另一个 kn 会话切换，暂时不能启动账号登录配置。" >&2
        return 1
    fi
    _kn_codex_recover_stale_lock "$lock_dir" "$auth"
}

_kn_codex_restore_auth_and_release() {
    local lock_dir="$1"
    local auth="$2"
    [ -d "$lock_dir" ] || return 0

    local written="$lock_dir/written.auth.json"
    local backup="$lock_dir/original.auth.json"
    local no_original="$lock_dir/no-original"
    local account="$(_kn_codex_account_auth_path)"

    if [ -f "$written" ] && [ -f "$auth" ] && cmp -s "$auth" "$written"; then
        if [ -f "$backup" ]; then
            _kn_codex_copy_auth_file "$backup" "$auth"
        elif [ -f "$no_original" ]; then
            rm -f "$auth"
        fi
    else
        if [ "$(_kn_codex_auth_kind "$auth")" = "account" ]; then
            _kn_codex_copy_auth_file "$auth" "$account"
        fi
        echo "kn: Codex auth.json 在会话期间被外部修改，已保留当前内容。" >&2
    fi
    rm -rf "$lock_dir"
}

_kn_codex_restore_delay_seconds() {
    local delay_ms="${KN_CODEX_AUTH_RESTORE_DELAY_MS:-500}"
    case "$delay_ms" in
        ''|*[!0-9]*) delay_ms=500 ;;
    esac
    if [ "$delay_ms" -gt 5000 ]; then
        delay_ms=5000
    fi
    awk -v ms="$delay_ms" 'BEGIN { printf "%.3f", ms / 1000 }'
}

_kn_codex_run_with_after_start_restore() {
    local tool="$1"
    local lock_dir="$2"
    local auth="$3"
    shift 3

    if [ -n "${ZSH_VERSION:-}" ]; then
        setopt localoptions nobgnice nomonitor 2>/dev/null || true
    fi

    (
        local delay_seconds
        delay_seconds="$(_kn_codex_restore_delay_seconds)"
        if [ "$delay_seconds" != "0.000" ]; then
            sleep "$delay_seconds"
        fi
        _kn_codex_restore_auth_and_release "$lock_dir" "$auth"
    ) &
    local restore_pid=$!

    command "$tool" "$@"
    local rc=$?

    if kill -0 "$restore_pid" 2>/dev/null; then
        _kn_codex_restore_auth_and_release "$lock_dir" "$auth"
    fi
    wait "$restore_pid" 2>/dev/null || true
    return "$rc"
}

# ── Launch tool with profile env injected ──
_ai_launch_with_profile() {
    local tool="$1"
    local profile_name="$2"
    shift 2

    _check_tool "$tool" || return 127

    local env_output
    env_output=$(_profile_env "$profile_name")
    if [ -z "$env_output" ]; then
        echo "Profile '$profile_name' not found or has no env vars." >&2
        command "$tool" "$@"
        return 1
    fi

    echo "-> Using profile: $profile_name"

    # Auto-register current directory as a project (before the tool starts,
    # so it appears immediately in Hook/Skill project selectors).
    python3 -c "
import json, os
d = os.path.realpath(os.environ['PWD'])
# Prefer KN_HOME or ~/.kn, fall back to legacy ~/.claude-profiles
kn_home = os.environ.get('KN_HOME')
if kn_home:
    proj_dir = kn_home
elif os.path.isdir(os.path.expanduser('~/.kn')):
    proj_dir = os.path.expanduser('~/.kn')
else:
    proj_dir = os.path.expanduser('~/.claude-profiles')
f = os.path.join(proj_dir, 'projects.json')
try:
    os.makedirs(os.path.dirname(f), exist_ok=True)
    try:
        with open(f) as fh: projs = json.load(fh)
    except: projs = []
    if not isinstance(projs, list): projs = []
    if not any(p.get('path') == d for p in projs if isinstance(p, dict)):
        projs.append({'name': os.path.basename(d), 'path': d})
        with open(f, 'w') as fh: json.dump(projs, fh, indent=2, ensure_ascii=False)
except: pass
" 2>/dev/null

    # All launch paths use a subshell so env vars don't leak to parent shell.
    # trap ensures temp files are cleaned up on any exit (normal, error, SIGINT).
    case "$tool" in
        claude)
            # settings.json 的 env swap 已在 ai() 中统一处理。
            # 这里只需把 profile env 写成临时文件传给 --settings（兜底）。
            if command -v python3 >/dev/null 2>&1; then
                local tmp_settings
                tmp_settings=$(mktemp "${TMPDIR:-/tmp}/kn-claude.XXXXXX")
                echo "$env_output" | python3 -c "
import sys, json
env = {}
for line in sys.stdin:
    line = line.strip()
    if not line.startswith('export '):
        continue
    rest = line[7:]
    eq = rest.index('=')
    key = rest[:eq]
    val = rest[eq+1:]
    if len(val) >= 2 and val[0] == val[-1] and val[0] in ('\"', \"'\"):
        val = val[1:-1]
    env[key] = val
print(json.dumps({'env': env}))
" > "$tmp_settings"
                (eval "$env_output" && export KN_PROFILE="$profile_name" && export KN_CLI_TOOL="$tool" && export KN_WORKING_DIR="$PWD" && command "$tool" --settings "$tmp_settings" "$@")
                local rc=$?
                rm -f "$tmp_settings"
                return $rc
            else
                (eval "$env_output" && export KN_PROFILE="$profile_name" && export KN_CLI_TOOL="$tool" && export KN_WORKING_DIR="$PWD" && command "$tool" "$@")
                return
            fi
            ;;
        codex)
            # Codex reads auth.json during startup. kn only swaps that file for
            # the launch window, then restores the previous file immediately.
            if _kn_codex_keyring_auth_configured; then
                echo "kn: Codex 已配置 keyring 凭据存储，当前版本只接管 file auth storage。" >&2
                return 1
            fi
            local _kn_apikey _kn_base _kn_model _kn_auth_mode
            _kn_apikey="$(unset OPENAI_API_KEY OPENAI_BASE_URL OPENAI_MODEL _KN_AUTH_MODE; eval "$env_output"; printf '%s' "${OPENAI_API_KEY-}")"
            _kn_base="$(unset OPENAI_API_KEY OPENAI_BASE_URL OPENAI_MODEL _KN_AUTH_MODE; eval "$env_output"; printf '%s' "${OPENAI_BASE_URL-}")"
            _kn_model="$(unset OPENAI_API_KEY OPENAI_BASE_URL OPENAI_MODEL _KN_AUTH_MODE; eval "$env_output"; printf '%s' "${OPENAI_MODEL-}")"
            _kn_auth_mode="$(unset OPENAI_API_KEY OPENAI_BASE_URL OPENAI_MODEL _KN_AUTH_MODE; eval "$env_output"; printf '%s' "${_KN_AUTH_MODE-}" | tr '[:upper:]' '[:lower:]')"
            local _kn_auth="$(_kn_codex_auth_path)"
            local _kn_account="$(_kn_codex_account_auth_path)"
            local _kn_explicit_local_login=0
            local _kn_explicit_api_key=0
            case "$_kn_auth_mode" in
                local_login|chatgpt) _kn_explicit_local_login=1 ;;
                api_key|apikey) _kn_explicit_api_key=1 ;;
            esac
            [ -n "$_kn_model" ] && set -- -c "model=$(_toml_string "$_kn_model")" "$@"
            if [ "$_kn_explicit_api_key" = "1" ] && [ -z "$_kn_apikey" ]; then
                echo "kn: Codex API Key profile 缺少 OPENAI_API_KEY。" >&2
                return 1
            fi
            if [ "$_kn_explicit_local_login" != "1" ] && { [ -n "$_kn_apikey" ] || [ "$_kn_explicit_api_key" = "1" ]; }; then
                if [ -n "$_kn_base" ]; then
                    set -- \
                        -c "model_provider=\"custom\"" \
                        -c "model_providers.custom.name=\"Custom\"" \
                        -c "model_providers.custom.env_key=\"OPENAI_API_KEY\"" \
                        -c "model_providers.custom.base_url=$(_toml_string "$_kn_base")" \
                        -c "model_providers.custom.requires_openai_auth=true" \
                        -c "model_providers.custom.wire_api=\"responses\"" \
                        "$@"
                else
                    set -- -c "model_provider=\"openai\"" "$@"
                fi
                local _kn_lock_dir
                _kn_lock_dir="$(_kn_codex_acquire_auth_lock "$profile_name" "$_kn_auth")" || return 1
                [ -d "$(dirname "$_kn_auth")" ] || mkdir -p "$(dirname "$_kn_auth")"
                if [ -f "$_kn_auth" ]; then
                    _kn_codex_copy_auth_file "$_kn_auth" "$_kn_lock_dir/original.auth.json" || { rm -rf "$_kn_lock_dir"; return 1; }
                    if [ "$(_kn_codex_auth_kind "$_kn_auth")" = "account" ]; then
                        _kn_codex_copy_auth_file "$_kn_auth" "$_kn_account" || { rm -rf "$_kn_lock_dir"; return 1; }
                    fi
                else
                    : > "$_kn_lock_dir/no-original" || { rm -rf "$_kn_lock_dir"; return 1; }
                    chmod 600 "$_kn_lock_dir/no-original" 2>/dev/null || true
                fi
                _kn_codex_write_api_auth "$(_kn_codex_profile_auth_path "$profile_name")" "$_kn_apikey" || { _kn_codex_restore_auth_and_release "$_kn_lock_dir" "$_kn_auth"; return 1; }
                _kn_codex_write_api_auth "$_kn_auth" "$_kn_apikey" || { _kn_codex_restore_auth_and_release "$_kn_lock_dir" "$_kn_auth"; return 1; }
                _kn_codex_copy_auth_file "$_kn_auth" "$_kn_lock_dir/written.auth.json" || { _kn_codex_restore_auth_and_release "$_kn_lock_dir" "$_kn_auth"; return 1; }
                (eval "$env_output" && unset OPENAI_API_KEY OPENAI_BASE_URL OPENAI_MODEL && export KN_PROFILE="$profile_name" && export KN_CLI_TOOL="$tool" && export KN_WORKING_DIR="$PWD" && _kn_codex_run_with_after_start_restore "$tool" "$_kn_lock_dir" "$_kn_auth" "$@")
                return $?
            fi

            set -- -c "model_provider=\"openai\"" "$@"
            if [ "$(_kn_codex_auth_kind "$_kn_auth")" = "account" ]; then
                _kn_codex_copy_auth_file "$_kn_auth" "$_kn_account" || return 1
                (eval "$env_output" && unset OPENAI_API_KEY OPENAI_BASE_URL OPENAI_MODEL && export KN_PROFILE="$profile_name" && export KN_CLI_TOOL="$tool" && export KN_WORKING_DIR="$PWD" && command "$tool" "$@")
                return
            fi
            _kn_codex_recover_or_reject_existing_lock "$_kn_auth" || return 1
            if [ -f "$_kn_account" ]; then
                local _kn_lock_dir
                _kn_lock_dir="$(_kn_codex_acquire_auth_lock "$profile_name" "$_kn_auth")" || return 1
                [ -d "$(dirname "$_kn_auth")" ] || mkdir -p "$(dirname "$_kn_auth")"
                if [ -f "$_kn_auth" ]; then
                    _kn_codex_copy_auth_file "$_kn_auth" "$_kn_lock_dir/original.auth.json" || { rm -rf "$_kn_lock_dir"; return 1; }
                else
                    : > "$_kn_lock_dir/no-original" || { rm -rf "$_kn_lock_dir"; return 1; }
                    chmod 600 "$_kn_lock_dir/no-original" 2>/dev/null || true
                fi
                _kn_codex_copy_auth_file "$_kn_account" "$_kn_auth" || { _kn_codex_restore_auth_and_release "$_kn_lock_dir" "$_kn_auth"; return 1; }
                _kn_codex_copy_auth_file "$_kn_auth" "$_kn_lock_dir/written.auth.json" || { _kn_codex_restore_auth_and_release "$_kn_lock_dir" "$_kn_auth"; return 1; }
                (eval "$env_output" && unset OPENAI_API_KEY OPENAI_BASE_URL OPENAI_MODEL && export KN_PROFILE="$profile_name" && export KN_CLI_TOOL="$tool" && export KN_WORKING_DIR="$PWD" && _kn_codex_run_with_after_start_restore "$tool" "$_kn_lock_dir" "$_kn_auth" "$@")
                return $?
            fi
            (eval "$env_output" && unset OPENAI_API_KEY OPENAI_BASE_URL OPENAI_MODEL && export KN_PROFILE="$profile_name" && export KN_CLI_TOOL="$tool" && export KN_WORKING_DIR="$PWD" && command "$tool" "$@")
            return
            ;;
        *)
            (eval "$env_output" && export KN_PROFILE="$profile_name" && export KN_CLI_TOOL="$tool" && export KN_WORKING_DIR="$PWD" && command "$tool" "$@")
            return
            ;;
    esac
}

# ── AI Model tips + personalized top profiles ──
_ai_tips() {
    echo "AI Model Tips:"
    echo "  编程开发   → claude-sonnet-4-6 / deepseek-v3"
    echo "  复杂推理   → claude-opus-4-8 / deepseek-reasoner"
    echo "  快速修改   → claude-haiku-4-5"
    echo "  中文场景   → deepseek-chat / deepseek-v4-pro"
    echo ""

    # Try to compute personalized top 3 from shell history
    local histfile="${HISTFILE:-}"
    if [ -z "$histfile" ]; then
        # Fallback to common history file locations
        if [ -f "$HOME/.zsh_history" ]; then
            histfile="$HOME/.zsh_history"
        elif [ -f "$HOME/.bash_history" ]; then
            histfile="$HOME/.bash_history"
        fi
    fi

    if [ -n "$histfile" ] && [ -f "$histfile" ]; then
        # Parse shell history to find top 3 most-used profiles.
        # Supports both zsh (: <ts>:<dur>;<cmd>) and bash (plain) formats.
        # The profile name is the 3rd field in "ai <tool> <profile> [args...]".
        local top_profiles
        top_profiles=$(sed 's/^: [0-9]*:[0-9]*;//' "$histfile" 2>/dev/null | \
            awk '$1 == "ai" && ($2 == "claude" || $2 == "codex" || $2 == "qoderclicn") {
                count[$3]++
            } END {
                for (p in count) print count[p], p
            }' | sort -rn | head -3 | awk '{print "    " $2 " (" $1 " 次)"}')

        if [ -n "$top_profiles" ]; then
            echo "  你最常用:"
            echo "$top_profiles"
        else
            echo "  你最常用:   尚未使用 profile，试试 ai claude <profile> 吧"
        fi
    else
        echo "  你最常用:   无法读取历史文件，试试 ai claude <profile> 吧"
    fi
}

# ── Help text ──
_ai_help() {
    echo "AI Profile Manager"
    echo "  ai claude <profile>       Run Claude Code with profile env"
    echo "  ai codex <profile>        Run Codex CLI with profile env"
    echo "  ai qoderclicn <profile>   Run Qoder with profile env"
    echo ""
    echo "  Manage profiles:"
    echo "    ai profile list           List all profiles"
    echo "    ai profile env <name>     Show env vars for a profile"
    echo "    ai profile switch <name>  Set default profile"
    echo "    ai tips                   Show model selection recommendations"
}

# ── Direct launch (original logic) ──
_ai_direct() {
    local cmd="${1:-}"
    if [ -z "$cmd" ]; then
        echo "Usage: ai <tool> [profile] [args...]"
        echo ""
        echo "  Supported tools: claude, codex, qoderclicn"
        echo ""
        echo "Examples:"
        echo "  ai claude deepseek      # Claude Code + deepseek profile"
        echo "  ai claude               # Interactive pick → Claude Code"
        echo "  ai codex codex-default  # Codex + codex-default profile"
        echo ""
        echo "  Manage profiles:"
        echo "    ai profile list           List all profiles"
        echo "    ai profile env <name>     Show env vars"
        echo "    ai profile switch <name>  Set default profile"
        echo "    ai tips                   Model selection recommendations"
        echo ""
        echo "Available profiles:"
        _profile_list
        return
    fi

    case "$cmd" in
        claude|codex|qoderclicn)
            local tool="$1"; shift
            # Check if next arg is a known profile name
            if [ $# -gt 0 ]; then
                if _profile_exists "$1"; then
                    local profile_name="$1"; shift
                    _ai_launch_with_profile "$tool" "$profile_name" "$@"
                    return
                fi
                echo "Profile '$1' not found. Use 'ai $tool' to launch with the default profile." >&2
                return 1
            fi

            # Check for project-level .ai-profile file
            local proj_profile
            proj_profile=$(_find_project_profile)
            if [ -n "$proj_profile" ]; then
                _ai_launch_with_profile "$tool" "$proj_profile" "$@"
                return
            fi

            # No profile specified → try default profile
            local default
            default=$(_default_profile)
            if [ -n "$default" ]; then
                _ai_launch_with_profile "$tool" "$default" "$@"
                return
            fi

            # Interactive selection
            local selected
            selected=$(_interactive_pick "$tool")
            if [ -n "$selected" ]; then
                _ai_launch_with_profile "$tool" "$selected" "$@"
            else
                command "$tool" "$@"
            fi
            ;;
        profile)
            shift
            case "${1:-}" in
                list)    _profile_list ;;
                env)     shift; _profile_show "${1:-}" ;;
                switch)  shift; _profile_switch "${1:-}" ;;
                *)
                    echo "Usage: ai profile {list|env <name>|switch <name>}" >&2
                    return 1
                    ;;
            esac
            ;;
        -h|--help|help)
            _ai_help
            ;;
        tips)
            _ai_tips
            ;;
        *)
            echo "Unknown command: $cmd" >&2
            echo "Supported: claude, codex, qoderclicn, profile" >&2
            return 1
            ;;
    esac
}

# ── ai() — Agent routing wrapper ──
ai() {
    local cmd="${1:-}"
    case "$cmd" in
        claude|codex|qoderclicn)
            local tool="$1"
            local profile="${2:-}"
            if [ -n "$profile" ]; then
                if _profile_exists "$profile"; then
                    shift 2
                else
                    echo "Profile '$profile' not found. Use 'ai $tool' to launch with the default profile." >&2
                    return 1
                fi
            else
                local def
                def=$(_default_profile 2>/dev/null)
                profile="${def:-}"
            fi

            # ── Claude Code 会优先读取 settings.json 的 env 而非 shell 环境变量 ──
            # 在启动前把 profile env 临时写入 settings.json，退出后恢复。
            # 放在 ai() 层级而非子函数中，确保 Agent IPC 和直接启动两条路径都生效。
            local _kn_settings="$HOME/.claude/settings.json"
            local _kn_bak=""
            if [ "$tool" = "claude" ] && [ -n "$profile" ] && \
               [ -f "$_kn_settings" ] && command -v python3 >/dev/null 2>&1; then
                _kn_bak=$(mktemp "${TMPDIR:-/tmp}/kn-settings-bak.XXXXXX")
                cp "$_kn_settings" "$_kn_bak"
                _profile_env "$profile" | python3 -c "
import sys, json
env = {}
for line in sys.stdin:
    line = line.strip()
    if not line.startswith('export '):
        continue
    rest = line[7:]
    eq = rest.index('=')
    key = rest[:eq]
    val = rest[eq+1:]
    if len(val) >= 2 and val[0] == val[-1] and val[0] in ('\"', \"'\"):
        val = val[1:-1]
    env[key] = val
with open('$_kn_bak') as f:
    base = json.load(f)
base['env'] = env
with open('$_kn_settings', 'w') as f:
    json.dump(base, f, indent=2)
" 2>/dev/null
            fi

            if [ -n "$profile" ]; then
                _ai_launch_with_profile "$tool" "$profile" "$@"
            else
                _ai_direct "$cmd" "${@:2}"
            fi
            local _kn_ai_rc=$?

            # 恢复原 settings.json
            [ -n "$_kn_bak" ] && [ -f "$_kn_bak" ] && mv "$_kn_bak" "$_kn_settings"
            return $_kn_ai_rc
            ;;
        *)
            _ai_direct "$@"
            ;;
    esac
}
