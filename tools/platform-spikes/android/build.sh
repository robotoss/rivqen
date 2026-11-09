#!/usr/bin/env sh
# SPDX-License-Identifier: Apache-2.0
# SPDX-FileCopyrightText: The Rivqen Authors
#
# Builds the debug APK of the prototype with the lab environment (DL-138): Gradle home, Android SDK,
# build output and the Gradle project cache in LAB_HOME, nothing in the repository.
# Usage: sh tools/platform-spikes/android/build.sh [gradle task ...]   (default: :app:assembleDebug)
# Prints the APK path on the last line.
set -eu
HERE="$(cd "$(dirname "$0")" && pwd)"
. "$HERE/../lab-env.sh"
mkdir -p "$LAB_HOME/tmp" "$LAB_HOME/build/platform-spikes-android"
[ -f "$LAB_TLS/ca.pem" ] || { echo "build.sh: no $LAB_TLS/ca.pem; start the spike host once (it makes the TLS files)" >&2; exit 1; }
[ -x "$JAVA_HOME/bin/java" ] || { echo "build.sh: no JDK at JAVA_HOME=$JAVA_HOME" >&2; exit 1; }
if [ $# -eq 0 ]; then set -- :app:assembleDebug; fi
cd "$HERE"
./gradlew --quiet --project-cache-dir "$LAB_HOME/build/platform-spikes-android/.gradle" "$@"
echo "$LAB_HOME/build/platform-spikes-android/app/outputs/apk/debug/app-debug.apk"
