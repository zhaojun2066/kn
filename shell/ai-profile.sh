# AI Profile Manager — Shell Wrapper (canonical source)
# Installed by install.sh and desktop app to ~/.claude-profiles/shell-rc
# Usage: ai <tool> [profile] [args...]
#        ai profile <command>

CONFIG="$HOME/.claude-profiles/config.yaml"

# ── Resolve profile CLI (preferred but optional) ──
if [ -x "$HOME/.claude-profiles/bin/profile" ]; then
    PROFILE_CMD="$HOME/.claude-profiles/bin/profile"
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

# ── Launch tool with profile env injected ──
_ai_launch_with_profile() {
    local tool="$1"
    local profile_name="$2"
    shift 2

    local env_output
    env_output=$(_profile_env "$profile_name")
    if [ -z "$env_output" ]; then
        echo "Profile '$profile_name' not found or has no env vars." >&2
        command "$tool" "$@"
        return 1
    fi

    echo "-> Using profile: $profile_name"

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
                (eval "$env_output" && export KN_PROFILE="$profile_name" && export KN_CLI_TOOL="$tool" && command "$tool" --settings "$tmp_settings" "$@")
                local rc=$?
                rm -f "$tmp_settings"
                return $rc
            else
                (eval "$env_output" && export KN_PROFILE="$profile_name" && export KN_CLI_TOOL="$tool" && command "$tool" "$@")
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
            [ -n "$_kn_model" ] && set -- -c "model=$_kn_model" "$@"
            [ -n "$_kn_base" ] && set -- -c "model_providers.custom.base_url=$_kn_base" "$@"
            if [ -n "$_kn_apikey" ]; then
                [ -d "$HOME/.codex" ] || mkdir -p "$HOME/.codex"
                [ -f "$_kn_auth" ] && cp "$_kn_auth" "$_kn_auth.kn-bak"
                printf '{"auth_mode":"apikey","OPENAI_API_KEY":"%s"}\n' "$_kn_apikey" > "$_kn_auth"
                (eval "$env_output" && export KN_PROFILE="$profile_name" && export KN_CLI_TOOL="$tool" && command "$tool" "$@")
                local _kn_rc=$?
                [ -f "$_kn_auth.kn-bak" ] && mv "$_kn_auth.kn-bak" "$_kn_auth"
                return $_kn_rc
            fi
            (eval "$env_output" && export KN_PROFILE="$profile_name" && export KN_CLI_TOOL="$tool" && command "$tool" "$@")
            return
            ;;
        *)
            (eval "$env_output" && export KN_PROFILE="$profile_name" && export KN_CLI_TOOL="$tool" && command "$tool" "$@")
            return
            ;;
    esac
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
        *)
            echo "Unknown command: $cmd" >&2
            echo "Supported: claude, codex, qoderclicn, profile" >&2
            return 1
            ;;
    esac
}
