#!/usr/bin/env bash
# Hook: 自动 Lint 检查
# Event: PostToolUse  Matcher: Write|Edit
# 写文件后自动运行 lint，错误注入 Claude 上下文供自动修复
set -euo pipefail

file=$(jq -r '.tool_input.file_path // .tool_input.path // ""' 2>/dev/null || echo "")

if [ -z "$file" ] || [ ! -f "$file" ]; then
  exit 0
fi

dir=$(dirname "$file")

if [ -f "$dir/package.json" ] || [ -f "package.json" ]; then
  npx -y eslint "$file" --format compact 2>/dev/null | head -20 || true
fi

exit 0
