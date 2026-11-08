#!/usr/bin/env sh
# SPDX-License-Identifier: Apache-2.0
# SPDX-FileCopyrightText: The Rivqen Authors
#
# Plain JVM check of the prototype logic that needs no Android SDK and no emulator (DL-139):
# BridgePipe (the bridge stream) and InterceptRule (A-04 rule, response head, log values).
# Usage: sh tools/platform-spikes/android/check.sh
# Needs only a JDK: JAVA_HOME, else the JDK of lab-env.sh. Output: a temporary folder, removed at exit.
set -eu
HERE="$(cd "$(dirname "$0")" && pwd)"
if [ -z "${JAVA_HOME:-}" ] || [ ! -x "$JAVA_HOME/bin/javac" ]; then
  # Only JAVA_HOME is read from lab-env.sh; the other lab variables are not used here.
  JAVA_HOME="$(. "$HERE/../lab-env.sh" && printf '%s' "$JAVA_HOME")"
fi
[ -x "$JAVA_HOME/bin/javac" ] || { echo "check.sh: no javac in JAVA_HOME=$JAVA_HOME" >&2; exit 1; }
OUT="$(mktemp -d "${TMPDIR:-/tmp}/spike-android-check.XXXXXX")"
trap 'rm -rf "$OUT"' EXIT
SRC="$HERE/app/src/main/java/dev/rivqen/spike"
# Java 11 level, as the app (compileOptions in app/build.gradle.kts). All lint warnings are errors.
"$JAVA_HOME/bin/javac" --release 11 -Xlint:all -Werror -encoding UTF-8 -d "$OUT" \
  "$SRC/BridgePipe.java" "$SRC/InterceptRule.java" "$HERE/check/SpikeLogicCheck.java"
"$JAVA_HOME/bin/java" -cp "$OUT" dev.rivqen.spike.SpikeLogicCheck
