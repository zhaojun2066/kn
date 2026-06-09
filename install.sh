#!/usr/bin/env bash
# Install AI Profile Manager
# Everything lives under ~/.claude-profiles/
#
#   ~/.claude-profiles/
#   ├── bin/profile        ← CLI
#   ├── lib/config.py      ← shared module
#   ├── completions/       ← shell completions
#   │   ├── _ai             ← Zsh completion
#   │   └── ai.bash         ← Bash completion
#   ├── shell-rc           ← shell wrapper
#   ├── config.yaml        ← user data
#   └── .config.lock       ← file lock

set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
INSTALL_DIR="$HOME/.claude-profiles"
MARKER_START="# >>> AI Profile Manager >>>"
MARKER_END="# <<< AI Profile Manager <<<"
COMPLETIONS_MARKER_START="# >>> AI Profile Completions >>>"
COMPLETIONS_MARKER_END="# <<< AI Profile Completions <<<"

echo "==> Installing AI Profile Manager → $INSTALL_DIR"

# Clean up old installation from ~/.local/bin (legacy)
if [ -f "$HOME/.local/bin/profile" ]; then
    rm -f "$HOME/.local/bin/profile"
    echo "  - Removed old ~/.local/bin/profile"
fi

# Create directory structure
mkdir -p "$INSTALL_DIR/bin"
mkdir -p "$INSTALL_DIR/lib"

# Install profile CLI
cp "$SCRIPT_DIR/bin/profile" "$INSTALL_DIR/bin/profile"
chmod +x "$INSTALL_DIR/bin/profile"
echo "  ✓ bin/profile"

# Install shared config module
cp "$SCRIPT_DIR/lib/config.py" "$INSTALL_DIR/lib/config.py"
echo "  ✓ lib/config.py"

# Install shell wrapper
cp "$SCRIPT_DIR/shell/ai-profile.sh" "$INSTALL_DIR/shell-rc"
echo "  ✓ shell-rc"

# Install shell completions
mkdir -p "$INSTALL_DIR/completions"
cp "$SCRIPT_DIR/shell/completions/_ai" "$INSTALL_DIR/completions/_ai"
cp "$SCRIPT_DIR/shell/completions/ai.bash" "$INSTALL_DIR/completions/ai.bash"
echo "  ✓ completions (_ai + ai.bash)"

# Install config template (only if not exists)
if [ ! -f "$INSTALL_DIR/config.yaml" ]; then
    cp "$SCRIPT_DIR/templates/config.yaml" "$INSTALL_DIR/config.yaml"
    echo "  ✓ config.yaml (new)"
else
    echo "  - config.yaml (already exists, skipped)"
fi

# ── Auto-activate in shell RC files ───────────────────────────
#
# Always configure both ~/.zshrc and ~/.bashrc.
# Writes are idempotent — we check before adding.
# The user's active shell determines which message we show at the end.

detect_shell() {
    case "$SHELL" in
        */zsh)  echo "zsh" ;;
        */bash) echo "bash" ;;
        *)      echo "other" ;;
    esac
}

ACTIVE_SHELL="$(detect_shell)"
CONFIGURED_FILES=""

configure_rc() {
    local rc_path="$1"
    local rc_name="$2"

    local need_path=false
    local need_source=false

    if ! echo "$PATH" | tr ':' '\n' | grep -qx "$INSTALL_DIR/bin"; then
        need_path=true
    fi

    if ! grep -Fq "$INSTALL_DIR/shell-rc" "$rc_path" 2>/dev/null; then
        need_source=true
    fi

    if ! $need_path && ! $need_source; then
        echo "  - ~/$rc_name already configured, skipping"
        return
    fi

    # Remove old marker block if exists
    if grep -Fq "$MARKER_START" "$rc_path" 2>/dev/null; then
        if [[ "$OSTYPE" == "darwin"* ]]; then
            sed -i '' "/$MARKER_START/,/$MARKER_END/d" "$rc_path"
        else
            sed -i "/$MARKER_START/,/$MARKER_END/d" "$rc_path"
        fi
    fi

    # Always write both — idempotent, no harm
    {
        echo ""
        echo "$MARKER_START"
        echo "export PATH=\"$INSTALL_DIR/bin:\$PATH\""
        echo "source \"$INSTALL_DIR/shell-rc\""
        echo "$MARKER_END"
    } >> "$rc_path"

    echo "  ✓ Added to ~/$rc_name"
    CONFIGURED_FILES="$CONFIGURED_FILES ~/$rc_name"
}

# Configure both zsh and bash (idempotent, harmless either way)
configure_rc "$HOME/.zshrc" ".zshrc"
configure_rc "$HOME/.bashrc" ".bashrc"

# ── Configure shell completions ────────────────────────────────

configure_completions() {
    local rc_path="$1"
    local rc_name="$2"
    local completions_dir="$INSTALL_DIR/completions"

    # Remove old marker block if exists
    if grep -Fq "$COMPLETIONS_MARKER_START" "$rc_path" 2>/dev/null; then
        if [[ "$OSTYPE" == "darwin"* ]]; then
            sed -i '' "/$COMPLETIONS_MARKER_START/,/$COMPLETIONS_MARKER_END/d" "$rc_path"
        else
            sed -i "/$COMPLETIONS_MARKER_START/,/$COMPLETIONS_MARKER_END/d" "$rc_path"
        fi
    fi

    if [ "$rc_name" = ".zshrc" ]; then
        # Zsh: add completions dir to fpath and autoload compinit
        {
            echo ""
            echo "$COMPLETIONS_MARKER_START"
            echo "fpath=(\"$completions_dir\" \$fpath)"
            echo "autoload -Uz compinit && compinit"
            echo "$COMPLETIONS_MARKER_END"
        } >> "$rc_path"
        echo "  ✓ Zsh completions added to ~/$rc_name"
    elif [ "$rc_name" = ".bashrc" ]; then
        # Bash: source the completion script
        {
            echo ""
            echo "$COMPLETIONS_MARKER_START"
            echo "source \"$completions_dir/ai.bash\""
            echo "$COMPLETIONS_MARKER_END"
        } >> "$rc_path"
        echo "  ✓ Bash completions added to ~/$rc_name"
    fi
}

configure_completions "$HOME/.zshrc" ".zshrc"
configure_completions "$HOME/.bashrc" ".bashrc"

if [ -z "$CONFIGURED_FILES" ]; then
    echo "  - Shell already configured"
fi

echo ""
echo "==> Done!"
echo ""

case "$ACTIVE_SHELL" in
    zsh)
        echo "    Run this to activate now (or restart your terminal):"
        echo "      source ~/.zshrc"
        ;;
    bash)
        echo "    Run this to activate now (or restart your terminal):"
        echo "      source ~/.bashrc"
        ;;
    *)
        echo "    Run this to activate now (or restart your terminal):"
        echo "      source ~/.zshrc   # if using zsh"
        echo "      source ~/.bashrc  # if using bash"
        ;;
esac

echo ""
echo "    Then try:"
echo "      profile list            # See all profiles"
echo "      ai claude deepseek      # Launch Claude Code with deepseek profile"
