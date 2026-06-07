#!/usr/bin/env bash
# Hook: 回合结束质量门禁
# Event: Stop
# 每回合结束时运行 tsc + lint + test，有错误则注入 Claude 形成自愈循环
set -euo pipefail

# Portable timeout helper — uses `timeout` (Linux) or `gtimeout` (macOS coreutils)
# Falls back to perl alarm if neither is available
_timeout() {
  local sec="$1"; shift
  if command -v timeout &>/dev/null; then
    timeout "${sec}" "$@" 2>&1
  elif command -v gtimeout &>/dev/null; then
    gtimeout "${sec}" "$@" 2>&1
  else
    perl -e 'alarm shift; exec @ARGV; print STDERR "TIMEOUT\n"' "${sec}" "$@" 2>&1
  fi
}

errors=""

# TypeScript type-check (max 30s)
if [ -f "tsconfig.json" ]; then
  out=$(_timeout 30 npx -y tsc --noEmit | head -30) || true
  if [ -n "$out" ]; then
    errors+="[TypeScript]\n$out\n\n"
  fi
fi

# ESLint (max 30s)
if [ -f "eslint.config.js" ] || [ -f ".eslintrc.js" ] || [ -f ".eslintrc.json" ]; then
  out=$(_timeout 30 npx -y eslint . --format compact | head -20) || true
  if [ -n "$out" ]; then
    errors+="[ESLint]\n$out\n\n"
  fi
fi

# Tests (max 60s) — timeout prevents hang on interactive test runners
if grep -q '"test"' package.json 2>/dev/null; then
  out=$(_timeout 60 npm test --silent 2>&1 | tail -15) || true
  if echo "$out" | grep -qi "fail\|error"; then
    errors+="[Tests]\n$out\n\n"
  fi
fi

if [ -n "$errors" ]; then
  echo "$errors" >&2
  exit 2  # 注入 Claude，触发自愈
fi

echo "[stop-quality-gate] ✅ 全部通过"
exit 0
