#!/usr/bin/env bash
# Hook: 自动格式化
# Event: PostToolUse  Matcher: Write|Edit
# AI 写文件后自动调用对应格式化工具
set -euo pipefail

file=$(jq -r '.tool_input.file_path // .tool_input.path // ""' 2>/dev/null || echo "")

if [ -z "$file" ] || [ ! -f "$file" ]; then
  exit 0
fi

case "$file" in
  *.js|*.ts|*.jsx|*.tsx|*.json|*.md|*.html|*.css|*.yaml|*.yml)
    npx -y prettier --write "$file" 2>/dev/null || true ;;
  *.py)
    ruff format "$file" 2>/dev/null || true ;;
  *.sh|*.bash|*.zsh)
    shfmt -w "$file" 2>/dev/null || true ;;
  *.rs)
    rustfmt "$file" 2>/dev/null || true ;;
  *.go)
    gofmt -w "$file" 2>/dev/null || true ;;
esac

exit 0
