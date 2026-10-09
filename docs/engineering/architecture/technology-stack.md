# Technology stack

This page lists the technologies Rivqen uses per layer, why, and the fallback if a choice fails. Versions are the state on **9 October 2026**. Exact versions are pinned in lockfiles, not in this document.

**Status:** <Badge type="info" text="DESIGN" /> choices; <Badge type="tip" text="FACT" /> version data (sources in [Platform baselines](/research/platform-baselines)).

[[toc]]

## 1. Stack by layer

| Layer | Choice | Why | Fallback / condition |
|---|---|---|---|
| Core | **Rust stable**, edition 2024, `#![forbid(unsafe_code)]` except FFI | One verified implementation; memory safety; strong test tools | `unsafe` only with ADR and review |
| Bindings | **UniFFI** (control plane) | Typed Kotlin and Swift bindings from one definition | Narrow C ABI for document bytes if measured (ADR-002) |
| Android | Kotlin 2.x, AndroidX WebKit, System WebView, coroutines | Modern lifecycle, public APIs, feature detection | Normal WebView load |
| Android HTTP | Adapter: app stack / `HttpEngine` / Cronet / OkHttp | Cookie and auth consistency first | ADR-014 by benchmark |
| iOS | Swift 6 (strict concurrency), `WKWebView`, `URLSession` | Public APIs; actor isolation | Disable features that cannot be safe |
| Web | TypeScript, framework-free core, React adapter | Works with and without React | Vanilla JS API |
| Node.js server | TypeScript on the Fetch API + Express/Fastify/Next adapters | Runs on all modern runtimes | — |
| Java server | Java 17+ library, Jakarta Servlet 6 filter, Spring Boot 4 starter | Wide JVM reach | WebFlux later |
| PHP server | PSR-7/15/17 middleware via Composer | Framework-neutral | Output-buffer adapter for legacy apps |
| HTTP | HTTP/1.1, HTTP/2, HTTP/3 through platform stacks | Correct navigation and caching | HTTP/1.1 always works |
| Realtime | WebSocket (RQP), WebTransport (experimental) | Push invalidation | HTTP revalidation |
| Storage | Files + SQLite (WAL) metadata, AEAD for private entries | Atomic commits, quotas, per-partition removal | — |
| Docs | VitePress 1.6.4 + Mermaid 11 | Static, fast, Markdown-first | — |

## 2. Version baselines (Oct 2026)

| Technology | Current state | Rivqen use |
|---|---|---|
| Rust | Stable releases every 6 weeks; 1.97.x released July 2026; edition 2024 | Pin in `rust-toolchain.toml` at WP-05; MSRV policy: latest stable − 2 |
| UniFFI | 0.32.x (pre-1.0); license **MPL-2.0** | See [Dependencies](/engineering/core/dependencies) for the license note |
| Android | Android 16 = API 36 (Play target requirement from 31 Aug 2026); Android 17 = API 37 | `targetSdk` 36+; `compileSdk` 37 |
| iOS | iOS 26 (2025), iOS 27 released 14 Sep 2026 | Minimum set in ADR-015 |
| React | 19.3 | Support React 18 and 19 |
| Next.js | 16.4 | App Router + Pages Router adapters |
| TypeScript | 7.0 | Build with current TS; emit ES2022 |
| Node.js | 24 (LTS), 26 (enters LTS 28 Oct 2026), 22 (maintenance until Apr 2027) | Support 22, 24, 26 at launch |
| Java | 25 LTS (Sept 2025), 21 LTS, 17 LTS | Bytecode 17; test 17/21/25 |
| Spring Boot | 4.1 line; requires Java 17+ and Servlet 6.1 | Starter for 4.x |
| PHP | 8.5 (Nov 2025), 8.4; 8.2 security support ends 31 Dec 2026 | Support 8.4+ at launch |

## 3. Rejected or postponed choices

| Option | Decision | Reason |
|---|---|---|
| Rust HTTP stack in the core | Postponed | Second cookie jar; auth drift with the WebView |
| Rust WebView or custom browser engine | Rejected | Huge scope; platform policies; security |
| PHP extension / JNI for servers | Postponed | Hard to install; spec + conformance is enough |
| Global allocator replacement in the SDK | Rejected | Side effects in host apps |
| QUIC datagrams for patches | Rejected | Unreliable delivery for mandatory data |
| Private WebKit APIs on iOS | Rejected | Store policy and stability |

## Related

- [Dependencies](/engineering/core/dependencies)
- [Support matrix](/guide/support-matrix)
- [Platform baselines](/research/platform-baselines)
