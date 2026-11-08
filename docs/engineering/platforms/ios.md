# iOS SDK

The iOS SDK connects the Rust core to `WKWebView`. iOS is the hardest platform for Rivqen, because it has no public API to intercept the standard HTTPS main-document load of a web view.

**Status:** <Badge type="info" text="DESIGN" /> <Badge type="danger" text="GATE" /> The navigation strategy is decided by the WP-04 spike and ADR-003.

[[toc]]

## 1. Upstream for comparison

<Badge type="tip" text="FACT" /> The upstream iOS SDK (Objective-C) registers a global `NSURLProtocol` subclass (`SonicURLProtocol`) and its sample uses `UIWebView` (`upstream:sonic-iOS/SonicSample/SonicSample/AppDelegate.m:35`; `SonicWebViewController.m:71`). The protocol class claims requests that carry a private marker header `sonic-load-type: __SONIC_HEADER_VALUE_WEBVIEW_LOAD__` and a `sonic-delegate-id` (`upstream:sonic-iOS/Sonic/Network/SonicURLProtocol.m:32-56`; `Sonic/SonicConstants.h:102-117`). The README integration example targets `platform :ios, '8.0'` and CocoaPods `VasSonic 3.0.0` (`upstream:sonic-iOS/README.md:7-15`).

<Badge type="tip" text="FACT" /> This design cannot be used today:

1. `UIWebView` is deprecated, and Apple no longer accepts new apps or updates that use it.
2. `NSURLProtocol` does not intercept `WKWebView` network loads through public API (WKWebView loads run in a separate process).

## 2. The core problem

| API | What it can do | What it cannot do |
|---|---|---|
| `WKURLSchemeHandler` | Serve responses for **custom** URL schemes | Intercept `https` loads |
| `loadHTMLString(_:baseURL:)` / `load(_:mimeType:characterEncodingName:baseURL:)` | Show HTML that the app fetched | Guarantee the same origin, cookies, CSP, Service Worker, history behavior as a real navigation |
| `loadSimulatedRequest(_:response:responseData:)` (iOS 15+) | Load a request with app-provided response and data. <Badge type="tip" text="FACT" /> iOS 27.0, 26.5 and 18.5: the page gets the URL and origin of the request, `text/html` and `UTF-8` (H-66); relative URLs and same-origin `fetch` go to that origin (CAP-I-01, CAP-I-02, §3). Cookies that the app copies into `WKHTTPCookieStore` before the load are there, with their attributes (CAP-I-11) | <Badge type="tip" text="FACT" /> No streaming: the body is one `Data` (CAP-I-07). WebKit ignores the `Set-Cookie` field of the given response (CAP-I-03). <Badge type="warning" text="RESEARCH" /> CSP and Service Worker: S2 |
| `load(URLRequest)` | Real navigation | Use bytes that the app already fetched |
| SwiftUI `WebView` / `WebPage` (iOS 26+) | Declarative web content | No interception of `https` either |

<Badge type="info" text="DESIGN" /> **Encoding (H-66).** Whatever prototype WP-04 selects, the SDK gives an RQP full document and every snapshot (also a patched or an offline snapshot) to the web view as `text/html` with the encoding `utf-8` ([negotiation §6.3](/engineering/protocol/negotiation#_6-3-response), "Delivery to the WebView"): `mimeType: "text/html"` and `characterEncodingName: "utf-8"` for `load(_:mimeType:characterEncodingName:baseURL:)`, a response with the MIME type `text/html` and the text encoding name `utf-8` for `loadSimulatedRequest`, and a string decoded from the bytes as UTF-8 for `loadHTMLString`. The SDK prefers the APIs that take bytes (`Data`) (DL-040, L-2). For `loadHTMLString` it removes a leading `EF BB BF` before it decodes the bytes. <Badge type="tip" text="FACT" /> Swift `String(decoding:as: UTF8.self)` keeps the byte order mark as U+FEFF; Foundation `String(data:encoding: .utf8)` removes it (Swift 6.4, macOS 26, checked on 2026-10-18). The SDK does not depend on the choice of the decoder.

<Badge type="info" text="DESIGN" /> **Origin of the final request (DL-040, L-3).** The SDK takes the origin for the allowlist and the cache key from the URL object that the network stack used: `URLResponse.url` (after redirects, the URL of the final response) or the `URL` of the `URLRequest` that it sent. The SDK knows about each redirect from its `URLSessionTaskDelegate` (`urlSession(_:task:willPerformHTTPRedirection:newRequest:completionHandler:)`); a result that came through a redirect never goes to the `public` partition (H-67). The SDK counts a response as not redirected only when its own task delegate saw the whole task and was not called for a redirect; in every other case (for example `URLSession.shared` without a task delegate, or a background session) it counts the response as redirected (DL-042, fail closed). <Badge type="tip" text="FACT" /> Apple documents for this method: "This method is called only for tasks in default and ephemeral sessions. Tasks in background sessions automatically follow redirects." So a background session does not tell the delegate about redirects, and the fail-closed rule above stays (checked 2026-10-18, [Apple documentation](https://developer.apple.com/documentation/foundation/urlsessiontaskdelegate/urlsession(_:task:willperformhttpredirection:newrequest:completionhandler:))). It does not parse a URL string again with another parser (for example `URLComponents(string:)`), because its rules can differ from the WHATWG URL parser (for example for `\` and IDN hosts). Whether `URL.host` is equal to the WHATWG host for every URL: NOT FOUND; the protocol fixtures `FX-RQ-C25-FINAL-BACKSLASH-*` and `FX-RQ-C25-IDN-HOST` check it in the SDK tests. A URL with a user name or a password gets no RQP (H-68).

Sources: [WKURLSchemeHandler](https://developer.apple.com/documentation/webkit/wkurlschemehandler), [loadHTMLString](https://developer.apple.com/documentation/webkit/wkwebview/loadhtmlstring(_:baseurl:)), [loadSimulatedRequest](https://developer.apple.com/documentation/webkit/wkwebview/loadsimulatedrequest(_:response:responsedata:)).

## 3. Prototypes (WP-04)

| Prototype | Mechanism | Benefits | Risks |
|---|---|---|---|
| **IOS-P1** | `URLSession` early fetch → `loadSimulatedRequest` (or `loadHTMLString` with base URL) | Early request; simple | Origin, cookies, CSP, history, Service Worker; no streaming |
| **IOS-P2** | Normal `load(URLRequest)` + cached page shown first + data updates through the bridge | True browser navigation and origin model | The prefetched bytes are not reused for the main document |
| **IOS-P3** | Custom scheme + `WKURLSchemeHandler` for trusted own content | Controlled streaming of local responses | Different origin; cookies and Service Workers differ |

**Forbidden shortcuts:** private WebKit API, global `NSURLProtocol` interception for `WKWebView`, disabling ATS or TLS validation, trusting all certificates, global cookie replacement, origin spoofing.

### Results on iOS 27.0, 26.5 and 18.5 (WP-04 S1, S2)

<Badge type="tip" text="FACT" /> Measured on 2026-11-08 with the prototype in `tools/platform-spikes/ios/` (public API only), Xcode 27.2, deployment target iOS 18.0, simulator iPhone 16 Pro. Each capability has 3 runs with the same normalized log on each version (`tools/platform-spikes/CONTRACT.md` §8). Runs: `evidence/spikes/wp-04/ios/<capability>/<os>/run-1.jsonl` to `run-3.jsonl`, with `run.json`. Times are indicative (simulator, H-129; the Mac had a high load during the S2 runs).

| Version | Build | WebKit | User agent names |
|---|---|---|---|
| 27.0 | 24A434 | 8625.1.29.10.29 | `iPhone OS 18_7` |
| 26.5 | 23F77 | 8624.2.5.10.4 | `iPhone OS 18_7` |
| 18.5 | 22F77 | 8621.2.5.10.10 | `iPhone OS 18_5` |

| Capability | 27.0 | 26.5 | 18.5 |
|---|---|---|---|
| CAP-I-01 IOS-P1 render, first render | Supported | Supported | Supported |
| CAP-I-02 IOS-P1 location, origin, relative URLs | Supported | Supported | Supported |
| CAP-I-03 IOS-P1 cookies | Conditional | Conditional | Conditional |
| CAP-I-04 IOS-P2 snapshot, navigation, bridge | Supported | Supported | Supported |
| CAP-I-05 IOS-P3 streaming | Supported | Supported | Supported |
| CAP-I-06 IOS-P3 origin, cookies, subresources | Conditional | Conditional | Conditional |
| CAP-I-07 https main-document streaming | Impossible | Impossible | Impossible |
| CAP-I-11 cookie sync, both directions | Conditional | Conditional | Conditional |

<Badge type="tip" text="FACT" /> **Same behavior on the three versions.** The normalized logs of CAP-I-01 to CAP-I-07 on 26.5 are equal to those on 27.0. On 18.5 the only difference is that WebKit sends no `Priority` request field (so the echo bodies are shorter). In CAP-I-11 the cookie behavior is the same on all three versions; only the reported `HTTPCookie.sameSitePolicy` value differs (see the cookie sync below).

<Badge type="tip" text="FACT" /> **iOS 18.5 simulator launch.** An app with a deployment target before iOS 18.4, built with Xcode 27.2, that calls a Swift-refined WebKit API (here `callAsyncJavaScript(_:arguments:in:in:completionHandler:)` and `evaluateJavaScript(_:in:in:completionHandler:)`) stops at launch on the iOS 18.5 simulator runtime (22F77): "Library not loaded: /usr/lib/swift/libswiftWebKit.dylib". The SDK binds these symbols to that library for deployment targets from 14.0 up to (not including) 18.4 (`$ld$previous` entries in `libswiftWebKit.tbd`); the 18.5 runtime has the library only in its OS cryptex. This is [WebKit bug 293831](https://bugs.webkit.org/show_bug.cgi?id=293831) ("This happens to apps that have a deployment target of iOS 18.3 or older"; fixed in WebKit, not in this runtime). The lab keeps deployment target 18.0 and uses the workaround of that bug for the app process only: `run.sh` sets `DYLD_FALLBACK_LIBRARY_PATH` to the cryptex folder for 18.4 to 18.x runtimes, and `run.json` names it (`launch_env`). Whether a device with iOS 18.4 or later has the same problem: NOT FOUND (no device, H-129). Input for WP-09 and WP-21 (minimum version): test the SDK sample app on an 18.4+ runtime with the lowest deployment target.

**IOS-P1.** The `URLSession` fetch starts before the web view exists and ends in 17 to 30 ms. `loadSimulatedRequest` with the fetched status and fields (`Content-Type: text/html; charset=utf-8`) and the fetched bytes renders the page: its scripts run, `document.contentType` is `text/html`, `document.characterSet` is `UTF-8`. In a new web view the commit comes 367 to 732 ms after the load call (27.0: 704 to 732; 26.5: 502 to 511; 18.5: 367 to 370); first contentful paint is 26 to 97 ms after the navigation start (CAP-I-01). After `loadSimulatedRequest` and after `loadHTMLString` with the page URL as base URL, `document.location`, `document.baseURI` and `window.origin` are those of the page URL; a relative link resolves against it, and a relative `fetch` goes to the host as a same-origin request (CAP-I-02). WebKit does not store a `Set-Cookie` of the simulated response and does not share the cookies of the app's `URLSession`. Cookies that the page sets (an `HttpOnly` cookie by `fetch`, a script cookie) are in `WKHTTPCookieStore` and go with same-origin `fetch` (CAP-I-03). Condition: the SDK copies the cookies of its native fetch into `WKHTTPCookieStore` before the load. With this sync in place the page has the native cookies (CAP-I-11, below).

**IOS-P2.** The snapshot (a native copy of the page, by `loadSimulatedRequest`) stays on screen until the real `load(URLRequest)` commits, 28 to 363 ms later (27.0: 348 to 363; 26.5: 28 to 34; 18.5: 37 to 58). The real page is the WebView's own network answer (its request fields differ from the native ones). After `didFinish`, the bridge script in its own `WKContentWorld` asks the app through `WKScriptMessageHandlerWithReply`, the app fetches the data natively and replies, and the script writes it into the DOM. The page world reads the update from the DOM, and `window.webkit.messageHandlers.rqBridge` is `undefined` there (CAP-I-04). The prefetched page bytes are not used for the real navigation.

**IOS-P3.** A `WKURLSchemeHandler` for `rqlab` that forwards each `URLSession` read with `didReceive(_:)` streams the page: with `gate=beacon` the beacon of every chunk reaches the host before the next chunk exists, and the script of chunk k runs before the host sends chunk k+1 (CAP-I-05, no `host.gate_timeout` in 3 runs on each version). Relative URLs of the page (the beacons) resolve against the `rqlab` base and come back to the handler, which forwards them to the host. The page origin is `rqlab://localhost:18443`; `document.cookie` is empty and a script cannot set a cookie; WebKit does not store a `Set-Cookie` of the handler response. Relative requests carry only the cookies of the app's `URLSession`. `https` requests from the page carry the cookies of the `https` site (a `SameSite=None; Secure` cookie in the test), but a script can read a cross-origin response only with CORS for the `rqlab` origin (CAP-I-06).

**https streaming (CAP-I-07).** `WKWebView.handlesURLScheme("https")` is `true`; Apple documents that `setURLSchemeHandler` raises an exception for a scheme that WebKit handles, such as `https` ([source](https://developer.apple.com/documentation/webkit/wkwebviewconfiguration/seturlschemehandler(_:forurlscheme:))). The run shows what each prototype does with the same `gate=beacon` page: IOS-P1 must have the whole body first (the native fetch waits for 2 gate timeouts, then the page renders at once); IOS-P2 lets WebKit stream the page itself, but the app sees no bytes; IOS-P3 streams through the app, on `rqlab` only. So the app cannot stream an `https` main document with public API.

**Cookie sync (CAP-I-11, Q-12).** <Badge type="tip" text="FACT" /> One cookie per attribute on site A (`localhost`) and site B (`127.0.0.1`): no attribute, `HttpOnly`, `Secure; SameSite=Strict`, `SameSite=Lax`, `Secure; SameSite=None`. Same results on 27.0, 26.5 and 18.5.

| Step | Result |
|---|---|
| Native fetch gets `Set-Cookie` | The ephemeral session's `HTTPCookieStorage` keeps `HttpOnly`, `Secure` and `SameSite=Strict`/`Lax`. Native fetches to A and B send all cookies of that site |
| Native → web view | Each session cookie, copied with `WKHTTPCookieStore.setCookie` (completion awaited) before `loadSimulatedRequest`, keeps its attributes. `document.cookie` shows all but the `HttpOnly` cookie; a same-origin `fetch` sends all five |
| Cross-site from the page | A `no-cors` fetch and an iframe of site B send no cookie, also not `SameSite=None; Secure`. A top-level navigation to B that the page starts sends all B cookies except `SameSite=Strict` |
| Web view → native | Cookies that the page sets (by `fetch` `Set-Cookie`, by script) come out of `getAllCookies` with their attributes; copied into a new session's `HTTPCookieStorage` (`setCookie`), they keep them, and native fetches to A and B send all |
| `SameSite` in `URLSession` | A native fetch to B with `mainDocumentURL` of A sends the `Strict` and `Lax` cookies of B too |
| `sameSitePolicy` value | 27.0: Foundation gives `none` for `SameSite=None`, and WebKit gives `none` also for a cookie without `SameSite`. 26.5 and 18.5: both are `nil`. `Strict` and `Lax` are the same on all versions |

Apple documents that a `nil` `sameSitePolicy` means "cross-site requests include the cookie" ([sameSitePolicy](https://developer.apple.com/documentation/foundation/httpcookie/samesitepolicy)), and that `mainDocumentURL` "is used for the cookie 'same domain as main document' policy" ([mainDocumentURL](https://developer.apple.com/documentation/foundation/urlrequest/maindocumenturl)); a documented `SameSite` rule of `URLSession`: NOT FOUND. WebKit blocks cookies of cross-site resources by default ([full third-party cookie blocking](https://webkit.org/blog/10218/full-third-party-cookie-blocking-and-more/)), and since iOS 14 Intelligent Tracking Prevention "is enabled by default in all WKWebView applications" ([App-Bound Domains](https://webkit.org/blog/10882/app-bound-domains/)). Copying 10 cookies one after the other with `setCookie` took 337 to 358 ms on every version (indicative); copying into `HTTPCookieStorage` took less than 1 ms.

**Q-12 on iOS (who owns the cookies).** <Badge type="info" text="DESIGN" /> From the CAP-I-11 runs, a proposal for the architect (Q-12 stays open until the decision):

1. The `WKHTTPCookieStore` of the session's `WKWebsiteDataStore` is the store of record. WebKit applies `HttpOnly`, `SameSite` and third-party blocking; `URLSession` does not apply `SameSite`.
2. Before a native fetch, the SDK copies the cookies for the request URL from `WKHTTPCookieStore` (`getAllCookies`) into the request's session. After the response, it writes each `Set-Cookie` cookie into `WKHTTPCookieStore` and waits for the `setCookie` completion before it loads the page (this is the iOS row of the [cookie table](/engineering/transport/http#_2-cookie-ownership); the runs confirm that both copies keep the attributes).
3. Because `URLSession` sends `SameSite=Strict` and `Lax` cookies in every context, the native early fetch is only for navigations that WebKit would send with all cookies of the target site (the app starts them, or a same-site page starts them). A navigation that a cross-site page starts goes to WebKit as a normal navigation. Not measured: whether WebKit sends `SameSite=Strict` cookies on a `load(URLRequest)` that the app starts (the S2 runs had no cookies at that point). This is a security rule, so the human decides it (options in the WP-04 S2 T-04 task report).
4. The SDK does not read `sameSitePolicy` to tell `None` from "not set" (the value differs by iOS version); it copies the `HTTPCookie` object as it is.

<Badge type="tip" text="FACT" /> The user agent of `WKWebView` names `iPhone OS 18_7` on the iOS 27.0 and 26.5 simulators, and `iPhone OS 18_5` on iOS 18.5 (`run.json`).

## 4. Spike matrix (gate G2)

Test each prototype on supported iPhone and iPad models and iOS versions:

1. HTTPS with valid TLS, redirects.
2. First render time; any form of early render or streaming.
3. `document.location`, `window.origin`, relative URLs, `SameSite` / `HttpOnly` cookies, session and auth.
4. CSP (header and meta), inline scripts with nonces, Service Worker registration, IndexedDB, localStorage.
5. History, back/forward, navigation delegate, deep links.
6. CORS, subresources, cross-origin iframes, video, downloads.
7. Bridge isolation, app-bound domains, content worlds.
8. Background, process termination (`webViewWebContentProcessDidTerminate`), memory pressure, offline.

<Badge type="tip" text="FACT" /> S1 and S2 (2026-11-08) give data for items 1 (valid TLS), 2, 3 (origin, relative URLs, `SameSite` and `HttpOnly` cookies and their sync) and 7 (bridge in a content world) on iOS 27.0, 26.5 and 18.5, iPhone 16 Pro simulator only (§3). Items 4 and 5 (CSP, Service Worker, history) are the next S2 task; the other items are not in WP-04 S2.

Output: `ios-parity-matrix.md` with **supported / conditional / impossible with public APIs** per item, prototype builds, screen recordings and traces. If full streaming parity is impossible, Rivqen does not promise it. It keeps cache, revalidation and data updates, which bring most of the warm-path benefit.

## 5. Swift API (sketch)

```swift
// Planned API — not released
let engine = try RivqenEngine(configuration: config)
let session = try await engine.open(.init(
    url: URL(string: "https://shop.example.com/catalog")!,
    partition: .user(id: currentUserID),
    policy: .authenticatedCacheDisabled
))
let webView = WKWebView(frame: .zero, configuration: session.webViewConfiguration)
session.attach(to: webView)
for await event in session.events { handle(event) }
```

| Rule | Detail |
|---|---|
| Concurrency | The engine is an `actor`; WebView work is `@MainActor`; Swift 6 strict concurrency |
| Networking | `URLSession` with the app's configuration; system TLS |
| Cookies | Sync with `WKHTTPCookieStore` of the session's `WKWebsiteDataStore` |
| Secrets | Keychain, `ThisDeviceOnly` |
| Request nonce (RQP-29, DL-063) | <Badge type="info" text="DESIGN" /> The 16 bytes of `Rq-Nonce` for each network attempt come from `SecRandomCopyBytes(kSecRandomDefault, 16, &bytes)`. The SDK uses them only when the result is `errSecSuccess`; else it sends no nonce: no `Rq-Nonce`, no `Rq-Have`, no delta media type ([negotiation §3.9](/engineering/protocol/negotiation#_3-9-rq-nonce)). A release build has no other source: a test input of the nonce exists only in a debug or test build. <Badge type="tip" text="FACT" /> Apple documentation: "Generates an array of cryptographically secure random bytes"; the return value is "`errSecSuccess` or some other value on failure"; "Always test the returned status" ([SecRandomCopyBytes](https://developer.apple.com/documentation/security/secrandomcopybytes(_:_:_:)), checked 2026-10-20) |
| Privacy | `PrivacyInfo.xcprivacy` for the APIs the SDK uses |
| Distribution | Swift Package + XCFramework (device and simulator slices) |

## Related

- [ADR-003](/engineering/architecture/adr/#adr-003)
- [Risk RSK-01, RSK-02](/engineering/plan/risks)
- [Getting started: iOS](/guide/getting-started/ios)
