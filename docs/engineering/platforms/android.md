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
4. `shouldInterceptRequest` checks: main frame, `GET`, URL equals the session URL, origin allowed. If all are true, it returns a `WebResourceResponse` with status, headers (including CSP) and the bridge `InputStream`.
5. The core receives HTTP events and returns actions: show cache, stream, patch, full load or fallback.
6. `onPageCommitVisible` and `onPageFinished` events go to the core; races follow the [FSM rules](/engineering/core/session-fsm#_4-races).
7. On `onDestroy`, navigation away, or partition change, the session is cancelled and all streams close.

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
| Redirects in interception | A returned response cannot express a redirect for the main frame in all WebView versions | <Badge type="warning" text="RESEARCH" /> WP-04 |
| `loadDataWithBaseURL` | Loads HTML without response headers (no CSP header) | <Badge type="tip" text="FACT" /> (upstream warns the same) |
| Target API | Google Play requires new apps and updates to target API 36 from 31 Aug 2026 | <Badge type="tip" text="FACT" /> [Play target API](https://developer.android.com/google/play/requirements/target-sdk) |
| Android 17 (API 37) | Platform stable in 2026; check WebView API diffs (for example `startSafeBrowsing` deprecated) | <Badge type="tip" text="FACT" /> [API 37 diff](https://developer.android.com/sdk/api_diff/37/changes/android.webkit.WebView) |
| HTTP stack | Platform `HttpEngine` (API 34+) / Cronet / OkHttp | <Badge type="warning" text="RESEARCH" /> ADR-014 |
| Prefetch APIs | Compare WebView prefetch/prerender APIs in AndroidX WebKit, `WebViewAssetLoader`, preconnect, and the normal WebView cache | <Badge type="warning" text="RESEARCH" /> WP-04 |

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
