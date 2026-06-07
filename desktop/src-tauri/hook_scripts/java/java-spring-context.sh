#!/usr/bin/env bash
# Hook: Java Spring Boot 项目上下文注入
# Event: SessionStart  Matcher: startup|compact
# 检测项目信息并注入给 AI：Java 版本、Spring Boot 版本、构建工具等
set -euo pipefail

context=""

# Java version
if command -v java &>/dev/null; then
  java_ver=$(java -version 2>&1 | head -1 | grep -oE '[0-9]+\.[0-9]+\.[0-9]+' || echo "unknown")
  context+="Java 版本: $java_ver\n"
fi

# Spring Boot version from pom.xml
if [ -f "pom.xml" ]; then
  spring_ver=$(grep -A1 'spring-boot-starter-parent' pom.xml 2>/dev/null | grep -oE '[0-9]+\.[0-9]+\.[0-9]+' | head -1 || echo "")
  [ -n "$spring_ver" ] && context+="Spring Boot 版本: $spring_ver\n"
  # Check for mvnw
  [ -f "mvnw" ] && context+="构建: Maven Wrapper (./mvnw)\n" || context+="构建: Maven (mvn)\n"
elif [ -f "build.gradle" ] || [ -f "build.gradle.kts" ]; then
  spring_ver=$(grep 'springBoot' build.gradle build.gradle.kts 2>/dev/null | grep -oE '[0-9]+\.[0-9]+\.[0-9]+' | head -1 || echo "")
  [ -n "$spring_ver" ] && context+="Spring Boot 版本: $spring_ver\n"
  [ -f "gradlew" ] && context+="构建: Gradle Wrapper (./gradlew)\n" || context+="构建: Gradle (gradle)\n"
fi

# EditorConfig
if [ -f ".editorconfig" ]; then
  context+="代码风格: .editorconfig 已配置\n"
fi

if [ -n "$context" ]; then
  cat <<EOF
{
  "hookSpecificOutput": {
    "hookEventName": "SessionStart",
    "additionalContext": "项目 Java 环境:\\n$context\\n请遵循: 用 ./mvnw 或 ./gradlew 而非系统命令; 代码生成后 compile; 提交前 spotless:check + test。"
  }
}
EOF
fi

exit 0
