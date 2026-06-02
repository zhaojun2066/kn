# AI Profile Manager — Shell Wrapper
# Source this from ~/.zshrc:  source ~/.claude-profiles/shell-rc
#
# Usage:
#   ai <tool> <profile>     Launch tool with profile's env injected
#   ai <tool>               Interactive profile picker → launch tool
#   ai                      Show help
#
# The original claude / codex commands are NOT affected.
# Use "ai claude" / "ai codex" when you want profile injection.

# Ensure profile CLI is on PATH
if [ -x "$HOME/.claude-profiles/bin/profile" ]; then
    PROFILE_CMD="$HOME/.claude-profiles/bin/profile"
elif [ -x "$HOME/.local/bin/profile" ]; then
    PROFILE_CMD="$HOME/.local/bin/profile"
else
    PROFILE_CMD="profile"
fi

# ── ai ─────────────────────────────────────────────────────────

ai() {
    if [ $# -eq 0 ]; then
        echo "Usage: ai <tool> [profile] [args...]"
        echo ""
        echo "  Supported tools: claude, codex"
        echo ""
        echo "Examples:"
        echo "  ai claude deepseek      # Claude Code + deepseek profile"
        echo "  ai claude               # Interactive pick → Claude Code"
        echo "  ai codex codex-default  # Codex + codex-default profile"
        echo ""
        echo "Available profiles:"
        "$PROFILE_CMD" list 2>/dev/null
        return
    fi

    local tool="$1"
    shift

    # Validate tool
    case "$tool" in
        claude|codex) ;;
        *)
            echo "Unknown tool: $tool (supported: claude, codex)" >&2
            return 1
            ;;
    esac

    # Check if next arg is a known profile name
    if [ $# -gt 0 ]; then
        if "$PROFILE_CMD" names 2>/dev/null | cut -f1 | grep -qx "$1"; then
            local profile_name="$1"
            shift
            _ai_launch_with_profile "$tool" "$profile_name" "$@"
            return
        fi
    fi

    # No profile specified → use default or interactive pick
    local default_profile
    default_profile=$("$PROFILE_CMD" default 2>/dev/null)

    if [ -n "$default_profile" ]; then
        _ai_launch_with_profile "$tool" "$default_profile" "$@"
        return
    fi

    # Interactive selection
    local selected
    if command -v fzf >/dev/null 2>&1; then
        selected=$("$PROFILE_CMD" names 2>/dev/null | fzf --prompt="Select profile for $tool: " --height=10 | cut -f1)
    else
        echo "Profiles:"
        "$PROFILE_CMD" list 2>/dev/null
        echo -n "Enter profile name (or Enter to skip): "
        read -r selected
    fi

    if [ -n "$selected" ]; then
        _ai_launch_with_profile "$tool" "$selected" "$@"
    else
        command "$tool" "$@"
    fi
}

# ── Internal: inject env and launch ───────────────────────────

_ai_launch_with_profile() {
    local tool="$1"
    local profile_name="$2"
    shift 2

    local env_output
    env_output=$("$PROFILE_CMD" env "$profile_name" 2>/dev/null)
    if [ -z "$env_output" ]; then
        echo "Profile '$profile_name' not found or has no env vars." >&2
        command "$tool" "$@"
        return 1
    fi

    echo "→ Using profile: $profile_name"

    # eval the export statements, then launch in a subshell
    # env vars only affect THIS subshell, parent shell is clean
    (
        eval "$env_output"
        command "$tool" "$@"
    )
}
