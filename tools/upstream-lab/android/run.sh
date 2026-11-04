#!/usr/bin/env sh
# SPDX-License-Identifier: Apache-2.0
# SPDX-FileCopyrightText: The Rivqen Authors
#
# One harness run (CONTRACT section 4): clear the app state, then start the instrumentation with
# only the run id and the control URL. The harness gets the scenario and the steps from the
# conductor, which must listen on 127.0.0.1:18091 (the emulator reaches it as 10.0.2.2:18091).
# Usage: sh tools/upstream-lab/android/run.sh <run-id> [control-url]
# Exit code: 0 when the instrumentation reports rivqen.ok=true, else 1.
set -eu
HERE="$(cd "$(dirname "$0")" && pwd)"
. "$HERE/lab-env.sh"
run="${1:?usage: run.sh <run-id> [control-url]}"
control="${2:-http://10.0.2.2:18091}"
case "$run" in
  *[!A-Za-z0-9._:/-]*) echo "run.sh: run id must be [A-Za-z0-9._:/-]" >&2; exit 2 ;;
esac
case "$control" in
  http://10.0.2.2:[0-9]*) ;;
  *) echo "run.sh: control URL must be http://10.0.2.2:<port>" >&2; exit 2 ;;
esac
ADB="$LAB_ADB -s $LAB_SERIAL"
[ "$($ADB get-state 2>/dev/null)" = "device" ] || { echo "run.sh: $LAB_SERIAL is not online" >&2; exit 1; }

$ADB shell pm clear "$LAB_PACKAGE" >/dev/null
out="$($ADB shell am instrument -w -e run "$run" -e control "$control" "$LAB_RUNNER" | tr -d '\r')"
printf '%s\n' "$out"
case "$out" in
  *"rivqen.ok=true"*) exit 0 ;;
  *) exit 1 ;;
esac
