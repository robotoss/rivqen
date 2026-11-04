#!/usr/bin/env sh
# SPDX-License-Identifier: Apache-2.0
# SPDX-FileCopyrightText: The Rivqen Authors
#
# Run the iOS function runner (WP-01 S1, H-119). Offline; reads only the case file.
# Usage: sh tools/upstream-lab/ios/run.sh [cases.json [out.jsonl]]
# Defaults: tools/upstream-lab/client/functions.json, tools/upstream-lab/ios/build/functions.jsonl.
# A metadata file (versions, private calls) is written next to the output as <out>.meta.json.
set -eu
HERE="$(cd "$(dirname "$0")" && pwd)"
CASES="${1:-$HERE/../client/functions.json}"
OUT="${2:-$HERE/build/functions.jsonl}"
BIN="$HERE/build/rivqen-ios-fn"
[ -x "$BIN" ] || { echo "run.sh: build first: sh tools/upstream-lab/ios/build.sh" >&2; exit 1; }
[ -f "$CASES" ] || { echo "run.sh: no case file: $CASES" >&2; exit 1; }
# The upstream cache code makes a cache directory when an account is set.
# Keep it inside the build directory, not in the home of the user.
HOME_DIR="$HERE/build/home"
rm -rf "$HOME_DIR"
mkdir -p "$HOME_DIR" "$(dirname "$OUT")"
CFFIXED_USER_HOME="$HOME_DIR" HOME="$HOME_DIR" "$BIN" "$CASES" "$OUT" "$OUT.meta.json"
