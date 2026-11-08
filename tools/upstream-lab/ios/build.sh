#!/usr/bin/env sh
# SPDX-License-Identifier: Apache-2.0
# SPDX-FileCopyrightText: The Rivqen Authors
#
# Build the iOS function runner for macOS (WP-01 S1, H-119).
# Usage: UPSTREAM=/path/to/VasSonic sh tools/upstream-lab/ios/build.sh
# UPSTREAM must be a checkout of Tencent/VasSonic at the pinned commit. The upstream
# files are compiled in place and unchanged, with manual reference counting.
set -eu
HERE="$(cd "$(dirname "$0")" && pwd)"
: "${UPSTREAM:?Set UPSTREAM to the VasSonic checkout}"
PIN="59936beff656d4b5718ff6444d6c5e001a2c5231"
HEAD_SHA="$(git -C "$UPSTREAM" rev-parse HEAD 2>/dev/null || true)"
if [ "$HEAD_SHA" != "$PIN" ]; then
  echo "build.sh: UPSTREAM is at '${HEAD_SHA:-unknown}', expected $PIN" >&2
  exit 1
fi
if [ -n "$(git -C "$UPSTREAM" status --porcelain -- sonic-iOS/Sonic 2>/dev/null)" ]; then
  echo "build.sh: upstream sonic-iOS/Sonic has local changes" >&2
  exit 1
fi

SRC="$UPSTREAM/sonic-iOS/Sonic"
OUT="$HERE/build"
OBJ="$OUT/obj"
rm -rf "$OUT"
mkdir -p "$OBJ"

SDK_PATH="$(xcrun --sdk macosx --show-sdk-path)"
SDK_VERSION="$(xcrun --sdk macosx --show-sdk-version)"
CLANG_VERSION="$(xcrun --sdk macosx clang --version | head -n 1)"
INC="-I$HERE/shim -I$SRC -I$SRC/Cache -I$SRC/Engine -I$SRC/Network -I$SRC/Session -I$SRC/Util -I$SRC/ResourceLoader -I$SRC/Statistics"
CC="xcrun --sdk macosx clang -isysroot $SDK_PATH"

# Upstream: manual reference counting, weak references allowed. Its warnings are expected.
n=0
for f in $(find "$SRC" -name '*.m' | LC_ALL=C sort); do
  n=$((n + 1))
  $CC -c -fno-objc-arc -fobjc-weak -w $INC "$f" -o "$OBJ/up$n.o"
done

# Runner: ARC, warnings are errors.
$CC -c -fobjc-arc -Wall -Wextra -Werror $INC \
  "-DRQ_SDK_VERSION=\"$SDK_VERSION\"" "-DRQ_CLANG_VERSION=\"$CLANG_VERSION\"" "-DRQ_UPSTREAM_SHA=\"$PIN\"" \
  "$HERE/runner/main.m" -o "$OBJ/runner.o"
$CC -c -fobjc-arc -Wall -Wextra -Werror "$HERE/runner/json.m" -o "$OBJ/json.o"

$CC "$OBJ"/*.o -framework Foundation -framework CoreFoundation -framework Security -lsqlite3 -o "$OUT/rivqen-ios-fn"
echo "built $OUT/rivqen-ios-fn ($n upstream files, macOS SDK $SDK_VERSION)"
