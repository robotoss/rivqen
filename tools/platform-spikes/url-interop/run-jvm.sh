#!/bin/sh
# SPDX-License-Identifier: Apache-2.0
# SPDX-FileCopyrightText: The Rivqen Authors
#
# URL interop, JVM stacks: OkHttp HttpUrl and java.net.URI (WP-04 S2 T-07). No network.
# Uses the jars that the Android spike resolved in the lab Gradle cache (OkHttp 5.5.1 aar, Okio, Kotlin
# stdlib), checks their sha256 against the recorded values, compiles with JDK 21 and runs.
set -eu
here="$(cd "$(dirname "$0")" && pwd)"
. "$here/../lab-env.sh"
work="$LAB_HOME/out/url-interop"
mkdir -p "$work/classes" "$work/aar"
C="$GRADLE_USER_HOME/caches/modules-2/files-2.1"
aar="$C/com.squareup.okhttp3/okhttp-android/5.5.1/8901fb7a84be05aa777639120071a89bb16ab813/okhttp.aar"
okio="$C/com.squareup.okio/okio-jvm/3.18.1/a3a8128bb3a0157d23ecc18c17e8330d9c0ac96c/okio-jvm-3.18.1.jar"
kstd="$C/org.jetbrains.kotlin/kotlin-stdlib/2.4.20/94c1451890d164aed65d43b3db0b7ecef7c183e7/kotlin-stdlib-2.4.20.jar"
for f in "$aar" "$okio" "$kstd"; do
  want="$(grep -F "$(basename "$f")" "$here/jars.sha256" | cut -d' ' -f1)"
  have="$(shasum -a 256 "$f" | cut -d' ' -f1)"
  [ "$want" = "$have" ] || { echo "run-jvm.sh: sha256 mismatch for $f" >&2; exit 1; }
done
unzip -o -q "$aar" classes.jar -d "$work/aar"
CP="$work/aar/classes.jar:$okio:$kstd"
"$JAVA_HOME/bin/javac" -Xlint:all -d "$work/classes" -cp "$CP" "$here"/java/Common.java "$here"/java/JvmMain.java
node "$here/cli.mjs" inputs "$work/urls.in"
for s in okhttp uri; do
  "$JAVA_HOME/bin/java" -cp "$work/classes:$CP" JvmMain "$work/urls.in" "$s" > "$work/raw-$s.txt"
  node "$here/cli.mjs" assemble "$work/raw-$s.txt"
done
