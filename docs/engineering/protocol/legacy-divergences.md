# Legacy protocol: divergences and parity

::: warning Legacy mode — temporary
This page describes the **legacy mode** (compatibility with the VasSonic legacy protocol). It ships as an optional module, is **deprecated from 1.0** and is **removed in 1.5**. New integrations use the [Rivqen protocol](/engineering/protocol/markup). See [Modes and legacy deprecation](/engineering/protocol/versioning).
:::

The upstream project has five implementations of legacy protocol (Java, Node.js and PHP servers; Android and iOS clients). They do not behave identically. This page lists every known divergence, the Rivqen decision, and the parity matrix that defines "compatible".

**Status:** <Badge type="tip" text="FACT" /> divergences from code at `59936bef`. Decisions are <Badge type="info" text="DESIGN" /> pending ADR-005.

[[toc]]

## 1. Server divergences

| # | Topic | Java | Node.js | PHP | Rivqen canonical |
|---|---|---|---|---|---|
| D-01 | Legacy request detection | `accept-diff == "true"` gates only `304` and cache headers; data response depends on `template-tag` alone | `accept-diff` present (any value) and gzip/deflate accepted | `accept-diff === "true"`; header name case-sensitive | `accept-diff == "true"` (case-insensitive name) gates all legacy behavior |
| D-02 | Headers on non-legacy responses | Adds `Etag`, `template-tag`, `template-change` to all text responses | Only on legacy path | Only on legacy path | Only on legacy path |
| D-03 | `304` comparison | Case-insensitive | Case-sensitive | Case-sensitive | Case-insensitive, after removing quotes and `W/` |
| D-04 | Title replace | All occurrences, case-sensitive replace | First, case-insensitive | All, case-insensitive | First, case-insensitive |
| D-05 | Empty block name | `{}` | `{auto0}`… | `{}` | Block not extracted |
| D-06 | Page without markers | Data response with title only | Always full HTML, `template-change: true` | Data response with title only | Full HTML on first load; data response with title when the template matches |
| D-07 | `etag` header name | `Etag` | `Etag` (published `sonic_differ` 1.0.7); `sonic-etag-key` only in the unused repo file `common/diff.js` | `Etag` | `etag`; honor `sonic-etag-key` when reading |
| D-08 | JSON key order and escaping | `HashMap` order; Gson HTML-escapes `<`, `>` as `<`… | Insertion order; no HTML escaping | Insertion order; escapes `/` as `\/` and non-ASCII as `\uXXXX` | Semantic JSON equality in tests; Rivqen emits UTF-8, no HTML escaping |
| D-09 | `Cache-Control` on legacy responses | `no-cache` | none | `no-cache` | `no-cache` + `private` |
| D-10 | Compression | Not handled | Gzip/deflate in middleware; legacy processing **skipped** if the client accepts only `identity` | Left to the web server | Independent of `Accept-Encoding` |
| D-11 | Content type of data responses | Unchanged (`text/html`) | Unchanged (`text/html`) | Unchanged | `text/html` in strict profile; `application/json` in Rivqen profile (clients accept both) |
| D-12 | `Cache-Offline` on `304` | Two fields: `true` **and** `store` | `store` | `store` (replaces earlier `true`) | One field: `store` |
| D-13 | Several `<title>` elements | Value from first match (case-insensitive), replacement case-sensitive → wrong element replaced | First title only | All titles replaced, first value stored | Not applicable in RQP; legacy module: Node.js behavior |

All server rows are confirmed by captured traces ([server trace report](/engineering/protocol/legacy-traces)). Evidence: Java `upstream:sonic-java/src/main/java/com/github/tencent/SonicFilter.java:60-149`; Node.js `upstream:sonic-nodejs/common/diff.js:12-122`, `upstream:sonic-nodejs/middleware/compress.js:18-83`; PHP `upstream:sonic-php/sdk/sonic.php:50-145`.

## 2. Client divergences

| # | Topic | Android | iOS | Rivqen canonical |
|---|---|---|---|---|
| D-20 | Session id base | `authority + path` (no scheme) | `scheme://host + path` (no port) | Not used as cache key; reproduced only per platform profile |
| D-21 | Kept-parameter order | Sorted | Undefined | Sorted |
| D-22 | Marker regex flags | Case-sensitive | Case-insensitive | Case-insensitive |
| D-23 | Block name source | Opening marker | Closing marker | Opening marker |
| D-24 | Client-side title rule | Literal `<title>` after block extraction | None | Same as server canonical rule |
| D-25 | Diff of new keys | Included | Ignored | Included |
| D-26 | Result JSON types | Numbers, boolean | Strings | Numbers and booleans; shim can emit strings for iOS-era pages |
| D-27 | Web view integration | `shouldInterceptRequest` + `loadDataWithBaseURL` | `UIWebView` + global `NSURLProtocol` | Public APIs only (see platform pages) |

Evidence: [legacy client behavior](/engineering/protocol/legacy-client) and [markers grammar](/engineering/protocol/legacy-markers).

## 3. Upstream behaviors that Rivqen will not reproduce

These behaviors are documented facts from the code. Rivqen replaces them with safe behavior, even in legacy mode. Each item becomes a `SEC-` ticket and a fixture that proves the safe behavior.

| # | Upstream behavior | Risk | Rivqen behavior | Evidence |
|---|---|---|---|---|
| U-01 | Java filter stores extracted blocks in **static** fields shared by all requests (`TemplateReplace.diffTagNames`) | Under concurrent requests, one user's data blocks can appear in another user's response | Per-request state only | `SonicFilter.java:20-48, 124-130` |
| U-02 | Cache key ignores ordinary query parameters | Different pages (or users, if IDs are in the query) share one cache entry | Full canonical URL + partition | `SonicRuntime.java:75-113` |
| U-03 | Account name in clear text in cache file names | Account disclosure from the file system | Keyed hash partition | `SonicRuntime.java:105-106` |
| U-04 | JS bridge via `addJavascriptInterface` and `javascript:` URL built from a page-supplied callback name | Any frame can call; script injection through the callback name | Origin-checked message bridge | sample `SonicJavaScriptInterface.java:51-71` |
| U-05 | Quick mode loads HTML as data without response security headers | CSP and other headers lost | Security headers are always preserved, or the page loads normally | `QuickSonicSession.java:39-43` |
| U-06 | Subresource cache on external storage `/sdcard/SonicResource/` | Readable and writable by other apps on old Android | App-private storage | `assets/VasSonic3.0_preload.md` §1.3 |
| U-07 | `Cache-Control` ignored unless `SUPPORT_CACHE_CONTROL` is on | `no-store` responses can be cached | `no-store` always wins | `SonicSessionConfig.java:72`; `SonicUtils.java:575-596` |
| U-08 | `sonic-sdk-version` header on every request | SDK fingerprinting | Off by default | `SonicSessionConnection.java:361` |
| U-09 | `Etag` value unquoted (not a valid HTTP entity-tag) | Intermediaries can mishandle it | Rivqen server emits the legacy value unquoted for compatibility, plus an RFC 9110 `ETag` only in RQP | `SonicFilter.java:103` |
| U-11 | Java and PHP corrupt pages with more than one `<title>` element (trace S16) | The page title is copied into another element (for example an SVG title) on rebuild | No implicit title rule in RQP; Node.js rule in the legacy module | `SonicFilter.java:110-112`; `sonic.php:99-104` |
| U-10 | iOS integration with `UIWebView` and global `NSURLProtocol` | `UIWebView` is no longer accepted in the App Store; `NSURLProtocol` does not intercept `WKWebView` through public API | WKWebView with public APIs (ADR-003) | `sonic-iOS/SonicSample/SonicSample/SonicWebViewController.m:71`; `AppDelegate.m:35` |

## 4. Parity matrix {#parity-matrix}

The parity matrix lists every upstream feature and how Rivqen provides it. 1.0 requires every row to be **Exact**, **Equivalent**, or a documented **Conditional / Unsupported** with an ADR.

| ID | Upstream feature | Rivqen | Level | Proof |
|---|---|---|---|---|
| PAR-01 | Early HTML fetch in parallel with WebView start | Parallel prepare and fetch in adapters | Exact (Android), Conditional (iOS) | Waterfall timestamps |
| PAR-02 | Bridge stream (cached prefix + network rest) | Bounded stream model | Exact (Android), Conditional (iOS) | First byte vs first paint on devices |
| PAR-03 | Quick mode | `quick` compatibility behavior on Android | Equivalent (security headers kept) | Golden Quick traces |
| PAR-04 | Standard mode | `standard` compatibility behavior | Exact | Golden Standard traces |
| PAR-05 | Instant display of cached page | L1/L2 snapshots | Exact | Airplane-mode test |
| PAR-06 | `304` hit | Validators + `304` | Exact | `FX-LEG-304` |
| PAR-07 | Template/data split by markers | Formal parser, canonical profile | Exact for canonical; profiles for divergences | Marker fixtures |
| PAR-08 | Data update through JS callback | Validated diff through secure bridge (+ shim) | Equivalent | Bridge tests |
| PAR-09 | Template change → full refresh | Atomic replace | Exact | `FX-LEG-TEMPLATE-CHANGE` |
| PAR-10 | `cache-offline` values `true`, `false`, `store`, `http` | Implemented in legacy mode | Exact (`no-store` overrides) | Four fixtures |
| PAR-11 | Local server mode (VasSonic 2.0) | Client-side split for non-legacy servers | Equivalent, opt-in | Fixture with plain server |
| PAR-12 | Subresource preload (`sonic-link`, VasSonic 3.0) | Budgeted prefetch in app-private storage | Equivalent | Prefetch tests |
| PAR-13 | React SSR demo (Next.js, Redux, Koa) | TS core, React hooks, SSR adapters | Equivalent | React SSR suite |
| PAR-14 | Java server | Java SDK (Servlet, Spring Boot) | Exact on canonical profile | Conformance |
| PAR-15 | Node.js server | Node SDK (Fetch, Express, Fastify) | Exact on canonical profile | Conformance |
| PAR-16 | PHP server | PSR-15 middleware | Exact on canonical profile | Conformance |
| PAR-17 | iOS SDK | Swift + WKWebView + Rust core | Conditional (ADR-003) | iOS parity matrix |
| PAR-18 | Android SDK | Kotlin + AndroidX WebKit + Rust core | Exact | Android matrix |
| PAR-19 | HTTP stack | Platform HTTP/1.1–3 | Equivalent | Network suite |
| PAR-20 | Memory and cache limits | Bounded L1/L2, pressure callbacks | Improved | Memory profiles |
| PAR-21 | Statistics callbacks | Typed metrics | Improved | Telemetry tests |
| PAR-22 | Security posture | TLS/CSP/origin/partition/OWASP | Improved (U-01…U-10) | Security gate |
| PAR-23 | Custom request/response headers | Config hooks | Equivalent | Unit tests |
| PAR-24 | IP-direct connection with SNI | Not planned (platform stacks handle DNS) | Unsupported — ADR needed | — |

## Related

- [ADR-005](/engineering/architecture/adr/#adr-005)
- [Security controls](/engineering/security/controls)
