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

## Related

- [Streaming](/engineering/core/streaming)
- [ADR-002](/engineering/architecture/adr/#adr-002), [ADR-009](/engineering/architecture/adr/#adr-009)
- [Security control SEC-22](/engineering/security/controls)
