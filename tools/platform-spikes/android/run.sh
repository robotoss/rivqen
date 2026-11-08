#!/usr/bin/env sh
# SPDX-License-Identifier: Apache-2.0
# SPDX-FileCopyrightText: The Rivqen Authors
#
# Runs the Android prototype for one capability on the lab emulator (CONTRACT.md sections 4, 5, 9).
# Usage: sh tools/platform-spikes/android/run.sh <capability> <api> [k | a..b]   (default 1..3)
#   capability  CAP-A-01 | CAP-A-02 | CAP-A-03 | CAP-A-04 | CAP-A-05 | CAP-A-08
#   api         37 | 29 (the running lab emulator must have this API level)
# Environment: SKIP_BUILD=1 installs the APK of the last build. SPIKE_SET=<set> runs a named
#   supporting set of runlog.mjs SETS (output in <capability>/<api>/<set>/). Exploration only (never
#   evidence): SPIKE_PARAMS=<query> replaces the page parameters; SPIKE_OUT=<dir> writes the output there.
# Needs: the lab emulator (lab/emulator.sh start <api>) and the spike host (host/main.mjs) running.
# Output: evidence/spikes/wp-04/android/<capability>/<api>[/<set>]/run-<k>.jsonl, run-<k>.metrics.json,
#   run.json (only when at least one run is ok; a failed run goes to $LAB_OUT/android-failed).
# Every adb call names the lab serial ($LAB_SERIAL); the script never starts or stops the host.
set -eu
HERE="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$HERE/../../.." && pwd)"
. "$HERE/../lab-env.sh"
PKG=dev.rivqen.spike
ACTIVITY="$PKG/.SpikeActivity"
CONTROL="http://127.0.0.1:$LAB_HOST_CONTROL_PORT"
APP_TIMEOUT_S=200

die() { echo "run.sh: $*" >&2; exit 1; }
usage() { echo "usage: [SPIKE_SET=<set>] run.sh CAP-A-01|02|03|04|05|06|07|08 37|29 [k | a..b]" >&2; exit 2; }

cap="${1:-}"
api="${2:-}"
spec="${3:-1..3}"
set_name="${SPIKE_SET:-}"
case "$cap" in CAP-A-01 | CAP-A-02 | CAP-A-03 | CAP-A-04 | CAP-A-05 | CAP-A-06 | CAP-A-07 | CAP-A-08) ;; *) usage ;; esac
case "$set_name" in "" | [a-z0-9]*) ;; *) usage ;; esac
case "$set_name" in *[!a-z0-9-]*) usage ;; esac
scenario="$(node "$HERE/runlog.mjs" scenario "$cap" "$set_name")" || die "unknown set '$set_name' of $cap"
seed="$(node "$HERE/runlog.mjs" seed "$cap" "$set_name")" || die "unknown set '$set_name' of $cap"
case "$api" in 37 | 29) ;; *) usage ;; esac
case "$spec" in
  [1-9]) first="$spec"; last="$spec" ;;
  [1-9]..[1-9]) first="${spec%%..*}"; last="${spec##*..}" ;;
  *) usage ;;
esac
[ "$first" -le "$last" ] || usage

adb_() { "$LAB_ADB" -s "$LAB_SERIAL" "$@"; }

# 1. Preconditions: the lab emulator with this API, the spike host control port.
sh "$HERE/../lab/emulator.sh" status | grep -q '^running' || die "the lab emulator is not running (lab/emulator.sh start $api)"
sdk="$(adb_ shell getprop ro.build.version.sdk | tr -d '\r')"
[ "${sdk%%.*}" = "$api" ] || die "the lab emulator has API $sdk, not $api"
code="$(curl -s -o /dev/null -w '%{http_code}' "$CONTROL/run/log?run=probe" || true)"
[ "$code" != "000" ] || die "the spike host control port $LAB_HOST_CONTROL_PORT does not answer (node tools/platform-spikes/host/main.mjs)"
sh "$HERE/../lab/emulator.sh" reverse >/dev/null

mkdir -p "$LAB_HOME/tmp"
WORK="$(mktemp -d "$LAB_HOME/tmp/spike-android.XXXXXX")"
trap 'rm -rf "$WORK"' EXIT

# 2. Build and install.
if [ "${SKIP_BUILD:-0}" = 1 ]; then
  APK="$LAB_HOME/build/platform-spikes-android/app/outputs/apk/debug/app-debug.apk"
else
  APK="$(sh "$HERE/build.sh" | tail -1)"
fi
[ -f "$APK" ] || die "no APK at $APK"
adb_ install -r -t "$APK" >/dev/null || die "adb install failed"
if [ "$sdk" -ge 37 ]; then
  adb_ shell pm grant "$PKG" android.permission.ACCESS_LOCAL_NETWORK || die "cannot grant ACCESS_LOCAL_NETWORK"
fi
adb_ shell dumpsys webviewupdate > "$WORK/webviewupdate.txt"
"$JAVA_HOME/bin/java" -version 2>&1 | head -1 > "$WORK/java-version.txt"

OUT="${SPIKE_OUT:-$ROOT/evidence/spikes/wp-04/android/$cap/$api${set_name:+/$set_name}}"
mkdir -p "$OUT"

post() {
  curl -sS --fail -X POST -H 'Content-Type: application/json' --data "$2" "$CONTROL$1"
}

# 3. The runs.
failed=0
okruns=0
k="$first"
while [ "$k" -le "$last" ]; do
  run="$cap-android-$api-$k"
  url="$(node "$HERE/runlog.mjs" url "$cap" "$run" "${SPIKE_PARAMS:-}" "$set_name" "$api")"
  rm -f "$WORK"/app.jsonl "$WORK"/host.jsonl "$WORK"/metrics.json "$WORK"/device.json "$WORK"/status.json
  adb_ shell am force-stop "$PKG"
  adb_ shell run-as "$PKG" rm -rf files/out
  # A seed launch (CAP-A-07 persist) prepares the app state before the run: no host run, no log.
  # The app ends its own process after status.json; its output is removed.
  case "$url" in *"'"*) die "the URL has a quote: $url" ;; esac
  if [ -n "$seed" ]; then
    adb_ shell "am start -n $ACTIVITY --es run $run-seed --es capability $cap --es scenario $seed --es url '$url'" >/dev/null
    i=0
    while ! adb_ shell run-as "$PKG" ls files/out/status.json >/dev/null 2>&1; do
      i=$((i + 1))
      [ "$i" -le 60 ] || die "the seed launch of $run wrote no status"
      sleep 1
    done
    adb_ shell run-as "$PKG" cat files/out/status.json | grep -q '"done"' || die "the seed launch of $run failed"
    adb_ shell am force-stop "$PKG"
    adb_ shell run-as "$PKG" rm -rf files/out
  fi
  # One run.sh per run id: a second one would close the host log of the first.
  lock="$LAB_HOME/tmp/run-$run.lock"
  mkdir "$lock" 2>/dev/null || die "another run.sh runs $run (remove $lock if no run.sh is left)"
  trap 'rmdir "$lock" 2>/dev/null; rm -rf "$WORK"' EXIT
  # A run id left open by an interrupted earlier run would give 409: close it first.
  post /run/end "{\"run\":\"$run\"}" >/dev/null 2>&1 || true
  post /run/start "{\"run\":\"$run\",\"capability\":\"$cap\",\"platform\":\"android\",\"os\":\"$api\",\"k\":$k}" >/dev/null \
    || die "POST /run/start failed for $run"
  t0="$(node -e 'process.stdout.write(String(Date.now()))')"
  # The URL has only [A-Za-z0-9.:/?&=%-]; single quotes keep the device shell from reading '&'.
  case "$url" in *"'"*) die "the URL has a quote: $url" ;; esac
  adb_ shell "am start -n $ACTIVITY --es run $run --es capability $cap --es scenario $scenario --es url '$url'" >/dev/null
  i=0
  # `ls`, not `test`: on API 29 run-as cannot execute `test` ("exec failed for test: Permission denied").
  while ! adb_ shell run-as "$PKG" ls files/out/status.json >/dev/null 2>&1; do
    i=$((i + 1))
    [ "$i" -le "$APP_TIMEOUT_S" ] || break
    sleep 1
  done
  t1="$(node -e 'process.stdout.write(String(Date.now()))')"
  # Always close the host log of the run (the host keeps at most 16 open runs).
  post /run/end "{\"run\":\"$run\"}" > "$WORK/host.jsonl" || die "POST /run/end failed for $run"
  for f in app.jsonl metrics.json device.json status.json; do
    adb_ exec-out run-as "$PKG" cat "files/out/$f" > "$WORK/$f" 2>/dev/null || rm -f "$WORK/$f"
    [ -s "$WORK/$f" ] || rm -f "$WORK/$f"
  done
  if node "$HERE/runlog.mjs" join "$WORK" "$OUT" "$cap" "$api" "$k" "$run" "$url" "$((t1 - t0))" "$set_name"; then
    okruns=$((okruns + 1))
    if [ -f "$WORK/device.json" ]; then cp "$WORK/device.json" "$WORK/device-last.json"; fi
  else
    failed=$((failed + 1))
  fi
  rmdir "$lock" 2>/dev/null || true
  k=$((k + 1))
done
adb_ shell am force-stop "$PKG"

# 4. run.json (section 9), only next to ok runs, and the verdict facts.
if [ "$okruns" -gt 0 ] && [ -f "$WORK/device-last.json" ]; then
  cp "$WORK/device-last.json" "$WORK/device.json"
  node "$HERE/runlog.mjs" runjson "$WORK" "$OUT" "$api" "$set_name" "$scenario"
fi
if ! ls "$OUT"/run-*.jsonl >/dev/null 2>&1; then
  echo "run.sh: $cap: no ok run in $OUT"
  [ "$failed" -eq 0 ] || die "$failed run(s) not ok"
  exit 0
fi
node "$HERE/runlog.mjs" analyze "$OUT" "$cap" > "$WORK/analysis.json"
node -e '
  const a = JSON.parse(require("node:fs").readFileSync(process.argv[1], "utf8"));
  console.log(`run.sh: ${a.capability}: ${a.runs} runs, stable ${a.stable}`);
' "$WORK/analysis.json"
[ "$failed" -eq 0 ] || die "$failed run(s) not ok"
