#!/usr/bin/env bash
# =============================================================================
# dev.sh — 完整开发启动：重新编译 agent + 重启 agent + 启动 Tauri dev
#
#   适用场景：改了 agent 代码（agent/src/）或 Rust 核心逻辑，需要全量重编。
#   如果只改前端/桌面 Rust，不需要重启 agent，请用 dev-ui.sh（秒级启动）。
# =============================================================================
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
AGENT_SRC="$ROOT_DIR/target/debug/kn-agent"
# Development is a complete separate runtime: profiles, device credentials,
# IPC and logs all live under ~/.kn-dev by default, never under production ~/.kn.
DEV_KN_HOME="${KN_DEV_HOME:-$HOME/.kn-dev}"
AGENT_DST="${KN_AGENT_DST:-$DEV_KN_HOME/agent/kn-agent}"
AGENT_DIR="$(dirname "$AGENT_DST")"
IPC_SOCK="$AGENT_DIR/ipc.sock"
LOG_DIR="$AGENT_DIR/logs"
PLIST_DIR="$HOME/Library/LaunchAgents"
PLIST_PATH="$PLIST_DIR/com.kn.agent.dev.plist"
UID_NUM="$(id -u)"
LAUNCHD_DOMAIN="gui/$UID_NUM"
LAUNCHD_SERVICE="$LAUNCHD_DOMAIN/com.kn.agent.dev"

agent_ipc_ok() {
  python3 -c 'import socket,sys
p=sys.argv[1]
s=socket.socket(socket.AF_UNIX)
s.settimeout(1.5)
s.connect(p)
s.sendall(b"{\"id\":\"dev-sh\",\"method\":\"status\",\"params\":{}}\n")
data=s.recv(1048576)
sys.exit(0 if b"\"result\"" in data else 1)
' "$IPC_SOCK" >/dev/null 2>&1
}

wait_for_agent_ipc() {
  local deadline=$((SECONDS + 15))
  while [ "$SECONDS" -lt "$deadline" ]; do
    if agent_ipc_ok; then
      return 0
    fi
    sleep 0.3
  done
  return 1
}

echo "[kn dev] Building dev kn-agent..."
cargo build -p kn-agent --manifest-path "$ROOT_DIR/Cargo.toml"

echo "[kn dev] Installing dev kn-agent to: $AGENT_DST"
mkdir -p "$AGENT_DIR"
cp "$AGENT_SRC" "$AGENT_DST"
chmod +x "$AGENT_DST"

echo "[kn dev] Writing dev launchd plist: $PLIST_PATH"
mkdir -p "$PLIST_DIR" "$LOG_DIR"
cat > "$PLIST_PATH" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>com.kn.agent.dev</string>
    <key>ProgramArguments</key>
    <array>
        <string>$AGENT_DST</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <true/>
    <key>ThrottleInterval</key>
    <integer>5</integer>
    <key>StandardOutPath</key>
    <string>$LOG_DIR/stdout.log</string>
    <key>StandardErrorPath</key>
    <string>$LOG_DIR/stderr.log</string>
    <key>EnvironmentVariables</key>
    <dict>
        <key>PATH</key>
        <string>/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin</string>
        <key>RUST_LOG</key>
        <string>info</string>
        <key>KN_CLOUD_URL</key>
        <string>ws://localhost:8081/v1/ws</string>
        <key>KN_CLOUD_HTTP_URL</key>
        <string>http://localhost:8080</string>
        <key>KN_HOME</key>
        <string>$DEV_KN_HOME</string>
        <key>KN_RUNTIME_ENV</key>
        <string>development</string>
    </dict>
</dict>
</plist>
EOF

echo "[kn dev] Restarting launchd agent..."
launchctl bootout "$LAUNCHD_SERVICE" >/dev/null 2>&1 || true
sleep 0.5
if ! launchctl bootstrap "$LAUNCHD_DOMAIN" "$PLIST_PATH" >/tmp/kn-agent-bootstrap.out 2>/tmp/kn-agent-bootstrap.err; then
  if ! grep -q "already bootstrapped" /tmp/kn-agent-bootstrap.err; then
    echo "[kn dev] launchctl bootstrap failed:" >&2
    cat /tmp/kn-agent-bootstrap.err >&2
    exit 1
  fi
fi

echo "[kn dev] Waiting for agent IPC: $IPC_SOCK"
if ! wait_for_agent_ipc; then
  echo "[kn dev] Agent IPC did not become ready." >&2
  echo "[kn dev] Check logs: $LOG_DIR/stdout.log and $LOG_DIR/stderr.log" >&2
  exit 1
fi

echo "[kn dev] Agent IPC is ready."
if [ "${KN_DEV_SKIP_DESKTOP:-}" = "true" ]; then
  echo "[kn dev] KN_DEV_SKIP_DESKTOP=true, skipping desktop launch."
  exit 0
fi

echo "[kn dev] Starting desktop Tauri dev..."
cd "$ROOT_DIR/desktop"
KN_HOME="$DEV_KN_HOME" KN_NO_AGENT_RESTART=true npm run tauri dev
