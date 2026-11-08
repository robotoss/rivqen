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

<Badge type="tip" text="FACT" /> WP-04 S2 (Android API 37 and 29, iOS 27.0, 26.5 and 18.5): the native path does not apply `SameSite`. `CookieManager.getCookie(url)` with OkHttp, and `URLSession` with `HTTPCookieStorage`, send `SameSite=Strict` and `Lax` cookies whatever started the navigation. The WebView and WebKit send `Strict` on a load that the app starts and on a navigation from a page of the same site, but not on a navigation that a page of another site starts ([Android §5](/engineering/platforms/android#_5-platform-facts-and-research-items), [iOS §3](/engineering/platforms/ios#_3-prototypes-wp-04)).

<Badge type="info" text="DESIGN" /> **Native early fetch and SameSite (H-131).** The SDK makes a native early fetch with the cookies of the store only in two cases. (1) The app starts the load (`open`, `loadUrl`, `load(URLRequest)`), and the web view is blank or shows a page of the same origin as the target. (2) A page starts the navigation, and the SDK has proof that the page and the target are the same site: the origin of the `Referer` field on Android, the origin of `WKNavigationAction.sourceFrame` on iOS. Without that proof (no `Referer`, an unknown source frame, another site) there is no early fetch: the web view loads normally. "Same site" means (H-132): the same origin, or origins that the app declares as one site in its configuration (for example `https://shop.example` and `https://www.shop.example`). The app can change that list without a new release (remote configuration). The SDK uses no Public Suffix List: a list inside the SDK gets old, and an old list can call two sites one site where the current WebView calls them two, which sends `Strict` cookies that the browser holds back. A domain that is not in the list yet only loses the early fetch.

## 3. Redirects

1. The native early fetch of a main document does not follow a redirect (DL-141). A 3xx stops it, and the WebView loads the URL itself and follows the redirect. <Badge type="tip" text="FACT" /> On Android `shouldInterceptRequest` answers only for the first URL, so a body that the native stack fetched after a redirect would appear under the first URL; a `WebResourceResponse` cannot carry a 3xx (CAP-A-04). On iOS a redirect gives a real navigation ([ADR-003](/engineering/architecture/adr/ADR-003-ios-navigation)).
2. Subresource and API requests of the SDK may follow a same-origin redirect; for a cross-origin redirect they stop.
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
