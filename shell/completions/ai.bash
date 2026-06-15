# AI Profile Manager — Bash completion script
# Installed by install.sh and desktop app to ~/.kn/completions/ai.bash
#
# Provides completions for the ai() shell function:
#   ai <tab>             → claude / codex / qoderclicn / profile
#   ai claude <tab>      → profile names from config.yaml
#   ai profile <tab>     → list / env / switch
#   ai profile env <tab> → profile names
#   ai profile switch <tab> → profile names

_ai_profiles() {
    local config="${KN_HOME:-$HOME/.kn}/config.yaml"
    if [ ! -f "$config" ] && [ -f "$HOME/.claude-profiles/config.yaml" ]; then
        config="$HOME/.claude-profiles/config.yaml"
    fi
    [ -f "$config" ] || return 0
    sed -n 's/^  \([a-zA-Z][a-zA-Z0-9_-]*\):.*/\1/p' "$config"
}

_ai_completion() {
    local cur cword profiles
    cur="${COMP_WORDS[COMP_CWORD]}"
    cword="$COMP_CWORD"
    COMPREPLY=()

    if [ "$cword" -eq 1 ]; then
        COMPREPLY=($(compgen -W "claude codex qoderclicn profile" -- "$cur"))
        return
    fi

    if [ "$cword" -eq 2 ]; then
        case "${COMP_WORDS[1]}" in
            claude|codex|qoderclicn)
                profiles=$(_ai_profiles)
                COMPREPLY=($(compgen -W "$profiles" -- "$cur"))
                ;;
            profile)
                COMPREPLY=($(compgen -W "list env switch" -- "$cur"))
                ;;
        esac
        return
    fi

    if [ "$cword" -eq 3 ]; then
        if [ "${COMP_WORDS[1]}" = "profile" ] && { [ "${COMP_WORDS[2]}" = "env" ] || [ "${COMP_WORDS[2]}" = "switch" ]; }; then
            profiles=$(_ai_profiles)
            COMPREPLY=($(compgen -W "$profiles" -- "$cur"))
        fi
    fi
}

complete -F _ai_completion ai
