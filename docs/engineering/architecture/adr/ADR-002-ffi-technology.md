# ADR-002: FFI technology

- Status: **Accepted** (human, 2026-11-09, H-135; in force when the WP-04 merge request is merged)
- Date: 2026-11-08
- Owner: Architect, WP-04 (data); WP-09 implements
- Reviewers: architecture lead, security reviewer, Android SDK owner (WP-10), iOS SDK owner (WP-11)

This record compares the ways to connect the Rust core to Kotlin and Swift, with the WP-04 benchmark data. It is the input for the decision of the human at gate G2.

**Status:** <Badge type="info" text="DESIGN" /> for the options and the recommendation. <Badge type="tip" text="FACT" /> for the measured numbers. All numbers are **indicative (H-129)**: Mac host, Android emulator and iOS simulator, no device.

[[toc]]

## Context

<Badge type="info" text="DESIGN" /> Two kinds of traffic cross the boundary between the core and the adapters:

| Traffic | Examples | Calls per navigation |
|---|---|---|
| Control plane | Open a session, events, actions, errors, configuration | Few (tens) |
| Document bytes | Each chunk of the main document, a snapshot, a patched document | One per chunk (64 for 1 MiB in 16 KiB) |

<Badge type="info" text="DESIGN" /> The rules FFI-01 to FFI-08 apply to every option ([FFI boundary](/engineering/core/ffi) §2): opaque handles, one owner for each byte buffer, no panic across the boundary, a versioned ABI.

<Badge type="tip" text="FACT" /> UniFFI (`uniffi*` crates) is an approved MPL-2.0 exception ([third-party licenses](/legal/third-party)). UniFFI calls from Kotlin go through JNA. The WP-04 benchmark used JNA 5.19.1 under its Apache-2.0 option (DL-137).

<Badge type="warning" text="RESEARCH" /> The earlier register text said: "Option 3 if the WP-04 FFI benchmark shows more than the agreed copy cost". A copy-cost budget is not written in the documentation ([resource limits](/engineering/core/resource-limits), [benchmarks](/engineering/quality/benchmarks)): NOT FOUND. So the human sets the budget with this decision (see [Recommendation](#recommendation)).

### Evidence

<Badge type="tip" text="FACT" /> The crate `tools/platform-spikes/ffi-bench` gives a 1 MiB document in chunks, pull model, through UniFFI 0.32.2 and through a hand-written C ABI (JNI on the JVM and on ART, a C header in Swift). Method and all tables: [FFI boundary](/engineering/core/ffi) §6; contract: [CONTRACT §10](https://github.com/robotoss/rivqen/blob/main/tools/platform-spikes/CONTRACT.md).

Median time for one 1 MiB document in 16 KiB chunks, in µs (indicative, H-129):

| Target | UniFFI | C ABI |
|---|---|---|
| Android 17 emulator, ART | 2,370 | 22.0–25.2 |
| 16 KB page emulator (H-133) | 6,678 | 39.1–69.2 |
| JVM, Mac host | 834 | 21.2–30.6 |
| iOS 27.0 simulator | 982 | 10.7 |
| Swift, Mac host | 496 | 12.2 |

The C ABI range is `jni-bytearray` to `jni-direct` on the JVM and ART, and `c-swift` in Swift. Files: [android-37.json](https://github.com/robotoss/rivqen/blob/main/evidence/spikes/wp-04/ffi/android-37.json), [android-37.2-16k.json](https://github.com/robotoss/rivqen/blob/main/evidence/spikes/wp-04/ffi/android-37.2-16k.json) (supplementary, another image), [host-jvm.json](https://github.com/robotoss/rivqen/blob/main/evidence/spikes/wp-04/ffi/host-jvm.json), [ios-sim-27.0.json](https://github.com/robotoss/rivqen/blob/main/evidence/spikes/wp-04/ffi/ios-sim-27.0.json), [host-swift.json](https://github.com/robotoss/rivqen/blob/main/evidence/spikes/wp-04/ffi/host-swift.json). Catalog rows: CAP-F-01 to CAP-F-04 in [`capabilities.json`](https://github.com/robotoss/rivqen/blob/main/tools/platform-spikes/capabilities.json).

<Badge type="tip" text="FACT" /> Further results (indicative, H-129; [FFI boundary](/engineering/core/ffi) §6.2, §6.3):

1. **Time ratio.** UniFFI takes 21 to 108 times the time of the fastest C ABI path of the same runtime and chunk size (16 KiB and 64 KiB, the four main targets).
2. **Copies.** Each document byte is copied 3 times with UniFFI Kotlin, 4 times with UniFFI Swift and once with the C ABI.
3. **Cause.** Most of the UniFFI cost is in Rust: UniFFI 0.32.2 writes a `Vec<u8>` element by element (5.8 µs per 16 KiB, against 0.2 µs for the `Vec` copy).
4. **Memory.** UniFFI on HotSpot grew the peak RSS by about 330 to 375 MiB in every S1 and S2 run (one `ByteArray` of garbage per chunk, `-Xmx512m`). On ART it grew by 4.9 to 5.9 MiB; there the garbage costs time, not memory. Swift and all C ABI paths grew by less than 3 MiB.
5. **Larger chunks.** 64 KiB chunks make UniFFI faster (ART: 1,685 µs) and do not change the C ABI paths (the copy dominates).
6. **16 KB pages.** On the 16 KB page emulator (H-133), UniFFI Kotlin through the JNA AAR works, and the own library (16 KB aligned `LOAD` segments) loads and passes the self-check.
7. **Safety of the C ABI prototype.** Opaque handle registry (no pointer is dereferenced), `catch_unwind` on every export, pointer contracts in the header, 38 Rust tests at the S1 close, mutation 177/192 (92.2 %). Security review: 0 critical, high or medium findings; 5 low and 6 info ([WP-04 S1](/engineering/plan/sprints/WP-04-S1) §4, §5).

### Not measured

| Item | State |
|---|---|
| Real devices | NOT FOUND (H-129); WP-09 measures them |
| Bytes from Kotlin or Swift into Rust | NOT FOUND: the benchmark reads from Rust only. UniFFI 0.32.0 documents zero copy for `&[u8]` arguments ([WP-04 S1](/engineering/plan/sprints/WP-04-S1) §1) |
| Control-plane calls with real types | NOT FOUND. Published empty-call cost: Kotlin 2.89 µs, Swift 165 ns (S1 §1) |
| Library size (UniFFI scaffolding, JNA, JNI glue) | NOT FOUND |
| UniFFI after 0.32.2 (bytes in one copy) | NOT FOUND |

## Options

### Option 1: UniFFI only

UniFFI generates the bindings for the control plane and for the document bytes (`Vec<u8>`).

- Pros: one mechanism; typed, generated API; no hand-written `unsafe` glue in the SDK.
- Cons: 3 or 4 copies of each byte; each chunk becomes a new foreign object (garbage on the JVM and ART); the Android artifact needs JNA.
- Measured cost: about 2.4 ms per MiB on the ART emulator (6.7 ms on the 16 KB page image) and about 1 ms per MiB in the iOS simulator. A document at `maxDocumentBytes` (5 MiB) costs about 12 ms on the ART emulator (indicative, H-129).

### Option 2: Hand-written C ABI everywhere

`extern "C"` functions for all traffic; JNI on Android; a C header and a Swift wrapper on iOS.

- Pros: one copy of each byte; no JNA; no UniFFI code in the artifacts (the license status of generated code is still open, [third-party licenses](/legal/third-party)).
- Cons: every control-plane type (events, actions, errors, configuration) is marshalled by hand in three languages. The `unsafe` code and the review work grow with every type (SEC-22).
- Measured cost: the byte path is the C ABI row of the table. The cost of the hand-written control plane: NOT FOUND (not built).

### Option 3: Hybrid

UniFFI for the control plane. A narrow C ABI byte channel for the document bytes (open, read, write, close on an opaque stream handle; JNI on Android, a C header in Swift).

- Pros: one copy where the volume is; a typed, generated API for everything else; the byte channel is already prototyped with its safety rules and tests.
- Cons: two mechanisms in one library; the Android artifact still needs JNA; a stream handle that the control plane creates must be valid in the byte channel (one handle table design in WP-09); more review than option 1.
- Measured cost: the byte path is the C ABI row of the table; the control plane is option 1 without the bytes.

## Recommendation

<Badge type="info" text="DESIGN" /> **Option 3 (hybrid)**, with these limits:

1. The byte channel carries only document bytes, in both directions. Everything else goes through UniFFI.
2. **Copy budget (proposal for the human):** each document byte crosses the boundary with at most one copy for each direction. The `ffi_copy_bytes` metric ([Streaming](/engineering/core/streaming) §3) shows it. UniFFI 0.32.2 does not meet it (3 or 4 copies); the C ABI does (1).
3. The buffer and handle model is [ADR-009](/engineering/architecture/adr/ADR-009-streaming-abi) (WP-09).
4. When a UniFFI release moves bytes with one copy, a new ADR can remove the byte channel.

Why: the document bytes are the only traffic with a high call count. The measured difference is 21 to 108 times in time and 3 to 4 times in copies. The C ABI part is small (four functions), and the WP-04 prototype already has the handle registry, the panic guard and the pointer contracts that FFI-01 to FFI-05 ask for.

Cost of the recommendation: two mechanisms to maintain and review, and JNA in the Android artifact.

## Decision

<Badge type="info" text="DESIGN" /> **Accepted (H-135, human, 2026-11-09): option 3, the hybrid, with the copy budget.** UniFFI carries the control plane; a narrow C ABI byte channel (JNI on Android, a C header in Swift) carries document bytes in both directions; each document byte is copied at most once per direction across the boundary, visible in `ffi_copy_bytes`. WP-09 verifies it on a device and in the direction into the core.

## Consequences

<Badge type="info" text="DESIGN" /> If the human accepts option 3:

- Easier: the control plane stays typed and generated; the byte path has a fixed, measured cost.
- Harder: WP-09 maintains a C header, JNI functions and Swift wrappers next to the UniFFI bindings, and reviews their `unsafe` code.
- Dependencies: the Android SDK artifact contains JNA under its Apache-2.0 option. [Third-party licenses](/legal/third-party) has no row for JNA yet; WP-09 adds it. The UniFFI condition "check generated-code status in WP-09" stays.
- Incompatibilities for apps: none. The FFI is internal to the SDK. The SDK artifacts carry native libraries with 16 KB aligned segments ([Android SDK](/engineering/platforms/android) §6).

<Badge type="info" text="DESIGN" /> If the human chooses option 1, the copy budget does not apply, and ADR-009 uses owned `Vec<u8>` chunks. If the human chooses option 2, WP-09 also designs the hand-written control plane.

### Security review notes for WP-09

<Badge type="tip" text="FACT" /> From the security review of the benchmark ([WP-04 S1](/engineering/plan/sprints/WP-04-S1) §5) and the S2 fixes ([WP-04 S2](/engineering/plan/sprints/WP-04-S2) §3):

| ID | Note for the SDK | State in the benchmark |
|---|---|---|
| L-1 | Refuse a read-only direct `ByteBuffer`: JNI `GetDirectBufferAddress` gives its address too | Fixed; costs 0.05–0.13 µs per read |
| L-2 | 64-bit handle ids on every target; no generation wrap | Open for WP-09 |
| L-3 | Gradle dependency verification and a wrapper checksum | Fixed |
| L-4 | A panic-injection test for each export (FFI-05) | Open for WP-09 |
| L-5 | Pointer contracts in the C header; a test keeps header and Rust equal | Fixed |
| Info | No JNI or foreign call while a core lock is held | Open for WP-09 |
| Info | One handle table per engine; `Cleaner` and `deinit` as a backstop (FFI-01) | Open for WP-09 |
| Info | Close in `use` (Kotlin) and `defer` (Swift) | Open for WP-09 |
| Info | Do not reuse the benchmark SHA-256; use an audited crate | Open for WP-09 |

<Badge type="info" text="DESIGN" /> Also for WP-09: Miri on the handle code, `cargo fuzz` on every export that takes bytes, and the use-after-close tests of [FFI boundary](/engineering/core/ffi) §5.

## Verification

<Badge type="danger" text="GATE" /> WP-09 checks the decision before it builds on it:

1. Run the benchmark of CONTRACT §10 on a mid-range Android device and an iPhone (H-129), in both directions (Rust to foreign, foreign to Rust), 1 MiB in 16 KiB and 64 KiB chunks.
2. Count the copies per byte for each direction. They must meet the copy budget.
3. Run the tests of [FFI boundary](/engineering/core/ffi) §5 on the chosen mechanism: use after close, panic guard, leak check, cancel races, fuzz.

## Related

- [FFI boundary](/engineering/core/ffi), [Streaming](/engineering/core/streaming)
- [ADR-009](/engineering/architecture/adr/ADR-009-streaming-abi), [ADR register](/engineering/architecture/adr/)
- [WP-04 S1](/engineering/plan/sprints/WP-04-S1), [WP-04 S2](/engineering/plan/sprints/WP-04-S2)
- Open question Q-06 ([open questions](/engineering/plan/open-questions))
