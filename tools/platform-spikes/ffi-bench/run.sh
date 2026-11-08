#!/bin/sh
# SPDX-License-Identifier: Apache-2.0
# SPDX-FileCopyrightText: The Rivqen Authors
#
# FFI benchmark of CONTRACT.md section 10.
#
#   sh tools/platform-spikes/ffi-bench/run.sh host [jvm|swift]
#
# Builds the Rust library and the UniFFI bindings, runs the self-checks, then one process per
# case (path x chunk size), and writes evidence/spikes/wp-04/ffi/host-jvm.json and
# host-swift.json. All build output goes to the lab root (lab-env.sh), never to the repository.
# RQ_FFI_WARMUP and RQ_FFI_DOCS change the document counts for a quick try; then the results go
# to $LAB_OUT/ffi-bench/ only, because the evidence needs 200 warm-up and 1000 measured documents.
# Run it with nothing else heavy on the machine. No network after the first build.
set -eu

HERE=$(cd "$(dirname "$0")" && pwd)
ROOT=$(cd "$HERE/../../.." && pwd)
# shellcheck source=../lab-env.sh
. "$ROOT/tools/platform-spikes/lab-env.sh"

usage() {
    echo "usage: sh tools/platform-spikes/ffi-bench/run.sh host [jvm|swift]" >&2
    exit 2
}

[ "${1:-}" = host ] || usage
ONLY=${2:-all}
case "$ONLY" in all | jvm | swift) ;; *) usage ;; esac

WARMUP=${RQ_FFI_WARMUP:-200}
DOCS=${RQ_FFI_DOCS:-1000}
SIZE=1048576
CHUNKS="16384 65536"

OUT="$LAB_OUT/ffi-bench"
mkdir -p "$LAB_HOME/tmp" "$GRADLE_USER_HOME" "$OUT/cases" "$OUT/gen"
if [ "$WARMUP" = 200 ] && [ "$DOCS" = 1000 ]; then
    RESULTS="$ROOT/evidence/spikes/wp-04/ffi"
else
    RESULTS="$OUT/results"
    echo "note: $WARMUP warm-up and $DOCS measured documents; results go to $RESULTS" >&2
fi
mkdir -p "$RESULTS"

LIB_DIR="$CARGO_TARGET_DIR/release"
DYLIB="$LIB_DIR/libffi_bench.dylib"
STATIC="$LIB_DIR/libffi_bench.a"
DATE=$(date -u +%Y-%m-%d)
MACHINE=$(sysctl -n machdep.cpu.brand_string)
OS="macOS $(sw_vers -productVersion)"
UNIFFI=$(awk '/^name = "uniffi"$/ {getline; gsub(/"/, "", $3); print $3}' "$HERE/Cargo.lock")

echo "== Rust: library and UniFFI bindings"
cd "$HERE"
# Inside the crate, so rust-toolchain.toml selects the pinned toolchain.
RUST=$(rustc --version | awk '{print $2}')
# The generator first: building it with the bindgen feature also rebuilds the library, so the
# library build without features comes last.
cargo build --quiet --locked --release --features bindgen --bin uniffi-bindgen
cargo build --quiet --locked --release --lib
"$LIB_DIR/uniffi-bindgen" generate --language kotlin --language swift --no-format \
    --metadata-no-deps --out-dir "$OUT/gen" "$DYLIB" >/dev/null

run_jvm() {
    echo "== JVM harness (UniFFI Kotlin through JNA; JNI direct ByteBuffer and ByteArray)"
    GRADLE=$(ls "$HOME"/.gradle/wrapper/dists/gradle-9.7.1-bin/*/gradle-9.7.1/bin/gradle | head -n 1)
    GRADLE_VERSION=9.7.1
    "$GRADLE" --quiet -p "$HERE/jvm" \
        --project-cache-dir "$OUT/gradle-project" \
        -PrqGenDir="$OUT/gen" -PrqBuildDir="$OUT/jvm-build" \
        -Pkotlin.project.persistent.dir="$OUT/kotlin" \
        installDist
    CP="$OUT/jvm-build/install/ffi-bench-jvm/lib/*"
    # JNA's own native part, unpacked once to a fixed path. By default JNA unpacks it on every
    # start under a new temporary name; under heavy load the first dlopen of each new file
    # blocked for minutes on this host (seen once, 2026-11-08).
    case "$(uname -m)" in
    arm64) JNA_ARCH=darwin-aarch64 ;;
    *) JNA_ARCH=darwin-x86-64 ;;
    esac
    mkdir -p "$OUT/jna"
    unzip -q -o -j "$OUT/jvm-build/install/ffi-bench-jvm/lib/jna-5.19.1.jar" \
        "com/sun/jna/$JNA_ARCH/libjnidispatch.jnilib" -d "$OUT/jna"
    # Fixed heap limit; JNA must load the unpacked file (jna.nounpack: fail, do not unpack).
    harness() {
        "$JAVA_HOME/bin/java" -Xmx512m -Djna.boot.library.path="$OUT/jna" -Djna.nounpack=true \
            -Djna.tmpdir="$LAB_HOME/tmp" -Djava.io.tmpdir="$LAB_HOME/tmp" \
            -cp "$CP" dev.rivqen.spikes.ffi.MainKt --lib "$DYLIB" "$@"
    }
    harness --selfcheck
    harness --versions "$OUT/jvm-versions.json"
    cases=""
    for path in uniffi-kotlin jni-direct jni-bytearray; do
        for chunk in $CHUNKS; do
            f="$OUT/cases/host-jvm-$path-$chunk.json"
            rm -f "$f"
            harness --path "$path" --size "$SIZE" --chunk "$chunk" --warmup "$WARMUP" \
                --docs "$DOCS" --out "$f"
            cases="$cases $f"
        done
    done
    # shellcheck disable=SC2086
    node "$HERE/merge.mjs" --target host-jvm --date "$DATE" --machine "$MACHINE" --os "$OS" \
        --toolchain "rust=$RUST" --toolchain "uniffi=$UNIFFI" --toolchain "gradle=$GRADLE_VERSION" \
        --toolchain-file "$OUT/jvm-versions.json" \
        --out "$RESULTS/host-jvm.json" $cases
}

run_swift() {
    echo "== Swift harness (UniFFI Swift; C header)"
    INC="$OUT/swift-include"
    mkdir -p "$INC"
    cat >"$INC/module.modulemap" <<EOF
module ffi_benchFFI {
    header "$OUT/gen/ffi_benchFFI.h"
    export *
}
module RqBenchC {
    header "$HERE/include/rq_bench.h"
    export *
}
EOF
    BIN="$OUT/swift-bench"
    # Static link of the Rust library (the .a path, so the linker cannot pick the .dylib).
    xcrun swiftc -O -swift-version 6 -module-name FfiBench -I "$INC" \
        -module-cache-path "$OUT/swift-module-cache" \
        "$HERE/swift/Bench.swift" "$HERE/swift/main.swift" "$OUT/gen/ffi_bench.swift" \
        "$STATIC" -o "$BIN"
    SWIFT=$(xcrun swiftc --version 2>&1 | sed -n 's/.*Swift version \([0-9.]*\).*/\1/p' | head -n 1)
    "$BIN" --selfcheck
    cases=""
    for path in uniffi-swift c-swift; do
        for chunk in $CHUNKS; do
            f="$OUT/cases/host-swift-$path-$chunk.json"
            rm -f "$f"
            "$BIN" --path "$path" --size "$SIZE" --chunk "$chunk" --warmup "$WARMUP" \
                --docs "$DOCS" --out "$f"
            cases="$cases $f"
        done
    done
    # shellcheck disable=SC2086
    node "$HERE/merge.mjs" --target host-swift --date "$DATE" --machine "$MACHINE" --os "$OS" \
        --toolchain "rust=$RUST" --toolchain "uniffi=$UNIFFI" --toolchain "swift=$SWIFT" \
        --out "$RESULTS/host-swift.json" $cases
}

case "$ONLY" in
all)
    run_jvm
    run_swift
    ;;
jvm) run_jvm ;;
swift) run_swift ;;
esac
echo "== done: $RESULTS"
