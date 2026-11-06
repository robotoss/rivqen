#!/usr/bin/env sh
# SPDX-License-Identifier: Apache-2.0
# SPDX-FileCopyrightText: The Rivqen Authors
#
# Plain JVM checks of the harness code that needs no Android SDK, no emulator and no UPSTREAM
# (WP-01 S2): TallyStream, the count, hash and once-only core of RecordingStream.
# Usage: sh tools/upstream-lab/android/check.sh
# Needs only a JDK (JAVA_HOME from lab-env.sh). Output: build/check (git-ignored).
set -eu
HERE="$(cd "$(dirname "$0")" && pwd)"
# Read JAVA_HOME from lab-env.sh in a subshell; the other lab variables are not used here.
JAVA_HOME="$( . "$HERE/lab-env.sh" && printf '%s' "$JAVA_HOME")"
[ -x "$JAVA_HOME/bin/javac" ] || { echo "check.sh: no javac in JAVA_HOME=$JAVA_HOME" >&2; exit 1; }
OUT="$HERE/build/check"
rm -rf "$OUT"
mkdir -p "$OUT"
# Java 8 level, as the app (compileOptions in app/build.gradle). All lint warnings are errors.
"$JAVA_HOME/bin/javac" --release 8 -Xlint:all -Xlint:-options -Werror -encoding UTF-8 -d "$OUT" \
  "$HERE/app/src/main/java/org/rivqen/lab/harness/TallyStream.java" \
  "$HERE/check/TallyStreamCheck.java"
"$JAVA_HOME/bin/java" -cp "$OUT" org.rivqen.lab.harness.TallyStreamCheck
