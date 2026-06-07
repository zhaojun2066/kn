#!/usr/bin/env bash
# Hook: TypeScript 类型检查 (tsc)
# Event: PostToolUse  Matcher: Write|Edit
# 写 .ts/.tsx 文件后自动 tsc --noEmit，类型错误注入 Claude 上下文
# 大型项目自动启用 --incremental 加速
set -euo pipefail

file=$(jq -r '.tool_input.file_path // .tool_input.path // ""' 2>/dev/null || echo "")

if [ -z "$file" ] || [ ! -f "$file" ]; then
  exit 0
fi

# 只处理 TypeScript 文件
case "$file" in *.ts|*.tsx) ;; *) exit 0 ;; esac

# 检查 tsconfig.json 是否存在
if [ ! -f "tsconfig.json" ]; then
  exit 0
fi

# 检查 tsc 是否可用（优先用项目本地的）
tsc_bin="npx"
if [ -f "./node_modules/.bin/tsc" ]; then
  tsc_bin="./node_modules/.bin/tsc"
else
  tsc_bin="npx -y tsc"
fi

# 大型项目用增量模式加速
inc_flag=""
if [ -f "tsconfig.json" ]; then
  # 简单判断: 文件数 > 50 则启用增量
  ts_count=$(find . -name '*.ts' -o -name '*.tsx' 2>/dev/null | head -60 | wc -l | tr -d ' ')
  if [ "$ts_count" -gt 50 ]; then
    inc_flag="--incremental"
  fi
fi

out=$($tsc_bin --noEmit $inc_flag 2>&1 | head -20) || true

if [ -n "$out" ]; then
  echo "[typescript-typecheck] tsc 发现类型错误:" >&2
  echo "$out" >&2
fi

exit 0
