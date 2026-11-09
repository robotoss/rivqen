# FFI benchmark (WP-04)

<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- SPDX-FileCopyrightText: The Rivqen Authors -->

Evidence code for ADR-002 and Q-06: the cost of moving a document from Rust to Kotlin and Swift through UniFFI and through a hand-written C ABI. The interface and the result file are fixed in [`CONTRACT.md`](../CONTRACT.md) section 10. This is not an SDK API. The results are indicative (Mac host, simulator, emulator; H-129). The method and the numbers are in [FFI boundary](../../../docs/engineering/core/ffi.md), section "WP-04 benchmark".

## Run

```sh
sh tools/platform-spikes/ffi-bench/run.sh host            # JVM and Swift on the Mac
sh tools/platform-spikes/ffi-bench/run.sh host jvm        # one harness
sh tools/platform-spikes/ffi-bench/run.sh ios-sim 27.0    # Swift app on rivqen-p2-ffi-27.0
RQ_FFI_ANDROID_BUILD_ONLY=1 sh tools/platform-spikes/ffi-bench/run.sh android   # build only
RQ_FFI_EMULATOR_SLOT=1 sh tools/platform-spikes/ffi-bench/run.sh android selfcheck
RQ_FFI_EMULATOR_SLOT=1 sh tools/platform-spikes/ffi-bench/run.sh android
```

1. Run it with nothing else heavy on the machine. Write the load average down with the numbers.
2. The script sources `../lab-env.sh`. Cargo, Gradle, Kotlin, Swift and the app go to `$LAB_HOME` (`/Volumes/BackCat/rivqen-p2-lab`), not to the repository.
3. The first run downloads the crates, the Gradle distribution and the Gradle plugins. After that the benchmark needs no network.
4. `RQ_FFI_WARMUP` and `RQ_FFI_DOCS` set smaller counts for a quick try. Then the results go to `$LAB_OUT/ffi-bench/results/`, not to `evidence/`.
5. `JAVA_HOME` must be a JDK 21. `lab-env.sh` sets it.
6. The script unpacks the native part of JNA once to `$LAB_OUT/ffi-bench/jna` and starts the JVM with `jna.nounpack=true`. With the default (a new temporary file on each start), one JVM start blocked for minutes in `dlopen` under heavy load on the host.

iOS simulator (`ios-sim`):

1. The script sets `LAB_SIM_TAG=ffi`. It uses only the device `rivqen-p2-ffi-<os>` (default `27.0`). It refuses another tag.
2. It creates and boots the device when it is missing (`lab/sim.sh`), and deletes it at the end, also after an error. `RQ_FFI_KEEP_SIM=1` keeps it. It never opens Simulator.app.
3. It builds the Rust library for `aarch64-apple-ios-sim`, links the same `Bench.swift` and `main.swift` into `FfiBench.app` (iOS 18.0 and later, ad-hoc signature), installs it, and starts one app process per case with `simctl launch`.
4. The app writes its result and a status file (`ok`, `FAIL: …`, `usage: …`) into its `Documents` folder. The status file is the verdict: `simctl launch` gives no exit code, and it reports "did not return a process handle" when the app exits before the launch returns.
5. The pinned toolchain needs the target: `rustup target add --toolchain 1.99.0 aarch64-apple-ios-sim`.

Android (`android`):

1. The emulator is shared (CONTRACT.md section 11). The script uses it only with `RQ_FFI_EMULATOR_SLOT=1`, which the operator sets when the architect gave the slot, and only when `lab/emulator.sh status` shows the AVD `rivqen-p2-api37`. It never starts or stops an emulator. Start it with `sh tools/platform-spikes/lab/emulator.sh start 37`.
2. It builds `libffi_bench.so` with `cargo ndk -t arm64-v8a -P 29` and NDK r30 from the lab SDK (`$RQ_FFI_NDK`; default `$LAB_SDK/ndk/30.0.16248370`), and stops when a `LOAD` segment of `libffi_bench.so` or `libjnidispatch.so` is not aligned to 16 KB.
3. It converts the JVM harness to DEX with `d8` of build-tools 37.0.0 (`--min-api 29`). JNA for Android comes from the AAR of the same JNA release: its classes and its `arm64-v8a/libjnidispatch.so`.
4. It pushes the files to `/data/local/tmp/rivqen-ffi` on `emulator-5602`, runs `--selfcheck` on ART first, then one `app_process` per case (the runtime starts as for an app, with the `dalvik.vm.*` heap limits), and removes the folder at the end. Every `adb` call has `-s emulator-5602`.
5. The pinned toolchain needs the target: `rustup target add --toolchain 1.99.0 aarch64-linux-android`.
6. Supplementary run on the human's own emulator with 16 KB pages (one-time human decision, 2026-11-08, FFI task only): `RQ_FFI_EMULATOR_SLOT=1 RQ_FFI_SERIAL=emulator-5554 RQ_FFI_HUMAN_DEVICE_OK=1 sh tools/platform-spikes/ffi-bench/run.sh android [selfcheck]`. The script refuses every other serial, refuses while the lab emulator runs, never starts, stops or changes that emulator, stops when `getconf PAGE_SIZE` is not 16384, writes only `/data/local/tmp/rivqen-ffi` and removes it on every exit. The result is a separate file `android-<sdk>-16k.json`, not the main `android-37` result.
7. Every target prints the load average (1, 5, 15 min) before and after its cases (`load <target> start|end`). The result file has no member for it, so write it in `ffi.md`.

Rust checks (in this directory):

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
node --test merge.test.mjs
```

## Layout

| Path | Content |
|---|---|
| `src/doc.rs` | Document (byte `i` is `(i * 31 + 7) mod 251`), pull cursor, chunk rules, property test |
| `src/sha256.rs` | SHA-256 from FIPS 180-4 (no hash crate in DL-137) |
| `src/registry.rs` | Handle table of the C ABI and JNI paths (at most 1024 open streams) |
| `src/api.rs` | UniFFI path: `open_stream`, `DocStream::next_chunk`, `doc_sha256` |
| `src/abi/` | The only module with `unsafe`: C ABI (`c.rs`), JNI glue (`jni.rs`), `getrusage` (`sys.rs`) |
| `include/rq_bench.h` | Hand-written C header with the pointer contracts (no cbindgen, DL-137) |
| `jvm/` | JVM harness (Kotlin): UniFFI through JNA, JNI with a direct `ByteBuffer`, JNI with a `ByteArray`; Gradle wrapper and dependency verification |
| `swift/` | Swift harness: UniFFI Swift, C header; `swift/ios/Info.plist` for the simulator app |
| `run.sh`, `merge.mjs`, `merge.test.mjs` | Build, self-checks, one process per case, result files |

## Interface

| Path | Calls per chunk | Copies of each document byte | Live copies of a chunk |
|---|---|---|---|
| `uniffi-kotlin` | `nextChunk()`: clone handle, method, `rustbuffer_free` (3 JNA calls) | 3: slice to `Vec` (Rust); `Vec` to `RustBuffer` (UniFFI `Vec<T>::write`, byte by byte); `RustBuffer` to `ByteArray` (`FfiConverterByteArray.read`) | 2 during the call; then 1 `ByteArray` per chunk that stays until the collector frees it |
| `jni-direct` | `NativeBench.readDirect(handle, ByteBuffer)`: `isReadOnly` in Kotlin, then the JNI function, which calls `isReadOnly()` again | 1: Rust writes into the direct buffer memory | 1 (the reused buffer) |
| `jni-bytearray` | `readArray(handle, ByteArray)` | 1: `SetByteArrayRegion` from the Rust slice | 1 (the reused array) |
| `uniffi-swift` | `nextChunk()`: clone handle, method | 4: slice to `Vec`; `Vec` to `RustBuffer`; `readBytes` to `[UInt8]`; `Data(...)` | 3 during the call (`RustBuffer`, `[UInt8]`, `Data`); then 1 `Data`, freed when the harness drops it |
| `c-swift` | `rq_bench_read(h, buf, cap, &n)` | 1: Rust writes into a Swift-owned buffer | 1 (the reused buffer) |

"Live copies" counts the buffers that hold the bytes of one chunk at the same time, without the document itself (a static table in Rust). It is counted from the code: `src/api.rs`, UniFFI 0.32.2 `Vec<T>::write` (the source `Vec` lives until the write ends), and the generated `FfiConverterByteArray.read` and `liftFromRustBuffer` (Kotlin), `FfiConverterData.read`, `readBytes` and `lift` (Swift).

C ABI rules (`include/rq_bench.h`):

- `RqBenchStream *` is an opaque handle: a key of the handle table, never a memory address. A null, closed, stale or forged handle gives `RQ_BENCH_E_HANDLE` (-1). `rq_bench_close` twice gives `RQ_BENCH_E_HANDLE` the second time, not a double free.
- `rq_bench_read` writes at most `min(chunk, cap)` bytes; `cap == 0` gives `RQ_BENCH_E_ARG`; a null `buf` or `out_len` gives `RQ_BENCH_E_NULL`. The end gives `RQ_BENCH_END` with `*out_len == 0`, also on every later call.
- The header has the pointer contracts of the Rust `# Safety` sections (review L-5): each pointer is NULL or valid for the stated writes for the whole call, no other thread uses that memory during the call, `buf` and `out_len` do not overlap, the library keeps no pointer. The test `header_has_the_pointer_contracts` keeps the header and `src/abi/c.rs` the same.
- No panic crosses the boundary: every exported function (C and JNI) runs in `catch_unwind` and gives `RQ_BENCH_E_PANIC`. The release profile keeps `panic = "unwind"`.
- `rq_bench_close` returns `int32_t` (the contract does not name a return type).
- `chunk` is `1..=65536`; `size` is at most 256 MiB. A bad value gives a null handle. The UniFFI `open_stream` has no error in the contract, so a bad value gives a stream that is already at its end.

JNI rules (`src/abi/jni.rs`):

- `readDirectRaw` gives `E_NULL` for a null buffer and `E_ARG` for a read-only buffer (review L-1): JNI `GetDirectBufferAddress` gives the address of a read-only direct buffer too. It calls `ByteBuffer.isReadOnly()` through a method id that it looks up once (the boot class loader never unloads `ByteBuffer`). A heap buffer gives `E_JNI` (no address).
- The Kotlin wrapper `NativeBench.readDirect` checks `isReadOnly` before the native call. The self-check calls both, also with a read-only heap buffer and a null buffer (through reflection).

## Method

- Each case runs in a new process: 200 warm-up documents, then 1000 measured documents of 1 MiB, in 16 KiB or 64 KiB chunks.
- The time of a document is the sum of the timed segments: open, each read (also the last read, which reports the end) and close. The SHA-256 update of a chunk is between two segments and is not timed. Each segment includes one clock-read pair. Clocks: `System.nanoTime` (JVM, ART) and `clock_gettime_nsec_np(CLOCK_UPTIME_RAW)` (Swift); on Apple silicon both have steps of about 41.7 ns (24 MHz).
- `median_ns` and `p95_ns`: nearest rank over the 1000 document times. `chunk_median_ns`: nearest-rank median of the reads that returned data.
- `copied_bytes = rust_copied_bytes + glue_copied_bytes` for one document. `rust_copied_bytes` is counted at run time (`copied_bytes()` / `rq_bench_copied` / `NativeBench.copied`, must equal the size). `glue_copied_bytes` are the copies inside the generated UniFFI code, counted from the generated sources and multiplied by the bytes the harness received (must equal the size). `Vec` growth inside `Vec<T>::write` can move the buffer again; that move is not counted (it depends on the allocator). `merge.mjs` refuses a split that does not add up.
- `rss_delta_kib`: `getrusage(RUSAGE_SELF).ru_maxrss` after the measured documents minus before the warm-up, in KiB (bytes on Darwin, KiB on Linux and Android). It is a peak, so it shows growth only. The JVM reads it through JNI (`NativeBench.peakRssBytes`).
- SHA-256: each harness hashes every document with the platform (Java `MessageDigest`, Swift `CryptoKit`) and compares it with the Rust digest; it also compares the Rust digest with a digest of the formula computed on the platform. A mismatch stops the run (exit 1), and `merge.mjs` refuses a case without `sha256_ok: true` and cases with different `docs`.
- Bounds: the harnesses refuse a chunk outside `1..=65536`, a size outside `1..=256 MiB` and more than 2^24 chunk times (usage error, no buffer is made).
- Self-checks before the cases: null, negative, forged and closed handles; close twice; a heap `ByteBuffer`; a read-only direct and heap `ByteBuffer`; a null `ByteBuffer`; a buffer of 0 bytes; the short last chunk; the end; the UniFFI call after `close` (an `IllegalStateException`).

## Dependencies

| Component | Version | License | Use |
|---|---|---|---|
| `uniffi` (and `uniffi_*`) | =0.32.2 | MPL-2.0 (approved exception, benchmark only) | UniFFI path; `uniffi-bindgen` (feature `bindgen`) |
| `jni` | =0.22.4 | MIT OR Apache-2.0 | JNI glue |
| `proptest` | =1.11.0 | MIT OR Apache-2.0 | Property test (dev) |
| JNA `net.java.dev.jna:jna` | 5.19.1 (`.jar` on the JVM, `.aar` on Android) | Apache-2.0 option of LGPL-2.1+ OR Apache-2.0 | UniFFI Kotlin runtime |
| Kotlin Gradle plugin | 2.4.20 | Apache-2.0 | Local toolchain |
| Gradle | 9.7.1 (wrapper) | Apache-2.0 | Local toolchain |

Every transitive crate is MIT, Apache-2.0, `Unlicense OR MIT`, Unicode-3.0 or MPL-2.0 (`uniffi*` only). The crate has its own `[workspace]` and `Cargo.lock` (DL-084) and the toolchain pin 1.99.0.

Supply chain of the JVM harness (review L-3):

- `jvm/gradlew` is the Gradle 9.7.1 wrapper. `gradle-wrapper.properties` has `distributionSha256Sum` (`acd53f1e…`, from `services.gradle.org`), so the wrapper refuses another distribution. `gradle-wrapper.jar` has the published SHA-256 of the 9.7.1 wrapper (`7a9ce74c…`).
- `jvm/gradle/verification-metadata.xml` has the SHA-256 of every plugin, library and POM of the build (`--write-verification-metadata sha256`). Gradle stops when an artifact differs. The values came from the lab Gradle cache; the SHA-1 of the JNA jar and AAR, `kotlin-stdlib` and `annotations` were the same as on Maven Central (checked 2026-11-08). To add an artifact, run `./gradlew --write-verification-metadata sha256 installDist` with the `run.sh` properties and review the diff.

Lab tools (not in the repository): NDK r30 (`ndk;30.0.16248370`, 2.8 GB) and build-tools 37.0.0 (192 MB), installed into the lab SDK with `sdkmanager`.
