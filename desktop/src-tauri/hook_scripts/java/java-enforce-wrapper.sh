#!/usr/bin/env bash
# Hook: Java 强制使用 Maven/Gradle Wrapper
# Event: PreToolUse  Matcher: Bash
# 如果项目有 mvnw/gradlew 但 AI 直接用了 mvn/gradle，阻断并提示
set -euo pipefail

cmd=$(jq -r '.tool_input.command // ""' 2>/dev/null || echo "")

# 检测是否直接使用 mvn (而非 ./mvnw)
if echo "$cmd" | grep -qE '^\s*mvn\s'; then
  if [ -f "mvnw" ]; then
    echo "[java-enforce-wrapper] 本项目有 Maven Wrapper，请使用 ./mvnw 而非 mvn" >&2
    exit 2
  fi
fi

# 检测是否直接使用 gradle (而非 ./gradlew)
if echo "$cmd" | grep -qE '^\s*gradle\s'; then
  if [ -f "gradlew" ]; then
    echo "[java-enforce-wrapper] 本项目有 Gradle Wrapper，请使用 ./gradlew 而非 gradle" >&2
    exit 2
  fi
fi

exit 0
