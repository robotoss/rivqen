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
| `loadSimulatedRequest(_:response:responseData:)` (iOS 15+) | Load a request with app-provided response and data | <Badge type="warning" text="RESEARCH" /> Streaming, cookie and Service Worker behavior must be tested |
| `load(URLRequest)` | Real navigation | Use bytes that the app already fetched |
| SwiftUI `WebView` / `WebPage` (iOS 26+) | Declarative web content | No interception of `https` either |

Sources: [WKURLSchemeHandler](https://developer.apple.com/documentation/webkit/wkurlschemehandler), [loadHTMLString](https://developer.apple.com/documentation/webkit/wkwebview/loadhtmlstring(_:baseurl:)), [loadSimulatedRequest](https://developer.apple.com/documentation/webkit/wkwebview/loadsimulatedrequest(_:response:responsedata:)).

## 3. Prototypes (WP-04)

| Prototype | Mechanism | Benefits | Risks |
|---|---|---|---|
| **IOS-P1** | `URLSession` early fetch → `loadSimulatedRequest` (or `loadHTMLString` with base URL) | Early request; simple | Origin, cookies, CSP, history, Service Worker; no streaming |
| **IOS-P2** | Normal `load(URLRequest)` + cached page shown first + data updates through the bridge | True browser navigation and origin model | The prefetched bytes are not reused for the main document |
| **IOS-P3** | Custom scheme + `WKURLSchemeHandler` for trusted own content | Controlled streaming of local responses | Different origin; cookies and Service Workers differ |

**Forbidden shortcuts:** private WebKit API, global `NSURLProtocol` interception for `WKWebView`, disabling ATS or TLS validation, trusting all certificates, global cookie replacement, origin spoofing.

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
| Privacy | `PrivacyInfo.xcprivacy` for the APIs the SDK uses |
| Distribution | Swift Package + XCFramework (device and simulator slices) |

## Related

- [ADR-003](/engineering/architecture/adr/#adr-003)
- [Risk RSK-01, RSK-02](/engineering/plan/risks)
- [Getting started: iOS](/guide/getting-started/ios)
