# Support matrix

This page lists the planned platform support and what each feature does on each platform.

::: warning Planned
Minimum versions are **proposals** (ADR-015). They are confirmed at the start of WP-10 and WP-11 from store rules and market data. Feature support on iOS depends on the WP-04 spike.
:::

## 1. Platforms

| Platform | Proposed minimum | Tested on (target) | Notes |
|---|---|---|---|
| Android | API 26 (Android 8.0) — to confirm | API 26 … 37, several System WebView versions | Google Play requires `targetSdk` 36 for new apps and updates from 31 Aug 2026 |
| iOS / iPadOS | iOS 17 — to confirm | Latest two majors (iOS 26, iOS 27) + minimum | `UIWebView` is not supported |
| Web (in WebView) | Evergreen engines | Android System WebView, WKWebView | Bridge needs the native SDK |
| React | 18 | 18, 19 | |
| Next.js | 15 | 15, 16 | App Router and Pages Router tested separately |
| Node.js | 22 | 22, 24, 26 | Follows Node.js LTS lines |
| Java | 17 | 17, 21, 25 | Spring Boot 4.x starter |
| PHP | 8.4 | 8.4, 8.5 | PSR-15 |

## 2. Features by platform

| Feature | Android | iOS | Web |
|---|---|---|---|
| Parallel early HTML fetch | ✅ | ✅ | — (browser) |
| Streaming the main document to the view | ✅ | ⚠️ Research (ADR-003) | — |
| Show cached page at once | ✅ | ✅ | — |
| Revalidation (`304`) | ✅ | ✅ | ✅ (HTTP cache) |
| Data updates without reload | ✅ | ✅ | ✅ |
| Offline display (allowed pages) | ✅ | ✅ | — |
| Encrypted cache for authenticated pages | ✅ | ✅ | — |
| Subresource prefetch | ✅ | ⚠️ Research | — |
| Push invalidation (RQP, WebSocket) | 🧪 | 🧪 | 🧪 |
| WebTransport (RQP) | 🧪 runtime check | 🧪 runtime check | 🧪 |

Legend: ✅ planned · ⚠️ depends on research · 🧪 experimental, opt-in · — not applicable

## 3. Not supported

- `UIWebView` (removed from App Store acceptance by Apple).
- Android WebView versions without the features required for the secure bridge (the bridge is disabled; pages still work with revalidation).
- Interception of third-party sites.
