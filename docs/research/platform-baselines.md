# Platform baselines (2026)

Current versions of the platforms that Rivqen targets, checked on **9 October 2026**. Update this page at each phase gate.

**Status:** <Badge type="tip" text="FACT" /> unless a row says otherwise.

| Platform | Current state | Source |
|---|---|---|
| Android | Android 16 = API 36; Android 17 = API 37 (platform stability reached in 2026) | [Android 17](https://developer.android.com/blog/topics/android-17) |
| Google Play | New apps and updates must target API 36 from 31 Aug 2026 (Wear OS / Automotive: 35; TV / XR: 34); extension to 1 Nov 2026 on request | [Target API requirements](https://developer.android.com/google/play/requirements/target-sdk) |
| iOS | iOS 26 (Sept 2025); iOS 27 released 14 Sept 2026; iPhone 11 is the lower device bound for iOS 27 (secondary sources) | [MacRumors](https://www.macrumors.com/2026/06/05/ios-27-release-date-how-to-install-beta/), press reports |
| SwiftUI web | `WebView` and `WebPage` APIs in iOS 26 (WWDC25) | [Overview](https://sarunw.com/posts/swiftui-native-webview/) — confirm in Apple docs |
| Rust | Six-week stable releases; 1.97.1 published 16 July 2026; edition 2024 | [Rust blog](https://blog.rust-lang.org/releases) |
| UniFFI | 0.32.2, MPL-2.0 | crates.io registry |
| Node.js | 24 LTS; 26 Current → LTS on 28 Oct 2026; 22 maintenance until 30 Apr 2027; from Node 27, one major release per year | [Node.js releases](https://nodejs.org/en/about/previous-releases), [InfoQ](https://infoq.com/news/2026/06/nodejs-release-changes) |
| React | 19.3.0 | npm registry |
| Next.js | 16.4.0 | npm registry |
| TypeScript | 7.0.2 | npm registry |
| Express / Fastify | 5.2.1 / 5.12.5 | npm registry |
| Java | 25 LTS (Sept 2025), 21 LTS, 17 LTS | [OpenJDK](https://openjdk.org/projects/jdk/25/) |
| Spring Boot | 4.1 line; Java 17+; Servlet 6.1 (Tomcat 11, Jetty 12.1) | [System requirements](https://docs.spring.io/spring-boot/4.1/system-requirements.html) |
| PHP | 8.5 (20 Nov 2025): active to 31 Dec 2027; 8.4: active to 31 Dec 2026; 8.2: security to 31 Dec 2026; 8.1: EOL | [php.net supported versions](https://www.php.net/supported-versions) |
| WebTransport | W3C Candidate Recommendation Snapshot, July 2026 | [W3C](https://www.w3.org/TR/webtransport/) |
| VitePress | 1.6.4 latest stable; 2.0.0-alpha in `next` tag | npm registry |
| Mermaid | 12.1.0 latest; the VitePress plugin supports 10–11, so the site uses 11.17.2 | npm registry |
| GitHub Pages actions | `checkout@v7`, `setup-node@v7`, `configure-pages@v6`, `upload-pages-artifact@v5`, `deploy-pages@v5` | Tags in the action repositories |

::: info Rows from secondary sources
iOS 27 device support and some Node.js schedule dates come from secondary sources. Confirm them with Apple and the Node.js Release working group before you set the support matrix (ADR-015).
:::
