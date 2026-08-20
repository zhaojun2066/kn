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
for fn in _profile_env _profile_list _profile_show _default_profile _find_project_profile _interactive_pick _ai_launch_with_profile _ai_help; do
    if declare -f "$fn" >/dev/null 2>&1; then pass "  $fn()"; else fail "  $fn()"; fi
done
if declare -f _toml_string >/dev/null 2>&1; then pass "  _toml_string()"; else fail "  _toml_string()"; fi

toml_model=$(_toml_string 'mimo-pro-codex')
[ "$toml_model" = '"mimo-pro-codex"' ] && pass "_toml_string quotes model" \
    || fail "_toml_string quotes model: got '$toml_model'"

toml_url=$(_toml_string 'https://api.example.com/v1')
[ "$toml_url" = '"https://api.example.com/v1"' ] && pass "_toml_string quotes URL" \
    || fail "_toml_string quotes URL: got '$toml_url'"

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
echo "--- Completion config path ---"

COMPLETION_HOME="$TMP_DIR/completion-home"
mkdir -p "$COMPLETION_HOME/.kn"
cat > "$COMPLETION_HOME/.kn/config.yaml" << 'YEOF'
default: alpha
profiles:
  alpha:
    desc: "Alpha"
    env:
      API_KEY: sk-alpha
  beta:
    desc: "Beta"
    env:
      API_KEY: sk-beta
YEOF

completion_output=$(HOME="$COMPLETION_HOME" bash -c '
    source "$1"
    _ai_profiles
' _ "$SCRIPT_DIR/shell/completions/ai.bash")
echo "$completion_output" | grep -q "alpha" \
    && pass "bash completion reads ~/.kn/config.yaml" \
    || fail "bash completion did not read ~/.kn/config.yaml"

echo ""
echo "--- Project auto-switch (.ai-profile) ---"

# Setup: project directory with .ai-profile
PROJ_DIR="$TMP_DIR/myproject"
mkdir -p "$PROJ_DIR/subdir"
CFG2="$TMP_DIR/.claude-profiles/config.yaml"
mkdir -p "$(dirname "$CFG2")"

cat > "$CFG2" << 'YEOF'
default: generic
profiles:
  generic:
    desc: "Generic fallback"
    env:
      API_KEY: sk-generic-key
      MODEL: generic-model
  myproj:
    desc: "Project-specific profile"
    env:
      API_KEY: sk-project-key
      MODEL: project-model
  other:
    desc: "Another profile"
    env:
      API_KEY: sk-other-key
      MODEL: other-model
YEOF

echo "myproj" > "$PROJ_DIR/.ai-profile"

# Override CONFIG and disable PROFILE_CMD to use pure-shell fallback
CONFIG="$CFG2"
PROFILE_CMD=""

# Disable set -e for project tests: bash's set -e exits on $(func_returning_1)
# even in assignment context (unlike zsh). _find_project_profile returns 1
# when no .ai-profile is found, which is expected behavior.
set +e

# Test 1: _find_project_profile finds .ai-profile in current dir
cd "$PROJ_DIR"
# Call directly (not in subshell) so env vars persist
_find_project_profile > /dev/null
ret=$?
[ "$ret" = "0" ] && pass "_find_project_profile finds .ai-profile in PWD" \
    || fail "_find_project_profile: expected return 0, got '$ret'"

# Test 1b: KN_PROJECT_DIR and KN_PROFILE_SOURCE are set
[ "${KN_PROJECT_DIR:-}" = "$PROJ_DIR" ] && pass "KN_PROJECT_DIR set correctly" \
    || fail "KN_PROJECT_DIR: expected '$PROJ_DIR', got '${KN_PROJECT_DIR:-}'"
[ "${KN_PROFILE_SOURCE:-}" = "project" ] && pass "KN_PROFILE_SOURCE=project" \
    || fail "KN_PROFILE_SOURCE: expected 'project', got '${KN_PROFILE_SOURCE:-}'"

# Test 2: _find_project_profile traverses up from subdirectory
cd "$PROJ_DIR/subdir"
result=$(_find_project_profile)
[ "$result" = "myproj" ] && pass "_find_project_profile traverses up from subdir" \
    || fail "_find_project_profile traverse: expected 'myproj', got '$result'"

# Test 3: _find_project_profile returns nothing when no .ai-profile exists
cd "$TMP_DIR"
unset KN_PROJECT_DIR KN_PROFILE_SOURCE
result=$(_find_project_profile)
[ -z "$result" ] && pass "_find_project_profile returns empty when no .ai-file" \
    || fail "_find_project_profile: expected empty, got '$result'"

# Test 4: _find_project_profile ignores .ai-profile with nonexistent profile
echo "nonexistent" > "$TMP_DIR/.ai-profile"
cd "$TMP_DIR"
result=$(_find_project_profile)
[ -z "$result" ] && pass "_find_project_profile ignores nonexistent profile name" \
    || fail "_find_project_profile: expected empty for bad profile, got '$result'"
rm -f "$TMP_DIR/.ai-profile"

# Test 5: .ai-profile with whitespace/newlines handled correctly
printf "  myproj  \n# comment\n" > "$PROJ_DIR/.ai-profile"
cd "$PROJ_DIR"
result=$(_find_project_profile)
[ "$result" = "myproj" ] && pass "_find_project_profile trims whitespace from .ai-profile" \
    || fail "_find_project_profile trim: expected 'myproj', got '$result'"

# Test 6: Explicit name takes priority over .ai-profile
# (Simulated: if _profile_env returns non-empty for 'other', that takes priority)
cd "$PROJ_DIR"
env_check=$(_profile_env "other" 2>/dev/null)
[ -n "$env_check" ] && pass "Explicit 'other' profile env resolves over project myproj" \
    || fail "Explicit profile lookup failed"

# Test 7: Default fallback works when no .ai-profile
cd "$TMP_DIR"
default=$(_default_profile)
[ "$default" = "generic" ] && pass "_default_profile returns 'generic'" \
    || fail "_default_profile: expected 'generic', got '$default'"

# Cleanup project test
rm -rf "$PROJ_DIR"
rm -rf "$(dirname "$CFG2")"

set -e  # Re-enable after project tests

echo ""
echo "--- Codex/Qoder auth modes ---"

AUTH_HOME="$TMP_DIR/auth-home"
AUTH_BIN="$TMP_DIR/auth-bin"
mkdir -p "$AUTH_HOME/.kn" "$AUTH_HOME/.codex" "$AUTH_BIN"
cat > "$AUTH_HOME/.codex/auth.json" << 'YEOF'
{"auth_mode":"chatgpt","tokens":{"id_token":"original"}}
YEOF
cat > "$AUTH_HOME/.kn/config.yaml" << 'YEOF'
default: codex-key
profiles:
  codex-key:
    desc: "Codex API key"
    env:
      _KN_CLI_TYPE: codex
      _KN_AUTH_MODE: api_key
      OPENAI_API_KEY: sk-codex-test
      OPENAI_BASE_URL: https://proxy.example.com/v1
  codex-login:
    desc: "Codex local login"
    env:
      _KN_CLI_TYPE: codex
      _KN_AUTH_MODE: local_login
  qoder-token:
    desc: "QoderCN token"
    env:
      _KN_CLI_TYPE: qoderclicn
      _KN_AUTH_MODE: token
      QODERCN_PERSONAL_ACCESS_TOKEN: qo-test-token
YEOF
cat > "$AUTH_BIN/codex" << 'YEOF'
#!/bin/bash
printf '%s\n' "$@" > "$HOME/codex-args.txt"
exit 0
YEOF
cat > "$AUTH_BIN/qoderclicn" << 'YEOF'
#!/bin/bash
[ "$QODERCN_PERSONAL_ACCESS_TOKEN" = "qo-test-token" ] || exit 9
exit 0
YEOF
chmod +x "$AUTH_BIN/codex" "$AUTH_BIN/qoderclicn"

auth_before=$(cat "$AUTH_HOME/.codex/auth.json")
HOME="$AUTH_HOME" KN_HOME="$AUTH_HOME/.kn" CONFIG="$AUTH_HOME/.kn/config.yaml" PATH="$AUTH_BIN:$PATH" _ai_launch_with_profile codex codex-key >/dev/null
auth_after=$(cat "$AUTH_HOME/.codex/auth.json")
[ "$auth_after" = "$auth_before" ] && pass "Codex API key profile restores auth.json" \
    || fail "Codex API key profile did not restore auth.json"
grep -q 'model_provider="custom"' "$AUTH_HOME/codex-args.txt" \
    && pass "Codex API key profile selects custom provider for base URL" \
    || fail "Codex API key profile did not select custom provider"

HOME="$AUTH_HOME" KN_HOME="$AUTH_HOME/.kn" CONFIG="$AUTH_HOME/.kn/config.yaml" PATH="$AUTH_BIN:$PATH" _ai_launch_with_profile codex codex-login >/dev/null
auth_login_after=$(cat "$AUTH_HOME/.codex/auth.json")
[ "$auth_login_after" = "$auth_before" ] && pass "Codex local-login profile does not modify auth.json" \
    || fail "Codex local-login profile modified auth.json"
HOME="$AUTH_HOME" KN_HOME="$AUTH_HOME/.kn" CONFIG="$AUTH_HOME/.kn/config.yaml" PATH="$AUTH_BIN:$PATH" ai codex codex-login >/dev/null
if grep -q '^codex-login$' "$AUTH_HOME/codex-args.txt"; then
    fail "ai codex <local-login-profile> leaked profile name to Codex"
else
    pass "ai codex <local-login-profile> consumes profile name"
fi
rm -f "$AUTH_HOME/codex-args.txt"
set +e
missing_output=$(HOME="$AUTH_HOME" KN_HOME="$AUTH_HOME/.kn" CONFIG="$AUTH_HOME/.kn/config.yaml" PATH="$AUTH_BIN:$PATH" ai codex codex-missing 2>&1 >/dev/null)
missing_rc=$?
set -e
[ "$missing_rc" != "0" ] && echo "$missing_output" | grep -q "Profile 'codex-missing' not found" \
    && pass "ai codex <missing-profile> errors instead of leaking argument" \
    || fail "ai codex <missing-profile> did not error cleanly"
[ ! -f "$AUTH_HOME/codex-args.txt" ] \
    && pass "ai codex <missing-profile> does not launch Codex" \
    || fail "ai codex <missing-profile> launched Codex"

HOME="$AUTH_HOME" KN_HOME="$AUTH_HOME/.kn" CONFIG="$AUTH_HOME/.kn/config.yaml" PATH="$AUTH_BIN:$PATH" _ai_launch_with_profile qoderclicn qoder-token >/dev/null \
    && pass "QoderCN token profile injects QODERCN_PERSONAL_ACCESS_TOKEN" \
    || fail "QoderCN token profile did not inject token"

echo ""
echo -e "=== ${GREEN}$PASS passed${RESET}, ${RED}$FAIL failed${RESET} ==="
[ "$FAIL" -eq 0 ] || exit 1
