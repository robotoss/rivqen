#!/usr/bin/env sh
# SPDX-License-Identifier: Apache-2.0
# SPDX-FileCopyrightText: The Rivqen Authors
#
# Remove the lab devices (DL-138): stop the lab emulator, delete the lab AVDs and delete the lab
# simulators. It lists what it would remove first; without --yes it removes nothing. It never
# deletes LAB_HOME: the human deletes top-level folders. At the end it prints what the human may delete.
# Usage: sh tools/platform-spikes/lab/clean.sh [--dry-run] [--yes]
set -eu
HERE="$(cd "$(dirname "$0")" && pwd)"
. "$HERE/../lab-env.sh"
. "$HERE/common.sh"
require_lab_root
yes=0
[ "${1:-}" != "--yes" ] || yes=1
[ "$LAB_DRY_RUN" = 0 ] || yes=0

echo "== disk before"; df -h / "$(dirname "$LAB_HOME")"
echo "== would remove"
avds=""
for a in "$LAB_AVD_37" "$LAB_AVD_29"; do
  if [ -f "$ANDROID_AVD_HOME/$a.ini" ] || [ -d "$ANDROID_AVD_HOME/$a.avd" ]; then
    avds="$avds $a"
    echo "  AVD $a ($(du -sh "$ANDROID_AVD_HOME/$a.avd" 2>/dev/null | cut -f1))"
  fi
done
sims="$(sh "$HERE/sim.sh" list 2>/dev/null || true)"
[ -z "$sims" ] || echo "$sims" | sed 's/^/  simulator /'
[ -n "$avds$sims" ] || echo "  nothing"

if [ "$yes" = 0 ]; then
  echo "clean.sh: nothing removed (add --yes to remove the list above)"
else
  if [ -x "$LAB_ADB" ]; then sh "$HERE/emulator.sh" stop || true; fi
  for a in $avds; do
    rm -rf "$ANDROID_AVD_HOME/$a.avd" "$ANDROID_AVD_HOME/$a.ini"
    echo "clean.sh: removed AVD $a"
  done
  [ -z "$sims" ] || LAB_SIM_ALL_TAGS=1 sh "$HERE/sim.sh" delete all
fi

echo "== the human may delete (clean.sh never does)"
echo "  $LAB_HOME   (SDK, images, Gradle, cargo target, derived data, TLS files, evidence output)"
du -sh "$LAB_HOME" 2>/dev/null | sed 's/^/  size: /' || true
echo "  /tmp/android-$(id -un)/   (crash-report folder of the emulator, system volume)"
echo "== disk after"; df -h / "$(dirname "$LAB_HOME")"
