#!/bin/sh
# SPDX-License-Identifier: Apache-2.0
# SPDX-FileCopyrightText: The Rivqen Authors
#
# FFI benchmark of CONTRACT.md section 10.
#
#   sh tools/platform-spikes/ffi-bench/run.sh host [jvm|swift]   Mac host: host-jvm, host-swift
#   sh tools/platform-spikes/ffi-bench/run.sh ios-sim [<os>]     iOS simulator app: ios-sim-<os>
#   sh tools/platform-spikes/ffi-bench/run.sh android [selfcheck] ART on emulator-5602: android-37
#
# Builds the Rust library and the UniFFI bindings, runs the self-checks, then one process per
# case (path x chunk size), and writes evidence/spikes/wp-04/ffi/<target>.json. All build output
# goes to the lab root (lab-env.sh), never to the repository.
# RQ_FFI_WARMUP and RQ_FFI_DOCS change the document counts for a quick try; then the results go
# to $LAB_OUT/ffi-bench/ only, because the evidence needs 200 warm-up and 1000 measured documents.
# Run it with nothing else heavy on the machine. No network after the first build.
#
# ios-sim: uses only the lab simulator rivqen-p2-ffi-<os> (LAB_SIM_TAG=ffi; default os 27.0).
#   It creates and boots that device when it is missing and deletes it at the end
#   (RQ_FFI_KEEP_SIM=1 keeps it). It never touches another simulator and never opens Simulator.app.
# android: needs the lab emulator, started by the lab script (emulator.sh start 37). Every adb
#   call names $LAB_SERIAL (emulator-5602). This script never starts or stops an emulator.
#   RQ_FFI_SERIAL=emulator-5554 RQ_FFI_HUMAN_DEVICE_OK=1 (with RQ_FFI_EMULATOR_SLOT=1) runs once on
#   the human's own 16 KB page emulator (human decision, FFI task only): it pushes only to
#   /data/local/tmp/rivqen-ffi, removes it at the end, and writes android-<sdk>-16k.json.
#   It runs on the device only with RQ_FFI_EMULATOR_SLOT=1 (the architect gave the emulator
#   slot, CONTRACT.md section 11) and only on the AVD rivqen-p2-api37; RQ_FFI_ANDROID_BUILD_ONLY=1
#   builds and stops before the device. NDK: $RQ_FFI_NDK (default: $LAB_SDK/ndk/30.0.16248370).
set -eu

HERE=$(cd "$(dirname "$0")" && pwd)
ROOT=$(cd "$HERE/../../.." && pwd)
# The FFI lane owns the simulators with this tag (CONTRACT.md section 11).
LAB_SIM_TAG=${LAB_SIM_TAG:-ffi}
export LAB_SIM_TAG
# shellcheck source=../lab-env.sh
. "$ROOT/tools/platform-spikes/lab-env.sh"

usage() {
    echo "usage: sh tools/platform-spikes/ffi-bench/run.sh host [jvm|swift] | ios-sim [<os>] | android [selfcheck]" >&2
    exit 2
}

MODE=${1:-}
ARG=${2:-}
case "$MODE" in
host) case "$ARG" in '' | jvm | swift) ;; *) usage ;; esac ;;
ios-sim) case "$ARG" in '' | [0-9]*.[0-9]*) ;; *) usage ;; esac ;;
android) case "$ARG" in '' | selfcheck) ;; *) usage ;; esac ;;
*) usage ;;
esac

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
HOST_CPU=$(sysctl -n machdep.cpu.brand_string)
UNIFFI=$(awk '/^name = "uniffi"$/ {getline; gsub(/"/, "", $3); print $3}' "$HERE/Cargo.lock")
GRADLE_VERSION=$(sed -n 's/^distributionUrl=.*gradle-\([0-9.]*\)-bin\.zip$/\1/p' \
    "$HERE/jvm/gradle/wrapper/gradle-wrapper.properties")

echo "== Rust: host library and UniFFI bindings"
cd "$HERE"
# Inside the crate, so rust-toolchain.toml selects the pinned toolchain.
RUST=$(rustc --version | awk '{print $2}')
# The generator first: building it with the bindgen feature also rebuilds the library, so the
# library build without features comes last.
cargo build --quiet --locked --release --features bindgen --bin uniffi-bindgen
cargo build --quiet --locked --release --lib
"$LIB_DIR/uniffi-bindgen" generate --language kotlin --language swift --no-format \
    --metadata-no-deps --out-dir "$OUT/gen" "$DYLIB" >/dev/null

# Stops when the pinned toolchain has no standard library for the target (no automatic install).
# Load average of the Mac (1, 5, 15 min) before and after the cases of a target; the numbers go
# to the log and to ffi.md (the result file format of section 10 has no member for them).
load() {
    echo "load $1: $(sysctl -n vm.loadavg | tr -d '{}' | awk '{$1 = $1; print}')"
}

need_rust_target() {
    rustup target list --installed | grep -qx "$1" || {
        echo "run.sh: the Rust toolchain $RUST has no target $1; run: rustup target add --toolchain $RUST $1" >&2
        exit 1
    }
}

# The JVM harness through the Gradle wrapper: the wrapper checks the SHA-256 of the Gradle
# distribution, and gradle/verification-metadata.xml the SHA-256 of every plugin and library.
build_jvm() {
    "$HERE/jvm/gradlew" --quiet -p "$HERE/jvm" \
        --project-cache-dir "$OUT/gradle-project" \
        -PrqGenDir="$OUT/gen" -PrqBuildDir="$OUT/jvm-build" \
        -Pkotlin.project.persistent.dir="$OUT/kotlin" \
        installDist
    JVM_BUILD="$OUT/jvm-build"
    JVM_LIB="$JVM_BUILD/install/ffi-bench-jvm/lib"
    JNA_JAR=$(ls "$JVM_LIB"/jna-*.jar)
}

run_jvm() {
    echo "== JVM harness (UniFFI Kotlin through JNA; JNI direct ByteBuffer and ByteArray)"
    build_jvm
    CP="$JVM_LIB/*"
    # JNA's own native part, unpacked once to a fixed path. By default JNA unpacks it on every
    # start under a new temporary name; under heavy load the first dlopen of each new file
    # blocked for minutes on this host (seen once, 2026-11-08).
    case "$(uname -m)" in
    arm64) JNA_ARCH=darwin-aarch64 ;;
    *) JNA_ARCH=darwin-x86-64 ;;
    esac
    mkdir -p "$OUT/jna"
    unzip -q -o -j "$JNA_JAR" "com/sun/jna/$JNA_ARCH/libjnidispatch.jnilib" -d "$OUT/jna"
    # Fixed heap limit; JNA must load the unpacked file (jna.nounpack: fail, do not unpack).
    harness() {
        "$JAVA_HOME/bin/java" -Xmx512m -Djna.boot.library.path="$OUT/jna" -Djna.nounpack=true \
            -Djna.tmpdir="$LAB_HOME/tmp" -Djava.io.tmpdir="$LAB_HOME/tmp" \
            -cp "$CP" dev.rivqen.spikes.ffi.MainKt --lib "$DYLIB" "$@"
    }
    harness --selfcheck
    harness --versions "$OUT/jvm-versions.json"
    load "host-jvm start"
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
    node "$HERE/merge.mjs" --target host-jvm --date "$DATE" --machine "$HOST_CPU" \
        --os "macOS $(sw_vers -productVersion)" \
        --toolchain "rust=$RUST" --toolchain "uniffi=$UNIFFI" --toolchain "gradle=$GRADLE_VERSION" \
        --toolchain-file "$OUT/jvm-versions.json" \
        --out "$RESULTS/host-jvm.json" $cases
    load "host-jvm end"
}

# Module map of the generated UniFFI header and of include/rq_bench.h.
swift_include() {
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
    SWIFT=$(xcrun swiftc --version 2>&1 | sed -n 's/.*Swift version \([0-9.]*\).*/\1/p' | head -n 1)
}

run_swift() {
    echo "== Swift harness (UniFFI Swift; C header)"
    swift_include
    BIN="$OUT/swift-bench"
    # Static link of the Rust library (the .a path, so the linker cannot pick the .dylib).
    xcrun swiftc -O -swift-version 6 -module-name FfiBench -I "$INC" \
        -module-cache-path "$OUT/swift-module-cache" \
        "$HERE/swift/Bench.swift" "$HERE/swift/main.swift" "$OUT/gen/ffi_bench.swift" \
        "$STATIC" -o "$BIN"
    "$BIN" --selfcheck
    load "host-swift start"
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
    node "$HERE/merge.mjs" --target host-swift --date "$DATE" --machine "$HOST_CPU" \
        --os "macOS $(sw_vers -productVersion)" \
        --toolchain "rust=$RUST" --toolchain "uniffi=$UNIFFI" --toolchain "swift=$SWIFT" \
        --out "$RESULTS/host-swift.json" $cases
    load "host-swift end"
}

run_ios_sim() {
    SIM_OS=${ARG:-27.0}
    SIM="sh $ROOT/tools/platform-spikes/lab/sim.sh"
    NAME="$LAB_SIM_NAME_PREFIX-$SIM_OS"
    BUNDLE=dev.rivqen.spikes.ffibench
    case "$NAME" in "$LAB_SIM_PREFIX"-ffi-*) ;; *) echo "run.sh: LAB_SIM_TAG must be ffi, got $NAME" >&2; exit 2 ;; esac
    echo "== iOS simulator app on $NAME (UniFFI Swift; C header)"
    need_rust_target aarch64-apple-ios-sim
    # The app runs on iOS 18.0 and later; the Rust library uses the same minimum.
    IPHONEOS_DEPLOYMENT_TARGET=18.0 cargo build --quiet --locked --release --lib \
        --target aarch64-apple-ios-sim
    swift_include
    APP="$OUT/ios/FfiBench.app"
    rm -rf "$APP"
    mkdir -p "$APP"
    SDK=$(xcrun --sdk iphonesimulator --show-sdk-path)
    xcrun --sdk iphonesimulator swiftc -O -swift-version 6 -module-name FfiBench \
        -target arm64-apple-ios18.0-simulator -sdk "$SDK" -I "$INC" \
        -module-cache-path "$OUT/swift-module-cache-ios" \
        "$HERE/swift/Bench.swift" "$HERE/swift/main.swift" "$OUT/gen/ffi_bench.swift" \
        "$CARGO_TARGET_DIR/aarch64-apple-ios-sim/release/libffi_bench.a" -o "$APP/FfiBench"
    cp "$HERE/swift/ios/Info.plist" "$APP/Info.plist"
    plutil -lint "$APP/Info.plist" >/dev/null
    # Ad-hoc signature (simulator only; no team, no entitlement).
    codesign --force --sign - --timestamp=none "$APP" 2>/dev/null

    created=0
    if [ -z "$($SIM list | awk -v n="$NAME" '$2 == n')" ]; then
        $SIM create "$SIM_OS"
        created=1
    fi
    cleanup_sim() {
        if [ "${RQ_FFI_KEEP_SIM:-0}" != 1 ]; then
            $SIM delete "$SIM_OS" || echo "run.sh: delete $NAME failed; run: LAB_SIM_TAG=ffi sh tools/platform-spikes/lab/sim.sh delete $SIM_OS" >&2
        fi
    }
    trap cleanup_sim EXIT
    $SIM boot "$SIM_OS"
    UDID=$($SIM list | awk -v n="$NAME" '$2 == n { print $1 }')
    [ -n "$UDID" ] || { echo "run.sh: no device $NAME" >&2; exit 1; }
    echo "device $NAME $UDID (created by this run: $created)"
    xcrun simctl install "$UDID" "$APP"
    DATA=$(xcrun simctl get_app_container "$UDID" "$BUNDLE" data)
    mkdir -p "$DATA/Documents"

    # One app process per case: launch, wait for the status file, check it. The status file is
    # the only verdict: when the app exits before launch returns, simctl reports "did not return
    # a process handle" (exit 3) although the app ran. Wait at most 600 s after a clean launch,
    # 30 s after a launch error.
    app() {
        rm -f "$DATA/Documents/status.txt"
        limit=600
        if ! xcrun simctl launch --console-pty --terminate-running-process "$UDID" "$BUNDLE" \
            "$@" --status status.txt; then
            limit=30
        fi
        i=0
        while [ ! -s "$DATA/Documents/status.txt" ]; do
            i=$((i + 1))
            [ $i -le $limit ] || { echo "run.sh: no status from the app in $limit s" >&2; exit 1; }
            sleep 1
        done
        st=$(cat "$DATA/Documents/status.txt")
        [ "$st" = ok ] || { echo "run.sh: app: $st" >&2; exit 1; }
    }
    app --selfcheck
    echo "self-check ok (iOS simulator)"
    RUNTIME=$(xcrun simctl list runtimes -j | node -e '
        const r = JSON.parse(require("fs").readFileSync(0, "utf8")).runtimes;
        const m = r.find((x) => x.platform === "iOS" && x.version === process.argv[1] && x.isAvailable);
        console.log(m ? `iOS ${m.version} (${m.buildversion}) simulator` : "");
    ' "$SIM_OS")
    [ -n "$RUNTIME" ] || { echo "run.sh: no runtime $SIM_OS" >&2; exit 1; }
    XCODE=$(xcodebuild -version | awk 'NR == 1 { print $2 }')
    load "ios-sim-$SIM_OS start"
    cases=""
    for path in uniffi-swift c-swift; do
        for chunk in $CHUNKS; do
            name="ios-sim-$path-$chunk.json"
            f="$OUT/cases/ios-sim-$SIM_OS-$path-$chunk.json"
            rm -f "$f" "$DATA/Documents/$name"
            app --path "$path" --size "$SIZE" --chunk "$chunk" --warmup "$WARMUP" \
                --docs "$DOCS" --out "$name"
            cp "$DATA/Documents/$name" "$f"
            cases="$cases $f"
        done
    done
    # shellcheck disable=SC2086
    node "$HERE/merge.mjs" --target "ios-sim-$SIM_OS" --date "$DATE" \
        --machine "$HOST_CPU (iOS simulator, $(echo "$LAB_SIM_DEVICE_TYPE" | sed 's/.*SimDeviceType\.//'))" \
        --os "$RUNTIME" \
        --toolchain "rust=$RUST" --toolchain "uniffi=$UNIFFI" --toolchain "swift=$SWIFT" \
        --toolchain "xcode=$XCODE" \
        --out "$RESULTS/ios-sim-$SIM_OS.json" $cases
    load "ios-sim-$SIM_OS end"
}

run_android() {
    NDK=${RQ_FFI_NDK:-$LAB_SDK/ndk/30.0.16248370}
    # d8 of build-tools 36.0.0 (8.10) warns on the Kotlin 2.4 metadata of every stdlib class;
    # 37.0.0 (d8 9.2) does not.
    BUILD_TOOLS=${RQ_FFI_BUILD_TOOLS:-$LAB_SDK/build-tools/37.0.0}
    ANDROID_JAR=${RQ_FFI_ANDROID_JAR:-$LAB_SDK/platforms/android-37.0/android.jar}
    # Device: the lab emulator, or (one-time human decision of 2026-11-08, FFI task only) the
    # human's own emulator-5554 with a 16 KB page image. Every other serial is refused.
    SERIAL=${RQ_FFI_SERIAL:-$LAB_SERIAL}
    case "$SERIAL" in
    "$LAB_SERIAL") HUMAN=0 ;;
    emulator-5554)
        [ "${RQ_FFI_HUMAN_DEVICE_OK:-0}" = 1 ] || {
            echo "run.sh: emulator-5554 is the human's device; it needs RQ_FFI_HUMAN_DEVICE_OK=1 (human decision)" >&2
            exit 2
        }
        HUMAN=1
        ;;
    *) echo "run.sh: refusing device $SERIAL (only $LAB_SERIAL, or emulator-5554 with RQ_FFI_HUMAN_DEVICE_OK=1)" >&2; exit 2 ;;
    esac
    ADB="$LAB_ADB -s $SERIAL"
    D=/data/local/tmp/rivqen-ffi
    [ -f "$NDK/source.properties" ] || { echo "run.sh: no NDK at $NDK" >&2; exit 1; }
    [ -x "$BUILD_TOOLS/d8" ] || { echo "run.sh: no d8 in $BUILD_TOOLS" >&2; exit 1; }
    [ -f "$ANDROID_JAR" ] || { echo "run.sh: no $ANDROID_JAR" >&2; exit 1; }
    NDK_VERSION=$(sed -n 's/^Pkg.Revision *= *//p' "$NDK/source.properties")
    need_rust_target aarch64-linux-android

    echo "== Android: Rust library (cargo ndk, NDK $NDK_VERSION, arm64-v8a, API 29+)"
    # 16 KB pages (android.md section 6): NDK r28 and later align LOAD segments to 16 KB by
    # default; the flag makes it explicit, and the readelf check below proves it.
    ANDROID_NDK_HOME="$NDK" \
        CARGO_TARGET_AARCH64_LINUX_ANDROID_RUSTFLAGS="-C link-arg=-Wl,-z,max-page-size=16384" \
        cargo ndk -t arm64-v8a -P 29 build --quiet --locked --release --lib
    SO="$CARGO_TARGET_DIR/aarch64-linux-android/release/libffi_bench.so"

    echo "== Android: JVM harness as DEX (d8 $(basename "$BUILD_TOOLS"))"
    build_jvm
    A="$OUT/android"
    rm -rf "$A"
    mkdir -p "$A/aar"
    # JNA for Android is the AAR of the same release (Gradle verified it): its classes and its
    # arm64-v8a libjnidispatch.so. The desktop jar of JNA is not dexed.
    unzip -q -o "$JVM_BUILD/jna-android/jna-5.19.1.aar" classes.jar jni/arm64-v8a/libjnidispatch.so -d "$A/aar"
    cp "$A/aar/jni/arm64-v8a/libjnidispatch.so" "$A/libjnidispatch.so"
    # shellcheck disable=SC2046
    "$BUILD_TOOLS/d8" --release --min-api 29 --lib "$ANDROID_JAR" --output "$A/bench.jar" \
        $(ls "$JVM_LIB"/*.jar | grep -v '/jna-[^/]*\.jar$') "$A/aar/classes.jar"
    READELF=$(ls "$NDK"/toolchains/llvm/prebuilt/*/bin/llvm-readelf | head -n 1)
    for so in "$SO" "$A/libjnidispatch.so"; do
        aligns=$("$READELF" -lW "$so" | awk '$1 == "LOAD" { print $NF }' | sort -u)
        [ "$aligns" = 0x4000 ] || { echo "run.sh: LOAD alignment of $so is not 16 KB: $aligns" >&2; exit 1; }
        echo "$(basename "$so"): LOAD segments aligned to 16 KB"
    done

    if [ "${RQ_FFI_ANDROID_BUILD_ONLY:-0}" = 1 ]; then
        echo "== Android: build only (RQ_FFI_ANDROID_BUILD_ONLY=1); $A is ready"
        return 0
    fi
    # The emulator is shared by the lanes (CONTRACT.md section 11): a running emulator is not
    # a permission to use it. The architect gives the slot; the operator then sets
    # RQ_FFI_EMULATOR_SLOT=1. The AVD must be the API 37 lab AVD (the target android-37).
    [ "${RQ_FFI_EMULATOR_SLOT:-0}" = 1 ] || {
        echo "run.sh: no emulator slot; set RQ_FFI_EMULATOR_SLOT=1 only when the architect gave the slot (or RQ_FFI_ANDROID_BUILD_ONLY=1)" >&2
        exit 1
    }
    echo "== Android: device $SERIAL"
    status=$(sh "$ROOT/tools/platform-spikes/lab/emulator.sh" status)
    if [ "$HUMAN" = 0 ]; then
        case "$status" in
        running*"$LAB_AVD_37"*) ;;
        *)
            echo "run.sh: need the lab emulator $LAB_AVD_37 on $LAB_SERIAL, got: $status" >&2
            echo "run.sh: start it: sh tools/platform-spikes/lab/emulator.sh start 37" >&2
            exit 1
            ;;
        esac
    else
        # Never both at the same time; the human's emulator is never started, stopped or changed.
        case "$status" in
        running*) echo "run.sh: stop the lab emulator first (one emulator run at a time)" >&2; exit 1 ;;
        esac
        $LAB_ADB devices | awk -v s="$SERIAL" '$1 == s && $2 == "device" { f = 1 } END { exit !f }' || {
            echo "run.sh: $SERIAL is not connected; this script does not start it" >&2
            exit 1
        }
    fi
    API=$($ADB shell getprop ro.build.version.sdk | tr -d '\r')
    ABI=$($ADB shell getprop ro.product.cpu.abi | tr -d '\r')
    REL=$($ADB shell getprop ro.build.version.release | tr -d '\r')
    BUILD_ID=$($ADB shell getprop ro.build.id | tr -d '\r')
    PAGE=$($ADB shell getconf PAGE_SIZE | tr -d '\r')
    HEAP="growth limit $($ADB shell getprop dalvik.vm.heapgrowthlimit | tr -d '\r'), max $($ADB shell getprop dalvik.vm.heapsize | tr -d '\r')"
    [ "$ABI" = arm64-v8a ] || { echo "run.sh: ABI $ABI, need arm64-v8a" >&2; exit 1; }
    echo "Android $REL (API $API, $BUILD_ID), $ABI, page size $PAGE, ART heap $HEAP"
    TARGET="android-$API"
    MACHINE="Android emulator $ABI on $HOST_CPU"
    if [ "$HUMAN" = 1 ]; then
        # The human's Play image with 16 KB pages: a separate, supplementary result file.
        [ "$PAGE" = 16384 ] || { echo "run.sh: $SERIAL has page size $PAGE, want 16384" >&2; exit 1; }
        FULL=$($ADB shell getprop ro.build.version.sdk_full | tr -d '\r')
        PRODUCT=$($ADB shell getprop ro.product.name | tr -d '\r')
        TARGET="android-${FULL:-$API}-16k"
        MACHINE="Android emulator $ABI ($PRODUCT, the human's Play image) on $HOST_CPU"
    fi
    # The cleanup runs on every exit after this point, also when a push fails.
    cleanup_android() { $ADB shell rm -rf "$D" || echo "run.sh: remove $D on $SERIAL failed" >&2; }
    trap cleanup_android EXIT
    $ADB shell rm -rf "$D"
    $ADB shell mkdir -p "$D"
    $ADB push -q "$SO" "$A/libjnidispatch.so" "$A/bench.jar" "$D/"
    # The harness on ART: app_process starts the runtime as an app process does (heap limits
    # of dalvik.vm.*), as the shell user, from the DEX file. JNA loads its native part from $D.
    harness() {
        $ADB shell "CLASSPATH=$D/bench.jar app_process -Djna.boot.library.path=$D -Djna.nounpack=true -Djava.io.tmpdir=$D /system/bin dev.rivqen.spikes.ffi.MainKt --lib $D/libffi_bench.so $*"
    }
    harness --selfcheck
    echo "self-check ok (ART, API $API)"
    [ "$ARG" != selfcheck ] || return 0
    harness --versions "$D/versions.json"
    $ADB pull -q "$D/versions.json" "$OUT/android-versions.json"
    load "$TARGET start"
    cases=""
    for path in uniffi-kotlin jni-direct jni-bytearray; do
        for chunk in $CHUNKS; do
            f="$OUT/cases/$TARGET-$path-$chunk.json"
            rm -f "$f"
            harness --path "$path" --size "$SIZE" --chunk "$chunk" --warmup "$WARMUP" \
                --docs "$DOCS" --out "$D/case.json"
            $ADB pull -q "$D/case.json" "$f"
            cases="$cases $f"
        done
    done
    # shellcheck disable=SC2086
    node "$HERE/merge.mjs" --target "$TARGET" --date "$DATE" \
        --machine "$MACHINE" --os "Android $REL (API $API, $BUILD_ID), page size $PAGE" \
        --toolchain "rust=$RUST" --toolchain "uniffi=$UNIFFI" --toolchain "gradle=$GRADLE_VERSION" \
        --toolchain "ndk=$NDK_VERSION" --toolchain "d8=$(basename "$BUILD_TOOLS")" \
        --toolchain-file "$OUT/android-versions.json" \
        --out "$RESULTS/$TARGET.json" $cases
    load "$TARGET end"
}

case "$MODE" in
host)
    case "$ARG" in
    '') run_jvm; run_swift ;;
    jvm) run_jvm ;;
    swift) run_swift ;;
    esac
    ;;
ios-sim) run_ios_sim ;;
android) run_android ;;
esac
echo "== done: $RESULTS"
