# Platform spikes contract (WP-04)

<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- SPDX-FileCopyrightText: The Rivqen Authors -->

This file fixes the interfaces between the parts of the WP-04 lab: the spike host (`host/`), the lab environment (`lab/`), the Android prototype (`android/`), the iOS prototype (`ios/`), the FFI benchmark (`ffi-bench/`), the evidence check (`check/`) and the stored evidence. The architect owns this file. A lane that needs a change writes a shared-file request in its report.

Decisions: DL-133 (location), DL-134 (host and TLS), DL-135 (Q-24 scope), DL-136 (evidence), DL-137 (dependencies), DL-138 (environment), DL-139 (offline checks), H-127…H-130. Sprint record: `docs/engineering/plan/sprints/WP-04-S1.md`.

> **Clean-room.** WP-04 uses no upstream code. The prototypes are written from the Rivqen pages (`docs/engineering/platforms/*.md`, `docs/engineering/core/*.md`) and the platform documentation.

> **Public API only.** No private WebKit or Android API, no reflection into WebView internals, no global `NSURLProtocol`, no trust-all TLS, no ATS exception, no origin spoofing ([iOS](../../docs/engineering/platforms/ios.md) §3, [platform hardening](../../docs/engineering/security/platform-hardening.md)).

## 1. Topology

```text
Android emulator (emulator-5602)          host (Mac)
 10.0.2.2:18443  (site B) ─────────┐      ┌──────────────────────────────┐
 localhost:18443 (site A, adb      ├────▶ │ spike host  :18443 TLS        │──▶ rust-reference --site
   reverse tcp:18443) ─────────────┘      │             :18444 TLS (bad)  │    127.0.0.1:18480
iOS simulator (rivqen-p2-*)               │ control     :18445 (plain,    │
 localhost:18443 (site A)  ──────────────▶│   loopback, run log only)    │
 127.0.0.1:18443 (site B)  ──────────────▶└──────────────────────────────┘
```

| Port | Listener | Use |
|---|---|---|
| 18443 | Spike host, TLS, good certificate | All scenarios (section 3) |
| 18444 | Spike host, TLS, bad certificate (section 2) | SSL error cases only |
| 18445 | Spike host control, plain HTTP | `POST /run/start`, `POST /run/end`, `GET /run/log` (section 4); loopback only |
| 18480 | `server/rust-reference --site` | Real legacy and RQP pages behind `/page/*` |

All listeners bind `127.0.0.1` only. Android reaches the host through `10.0.2.2` and through `adb -s emulator-5602 reverse tcp:18443 tcp:18443` (then `localhost` on the emulator is the host).

**Two sites.** Site A is `https://localhost:18443`, site B is the IP form (`https://127.0.0.1:18443` on iOS, `https://10.0.2.2:18443` on Android). They are different sites for `SameSite` and different origins. A case that needs one site uses site A.

## 2. TLS

- `host/tls.mjs` makes the files on each `host` start when they are missing, with the system `openssl`, in `$LAB_HOME/tls/`:
  - `ca.pem`, `ca.key` — a CA valid for 30 days, subject `CN=Rivqen WP-04 lab CA`.
  - `leaf.pem`, `leaf.key` — signed by the CA; SAN `DNS:localhost, IP:127.0.0.1, IP:10.0.2.2`; valid 30 days.
  - `bad-leaf.pem`, `bad-leaf.key` — self-signed, same SAN (port 18444).
- No key or certificate goes into the repository. `.gitignore` covers `tls/`.
- Android: the debug build copies `$LAB_HOME/tls/ca.pem` into a generated resource directory under `build/` and names it in a `debug-overrides` trust anchor of `network_security_config.xml`. The release build trusts the system store only.
- iOS: `lab/sim.sh trust <device>` runs `xcrun simctl keychain <udid> add-root-cert $LAB_HOME/tls/ca.pem` on a lab simulator only.

## 3. Host routes

Every route is on both sites. Query values are ASCII. Every response has `Cache-Control: no-store` unless the route says otherwise. Every request gets a `host.request` event (section 5) with the `run` id from the `Rq-Lab-Run` request field or the `run` query parameter.

| Route | Behavior |
|---|---|
| `GET /page/<path>` | Pass-through to `rust-reference --site` on 18480 (status, fields and body unchanged, except hop-by-hop fields) |
| `GET /s/stream?run=&chunks=N&size=B&first=MS&gate=beacon\|delay&delay=MS&timeout=MS&csp=0\|1` | `200 text/html; charset=utf-8`, chunked. Waits `first` ms before the head. Sends `N` chunks of about `B` bytes. Chunk `k` (0-based) ends with `<img src="/s/beacon?run=…&k=<k>" hidden>` and a script that posts `chunk` probe `k` (section 6). `gate=beacon`: chunk `k+1` is sent only after `/s/beacon?k=<k>` arrived, or after `timeout` ms (then the host writes `host.gate_timeout`). `gate=delay`: chunk `k+1` after `delay` ms. `csp=1` adds `Content-Security-Policy: script-src 'nonce-<n>'` and puts that nonce on every script; `<n>` is the run id with `.` replaced by `_` (a dot is not a nonce character). Limit: `chunks * size` ≤ 64 MiB |
| `GET /s/beacon?run=&k=` | `204`. Writes `host.beacon` |
| `GET /s/redirect?run=&n=K&code=301\|302\|303\|307\|308&to=<path>` | Chain of `K` redirects with the code; the last one goes to `<path>` (default `/s/echo`). Hop `i` goes to the other site when `cross=1`; `alt=127.0.0.1\|10.0.2.2` (default `127.0.0.1`) names the IP form of site B, so the Android run script passes `alt=10.0.2.2` |
| `GET /s/cookie/set?run=&name=&value=&attrs=<list>` | `200` page; one `Set-Cookie: <name>=<value>; Path=/; <attrs>`. `attrs` is a comma list of `HttpOnly`, `Secure`, `SameSite=Strict\|Lax\|None`, `Max-Age=<n>`. Values are synthetic (`[a-z0-9-]` only) |
| `GET /s/echo?run=` | `200 application/json`: `{"cookie": <Cookie field or null>, "site": "A"\|"B", "method", "target", "fields": [names only]}`. Also as `text/html` with the JSON in a `<pre id="echo">` when `html=1` |
| `GET /s/range?run=&size=B` | Body of `B` bytes. Honors one `Range: bytes=a-b` (or `a-`, or the suffix form `-n`) with `206`, `Content-Range`; a bad range gives `416` |
| `GET /s/cut?run=&after=BYTES&size=B` | `200 text/html` with `Content-Length` of the full body (`size`, default 4096; `after` < `size`); closes the connection after `BYTES` body bytes |
| `GET /s/sw.js`, `GET /s/sw-page?run=` | Service Worker script and a page that registers it (S2; `501` in S1) |

A request whose `Host` is not `localhost`, `127.0.0.1` or `10.0.2.2` (with or without the port) gets `400`.

The host never logs a `Cookie` or `Set-Cookie` value that does not match `[a-z0-9-]*`.

## 4. Control

Plain HTTP on `127.0.0.1:18445`. A run script calls it; an app never does. A request needs `Content-Type: application/json` (for `POST`) and `Host` `127.0.0.1` or `localhost` (guard against cross-site requests and DNS rebinding). Limits: 16 open runs, 100,000 events per run. A host event of a run that was not started is dropped.

| Request | Effect |
|---|---|
| `POST /run/start` body `{"run": "<id>", "capability": "<CAP-…>", "platform", "os", "k"}` | Starts the host log of the run |
| `POST /run/end` body `{"run": "<id>"}` | Closes it; the response body is the host part of the JSONL log |
| `GET /run/log?run=<id>` | The host part so far |

Run id: `<capability>-<platform>-<os>-<k>`, for example `CAP-A-01-android-37-1`. `[A-Za-z0-9.-]` only.

## 5. Event log

One JSON Lines file per run: `evidence/spikes/wp-04/<platform>/<capability>/<os>/run-<k>.jsonl`. It joins the app part and the host part: first `run.start`, then the app events in app order, then the host events in host order, then `run.end`. Keys sorted, UTF-8, LF.

Common members: `ev`, `src` (`app`, `host`, `script`), `seq` (order inside its source), `t` (milliseconds since the run start, clock of its source; never compared).

| Event | Source | Members |
|---|---|---|
| `run.start` | script | `run`, `capability`, `platform`, `os`, `device`, `webview` (Android: WebView package and version; iOS: `null`), `prototype`, `k` |
| `app.fetch.start`, `app.fetch.head`, `app.fetch.end` | app | `url`; `status`, `fields` (names and synthetic values) on `head`; `bytes`, `complete` on `end` |
| `app.intercept` | app | `url`, `method`, `main_frame`, `decision` (`stream`, `pass`, `block`), `status`, `fields`. `stream` also covers a subresource that a scheme handler forwards (IOS-P3 beacons) |
| `app.stream.read` | app | `bytes` (one event for each read that returned data; `k` of the last chunk marker seen when known) |
| `app.nav` | app | `kind` (`start`, `redirect`, `commit`, `finish`, `fail`), `url`, `error` |
| `app.probe` | app | `name`, `value` (section 6) |
| `app.ssl_error` | app | `url`, `primary_error`, `decision` (`cancel`) |
| `app.error` | app | `error` (class name and message, no data) |
| `host.request` | host | `site`, `method`, `target`, `fields` (sorted field names only), `range` (the `Range` value, when present) |
| `host.response` | host | `status`, `fields` (names and values; cookie values synthetic) |
| `host.chunk` | host | `k`, `bytes` |
| `host.beacon`, `host.gate_timeout` | host | `k` |
| `host.cut` | host | `after` |
| `run.end` | script | `ok`, `error` |

App order notes (for section 8): `app.intercept` is written for main-frame requests (and for subresources that a scheme handler forwards); `app.fetch.end` may be written at the end of the run with `t` set to the time the fetch ended, because the end of the network body races the probes of the last chunks; Android `onPageCommitVisible` is the metric `metric.commit`, not an `app.nav` `commit` event, because it races the chunk probes.

Never log: real hosts, real accounts, real cookies, tokens, device identifiers. Debug log tag: `RivqenSpike`.

**Metrics.** A number that depends on timing (for example time to first render) goes in `app.probe` with a name that starts with `metric.` and in the run file `run-<k>.metrics.json`. Metrics are never compared for determinism.

## 6. Probes

A page reports a probe by `window.rqLab.probe(name, value)`. The app installs `rqLab` in an isolated world where the platform allows it (Android `WebViewCompat.addWebMessageListener` with an origin rule; iOS `WKScriptMessageHandler` in a `WKContentWorld`) and writes `app.probe`.

| Probe | Value |
|---|---|
| `chunk` | `k` of the chunk whose script ran |
| `location` | `document.location.href` |
| `origin` | `window.origin` |
| `cookie` | `document.cookie` |
| `echo` | the JSON of `/s/echo` that a `fetch` from the page got |
| `csp.inline` | `true` when an inline script without the nonce ran |
| `history` | `history.length` |
| `sw` | registration result (`ok` or the error name) |
| `metric.first_paint` | `performance` entry `first-contentful-paint` start time, ms |
| `metric.commit` | app clock, ms from load call to commit |
| `status` | `PerformanceNavigationTiming.responseStatus` of the document |

- The prefix `native.` is reserved for probes that the app writes itself (for example `native.step`, `native.webview`, `native.wk_cookies`); the app refuses a page probe with this prefix.
- A lane may add probe names for its own cases; it lists them in its README (iOS S1: `base`, `relative`, `content_type`, `charset`, `title`, `data`, `bridge.*`, `rqlab.page_world`, `webview.cookie_set`, `https.*`, `metric.fetch`, `metric.live_finish`).
- An app probe script may also return name and value pairs through a script call of the platform (iOS `callAsyncJavaScript` in the page world).
- <Badge type="tip" text="FACT" /> On iOS a page script cannot see a message handler of another `WKContentWorld`, so `rqLab` is in the page world; the bridge of IOS-P2 is in its own content world (iOS S1, CAP-I-04).
- On Android `rqLab` is in the page world too (the host page scripts call it); the boundary is the origin rule of `addWebMessageListener` and the checks of each message.

## 7. Capability catalog

`tools/platform-spikes/capabilities.json` (architect-owned; a lane sends its result rows in its report):

```json
{
  "id": "CAP-A-01",
  "platform": "android",
  "title": "Main-frame response streamed through shouldInterceptRequest renders progressively",
  "prototype": "A-STREAM",
  "questions": ["RSK-01"],
  "sprint": "S1",
  "results": [
    {
      "os": "37",
      "webview": "com.google.android.webview 1xx.x",
      "verdict": "supported",
      "condition": null,
      "runs": ["evidence/spikes/wp-04/android/CAP-A-01/37/run-1.jsonl"],
      "recording": "evidence/recordings/wp-04/CAP-A-01-android-37.mp4"
    }
  ],
  "verdict": "supported"
}
```

- `verdict` of a result: `supported`, `conditional`, `impossible`, `not-tested`. `conditional` needs a `condition` sentence. `impossible` needs a `source` (URL of the platform documentation) or a run that shows the failure.
- `verdict` of the capability: `pending` until all versions of H-128 have a result; then the weakest result (`impossible` < `conditional` < `supported`; `not-tested` keeps `pending`).
- A `supported` or `conditional` result, and any result that lists runs, needs at least 3 runs with the same normalized log (section 8) and, from S3, a recording. A `not-tested` result needs no run; an `impossible` result with a `source` needs no run.
- Sizes: 1 MB = 1,000,000 bytes.
- A measurement result has `"kind": "benchmark"`: it needs a `metrics` file (for FFI the result file of section 10) and has no runs; its stability comes from the repeated documents inside the measurement.
- `metrics` (optional, FFI and timing): path to a metrics JSON.

## 8. Determinism

Normalization: remove `t` and `seq`; remove `k` from `run.start`; replace every occurrence of the run id (section 4) in any string value with `<run>` (it is in `run.start.run`, in `run=` query values, in host targets; the CSP nonce form with `_` for `.` too); remove `app.stream.read` events (their count depends on buffer sizes) but keep the sum of their `bytes`; remove `metric.*` probes; blank `Date` values. Comparison: app events in order, host events in order, each source on its own. A run that is not stable gives no verdict.

## 9. Evidence layout

```text
evidence/spikes/wp-04/
├── android/<capability>/<api>/run-<k>.jsonl, run-<k>.metrics.json, run.json
├── ios/<capability>/<os>/run-<k>.jsonl, run-<k>.metrics.json, run.json
└── ffi/<target>.json                      # section 10
evidence/recordings/wp-04/<capability>-<platform>-<os>.mp4   # ≤ 1 MB, ≤ 30 s; total ≤ 25 MB
```

`run.json`: date, tool versions (Xcode, AGP, Kotlin, Swift), device model, OS build, WebView version, user agent, app commit.

## 10. FFI benchmark

One Rust crate `ffi-bench/` with its own `[workspace]` (as DL-084). The Rust side makes a deterministic document of `size` bytes (byte `i` is `(i * 31 + 7) mod 251`) and gives it in chunks of `chunk` bytes, pull model.

| Path | Interface |
|---|---|
| UniFFI | `fn open_stream(size: u64, chunk: u32) -> Arc<DocStream>`; `DocStream::next_chunk(&self) -> Option<Vec<u8>>` (UniFFI copies the bytes out) |
| C ABI | `rq_bench_open(size: u64, chunk: u32) -> *mut RqBenchStream`; `rq_bench_read(s, buf: *mut u8, cap: usize, out_len: *mut usize) -> i32` (copies into the caller buffer); `rq_bench_close(s) -> i32` (0, or `-1` for a handle that is not open: a second close is a defined error). Codes: `0` OK, `1` END, `-1` E_HANDLE, `-2` E_NULL, `-3` E_ARG, `-4` E_PANIC, `-5` E_JNI. The handle is an opaque registry id, never a dereferenced pointer. Limits: chunk 1..=65536, size ≤ 256 MiB, at most 1024 open streams. Kotlin calls it through JNI (`jni` crate) with a reused direct `ByteBuffer` and with a `ByteArray`; Swift calls the hand-written C header |

Measure for each path and target: 1 MiB in 16 KiB chunks (64 reads), and 1 MiB in 64 KiB; 200 warm-up documents, then 1000 measured documents. Report per document: median and p95 time (ns), per-chunk median (ns), bytes copied across the boundary (counted by the Rust side and by the foreign side), peak resident memory change. Also check each document with SHA-256 against the Rust value.

Result file `evidence/spikes/wp-04/ffi/<target>.json` (`<target>`: `host-jvm`, `host-swift`, `android-37`, `ios-sim-27.0`, …):

```json
{"target":"host-jvm","date":"2026-11-..","machine":"Apple M4 Pro","os":"macOS 26.6.2","toolchain":{"rust":"…","uniffi":"0.32.2","kotlin":"…","jna":"…"},
 "indicative":true,
 "cases":[{"path":"uniffi-kotlin","size":1048576,"chunk":16384,"docs":1000,"median_ns":0,"p95_ns":0,"chunk_median_ns":0,"copied_bytes":0,"rss_delta_kib":0,"sha256_ok":true}]}
```

`indicative` is `true` for every WP-04 result (H-129). Optional case members `rust_copied_bytes` and `glue_copied_bytes` split `copied_bytes` into the copy on the Rust side and the copies in the binding glue. `open_stream` with a size or chunk outside the limits gives a stream that is already at its end (fail closed: the SHA-256 check of the harness then fails); the benchmark keeps this signature, an SDK API returns an error instead.

## 11. Commands

| Task | Command |
|---|---|
| Lab environment | `sh tools/platform-spikes/lab/env.sh` (then `lab/emulator.sh start 37`, `lab/sim.sh create 27.0`) |
| Spike host | `node tools/platform-spikes/host/main.mjs` (needs `rust-reference` built; prints `listening 18443 18444 18445`) |
| Host tests | `cd tools/platform-spikes/host && npm test` |
| Evidence check | `node tools/platform-spikes/check/check.mjs` |
| Android run | `sh tools/platform-spikes/android/run.sh <capability> <api> [k]` |
| iOS run | `sh tools/platform-spikes/ios/run.sh <capability> <os> [k]` |
| FFI benchmark | `sh tools/platform-spikes/ffi-bench/run.sh host` |

Every script sources `tools/platform-spikes/lab-env.sh`.
