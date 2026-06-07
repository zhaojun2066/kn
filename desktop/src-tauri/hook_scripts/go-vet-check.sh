#!/usr/bin/env bash
# Hook: Go 静态检查 (go vet)
# Event: PostToolUse  Matcher: Write|Edit
# 写 .go 文件后自动 go vet，检查结果注入 Claude 上下文
set -euo pipefail

file=$(jq -r '.tool_input.file_path // .tool_input.path // ""' 2>/dev/null || echo "")

if [ -z "$file" ] || [ ! -f "$file" ]; then
  exit 0
fi

# 只处理 Go 文件
case "$file" in *.go) ;; *) exit 0 ;; esac

# 检查是否在 Go 模块内
if ! go list ./... >/dev/null 2>&1; then
  exit 0
fi

# 针对文件所在包运行 go vet
pkg_dir=$(dirname "$file")
out=$(cd "$pkg_dir" && go vet 2>&1 | head -15) || true

if [ -n "$out" ]; then
  echo "[go-vet] go vet 发现问题:" >&2
  echo "$out" >&2
fi

exit 0
