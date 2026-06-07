#!/usr/bin/env bash
# Hook: 敏感文件保护
# Event: PreToolUse  Matcher: Write|Edit
# 阻止 AI 修改 .env、.git/、密钥文件等
set -euo pipefail

# 获取文件路径（不同 CLI 的字段名可能不同）
file=$(jq -r '.tool_input.file_path // .tool_input.path // ""' 2>/dev/null || echo "")

if [ -z "$file" ]; then
  exit 0
fi

deny_globs=(
  '.env'
  '.env.*'
  '.git/'
  '.gitignore'
  '*.pem'
  '*.key'
  'id_rsa'
  'id_ed25519'
  'credentials'
  'package-lock.json'
  'yarn.lock'
  'pnpm-lock.yaml'
  '.npmrc'
)

for g in "${deny_globs[@]}"; do
  # Convert glob to grep pattern
  pat="${g//\./\\.}"
  pat="${pat//\*/.*}"
  if echo "$file" | grep -Eiq "(^|/)${pat}$"; then
    echo "🛡️ [protect-sensitive-files] 敏感文件修改被阻止: $file" >&2
    exit 2
  fi
done

exit 0
