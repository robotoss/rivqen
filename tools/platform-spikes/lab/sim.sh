#!/usr/bin/env sh
# SPDX-License-Identifier: Apache-2.0
# SPDX-FileCopyrightText: The Rivqen Authors
#
# Create and use the lab iOS simulators (H-128). A lab simulator is named rivqen-p2-<os>, for
# example rivqen-p2-27.0. Every command that changes or boots a device first looks the device up
# by that exact name, so no other simulator is ever booted, changed or deleted.
# Usage: sh tools/platform-spikes/lab/sim.sh [--dry-run] <command>
#   list                   the lab simulators (name, udid, state)
#   create <os>            make rivqen-p2-<os> with LAB_SIM_DEVICE_TYPE on the runtime of that version
#   boot <os>              boot it (waits until it is ready)
#   trust <os>             install $LAB_TLS/ca.pem as a root certificate (CONTRACT.md section 2)
#   record start|stop <os> <file>   simctl io recordVideo into $LAB_OUT (a file name, no folder)
#   shutdown <os>          shut it down
#   delete <os>|all        shut down and delete it, or all devices with the prefix
set -eu
HERE="$(cd "$(dirname "$0")" && pwd)"
. "$HERE/../lab-env.sh"
. "$HERE/common.sh"

usage() {
  echo "usage: sim.sh [--dry-run] list | create <os> | boot <os> | trust <os> | record start|stop <os> <file> | shutdown <os> | delete <os>|all" >&2
  exit 2
}

check_os() {
  case "$1" in
    '' | *[!0-9.]*) echo "sim.sh: <os> must be a version such as 27.0" >&2; exit 2 ;;
  esac
}

# Print "udid state" of the lab device with the exact name rivqen-p2-<os>, or nothing.
find_device() {
  xcrun simctl list devices -j | node -e '
    const d = JSON.parse(require("fs").readFileSync(0, "utf8")).devices;
    const name = process.argv[1];
    for (const list of Object.values(d)) for (const x of list) if (x.name === name) console.log(x.udid + " " + x.state);
  ' "$LAB_SIM_PREFIX-$1" | head -1
}

# Print "udid name" of every device whose name starts with the lab prefix and a dash.
lab_devices() {
  xcrun simctl list devices -j | node -e '
    const d = JSON.parse(require("fs").readFileSync(0, "utf8")).devices;
    const p = process.argv[1] + "-";
    for (const list of Object.values(d)) for (const x of list) if (x.name.startsWith(p)) console.log(x.udid + " " + x.name + " " + x.state);
  ' "$LAB_SIM_PREFIX"
}

# Print the runtime identifier of the available iOS runtime with this exact version.
runtime_id() {
  xcrun simctl list runtimes -j | node -e '
    const r = JSON.parse(require("fs").readFileSync(0, "utf8")).runtimes;
    const m = r.filter((x) => x.isAvailable && x.platform === "iOS" && x.version === process.argv[1]);
    if (m.length) console.log(m[0].identifier);
  ' "$1"
}

# Set UDID and STATE for <os>, or stop with an error when the device does not exist.
need_device() {
  check_os "$1"
  found="$(find_device "$1")"
  [ -n "$found" ] || { echo "sim.sh: no lab simulator $LAB_SIM_PREFIX-$1; run: sim.sh create $1" >&2; exit 1; }
  UDID="${found%% *}"
  STATE="${found#* }"
}

cmd="${1:-}"
[ $# -eq 0 ] || shift
case "$cmd" in
  list)
    lab_devices
    ;;
  create)
    os="${1:-}"
    check_os "$os"
    [ -z "$(find_device "$os")" ] || { echo "sim.sh: $LAB_SIM_PREFIX-$os exists"; exit 0; }
    rt="$(runtime_id "$os")"
    [ -n "$rt" ] || { echo "sim.sh: no available iOS runtime $os (xcrun simctl list runtimes)" >&2; exit 1; }
    free=$(system_free_gb)
    [ "$free" -ge 20 ] || { echo "sim.sh: only $free GB free on the system volume (need 20)" >&2; exit 1; }
    if [ "$LAB_DRY_RUN" = 1 ]; then
      echo "dry-run: xcrun simctl create $LAB_SIM_PREFIX-$os $LAB_SIM_DEVICE_TYPE $rt"
    else
      udid="$(xcrun simctl create "$LAB_SIM_PREFIX-$os" "$LAB_SIM_DEVICE_TYPE" "$rt")"
      echo "sim.sh: created $LAB_SIM_PREFIX-$os $udid ($rt)"
    fi
    ;;
  boot)
    need_device "${1:-}"
    if [ "$STATE" = Booted ]; then echo "sim.sh: already booted"; exit 0; fi
    run xcrun simctl boot "$UDID"
    [ "$LAB_DRY_RUN" = 1 ] || xcrun simctl bootstatus "$UDID" >/dev/null
    ;;
  trust)
    need_device "${1:-}"
    [ -f "$LAB_TLS/ca.pem" ] || { echo "sim.sh: no $LAB_TLS/ca.pem; start the spike host first" >&2; exit 1; }
    run xcrun simctl keychain "$UDID" add-root-cert "$LAB_TLS/ca.pem"
    ;;
  record)
    sub="${1:-}"
    os="${2:-}"
    file="${3:-}"
    [ -n "$file" ] || usage
    case "$file" in */*) echo "sim.sh: give a file name without a folder (it goes into $LAB_OUT)" >&2; exit 2 ;; esac
    need_device "$os"
    pidfile="$LAB_HOME/logs/sim-record-$UDID.pid"
    case "$sub" in
      start)
        [ "$STATE" = Booted ] || { echo "sim.sh: $LAB_SIM_PREFIX-$os is not booted" >&2; exit 1; }
        if [ "$LAB_DRY_RUN" = 1 ]; then echo "dry-run: xcrun simctl io $UDID recordVideo $LAB_OUT/$file"; exit 0; fi
        [ ! -f "$pidfile" ] || { echo "sim.sh: a recording is already running" >&2; exit 1; }
        mkdir -p "$LAB_OUT" "$LAB_HOME/logs"
        # A background job of a script ignores SIGINT; perl resets it before it starts simctl.
        nohup perl -e '$SIG{INT} = "DEFAULT"; exec @ARGV' xcrun simctl io "$UDID" recordVideo --force "$LAB_OUT/$file" > "$LAB_HOME/logs/sim-record.log" 2>&1 &
        echo $! > "$pidfile"
        echo "sim.sh: recording to $LAB_OUT/$file"
        ;;
      stop)
        if [ "$LAB_DRY_RUN" = 1 ]; then echo "dry-run: stop recordVideo of $UDID"; exit 0; fi
        [ -f "$pidfile" ] || { echo "sim.sh: no recording is running" >&2; exit 1; }
        pid="$(cat "$pidfile")"
        # SIGINT makes recordVideo finish the file.
        kill -INT "$pid" 2>/dev/null || true
        i=0
        while kill -0 "$pid" 2>/dev/null; do
          i=$((i + 1))
          [ $i -le 30 ] || { kill "$pid" 2>/dev/null || true; break; }
          sleep 1
        done
        rm -f "$pidfile"
        echo "sim.sh: saved $LAB_OUT/$file"
        ;;
      *) usage ;;
    esac
    ;;
  shutdown)
    need_device "${1:-}"
    if [ "$STATE" = Booted ]; then run xcrun simctl shutdown "$UDID"; else echo "sim.sh: not booted"; fi
    ;;
  delete)
    target="${1:-}"
    [ -n "$target" ] || usage
    if [ "$target" = all ]; then
      devices="$(lab_devices)"
    else
      check_os "$target"
      found="$(find_device "$target")"
      devices=""
      [ -z "$found" ] || devices="${found%% *} $LAB_SIM_PREFIX-$target ${found#* }"
    fi
    if [ -z "$devices" ]; then echo "sim.sh: nothing to delete"; exit 0; fi
    echo "$devices" | while read -r udid name state; do
      case "$name" in
        "$LAB_SIM_PREFIX"-*) ;;
        *) echo "sim.sh: refusing to delete $name" >&2; exit 1 ;;
      esac
      if [ "$state" = Booted ]; then run xcrun simctl shutdown "$udid"; fi
      run xcrun simctl delete "$udid"
      echo "sim.sh: deleted $name $udid"
    done
    ;;
  *)
    usage
    ;;
esac
