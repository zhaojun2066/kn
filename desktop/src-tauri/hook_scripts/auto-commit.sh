#!/usr/bin/env bash
# Hook: 自动 Git 提交
# Event: Stop
# 回合结束时自动 git add -A && commit，适合无人值守场景
set -euo pipefail

if ! git rev-parse --git-dir >/dev/null 2>&1; then
  exit 0  # 不在 git 仓库中
fi

git add -A

if git diff --cached --quiet; then
  exit 0  # 没有变更
fi

timestamp=$(date '+%Y-%m-%d %H:%M')
git commit -m "auto(ai): session save $timestamp" >/dev/null 2>&1 || true

echo "[auto-commit] ✅ 已自动提交"
exit 0
