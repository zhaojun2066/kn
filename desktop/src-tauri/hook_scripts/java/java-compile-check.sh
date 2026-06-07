#!/usr/bin/env bash
# Hook: Java 编译检查
# Event: PostToolUse  Matcher: Write|Edit
# 写 .java 文件后自动编译，错误注入 Claude 上下文
set -euo pipefail

file=$(jq -r '.tool_input.file_path // .tool_input.path // ""' 2>/dev/null || echo "")

if [ -z "$file" ] || [ ! -f "$file" ]; then exit 0; fi
if ! echo "$file" | grep -q '\.java$'; then exit 0; fi

dir=$(dirname "$file")
while [ "$dir" != "/" ] && [ "$dir" != "." ]; do
  if [ -f "$dir/pom.xml" ] || [ -f "$dir/build.gradle" ] || [ -f "$dir/build.gradle.kts" ]; then
    break
  fi
  dir=$(dirname "$dir")
done

cd "$dir" 2>/dev/null || exit 0

errors=""

if [ -f "pom.xml" ]; then
  cmd="../mvnw"
  [ ! -f "$cmd" ] && cmd="mvn"
  errors=$($cmd compile -pl . -q 2>&1 | head -30) || true
elif [ -f "build.gradle" ] || [ -f "build.gradle.kts" ]; then
  cmd="../gradlew"
  [ ! -f "$cmd" ] && cmd="gradle"
  errors=$($cmd compileJava -p . -q 2>&1 | head -30) || true
fi

if [ -n "$errors" ]; then
  echo "[Java 编译错误]" >&2
  echo "$errors" >&2
fi

exit 0
