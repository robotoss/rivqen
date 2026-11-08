#!/bin/sh
# SPDX-License-Identifier: Apache-2.0
# SPDX-FileCopyrightText: The Rivqen Authors
#
# URL interop, Foundation: macos | ios-sim <os> (WP-04 S2 T-07). No network.
#   sh run-swift.sh macos
#   LAB_SIM_TAG=url sh run-swift.sh ios-sim 27.0   (creates and deletes rivqen-p2-url-27.0 itself)
# The iOS binary runs with `simctl spawn` on a booted lab simulator of the tag url; no Simulator.app.
set -eu
here="$(cd "$(dirname "$0")" && pwd)"
. "$here/../lab-env.sh"
work="$LAB_HOME/out/url-interop"
mkdir -p "$work"
node "$here/cli.mjs" inputs "$work/urls.in"
export RQ_SWIFT_VERSION="$(swiftc --version 2>&1 | head -1)"
export SIMCTL_CHILD_RQ_SWIFT_VERSION="$RQ_SWIFT_VERSION"
src="$here/swift/main.swift"
case "$1" in
  macos)
    swiftc -O -o "$work/url-macos" "$src"
    for m in url components; do
      "$work/url-macos" "$work/urls.in" "$m" "foundation-$( [ $m = url ] && echo url || echo urlcomponents )-macos" > "$work/raw-macos-$m.txt"
      node "$here/cli.mjs" assemble "$work/raw-macos-$m.txt"
    done
    ;;
  ios-sim)
    os="$2"
    [ "${LAB_SIM_TAG:-}" = url ] || { echo "run-swift.sh: set LAB_SIM_TAG=url" >&2; exit 1; }
    sdk="$(xcrun --sdk iphonesimulator --show-sdk-path)"
    swiftc -O -sdk "$sdk" -target arm64-apple-ios17.0-simulator -o "$work/url-ios" "$src"
    sh "$here/../lab/sim.sh" create "$os"
    trap 'sh "$here/../lab/sim.sh" delete "$os" || true' EXIT
    sh "$here/../lab/sim.sh" boot "$os"
    udid="$(xcrun simctl list devices -j | node -e 'let s="";process.stdin.on("data",d=>s+=d).on("end",()=>{const j=JSON.parse(s);for(const k in j.devices)for(const d of j.devices[k])if(d.name===process.argv[1])console.log(d.udid)})' "$LAB_SIM_NAME_PREFIX-$os")"
    [ -n "$udid" ] || { echo "run-swift.sh: no lab simulator" >&2; exit 1; }
    cp "$work/urls.in" "$work/urls-sim.in"
    for m in url components; do
      xcrun simctl spawn "$udid" "$work/url-ios" "$work/urls-sim.in" "$m" "foundation-$( [ $m = url ] && echo url || echo urlcomponents )-ios-sim" > "$work/raw-ios-$m.txt"
      node "$here/cli.mjs" assemble "$work/raw-ios-$m.txt"
    done
    ;;
  *) echo "usage: run-swift.sh macos | ios-sim <os>" >&2; exit 2 ;;
esac
