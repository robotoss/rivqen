#!/usr/bin/env sh
# SPDX-License-Identifier: Apache-2.0
# SPDX-FileCopyrightText: The Rivqen Authors
#
# Check of the runner code that needs no upstream file (WP-01 S2): the JSON writer runner/json.m.
# Usage: sh tools/upstream-lab/ios/check.sh
# Needs only the macOS SDK (xcrun). No UPSTREAM. Output: build/check (git-ignored).
set -eu
HERE="$(cd "$(dirname "$0")" && pwd)"
OUT="$HERE/build/check"
rm -rf "$OUT"
mkdir -p "$OUT"
SDK_PATH="$(xcrun --sdk macosx --show-sdk-path)"
CC="xcrun --sdk macosx clang -isysroot $SDK_PATH"
# The same flags as the runner in build.sh: ARC, warnings are errors.
$CC -fobjc-arc -Wall -Wextra -Werror -I"$HERE/runner" \
  "$HERE/runner/json.m" "$HERE/check/json_check.m" -framework Foundation -o "$OUT/json-check"
"$OUT/json-check"
