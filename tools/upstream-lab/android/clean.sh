#!/usr/bin/env sh
# SPDX-License-Identifier: Apache-2.0
# SPDX-FileCopyrightText: The Rivqen Authors
#
# Delete the Android lab (DL-102): stop the lab emulator and the Gradle daemons of the lab, then
# delete LAB_HOME (lab SDK root, AVD, GRADLE_USER_HOME) and the build output of the harness.
# Usage: sh tools/upstream-lab/android/clean.sh
set -eu
HERE="$(cd "$(dirname "$0")" && pwd)"
. "$HERE/lab-env.sh"

case "$LAB_HOME" in
  /Volumes/*/rivqen-lab) ;;
  *) echo "clean.sh: refusing to delete $LAB_HOME (expected /Volumes/<name>/rivqen-lab)" >&2; exit 2 ;;
esac

echo "== disk before"; df -h / "$(dirname "$LAB_HOME")"
if [ -x "$LAB_ADB" ]; then
  sh "$HERE/emulator.sh" stop || true
fi
# The emulator keeps a crash-report folder in /tmp/android-<user>/emu-crash-<version>.db (system
# volume; it does not follow TMPDIR). Delete the folder of the lab emulator version only when the
# main SDK has another version, so that the folder of another emulator stays.
lab_ver="$(sed -n 's/^Pkg.Revision=//p' "$LAB_SDK/emulator/source.properties" 2>/dev/null || true)"
main_ver="$(sed -n 's/^Pkg.Revision=//p' "$MAIN_SDK/emulator/source.properties" 2>/dev/null || true)"
if [ -n "$lab_ver" ] && [ "$lab_ver" != "$main_ver" ]; then
  rm -rf "/tmp/android-$(id -un)/emu-crash-$lab_ver.db"
fi
if [ -d "$GRADLE_USER_HOME/wrapper/dists" ]; then
  (cd "$HERE" && ./gradlew --stop >/dev/null 2>&1) || true
fi
rm -rf "$LAB_HOME"
rm -rf "$HERE/app/build" "$HERE/build" "$HERE/.gradle" "$HERE/local.properties"
echo "clean.sh: deleted $LAB_HOME and the build output"
echo "== disk after"; df -h / "$(dirname "$LAB_HOME")"
