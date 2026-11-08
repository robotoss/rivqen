# Android prototype A-STREAM (WP-04)

<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- SPDX-FileCopyrightText: The Rivqen Authors -->

Evidence code for CAP-A-01…05 and CAP-A-08 ([CONTRACT.md](../CONTRACT.md), DL-133). It is not the base of the SDK. Kotlin app `dev.rivqen.spike`, minSdk 29, target 37. Dependencies (DL-137, pinned exactly): androidx.webkit 1.18.0, OkHttp 5.5.1 (both Apache-2.0); AGP 9.1.1, Kotlin 2.4.20, Gradle 9.5.1 as `tools/webview-lab`.

## What the app does

One run per launch (`SpikeActivity`):

1. Start the OkHttp fetch of the page (`EarlyFetch`) **before** the WebView exists. No redirects, no silent retry, no cache.
2. Create the WebView. Install the probe channel (`ProbeBridge`): `addWebMessageListener` (object `rqLabPort`) and `addDocumentStartJavaScript`, both only for `https://localhost:18443` and `https://10.0.2.2:18443`. No `addJavascriptInterface`. Without `DOCUMENT_START_SCRIPT` (WebView 91 of the API 29 image) the channel is installed after the load (see below).
3. `loadUrl(url)`. In `shouldInterceptRequest` the rule (`InterceptRule`, A-04) gives the main-frame request the fetched head and the reader side of the bridge pipe (`BridgePipe`) only when: main frame, `GET`, not a redirect, URL equal to the session URL, origin allowed (the spike host on 18443 and 18444), not claimed before, and the head can serve it (`InterceptRule.headPass`). The head cannot serve it when the response has a prior response (`redirected`), when it is a 3xx (`redirect-status`), or when it is not a valid `WebResourceResponse` head (status 100–299 or 400–599, reason not empty, valid `Content-Type`; else `head-not-expressible`). Otherwise the request passes to the WebView, and the unused early fetch is cancelled. The early fetch does not read the body of a head that cannot serve the request. A pass delivers no byte of the early fetch, so there is no result for any cache partition (H-67).
4. `onReceivedSslError` always calls `cancel()` (A-03). `shouldOverrideUrlLoading` never overrides; it logs each main-frame redirect (`app.nav` `redirect`). `onReceivedHttpError` for the main frame logs `app.nav` `fail` with `http <status>`.
5. The load **settles** when a document finished and its DOMContentLoaded probes arrived (the finished page is the page of the `location` probe), or when the main frame failed (`onReceivedError` or a TLS error) and a page finished.
6. Scenario steps (intent extra `scenario`):
   - `redirect-3xx` (CAP-A-04 probe): the interceptor gives the WebView the fetched 3xx head as a `WebResourceResponse`. The constructor refuses it, and the request passes.
   - `cut-range` (CAP-A-08): the delivered stream ended with an error and the load settled. The app writes `native.doc`, then calls `loadUrl` once more (CL-33: the request passes as `claimed`, and the WebView loads it itself). When that load settled, it writes `native.doc` again, then runs a page script with two byte-range fetches of `/s/range`.
7. The run ends when the last load settled, the fetch ended and the pipe has its outcome (then at most 2 s for `metric.first_paint`), or after 150 s.

The page scripts of the spike host call `window.rqLab.probe(name, value)` from the page world, so the probe object is in the page world. The origin rule is the boundary; the app checks every message (main frame, origin, size, shape).

**After-load channel.** <Badge type="tip" text="FACT" /> WebView 91.0.4472.114 (API 29 `google_apis` image) has `WEB_MESSAGE_LISTENER` but not `DOCUMENT_START_SCRIPT` (`WebViewFeature.isFeatureSupported`; `run.json` `features`). Then the app runs the same script with `evaluateJavascript` in `onPageFinished`, only when the finished URL is on a probe origin (`evaluateJavascript` has no origin rule of its own). The DOMContentLoaded probes then run at once, after `app.nav` `finish`. Page scripts that ran before (the chunk scripts) find no `rqLab`, so there are no `chunk` probes. The chunk beacon requests that the interceptor sees start the visual-state callbacks instead (`metric.visual_beacon_<k>`). A script result other than `installed` is logged as `native.after_load`.

**Lab probes of S2.** App probes (prefix `native.`, CONTRACT §6): `native.range_request` (the `Range` field that `shouldInterceptRequest` gets for a `/s/range` subresource), `native.step` (`fallback-load`, `range-fetch`), `native.doc` (JSON from `evaluateJavascript`: `href`, `title`, `ready`, `text` length of the body, `rqlab`), `native.after_load`. Page probes of the range script: `range.status`, `range.content_range`, `range.bytes`, `range.match` (every byte equals `(i * 31 + 7) mod 251`), `range.bad_status`, `range.error`, `range.done`. Extra members: `app.intercept` has `redirect` (`WebResourceRequest.isRedirect`) and `reason` (the pass reason); `app.stream.read` has `want` (the length the WebView asked for).

### Bridge pipe

`BridgePipe.java` is plain Java (no Android API) so that `check.sh` tests it on the JVM.

| Item | Value |
|---|---|
| Invariants | Bytes leave unchanged and in order. At most `capacity` bytes wait (back pressure on the fetch). One outcome: `end` (reader read the end), `error` (writer failure after all bytes before it, or a stall), `cancel` (reader closed first). The outcome holds the count and SHA-256 of the bytes the reader got. Callbacks run outside the lock |
| Limits | Capacity 64 KiB (max 1 MiB). Every wait at most 60 s; a stall breaks the pipe on both sides. At most 20,000 app events, 1,000 probes, 4,096 characters per probe message |
| Failure modes | Fetch error → reader gets the bytes before it, then an `IOException`. WebView closes early → writer gets `CancelledException` → the OkHttp call is cancelled. No reader → writer stalls after 60 s. Interrupt → `InterruptedIOException`, no outcome |
| `available()` | Returns the buffered byte count. WebView 145 sizes its **first** read with it (see below) |

### Read blocks

<Badge type="tip" text="FACT" /> The WebView fills each read request before it passes the bytes to the renderer: `InputStream::Read` loops until the requested length or the end of the stream, at most 4096 bytes per Java call ([Chromium source](https://chromium.googlesource.com/chromium/src/+/6036187a1d76f93d870769db672a39bfece18b4d/android_webview/browser/input_stream.cc)).

- WebView 145.0.7632.218 (API 37): the first read asks for the bytes that `available()` reports; every later read asks for 2048 bytes. A read with 1500 bytes waited 4 s for the next chunk (runs `CAP-A-01/37/block-1500`). With `available()` returning 0 the first read also waited for 2048 bytes (one exploration run, not stored). So the API 37 sets use 2048-byte chunks.
- WebView 91.0.4472.114 (API 29): every read asks for **64 KiB**. The loader asks for the free space of its Mojo data pipe ([`android_stream_reader_url_loader.cc`](https://chromium.googlesource.com/chromium/src/+/refs/tags/91.0.4472.114/components/embedder_support/android/util/android_stream_reader_url_loader.cc)), whose default capacity is `64 * 1024` ([`mojo/core/core.cc`](https://chromium.googlesource.com/chromium/src/+/refs/tags/91.0.4472.114/mojo/core/core.cc)); every Java read asks for 4096 bytes (`want`). With the S1 scenario (8 chunks of 2048 bytes) nothing reached the renderer before the end of the body: every gate timed out, and the beacons came after the last chunk (set `CAP-A-01/29/small-2048`). With 16 KiB chunks the beacons of chunks 0–3 came together after chunk 3 (one exploration run, not stored). So the API 29 sets use 65,536-byte chunks (`runlog.mjs` `BLOCK_29`).

## Files

| Path | Content |
|---|---|
| `app/src/main/kotlin/dev/rivqen/spike/` | `SpikeActivity` (run), `EarlyFetch`, `ProbeBridge`, `EventLog` (JSONL app part), `RunParams` (with `Scenario`) |
| `app/src/main/java/dev/rivqen/spike/` | `BridgePipe`, `InterceptRule` (pure Java) |
| `app/src/debug/res/xml/network_security_config.xml` | Debug only: lab CA as a `debug-overrides` trust anchor. The release config trusts the system store only |
| `check/SpikeLogicCheck.java`, `check.sh` | Offline JVM check (DL-139) |
| `build.sh` | Debug build with Gradle home, SDK and build output in `LAB_HOME` |
| `run.sh` | One capability (or supporting set) on the lab emulator |
| `runlog.mjs` | Run sets (`SETS`), page URLs, join of the run log, run checks, `run.json`, verdict facts |
| `test/runlog.test.mjs` | Offline tests of the run sets and the run checks |

The `labCaResources` task copies `$LAB_TLS/ca.pem` into a generated raw resource of the debug build. No key or certificate is in the repository.

## Run

```sh
sh tools/platform-spikes/android/check.sh               # offline, JDK only
node --test tools/platform-spikes/android/test           # offline, run sets and run checks
sh tools/platform-spikes/lab/emulator.sh start 37        # the lab emulator (emulator-5602)
node tools/platform-spikes/host/main.mjs                 # the spike host (if it does not run)
sh tools/platform-spikes/android/run.sh CAP-A-01 37 1..3
sh tools/platform-spikes/android/run.sh CAP-A-04 37 1..3
SPIKE_SET=cross-site sh tools/platform-spikes/android/run.sh CAP-A-04 37 1..3
node tools/platform-spikes/android/runlog.mjs analyze evidence/spikes/wp-04/android/CAP-A-01/37 CAP-A-01
sh tools/platform-spikes/lab/emulator.sh stop
```

`run.sh` steps for each run `k`: `POST /run/start`, start the app with the extras `run`, `url`, `capability`, `scenario`, wait for `files/out/status.json` (at most 200 s; with `run-as ls`, because on API 29 `run-as` cannot execute `test`), `POST /run/end`, read the app files with `run-as`, join `run.start`, the app part, the host part and `run.end` (keys sorted, LF). `run.end.ok` is false when the app failed or the run check of the set kind fails:

| Kind | An ok run needs |
|---|---|
| `stream` | The fetched and the delivered bytes equal the body that `host/stream.mjs` builds for the URL (count and SHA-256) and the sum of `host.chunk`; outcome `end` |
| `pass` | Every main-frame decision is `pass`; the WebView read no byte of the early fetch (outcome `cancel`, 0 bytes) |
| `probe` | The WebView read no byte of the early fetch (the decision is the observation) |
| `cut` | The first main-frame request streamed; the fetch ended at `host.cut.after` and did not complete; the WebView read the same bytes (count and SHA-256), outcome `error`; the fallback load ran, and its request passed as `claimed` |

A failed run goes to `$LAB_OUT/android-failed/…`, never into `evidence/`. `run.json` is written only next to ok runs.

Run sets (`runlog.mjs` `SETS`). `SPIKE_SET=<set>` selects a supporting set; its output goes to `<capability>/<api>/<set>/`. On API 29 the stream sets use 65,536-byte chunks (see "Read blocks").

| Capability / set | Page | Kind |
|---|---|---|
| CAP-A-01 | `/s/stream` `chunks=8&size=2048&gate=beacon&timeout=15000` (API 29: `chunks=4&size=65536`) | stream |
| CAP-A-01 `block-1500` | `chunks=3&size=1500&gate=beacon&timeout=4000` | stream |
| CAP-A-01 `paint-delay` | `chunks=3&size=2048&gate=delay&delay=1500` (API 29: `size=65536`) | stream |
| CAP-A-01 `small-2048` | the API 37 parameters, run on API 29 | stream |
| CAP-A-02 | `chunks=4&size=2048&gate=beacon&timeout=15000&first=1000` (API 29: `chunks=2&size=65536`); the host waits 1 s before the head | stream |
| CAP-A-03 | `chunks=3&size=2048&gate=beacon&timeout=15000&csp=1` (API 29: `chunks=2&size=65536`) | stream |
| CAP-A-03 `status-400` | `chunks=0` (the host answers 400) | stream |
| CAP-A-04 | `/s/redirect` `n=1&code=302&to=/s/echo?html=1&run=…` | pass |
| CAP-A-04 `cross-site` | as CAP-A-04 with `cross=1&alt=10.0.2.2` (the final page is on site B) | pass |
| CAP-A-04 `chain-307` | `n=2&code=307` | pass |
| CAP-A-04 `wrr-302` | as CAP-A-04, scenario `redirect-3xx` | probe |
| CAP-A-05 | `https://10.0.2.2:18444/s/echo?html=1` (site B, bad certificate; no extra `adb reverse`) | pass |
| CAP-A-08 | `/s/cut` `after=3000&size=8192`, scenario `cut-range` | cut |

The gate timeout (15 s) is longer than a cold WebView start: the first run after the emulator boot needed 7.4 s to create the WebView.

Exploration (never evidence unless stated): `SPIKE_PARAMS='chunks=3&size=1500&gate=beacon&timeout=4000'` replaces the page parameters and `SPIKE_OUT=<dir>` writes the output there. `SKIP_BUILD=1` installs the last build.

## Log order

The app writes events in the order they happen, with these rules that keep the order the same in every run (CONTRACT §8):

- `app.fetch.end` is written when the run ends; its `t` is the time the fetch ended. The end of the network body races the probes of the last chunks.
- `onPageCommitVisible` is the metric `metric.commit`, not an `app.nav` event: it races the chunk probes.
- A fetch failure (`app.error`) is written before the interceptor gets the result, so it comes before `app.intercept`.
- The range script sends its page probes after both range requests, so the two `native.range_request` events come first.

`app.intercept` is written only for main-frame requests; the count of passed subresource requests is in the metrics file. Metrics (`metric.*`, ms on the app clock): `webview_create`, `load_call` (from the fetch start); `intercept`, `commit`, `finish`, `fetch_end`, `visual_chunk_<k>` (`postVisualStateCallback` after the chunk probe) or `visual_beacon_<k>` (after the chunk beacon request, after-load channel) from the first `loadUrl` call; `head_wait`; `first_paint` from the page. The metrics file also has `scenario`, `channel` and `loads`.

## Evidence

`evidence/spikes/wp-04/android/<capability>/<api>/`: `run-<k>.jsonl`, `run-<k>.metrics.json`, `run.json`. Supporting sets (same layout, not catalog runs): `CAP-A-01/37/block-1500` (read-block stall), `CAP-A-01/<api>/paint-delay` (paint before the end of the body), `CAP-A-01/29/small-2048` (2048-byte chunks wait for the 64 KiB block), `CAP-A-03/<api>/status-400` (an intercepted 400), `CAP-A-04/<api>/cross-site`, `chain-307`, `wrr-302`.
