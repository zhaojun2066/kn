# kn — Shell Wrapper (canonical source)
# Installed by install.sh and desktop app to ~/.kn/shell-rc
# Usage: ai <tool> [profile] [args...]
#        ai profile <command>

# Config directory: prefer ~/.kn (new), fall back to ~/.claude-profiles (legacy)
if [ -d "$HOME/.kn" ]; then
    KN_DIR="$HOME/.kn"
elif [ -d "$HOME/.claude-profiles" ]; then
    KN_DIR="$HOME/.claude-profiles"
else
    KN_DIR="$HOME/.kn"
fi
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
        sed -i '' "s/^default:.*/default: \"$name\"/" "$CONFIG" 2>/dev/null || \
        sed -i "s/^default:.*/default: \"$name\"/" "$CONFIG"
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
            # Claude Code v2.0.1+ bug: settings.json env overrides shell env vars.
            # Workaround: generate temp settings file from profile, pass via --settings.
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
            # Codex ignores OPENAI_API_KEY env var; reads only ~/.codex/auth.json.
            # Write the profile's key to auth.json, pass base_url/model via -c.
            local _kn_apikey=$(echo "$env_output" | sed -n "s/^export OPENAI_API_KEY='\\(.*\\)'/\\1/p")
            local _kn_base=$(echo "$env_output" | sed -n "s/^export OPENAI_BASE_URL='\\(.*\\)'/\\1/p")
            local _kn_model=$(echo "$env_output" | sed -n "s/^export OPENAI_MODEL='\\(.*\\)'/\\1/p")
            local _kn_auth="$HOME/.codex/auth.json"
            [ -n "$_kn_model" ] && set -- -c "model=$(_toml_string "$_kn_model")" "$@"
            [ -n "$_kn_base" ] && set -- -c "model_providers.custom.base_url=$(_toml_string "$_kn_base")" "$@"
            if [ -n "$_kn_apikey" ]; then
                [ -d "$HOME/.codex" ] || mkdir -p "$HOME/.codex"
                [ -f "$_kn_auth" ] && cp "$_kn_auth" "$_kn_auth.kn-bak"
                printf '{"auth_mode":"apikey","OPENAI_API_KEY":"%s"}\n' "$_kn_apikey" > "$_kn_auth"
                (eval "$env_output" && export KN_PROFILE="$profile_name" && export KN_CLI_TOOL="$tool" && export KN_WORKING_DIR="$PWD" && command "$tool" "$@")
                local _kn_rc=$?
                [ -f "$_kn_auth.kn-bak" ] && mv "$_kn_auth.kn-bak" "$_kn_auth"
                return $_kn_rc
            fi
            (eval "$env_output" && export KN_PROFILE="$profile_name" && export KN_CLI_TOOL="$tool" && export KN_WORKING_DIR="$PWD" && command "$tool" "$@")
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

# ── Main ai() function ──
ai() {
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
                local env_check
                env_check=$(_profile_env "$1" 2>/dev/null)
                if [ -n "$env_check" ]; then
                    local profile_name="$1"; shift
                    _ai_launch_with_profile "$tool" "$profile_name" "$@"
                    return
                fi
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
