# FFI benchmark (WP-04)

<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- SPDX-FileCopyrightText: The Rivqen Authors -->

Evidence code for ADR-002 and Q-06: the cost of moving a document from Rust to Kotlin and Swift through UniFFI and through a hand-written C ABI. The interface and the result file are fixed in [`CONTRACT.md`](../CONTRACT.md) section 10. This is not an SDK API. The results are indicative (Mac host, H-129). The method and the numbers are in [FFI boundary](../../../docs/engineering/core/ffi.md), section "WP-04 benchmark".

## Run

```sh
sh tools/platform-spikes/ffi-bench/run.sh host          # JVM and Swift
sh tools/platform-spikes/ffi-bench/run.sh host jvm      # one harness
```

1. Run it on the Mac host with nothing else heavy on the machine.
2. The script sources `../lab-env.sh`. Cargo, Gradle, Kotlin and Swift output goes to `$LAB_HOME` (`/Volumes/BackCat/rivqen-p2-lab`), not to the repository.
3. The first run downloads the crates and the Gradle plugins. After that the benchmark needs no network.
4. `RQ_FFI_WARMUP` and `RQ_FFI_DOCS` set smaller counts for a quick try. Then the results go to `$LAB_OUT/ffi-bench/results/`, not to `evidence/`.
5. `JAVA_HOME` must be a JDK 21. `lab-env.sh` sets it only when it is not set already.
6. The script unpacks the native part of JNA once to `$LAB_OUT/ffi-bench/jna` and starts the JVM with `jna.nounpack=true`. With the default (a new temporary file on each start), one JVM start blocked for minutes in `dlopen` under heavy load on the host.

Rust checks (in this directory):

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

## Layout

| Path | Content |
|---|---|
| `src/doc.rs` | Document (byte `i` is `(i * 31 + 7) mod 251`), pull cursor, chunk rules, property test |
| `src/sha256.rs` | SHA-256 from FIPS 180-4 (no hash crate in DL-137) |
| `src/registry.rs` | Handle table of the C ABI and JNI paths (at most 1024 open streams) |
| `src/api.rs` | UniFFI path: `open_stream`, `DocStream::next_chunk`, `doc_sha256` |
| `src/abi/` | The only module with `unsafe`: C ABI (`c.rs`), JNI glue (`jni.rs`), `getrusage` (`sys.rs`) |
| `include/rq_bench.h` | Hand-written C header (no cbindgen, DL-137) |
| `jvm/` | JVM harness (Kotlin): UniFFI through JNA, JNI with a direct `ByteBuffer`, JNI with a `ByteArray` |
| `swift/` | Swift harness: UniFFI Swift, C header |
| `run.sh`, `merge.mjs` | Build, self-checks, one process per case, result files |

## Interface

| Path | Calls per chunk | Copies of each document byte |
|---|---|---|
| `uniffi-kotlin` | `nextChunk()`: clone handle, method, `rustbuffer_free` (3 JNA calls) | 3: slice to `Vec` (Rust); `Vec` to `RustBuffer` (UniFFI `Vec<T>::write`, byte by byte); `RustBuffer` to `ByteArray` (`FfiConverterByteArray.read`) |
| `jni-direct` | `readDirect(handle, ByteBuffer)` | 1: Rust writes into the direct buffer memory |
| `jni-bytearray` | `readArray(handle, ByteArray)` | 1: `SetByteArrayRegion` from the Rust slice |
| `uniffi-swift` | `nextChunk()`: clone handle, method | 4: slice to `Vec`; `Vec` to `RustBuffer`; `readBytes` to `[UInt8]`; `Data(...)` |
| `c-swift` | `rq_bench_read(h, buf, cap, &n)` | 1: Rust writes into a Swift-owned buffer |

C ABI rules (`include/rq_bench.h`):

- `RqBenchStream *` is an opaque handle: a key of the handle table, never a memory address. A null, closed, stale or forged handle gives `RQ_BENCH_E_HANDLE` (-1). `rq_bench_close` twice gives `RQ_BENCH_E_HANDLE` the second time, not a double free.
- `rq_bench_read` writes at most `min(chunk, cap)` bytes; `cap == 0` gives `RQ_BENCH_E_ARG`; a null `buf` or `out_len` gives `RQ_BENCH_E_NULL`. The end gives `RQ_BENCH_END` with `*out_len == 0`, also on every later call.
- No panic crosses the boundary: every exported function (C and JNI) runs in `catch_unwind` and gives `RQ_BENCH_E_PANIC`. The release profile keeps `panic = "unwind"`.
- `rq_bench_close` returns `int32_t` (the contract does not name a return type).
- `chunk` is `1..=65536`; `size` is at most 256 MiB. A bad value gives a null handle. The UniFFI `open_stream` has no error in the contract, so a bad value gives a stream that is already at its end.

## Method

- Each case runs in a new process: 200 warm-up documents, then 1000 measured documents of 1 MiB, in 16 KiB or 64 KiB chunks.
- The time of a document is the sum of the timed segments: open, each read (also the last read, which reports the end) and close. The SHA-256 update of a chunk is between two segments and is not timed. Each segment includes one clock-read pair. Clocks: `System.nanoTime` (JVM) and `clock_gettime_nsec_np(CLOCK_UPTIME_RAW)` (Swift); on Apple silicon both have steps of about 41.7 ns (24 MHz).
- `median_ns` and `p95_ns`: nearest rank over the 1000 document times. `chunk_median_ns`: nearest-rank median of the reads that returned data.
- `copied_bytes`: the copies of the table "Interface" for one document. The Rust part is counted at run time (`copied_bytes()` / `rq_bench_copied` / `NativeBench.copied`, must equal the size); the copies inside generated UniFFI code are counted from the generated sources and multiplied by the bytes the harness received (must equal the size). `Vec` growth inside `Vec<T>::write` can move the buffer again; that move is not counted (it depends on the allocator).
- `rss_delta_kib`: `getrusage(RUSAGE_SELF).ru_maxrss` (bytes on Darwin) after the measured documents minus before the warm-up, in KiB. It is a peak, so it shows growth only. The JVM reads it through JNI (`NativeBench.peakRssBytes`).
- SHA-256: each harness hashes every document with the platform (Java `MessageDigest`, Swift `CryptoKit`) and compares it with the Rust digest; it also compares the Rust digest with a digest of the formula computed on the platform. A mismatch stops the run (exit 1), and `merge.mjs` refuses a case without `sha256_ok: true`.
- Self-checks before the cases: null, negative, forged and closed handles; close twice; a heap `ByteBuffer`; a buffer of 0 bytes; the short last chunk; the end; the UniFFI call after `close` (an `IllegalStateException`).

## S2 (emulator and simulator)

`Bench.kt` and `Bench.swift` use only APIs that Android and iOS have. S2 builds the library with `cargo ndk` (Android) and for `aarch64-apple-ios-sim`, runs the same core in an instrumented test and in a simulator app, and writes `android-37.json` and `ios-sim-27.0.json`. `src/abi/sys.rs` already reads `ru_maxrss` in KiB on Linux and Android.

## Dependencies

| Component | Version | License | Use |
|---|---|---|---|
| `uniffi` (and `uniffi_*`) | =0.32.2 | MPL-2.0 (approved exception, benchmark only) | UniFFI path; `uniffi-bindgen` (feature `bindgen`) |
| `jni` | =0.22.4 | MIT OR Apache-2.0 | JNI glue |
| `proptest` | =1.11.0 | MIT OR Apache-2.0 | Property test (dev) |
| JNA `net.java.dev.jna:jna` | 5.19.1 | Apache-2.0 option of LGPL-2.1+ OR Apache-2.0 | UniFFI Kotlin runtime |
| Kotlin Gradle plugin | 2.4.20 | Apache-2.0 | Local toolchain |
| Gradle | 9.7.1 | Apache-2.0 | Local toolchain (`~/.gradle/wrapper/dists`) |

Every transitive crate is MIT, Apache-2.0, `Unlicense OR MIT`, Unicode-3.0 or MPL-2.0 (`uniffi*` only). The crate has its own `[workspace]` and `Cargo.lock` (DL-084) and the toolchain pin 1.99.0.
