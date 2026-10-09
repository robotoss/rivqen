# Upstream: VasSonic audit

This page summarizes the behavioral audit of Tencent VasSonic, done in milestone M1 (work package WP-01). The detailed, normative results are in the [Protocol](/engineering/protocol/) pages.

**Status:** <Badge type="tip" text="FACT" /> All statements cite upstream files at the pinned commit. Audit type: **behavioral** (protocol- and behavior-relevant code), not a line-by-line review of every file.

[[toc]]

## 1. Pinned source

| Item | Value |
|---|---|
| Repository | [github.com/Tencent/VasSonic](https://github.com/Tencent/VasSonic) |
| Commit | `59936beff656d4b5718ff6444d6c5e001a2c5231` |
| Commit date | 2019-04-15 (last commit on `master`) |
| Wiki commit | `a81e90dc5497af0470c96bdfb13125a95bf7c0db` (2018-02-09) |
| License | BSD-3-Clause; includes Android Source Code 4.4_r1 under Apache-2.0 |

## 2. What VasSonic is

VasSonic is a hybrid-page acceleration framework from Tencent's VAS team (2017–2019). Its stated goal is a fast first screen for HTML pages in mobile apps (upstream wiki `VasSonic入门须知.md`). Its main ideas:

1. The native app requests the main HTML in parallel with WebView initialization, then streams it into the WebView.
2. The server marks the page into a **template** and **data blocks** with HTML comments.
3. The client caches the page and revalidates it; the server answers with `304`, data only, or full HTML.
4. VasSonic 2.0 added a client-only "local server" mode. VasSonic 3.0 added subresource preload.

## 3. Module inventory

| Module | Language | Size (approx. lines of source and docs) | Role |
|---|---|---|---|
| `sonic-android` | Java | 11 900 | Client SDK + sample |
| `sonic-iOS` | Objective-C | 7 300 | Client SDK + sample (UIWebView) |
| `sonic-java` | Java | 600 | Servlet filter |
| `sonic-nodejs` | JavaScript | 1 350 | Koa demo + differ |
| `sonic-php` | PHP | 820 | Output-buffer helper |
| `sonic-react` | JavaScript | 1 100 | Next.js/Redux/Koa demo |
| `assets` | Markdown, images, PDF | 400 | Design history, VasSonic 2.0 and 3.0 notes |

## 4. Key findings

| # | Finding | Impact on Rivqen | Details |
|---|---|---|---|
| F-01 | The wire protocol is small: 3 request headers, 5 response headers, one JSON body format | legacy compatibility is feasible and testable with fixtures | [legacy wire contract](/engineering/protocol/legacy-wire) |
| F-02 | Five implementations share one marker regex but differ in title handling, empty names, case, detection and output | A canonical profile + named profiles are required | [Divergences](/engineering/protocol/legacy-divergences) |
| F-03 | Hashes are SHA-1 hex of UTF-8 text; etag values are unquoted | Reproducible; not HTTP-standard entity-tags | [legacy wire §4](/engineering/protocol/legacy-wire#_4-hashes) |
| F-04 | The client session id ignores normal query parameters and, on Android, the scheme | Rivqen must use a different cache key | [legacy client §1](/engineering/protocol/legacy-client#_1-session-identity) |
| F-05 | Java server keeps per-request data in static fields | Cross-request data mixing under load | U-01 |
| F-06 | iOS design depends on `UIWebView` + global `NSURLProtocol` | Not usable on current iOS; iOS needs a new design | [iOS SDK](/engineering/platforms/ios) |
| F-07 | Quick mode loads HTML as data and loses response headers (documented upstream) | Rivqen keeps security headers | U-05 |
| F-08 | Sample JS bridge uses `addJavascriptInterface` and builds `javascript:` URLs from a page-supplied name | Rivqen uses an origin-checked message bridge | U-04 |
| F-09 | `Cache-Control` is ignored by default on the client | Rivqen makes `no-store` always win | U-07 |
| F-10 | Documentation shows markers with spaces that the code does not match | Docs-vs-code conflicts exist; code is the source of truth | [Markers grammar](/engineering/protocol/legacy-markers) |
| F-11 | Dependencies are from 2017: Android SDK 25 / minSdk 9, React 16, Koa 2, `next: latest` | Nothing can be reused; all stacks must be current | [Technology stack](/engineering/architecture/technology-stack) |
| F-12 | The server modules have no automated tests (`sonic-nodejs/package.json` test script is a placeholder); Android has instrumented tests in `sdk/src/androidTest` | Golden fixtures must be created from code reading and captured traces | WP-01, WP-02 |
| F-13 | Server traces (115 checks) confirm the documented server behavior and add five findings: Java data response independent of `accept-diff`, two `Cache-Offline` fields on `304`, PHP header-name case sensitivity, title corruption with several `<title>` elements (Java, PHP), and the published `sonic_differ` without `sonic-etag-key` | RQP drops implicit rules; legacy module follows the canonical profile | [Server trace report](/engineering/protocol/legacy-traces) |

## 5. Historical performance claims

The upstream README and design notes make performance claims for Tencent's own apps (first-screen time reductions). They are **historical claims** without published method or raw data. Rivqen does not reuse them. Rivqen publishes its own measurements with method ([Benchmarks](/engineering/quality/benchmarks)).

## 6. Follow-up work (WP-01)

1. ✅ `evidence/source-manifest.lock` with SHA-256 hashes of all 329 upstream files.
2. ✅ HTTP traces from the Java, Node.js and PHP servers (`evidence/traces/server/`, lab in `tools/upstream-lab/`).
3. Capture Android client traces on an old emulator image (API 25) for Quick and Standard modes.
4. Close the remaining questions in [Open questions](/engineering/plan/open-questions).
