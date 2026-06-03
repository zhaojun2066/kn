#!/usr/bin/env bash
# Install AI Profile Manager
# Everything lives under ~/.claude-profiles/
#
#   ~/.claude-profiles/
#   ├── bin/profile        ← CLI
#   ├── lib/config.py      ← shared module
#   ├── shell-rc           ← shell wrapper
#   ├── config.yaml        ← user data
#   └── .config.lock       ← file lock

set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
INSTALL_DIR="$HOME/.claude-profiles"
MARKER_START="# >>> AI Profile Manager >>>"
MARKER_END="# <<< AI Profile Manager <<<"

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

# Install config template (only if not exists)
if [ ! -f "$INSTALL_DIR/config.yaml" ]; then
    cp "$SCRIPT_DIR/templates/config.yaml" "$INSTALL_DIR/config.yaml"
    echo "  ✓ config.yaml (new)"
else
    echo "  - config.yaml (already exists, skipped)"
fi

# ── Auto-activate in ~/.zshrc ──────────────────────────────────

ZSHRC="$HOME/.zshrc"
NEED_PATH=false
NEED_SOURCE=false

# Check what's missing
if ! echo "$PATH" | tr ':' '\n' | grep -qx "$INSTALL_DIR/bin"; then
    NEED_PATH=true
fi

if ! grep -Fq "$INSTALL_DIR/shell-rc" "$ZSHRC" 2>/dev/null; then
    NEED_SOURCE=true
fi

if $NEED_PATH || $NEED_SOURCE; then
    echo ""

    # Remove old marker block if exists
    if grep -Fq "$MARKER_START" "$ZSHRC" 2>/dev/null; then
        if [[ "$OSTYPE" == "darwin"* ]]; then
            sed -i '' "/$MARKER_START/,/$MARKER_END/d" "$ZSHRC"
        else
            sed -i "/$MARKER_START/,/$MARKER_END/d" "$ZSHRC"
        fi
    fi

    # Always write both — idempotent, no harm
    {
        echo ""
        echo "$MARKER_START"
        echo "export PATH=\"$INSTALL_DIR/bin:\$PATH\""
        echo "source \"$INSTALL_DIR/shell-rc\""
        echo "$MARKER_END"
    } >> "$ZSHRC"

    echo "  ✓ Added $INSTALL_DIR/bin to PATH"
    echo "  ✓ Added shell-rc auto-source to ~/.zshrc"
else
    echo ""
    echo "  - ~/.zshrc already configured, skipping"
fi

echo ""
echo "==> Done!"
echo ""
echo "    Run this to activate now (or restart your terminal):"
echo "      source ~/.zshrc"
echo ""
echo "    Then try:"
echo "      profile list            # See all profiles"
echo "      ai claude deepseek      # Launch Claude Code with deepseek profile"
