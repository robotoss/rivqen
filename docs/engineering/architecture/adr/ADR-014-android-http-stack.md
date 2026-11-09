# ADR-014: Android HTTP stack

- Status: **Proposed — decided in WP-10** (H-127)
- Date: 2026-11-08
- Owner: WP-10; WP-04 adds the data
- Reviewers: architecture lead, security reviewer, Android SDK owner (WP-10)

This record proposes which HTTP stack the Android SDK uses for the native fetch of the main document. It collects what WP-04 measured and what it did not, for WP-10, which decides it.

**Status:** <Badge type="info" text="DESIGN" /> for the options and the recommendation. <Badge type="tip" text="FACT" /> for the measured data and the cited documentation.

[[toc]]

## Context

<Badge type="info" text="DESIGN" /> The adapters own the I/O (ADR-001). The Android SDK has an HTTP adapter interface (`RivqenHttp`, [Android SDK](/engineering/platforms/android) §2). The candidates are in [HTTP stacks](/engineering/transport/http) §1: the platform `HttpEngine` (API 34+), embedded Cronet, OkHttp, `HttpURLConnection`.

<Badge type="info" text="DESIGN" /> Whatever the stack, the SDK depends on these properties for the main document: the redirect is visible (H-67, DL-042, CL-44), a TLS error fails closed (A-03), the cookies come from `CookieManager` and go back to it (H-131), an error in the body is an error and not an end, and the URL of the binding is the WHATWG serialization (DL-040, Q-24).

### What WP-04 measured (OkHttp)

<Badge type="tip" text="FACT" /> The Android prototype used OkHttp 5.5.1 (Apache-2.0, DL-137) with androidx.webkit 1.18.0 on API 37 (WebView 145.0.7632.218) and API 29 (WebView 91.0.4472.114). Each result has 3 stable runs on each API ([Android SDK](/engineering/platforms/android) §5; runs in [`evidence/spikes/wp-04/android/`](https://github.com/robotoss/rivqen/tree/main/evidence/spikes/wp-04/android)).

| Property | Result with OkHttp | Evidence |
|---|---|---|
| Early fetch before the WebView exists | Works; one request for the page | CAP-A-02 |
| Redirect | Not followed; the request goes to the WebView, which requests the first URL again | CAP-A-04 |
| TLS error | `SSLHandshakeException`; nothing intercepted | CAP-A-05 |
| Cookies to the native fetch | `CookieManager.getCookie(url)` gives every cookie, also `HttpOnly` | CAP-A-07 |
| `SameSite` | Not applied: `Strict` and `Lax` go in every case | CAP-A-07 |
| Cookies back to the WebView | `CookieManager.setCookie` before the response; `flush()` | CAP-A-06, CAP-A-07 |
| Body cut by the server | `ProtocolException` after the delivered bytes | CAP-A-08 |
| URL serialization | `HttpUrl` equals WHATWG for 57 of 67 inputs | CAP-U-01 |

<Badge type="tip" text="FACT" /> Details:

1. **Early fetch (CAP-A-02).** The fetch started before the WebView; the WebView was created 72 to 156 ms later, and the host saw one request for the page. With a server that waited 1 s before the head, the interceptor waited 0.90 to 0.98 s for it.
2. **Redirect (CAP-A-04).** A `WebResourceResponse` cannot carry a 3xx (its constructor throws), and `shouldInterceptRequest` is not called for the redirect targets. So a redirected document is never streamed, and it costs one extra request. The OkHttp Javadoc says that `Response.request()` can be the request of a redirect and that `priorResponse()` is not null then ([Android SDK](/engineering/platforms/android) §3).
3. **Cookies (CAP-A-07).** `getCookie(url)` has no request context, so the SDK must apply `SameSite` itself; the human decided the rule (H-131, H-132). A cookie set after the last `flush()` was lost when the process died.
4. **URL (CAP-U-01).** `HttpUrl` equals WHATWG on every query class of Q-24. It differs on `|` in the path, a space in the fragment, numeric IPv4 forms, the host `.` and port 0. `java.net.URI` equals WHATWG for 19 of 67 inputs and `android.net.Uri` for 22 ([URL interop](/research/url-interop), [`compare.json`](https://github.com/robotoss/rivqen/blob/main/evidence/spikes/wp-04/url/compare.json)).

### What WP-04 did not measure

| Stack or item | State |
|---|---|
| `HttpEngine` (API 34+) | NOT FOUND: no run |
| Embedded Cronet | NOT FOUND: no run |
| `HttpURLConnection` (and `java.net.URL`) | NOT FOUND: no run |
| The app's own `OkHttpClient` (its interceptors, authenticator) | NOT FOUND |
| HTTP/2, HTTP/3, connection reuse with the WebView | NOT FOUND |
| Size of OkHttp and Okio in the SDK artifact | NOT FOUND |
| Devices | NOT FOUND (H-129) |

<Badge type="tip" text="FACT" /> What the documentation says about `HttpEngine` (checked 2026-11-08): it was added in API level 34 and is "Also in S Extensions 7" ([HttpEngine](https://developer.android.com/reference/android/net/http/HttpEngine)). A redirect "will not be followed until the URLRequest's `followRedirect()` method is called" ([UrlRequest.Callback](https://developer.android.com/reference/android/net/http/UrlRequest.Callback)). Its `HttpURLConnection` implementation does not use the cookie storage of `CookieHandler.setDefault` ([HttpEngine](https://developer.android.com/reference/android/net/http/HttpEngine)). These are documentation facts, not WP-04 results.

## Options

### Option 1: OkHttp as the default stack

- Pros: the only stack with WP-04 data for every property above; the URL type closest to WHATWG; Apache-2.0.
- Cons: a library in the SDK artifact (size: NOT FOUND); a second stack when the app uses another one.

### Option 2: `HttpEngine` on API 34+, another stack below

- Pros: part of the platform (no library); manual redirects by its API.
- Cons: two stacks by API level, so two sets of tests; no WP-04 data; its URL serialization and its cookie handoff: NOT FOUND.

### Option 3: Embedded Cronet

- Pros: the Chromium network stack with HTTP/3 ([HTTP stacks](/engineering/transport/http) §1).
- Cons: a large library or a Play services dependency (size: NOT FOUND); no WP-04 data.

### Option 4: `HttpURLConnection`

- Pros: no library.
- Cons: no WP-04 data; the `java.net` URL types differ much from WHATWG (`java.net.URI`: 19 of 67 equal).

### Option 5: Adapter interface, the app's stack as default

The register recommendation: the SDK defines the adapter, and the app plugs in the stack that it already uses for cookies and authentication.

- Pros: one stack in the app; the app's authentication stays in one place.
- Cons: every adapter must keep the properties of the [Context](#context); an app stack with its own interceptors, cookie jar or authenticator can break them (DL-042 already counts any prior response as a redirect).

## Recommendation

<Badge type="info" text="DESIGN" /> **Option 5 with OkHttp as the reference adapter** (option 1 inside option 5), and measure `HttpEngine` before WP-10 fixes the default:

1. The SDK defines the adapter contract from the WP-04 data: no automatic redirect for the main document (or a visible redirect that gives no streamed result, CAP-A-04); cookies only from `CookieManager`, by H-131 and H-132; each `Set-Cookie` written to `CookieManager` before the response goes to the WebView, then `flush()`; system trust only; an error in the body is an error; the binding URL is never taken from `java.net.URI` or `android.net.Uri`.
2. The SDK ships one adapter for OkHttp, the measured stack.
3. WP-10 runs CAP-A-02, CAP-A-04, CAP-A-05, CAP-A-07, CAP-A-08 and CAP-U-01 with `HttpEngine` on API 34+. If it keeps the contract, WP-10 may make it the default on API 34+ (no library on those devices).
4. An app adapter passes the same contract tests before the SDK uses it.

Why: OkHttp is the only stack with measured results for every property that the SDK depends on. The contract keeps the choice open for `HttpEngine` without new risk.

Cost of the recommendation: OkHttp in the SDK artifact; a contract test suite for adapters.

## Decision

Not decided. **Decided in WP-10** (H-127). A new runtime library in the SDK artifact follows DL-007.

## Consequences

<Badge type="info" text="DESIGN" />

- **Redirects.** [HTTP stacks](/engineering/transport/http) §3 says "Follow redirects in the native stack only for the same origin, by default". On Android, `shouldInterceptRequest` answers only for the first URL (CAP-A-04), so a body that came through a redirect would be given for the first URL. WP-04 did not run that case (NOT FOUND). Until WP-10 measures it, the measured rule applies: the native fetch does not follow a redirect of the main document; the WebView does.
- **Cookies.** The cookie rule does not depend on the stack: `CookieManager` is the store of record ([HTTP stacks](/engineering/transport/http) §2, Q-12).
- **Incompatibilities to document publicly (G2).** A redirected main document is never streamed and costs one extra request (CAP-A-04). WebView 91 drops a `Set-Cookie` of an intercepted response unless the SDK writes it (CAP-A-06). Both do not depend on the stack. They are conditions of CAP-A-04 and CAP-A-06 in the catalog, from which the Android matrix is generated.

## Verification

<Badge type="danger" text="GATE" /> WP-10 proves the decision:

1. The adapter contract tests pass with OkHttp, and with `HttpEngine` if WP-10 uses it, on the device matrix.
2. The CAP-A runs listed in the recommendation pass with the default stack on each API level that the SDK supports.
3. The binding URL tests of Q-24 (`FX-RQ-C25-*`) pass with the default stack.

## Related

- [HTTP stacks](/engineering/transport/http), [Android SDK](/engineering/platforms/android), [URL interop](/research/url-interop)
- [ADR-009](/engineering/architecture/adr/ADR-009-streaming-abi), [ADR register](/engineering/architecture/adr/)
- [WP-04 S2](/engineering/plan/sprints/WP-04-S2) (H-131, H-132)
