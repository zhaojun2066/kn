#!/usr/bin/env bash
# Hook: 自动 Git Add
# Event: PostToolUse  Matcher: Edit|Write
# AI 编辑文件后自动 git add，适合与 auto-commit 搭配使用
set -euo pipefail

file=$(jq -r '.tool_input.file_path // .tool_input.path // ""' 2>/dev/null || echo "")

if [ -z "$file" ] || [ ! -f "$file" ]; then
  exit 0
fi

# 检查是否在 git 仓库内
if ! git rev-parse --git-dir >/dev/null 2>&1; then
  exit 0
fi

# 只 add 被 hook 触发的文件（而非整个工作区）
git add "$file" 2>/dev/null || true

exit 0
