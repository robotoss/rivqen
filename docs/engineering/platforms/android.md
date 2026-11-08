# Android SDK

The Android SDK connects the Rust core to Android System WebView. It owns the WebView lifecycle, request interception, HTTP, cookies, keys and the JS bridge.

**Status:** <Badge type="info" text="DESIGN" /> Work package WP-10. Feasibility spike in WP-04.

[[toc]]

## 1. Upstream for comparison

<Badge type="tip" text="FACT" /> Upstream Android SDK (`upstream:sonic-android/sdk`, Java): `compileSdkVersion 25`, `minSdkVersion 9`, `targetSdkVersion 25`, dependency `com.android.support:support-annotations:25.2.0` (`sdk/build.gradle:7-12, 40`). It uses `HttpURLConnection`, `shouldInterceptRequest`, `loadDataWithBaseURL`, a SQLite metadata table and a JS interface added with `addJavascriptInterface` in the sample. See [legacy client behavior](/engineering/protocol/legacy-client).

Rivqen is a new implementation in Kotlin. It keeps the behavior, not the code.

## 2. Components

| Component | Responsibility |
|---|---|
| `Rivqen` / `RivqenEngine` | Create the engine; hold the core handle, config, storage driver |
| `RivqenSession` | One navigation; exposes `events: Flow<SessionEvent>` |
| `RivqenWebViewController` | Attach to a `WebView`; install `WebViewClient` hooks; deliver documents |
| `RivqenRequestInterceptor` | `shouldInterceptRequest` for the **main document** only |
| `RivqenBridge` | `WebViewCompat.addWebMessageListener` with origin allowlist |
| `RivqenHttp` | HTTP adapter interface; default implementation chosen by ADR-014 |
| `RivqenCookieSync` | Read and write `CookieManager` around native requests |
| `RivqenStorage` | Files + SQLite in `noBackupFilesDir`; keys in Android Keystore |
| `RivqenMetrics` | Metric sink and optional OpenTelemetry adapter |

## 3. Integration flow

1. The app calls `open(request)` **before** it creates or shows the WebView.
2. The SDK creates a core session and runs the cache lookup and the network request in parallel.
3. The app attaches the WebView. The controller calls `loadUrl(url)` (Standard presentation) or loads the ready document (Quick presentation, only if security headers are preserved).
4. `shouldInterceptRequest` checks: main frame, `GET`, URL equals the session URL, origin allowed. If all are true, it returns a `WebResourceResponse` with status, headers (including CSP) and the bridge `InputStream`. <Badge type="info" text="DESIGN" /> For an RQP full document or snapshot (also a patched or an offline snapshot), the `mimeType` is `text/html` and the `encoding` is `utf-8`, whatever the response said (H-66; [negotiation §6.3](/engineering/protocol/negotiation#_6-3-response), "Delivery to the WebView"). The `InputStream` gives the stored bytes unchanged (DL-040, L-2). The same `mimeType` and `encoding` hold for `loadDataWithBaseURL`, which takes a `String`: the adapter removes a leading `EF BB BF` from the bytes before it decodes them, because the Java UTF-8 decoder keeps the byte order mark as U+FEFF (<Badge type="tip" text="FACT" /> JDK 25.0.3, checked on 2026-10-18; [JDK-4508058](https://bugs.openjdk.org/browse/JDK-4508058)).
5. The core receives HTTP events and returns actions: show cache, stream, patch, full load or fallback.
6. `onPageCommitVisible` and `onPageFinished` events go to the core; races follow the [FSM rules](/engineering/core/session-fsm#_4-races).
7. On `onDestroy`, navigation away, or partition change, the session is cancelled and all streams close.

<Badge type="info" text="DESIGN" /> **Origin of the final request (DL-040, L-3).** The SDK takes the origin for the allowlist and the cache key from the URL object that the network stack used: for OkHttp `Response.request.url` (an `HttpUrl`). <Badge type="tip" text="FACT" /> The OkHttp Javadoc of `Response.request()` says that it "may be the request generated in response to an HTTP redirect or authentication challenge", with another URL than the initial request; `priorResponse()` returns the response of the redirect or challenge that triggered this response, or null ([OkHttp 3.7.0 `Response`](https://javadoc.io/static/com.squareup.okhttp3/okhttp/3.7.0/okhttp3/Response.html)). <Badge type="info" text="DESIGN" /> So a response with any non-null `priorResponse()` (a redirect or an authentication challenge) never gives a result in the `public` partition (H-67, DL-042): a challenge can also set a cookie, or an OkHttp `Authenticator` can add `Authorization` that the SDK did not add. It does not parse a URL string again with `android.net.Uri` or `java.net.URI`: their rules differ from the WHATWG URL parser (for example for `\` and IDN hosts). Whether the `HttpUrl` host of every URL is equal to the WHATWG host: NOT FOUND; the protocol fixtures `FX-RQ-C25-FINAL-BACKSLASH-*` and `FX-RQ-C25-IDN-HOST` check it in the SDK tests. A URL with a user name or a password gets no RQP (H-68).

## 4. Requirements

| ID | Requirement |
|---|---|
| A-01 | Kotlin-first API; Java callers supported with `@JvmStatic`/`@JvmOverloads` where useful |
| A-02 | Modern WebView APIs through AndroidX WebKit with `WebViewFeature.isFeatureSupported` checks; no deprecated-only design |
| A-03 | `onReceivedSslError` always cancels |
| A-04 | Interception checks `isForMainFrame`, URL, method, redirect and origin |
| A-05 | Cookie sync, auth headers, `Vary` and partition are correct |
| A-06 | No `addJavascriptInterface` for untrusted content; `addWebMessageListener` with origin rules |
| A-07 | UI work on the main thread; heavy work off the main thread |
| A-08 | Survives process recreation, renderer crash (`onRenderProcessGone`), Activity/Fragment recreation |
| A-09 | `minSdk` and `targetSdk` set at WP-10 start from current policy (see [Support matrix](/guide/support-matrix)) |
| A-10 | Built-in fallback and kill switch per URL, origin and session |

## 5. Platform facts and research items

| Topic | Fact / research | Status |
|---|---|---|
| `shouldInterceptRequest` | Can return `WebResourceResponse` with an `InputStream`, status and headers | <Badge type="tip" text="FACT" /> [Android reference](https://developer.android.com/reference/android/webkit/WebResourceResponse) |
| Redirects in interception | A `WebResourceResponse` cannot express a redirect: its status must be in 100–299 or 400–599, "Causing a redirect by specifying a 3xx code is not supported". `shouldInterceptRequest` is called only for the initial URL of a redirect chain, not for the redirect URLs. <Badge type="info" text="DESIGN" /> The WP-04 prototype does not follow redirects in the early fetch: a 3xx head, or a response with a prior response, passes the request to the WebView. The behavior of the WebView in a redirect chain is measured in WP-04 S2 | <Badge type="tip" text="FACT" /> [WebResourceResponse](https://developer.android.com/reference/android/webkit/WebResourceResponse), [WebViewClient](https://developer.android.com/reference/android/webkit/WebViewClient) (checked 2026-11-08). <Badge type="warning" text="RESEARCH" /> WP-04 S2 |
| Streaming a main-frame response (CAP-A-01) | A `WebResourceResponse` with a pipe `InputStream` that the native fetch fills while the WebView reads it renders the page progressively. The spike host sends chunk k+1 only after the page requested the image of chunk k: 3 runs with 8 chunks of 2048 bytes had no gate timeout, every chunk script ran, and the WebView read the fetched bytes unchanged (count and SHA-256 equal to the host body). With 1.5 s between chunks the content of chunk 0 was ready to draw (`postVisualStateCallback`) 2.7–2.8 s before the end of the body (3 runs). Verdict on API 37: **conditional** (read blocks, next row) | <Badge type="tip" text="FACT" /> API 37, WebView 145.0.7632.218, 2026-11-08: [runs](https://github.com/robotoss/rivqen/tree/main/evidence/spikes/wp-04/android/CAP-A-01/37), [paint runs](https://github.com/robotoss/rivqen/tree/main/evidence/spikes/wp-04/android/CAP-A-01/37/paint-delay). API 29: not tested yet (S2) |
| Read blocks of an intercepted stream | The WebView fills each read request before it passes the bytes on: `InputStream::Read` loops until the requested length or the end of the stream. Measured: the first read asks for the bytes that `available()` reports, each later read for 2048 bytes. A flush smaller than the block waits for more bytes or the end of the body (1500-byte chunks: the first read took chunk 0; chunk 1 then waited 4 s for the 548 bytes that fill its 2048-byte read, 3 runs). <Badge type="info" text="DESIGN" /> The bridge stream reports its buffered bytes in `available()`; it cannot flush a partial block (a read that returns fewer bytes only makes the WebView read again) | <Badge type="tip" text="FACT" /> [Chromium `input_stream.cc`](https://chromium.googlesource.com/chromium/src/+/6036187a1d76f93d870769db672a39bfece18b4d/android_webview/browser/input_stream.cc) (source revision, not the 145 branch); [block runs](https://github.com/robotoss/rivqen/tree/main/evidence/spikes/wp-04/android/CAP-A-01/37/block-1500) (API 37, WebView 145.0.7632.218). The source of the 2048-byte request size: NOT FOUND |
| Early fetch reused as the main-frame response (CAP-A-02) | The OkHttp fetch starts before the WebView exists; `shouldInterceptRequest` waits for its head and returns it with the bridge stream. In 3 runs `app.fetch.start` was the first app event, the WebView was created 110–119 ms later, and the host saw exactly one request for the page (no second request by the WebView). With a server that waits 1 s before the head, the interceptor waited 0.93–0.94 s for it. The first WebView after a cold emulator boot took 7.4 s to create (one run, not stored), so an early fetch can hide a large part of the WebView start. Verdict on API 37: **supported** | <Badge type="tip" text="FACT" /> API 37, WebView 145.0.7632.218, 2026-11-08: [runs](https://github.com/robotoss/rivqen/tree/main/evidence/spikes/wp-04/android/CAP-A-02/37) |
| Status and fields of an intercepted response (CAP-A-03) | `Content-Security-Policy` from the response fields is enforced: with `script-src 'nonce-…'` every nonce script ran and an inline script without the nonce did not run (3 runs). The status reaches the page: `PerformanceNavigationTiming.responseStatus` was 200, and 400 for an intercepted 400 page (3 runs each). The reason phrase has no page API, so the page cannot see it (NOT FOUND); the constructor needs a non-empty one. Verdict on API 37: **supported** (status and fields) | <Badge type="tip" text="FACT" /> API 37, WebView 145.0.7632.218, 2026-11-08: [runs](https://github.com/robotoss/rivqen/tree/main/evidence/spikes/wp-04/android/CAP-A-03/37), [400 runs](https://github.com/robotoss/rivqen/tree/main/evidence/spikes/wp-04/android/CAP-A-03/37/status-400) |
| `loadDataWithBaseURL` | Loads HTML without response headers (no CSP header) | <Badge type="tip" text="FACT" /> (upstream warns the same) |
| Target API | Google Play requires new apps and updates to target API 36 from 31 Aug 2026 | <Badge type="tip" text="FACT" /> [Play target API](https://developer.android.com/google/play/requirements/target-sdk) |
| Android 17 (API 37) | Platform stable in 2026; check WebView API diffs (for example `startSafeBrowsing` deprecated) | <Badge type="tip" text="FACT" /> [API 37 diff](https://developer.android.com/sdk/api_diff/37/changes/android.webkit.WebView) |
| HTTP stack | Platform `HttpEngine` (API 34+) / Cronet / OkHttp | <Badge type="warning" text="RESEARCH" /> ADR-014 |
| Prefetch APIs | Compare WebView prefetch/prerender APIs in AndroidX WebKit, `WebViewAssetLoader`, preconnect, and the normal WebView cache | <Badge type="warning" text="RESEARCH" /> WP-04 |
| Request nonce (RQP-29, DL-063) | <Badge type="info" text="DESIGN" /> The SDK takes the 16 bytes of `Rq-Nonce` for each network attempt from `java.security.SecureRandom`: `SecureRandom()` (no algorithm and no provider name) and `nextBytes` on a 16-byte array. A release build has no other source: a test input of the nonce exists only in a debug or test build. When the generator fails (an exception), the SDK sends no nonce: no `Rq-Nonce`, no `Rq-Have`, no delta media type ([negotiation §3.9](/engineering/protocol/negotiation#_3-9-rq-nonce)) | <Badge type="tip" text="FACT" /> Java SE 21 API: "This class provides a cryptographically strong random number generator (RNG)" ([SecureRandom](https://docs.oracle.com/en/java/javase/21/docs/api/java.base/java/security/SecureRandom.html)); Android uses `SecureRandom().nextBytes(iv)` without a provider and says that Android guarantees no provider for an algorithm outside the Android Keystore system; the `Crypto` provider is removed since API 28 ([Android cryptography](https://developer.android.com/privacy-and-security/cryptography)); checked 2026-10-20. The Android reference page of `SecureRandom`: NOT FOUND (the page did not load) |

::: warning `WebViewAssetLoader` is not HTTP navigation
`WebViewAssetLoader` serves local files on an app-defined origin. It is not a replacement for real HTTPS navigation of your site.
:::

## 6. Packaging

| Item | Plan |
|---|---|
| Artifact | AAR on Maven Central with `arm64-v8a`, `armeabi-v7a`, `x86_64` native libraries |
| Native build | `cargo-ndk` in CI; 16 KB page size alignment for native libraries |
| Shrinking | Consumer R8 rules: keep only JNI entry points and generated binding classes |
| Size budget | Measured per ABI; reported in each release |

## 7. Demo app (WP-22)

Five screens: **Cold**, **Warm**, **Live diff**, **Template changed**, **Offline**. Controls: disable Rivqen, slow network, clear cache, logout. The demo shows real metrics, transport and revision — never tokens.

## Related

- [JS bridge contract](/engineering/platforms/js-bridge)
- [HTTP stacks](/engineering/transport/http)
- [Getting started: Android](/guide/getting-started/android)
