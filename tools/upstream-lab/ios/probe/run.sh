#!/usr/bin/env sh
# SPDX-License-Identifier: Apache-2.0
# SPDX-FileCopyrightText: The Rivqen Authors
#
# Foundation probe of -[NSString boolValue] (WP-01 S1 T-05, D-29). No upstream code, no network.
# Usage: sh tools/upstream-lab/ios/probe/run.sh [cases.json [out.jsonl]]
# Defaults: tools/upstream-lab/client/functions.json, tools/upstream-lab/ios/build/boolvalue.jsonl.
set -eu
HERE="$(cd "$(dirname "$0")" && pwd)"
CASES="${1:-$HERE/../../client/functions.json}"
OUT="${2:-$HERE/../build/boolvalue.jsonl}"
BIN="$HERE/../build/rivqen-ios-boolvalue"
mkdir -p "$HERE/../build" "$(dirname "$OUT")"
clang -fobjc-arc -Wall -Wextra -Werror -framework Foundation -o "$BIN" "$HERE/boolvalue.m"
"$BIN" "$CASES" "$OUT"
echo "probe: $(wc -l < "$OUT" | tr -d ' ') values -> $OUT (macOS $(sw_vers -productVersion))"
