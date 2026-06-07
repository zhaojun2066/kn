#!/usr/bin/env bash
# Hook: Java Checkstyle 检查
# Event: PostToolUse  Matcher: Write|Edit
# 写 .java 文件后运行 checkstyle，命名/规范问题注入 Claude
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

out=""
if [ -f "pom.xml" ]; then
  cmd="../mvnw"; [ ! -f "$cmd" ] && cmd="mvn"
  out=$($cmd checkstyle:check -pl . -q 2>&1 | head -20) || true
elif [ -f "build.gradle" ] || [ -f "build.gradle.kts" ]; then
  cmd="../gradlew"; [ ! -f "$cmd" ] && cmd="gradle"
  out=$($cmd checkstyleMain -p . -q 2>&1 | head -20) || true
fi

if [ -n "$out" ]; then
  echo "[Checkstyle]" >&2
  echo "$out" >&2
fi

exit 0
