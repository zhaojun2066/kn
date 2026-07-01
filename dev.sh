#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
AGENT_SRC="$ROOT_DIR/target/debug/kn-agent"
AGENT_DST="${KN_AGENT_DST:-$HOME/.kn/agent/kn-agent}"

echo "[kn dev] Building dev kn-agent..."
cargo build -p kn-agent --manifest-path "$ROOT_DIR/Cargo.toml"

echo "[kn dev] Installing dev kn-agent to: $AGENT_DST"
mkdir -p "$(dirname "$AGENT_DST")"
cp "$AGENT_SRC" "$AGENT_DST"
chmod +x "$AGENT_DST"

echo "[kn dev] Starting desktop Tauri dev..."
cd "$ROOT_DIR/desktop"
npm run tauri dev
