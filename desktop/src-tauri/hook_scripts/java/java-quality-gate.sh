#!/usr/bin/env bash
# Hook: Java 回合结束质量门禁
# Event: Stop
# 依次执行 spotless:check → compile → test，有错注入 Claude 自愈
set -euo pipefail

errors=""

# 检测 Maven 还是 Gradle
if [ -f "pom.xml" ]; then
  BUILD="maven"
  MVN="./mvnw"
  [ ! -f "$MVN" ] && MVN="mvn"
elif [ -f "build.gradle" ] || [ -f "build.gradle.kts" ]; then
  BUILD="gradle"
  GW="./gradlew"
  [ ! -f "$GW" ] && GW="gradle"
else
  exit 0
fi

if [ "$BUILD" = "maven" ]; then
  # Spotless check
  out=$($MVN spotless:check -q 2>&1 | head -20) || true
  [ -n "$out" ] && errors+="[Spotless]\n$out\n\n"

  # Compile
  out=$($MVN compile -q 2>&1 | head -30) || true
  [ -n "$out" ] && errors+="[Compile]\n$out\n\n"

  # Test
  out=$($MVN test -q 2>&1 | tail -20) || true
  if echo "$out" | grep -qi "fail\|error\|BUILD FAILURE"; then
    errors+="[Tests]\n$out\n\n"
  fi
else
  out=$($GW spotlessCheck -q 2>&1 | head -20) || true
  [ -n "$out" ] && errors+="[Spotless]\n$out\n\n"

  out=$($GW compileJava -q 2>&1 | head -30) || true
  [ -n "$out" ] && errors+="[Compile]\n$out\n\n"

  out=$($GW test -q 2>&1 | tail -20) || true
  if echo "$out" | grep -qi "fail\|error\|BUILD FAILED"; then
    errors+="[Tests]\n$out\n\n"
  fi
fi

if [ -n "$errors" ]; then
  echo "$errors" >&2
  exit 2
fi

echo "[java-quality-gate] ✅ Spotless + Compile + Test 全部通过"
exit 0
