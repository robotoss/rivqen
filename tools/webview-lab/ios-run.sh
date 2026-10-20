#!/bin/sh
# SPDX-License-Identifier: Apache-2.0
# SPDX-FileCopyrightText: The Rivqen Authors
#
# Runs the iOS WebView lab end to end (CONTRACT section 4 and 5):
#   1. checks that cases/ is current, starts host/server.mjs on 127.0.0.1:$LAB_PORT;
#   2. builds the iosApp for the simulator (DerivedData outside the repository);
#   3. boots the simulator if it is not booted, installs the app, copies serialize.js and the case
#      files into the data container, launches the app with -labServer;
#   4. waits for Documents/lab-output/DONE, copies the results to results/ios/, writes run.json;
#   5. runs host/compare.mjs --platform ios (writes results/ios/summary.md);
#   6. stops the server and shuts down the simulator only if this script booted it.
#
# Environment (all optional):
#   IOS_SIM_UDID  simulator id (default: the iPhone 17 Pro Max of the S5 run)
#   LAB_PORT      server port (default 18091; the Android lane uses 18092)
#   DERIVED_DATA  xcodebuild -derivedDataPath (default: $TMPDIR/rivqen-webview-lab-ios)
#   SKIP_BUILD=1  use the app in DERIVED_DATA as it is
#   LAB_SETTLE    seconds after didFinish, 0 to 2 (default 1)
#   LAB_TIMEOUT   seconds for one load or one script, 1 to 120 (default 20)
#   RUN_WAIT      seconds to wait for the whole app run (default 900)

set -eu

LAB_DIR=$(cd "$(dirname "$0")" && pwd)
UDID=${IOS_SIM_UDID:-272E9409-7098-4A54-B64C-8D9824AB02DA}
PORT=${LAB_PORT:-18091}
DERIVED=${DERIVED_DATA:-${TMPDIR:-/tmp}/rivqen-webview-lab-ios}
SETTLE=${LAB_SETTLE:-1}
TIMEOUT=${LAB_TIMEOUT:-20}
WAIT=${RUN_WAIT:-900}
BUNDLE_ID=dev.rivqen.webviewlab.WebViewLab
PROJECT=$LAB_DIR/app/WebViewLab/iosApp/iosApp.xcodeproj
RESULTS=$LAB_DIR/results/ios
APP=$DERIVED/Build/Products/Debug-iphonesimulator/WebViewLab.app

log() { printf 'ios-run: %s\n' "$*" >&2; }
fail() {
  log "$*"
  exit 1
}

case $PORT in '' | *[!0-9]*) fail "LAB_PORT must be a number" ;; esac
case $WAIT in '' | *[!0-9]*) fail "RUN_WAIT must be a number of seconds" ;; esac
case $UDID in '' | *[!0-9A-Fa-f-]*) fail "IOS_SIM_UDID must be a simulator id" ;; esac

WORK=$(mktemp -d "${TMPDIR:-/tmp}/rivqen-ios-run.XXXXXX")
SERVER_PID=
BOOTED_BY_US=0

cleanup() {
  status=$?
  [ "$status" -eq 0 ] || log "stopped with exit status $status"
  # Best effort: each step can fail when the run stopped before it started.
  if [ -n "$SERVER_PID" ]; then kill "$SERVER_PID" 2>/dev/null || log "server already stopped"; fi
  xcrun simctl terminate "$UDID" "$BUNDLE_ID" >/dev/null 2>&1 || true
  if [ "$BOOTED_BY_US" = 1 ]; then
    xcrun simctl shutdown "$UDID" || log "could not shut down simulator $UDID"
  fi
  rm -rf "$WORK"
  exit "$status"
}
trap cleanup EXIT
trap 'exit 130' INT TERM

# 1. Cases and server.
node "$LAB_DIR/host/build-cases.mjs" --check || fail "cases/ is not current: run node host/build-cases.mjs"
if curl -s -o /dev/null "http://127.0.0.1:$PORT/cases.json"; then
  fail "port $PORT is in use; stop the other server or set LAB_PORT"
fi
node "$LAB_DIR/host/server.mjs" --port "$PORT" >"$WORK/server.log" 2>&1 &
SERVER_PID=$!
i=0
until curl -sf -o /dev/null "http://127.0.0.1:$PORT/cases.json"; do
  i=$((i + 1))
  [ "$i" -le 50 ] || fail "server did not start: $(cat "$WORK/server.log")"
  sleep 0.2
done
log "server on 127.0.0.1:$PORT"

# 2. Build.
if [ "${SKIP_BUILD:-0}" != 1 ]; then
  log "build (DerivedData: $DERIVED)"
  xcodebuild -project "$PROJECT" -scheme iosApp -sdk iphonesimulator -destination "id=$UDID" \
    -derivedDataPath "$DERIVED" -quiet build >"$WORK/build.log" 2>&1 || {
    tail -n 40 "$WORK/build.log" >&2
    fail "build failed"
  }
fi
[ -d "$APP" ] || fail "no app at $APP"

# 3. Simulator, input files, launch.
xcrun simctl list devices -j >"$WORK/devices.json"
STATE=$(node -e '
  const d = JSON.parse(require("fs").readFileSync(process.argv[1], "utf8"));
  for (const list of Object.values(d.devices)) for (const x of list) if (x.udid === process.argv[2]) console.log(x.state);
' "$WORK/devices.json" "$UDID")
[ -n "$STATE" ] || fail "no simulator $UDID"
if [ "$STATE" != Booted ]; then
  log "boot $UDID"
  xcrun simctl boot "$UDID"
  BOOTED_BY_US=1
fi
xcrun simctl bootstatus "$UDID" -b >/dev/null
xcrun simctl terminate "$UDID" "$BUNDLE_ID" >/dev/null 2>&1 || true # not running is fine
xcrun simctl install "$UDID" "$APP"
DATA=$(xcrun simctl get_app_container "$UDID" "$BUNDLE_ID" data)
[ -d "$DATA" ] || fail "no data container"
IN=$DATA/Documents/lab-input
OUT=$DATA/Documents/lab-output
rm -rf "$IN" "$OUT"
mkdir -p "$IN/cases"
cp "$LAB_DIR/host/serialize.js" "$IN/serialize.js"
cp "$LAB_DIR"/cases/*.json "$IN/cases/"
log "launch"
xcrun simctl launch "$UDID" "$BUNDLE_ID" -labServer "http://127.0.0.1:$PORT" -labSettle "$SETTLE" \
  -labTimeout "$TIMEOUT" >/dev/null

# 4. Wait, copy, run log.
i=0
until [ -f "$OUT/DONE" ]; do
  i=$((i + 2))
  [ "$i" -le "$WAIT" ] || fail "no result after $WAIT s"
  sleep 2
done
OUTCOME=$(cat "$OUT/DONE")
[ "$OUTCOME" = ok ] || fail "app: $OUTCOME"
[ -f "$OUT/app-run.json" ] || fail "no app-run.json"

mkdir -p "$RESULTS"
find "$RESULTS" -maxdepth 1 -type f \( -name '*.on.json' -o -name '*.off.json' -o -name run.json \) -delete
find "$OUT" -maxdepth 1 -type f \( -name '*.on.json' -o -name '*.off.json' \) -exec cp {} "$RESULTS/" \;
xcrun simctl list devices -j >"$WORK/devices.json"
xcodebuild -version >"$WORK/xcode.txt"
node - "$OUT/app-run.json" "$WORK/devices.json" "$WORK/xcode.txt" "$LAB_DIR/host/serialize.js" "$UDID" \
  "$RESULTS/run.json" <<'EOF'
// Merge the app run log with the host facts. Paths come as arguments, never as code.
const fs = require("fs");
const crypto = require("crypto");
const [appRun, devices, xcode, serializer, udid, out] = process.argv.slice(2);
const run = JSON.parse(fs.readFileSync(appRun, "utf8"));
let device = null;
let runtime = null;
for (const [rt, list] of Object.entries(JSON.parse(fs.readFileSync(devices, "utf8")).devices)) {
  for (const x of list) if (x.udid === udid) { device = x.name; runtime = rt; }
}
const host = {
  simulatorName: device,
  simulatorId: udid,
  simulatorRuntime: runtime,
  xcode: fs.readFileSync(xcode, "utf8").trim().split("\n").join(", "),
  serializerSha256: crypto.createHash("sha256").update(fs.readFileSync(serializer)).digest("hex"),
  date: new Date().toISOString(),
};
fs.writeFileSync(out, JSON.stringify({ ...run, host }, null, 2) + "\n");
EOF
log "results in $RESULTS"

# 5. Compare.
node "$LAB_DIR/host/compare.mjs" --platform ios
