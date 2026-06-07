#!/usr/bin/env bash
# Hook: Python 类型检查 (mypy)
# Event: PostToolUse  Matcher: Write|Edit
# 写 .py 文件后自动 mypy 检查，类型错误注入 Claude 上下文
set -euo pipefail

file=$(jq -r '.tool_input.file_path // .tool_input.path // ""' 2>/dev/null || echo "")

if [ -z "$file" ] || [ ! -f "$file" ]; then
  exit 0
fi

# 只处理 Python 文件
case "$file" in *.py) ;; *) exit 0 ;; esac

# 检查 mypy 是否可用
if ! command -v mypy &>/dev/null; then
  exit 0
fi

# 配置文件优先级: pyproject.toml > mypy.ini > 默认
out=$(mypy "$file" --no-error-summary 2>&1 | head -15) || true

if [ -n "$out" ]; then
  echo "[python-typecheck] mypy 发现类型问题:" >&2
  echo "$out" >&2
fi

exit 0
