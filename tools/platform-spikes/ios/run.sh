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
# Environment: SKIP_BUILD=1 uses the app that is already built; RUN_WAIT (seconds, default 240).
set -eu
HERE="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$HERE/../../.." && pwd)"
. "$HERE/../lab-env.sh"

BUNDLE_ID=dev.rivqen.platformspike
DERIVED="$LAB_DERIVED/ios-spike"
APP="$DERIVED/Build/Products/Debug-iphonesimulator/PlatformSpike.app"
CONTROL="http://127.0.0.1:$LAB_HOST_CONTROL_PORT"
ORIGIN="https://localhost:$LAB_HOST_PORT"
WAIT="${RUN_WAIT:-240}"

log() { printf 'ios/run.sh: %s\n' "$*" >&2; }
fail() {
  log "$*"
  exit 1
}

[ $# -ge 2 ] || fail "usage: run.sh <capability> <os> [k | a..b]"
CAP="$1"
OS="$2"
RANGE="${3:-1}"
case "$CAP" in CAP-I-0[1-7]) ;; *) fail "capability must be CAP-I-01 to CAP-I-07" ;; esac
case "$OS" in '' | *[!0-9.]*) fail "<os> must be a version such as 27.0" ;; esac
case "$RANGE" in
  *..*) FIRST="${RANGE%%..*}"; LAST="${RANGE##*..}" ;;
  *) FIRST="$RANGE"; LAST="$RANGE" ;;
esac
case "$FIRST$LAST" in '' | *[!0-9]*) fail "k must be a number or a..b" ;; esac
[ "$FIRST" -ge 1 ] && [ "$LAST" -ge "$FIRST" ] && [ "$LAST" -le 99 ] || fail "k range must be within 1..99"
case "$WAIT" in '' | *[!0-9]*) fail "RUN_WAIT must be seconds" ;; esac

mkdir -p "$LAB_HOME/tmp" "$LAB_OUT"
WORK="$(mktemp -d "$LAB_HOME/tmp/ios-run.XXXXXX")"
cleanup() {
  status=$?
  rm -rf "$WORK"
  exit "$status"
}
trap cleanup EXIT
trap 'exit 130' INT TERM

# 1. Host (shared; only checked).
code="$(curl -s -o /dev/null -w '%{http_code}' --cacert "$LAB_TLS/ca.pem" "$ORIGIN/s/echo" || true)"
[ "$code" = 200 ] || fail "the spike host does not answer on $ORIGIN (got $code); start it: node tools/platform-spikes/host/main.mjs"

# 2. Lab simulator, by its exact name.
line="$(sh "$ROOT/tools/platform-spikes/lab/sim.sh" list | awk -v n="$LAB_SIM_PREFIX-$OS" '$2 == n { print; exit }')"
[ -n "$line" ] || fail "no lab simulator $LAB_SIM_PREFIX-$OS; run: sh tools/platform-spikes/lab/sim.sh create $OS && sim.sh boot $OS && sim.sh trust $OS"
UDID="$(echo "$line" | awk '{ print $1 }')"
STATE="$(echo "$line" | awk '{ print $3 }')"
[ "$STATE" = Booted ] || sh "$ROOT/tools/platform-spikes/lab/sim.sh" boot "$OS"
DEVICE="$(xcrun simctl list devicetypes -j | node -e '
  const t = JSON.parse(require("fs").readFileSync(0, "utf8")).devicetypes.find((d) => d.identifier === process.argv[1]);
  console.log(t ? t.name : "unknown");
' "$LAB_SIM_DEVICE_TYPE")"

# 3. Build and install.
if [ "${SKIP_BUILD:-0}" != 1 ]; then
  log "build (DerivedData: $DERIVED)"
  xcodebuild -project "$HERE/App/SpikeApp.xcodeproj" -scheme SpikeApp -sdk iphonesimulator \
    -destination "id=$UDID" -derivedDataPath "$DERIVED" -quiet build >"$WORK/build.log" 2>&1 || {
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
EVIDENCE="$ROOT/evidence/spikes/wp-04/ios/$CAP/$OS"

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
    -rqRun "$RUN" -rqCap "$CAP" -rqOrigin "$ORIGIN" >/dev/null
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
  xcrun simctl terminate "$UDID" "$BUNDLE_ID" >/dev/null 2>&1 || true
  if [ -f "$OUTDIR/DONE" ]; then OUTCOME="$(cat "$OUTDIR/DONE")"; else OUTCOME="error: no DONE after $WAIT s"; fi
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
    DEST="$LAB_OUT/ios-failed/$CAP/$OS"
    failed=1
    log "run $RUN: $OUTCOME (log in $DEST)"
  fi
  node "$HERE/join.mjs" --app "$OUTDIR/app.jsonl" --host "$WORK/host.jsonl" --metrics "$OUTDIR/metrics.json" \
    --meta "$WORK/meta.json" --out "$DEST"
  if [ "$OUTCOME" = ok ] && [ -f "$OUTDIR/app-run.json" ]; then
    cp "$OUTDIR/app-run.json" "$WORK/app-run.json"
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
    const [date, xcode, swift, device, commit] = process.argv.slice(1);
    process.stdout.write(JSON.stringify({ date, tools: { xcode, swift }, device_model: device, app_commit: commit }));
  ' "$(date -u +%Y-%m-%d)" "$XCODE" "$SWIFT" "$DEVICE" "$COMMIT" >"$WORK/facts.json"
  node "$HERE/join.mjs" --run-json "$WORK/app-run.json" --facts "$WORK/facts.json" --out "$EVIDENCE"
fi
[ "$failed" = 0 ] || exit 1
log "done: $EVIDENCE"
