# Platform hardening

This page lists the security settings and practices for each platform SDK and for the server SDKs.

**Status:** <Badge type="info" text="DESIGN" />

[[toc]]

## Android

| Item | Requirement |
|---|---|
| TLS errors | `onReceivedSslError` always cancels. Never call `proceed()`. |
| File access | `setAllowFileAccess(false)`, `setAllowContentAccess(false)` unless the host app needs them for other content. Never `setAllowUniversalAccessFromFileURLs(true)`. |
| Mixed content | `MIXED_CONTENT_NEVER_ALLOW` by default. |
| JS bridge | Prefer `WebViewCompat.addWebMessageListener` with an explicit origin allowlist. Do not use `addJavascriptInterface` for untrusted origins. |
| Safe Browsing | Keep enabled (default). |
| Network security config | The SDK respects the app's `network_security_config`. The SDK adds no trust anchors. |
| Storage | Private app directories; excluded from backup for private entries. |
| Keys | Android Keystore; hardware-backed when available. |
| Shrinking | Consumer R8 rules are narrow; JNI entry points kept explicitly. |
| Exported components | The SDK declares no exported Android components. |

## iOS

| Item | Requirement |
|---|---|
| APIs | Public WebKit APIs only. No private selectors, no `WKBrowsingContextController`, no global `NSURLProtocol` interception for `WKWebView`. |
| ATS | The SDK requires no ATS exceptions. |
| Message handlers | Minimal `WKScriptMessageHandler` set; check `message.frameInfo.securityOrigin` and `isMainFrame`. |
| Content worlds | Use a dedicated `WKContentWorld` for SDK scripts. |
| Data store | Respect the app's `WKWebsiteDataStore` choice; never mix data stores of different profiles. |
| Keys | Keychain with `kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly` (or stricter). |
| Privacy manifest | Ship `PrivacyInfo.xcprivacy` declaring required-reason APIs actually used (for example file timestamps, if used). |

## Web / React

| Item | Requirement |
|---|---|
| Code execution | No `eval`, `new Function`, or string-to-method dispatch. |
| Insertion | Prefer state updates; DOM insertion only into SDK-owned regions; Trusted Types policy where available. |
| CSP | The SDK works under a strict CSP with nonces. It never requires `unsafe-inline` or `unsafe-eval`. |
| Messages | Validate every bridge message with a schema; check revisions before state changes. |

## Server SDKs (Java, Node.js, PHP)

| Item | Requirement |
|---|---|
| SSRF | The middleware never fetches a URL from request data. |
| Caching | Personalized responses get `Cache-Control: private` and correct `Vary`. |
| Errors | No stack traces in HTTP responses. |
| Headers | Validate and encode all header values (no CR/LF injection). |
| Proxies | Test request smuggling and header normalization behind Nginx, Envoy and the chosen CDN. |
| Conformance | Same test vectors as all other SDKs. |

## Related

- [Release profiles](/engineering/security/release-profiles)
- [JS bridge contract](/engineering/platforms/js-bridge)
