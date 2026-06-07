#!/usr/bin/env bash
# Hook: TODO/FIXME 收集器
# Event: PostToolUse  Matcher: Edit|Write
# AI 写文件后扫描新增的 TODO/FIXME/HACK 标记，汇总到上下文
set -euo pipefail

file=$(jq -r '.tool_input.file_path // .tool_input.path // ""' 2>/dev/null || echo "")

if [ -z "$file" ] || [ ! -f "$file" ]; then
  exit 0
fi

# 只扫描代码文件
case "$file" in
  *.py|*.js|*.ts|*.jsx|*.tsx|*.rs|*.go|*.java|*.rb|*.swift|*.kt|*.c|*.cpp|*.h|*.sh|*.bash|*.zsh) ;;
  *) exit 0 ;;
esac

# 查找 TODO / FIXME / HACK / XXX / OPTIMIZE 标记
markers=$(grep -nE '(TODO|FIXME|HACK|XXX|OPTIMIZE)\b' "$file" 2>/dev/null | head -10 || true)

if [ -n "$markers" ]; then
  count=$(echo "$markers" | wc -l | tr -d ' ')
  echo "[todo-collector] 📝 $file 中发现 $count 个标记:" >&2
  echo "$markers" >&2
fi

exit 0
