#!/usr/bin/env bash
# Hook: 阻断 main/master 分支编辑
# Event: PreToolUse  Matcher: Edit|Write
# 禁止在 main 或 master 分支上直接编辑文件，强制 AI 先切分支
set -euo pipefail

branch=$(git branch --show-current 2>/dev/null || echo "")

if [ "$branch" = "main" ] || [ "$branch" = "master" ]; then
  cat >&2 <<EOF
🚨 [block-main-branch-edits] 禁止在 "$branch" 分支直接编辑文件！

请先创建功能分支:
  git checkout -b feat/<任务描述>

或使用 /worktree 隔离工作区。
EOF
  exit 2
fi

exit 0
