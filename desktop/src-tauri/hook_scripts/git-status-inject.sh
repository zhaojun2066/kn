#!/usr/bin/env bash
# Hook: Git 状态注入
# Event: SessionStart  Matcher: startup|resume
# 会话启动时注入 git 状态、当前分支、最近 commit，让 AI 立即了解项目状况
set -euo pipefail

if ! git rev-parse --git-dir >/dev/null 2>&1; then
  exit 0
fi

branch=$(git branch --show-current 2>/dev/null || echo "未知")
status_short=$(git status --short 2>/dev/null | head -20 || echo "")
recent_commits=$(git log --oneline -5 2>/dev/null || echo "")
staged=$(git diff --cached --stat 2>/dev/null | tail -1 || echo "")
unstaged=$(git diff --stat 2>/dev/null | tail -1 || echo "")

# Build context string
context="Git 状态:
  分支: ${branch}"

if [ -n "$staged" ]; then
  context="${context}
  已暂存: ${staged}"
fi
if [ -n "$unstaged" ]; then
  context="${context}
  未暂存: ${unstaged}"
fi
if [ -n "$status_short" ]; then
  context="${context}

变更文件:
${status_short}"
fi
if [ -n "$recent_commits" ]; then
  context="${context}

最近提交:
${recent_commits}"
fi

# Use jq to safely construct JSON — avoids injection from git output containing
# double quotes, backslashes, or newlines that would break heredoc interpolation.
jq -n \
  --arg ctx "$context" \
  '{
    hookSpecificOutput: {
      hookEventName: "SessionStart",
      additionalContext: $ctx
    }
  }'

exit 0
