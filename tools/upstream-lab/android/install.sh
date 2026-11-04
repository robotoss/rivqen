#!/usr/bin/env sh
# SPDX-License-Identifier: Apache-2.0
# SPDX-FileCopyrightText: The Rivqen Authors
#
# Install the harness APK (build.sh output) on the lab emulator.
# Usage: sh tools/upstream-lab/android/install.sh
set -eu
HERE="$(cd "$(dirname "$0")" && pwd)"
. "$HERE/lab-env.sh"
apk="$HERE/app/build/outputs/apk/debug/app-debug.apk"
[ -f "$apk" ] || { echo "install.sh: no APK; run build.sh first" >&2; exit 1; }
[ "$("$LAB_ADB" -s "$LAB_SERIAL" get-state 2>/dev/null)" = "device" ] || { echo "install.sh: $LAB_SERIAL is not online" >&2; exit 1; }
"$LAB_ADB" -s "$LAB_SERIAL" install -r -t "$apk"
echo "install.sh: $LAB_PACKAGE installed on $LAB_SERIAL"
