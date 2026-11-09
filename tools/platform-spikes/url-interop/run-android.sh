#!/bin/sh
# SPDX-License-Identifier: Apache-2.0
# SPDX-FileCopyrightText: The Rivqen Authors
#
# URL interop, android.net.Uri (WP-04 S2 T-07, CONTRACT section 11). No network, no APK: the dex runs
# with app_process on the lab emulator. The emulator slot belongs to one lane at a time.
#   sh run-android.sh build              compile and dex (no emulator needed)
#   sh run-android.sh run                run on the booted lab emulator (emulator-5602 only)
# Start and stop the emulator with: sh tools/platform-spikes/lab/emulator.sh start 37 / stop
set -eu
here="$(cd "$(dirname "$0")" && pwd)"
. "$here/../lab-env.sh"
work="$LAB_HOME/out/url-interop"
mkdir -p "$work/aclasses"
case "$1" in
  build)
    android_jar="$LAB_SDK/platforms/android-37.0/android.jar"
    "$JAVA_HOME/bin/javac" -Xlint:all --release 17 -d "$work/aclasses" -cp "$android_jar" "$here/java/Common.java" "$here/java/AndroidMain.java"
    "$LAB_SDK/build-tools/37.0.0/d8" --min-api 26 --lib "$android_jar" --output "$work" "$work"/aclasses/*.class
    node "$here/cli.mjs" inputs "$work/urls.in"
    ;;
  run)
    adb="$LAB_ADB -s $LAB_SERIAL"
    case "$LAB_SERIAL" in emulator-5602) ;; *) echo "run-android.sh: wrong serial $LAB_SERIAL" >&2; exit 1 ;; esac
    $adb push "$work/classes.dex" /data/local/tmp/url-interop.dex
    $adb push "$work/urls.in" /data/local/tmp/url-interop.in
    $adb shell "CLASSPATH=/data/local/tmp/url-interop.dex app_process /system/bin AndroidMain /data/local/tmp/url-interop.in" | tr -d '\r' | awk -F'\t' '$1=="H"||$1=="R"' > "$work/raw-android.txt"
    $adb shell rm -f /data/local/tmp/url-interop.dex /data/local/tmp/url-interop.in
    node "$here/cli.mjs" assemble "$work/raw-android.txt"
    ;;
  *) echo "usage: run-android.sh build | run" >&2; exit 2 ;;
esac
