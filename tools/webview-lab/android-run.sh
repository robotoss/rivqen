#!/bin/sh
# SPDX-License-Identifier: Apache-2.0
# SPDX-FileCopyrightText: The Rivqen Authors
#
# Runs the WebView lab on the Android emulator (CONTRACT section 4) and compares the results with
# parse5 (CONTRACT section 5). See README.md, part "Android".
#
#   sh tools/webview-lab/android-run.sh
#
# Environment (all optional):
#   PORT            lab server port on the host (default 18092)
#   AVD             emulator to start when no emulator runs (default Pixel_10_Pro_XL)
#   EMU_PORT        console port of an emulator that this script starts (default 5584)
#   ANDROID_SERIAL  use this device; do not start an emulator
#   SETTLE_MS       wait after onPageFinished, 0 to 2000 (default 1000)
#   CASES           comma-separated case ids: run only these (a partial run; for development)
#   RUN_TIMEOUT     seconds for the app run (default 1200)
#   ANDROID_HOME    Android SDK (default ~/Library/Android/sdk)
#   JAVA_HOME       JDK 21 for Gradle
set -eu

LAB_DIR=$(CDPATH='' cd -- "$(dirname -- "$0")" && pwd)
APP_DIR=$LAB_DIR/app/WebViewLab
RESULTS=$LAB_DIR/results/android
PORT=${PORT:-18092}
AVD=${AVD:-Pixel_10_Pro_XL}
EMU_PORT=${EMU_PORT:-5584}
SETTLE_MS=${SETTLE_MS:-1000}
RUN_TIMEOUT=${RUN_TIMEOUT:-1200}
CASES=${CASES:-}
ANDROID_HOME=${ANDROID_HOME:-${ANDROID_SDK_ROOT:-$HOME/Library/Android/sdk}}
export ANDROID_HOME
DEFAULT_JDK=$HOME/.gradle/jdks/azul_systems__inc_-21-aarch64-os_x.2/zulu21.50.19-ca-jdk21.0.11-macosx_aarch64/Contents/Home
if [ -z "${JAVA_HOME:-}" ] && [ -d "$DEFAULT_JDK" ]; then
    JAVA_HOME=$DEFAULT_JDK
    export JAVA_HOME
fi
ADB=$ANDROID_HOME/platform-tools/adb
EMULATOR=$ANDROID_HOME/emulator/emulator
PKG=dev.rivqen.webviewlab
ACTIVITY=$PKG/.MainActivity
DEVICE_TMP=/data/local/tmp/rivqen-webview-lab

say() { printf 'android-run: %s\n' "$*"; }
fail() {
    printf 'android-run: error: %s\n' "$*" >&2
    exit 1
}

WORK=$(mktemp -d "${TMPDIR:-/tmp}/webview-lab-android.XXXXXX")
SERVER_PID=
STARTED_SERIAL=
EMU_PID=

cleanup() {
    if [ -n "$SERVER_PID" ]; then kill "$SERVER_PID" 2>/dev/null || true; fi
    if [ -n "$STARTED_SERIAL" ]; then
        say "stopping the emulator that this script started ($STARTED_SERIAL)"
        "$ADB" -s "$STARTED_SERIAL" emu kill >/dev/null 2>&1 || true
        if [ -n "$EMU_PID" ]; then wait "$EMU_PID" 2>/dev/null || true; fi
    fi
    rm -rf "$WORK"
}
trap cleanup EXIT
trap 'exit 130' INT TERM

case $PORT in '' | *[!0-9]*) fail "PORT must be a number" ;; esac
case $SETTLE_MS in '' | *[!0-9]*) fail "SETTLE_MS must be a number" ;; esac
[ "$SETTLE_MS" -le 2000 ] || fail "SETTLE_MS must be 0 to 2000 (CONTRACT section 4)"
command -v node >/dev/null || fail "node is not installed"
[ -x "$ADB" ] || fail "adb not found at $ADB (set ANDROID_HOME)"

# 1. The cases must be current.
node "$LAB_DIR/host/build-cases.mjs" --check || fail "cases/ is not current: run node host/build-cases.mjs"

# 2. The lab server on 127.0.0.1. The emulator reaches it at 10.0.2.2.
node "$LAB_DIR/host/server.mjs" --port "$PORT" >"$WORK/server.log" 2>&1 &
SERVER_PID=$!
i=0
until curl -fsS "http://127.0.0.1:$PORT/cases.json" >/dev/null 2>&1; do
    i=$((i + 1))
    if [ "$i" -gt 50 ] || ! kill -0 "$SERVER_PID" 2>/dev/null; then
        cat "$WORK/server.log" >&2
        fail "the lab server did not start on port $PORT"
    fi
    sleep 0.2
done
# A server of another run on the same port would answer too; this run needs its own.
kill -0 "$SERVER_PID" 2>/dev/null || fail "port $PORT is in use by another process"
say "lab server on 127.0.0.1:$PORT"

# 3. The device: ANDROID_SERIAL, a running emulator, or a new emulator.
SERIAL=${ANDROID_SERIAL:-}
if [ -z "$SERIAL" ]; then
    SERIAL=$("$ADB" devices | awk '$1 ~ /^emulator-/ && $2 == "device" { print $1; exit }')
fi
if [ -z "$SERIAL" ]; then
    [ -x "$EMULATOR" ] || fail "emulator not found at $EMULATOR"
    say "starting emulator $AVD on port $EMU_PORT"
    "$EMULATOR" -avd "$AVD" -port "$EMU_PORT" -no-snapshot-save -no-window -no-audio -no-boot-anim \
        >"$WORK/emulator.log" 2>&1 &
    EMU_PID=$!
    SERIAL=emulator-$EMU_PORT
    STARTED_SERIAL=$SERIAL
    i=0
    until [ "$("$ADB" -s "$SERIAL" shell getprop sys.boot_completed 2>/dev/null | tr -d '\r')" = "1" ]; do
        i=$((i + 1))
        if [ "$i" -gt 150 ] || ! kill -0 "$EMU_PID" 2>/dev/null; then
            tail -20 "$WORK/emulator.log" >&2
            fail "the emulator did not boot in 300 s"
        fi
        sleep 2
    done
fi
export ANDROID_SERIAL=$SERIAL
say "device $SERIAL"
adb_() { "$ADB" -s "$SERIAL" "$@"; }

# 4. Build and install the app.
(cd "$APP_DIR" && ./gradlew --quiet :androidApp:assembleDebug) || fail "the Android build failed"
APK=$APP_DIR/androidApp/build/outputs/apk/debug/androidApp-debug.apk
adb_ install -r -t "$APK" >/dev/null || fail "adb install failed"
# Android 17 treats 10.0.2.2 as a local network address: the app needs this runtime permission.
SDK_INT=$(adb_ shell getprop ro.build.version.sdk | tr -d '\r')
if [ "$SDK_INT" -ge 37 ]; then
    adb_ shell pm grant "$PKG" android.permission.ACCESS_LOCAL_NETWORK || fail "cannot grant ACCESS_LOCAL_NETWORK"
fi

# 5. The input: the case files and the walker, copied into the app (run-as works for a debug app).
mkdir -p "$WORK/in"
cp "$LAB_DIR"/cases/*.json "$LAB_DIR/host/serialize.js" "$WORK/in/"
if [ -n "$CASES" ]; then
    node -e '
        const ids = process.argv[1].split(",").filter(Boolean);
        for (const id of ids) if (!/^[A-Z0-9-]{1,64}$/.test(id)) { console.error("bad id " + id); process.exit(2); }
        require("node:fs").writeFileSync(process.argv[2], JSON.stringify(ids) + "\n");
    ' "$CASES" "$WORK/in/index.json"
    say "partial run: $CASES"
fi
adb_ shell rm -rf "$DEVICE_TMP"
adb_ shell mkdir -p "$DEVICE_TMP"
adb_ push "$WORK/in/." "$DEVICE_TMP/" >/dev/null
adb_ shell am force-stop "$PKG"
adb_ shell run-as "$PKG" rm -rf files/in files/out
adb_ shell run-as "$PKG" mkdir -p files/in
adb_ shell "run-as $PKG sh -c 'cp $DEVICE_TMP/* files/in/'"
adb_ shell rm -rf "$DEVICE_TMP"

# Wait until the app writes files/out/$1, at most $2 seconds.
wait_for() {
    i=0
    while ! adb_ shell run-as "$PKG" test -f "files/out/$1" 2>/dev/null; do
        i=$((i + 1))
        [ "$i" -le "$2" ] || fail "the app wrote no files/out/$1 in $2 s"
        sleep 1
    done
}

# 6. Self-test of the app input and output code.
adb_ shell am start -W -n "$ACTIVITY" --ez selftest true >/dev/null
wait_for selftest.json 60
adb_ exec-out run-as "$PKG" cat files/out/selftest.json >"$WORK/selftest.json"
node -e '
    const s = JSON.parse(require("node:fs").readFileSync(process.argv[1], "utf8"));
    console.log(`android-run: self-test ${s.state}: ${s.checks} checks, failed: ${JSON.stringify(s.failures)}`);
    process.exit(s.state === "passed" ? 0 : 1);
' "$WORK/selftest.json" || fail "the app self-test failed"

# 7. The lab run.
adb_ shell am force-stop "$PKG"
adb_ shell am start -n "$ACTIVITY" --ei port "$PORT" --ei settleMs "$SETTLE_MS" >/dev/null
say "app started; waiting for the run (at most $RUN_TIMEOUT s)"
wait_for status.json "$RUN_TIMEOUT"

# 8. Copy the results.
mkdir -p "$WORK/out" "$RESULTS"
adb_ exec-out run-as "$PKG" tar -cf - -C files/out . | tar -xf - -C "$WORK/out"
[ -f "$WORK/out/status.json" ] || fail "no status.json from the app"
node -e '
    const s = JSON.parse(require("node:fs").readFileSync(process.argv[1], "utf8"));
    if (s.state !== "done") { console.error("android-run: app status " + JSON.stringify(s)); process.exit(1); }
' "$WORK/out/status.json" || fail "the app run failed"
if [ -z "$CASES" ]; then
    find "$RESULTS" -maxdepth 1 -name '*.json' -type f -exec rm -f {} +
fi
cp "$WORK"/out/results/*.json "$RESULTS/"

# 9. run.json: the device facts of the app, plus host facts.
adb_ shell dumpsys webviewupdate >"$WORK/webviewupdate.txt"
AVD_NAME=$(adb_ emu avd name 2>/dev/null | head -1 | tr -d '\r' || true)
node --input-type=module - "$WORK" "$RESULTS/run.json" "$AVD_NAME" "$SERIAL" "$CASES" <<'EOF'
import { readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
const [work, out, avd, serial, cases] = process.argv.slice(2);
const device = JSON.parse(readFileSync(join(work, "out", "device.json"), "utf8"));
const status = JSON.parse(readFileSync(join(work, "out", "status.json"), "utf8"));
const dump = readFileSync(join(work, "webviewupdate.txt"), "utf8");
const current = /Current WebView package \(name, version\): \(([^,]+), ([^)]+)\)/.exec(dump);
const code = /versionName: [^,]+, versionCode: (\d+)/.exec(dump);
const rows = device.rows;
const count = (f) => rows.filter(f).length;
const off = rows.filter((r) => r.view === "off");
const on = rows.filter((r) => r.view === "on");
const notes = [
  "Method: serialize.js runs only in an isolated JavaScript world of androidx.webkit 1.17.1 " +
    "(WebViewCompat.getExecutionWorld; a script added with addJavaScriptOnEvent at INJECTION_EVENT_DOCUMENT_END). " +
    "Page scripts do not share this world.",
  "View on: the result is read after onPageFinished plus settleMs with JavaScriptReplyProxy.executeJavaScript " +
    "(the proxy of the ready message that the lab-world script posts to addWebMessageListener).",
  "View off: WebSettings.setJavaScriptEnabled(false); nothing turns it on. The androidx.webkit message object is " +
    "not installed in any world when scripting is disabled, so there is no reply proxy. The lab-world script still runs; " +
    "it waits for the window load event (load listeners, microtasks and requestAnimationFrame run in the lab world; " +
    "setTimeout callbacks do not), reads the DOM, and sends the serialize.js result as console.info chunks " +
    "(random nonce, length check) to WebChromeClient.onConsoleMessage. Settle time in view off: 0 ms (no timer runs; " +
    "with scripting disabled no script can change the DOM after the load).",
  `View off: lab-world result through the console channel in ${count((r) => r.view === "off" && r.channel === "console")} of ${off.length} rows; ` +
    `message object present in ${count((r) => r.view === "off" && r.bridge === true)} of ${off.length}.`,
  `View off: WebView.evaluateJavascript (page world) did not run in ${count((r) => r.view === "off" && r.mainWorld === "not-run")} of ${off.length} rows (callback value null).`,
  `View on: the load-event tree of the console channel equals the tree read after onPageFinished plus settleMs in ${count((r) => r.view === "on" && r.consoleCheck === "same-tree")} of ${on.length} rows.`,
  `View on: the page-world tree of evaluateJavascript equals the lab-world tree in ${count((r) => r.view === "on" && r.mainWorld === "same-tree")} of ${on.length} rows.`,
  `Blocked requests (not to the lab server): ${rows.reduce((n, r) => n + r.blocked, 0)}, in ${count((r) => r.blocked > 0)} rows.`,
];
const run = {
  platform: "android",
  date: new Date().toISOString(),
  partial: cases ? cases.split(",") : null,
  os: { name: "Android", release: device.androidRelease, sdkInt: device.sdkInt },
  device: { avd: avd || null, serial, model: device.model, image: device.fingerprint },
  webView: {
    package: current ? current[1] : device.webViewPackage,
    version: current ? current[2] : device.webViewVersion,
    versionCode: code ? Number(code[1]) : null,
    userAgent: device.userAgent,
  },
  features: device.features,
  labOrigin: device.labOrigin,
  settleMs: device.settleMs,
  walker: {
    method: "androidx.webkit isolated world (view on: reply proxy after onPageFinished plus settleMs; view off: console channel at the load event)",
    sha256: device.walkerSha256,
    labWorldSupported: device.labWorldSupported,
    notes,
  },
  counts: { rows: status.rows, ok: status.ok, unavailable: status.unavailable },
  rows,
};
writeFileSync(out, JSON.stringify(run, null, 2) + "\n");
EOF
say "run.json written"

# 10. Compare with parse5.
node "$LAB_DIR/host/compare.mjs" --platform android
head -5 "$RESULTS/summary.md" | tail -1
