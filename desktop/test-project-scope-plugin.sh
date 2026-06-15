#!/usr/bin/env bash
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"
PASS=0; FAIL=0

G='\033[32m'; R='\033[31m'; Y='\033[33m'; B='\033[1m'; N='\033[0m'

check() {
  local desc="$1" file="$2" pattern="$3"
  if grep -q "$pattern" "$file" 2>/dev/null; then
    printf "  ${G}PASS${N} ${B}%s${N}\n" "$desc"
    PASS=$((PASS + 1))
  else
    printf "  ${R}FAIL${N} ${B}%s${N} — ${Y}not found in %s${N}\n" "$desc" "$file"
    FAIL=$((FAIL + 1))
  fi
}

check_not() {
  local desc="$1" file="$2" pattern="$3"
  if ! grep -q "$pattern" "$file" 2>/dev/null; then
    printf "  ${G}PASS${N} ${B}%s${N}\n" "$desc"
    PASS=$((PASS + 1))
  else
    printf "  ${R}FAIL${N} ${B}%s${N} — ${Y}unexpected match in %s${N}\n" "$desc" "$file"
    FAIL=$((FAIL + 1))
  fi
}

echo ""
echo "${B}════════════════════════════════════════════════════════${N}"
echo "  Project-Scope Plugin — Plan Conformance Check"
echo "${B}════════════════════════════════════════════════════════${N}"
echo ""

# ═══════════════════════════════════════════════════════════════
# 1. Frontend — MarketplaceBrowser.tsx
# ═══════════════════════════════════════════════════════════════
echo "${B}[1/7] MarketplaceBrowser.tsx — projectPath prop${N}"

check "Interface: projectPath?: string | null" \
  src/components/MarketplaceBrowser.tsx \
  'projectPath.*string.*null'

check "handleInstall passes projectPath to invoke" \
  src/components/MarketplaceBrowser.tsx \
  'projectPath: projectPath'

check "Destructures projectPath from props" \
  src/components/MarketplaceBrowser.tsx \
  '{ open, onClose, onInstalled, projectPath }'

echo ""

# ═══════════════════════════════════════════════════════════════
# 2. Frontend — ProjectWorkspace.tsx
# ═══════════════════════════════════════════════════════════════
echo "${B}[2/7] ProjectWorkspace.tsx — own MarketplaceBrowser${N}"

check "Has marketplaceOpen state" \
  src/components/ProjectWorkspace.tsx \
  'marketplaceOpen.*useState(false)'

check "Imports MarketplaceBrowser" \
  src/components/ProjectWorkspace.tsx \
  'import { MarketplaceBrowser }'

check "Renders MarketplaceBrowser with projectPath" \
  src/components/ProjectWorkspace.tsx \
  'projectPath={project.path}'

check "Has handleMarketplaceInstalled callback" \
  src/components/ProjectWorkspace.tsx \
  'handleMarketplaceInstalled'

check "handleUninstallPlugin passes projectPath" \
  src/components/ProjectWorkspace.tsx \
  '"uninstall_plugin".*projectPath'

echo ""

# ═══════════════════════════════════════════════════════════════
# 3. Frontend — App.tsx regression
# ═══════════════════════════════════════════════════════════════
echo "${B}[3/7] App.tsx — global MarketplaceBrowser unchanged${N}"

check "Imports MarketplaceBrowser" \
  src/App.tsx \
  'import { MarketplaceBrowser }'

# Verify global MarketplaceBrowser block does NOT pass projectPath
BODY=$(sed -n '/<MarketplaceBrowser$/,/\/>/p' src/App.tsx | head -6)
if echo "$BODY" | grep -q 'projectPath'; then
  printf "  ${R}FAIL${N} ${B}Global MarketplaceBrowser must NOT have projectPath${N}\n"
  FAIL=$((FAIL + 1))
else
  printf "  ${G}PASS${N} ${B}Global MarketplaceBrowser has no projectPath${N}\n"
  PASS=$((PASS + 1))
fi

echo ""

# ═══════════════════════════════════════════════════════════════
# 4. Rust — install_plugin
# ═══════════════════════════════════════════════════════════════
echo "${B}[4/7] Rust: install_plugin${N}"
RUST=src-tauri/src/skill_manager/mod.rs

check "install_plugin command exists" \
  "$RUST" \
  'pub async fn install_plugin'

check "Claude install passes --scope" \
  "$RUST" \
  '\"--scope\"'

check "Codex install calls write_codex_project_plugin_enabled" \
  "$RUST" \
  'write_codex_project_plugin_enabled'

echo ""

# ═══════════════════════════════════════════════════════════════
# 5. Rust — install_standalone_skill
# ═══════════════════════════════════════════════════════════════
echo "${B}[5/7] Rust: install_standalone_skill${N}"

check "install_standalone_skill command exists" \
  "$RUST" \
  'pub fn install_standalone_skill'

check "Claude: .claude/skills project path" \
  "$RUST" \
  '.join(\".claude\").join(\"skills\")'

check "Codex: .codex/skills project path" \
  "$RUST" \
  '.join(\".codex\").join(\"skills\")'

check "Qoder: .qoder/skills project path (NOT .qoder-cn)" \
  "$RUST" \
  '.join(\".qoder\").join(\"skills\")'

echo ""

# ═══════════════════════════════════════════════════════════════
# 6. Rust — helpers, uninstall, scan_all
# ═══════════════════════════════════════════════════════════════
echo "${B}[6/7] Rust: helpers, uninstall, scan_all${N}"

check "write_codex_project_plugin_enabled helper" \
  "$RUST" \
  'fn write_codex_project_plugin_enabled'

check "remove_codex_project_plugin helper" \
  "$RUST" \
  'fn remove_codex_project_plugin'

check "uninstall_plugin command exists" \
  "$RUST" \
  'pub async fn uninstall_plugin'

check "Claude uninstall --scope project" \
  "$RUST" \
  '\"uninstall\".*\"--scope\".*\"project\"'

check "Codex uninstall uses remove_codex_project_plugin" \
  "$RUST" \
  'remove_codex_project_plugin'

check "scan_claude_project_plugins exists" \
  "$RUST" \
  'fn scan_claude_project_plugins'

check "scan_codex_project_plugins exists" \
  "$RUST" \
  'fn scan_codex_project_plugins'

# Multi-line: "project_claude_plugins = scan_..." followed by "plugins.extend(project_claude_plugins)"
if grep -A1 'project_claude_plugins = scan_claude_project_plugins' "$RUST" | grep -q 'plugins.extend(project_claude_plugins)'; then
  printf "  ${G}PASS${N} ${B}scan_all merges project plugins (extend)${N}\n"
  PASS=$((PASS + 1))
else
  printf "  ${R}FAIL${N} ${B}scan_all merges project plugins (extend)${N}\n"
  FAIL=$((FAIL + 1))
fi

check "scan_all cross-references user snapshot" \
  "$RUST" \
  'user_plugins_snapshot'

echo ""

# ═══════════════════════════════════════════════════════════════
# 7. Build & Test
# ═══════════════════════════════════════════════════════════════
echo "${B}[7/7] Build & Test${N}"

printf '  TypeScript type check ... '
if npx tsc --noEmit 2>&1; then
  printf "${G}PASS${N}\n"; PASS=$((PASS + 1))
else
  printf "${R}FAIL${N}\n"; FAIL=$((FAIL + 1))
fi

printf '  Rust cargo check ... '
if (cd src-tauri && cargo check 2>&1 >/dev/null); then
  printf "${G}PASS${N}\n"; PASS=$((PASS + 1))
else
  printf "${R}FAIL${N}\n"; FAIL=$((FAIL + 1))
fi

printf '  Vitest unit tests ... '
if npx vitest run 2>&1 >/dev/null; then
  printf "${G}PASS${N} — all tests pass\n"; PASS=$((PASS + 1))
else
  printf "${R}FAIL${N}\n"; FAIL=$((FAIL + 1))
fi

# ═══════════════════════════════════════════════════════════════
# SUMMARY
# ═══════════════════════════════════════════════════════════════
echo ""
echo "${B}════════════════════════════════════════════════════════${N}"
if [ "$FAIL" -eq 0 ]; then
  printf "  %s  (${PASS} checks)\n" "${G}ALL PASSED${N}"
  echo "${B}════════════════════════════════════════════════════════${N}"
  echo ""
  echo "  All plan items are implemented and verified."
  echo ""
  echo "  Manual E2E tests (requires desktop app):"
  echo "  ─────────────────────────────────────────"
  echo "  1. cd desktop && npm run tauri dev"
  echo "  2. ProjectWorkspace > Resource tab > Marketplace"
  echo "  3. Install plugin > check <project>/.claude/settings.json"
  echo "  4. Uninstall > check project config cleaned"
  echo "  5. Global ResourceDrawer > install > verify user-level"
  echo ""
  exit 0
else
  printf "  %s  (%s passed, %s FAILED)\n" "${R}SOME FAILED${N}" "$PASS" "$FAIL"
  echo "${B}════════════════════════════════════════════════════════${N}"
  exit 1
fi
