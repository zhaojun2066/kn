#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DEV_KN_HOME="${KN_DEV_HOME:-$HOME/.kn-dev}"
AGENT_DST="${KN_AGENT_DST:-$DEV_KN_HOME/agent/kn-agent}"
AGENT_SRC="$ROOT_DIR/target/debug/kn-agent"
AGENT_DIR="$(dirname "$AGENT_DST")"
IPC_SOCK="$AGENT_DIR/ipc.sock"

agent_ipc_ok() {
  python3 -c 'import socket,sys
p=sys.argv[1]
s=socket.socket(socket.AF_UNIX)
s.settimeout(1.5)
s.connect(p)
s.sendall(b"{\"id\":\"dev-ui\",\"method\":\"status\",\"params\":{}}\n")
data=s.recv(1048576)
sys.exit(0 if b"\"result\"" in data else 1)
' "$IPC_SOCK" >/dev/null 2>&1
}

echo "[kn dev-ui] Starting desktop Tauri dev (agent untouched)..."
echo "[kn dev-ui] Tip: use dev.sh if you need to rebuild + restart the agent."

if ! agent_ipc_ok; then
  echo "[kn dev-ui] Agent IPC is not responding at: $IPC_SOCK" >&2
  if [ -e "$AGENT_DST" ]; then
    echo "[kn dev-ui] Installed agent: $AGENT_DST" >&2
  else
    echo "[kn dev-ui] Installed agent is missing: $AGENT_DST" >&2
  fi
  if [ -e "$AGENT_SRC" ]; then
    echo "[kn dev-ui] Debug agent exists: $AGENT_SRC" >&2
  fi
  echo "[kn dev-ui] dev-ui.sh does not rebuild or restart the agent." >&2
  echo "[kn dev-ui] Run ./dev.sh once to install/restart the current agent, then use ./dev-ui.sh for UI-only work." >&2
  exit 1
fi

cd "$ROOT_DIR/desktop"
KN_HOME="$DEV_KN_HOME" KN_NO_AGENT_RESTART=true npm run tauri dev
