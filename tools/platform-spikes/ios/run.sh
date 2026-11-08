#!/usr/bin/env sh
# SPDX-License-Identifier: Apache-2.0
# SPDX-FileCopyrightText: The Rivqen Authors
#
# Runs one capability of the iOS prototype on a lab simulator (CONTRACT.md sections 4, 5, 9).
# Usage: sh tools/platform-spikes/ios/run.sh <capability> <os> [k | a..b]     (default k = 1)
#   for example: sh tools/platform-spikes/ios/run.sh CAP-I-01 27.0 1..3
#
# Needs: the spike host on $LAB_HOST_PORT (shared, this script never starts or stops it), and the
# lab simulator rivqen-p2-<os> made with `lab/sim.sh create <os>` and trusted with
# `lab/sim.sh trust <os>`. The script boots it when it is shut down and leaves it booted.
# For each k: POST /run/start, launch the app with the run arguments, wait for DONE in the app
# data container, POST /run/end, join script + app + host parts into
# evidence/spikes/wp-04/ios/<capability>/<os>/run-<k>.jsonl. A run that is not ok goes to
# $LAB_OUT/ios-failed/ instead, and the script exits 1.
# Environment: SKIP_BUILD=1 uses the app that is already built; RUN_WAIT (seconds, default 240);
# SET=<name> runs a supporting set of the capability (README "Supporting sets"): the app gets
# -rqSet <name>, the runs go to evidence/.../<capability>/<os>/<name>/, and the set app-bound uses
# the AppBound build configuration (Info.plist WKAppBoundDomains, lab only).
# RECORD=1 records one run (one k, for example 4) with `lab/sim.sh record` (simctl io recordVideo of
# the lab device only): the run log goes to $LAB_OUT/ios-recorded/, never to evidence/; the run must
# be ok and equal to the stored run-1 after normalization (record.mjs stable); then record.mjs clip
# writes evidence/recordings/wp-04/<capability>-ios-<os>.mp4 in the format of CONTRACT.md section 9.
# FFMPEG and FFPROBE name the lab tools (default: on PATH, else /opt/homebrew/bin).
set -eu
HERE="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$HERE/../../.." && pwd)"
. "$HERE/../lab-env.sh"

SET="${SET:-}"
case "$SET" in '' | app-bound | initiator) ;; *) printf 'ios/run.sh: SET must be app-bound or initiator\n' >&2; exit 1 ;; esac
BUNDLE_ID=dev.rivqen.platformspike
CONFIGURATION=Debug
[ "$SET" != app-bound ] || CONFIGURATION=AppBound
DERIVED="$LAB_DERIVED/ios-spike"
APP="$DERIVED/Build/Products/$CONFIGURATION-iphonesimulator/PlatformSpike.app"
CONTROL="http://127.0.0.1:$LAB_HOST_CONTROL_PORT"
ORIGIN="https://localhost:$LAB_HOST_PORT"
WAIT="${RUN_WAIT:-240}"
RECORD="${RECORD:-0}"

log() { printf 'ios/run.sh: %s\n' "$*" >&2; }
fail() {
  log "$*"
  exit 1
}

[ $# -ge 2 ] || fail "usage: run.sh <capability> <os> [k | a..b]"
CAP="$1"
OS="$2"
RANGE="${3:-1}"
case "$CAP" in CAP-I-0[1-9] | CAP-I-1[01]) ;; *) fail "capability must be CAP-I-01 to CAP-I-11" ;; esac
case "$CAP/$SET" in */ | CAP-I-09/app-bound | CAP-I-11/initiator) ;; *) fail "SET=$SET is not a set of $CAP" ;; esac
case "$OS" in '' | *[!0-9.]*) fail "<os> must be a version such as 27.0" ;; esac
case "$RANGE" in
  *..*) FIRST="${RANGE%%..*}"; LAST="${RANGE##*..}" ;;
  *) FIRST="$RANGE"; LAST="$RANGE" ;;
esac
case "$FIRST$LAST" in '' | *[!0-9]*) fail "k must be a number or a..b" ;; esac
[ "$FIRST" -ge 1 ] && [ "$LAST" -ge "$FIRST" ] && [ "$LAST" -le 99 ] || fail "k range must be within 1..99"
case "$WAIT" in '' | *[!0-9]*) fail "RUN_WAIT must be seconds" ;; esac
case "$RECORD" in 0 | 1) ;; *) fail "RECORD must be 0 or 1" ;; esac
if [ "$RECORD" = 1 ]; then
  [ "$FIRST" = "$LAST" ] || fail "RECORD=1 records one run: give one k (for example 4)"
  FFMPEG="${FFMPEG:-$(command -v ffmpeg || echo /opt/homebrew/bin/ffmpeg)}"
  FFPROBE="${FFPROBE:-$(command -v ffprobe || echo /opt/homebrew/bin/ffprobe)}"
  [ -x "$FFMPEG" ] && [ -x "$FFPROBE" ] || fail "RECORD=1 needs ffmpeg and ffprobe (lab tools; set FFMPEG and FFPROBE)"
fi

mkdir -p "$LAB_HOME/tmp" "$LAB_OUT"
WORK="$(mktemp -d "$LAB_HOME/tmp/ios-run.XXXXXX")"
RECORDING=0
REC_FILE=""
cleanup() {
  status=$?
  # A recording that still runs when the script stops is stopped (its file stays in $LAB_OUT).
  if [ "$RECORDING" = 1 ]; then
    sh "$ROOT/tools/platform-spikes/lab/sim.sh" record stop "$OS" "$REC_FILE" >/dev/null 2>&1 || true
  fi
  rm -rf "$WORK"
  exit "$status"
}
trap cleanup EXIT
trap 'exit 130' INT TERM

# 1. Host (shared; only checked).
code="$(curl -s -o /dev/null -w '%{http_code}' --cacert "$LAB_TLS/ca.pem" "$ORIGIN/s/echo" || true)"
[ "$code" = 200 ] || fail "the spike host does not answer on $ORIGIN (got $code); start it: node tools/platform-spikes/host/main.mjs"

# 2. Lab simulator, by its exact name.
line="$(sh "$ROOT/tools/platform-spikes/lab/sim.sh" list | awk -v n="$LAB_SIM_NAME_PREFIX-$OS" '$2 == n { print; exit }')"
[ -n "$line" ] || fail "no lab simulator $LAB_SIM_NAME_PREFIX-$OS; run: sh tools/platform-spikes/lab/sim.sh create $OS && sim.sh boot $OS && sim.sh trust $OS"
UDID="$(echo "$line" | awk '{ print $1 }')"
STATE="$(echo "$line" | awk '{ print $3 }')"
[ "$STATE" = Booted ] || sh "$ROOT/tools/platform-spikes/lab/sim.sh" boot "$OS"
DEVICE="$(xcrun simctl list devicetypes -j | node -e '
  const t = JSON.parse(require("fs").readFileSync(0, "utf8")).devicetypes.find((d) => d.identifier === process.argv[1]);
  console.log(t ? t.name : "unknown");
' "$LAB_SIM_DEVICE_TYPE")"

# The iOS 18.4 to 18.x simulator runtimes keep libswiftWebKit.dylib only in their OS cryptex, where
# dyld does not look; an app with a deployment target before 18.4 that uses a Swift-refined WebKit
# API (callAsyncJavaScript, evaluateJavaScript with a content world) then stops at launch with
# "Library not loaded: /usr/lib/swift/libswiftWebKit.dylib" (WebKit bug 293831). The workaround of
# that bug: a dyld fallback path to the cryptex copy, for the app process only (simctl passes
# SIMCTL_CHILD_* to the launched app). Lab only; the app and its deployment target do not change.
LAUNCH_ENV=""
case "$OS" in
  18.[4-9] | 18.[4-9].*)
    RUNTIME_ROOT="$(xcrun simctl list runtimes -j | node -e '
      const r = JSON.parse(require("fs").readFileSync(0, "utf8")).runtimes;
      const m = r.find((x) => x.isAvailable && x.platform === "iOS" && x.version === process.argv[1]);
      console.log(m ? m.runtimeRoot : "");
    ' "$OS")"
    SWIFT_CRYPTEX="$RUNTIME_ROOT/System/Cryptexes/OS/usr/lib/swift"
    if [ -n "$RUNTIME_ROOT" ] && [ ! -f "$RUNTIME_ROOT/usr/lib/swift/libswiftWebKit.dylib" ] &&
      [ -f "$SWIFT_CRYPTEX/libswiftWebKit.dylib" ]; then
      export SIMCTL_CHILD_DYLD_FALLBACK_LIBRARY_PATH="$SWIFT_CRYPTEX"
      LAUNCH_ENV="DYLD_FALLBACK_LIBRARY_PATH=<runtime root>/System/Cryptexes/OS/usr/lib/swift (WebKit bug 293831)"
      log "iOS $OS: $LAUNCH_ENV"
    fi
    ;;
esac

# 3. Build and install.
if [ "${SKIP_BUILD:-0}" != 1 ]; then
  log "build (DerivedData: $DERIVED)"
  xcodebuild -project "$HERE/App/SpikeApp.xcodeproj" -scheme SpikeApp -sdk iphonesimulator \
    -configuration "$CONFIGURATION" -destination "id=$UDID" -derivedDataPath "$DERIVED" -quiet build >"$WORK/build.log" 2>&1 || {
    tail -n 40 "$WORK/build.log" >&2
    fail "build failed"
  }
fi
[ -d "$APP" ] || fail "no app at $APP"
xcrun simctl install "$UDID" "$APP"
DATA="$(xcrun simctl get_app_container "$UDID" "$BUNDLE_ID" data)"
OUTDIR="$DATA/Documents/spike-output"
PROTOTYPE="$(node -e '
  const c = JSON.parse(require("fs").readFileSync(process.argv[1], "utf8")).find((r) => r.id === process.argv[2]);
  console.log(c ? c.prototype : "unknown");
' "$ROOT/tools/platform-spikes/capabilities.json" "$CAP")"
EVIDENCE="$ROOT/evidence/spikes/wp-04/ios/$CAP/$OS${SET:+/$SET}"
if [ "$RECORD" = 1 ]; then
  STORED="$EVIDENCE/run-1.jsonl"
  [ -f "$STORED" ] || fail "RECORD=1 needs the stored run $STORED"
  # The recorded run is not evidence of the verdict: its log and run.json stay in the lab output.
  EVIDENCE="$LAB_OUT/ios-recorded/$CAP/$OS${SET:+/$SET}"
  CLIP="$ROOT/evidence/recordings/wp-04/$CAP-ios-$OS.mp4"
  mkdir -p "$ROOT/evidence/recordings/wp-04"
  # A fixed status bar (time 9:41, full battery) on the lab device, so the clip shows no clock time.
  xcrun simctl status_bar "$UDID" override --time 9:41 --dataNetwork wifi --wifiMode active --wifiBars 3 \
    --cellularMode notSupported --batteryState charged --batteryLevel 100
fi
SET_ARGS=""
[ -z "$SET" ] || SET_ARGS="-rqSet $SET"
# -rqRecord 1: a closed web view leaves a still frame on screen (screen only; README "Recordings").
[ "$RECORD" = 0 ] || SET_ARGS="$SET_ARGS -rqRecord 1"

now_ms() { perl -MTime::HiRes=time -e 'printf "%d", time * 1000'; }

post() {
  curl -s -o "$2" -w '%{http_code}' -X POST -H 'Content-Type: application/json' --data "$3" "$CONTROL$1"
}

failed=0
k="$FIRST"
while [ "$k" -le "$LAST" ]; do
  RUN="$CAP-ios-$OS-$k"
  log "run $RUN"
  xcrun simctl terminate "$UDID" "$BUNDLE_ID" >/dev/null 2>&1 || true # not running is fine
  rm -rf "$OUTDIR"
  if [ "$RECORD" = 1 ]; then
    REC_FILE="ios-rec-$RUN.mov"
    REC_LOG="$LAB_HOME/logs/sim-record.log"
    rm -f "$REC_LOG"
    sh "$ROOT/tools/platform-spikes/lab/sim.sh" record start "$OS" "$REC_FILE" >&2
    RECORDING=1
    i=0
    until grep -q 'Recording started' "$REC_LOG" 2>/dev/null; do
      i=$((i + 1))
      [ "$i" -le 15 ] || fail "simctl io recordVideo did not start in 15 s ($REC_LOG)"
      sleep 1
    done
  fi
  START_BODY="{\"run\":\"$RUN\",\"capability\":\"$CAP\",\"platform\":\"ios\",\"os\":\"$OS\",\"k\":$k}"
  code="$(post /run/start "$WORK/start.out" "$START_BODY")"
  if [ "$code" = 409 ]; then
    # A run with this id is still open from a stopped earlier attempt: close it and start again.
    post /run/end "$WORK/stale.out" "{\"run\":\"$RUN\"}" >/dev/null
    code="$(post /run/start "$WORK/start.out" "$START_BODY")"
  fi
  [ "$code" = 200 ] || fail "POST /run/start: $code $(cat "$WORK/start.out")"
  T0="$(now_ms)"
  xcrun simctl launch --terminate-running-process "$UDID" "$BUNDLE_ID" \
    -rqRun "$RUN" -rqCap "$CAP" -rqOrigin "$ORIGIN" $SET_ARGS >/dev/null
  i=0
  until [ -f "$OUTDIR/DONE" ]; do
    i=$((i + 1))
    [ "$i" -le "$WAIT" ] || break
    sleep 1
  done
  # Give the host time to log the last responses of the page.
  sleep 1
  T1="$(now_ms)"
  code="$(post /run/end "$WORK/host.jsonl" "{\"run\":\"$RUN\"}")"
  [ "$code" = 200 ] || fail "POST /run/end: $code"
  if [ "$RECORDING" = 1 ]; then
    # The last page stays on screen for 2 s more; the host log is already closed.
    sleep 2
    sh "$ROOT/tools/platform-spikes/lab/sim.sh" record stop "$OS" "$REC_FILE" >&2
    RECORDING=0
  fi
  xcrun simctl terminate "$UDID" "$BUNDLE_ID" >/dev/null 2>&1 || true
  if [ -f "$OUTDIR/DONE" ]; then OUTCOME="$(cat "$OUTDIR/DONE")"; else OUTCOME="error: no DONE after $WAIT s"; fi
  # An app that stops before it writes anything (for example at launch) leaves no folder; the run
  # still goes to ios-failed/ with empty app parts.
  mkdir -p "$OUTDIR"
  for f in app.jsonl metrics.json; do [ -f "$OUTDIR/$f" ] || printf '' >"$OUTDIR/$f"; done
  [ -s "$OUTDIR/metrics.json" ] || echo '{}' >"$OUTDIR/metrics.json"
  node -e '
    const [run, cap, os, k, device, prototype, outcome, t] = process.argv.slice(1);
    const ok = outcome === "ok";
    process.stdout.write(JSON.stringify({ run, capability: cap, os, k: Number(k), device, prototype, ok,
      error: ok ? null : outcome.slice(0, 200), t_end: Number(t) }));
  ' "$RUN" "$CAP" "$OS" "$k" "$DEVICE" "$PROTOTYPE" "$OUTCOME" "$((T1 - T0))" >"$WORK/meta.json"
  if [ "$OUTCOME" = ok ]; then
    DEST="$EVIDENCE"
  else
    DEST="$LAB_OUT/ios-failed/$CAP/$OS${SET:+/$SET}"
    failed=1
    log "run $RUN: $OUTCOME (log in $DEST)"
  fi
  node "$HERE/join.mjs" --app "$OUTDIR/app.jsonl" --host "$WORK/host.jsonl" --metrics "$OUTDIR/metrics.json" \
    --meta "$WORK/meta.json" --out "$DEST"
  if [ "$OUTCOME" = ok ] && [ -f "$OUTDIR/app-run.json" ]; then
    cp "$OUTDIR/app-run.json" "$WORK/app-run.json"
  fi
  if [ "$RECORD" = 1 ] && [ "$OUTCOME" = ok ]; then
    # The clip goes into evidence only from an ok run whose log equals the stored runs.
    node "$HERE/record.mjs" stable --recorded "$DEST/run-$k.jsonl" --stored "$STORED" ||
      fail "the recorded run differs from the stored runs; no clip (raw video: $LAB_OUT/$REC_FILE)"
    node "$HERE/record.mjs" clip --in "$LAB_OUT/$REC_FILE" --out "$CLIP" --ffmpeg "$FFMPEG" --ffprobe "$FFPROBE" ||
      fail "no clip (raw video: $LAB_OUT/$REC_FILE)"
    rm -f "$LAB_OUT/$REC_FILE"
  fi
  k=$((k + 1))
done

# 4. run.json (section 9) from the last ok run.
if [ -f "$WORK/app-run.json" ]; then
  COMMIT="$(git -C "$ROOT" rev-parse --short=12 HEAD)"
  [ -z "$(git -C "$ROOT" status --porcelain -- tools/platform-spikes/ios)" ] || COMMIT="$COMMIT+dirty"
  XCODE="$(xcodebuild -version | tr '\n' ' ' | sed 's/ *$//')"
  SWIFT="$(xcrun swift --version 2>/dev/null | head -1)"
  node -e '
    const [date, xcode, swift, device, commit, launchEnv] = process.argv.slice(1);
    const facts = { date, tools: { xcode, swift }, device_model: device, app_commit: commit };
    if (launchEnv !== "") facts.launch_env = launchEnv;
    process.stdout.write(JSON.stringify(facts));
  ' "$(date -u +%Y-%m-%d)" "$XCODE" "$SWIFT" "$DEVICE" "$COMMIT" "$LAUNCH_ENV" >"$WORK/facts.json"
  node "$HERE/join.mjs" --run-json "$WORK/app-run.json" --facts "$WORK/facts.json" --out "$EVIDENCE"
fi
[ "$failed" = 0 ] || exit 1
log "done: $EVIDENCE"
