#!/usr/bin/env sh
# SPDX-License-Identifier: Apache-2.0
# SPDX-FileCopyrightText: The Rivqen Authors
#
# Start, stop or show the lab emulator (one emulator at a time; console port LAB_PORT, default 5600).
# Usage: sh tools/upstream-lab/android/emulator.sh start|stop|status
# start: cold boot without a window and without snapshots; it waits for the boot (max 300 s),
#        wakes the screen and prints the boot time. stop: kills the lab emulator only.
set -eu
HERE="$(cd "$(dirname "$0")" && pwd)"
. "$HERE/lab-env.sh"
ADB="$LAB_ADB -s $LAB_SERIAL"
PIDFILE="$LAB_HOME/logs/emulator.pid"

running() {
  [ -f "$PIDFILE" ] && kill -0 "$(cat "$PIDFILE")" 2>/dev/null
}

case "${1:-}" in
  start)
    if running; then echo "emulator.sh: already running (pid $(cat "$PIDFILE"), $LAB_SERIAL)"; exit 0; fi
    [ -f "$ANDROID_AVD_HOME/$LAB_AVD.ini" ] || { echo "emulator.sh: no AVD; run env.sh first" >&2; exit 1; }
    mkdir -p "$LAB_HOME/logs" "$TMPDIR"
    t0=$(date +%s)
    nohup "$LAB_EMULATOR" -avd "$LAB_AVD" -port "$LAB_PORT" -no-window -no-audio -no-boot-anim \
      -no-snapshot -gpu swiftshader_indirect -no-metrics > "$LAB_HOME/logs/emulator.log" 2>&1 &
    echo $! > "$PIDFILE"
    "$LAB_ADB" start-server >/dev/null 2>&1
    i=0
    until [ "$($ADB shell getprop sys.boot_completed 2>/dev/null | tr -d '\r')" = "1" ]; do
      i=$((i + 1))
      if [ $i -gt 150 ] || ! running; then
        echo "emulator.sh: no boot in 300 s; see $LAB_HOME/logs/emulator.log" >&2
        tail -20 "$LAB_HOME/logs/emulator.log" >&2
        exit 1
      fi
      sleep 2
    done
    t1=$(date +%s)
    $ADB shell input keyevent KEYCODE_WAKEUP
    $ADB shell wm dismiss-keyguard
    $ADB shell settings put global window_animation_scale 0
    $ADB shell settings put global transition_animation_scale 0
    $ADB shell settings put global animator_duration_scale 0
    $ADB shell svc power stayon true
    echo "emulator.sh: $LAB_SERIAL booted in $((t1 - t0)) s (cold boot)"
    echo "emulator.sh: Android $($ADB shell getprop ro.build.version.release | tr -d '\r'), API $($ADB shell getprop ro.build.version.sdk | tr -d '\r'), $($ADB shell getprop ro.product.cpu.abi | tr -d '\r')"
    ;;
  stop)
    if ! running; then echo "emulator.sh: not running"; rm -f "$PIDFILE"; exit 0; fi
    pid="$(cat "$PIDFILE")"
    $ADB emu kill >/dev/null 2>&1 || kill "$pid"
    i=0
    while kill -0 "$pid" 2>/dev/null; do
      i=$((i + 1))
      if [ $i -gt 30 ]; then kill "$pid" 2>/dev/null || true; fi
      if [ $i -gt 60 ]; then echo "emulator.sh: pid $pid does not stop" >&2; exit 1; fi
      sleep 1
    done
    rm -f "$PIDFILE"
    echo "emulator.sh: stopped"
    ;;
  status)
    if running; then echo "running (pid $(cat "$PIDFILE"), $LAB_SERIAL)"; else echo "not running"; fi
    ;;
  *)
    echo "usage: emulator.sh start|stop|status" >&2; exit 2 ;;
esac
