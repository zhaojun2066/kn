#!/usr/bin/env bash
# Hook: Rust Clippy 检查
# Event: PostToolUse  Matcher: Write|Edit
# 写 .rs 文件后自动 cargo clippy，lint 结果注入 Claude 上下文
set -euo pipefail

file=$(jq -r '.tool_input.file_path // .tool_input.path // ""' 2>/dev/null || echo "")

if [ -z "$file" ] || [ ! -f "$file" ]; then
  exit 0
fi

# 只处理 Rust 文件
case "$file" in *.rs) ;; *) exit 0 ;; esac

# 检查 Cargo.toml 是否存在
if [ ! -f "Cargo.toml" ]; then
  exit 0
fi

# 检查 cargo 是否可用
if ! command -v cargo &>/dev/null; then
  exit 0
fi

# 针对单个文件运行 clippy（取包名来限定范围）
# cargo clippy 不支持单文件，但可以用 --message-format short 减少输出
out=$(cargo clippy --message-format short 2>&1 | head -20) || true

if [ -n "$out" ] && ! echo "$out" | grep -q "Finished.*dev.*profile"; then
  echo "[rust-clippy] Clippy 发现建议:" >&2
  echo "$out" >&2
fi

exit 0
