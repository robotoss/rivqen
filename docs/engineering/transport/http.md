# HTTP stacks

Rivqen uses the platform HTTP stack for the main document. This keeps cookies, authentication and TLS consistent with the app and, as much as possible, with the WebView.

**Status:** <Badge type="info" text="DESIGN" /> <Badge type="warning" text="RESEARCH" /> ADR-014 decides the Android default.

[[toc]]

## 1. Per-platform choice

| Platform | Primary candidate | Alternative | Must be proven by experiment |
|---|---|---|---|
| Android | Adapter interface; default to the app's stack | `HttpEngine` (platform Cronet, Android 14+ / API 34) or embedded Cronet; OkHttp; `HttpURLConnection` | Cookie sync with `CookieManager`, redirect handling, cost of a second stack |
| iOS | `URLSession` | Network framework for special realtime/QUIC needs | Cookie, credential and redirect consistency with `WKWebView` |
| Web | Browser `fetch` and navigation | WSS / WebTransport after capability checks | Browser connection policy |

<Badge type="tip" text="FACT" /> Cronet is the Chromium network stack for Android with HTTP/1.1, HTTP/2, HTTP/3/QUIC, request priorities and a cache ([Android Cronet guide](https://developer.android.com/develop/connectivity/cronet)).

<Badge type="tip" text="FACT" /> Apple recommends `URLSession` for HTTPS, including HTTP/2 and HTTP/3; Network framework is for custom protocols ([TN3151](https://developer.apple.com/documentation/technotes/tn3151-choosing-the-right-networking-api)).

::: warning The app's HTTP stack is not the WebView's HTTP stack
A request made by `URLSession` or OkHttp does not share the WebView's HTTP cache, connection pool or TLS session. Cookies are shared only if the SDK synchronizes them. Every design must state who owns cookies for each request.
:::

## 2. Cookie ownership

| Platform | WebView cookie store | Native request cookie source | Sync rule |
|---|---|---|---|
| Android | `android.webkit.CookieManager` | Read from `CookieManager` for the request URL | After the response, write `Set-Cookie` back to `CookieManager` and call `flush()` |
| iOS | `WKHTTPCookieStore` (per `WKWebsiteDataStore`) | Copy cookies for the URL from `WKHTTPCookieStore` into the `URLRequest` | After the response, write `Set-Cookie` into `WKHTTPCookieStore` before the WebView loads |

`HttpOnly`, `Secure` and `SameSite` attributes must be kept exactly. A test suite (WP-04) verifies each attribute on each platform.

## 3. Redirects

1. Follow redirects in the native stack only for the same origin, by default.
2. For a cross-origin redirect, stop and let the WebView do a normal navigation to the final URL.
3. Never forward `Authorization` or cookies to an unexpected origin.
4. Record the final URL. The cache key uses the URL the user requested **and** the final URL is stored in metadata.

## 4. Compression

| Encoding | Handling |
|---|---|
| `gzip`, `br` | Decoded by the platform stack in most cases. The core sees decoded bytes. |
| `zstd` | Platform support varies. Request it only if the stack decodes it. |
| Dictionary compression (`dcb`, `dcz`) | Experimental in RQP (ADR-013). |

The decoded size is checked against `maxDocumentBytes` and `maxCompressedRatio`.

## 5. Timeouts and retries

| Setting | Initial value | Retry |
|---|---|---|
| Connect | 5 s | Once, only for idempotent `GET` |
| First byte | 10 s | No; fallback |
| Total | 30 s | No; fallback |

## Related

- [Transport overview](/engineering/transport/)
- [Android SDK](/engineering/platforms/android), [iOS SDK](/engineering/platforms/ios)
- [Platform baselines](/research/platform-baselines)
