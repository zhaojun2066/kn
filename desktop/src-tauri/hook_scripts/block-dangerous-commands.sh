#!/usr/bin/env bash
# Hook: 危险命令拦截
# Event: PreToolUse  Matcher: Bash
# 拦截 rm -rf /、git push --force main、curl|sh、fork bomb 等危险操作
set -euo pipefail

cmd=$(jq -r '.tool_input.command // ""' 2>/dev/null || echo "")

# 危险模式列表
deny_patterns=(
  'rm\s+-rf\s+/'
  'rm\s+-rf\s+~'
  'rm\s+-rf\s+\$HOME'
  'git\s+push\s+--force.*(main|master)'
  'git\s+reset\s+--hard'
  'git\s+clean\s+-[fdx]+'
  'curl.*\|\s*(ba)?sh'
  'wget.*\|\s*(ba)?sh'
  'DROP\s+TABLE'
  'chmod\s+777\s+/'
  ':\(\)\s*\{'          # fork bomb
  'mkfs\.'
  'dd\s+if='
  '>\s*/dev/sda'
)

for pat in "${deny_patterns[@]}"; do
  if echo "$cmd" | grep -Eiq "$pat"; then
    echo "🚨 [block-dangerous-commands] 危险命令被阻止: $cmd" >&2
    echo "   匹配规则: $pat" >&2
    exit 2
  fi
done

exit 0
