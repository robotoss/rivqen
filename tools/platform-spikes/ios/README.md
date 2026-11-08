# iOS prototype (WP-04)

<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- SPDX-FileCopyrightText: The Rivqen Authors -->

A SwiftUI app with one mode for each capability CAP-I-01 to CAP-I-07 and CAP-I-11 of [`capabilities.json`](../capabilities.json): IOS-P1 (`loadSimulatedRequest`, `loadHTMLString` with a base URL), IOS-P2 (real navigation with a cached snapshot first and a bridge update), IOS-P3 (own scheme `rqlab` with a `WKURLSchemeHandler`). Contract: [CONTRACT.md](../CONTRACT.md). Results: [iOS SDK](../../../docs/engineering/platforms/ios.md) §2 to §4.

Public API only: no private WebKit API, no global `URLProtocol`, no ATS exception (the app has no `NSAppTransportSecurity` key), no trust override. TLS trust of the lab CA comes from `lab/sim.sh trust` on a `rivqen-p2-*` simulator. No third-party dependencies.

## Layout

| Path | Content |
|---|---|
| `SpikeCore/` | SwiftPM package, no WebKit: run log (`EventLog`, canonical JSON), launch arguments, probe parsing, chunk marker scanner and splitter, `rqlab` to `https` mapping, field rules. `swift test` on the Mac |
| `App/` | Xcode project `SpikeApp.xcodeproj` (synchronized folder `SpikeApp/`, local package `SpikeCore`), iOS 18.0+, Swift 6, strict concurrency, warnings are errors, ad-hoc signature for the simulator |
| `run.sh` | One capability, one or more runs (section "Run") |
| `join.mjs` | Joins script, app and host parts into `run-<k>.jsonl`; writes `run-<k>.metrics.json` and `run.json` |
| `test/` | `node --test` for `join.mjs` |

## Run

1. Start the spike host (or use the running one): `node tools/platform-spikes/host/main.mjs`.
2. Set the lane tag when another lane uses simulators at the same time (CONTRACT.md section 11): `export LAB_SIM_TAG=ios`. Then the device is `rivqen-p2-ios-<os>`, and `sim.sh` and `run.sh` use only that device.
3. Make the lab simulator: `sh tools/platform-spikes/lab/sim.sh create 27.0`, then `boot 27.0`, then `trust 27.0`.
4. Run a capability: `sh tools/platform-spikes/ios/run.sh CAP-I-01 27.0 1..3`.
5. Check the evidence: `node tools/platform-spikes/check/check.mjs`.
6. Delete the lab simulator: `sh tools/platform-spikes/lab/sim.sh delete 27.0`.

`run.sh` builds into `$LAB_DERIVED/ios-spike` (`SKIP_BUILD=1` skips the build), installs the app, and for each `k`: `POST /run/start`, `simctl launch` with `-rqRun <capability>-ios-<os>-<k> -rqCap <capability> -rqOrigin https://localhost:18443`, waits for `Documents/spike-output/DONE` in the app data container, `POST /run/end`, joins the parts into `evidence/spikes/wp-04/ios/<capability>/<os>/`. A run that is not `ok` goes to `$LAB_OUT/ios-failed/` and the script exits 1. The script never starts or stops the host and never touches a simulator without the lab prefix.

Checks: `cd tools/platform-spikes/ios/SpikeCore && swift test`, `node --test tools/platform-spikes/ios/test`, `xcrun swift-format lint --strict -r SpikeCore/Sources SpikeCore/Tests App/SpikeApp`.

## What each capability does

Every run uses a new `WKWebView` with `WKWebsiteDataStore.nonPersistent()` and one ephemeral `URLSession`, so each run starts without cookies or cache.

| Capability | Steps |
|---|---|
| CAP-I-01 | Native fetch of `/s/stream?chunks=1&size=4096`, then the web view is made, then `loadSimulatedRequest` with the fetched status, the fetched fields (transfer fields removed, `Content-Type: text/html; charset=utf-8`, H-66) and the bytes. Probes: `location`, `origin`, `content_type`, `charset`, `title`, `metric.first_paint` |
| CAP-I-02 | Native fetch of `/s/echo?html=1`; `loadSimulatedRequest`, then (new web view) `loadHTMLString` with the page URL as base URL. Probes for each: `location`, `origin`, `base`, `relative` (an `a.href` of `rel/x?y=1`), `echo` (a relative same-origin `fetch`) |
| CAP-I-03 | Native fetch of `/s/cookie/set` (`rq-native`), `loadSimulatedRequest` with its `Set-Cookie`; the page sets `rq-web` (`HttpOnly`, by `fetch`) and `rq-js` (script); probes `cookie`, `echo`; `native.wk_cookies` (cookie names in `WKHTTPCookieStore`); `native.echo` (a native fetch with the run session) |
| CAP-I-04 | IOS-P2: snapshot by `loadSimulatedRequest` of a native copy of `/s/echo?html=1`; then `load(URLRequest)` of the same URL; the snapshot stays until the commit. The bridge script in the content world `rivqen-bridge` asks `rqBridge` (a `WKScriptMessageHandlerWithReply`) for an update; after `didFinish` the app fetches `/s/echo` natively and replies; the script writes `<pre id="rq-data">`. Probes: `bridge.ready`, `bridge.applied`, `data` (page world reads the DOM), `bridge.page_world` (`typeof` the handler in the page world) |
| CAP-I-05 | IOS-P3: `rqlab://localhost:18443/s/stream?chunks=4&size=2048&gate=beacon&timeout=3000` through the scheme handler. No `host.gate_timeout` means the beacon of chunk k (a request of the parsed chunk) reached the host before chunk k+1 existed |
| CAP-I-06 | IOS-P3: a real navigation to site A sets `rq-site` (`Secure`, `SameSite=None`); then `rqlab://.../s/cookie/set` (`rq-scheme`). Probes: `location`, `origin`, `cookie` (before and after a script cookie), `echo` (relative, so through the handler), `https.cors`, `https.no_cors`, `https.image` (requests to site A); `native.wk_cookies` |
| CAP-I-07 | `native.handles_url_scheme.https` and `.rqlab` (`WKWebView.handlesURLScheme`); then the same `gate=beacon` page by IOS-P1 (native fetch first, beacons blocked by a content rule list), IOS-P2 (`load(URLRequest)`), IOS-P3 (`rqlab`) |
| CAP-I-11 | Cookie sync (Q-12), one cookie per attribute (`CookiePlan`: none, `HttpOnly`, `Secure; SameSite=Strict`, `SameSite=Lax`, `Secure; SameSite=None`) on site A and site B. **Native → web view:** native fetches of `/s/cookie/set` (`na-*`, `nb-*`); `native.url_store` (attributes in the session `HTTPCookieStorage`); native echoes `native.echo.a`, `native.echo.b`, `native.echo.b_cross` (`mainDocumentURL` of site A); each session cookie is copied with `WKHTTPCookieStore.setCookie` before the load; `native.wk_store`; IOS-P1 page on site A: `cookie`, `echo` (same origin), `https.no_cors` and `frame.echo` (site B as a subresource and as an iframe), then a page-started top-level navigation to site B (`location`, `cookie`, `echo`). **Web view → native:** a new web view and a new native session; `load(URLRequest)` of site A and site B, same-origin page fetches set `wa-*`/`wb-*` and a script sets `wa-js`/`wb-js`; `native.wk_store`; `getAllCookies` copied into the native `HTTPCookieStorage`; `native.url_store`; the three native echoes |

## Probe channel and bridge

- `window.rqLab.probe(name, value)` (CONTRACT.md section 6) is a user script in the **page world**, with the message handler `rqLab` in the page world. The host pages call `rqLab` from their own scripts, and Apple documents that a message handler is defined "in all frames in the specified content world" only ([`add(_:contentWorld:name:)`](https://developer.apple.com/documentation/webkit/wkusercontentcontroller/add(_:contentworld:name:))), so a handler of an isolated world is not visible to page scripts. The run confirms it: `bridge.page_world` is `undefined` in CAP-I-04.
- The handler takes a message only from a frame with a lab origin (`https` or `rqlab`, host `localhost` or `127.0.0.1`, port 18443); a page cannot write a `native.*` probe; names are `[a-z][a-z0-9._-]{0,63}`; values have limits (depth 8, 256 items, 16 KiB text).
- The IOS-P2 bridge runs in its own content world `rivqen-bridge` (task security rule).
- App probe scripts run with `callAsyncJavaScript` in the page world and return `[name, value]` pairs; network steps run one after the other, so the host sees a fixed order.
- `native.*` probes are app-side facts (`native.step`, `native.webview`, `native.simulated_response`, `native.wk_cookies`, `native.echo`, `native.handles_url_scheme.*`). CAP-I-11 adds `native.url_store`, `native.wk_store` (one word per cookie: `name@domain path=/ [HttpOnly] [Secure] SameSite=<Strict|Lax|None|unset> session|persistent`), `native.echo.a`, `native.echo.b`, `native.echo.b_cross` (`{"site", "cookies": [sorted names]}`).
- CAP-I-11 page probes `cookie`, `echo` and `frame.echo` are logged as sorted cookie names (`CookieNames.pageView`): WebKit gives cookies that it made in the same millisecond in any order. `frame.echo` comes from a page-world user script in subframes (CAP-I-11 sessions only); the session keeps it, and the runner logs it after `frame.load`, so its place in the log is fixed.

## Ordering rules (determinism, CONTRACT.md section 8)

- The scheme handler forwards the beacon of chunk k only after the `chunk` probe k arrived (at most 2 s). The beacon and the probe come from the same chunk and reach the app in either order; the rule fixes the log order. A run where every beacon is on time shows that the script of chunk k ran before the host sent chunk k+1.
- IOS-P2 starts the data fetch after the live `didFinish`.
- CAP-I-07 IOS-P1 blocks the beacons of the simulated page with a `WKContentRuleList`: the body is already complete, and the parallel beacon requests would reach the host in any order.
- `date` is not logged in `app.fetch.head` fields (it changes on every run).
- CAP-I-11 copies cookies in sorted order (name, domain, path), so their creation order is the same in every run.
- The user agent is read after the run in a separate web view.

## Limits

Native fetch at most 8 MiB; at most 20,000 app events per run; one run at most 180 s; each load and script at most 20 s (`-rqTimeout`); `run.sh` waits at most `RUN_WAIT` (240 s) for `DONE`.
