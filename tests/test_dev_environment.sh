#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

bash -n "$ROOT_DIR/dev.sh"
bash -n "$ROOT_DIR/dev-ui.sh"

grep -Fq 'DEV_KN_HOME="${KN_DEV_HOME:-$HOME/.kn-dev}"' "$ROOT_DIR/dev.sh"
grep -Fq '<string>com.kn.agent.dev</string>' "$ROOT_DIR/dev.sh"
grep -Fq '<key>KN_HOME</key>' "$ROOT_DIR/dev.sh"
grep -Fq 'KN_HOME="$DEV_KN_HOME" KN_NO_AGENT_RESTART=true npm run tauri dev' "$ROOT_DIR/dev.sh"
grep -Fq 'DEV_KN_HOME="${KN_DEV_HOME:-$HOME/.kn-dev}"' "$ROOT_DIR/dev-ui.sh"
grep -Fq 'KN_HOME="$DEV_KN_HOME" KN_NO_AGENT_RESTART=true npm run tauri dev' "$ROOT_DIR/dev-ui.sh"

echo "dev environment isolation checks passed"
