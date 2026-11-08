# Android prototype A-STREAM (WP-04)

<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- SPDX-FileCopyrightText: The Rivqen Authors -->

Evidence code for CAP-A-01, CAP-A-02 and CAP-A-03 ([CONTRACT.md](../CONTRACT.md), DL-133). It is not the base of the SDK. Kotlin app `dev.rivqen.spike`, minSdk 29, target 37. Dependencies (DL-137, pinned exactly): androidx.webkit 1.18.0, OkHttp 5.5.1 (both Apache-2.0); AGP 9.1.1, Kotlin 2.4.20, Gradle 9.5.1 as `tools/webview-lab`.

## What the app does

One run per launch (`SpikeActivity`):

1. Start the OkHttp fetch of the page (`EarlyFetch`) **before** the WebView exists. No redirects, no silent retry, no cache.
2. Create the WebView. Install the probe channel (`ProbeBridge`): `addWebMessageListener` (object `rqLabPort`) and `addDocumentStartJavaScript`, both only for `https://localhost:18443` and `https://10.0.2.2:18443`. No `addJavascriptInterface`.
3. `loadUrl(url)`. In `shouldInterceptRequest` the rule (`InterceptRule`, A-04) gives the main-frame request the fetched head and the reader side of the bridge pipe (`BridgePipe`) only when: main frame, `GET`, not a redirect, URL equal to the session URL, origin allowed, not claimed before, and the head can be a `WebResourceResponse` (status 100–299 or 400–599, reason not empty, valid `Content-Type`). Otherwise the request passes to the WebView, and the unused early fetch is cancelled.
4. `onReceivedSslError` always calls `cancel()` (A-03).
5. The run ends when the page finished, the DOMContentLoaded probes arrived, the fetch ended and the pipe has its outcome (then at most 2 s for `metric.first_paint`), or after 150 s.

The page scripts of the spike host call `window.rqLab.probe(name, value)` from the page world, so the probe object is in the page world. The origin rule is the boundary; the app checks every message (main frame, origin, size, shape).

### Bridge pipe

`BridgePipe.java` is plain Java (no Android API) so that `check.sh` tests it on the JVM.

| Item | Value |
|---|---|
| Invariants | Bytes leave unchanged and in order. At most `capacity` bytes wait (back pressure on the fetch). One outcome: `end` (reader read the end), `error` (writer failure after all bytes before it, or a stall), `cancel` (reader closed first). The outcome holds the count and SHA-256 of the bytes the reader got. Callbacks run outside the lock |
| Limits | Capacity 64 KiB (max 1 MiB). Every wait at most 60 s; a stall breaks the pipe on both sides. At most 20,000 app events, 1,000 probes, 4,096 characters per probe message |
| Failure modes | Fetch error → reader gets the bytes before it, then an `IOException`. WebView closes early → writer gets `CancelledException` → the OkHttp call is cancelled. No reader → writer stalls after 60 s. Interrupt → `InterruptedIOException`, no outcome |
| `available()` | Returns the buffered byte count. The WebView sizes its **first** read with it (see below) |

### Read blocks

<Badge type="tip" text="FACT" /> The WebView fills each read request before it passes the bytes to the renderer: `InputStream::Read` loops until the requested length or the end of the stream, at most 4096 bytes per Java call ([Chromium source](https://chromium.googlesource.com/chromium/src/+/6036187a1d76f93d870769db672a39bfece18b4d/android_webview/browser/input_stream.cc)). Measured on WebView 145.0.7632.218 (API 37): the first read asks for the bytes that `available()` reports; every later read asks for 2048 bytes. A read with 1500 bytes waited 4 s for the next chunk (runs `CAP-A-01/37/block-1500`). With `available()` returning 0 the first read also waited for 2048 bytes (one exploration run, not stored). So the S1 scenarios use 2048-byte chunks.

## Files

| Path | Content |
|---|---|
| `app/src/main/kotlin/dev/rivqen/spike/` | `SpikeActivity` (run), `EarlyFetch`, `ProbeBridge`, `EventLog` (JSONL app part), `RunParams` |
| `app/src/main/java/dev/rivqen/spike/` | `BridgePipe`, `InterceptRule` (pure Java) |
| `app/src/debug/res/xml/network_security_config.xml` | Debug only: lab CA as a `debug-overrides` trust anchor. The release config trusts the system store only |
| `check/SpikeLogicCheck.java`, `check.sh` | Offline JVM check (DL-139) |
| `build.sh` | Debug build with Gradle home, SDK and build output in `LAB_HOME` |
| `run.sh` | One capability on the lab emulator |
| `runlog.mjs` | Page URLs, join of the run log, byte check, `run.json`, verdict facts |

The `labCaResources` task copies `$LAB_TLS/ca.pem` into a generated raw resource of the debug build. No key or certificate is in the repository.

## Run

```sh
sh tools/platform-spikes/android/check.sh               # offline, JDK only
sh tools/platform-spikes/lab/emulator.sh start 37        # the lab emulator (emulator-5602)
node tools/platform-spikes/host/main.mjs                 # the spike host (if it does not run)
sh tools/platform-spikes/android/run.sh CAP-A-01 37 1..3
sh tools/platform-spikes/android/run.sh CAP-A-02 37 1..3
sh tools/platform-spikes/android/run.sh CAP-A-03 37 1..3
node tools/platform-spikes/android/runlog.mjs analyze evidence/spikes/wp-04/android/CAP-A-01/37 CAP-A-01
sh tools/platform-spikes/lab/emulator.sh stop
```

`run.sh` steps for each run `k`: `POST /run/start`, start the app with the extras `run`, `url`, `capability`, wait for `files/out/status.json` (at most 200 s), `POST /run/end`, read the app files with `run-as`, join `run.start`, the app part, the host part and `run.end` (keys sorted, LF). `run.end.ok` is false when the app failed or the bytes differ: the fetched and the delivered bytes must equal the body that `host/stream.mjs` builds for the URL (count and SHA-256) and the sum of `host.chunk`.

Exploration (never evidence unless stated): `SPIKE_PARAMS='chunks=3&size=1500&gate=beacon&timeout=4000'` replaces the page parameters and `SPIKE_OUT=<dir>` writes the output there. `SKIP_BUILD=1` installs the last build.

| Capability | Page parameters |
|---|---|
| CAP-A-01 | `chunks=8&size=2048&gate=beacon&timeout=15000` |
| CAP-A-02 | `chunks=4&size=2048&gate=beacon&timeout=15000&first=1000` (the host waits 1 s before the head) |
| CAP-A-03 | `chunks=3&size=2048&gate=beacon&timeout=15000&csp=1` |

The gate timeout (15 s) is longer than a cold WebView start: the first run after the emulator boot needed 7.4 s to create the WebView.

## Log order

The app writes events in the order they happen, with two exceptions that keep the order the same in every run (CONTRACT §8):

- `app.fetch.end` is written when the run ends; its `t` is the time the fetch ended. The end of the network body races the probes of the last chunks.
- `onPageCommitVisible` is the metric `metric.commit`, not an `app.nav` event: it races the chunk probes.

`app.intercept` is written only for main-frame requests; the count of passed subresource requests is in the metrics file. Metrics (`metric.*`, ms on the app clock): `webview_create`, `load_call` (from the fetch start); `intercept`, `commit`, `finish`, `fetch_end`, `visual_chunk_<k>` (`postVisualStateCallback` after the chunk probe) from the `loadUrl` call; `head_wait`; `first_paint` from the page.

## Evidence

`evidence/spikes/wp-04/android/<capability>/37/`: `run-<k>.jsonl`, `run-<k>.metrics.json`, `run.json`. Supporting sets (same layout, not catalog runs): `CAP-A-01/37/block-1500` (read-block stall), `CAP-A-01/37/paint-delay` (paint before the end of the body), `CAP-A-03/37/status-400` (an intercepted 400).
