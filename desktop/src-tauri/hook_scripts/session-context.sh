#!/usr/bin/env bash
# Hook: 会话注入项目规范
# Event: SessionStart  Matcher: startup|resume
# 会话启动时读取项目规范文件，注入给 AI
set -euo pipefail

context=""

for f in "AGENTS.md" "CONTRIBUTING.md" "CLAUDE.md" "GEMINI.md"; do
  if [ -f "$f" ]; then
    content=$(head -50 "$f")
    context="${context}
[${f}]
${content}
"
  fi
done

if [ -n "$context" ]; then
  # Use jq to safely construct JSON — avoids injection from file content
  # containing double quotes, backslashes, or special characters.
  jq -n \
    --arg ctx "$context" \
    '{
      hookSpecificOutput: {
        hookEventName: "SessionStart",
        additionalContext: ("项目规范已加载:" + $ctx)
      }
    }'
fi

# 重置上下文计数器
COUNTER_FILE="${HOME}/.claude-profiles/hooks/.context-counter"
echo "0" > "$COUNTER_FILE"

exit 0
