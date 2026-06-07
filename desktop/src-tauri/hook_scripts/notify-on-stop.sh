#!/usr/bin/env bash
# Hook: 任务完成桌面通知
# Event: Stop
# 回合结束时发送桌面通知（macOS/Linux 自适应）
set -euo pipefail

msg="AI 任务已完成"

if [[ "$(uname)" == "Darwin" ]]; then
  osascript -e "display notification \"$msg\" with title \"Hook\""
elif command -v notify-send &>/dev/null; then
  notify-send "Hook" "$msg"
fi

exit 0
