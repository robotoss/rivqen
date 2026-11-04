#!/usr/bin/env sh
# SPDX-License-Identifier: Apache-2.0
# SPDX-FileCopyrightText: The Rivqen Authors
#
# Build the harness APK around the unmodified upstream SDK sources (H-118).
# Usage: UPSTREAM=<VasSonic clone at 59936bef> sh tools/upstream-lab/android/build.sh
# Output: tools/upstream-lab/android/app/build/outputs/apk/debug/app-debug.apk
set -eu
HERE="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$HERE/../../.." && pwd)"
. "$HERE/lab-env.sh"
: "${UPSTREAM:?Set UPSTREAM to the read-only VasSonic clone at 59936bef}"
PINNED=59936beff656d4b5718ff6444d6c5e001a2c5231
SDK_SRC=sonic-android/sdk/src/main/java
MANIFEST="$ROOT/evidence/source-manifest.lock"

# 1. Pinned commit and unchanged SDK sources (the Gradle build checks the commit again).
head="$(git -C "$UPSTREAM" rev-parse HEAD)"
[ "$head" = "$PINNED" ] || { echo "build.sh: UPSTREAM is at $head, not at $PINNED" >&2; exit 1; }
# Every SDK source file must have the SHA-256 of the source manifest, and no other file may exist.
expected="$(grep "  $SDK_SRC/" "$MANIFEST" | sort -k2)"
[ -n "$expected" ] || { echo "build.sh: no $SDK_SRC entries in $MANIFEST" >&2; exit 1; }
actual="$(cd "$UPSTREAM" && find "$SDK_SRC" -type f | LC_ALL=C sort | while IFS= read -r f; do shasum -a 256 "$f"; done | sort -k2)"
if [ "$expected" != "$actual" ]; then
  echo "build.sh: the SDK sources in UPSTREAM differ from evidence/source-manifest.lock" >&2
  printf '%s\n' "$expected" > "$TMPDIR/sdk-expected.txt"
  printf '%s\n' "$actual" > "$TMPDIR/sdk-actual.txt"
  diff "$TMPDIR/sdk-expected.txt" "$TMPDIR/sdk-actual.txt" >&2 || true
  exit 1
fi
echo "build.sh: UPSTREAM $PINNED, $(printf '%s\n' "$actual" | wc -l | tr -d ' ') SDK source files match the source manifest"

# 2. Compile against the main SDK (read only; android.builder.sdkDownload=false stops downloads).
[ -d "$MAIN_SDK/platforms/android-36" ] || { echo "build.sh: $MAIN_SDK has no platforms/android-36" >&2; exit 1; }
[ -d "$MAIN_SDK/build-tools/36.0.0" ] || { echo "build.sh: $MAIN_SDK has no build-tools/36.0.0" >&2; exit 1; }
mkdir -p "$TMPDIR"
printf 'sdk.dir=%s\n' "$MAIN_SDK" > "$HERE/local.properties"
export UPSTREAM
cd "$HERE"
ANDROID_HOME="$MAIN_SDK" ANDROID_SDK_ROOT="$MAIN_SDK" ./gradlew --console=plain -q assembleDebug

# 3. The only library is support-annotations 28.0.0 (H-118).
libs="$(ANDROID_HOME="$MAIN_SDK" ANDROID_SDK_ROOT="$MAIN_SDK" ./gradlew --console=plain -q :app:dependencies \
  --configuration debugRuntimeClasspath | grep -E '^[+\\]--- ' | sed 's/^[+\\]--- //' | sort -u)"
if [ "$libs" != "com.android.support:support-annotations:28.0.0" ]; then
  echo "build.sh: unexpected runtime libraries:" >&2; printf '%s\n' "$libs" >&2; exit 1
fi

apk="$HERE/app/build/outputs/apk/debug/app-debug.apk"
[ -f "$apk" ] || { echo "build.sh: no APK at $apk" >&2; exit 1; }
echo "build.sh: libraries: $libs"
echo "build.sh: APK $apk ($(wc -c < "$apk" | tr -d ' ') bytes, sha256 $(shasum -a 256 "$apk" | cut -d' ' -f1))"
