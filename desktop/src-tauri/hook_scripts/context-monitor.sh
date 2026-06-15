#!/usr/bin/env bash
# Hook: 上下文用量监控
# Event: PostToolUse
# 跟踪工具调用次数，接近上限时警告
set -euo pipefail

COUNTER_FILE="${KN_HOME:-${HOME}/.kn}/hooks/.context-counter"

# 读取当前计数
count=0
if [ -f "$COUNTER_FILE" ]; then
  count=$(cat "$COUNTER_FILE")
fi
count=$((count + 1))
echo "$count" > "$COUNTER_FILE"

# 阈值警告
if [ "$count" -eq 80 ]; then
  echo "[context-monitor] ⚠️ 已执行 $count 次工具调用，建议 /compact 或保存 checkpoint" >&2
elif [ "$count" -eq 120 ]; then
  echo "[context-monitor] 🟠 已执行 $count 次工具调用，上下文接近溢出！请立即 /compact" >&2
elif [ "$count" -eq 150 ]; then
  echo "[context-monitor] 🔴 已执行 $count 次工具调用，强烈建议停止当前任务并 /clear" >&2
fi

exit 0
