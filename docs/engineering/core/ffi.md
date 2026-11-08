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

<Badge type="warning" text="RESEARCH" /> Data for ADR-002 and Q-06. All numbers are **indicative (Mac host, H-129)**: no device, no emulator, no simulator yet. S2 runs the same harness on the Android emulator and the iOS simulator. This section does not choose the ADR-002 option; the human decides it at G2.

### 6.1 What was measured

<Badge type="tip" text="FACT" /> The crate `tools/platform-spikes/ffi-bench` gives a deterministic document of 1 MiB (byte `i` is `(i * 31 + 7) mod 251`) in chunks, pull model ([contract](https://github.com/robotoss/rivqen/blob/main/tools/platform-spikes/CONTRACT.md) §10). Five paths:

| Path | Interface | Copies per byte |
|---|---|---|
| `uniffi-kotlin` | UniFFI 0.32.2, JNA 5.19.1 | 3 |
| `jni-direct` | C ABI through JNI, reused direct `ByteBuffer` | 1 |
| `jni-bytearray` | C ABI through JNI, reused `ByteArray` (`SetByteArrayRegion`) | 1 |
| `uniffi-swift` | UniFFI 0.32.2 | 4 |
| `c-swift` | Hand-written C header, Swift-owned buffer | 1 |

The UniFFI copies: Rust copies the chunk into a `Vec`; UniFFI writes it into a `RustBuffer`; Kotlin copies the buffer into a new `ByteArray`; Swift copies it into a `[UInt8]` and then into a new `Data`. The C ABI paths copy once, from the Rust document into memory that the caller owns. Each `nextChunk` of UniFFI Kotlin makes three JNA calls (clone handle, method, free buffer).

Method: one process per case; 200 warm-up and 1000 measured documents; document time is the sum of the open, read and close calls (the SHA-256 update between reads is not timed); nearest-rank median and p95; peak RSS from `getrusage` (`ru_maxrss`), measured documents and warm-up together. Every document is checked with SHA-256 against the Rust digest. Details: [`ffi-bench/README.md`](https://github.com/robotoss/rivqen/blob/main/tools/platform-spikes/ffi-bench/README.md).

Set-up: Apple M4 Pro, macOS 26.6.2, Rust 1.99.0, Kotlin 2.4.20 on JDK 21.0.11 (`-Xmx512m`, default collector), Swift 6.4.2 (`-O`), 2026-11-08. Other programs of the user ran during the runs (load average 10 to 27 on 14 cores).

### 6.2 Results

<Badge type="tip" text="FACT" /> Indicative (Mac host, H-129). Time per 1 MiB document in µs; per-chunk median in µs; RSS Δ in MiB. Files: `evidence/spikes/wp-04/ffi/host-jvm.json`, `host-swift.json`.

JVM:

| Path | Chunk | Median | p95 | Chunk | RSS Δ |
|---|---|---|---|---|---|
| `uniffi-kotlin` | 16 KiB | 817 | 1183 | 12.4 | 362 |
| `uniffi-kotlin` | 64 KiB | 478 | 619 | 28.8 | 350 |
| `jni-direct` | 16 KiB | 20.0 | 23.7 | 0.29 | 1.1 |
| `jni-direct` | 64 KiB | 22.4 | 27.3 | 1.38 | 1.5 |
| `jni-bytearray` | 16 KiB | 20.8 | 25.8 | 0.33 | 2.0 |
| `jni-bytearray` | 64 KiB | 24.2 | 68.5 | 1.50 | 1.6 |

Swift:

| Path | Chunk | Median | p95 | Chunk | RSS Δ |
|---|---|---|---|---|---|
| `uniffi-swift` | 16 KiB | 490 | 644 | 7.58 | 0.4 |
| `uniffi-swift` | 64 KiB | 414 | 969 | 25.8 | 1.4 |
| `c-swift` | 16 KiB | 12.1 | 17.2 | 0.17 | 0.1 |
| `c-swift` | 64 KiB | 13.2 | 20.8 | 0.83 | 0.1 |

Copied bytes per document: 3 MiB (`uniffi-kotlin`), 4 MiB (`uniffi-swift`), 1 MiB (C ABI paths).

<Badge type="tip" text="FACT" /> Four full runs: the medians of a case differ by at most 12 %; the p95 values differ by up to 2.6 times (system load). The tables show the last run (load average 13 to 15), which is in the result files.

### 6.3 Observations

<Badge type="tip" text="FACT" /> Measured on the Mac host:

1. The C ABI paths move 1 MiB in 12 to 24 µs. A read costs about the time of the copy itself.
2. UniFFI 0.32.2 is about 20 to 40 times slower per document. Most of the cost is in Rust: a separate measurement of the UniFFI lowering of `Option<Vec<u8>>` took 5.8 µs per 16 KiB and 20.5 µs per 64 KiB (median of 18 000 calls), against 0.2 µs and 1.4 µs for the `Vec` copy. UniFFI 0.32.2 writes a `Vec<u8>` element by element (`uniffi_core` 0.32.2, `src/ffi_converter_impls.rs`, `impl Lower for Vec<T>`).
3. On the JVM, the UniFFI path made the peak RSS grow by about 330 to 375 MiB in four runs: each document makes about 1 MiB of new `ByteArray` objects (the `RustBuffer` is native memory), and the collector grows the heap toward the `-Xmx512m` limit. This depends on the collector and the heap limit. The JNI paths, which reuse one buffer, grew by less than 3 MiB. On Swift, both paths grew by less than 2 MiB.
4. Larger chunks help UniFFI (fewer calls) and do not help the C ABI paths (the copy dominates).

<Badge type="warning" text="RESEARCH" /> Open for S2 and later: the same harness on the Android emulator (ART, not HotSpot) and the iOS simulator; device numbers in WP-09 (H-129). Whether a later UniFFI release writes `Vec<u8>` in one copy: not checked (NOT FOUND for 0.32.2).

## Related

- [Streaming](/engineering/core/streaming)
- [ADR-002](/engineering/architecture/adr/#adr-002), [ADR-009](/engineering/architecture/adr/#adr-009)
- [Security control SEC-22](/engineering/security/controls)
