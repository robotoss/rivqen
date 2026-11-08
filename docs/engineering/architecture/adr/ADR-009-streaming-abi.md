# ADR-009: Streaming ABI and buffer ownership

- Status: **Proposed — decided in WP-09** (H-127)
- Date: 2026-11-08
- Owner: WP-09; WP-04 adds the data
- Reviewers: architecture lead, security reviewer, Android SDK owner (WP-10), iOS SDK owner (WP-11)

This record proposes how document bytes move between the core and the adapters: who owns a chunk, who asks for it, and how large it is. It collects the WP-04 data for WP-09, which decides it.

**Status:** <Badge type="info" text="DESIGN" /> for the options and the recommendation. <Badge type="tip" text="FACT" /> for the measured data. Numbers are **indicative (H-129)**.

[[toc]]

## Context

<Badge type="info" text="DESIGN" /> [Streaming](/engineering/core/streaming) §2 and §3 define the bridge stream: a bounded prefix buffer, chunks of 8 to 64 KiB, one owner for each chunk, a pull model for backpressure. The limits are `prefixBufferBytes` (512 KiB) and `maxChunkBytes` (64 KiB) ([resource limits](/engineering/core/resource-limits)). [ADR-002](/engineering/architecture/adr/ADR-002-ffi-technology) chooses the FFI mechanism; this ADR chooses the shape of the byte channel on top of it.

### Evidence: the WebView reads in blocks (Android)

<Badge type="tip" text="FACT" /> The WebView fills each read request before it passes bytes on: `InputStream::Read` loops until the requested length or the end of the stream ([Android SDK](/engineering/platforms/android) §5; [Chromium `input_stream.cc`](https://chromium.googlesource.com/chromium/src/+/6036187a1d76f93d870769db672a39bfece18b4d/android_webview/browser/input_stream.cc)).

| WebView | First read | Later reads |
|---|---|---|
| 145 (API 37 image) | The bytes that `available()` reports | 2048 bytes |
| 91 (API 29 image) | Free space of a 64 KiB data pipe | 65,536 bytes |

<Badge type="tip" text="FACT" /> Effects (CAP-A-01, 3 runs on each API):

1. A flush smaller than the block waits for more bytes or the end of the body. With 1500-byte chunks on WebView 145, chunk 1 waited 4 s for the 548 bytes that filled its read ([block-1500 runs](https://github.com/robotoss/rivqen/tree/main/evidence/spikes/wp-04/android/CAP-A-01/37/block-1500)).
2. On WebView 91, a page of 8 chunks of 2048 bytes reached the renderer only at the end of the body ([small-2048 runs](https://github.com/robotoss/rivqen/tree/main/evidence/spikes/wp-04/android/CAP-A-01/29/small-2048)).
3. With chunks of the block size (2048 bytes on API 37, 65,536 bytes on API 29), the beacon of each chunk reached the host before the host sent the next chunk: no gate timeout ([API 37 runs](https://github.com/robotoss/rivqen/tree/main/evidence/spikes/wp-04/android/CAP-A-01/37), [API 29 runs](https://github.com/robotoss/rivqen/tree/main/evidence/spikes/wp-04/android/CAP-A-01/29)).
4. A read that returns fewer bytes only makes the WebView read again. So the producer cannot make the WebView render a partial block.

The source of the 2048-byte size on WebView 145: NOT FOUND.

### Evidence: a stream that ends with an error (Android)

<Badge type="tip" text="FACT" /> A main-frame stream that throws after 3000 of 8192 bytes gives `net::ERR_FAILED` and the WebView error page, not the delivered bytes. The session must load the URL again (CL-33). The WebView read the same 3000 bytes (count and SHA-256) before the error (CAP-A-08, [API 37 runs](https://github.com/robotoss/rivqen/tree/main/evidence/spikes/wp-04/android/CAP-A-08/37), [API 29 runs](https://github.com/robotoss/rivqen/tree/main/evidence/spikes/wp-04/android/CAP-A-08/29)). A cut after the document commits (a partial page): NOT FOUND at the date of this ADR (the catalog says "not tested").

### Evidence: iOS

<Badge type="tip" text="FACT" /> Only the own-scheme handler streams: the adapter gives each chunk to WebKit with `didReceive(_:)` (push into WebKit), and chunks of 2048 bytes rendered one by one (CAP-I-05). `loadSimulatedRequest` takes one `Data` for the whole body (CAP-I-07). See [ADR-003](/engineering/architecture/adr/ADR-003-ios-navigation). Chunks smaller than 2048 bytes on iOS: NOT FOUND.

### Evidence: the byte channel across the FFI

<Badge type="tip" text="FACT" /> The WP-04 benchmark ([FFI boundary](/engineering/core/ffi) §6) used a pull model: the caller asks for the next chunk.

| Shape | Copies per byte | Read of 16 KiB (median) |
|---|---|---|
| C ABI: Rust writes into a buffer that the caller owns and reuses | 1 | 0.17–0.46 µs |
| UniFFI: each chunk is a new owned `Vec<u8>` | 3–4 | 7.7–23.8 µs |

Ranges over the Mac host, the iOS simulator and the ART emulator (indicative, H-129). The C ABI rules of the prototype: the handle is an opaque registry key; `read` writes at most `min(chunk, cap)` bytes; the end gives `END` with 0 bytes on every later call; a second `close` is a defined error; the library keeps no pointer after the call ([`ffi-bench/README.md`](https://github.com/robotoss/rivqen/blob/main/tools/platform-spikes/ffi-bench/README.md)).

<Badge type="tip" text="FACT" /> The platform `HttpEngine` (API 34+) also reads into a buffer of the caller: `onReadCompleted` gives back "the buffer that was passed in to `UrlRequest#read(java.nio.ByteBuffer)`" ([UrlRequest.Callback](https://developer.android.com/reference/android/net/http/UrlRequest.Callback), checked 2026-11-08).

### Not measured

| Item | State |
|---|---|
| Bytes from Kotlin or Swift into the core | NOT FOUND |
| An ownership transfer (the core allocates, the adapter frees) | NOT FOUND |
| A cut after the commit (partial page) | NOT FOUND at the date of this ADR |
| Devices | NOT FOUND (H-129) |

## Options

### Option 1: Pull into a buffer of the caller

`read(stream, buf, cap)` gives `n` bytes, `END` or an error code. The core writes at most `cap` bytes during the call and keeps no pointer. The other direction is `write(stream, buf, len)`: the core copies or parses the bytes during the call and keeps no pointer.

- Pros: one copy; the buffer has one owner (the caller) before, during and after the call; no lifetime crosses the boundary; measured in WP-04.
- Cons: the core keeps its own bounded buffer for bytes that nobody read yet.

### Option 2: Ownership transfer

The core allocates a chunk and gives it to the adapter, which frees it with a `free` call (or wraps it, for example in a direct `ByteBuffer`).

- Pros: no copy on the Rust side.
- Cons: a chunk lives across the boundary; a garbage-collected wrapper can outlive the `free`, or the `free` can be missed (use after free, leak). Not measured.

### Option 3: Push from the core

The core calls the adapter with each chunk.

- Cons: callbacks for the main flow are against FFI-06; backpressure needs a second channel.

### Option 4: Owned chunk per call (UniFFI `Vec<u8>`)

- Measured: 3 or 4 copies and the time of [ADR-002](/engineering/architecture/adr/ADR-002-ffi-technology) option 1.

## Recommendation

<Badge type="info" text="DESIGN" /> **Option 1**, with these rules for WP-09:

1. **Ownership.** The caller owns every buffer that it passes. The core owns its prefix buffer and its pending chunks. No buffer changes owner across the boundary.
2. **Pull.** On Android, the bridge `InputStream.read(b, off, len)` asks the core (or the network buffer) for at most `len` bytes and writes them into `b`. This is one copy from the source into the array that the WebView passed. On iOS, the own-scheme adapter pulls a chunk and gives it to `didReceive(_:)`.
3. **No bytes yet.** The core never blocks. When it has no bytes and no end, it says so (for example a `PENDING` code), and the adapter waits for the next network event. WP-09 names the code.
4. **Chunk size.** The WebView sets the block (2048 bytes on WebView 145, 64 KiB on WebView 91), not the core. The default chunk stays 16 KiB, within 8 to 64 KiB and at most `maxChunkBytes`. `available()` of the bridge stream reports the bytes that it can give without a wait.
5. **Errors.** A network error or a validation error is an error code, never `END`. The Android adapter then throws an `IOException`; the WebView shows its error page, and the session loads the URL again (CL-33, INV-05). A body that ends early never looks like a complete page.
6. **Bounds.** Pending bytes in the core are at most `prefixBufferBytes + maxChunkBytes` ([Streaming](/engineering/core/streaming) §5). When the buffer is full, the core stops asking the network (backpressure).
7. **One reader.** A stream has one reader at a time; the order of the bytes is the order of the calls. WP-09 decides whether the sequence number of [Streaming](/engineering/core/streaming) §3 stays in the ABI.

Why: option 1 is the only measured shape with one copy, and it has no lifetime across the boundary (FFI-03). The WebView block rule means that a smaller chunk or a flush gives no earlier render, so the core does not need a flush operation.

Cost of the recommendation: the core keeps a bounded buffer for unread bytes; the adapters own the wait.

## Decision

Not decided. **Decided in WP-09** (H-127), after [ADR-002](/engineering/architecture/adr/ADR-002-ffi-technology). With ADR-002 option 1 (UniFFI only), this ADR changes to option 4.

## Consequences

<Badge type="info" text="DESIGN" />

- Progressive rendering on Android has the granularity of the WebView block. On WebView versions with 64 KiB blocks, a page smaller than 64 KiB is not progressive. The support matrix must state this as a limit, not a defect.
- On iOS the byte channel serves the own-scheme path and the checks of the core; `https` pages are not streamed ([ADR-003](/engineering/architecture/adr/ADR-003-ios-navigation)).
- [Streaming](/engineering/core/streaming) §3 changes in WP-09: "pull into a buffer of the caller" replaces "the adapter hands a chunk to the core", and the order rule follows item 7.
- WP-09 measures the direction into the core before it fixes the `write` call.

## Verification

<Badge type="danger" text="GATE" /> WP-09 proves the decision:

1. Property test: the bytes that the WebView receives equal the network bytes, for any chunk size, any `cap` and any timing ([Streaming](/engineering/core/streaming) §5).
2. Fault test: an error in the middle of a body gives an error code, never `END`, and no cache commit.
3. Memory test: pending bytes never exceed `prefixBufferBytes + maxChunkBytes` with a slow reader.
4. Miri on the handle code; `cargo fuzz` on `read` and `write`; the use-after-close and double-close tests of [FFI boundary](/engineering/core/ffi) §5.
5. WP-10: on the device matrix, chunks of the block size render progressively, and a cut stream ends in a normal load.

## Related

- [Streaming](/engineering/core/streaming), [FFI boundary](/engineering/core/ffi), [Resource limits](/engineering/core/resource-limits)
- [ADR-002](/engineering/architecture/adr/ADR-002-ffi-technology), [ADR-003](/engineering/architecture/adr/ADR-003-ios-navigation), [ADR-014](/engineering/architecture/adr/ADR-014-android-http-stack), [ADR register](/engineering/architecture/adr/)
- [Android SDK](/engineering/platforms/android) §5
