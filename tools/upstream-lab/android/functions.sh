#!/usr/bin/env sh
# SPDX-License-Identifier: Apache-2.0
# SPDX-FileCopyrightText: The Rivqen Authors
#
# Run the function cases of CONTRACT section 7 in the harness (mode functions).
# Usage: sh tools/upstream-lab/android/functions.sh <functions.json> <functions.jsonl>
# The case file goes into the app storage (run-as; the APK is debuggable), the results come back
# from there. No network is used.
set -eu
HERE="$(cd "$(dirname "$0")" && pwd)"
. "$HERE/lab-env.sh"
in="${1:?usage: functions.sh <functions.json> <functions.jsonl>}"
out="${2:?usage: functions.sh <functions.json> <functions.jsonl>}"
[ -f "$in" ] || { echo "functions.sh: no file $in" >&2; exit 2; }
ADB="$LAB_ADB -s $LAB_SERIAL"
[ "$($ADB get-state 2>/dev/null)" = "device" ] || { echo "functions.sh: $LAB_SERIAL is not online" >&2; exit 1; }

$ADB shell pm clear "$LAB_PACKAGE" >/dev/null
$ADB shell run-as "$LAB_PACKAGE" mkdir -p files
$ADB shell "run-as $LAB_PACKAGE sh -c 'cat > files/functions.json'" < "$in"
result="$($ADB shell am instrument -w -e mode functions "$LAB_RUNNER" | tr -d '\r')"
printf '%s\n' "$result"
case "$result" in
  *"rivqen.ok=true"*) ;;
  *) echo "functions.sh: the function run failed" >&2; exit 1 ;;
esac
$ADB exec-out run-as "$LAB_PACKAGE" cat files/functions.jsonl > "$out.tmp"
mv "$out.tmp" "$out"
echo "functions.sh: $(wc -l < "$out" | tr -d ' ') results -> $out"

# Meta file next to the results (CONTRACT section 8, run.json functions.android): platform build,
# upstream SHA, case count and the package-access members (non_public_calls). No device identifier:
# only the API level, the release, the ABI, the image name and the WebView version.
prop() { $ADB shell getprop "$1" | tr -d '\r'; }
webview="$($ADB shell dumpsys package com.android.webview | sed -n 's/^ *versionName=//p' | head -1 | tr -d '\r')"
API="$(prop ro.build.version.sdk)" RELEASE="$(prop ro.build.version.release)" ABI="$(prop ro.product.cpu.abi)" \
  WEBVIEW="$webview" IMAGE="$LAB_IMAGE" CASES="$in" ACCESS="$HERE/package-access.json" OUT="$out.meta.json" \
  node "$HERE/tools/functions-meta.cjs"
echo "functions.sh: meta -> $out.meta.json"
