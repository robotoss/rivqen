# FFI boundary

The FFI boundary connects the Rust core to Kotlin and Swift. It is the most safety-critical code in Rivqen, because the compiler cannot check memory rules across languages.

**Status:** <Badge type="info" text="DESIGN" /> <Badge type="warning" text="RESEARCH" /> ADR-002 and ADR-009 decide the technology after the WP-04 benchmark.

[[toc]]

## 1. Options

| Option | How it works | Strengths | Weaknesses |
|---|---|---|---|
| **UniFFI** | Generates Kotlin and Swift bindings from Rust definitions | Typed API, less hand-written glue, async support, used in production by Mozilla | Byte buffers are serialized/copied per call; generated code size |
| **C ABI + JNI + Swift wrapper** | Hand-written `extern "C"` functions | Full control of memory and copies | More `unsafe`, more glue code, more review |
| **Hybrid** | UniFFI for the control plane; small C ABI for document bytes | Typed API where it matters, control where copies matter | Two mechanisms to maintain |

Recommendation: start with UniFFI. Add the C ABI byte channel only if the benchmark shows a real cost.

## 2. Ownership rules

| ID | Rule |
|---|---|
| `FFI-01` | Every object that crosses the boundary is an **opaque handle** with an explicit `free`/`close`. Kotlin uses `AutoCloseable` + a `Cleaner` as a safety net. Swift uses `deinit`. |
| `FFI-02` | A handle is valid until `close`. A call on a closed handle returns `Cancelled`. It never crashes. |
| `FFI-03` | Byte buffers have one owner. The receiver copies or takes ownership; the sender never reads the buffer after the transfer. |
| `FFI-04` | Strings cross as UTF-8 with explicit length. No NUL-terminated assumptions. |
| `FFI-05` | No panic crosses the boundary. Every exported function catches unwinding and returns an error. Release builds still keep this guard. |
| `FFI-06` | Callbacks from Rust into Kotlin/Swift are not used for the main flow. The core returns actions instead. |
| `FFI-07` | The exported symbol list is minimal and versioned. Tests and debug functions are not exported in release builds. |
| `FFI-08` | The core checks an ABI version number at engine creation. A mismatch is a hard error. |

## 3. Threading

| Platform | Rule |
|---|---|
| Kotlin | Calls into the core happen on the engine's single-threaded dispatcher. The JNI environment is never cached across threads. |
| Swift | Calls happen inside the engine `actor`. Values returned to the main actor are value types. |
| Rust | Core types used across threads are `Send`. Session state is not shared across threads without a lock. |

## 4. Error transport

Errors cross the boundary as a typed enum with a stable numeric code, a category and a redacted message. See [Errors](/engineering/core/errors). The message never contains URLs with query strings, headers, cookies or HTML.

## 5. Verification

| Test | Tool |
|---|---|
| Use after close, double close, close during a call | Kotlin and Swift unit tests; Rust tests with Miri on the handle table |
| Unwinding guard | Test that injects a panic in each exported function |
| Leak check | Android: LeakCanary in instrumented tests; iOS: Instruments Leaks; Rust: counters per handle type |
| Concurrency | Stress test with cancel during every action type |
| Fuzz | `cargo fuzz` targets on every exported function that takes bytes |

## 6. WP-04 benchmark

<Badge type="warning" text="RESEARCH" /> Data for ADR-002 and Q-06. All numbers are **indicative (H-129)**: Mac host, iOS simulator and Android emulator, no device. This section does not choose the ADR-002 option; the human decides it at G2.

### 6.1 What was measured

<Badge type="tip" text="FACT" /> The crate `tools/platform-spikes/ffi-bench` gives a deterministic document of 1 MiB (byte `i` is `(i * 31 + 7) mod 251`) in chunks, pull model ([contract](https://github.com/robotoss/rivqen/blob/main/tools/platform-spikes/CONTRACT.md) §10). Five paths:

| Path | Interface | Copies per byte | Live copies |
|---|---|---|---|
| `uniffi-kotlin` | UniFFI 0.32.2, JNA 5.19.1 | 3 | 2, then 1 garbage |
| `jni-direct` | C ABI through JNI, reused direct `ByteBuffer` | 1 | 1 |
| `jni-bytearray` | C ABI through JNI, reused `ByteArray` | 1 | 1 |
| `uniffi-swift` | UniFFI 0.32.2 | 4 | 3, then 1 |
| `c-swift` | Hand-written C header, Swift-owned buffer | 1 | 1 |

The UniFFI copies: Rust copies the chunk into a `Vec`; UniFFI writes it into a `RustBuffer`; Kotlin copies the buffer into a new `ByteArray`; Swift copies it into a `[UInt8]` and then into a new `Data`. The C ABI paths copy once, from the Rust document into memory that the caller owns. Each `nextChunk` of UniFFI Kotlin makes three JNA calls (clone handle, method, free buffer). The result files split the copies into `rust_copied_bytes` (counted by Rust at run time) and `glue_copied_bytes` (counted from the generated UniFFI code).

"Live copies" is the number of buffers that hold the bytes of one chunk at the same time (counted from the code; the document itself is a static table). In Rust, UniFFI writes the `Vec` into a new buffer while the `Vec` is still alive (2); growth of that buffer can add a short-lived third one. Kotlin holds the `RustBuffer` and the new `ByteArray` (2), frees the `RustBuffer`, and leaves one `ByteArray` per chunk to the garbage collector. Swift holds the `RustBuffer`, the `[UInt8]` and the `Data` (3); reference counting frees the `Data` when the caller drops it. The C ABI paths write into one buffer that the caller reuses (1).

Method: one process per case; 200 warm-up and 1000 measured documents; document time is the sum of the open, read and close calls (the SHA-256 update between reads is not timed); nearest-rank median and p95; peak RSS from `getrusage` (`ru_maxrss`), measured documents and warm-up together. Every document is checked with SHA-256 against the Rust digest. Details: [`ffi-bench/README.md`](https://github.com/robotoss/rivqen/blob/main/tools/platform-spikes/ffi-bench/README.md).

| Target | How it runs |
|---|---|
| `host-jvm` | Kotlin 2.4.20 on JDK 21.0.11 (`-Xmx512m`, default collector) |
| `host-swift` | Swift 6.4.2 (`-O`), command-line program |
| `ios-sim-27.0` | The same Swift code as an app (iOS 18.0 and later) on the lab simulator `rivqen-p2-ffi-27.0` (iPhone 16 Pro, iOS 27.0 24A434); one `simctl launch` per case |
| `android-37` | The same Kotlin code as DEX (d8 9.2.4) under `app_process` on ART, emulator `rivqen-p2-api37` (Android 17, `CE2A.260420.051.A3`, 4 KB pages, heap growth limit 192 MB); library from `cargo ndk` with NDK r30, `LOAD` segments aligned to 16 KB |

Set-up: Apple M4 Pro (14 cores), macOS 26.6.2, Rust 1.99.0, UniFFI 0.32.2, Xcode 27.2, 2026-11-08. Other projects of the user ran mutation tests on the Mac. One-minute load average at the start and end of each target: `android-37` 25 and 22, `host-jvm` 26 and 33, `host-swift` 47 and 50, `ios-sim-27.0` 217 and 254.

S2 review fixes (S1 security review): the JNI read refuses a read-only direct `ByteBuffer` (L-1), so `jni-direct` now calls `ByteBuffer.isReadOnly()` once per read from Rust and once from Kotlin. The JVM harness builds with a Gradle wrapper that checks the distribution SHA-256 and with dependency verification metadata (L-3). The C header has the pointer contracts (L-5).

### 6.2 Results

<Badge type="tip" text="FACT" /> Indicative (H-129). Time per 1 MiB document in µs; per-chunk median in µs; RSS Δ in MiB. Files: `evidence/spikes/wp-04/ffi/android-37.json`, `host-jvm.json`, `host-swift.json`, `ios-sim-27.0.json`.

Android 17 emulator, ART:

| Path | Chunk | Median | p95 | Chunk | RSS Δ |
|---|---|---|---|---|---|
| `uniffi-kotlin` | 16 KiB | 2370 | 4132 | 23.8 | 5.9 |
| `uniffi-kotlin` | 64 KiB | 1685 | 2348 | 77.1 | 4.9 |
| `jni-direct` | 16 KiB | 25.2 | 31.9 | 0.38 | 1.1 |
| `jni-direct` | 64 KiB | 38.1 | 49.3 | 2.29 | 0.8 |
| `jni-bytearray` | 16 KiB | 22.0 | 43.9 | 0.33 | 0.8 |
| `jni-bytearray` | 64 KiB | 24.5 | 47.6 | 1.50 | 0.4 |

JVM (Mac host, HotSpot):

| Path | Chunk | Median | p95 | Chunk | RSS Δ |
|---|---|---|---|---|---|
| `uniffi-kotlin` | 16 KiB | 834 | 7155 | 12.7 | 368 |
| `uniffi-kotlin` | 64 KiB | 476 | 3668 | 29.1 | 336 |
| `jni-direct` | 16 KiB | 30.6 | 106 | 0.46 | 2.6 |
| `jni-direct` | 64 KiB | 27.5 | 73.5 | 1.67 | 1.6 |
| `jni-bytearray` | 16 KiB | 21.2 | 74.8 | 0.33 | 1.3 |
| `jni-bytearray` | 64 KiB | 22.6 | 47.6 | 1.42 | 0.5 |

Swift (Mac host):

| Path | Chunk | Median | p95 | Chunk | RSS Δ |
|---|---|---|---|---|---|
| `uniffi-swift` | 16 KiB | 496 | 1793 | 7.71 | 0.4 |
| `uniffi-swift` | 64 KiB | 409 | 1399 | 25.4 | 1.4 |
| `c-swift` | 16 KiB | 12.2 | 31.8 | 0.21 | 0.1 |
| `c-swift` | 64 KiB | 13.2 | 40.5 | 0.83 | 0.1 |

iOS 27.0 simulator:

| Path | Chunk | Median | p95 | Chunk | RSS Δ |
|---|---|---|---|---|---|
| `uniffi-swift` | 16 KiB | 982 | 1102 | 15.1 | 0.4 |
| `uniffi-swift` | 64 KiB | 691 | 773 | 42.4 | 0.5 |
| `c-swift` | 16 KiB | 10.7 | 11.1 | 0.17 | 0.0 |
| `c-swift` | 64 KiB | 12.1 | 13.0 | 0.75 | 0.0 |

Copied bytes per document: 3 MiB (`uniffi-kotlin`: 1 MiB Rust, 2 MiB glue), 4 MiB (`uniffi-swift`: 1 MiB Rust, 3 MiB glue), 1 MiB (C ABI paths, Rust only).

<Badge type="tip" text="FACT" /> Repeatability: in S1 (load average 13 to 15, four host runs) the medians of a case differed by at most 12 % and the p95 values by up to 2.6 times. In S2 the host p95 values are 3 to 9 times the medians because the load changed during the runs; use the medians. A first S2 run at load average 300 (host) and 600 (simulator) gave medians within 15 % of these for every case.

<Badge type="tip" text="FACT" /> **16 KB pages (supplementary, H-133).** One run on the human's own emulator (Play image `sdk_gphone16k_arm64`, Android 17, API 37, `PAGE_SIZE` 16384), with the same harness: both 16 KB aligned libraries load and the self-check passes; UniFFI Kotlin 6,678 µs, JNI `ByteArray` 39 µs and JNI direct buffer 69 µs median per 1 MiB in 16 KiB chunks (load average 14–16). It is not the main Android result (another image than the lab AVD): `evidence/spikes/wp-04/ffi/android-37.2-16k.json`.

### 6.3 Observations

<Badge type="tip" text="FACT" /> Measured on the Mac host, the iOS simulator and the Android emulator:

1. The C ABI paths move 1 MiB in 11 to 38 µs on every target. A read costs about the time of the copy itself.
2. UniFFI 0.32.2 is about 15 to 110 times slower per document than the C ABI path of the same runtime. Most of the cost is in Rust: a separate S1 measurement of the UniFFI lowering of `Option<Vec<u8>>` took 5.8 µs per 16 KiB and 20.5 µs per 64 KiB (median of 18 000 calls), against 0.2 µs and 1.4 µs for the `Vec` copy. UniFFI 0.32.2 writes a `Vec<u8>` element by element (`uniffi_core` 0.32.2, `src/ffi_converter_impls.rs`, `impl Lower for Vec<T>`).
3. On ART, UniFFI Kotlin is about 3 times slower than on HotSpot (2370 µs against 834 µs at 16 KiB); the JNI paths are about as fast as on the host.
4. Memory (Q-06). On HotSpot, the UniFFI path made the peak RSS grow by about 330 to 375 MiB in every run (S1 and S2): each document leaves about 1 MiB of `ByteArray` garbage (one per chunk), and the collector grows the heap toward the `-Xmx512m` limit. On ART, the same path grew by 4.9 to 5.9 MiB: the runtime collects the garbage in a heap with a 192 MB growth limit, so the garbage costs time (included in item 3), not resident memory. The JNI paths reuse one buffer and grew by less than 3 MiB on both runtimes. On Swift (host and simulator), both paths grew by less than 2 MiB: reference counting frees each `Data` at once, so the 3 live copies are short-lived.
5. The read-only check of L-1 costs one JNI call into Java per read: the `jni-direct` read median is above `jni-bytearray` by 0.13 µs on HotSpot (0.46 against 0.33 µs) and by 0.05 µs on ART (0.38 against 0.33 µs). In S1, without the check, `jni-direct` was the faster path (0.29 µs). At 64 KiB on ART, `jni-direct` is slower than `jni-bytearray` (2.29 against 1.50 µs per read); the cause is not known.
6. In the simulator, `c-swift` is as fast as on the host, and `uniffi-swift` takes about twice the host time in both S2 runs (load average 600 and 217); the cause is not known (the simulator uses the libraries of the iOS runtime, not of macOS).
7. Larger chunks help UniFFI (fewer calls) and do not help the C ABI paths (the copy dominates).

<Badge type="warning" text="RESEARCH" /> Open: device numbers in WP-09 (H-129); a run on a 16 KB page image (the libraries are aligned, but the API 37 image used here has 4 KB pages). `run.sh` has an opt-in path for one supplementary run on the human's 16 KB emulator (`android-<sdk>-16k.json`, not the main result); it is not run yet. Whether a later UniFFI release writes `Vec<u8>` in one copy: not checked (NOT FOUND for 0.32.2).

## Related

- [Streaming](/engineering/core/streaming)
- [ADR-002](/engineering/architecture/adr/#adr-002), [ADR-009](/engineering/architecture/adr/#adr-009)
- [Security control SEC-22](/engineering/security/controls)
