#!/bin/bash
# Smoke tests for shell/ai-profile.sh
set -euo pipefail
RED='\033[31m'; GREEN='\033[32m'; RESET='\033[0m'
PASS=0; FAIL=0
pass() { echo -e "${GREEN}PASS${RESET} $1"; PASS=$((PASS+1)); }
fail() { echo -e "${RED}FAIL${RESET} $1"; FAIL=$((FAIL+1)); }

SCRIPT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
SHELL_RC="$SCRIPT_DIR/shell/ai-profile.sh"

echo "=== Shell Wrapper Smoke Tests ==="
echo ""

# -- Syntax --
echo "--- Syntax ---"
if bash -n "$SHELL_RC" 2>/dev/null; then
    pass "bash -n ai-profile.sh"
else
    fail "bash -n ai-profile.sh"
fi

# -- Source --
# shellcheck disable=SC1090
source "$SHELL_RC" 2>/dev/null || true

if declare -f ai >/dev/null 2>&1; then pass "ai() defined"; else fail "ai() not defined"; fi
for fn in _profile_env _profile_list _profile_show _default_profile _interactive_pick _ai_launch_with_profile _ai_help; do
    if declare -f "$fn" >/dev/null 2>&1; then pass "  $fn()"; else fail "  $fn()"; fi
done

# -- Env extraction with temp config --
echo ""
echo "--- Env extraction ---"
TMP_DIR=$(mktemp -d)
trap 'rm -rf "$TMP_DIR"' EXIT
CFG="$TMP_DIR/config.yaml"

cat > "$CFG" << 'YEOF'
default: test
profiles:
  test:
    desc: "Test profile"
    env:
      API_KEY: sk-test-123456
      API_URL: "https://api.example.com/v1"
      EMPTY_VAR: ""
YEOF

# Run sed/awk extraction inline (same logic as _profile_env)
output=$(sed -n "/^  test:/,/^  [a-z]/p" "$CFG" | \
    awk -F': ' '/^      [A-Za-z_][A-Za-z0-9_]*:/ {
        k=$1; sub(/^      /,"",k)
        v=substr($0,length(k)+9)
        if (v ~ /^".*"$/) v=substr(v,2,length(v)-2)
        else if (v ~ /^'"'"'.*'"'"'$/) v=substr(v,2,length(v)-2)
        print "export " k "='"'"'" v "'"'"'"
    }')

echo "$output" | grep -q "export API_KEY='sk-test-123456'" \
    && pass "API_KEY extracted" || fail "API_KEY extraction"
echo "$output" | grep -q "export API_URL='https://api.example.com/v1'" \
    && pass "API_URL extracted" || fail "API_URL extraction"
echo "$output" | grep -q "export EMPTY_VAR=''" \
    && pass "EMPTY_VAR extracted as empty" || fail "EMPTY_VAR extraction"

echo ""
echo -e "=== ${GREEN}$PASS passed${RESET}, ${RED}$FAIL failed${RESET} ==="
[ "$FAIL" -eq 0 ] || exit 1
