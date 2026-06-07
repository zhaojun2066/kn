#!/usr/bin/env bash
# Hook: Java 自动格式化 (Spotless)
# Event: PostToolUse  Matcher: Write|Edit
# 写 .java 文件后自动运行 Spotless 格式化，Maven/Gradle 自动检测
set -euo pipefail

file=$(jq -r '.tool_input.file_path // .tool_input.path // ""' 2>/dev/null || echo "")

if [ -z "$file" ] || [ ! -f "$file" ]; then exit 0; fi
if ! echo "$file" | grep -q '\.java$'; then exit 0; fi

# 找到 java 文件所在的模块目录
dir=$(dirname "$file")
while [ "$dir" != "/" ] && [ "$dir" != "." ]; do
  if [ -f "$dir/pom.xml" ] || [ -f "$dir/build.gradle" ] || [ -f "$dir/build.gradle.kts" ]; then
    break
  fi
  dir=$(dirname "$dir")
done

cd "$dir" 2>/dev/null || exit 0

# Maven
if [ -f "pom.xml" ]; then
  if [ -f "../mvnw" ]; then
    ../mvnw spotless:apply -pl . -q 2>/dev/null || true
  elif command -v mvn &>/dev/null; then
    mvn spotless:apply -pl . -q 2>/dev/null || true
  fi
# Gradle
elif [ -f "build.gradle" ] || [ -f "build.gradle.kts" ]; then
  if [ -f "../gradlew" ]; then
    ../gradlew spotlessApply -p . -q 2>/dev/null || true
  elif command -v gradle &>/dev/null; then
    gradle spotlessApply -p . -q 2>/dev/null || true
  fi
fi

exit 0
