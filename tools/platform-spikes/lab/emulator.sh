#!/usr/bin/env sh
# SPDX-License-Identifier: Apache-2.0
# SPDX-FileCopyrightText: The Rivqen Authors
#
# Start, stop and use the lab emulator (one emulator at a time; console port LAB_PORT, default 5602).
# Every adb call names the lab serial, so no call reaches emulator-5554 or another device.
# Usage: sh tools/platform-spikes/lab/emulator.sh [--dry-run] <command>
#   start 37|29            cold boot of rivqen-p2-api37 or rivqen-p2-api29 without a window; waits
#                          for the boot (max 600 s); refuses when another process holds the port
#   stop                   stop the lab emulator that this script started
#   status                 running or not, with the AVD and the serial
#   reverse                adb reverse of the spike host ports (CONTRACT.md section 1)
#   record start|stop <f>  screenrecord on the emulator; stop pulls the file into $LAB_OUT
#                          (a relative name goes into $LAB_OUT; one recording is at most 180 s)
#   webview                print the WebView provider package and version
set -eu
HERE="$(cd "$(dirname "$0")" && pwd)"
. "$HERE/../lab-env.sh"
. "$HERE/common.sh"
ADB="$LAB_ADB -s $LAB_SERIAL"
PIDFILE="$LAB_HOME/logs/emulator.pid"
AVDFILE="$LAB_HOME/logs/emulator.avd"
RECFILE="$LAB_HOME/logs/emulator.rec"
REMOTE_REC=/sdcard/rivqen-p2-rec.mp4

# The pid file is on the lab volume and survives a reboot: a pid counts only while it still is an
# emulator process (pid reuse must never make stop kill another process).
running() {
  [ -f "$PIDFILE" ] || return 1
  pid="$(cat "$PIDFILE")"
  case "$(ps -p "$pid" -o comm= 2>/dev/null)" in
    *emulator* | *qemu-system*) return 0 ;;
    *) return 1 ;;
  esac
}

# Print the processes that listen on the console port or the adb port of the emulator.
port_holders() {
  for p in "$LAB_PORT" $((LAB_PORT + 1)); do
    lsof -nP -iTCP:"$p" -sTCP:LISTEN 2>/dev/null | awk 'NR > 1 { print $1 " pid " $2 " port '"$p"'" }'
  done
}

need_running() {
  running || { echo "emulator.sh: the lab emulator is not running" >&2; exit 1; }
}

cmd="${1:-}"
[ $# -eq 0 ] || shift
case "$cmd" in
  start)
    case "${1:-}" in
      37) avd="$LAB_AVD_37" ;;
      29) avd="$LAB_AVD_29" ;;
      *) echo "usage: emulator.sh start 37|29" >&2; exit 2 ;;
    esac
    if running; then
      echo "emulator.sh: already running (pid $(cat "$PIDFILE"), $(cat "$AVDFILE" 2>/dev/null), $LAB_SERIAL)"
      [ "$(cat "$AVDFILE" 2>/dev/null)" = "$avd" ] || { echo "emulator.sh: another AVD runs; stop it first" >&2; exit 1; }
      exit 0
    fi
    holders="$(port_holders)"
    if [ -n "$holders" ]; then
      echo "emulator.sh: port $LAB_PORT or $((LAB_PORT + 1)) is in use by another process; not starting:" >&2
      echo "$holders" >&2
      exit 1
    fi
    if [ "$LAB_DRY_RUN" = 1 ]; then
      echo "dry-run: $LAB_EMULATOR -avd $avd -port $LAB_PORT -no-window -no-audio -no-boot-anim -no-snapshot -gpu swiftshader_indirect"
      exit 0
    fi
    [ -f "$ANDROID_AVD_HOME/$avd.ini" ] || { echo "emulator.sh: no AVD $avd; run env.sh first" >&2; exit 1; }
    mkdir -p "$LAB_HOME/logs" "$LAB_HOME/tmp"
    t0=$(date +%s)
    nohup "$LAB_EMULATOR" -avd "$avd" -port "$LAB_PORT" -no-window -no-audio -no-boot-anim \
      -no-snapshot -gpu swiftshader_indirect -no-metrics > "$LAB_HOME/logs/emulator.log" 2>&1 &
    echo $! > "$PIDFILE"
    echo "$avd" > "$AVDFILE"
    "$LAB_ADB" start-server >/dev/null 2>&1
    i=0
    until [ "$($ADB shell getprop sys.boot_completed 2>/dev/null | tr -d '\r')" = "1" ]; do
      i=$((i + 1))
      # A cold boot of the API 29 image can take more than 300 s on a loaded Mac.
      if [ $i -gt 300 ] || ! running; then
        echo "emulator.sh: no boot in 600 s; see $LAB_HOME/logs/emulator.log" >&2
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
    echo "emulator.sh: $avd on $LAB_SERIAL booted in $((t1 - t0)) s (cold boot)"
    echo "emulator.sh: Android $($ADB shell getprop ro.build.version.release | tr -d '\r'), API $($ADB shell getprop ro.build.version.sdk | tr -d '\r'), $($ADB shell getprop ro.product.cpu.abi | tr -d '\r')"
    ;;
  stop)
    if ! running; then echo "emulator.sh: not running"; rm -f "$PIDFILE" "$AVDFILE" "$RECFILE"; exit 0; fi
    pid="$(cat "$PIDFILE")"
    if [ "$LAB_DRY_RUN" = 1 ]; then echo "dry-run: adb -s $LAB_SERIAL emu kill (pid $pid)"; exit 0; fi
    $ADB emu kill >/dev/null 2>&1 || kill "$pid"
    i=0
    while kill -0 "$pid" 2>/dev/null; do
      i=$((i + 1))
      if [ $i -gt 30 ]; then kill "$pid" 2>/dev/null || true; fi
      if [ $i -gt 60 ]; then echo "emulator.sh: pid $pid does not stop" >&2; exit 1; fi
      sleep 1
    done
    rm -f "$PIDFILE" "$AVDFILE" "$RECFILE"
    echo "emulator.sh: stopped"
    ;;
  status)
    if running; then
      echo "running (pid $(cat "$PIDFILE"), $(cat "$AVDFILE" 2>/dev/null), $LAB_SERIAL)"
    else
      echo "not running"
    fi
    ;;
  reverse)
    need_running
    run $ADB reverse tcp:"$LAB_HOST_PORT" tcp:"$LAB_HOST_PORT"
    ;;
  record)
    sub="${1:-}"
    file="${2:-}"
    [ -n "$file" ] || { echo "usage: emulator.sh record start|stop <file>" >&2; exit 2; }
    case "$file" in
      */*) echo "emulator.sh: give a file name without a folder (it goes into $LAB_OUT)" >&2; exit 2 ;;
    esac
    case "$sub" in
      start)
        need_running
        if [ "$LAB_DRY_RUN" = 1 ]; then echo "dry-run: adb -s $LAB_SERIAL shell screenrecord $REMOTE_REC"; exit 0; fi
        [ ! -f "$RECFILE" ] || { echo "emulator.sh: a recording is already running" >&2; exit 1; }
        $ADB shell rm -f "$REMOTE_REC"
        nohup $ADB shell screenrecord "$REMOTE_REC" > "$LAB_HOME/logs/screenrecord.log" 2>&1 &
        echo $! > "$RECFILE"
        echo "emulator.sh: recording to $LAB_OUT/$file after stop"
        ;;
      stop)
        need_running
        if [ "$LAB_DRY_RUN" = 1 ]; then echo "dry-run: stop screenrecord, adb -s $LAB_SERIAL pull $REMOTE_REC $LAB_OUT/$file"; exit 0; fi
        [ -f "$RECFILE" ] || { echo "emulator.sh: no recording is running" >&2; exit 1; }
        $ADB shell pkill -INT screenrecord || true
        i=0
        while kill -0 "$(cat "$RECFILE")" 2>/dev/null; do
          i=$((i + 1))
          [ $i -le 20 ] || { kill "$(cat "$RECFILE")" 2>/dev/null || true; break; }
          sleep 1
        done
        rm -f "$RECFILE"
        mkdir -p "$LAB_OUT"
        $ADB pull "$REMOTE_REC" "$LAB_OUT/$file"
        $ADB shell rm -f "$REMOTE_REC"
        echo "emulator.sh: saved $LAB_OUT/$file"
        ;;
      *) echo "usage: emulator.sh record start|stop <file>" >&2; exit 2 ;;
    esac
    ;;
  webview)
    need_running
    # The current WebView provider package and its version.
    $ADB shell dumpsys webviewupdate | grep -E "Current WebView package|Current WebView|^ *Valid package|versionName" || true
    ;;
  *)
    echo "usage: emulator.sh [--dry-run] start 37|29 | stop | status | reverse | record start|stop <file> | webview" >&2
    exit 2
    ;;
esac
