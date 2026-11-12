# Architecture decision records

An **ADR** records one important decision: the context, the options, the decision and its consequences. ADRs make decisions traceable. A decision in an ADR can change only through a new ADR that supersedes it.

**Status:** <Badge type="info" text="DESIGN" /> ADR-005 and ADR-006 are **Accepted** (human, 2026-10-09); ADR-002 and ADR-003 are **Accepted** (human, 2026-11-09, H-135, H-136); ADR-001 and ADR-017 are **Accepted** (human, 2026-11-11, H-139, H-140). All other ADRs below are **Proposed**. Each has an owner work package and the evidence that will close it. ADR-002 and ADR-003 go to the human at G2 (H-127); ADR-002, ADR-003, ADR-009 and ADR-014 have their own files (WP-04 S3); ADR-001 and ADR-017 have their own files (WP-05 S2).

[[toc]]

## ADR process

1. Copy the template below into `docs/engineering/architecture/adr/ADR-NNN-short-name.md`.
2. Fill in context, options and evidence.
3. Open a pull request with the label `adr`.
4. Get approval from the architecture lead and the reviewers listed in the ADR.
5. Set the status to **Accepted**. Link the ADR from the pages it affects.

| Status | Meaning |
|---|---|
| Proposed | Under discussion. Work may continue with the recommended option. |
| Accepted | The decision is in force. |
| Superseded | A newer ADR replaces it. The link is mandatory. |
| Rejected | The decision was considered and declined. Keep it for history. |

## ADR template

```md
# ADR-NNN: <title>

- Status: Proposed | Accepted | Superseded by ADR-XXX | Rejected
- Date: YYYY-MM-DD
- Owner: <role>, WP-XX
- Reviewers: <roles>

## Context
<problem, constraints, evidence links>

## Options
1. <option> — pros / cons / cost
2. <option> — pros / cons / cost

## Decision
<the chosen option, in one sentence>

## Consequences
<what becomes easier, what becomes harder, follow-up work>

## Verification
<test, benchmark, or spike that proves the decision>
```

## Register

| ADR | Title | Owner WP | Blocks | Recommended option |
|---|---|---|---|---|
| [ADR-001](#adr-001) | Rust core boundaries | WP-05 | Core design | Pure logic core; adapters own I/O |
| [ADR-002](/engineering/architecture/adr/ADR-002-ffi-technology) | FFI technology: UniFFI vs C ABI | WP-04 (accepted at G2, H-135); WP-09 implements | Bindings | **Accepted:** hybrid — UniFFI control plane, C ABI byte channel; at most one copy per byte per direction |
| [ADR-003](/engineering/architecture/adr/ADR-003-ios-navigation) | iOS navigation strategy | WP-04 (accepted at G2, H-136); WP-11 implements | iOS SDK | **Accepted:** one path per page class — IOS-P1 public and offline, IOS-P2 authenticated, IOS-P3 own content; no https streaming |
| [ADR-004](#adr-004) | Cache storage format | WP-07 | Cache | Files for bodies + SQLite for metadata, atomic rename |
| [ADR-005](#adr-005) | Legacy protocol exact semantics and profiles | WP-01, WP-02 | legacy compatibility | **Accepted:** "Rivqen canonical" profile + `legacy-java`, `legacy-node`, `legacy-php` profiles |
| [ADR-006](#adr-006) | RQP wire schema and markup | WP-17 | RQP | **Accepted:** `data-rq-block` attributes, inline manifest, `Rq-*` Structured Field headers, base64url SHA-256 |
| [ADR-007](#adr-007) | WebSocket vs WebTransport | WP-16 | Realtime | WebSocket first; WebTransport experimental |
| [ADR-008](#adr-008) | Security and privacy model | WP-18 | Release | Per threat model |
| [ADR-009](/engineering/architecture/adr/ADR-009-streaming-abi) | Streaming ABI and buffer ownership | WP-09 (WP-04 adds data) | Streaming | Pull into a caller-owned buffer; no ownership transfer; the WebView block sets the granularity |
| [ADR-010](#adr-010) | Server SDK implementation strategy | WP-13…15 | Servers | Native-language SDKs + shared conformance |
| [ADR-011](#adr-011) | Cache encryption (AEAD and keys) | WP-07, WP-18 | Authenticated cache | Platform keys + audited AEAD crate |
| [ADR-012](#adr-012) | Release hardening profiles | WP-18, WP-21 | Release | Standard toolchain optimizations only |
| [ADR-013](#adr-013) | Delta mechanism for RQP | WP-17 | RQP | Evaluate Compression Dictionary Transport against block patches |
| [ADR-014](/engineering/architecture/adr/ADR-014-android-http-stack) | Android HTTP stack | WP-10 (WP-04 adds data) | Android | Adapter contract from the WP-04 data; OkHttp reference adapter; measure HttpEngine in WP-10 |
| [ADR-015](#adr-015) | Minimum platform versions | WP-21 | Support matrix | See support matrix |
| [ADR-016](#adr-016) | Native protocol first; legacy mode end of life | WP-00 | Roadmap, packaging | **Accepted direction:** RQP default in 1.0; legacy as optional modules, deprecated 1.0, removed 1.5 |
| [ADR-017](#adr-017) | Error handling and logging | WP-05 | All code | No ignored errors; typed error chains; redacted structured logs |

## ADR-001: Rust core boundaries {#adr-001}

- **Context.** The core must give identical behavior on Android and iOS. A core that owns sockets would need its own cookie jar and TLS state, which can disagree with the WebView.
- **Options.** (1) Pure logic core: events in, actions out. (2) Core owns HTTP. (3) Hybrid: core owns HTTP only for realtime channels.
- **Recommendation.** Option 1 now. Option 3 is an experiment after cookie/auth correctness is proven.
- **Verification.** Core builds with no platform dependency; all core tests run on a host machine with fake transports.
- **File:** [ADR-001](/engineering/architecture/adr/ADR-001-core-boundaries) with the WP-05 evidence (**Accepted**, human, 2026-11-11, H-139).

## ADR-002: FFI technology {#adr-002}

**Status: Accepted** (human, 2026-11-09, H-135). File: [ADR-002](/engineering/architecture/adr/ADR-002-ffi-technology). Recommendation: UniFFI for the control plane and a narrow C ABI byte channel for document bytes. Measured (indicative, H-129): UniFFI takes 21 to 108 times the time of the C ABI and copies each byte 3 or 4 times, against once. Verification: WP-09 on a device, both directions.

## ADR-003: iOS navigation strategy {#adr-003}

- **Context.** iOS has no public API to intercept standard HTTPS requests of `WKWebView` like Android's `shouldInterceptRequest`. `WKURLSchemeHandler` works only for custom schemes. <Badge type="tip" text="FACT" />
- **Status: Accepted** (human, 2026-11-09, H-136). File: [ADR-003](/engineering/architecture/adr/ADR-003-ios-navigation). Recommendation: IOS-P1 (`loadSimulatedRequest`) for public pages and offline snapshots, IOS-P2 (real navigation) for authenticated pages, IOS-P3 (own scheme) only for own local content; never `loadHTMLString` for a site's page. `https` main-document streaming is impossible with public API (CAP-I-07). Evidence: [iOS parity matrix](/engineering/platforms/ios-parity-matrix).

## ADR-004: Cache storage format {#adr-004}

- **Context.** The cache must survive crashes without mixing versions, and must be removable per partition.
- **Recommendation.** Body files written as temp file → fsync → rename. Metadata in SQLite (WAL mode) with one transaction per commit. Each partition is a separate directory.
- **Verification.** Kill-9 fault injection at every write step; corruption tests.

## ADR-005: Legacy protocol exact semantics {#adr-005}

- **Context.** The upstream Java, Node.js and PHP servers and the Android and iOS clients do not behave identically. See [Divergences](/engineering/protocol/legacy-divergences).
- **Status: Accepted** (human, 2026-10-09; closes Q-14).
- **Decision.** The canonical legacy profile `legacy-canonical` is the "Rivqen canonical" column of [Divergences](/engineering/protocol/legacy-divergences) and section 4 of the [markers grammar](/engineering/protocol/legacy-markers). Named profiles `legacy-java`, `legacy-node` and `legacy-php` reproduce each upstream server. A profile gets its own fixture only where its behavior differs from `legacy-canonical`.
- **Verification.** Golden fixtures for each profile (WP-02); the reference server selects the profile by configuration (WP-03).

## ADR-006: RQP wire schema and markup {#adr-006}

- **Status: Accepted** (human, 2026-10-09). Refinements: the page manifest is delivered **inline only**, in `<script type="application/rivqen-manifest+json">` in `<head>` (no header link); block hashes are **full SHA-256 encoded as base64url without padding** (43 characters; closes Q-15).
- **Further refinements** (human, 2026-10-09; [WP-17 S1](/engineering/plan/sprints/WP-17-S1) H-7…H-12): the client sends its block digests and a stateless server answers with changed blocks only (fallback: all blocks); revisions are normative and content-derived (`t1.`/`r1.` + base64url SHA-256); `page_id` and `sequence` exist only on push channels; each patch operation keeps its block hash; the envelope is JSON, block content is a string, and the hash covers the UTF-8 bytes of that string (no JCS); a block element without an explicit end tag makes the document invalid for RQP.
- **Further refinements** (human, 2026-10-12; [WP-17 S2](/engineering/plan/sprints/WP-17-S2) H-44…H-56): the client sends `Rq-Have` (template revision and 64-bit digest prefixes) and `If-None-Match` with its page revision; a delta has `Cache-Control: no-store` and no ETag; the full document is the same for every request, with ETag `r1.…` (weak with a content coding); `Rq-Capabilities` is an Inner List of Tokens; the only patch operation is `replace_block`; the client re-runs the markup analysis on the original and the patched document; `policy` and `limits` are part of the page revision; the server sends the full document when a changed block fails the client attribute rule.
- **Further refinements** (human, 2026-10-12 to 2026-10-18; [WP-17 S2](/engineering/plan/sprints/WP-17-S2) rounds 2–5 and [WP-17 S3](/engineering/plan/sprints/WP-17-S3); H-58, H-61…H-70, H-71…H-76): response binding and cache partitions (a `304` only for a matching `If-None-Match`; an explicit `public` for the `public` partition; no `public` after a redirect or a URL with userinfo); one exact `Content-Type` for the full document and UTF-8 only; allowlist entries must equal their serialized origin; the `noscript` content model, `svg` and `math` in blocks, and SVG ids in patch content (H-71, H-73…H-76). **H-72 changes the wire format:** the manifest element goes immediately after the head start tag of the document, not at the first `</head>`. An old client that expects the earlier position does not find the manifest, falls back to the normal page and reports `MANIFEST_INVALID`.
- **Further refinements** (human, [WP-17 S6](/engineering/plan/sprints/WP-17-S6) H-103…H-109): the manifest is `protocol`, `origin`, `path`, `page_revision` and optional `policy` and `limits`; the delta is `binding` (H-107, H-108: SHA-256 over the final URL, the base page revision, the client nonce of the field `Rq-Nonce` (H-109) and the next page revision) and `operations` of `block_id` and `content`; the client computes the template revision and the block hashes and checks them through the page revision; `TEMPLATE_MISMATCH` and `PATCH_BASE_MISMATCH` are reserved; `Rq-Have` keeps 64-bit prefixes.
- **Decision:** blocks are marked with the `data-rq-block` attribute on normal HTML elements; the server publishes a page manifest with revisions and SHA-256 block hashes (amended by H-103, [WP-17 S6](/engineering/plan/sprints/WP-17-S6): the manifest has the page revision only; the client computes the block digests and the template revision); headers use the `Rq-` prefix and Structured Field Values (RFC 9651). Bodies are JSON with JSON Schemas. A delta response uses its own media type and `Cache-Control: no-store` (amended 2026-10-12 by the human decision H-45, [WP-17 S2](/engineering/plan/sprints/WP-17-S2)).
- **Why:** the legacy comment markers were matched differently by five implementations (see [divergences](/engineering/protocol/legacy-divergences)); attributes are parsed by real HTML tokenizers and survive minifiers.
- **Verification:** interop tests across Rust, Node.js, Java and PHP parsers; intermediary tests through CDN and proxies.

## ADR-007: WebSocket vs WebTransport {#adr-007}

- **Recommendation.** WebSocket (WSS) for push invalidation in 1.x (P8). WebTransport behind a feature flag after device capability tests.
- **Verification.** Runtime feature probes on target WebViews; chaos tests.

## ADR-008: Security and privacy model {#adr-008}

- **Recommendation.** Adopt the [threat model](/engineering/security/threat-model) and [controls](/engineering/security/controls) as the baseline.

## ADR-009: Streaming ABI {#adr-009}

**Status: Proposed — decided in WP-09** (H-127). File: [ADR-009](/engineering/architecture/adr/ADR-009-streaming-abi). Recommendation: pull into a buffer that the caller owns (one copy, no ownership transfer); an error is never END; pending bytes are at most `prefixBufferBytes + maxChunkBytes`. The Android WebView reads in 2048-byte (WebView 145) or 64 KiB (WebView 91) blocks.

## ADR-010: Server SDK strategy {#adr-010}

- **Context.** A PHP extension or JNI library is hard to install on many hosts.
- **Recommendation.** Each server SDK is native to its language and passes the shared conformance suite. The Rust reference server is the oracle.

## ADR-011: Cache encryption {#adr-011}

- **Recommendation.** AEAD (AES-256-GCM or ChaCha20-Poly1305) from an audited crate. Data keys wrapped by Android Keystore / iOS Keychain keys. One key per partition, so logout revokes by key deletion.

## ADR-012: Release hardening profiles {#adr-012}

- **Recommendation.** Use only standard, documented toolchain features (R8, linker dead-code stripping, symbol stripping with private symbol files). See [Release profiles](/engineering/security/release-profiles).

## ADR-013: Delta mechanism for RQP {#adr-013}

- **Context.** A modern standard, Compression Dictionary Transport, lets a server compress a new response using the previous response as a dictionary. It can give small transfers without any template markup. <Badge type="warning" text="RESEARCH" />
- **Options.** (1) Block patches only. (2) Dictionary transport only. (3) Both: block patches for in-place updates, dictionary compression for full documents.
- **Recommendation.** Option 3, decided by benchmark.

## ADR-014: Android HTTP stack {#adr-014}

**Status: Proposed — decided in WP-10** (H-127). File: [ADR-014](/engineering/architecture/adr/ADR-014-android-http-stack). Recommendation: an adapter contract from the WP-04 data (visible redirects, fail-closed TLS, `CookieManager` cookies by H-131 and H-132, body errors, WHATWG binding URL), OkHttp as the reference adapter, and `HttpEngine` measured before WP-10 fixes the default. `HttpEngine`, Cronet and `HttpURLConnection`: NOT FOUND (not measured). Evidence: [Android capability matrix](/engineering/platforms/android-matrix).

## ADR-015: Minimum platform versions {#adr-015}

- **Recommendation.** Set the minimum versions at the start of WP-10/WP-11 from current store requirements and market data. See [Support matrix](/guide/support-matrix).
- **Data from WP-04** (S1, S2):
  - <Badge type="tip" text="FACT" /> WebView updates through Google Play since Android 5.0, but Chromium no longer supports WebView updates on Android 5 to 9 ([Lollipop](https://developer.android.com/about/versions/lollipop), [Chromium WebView prerelease](https://chromium.googlesource.com/chromium/src/+/HEAD/android_webview/docs/prerelease.md)).
  - <Badge type="tip" text="FACT" /> Device share (Android Studio data, 2026-10-01): API ≥ 24 99.4 %, ≥ 26 99.0 %, ≥ 28 97.0 %, ≥ 29 93.9 % ([distributions.json](https://dl.google.com/android/studio/metadata/distributions.json)). androidx.webkit 1.18.0 has minSdk 24. Google Play requires targetSdk 36 from 31 Aug 2026.
  - <Badge type="tip" text="FACT" /> The WebView of the API 29 image (91) has no `DOCUMENT_START_SCRIPT` and no `COOKIE_INTERCEPT`, reads in 64 KiB blocks and can give a null `getRequestHeaders()`. It is the WebView of the image, not of users (R-3).
  - <Badge type="tip" text="FACT" /> An app with a deployment target before 18.4, built with Xcode 27.2, that calls Swift-refined WebKit APIs, stops at launch on the iOS 18.5 simulator ([WebKit bug 293831](https://bugs.webkit.org/show_bug.cgi?id=293831)); on a device: NOT FOUND. WP-21 tests the SDK sample app on an 18.4+ runtime with the lowest deployment target.
  - <Badge type="tip" text="FACT" /> iOS 27.0, 26.5 and 18.5 give the same verdicts, except the `SameSite=Strict` behavior on an app load of another site and the `sameSitePolicy` value. iOS 17 is not tested (H-128): the proposed minimum "iOS 17" has no WP-04 data.
  - <Badge type="tip" text="FACT" /> FFI libraries with 16 KB aligned segments load on a 16 KB page image (API 37, H-133).

## ADR-016: Native protocol first; legacy mode end of life {#adr-016}

- **Decision direction (owner approved, 2026-10-09):** the Rivqen protocol (RQP) is the default from 1.0. Compatibility with the VasSonic legacy protocol is temporary.
- **Packaging:** legacy support ships as separate optional modules (Cargo feature, Android artifact, Swift product, npm/Maven/Composer packages), so that its removal does not break SemVer of the core.
- **Timeline:** deprecated from 1.0; security fixes only from 1.1; last shipped in 1.4.x; not published from 1.5.
- **Naming:** the word "Sonic" appears only in attribution and in literal legacy wire tokens (`sonicdiff`, header names).
- **Details:** [Modes and legacy deprecation](/engineering/protocol/versioning).

## ADR-017: Error handling and logging {#adr-017}

- **Decision direction:** no error is ignored; every error is typed, carries context and a stable code, and is propagated to a place that decides; logs are structured and redacted; release builds never crash on recoverable errors and never log secrets or page content.
- **Details:** [Error handling and logging](/engineering/architecture/errors-logging).
- **File:** [ADR-017](/engineering/architecture/adr/ADR-017-errors-logging) with the WP-05 evidence (**Accepted**, human, 2026-11-11, H-140).
