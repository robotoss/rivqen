# Architecture decision records

An **ADR** records one important decision: the context, the options, the decision and its consequences. ADRs make decisions traceable. A decision in an ADR can change only through a new ADR that supersedes it.

**Status:** <Badge type="info" text="DESIGN" /> ADR-005 and ADR-006 are **Accepted** (human, 2026-10-09). All other ADRs below are **Proposed**. Each has an owner work package and the evidence that will close it.

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
| [ADR-002](#adr-002) | FFI technology: UniFFI vs C ABI | WP-09 | Bindings | UniFFI for control plane; narrow C ABI for byte streams if measured |
| [ADR-003](#adr-003) | iOS navigation strategy | WP-11 | iOS SDK | Decide after spike IOS-P1/P2/P3 |
| [ADR-004](#adr-004) | Cache storage format | WP-07 | Cache | Files for bodies + SQLite for metadata, atomic rename |
| [ADR-005](#adr-005) | Legacy protocol exact semantics and profiles | WP-01, WP-02 | legacy compatibility | **Accepted:** "Rivqen canonical" profile + `legacy-java`, `legacy-node`, `legacy-php` profiles |
| [ADR-006](#adr-006) | RQP wire schema and markup | WP-17 | RQP | **Accepted:** `data-rq-block` attributes, inline manifest, `Rq-*` Structured Field headers, base64url SHA-256 |
| [ADR-007](#adr-007) | WebSocket vs WebTransport | WP-16 | Realtime | WebSocket first; WebTransport experimental |
| [ADR-008](#adr-008) | Security and privacy model | WP-18 | Release | Per threat model |
| [ADR-009](#adr-009) | Streaming ABI and buffer ownership | WP-09 | Streaming | Bounded chunk API with single owner |
| [ADR-010](#adr-010) | Server SDK implementation strategy | WP-13…15 | Servers | Native-language SDKs + shared conformance |
| [ADR-011](#adr-011) | Cache encryption (AEAD and keys) | WP-07, WP-18 | Authenticated cache | Platform keys + audited AEAD crate |
| [ADR-012](#adr-012) | Release hardening profiles | WP-18, WP-21 | Release | Standard toolchain optimizations only |
| [ADR-013](#adr-013) | Delta mechanism for RQP | WP-17 | RQP | Evaluate Compression Dictionary Transport against block patches |
| [ADR-014](#adr-014) | Android HTTP stack | WP-10 | Android | Adapter interface; choose by benchmark |
| [ADR-015](#adr-015) | Minimum platform versions | WP-21 | Support matrix | See support matrix |
| [ADR-016](#adr-016) | Native protocol first; legacy mode end of life | WP-00 | Roadmap, packaging | **Accepted direction:** RQP default in 1.0; legacy as optional modules, deprecated 1.0, removed 1.5 |
| [ADR-017](#adr-017) | Error handling and logging | WP-05 | All code | No ignored errors; typed error chains; redacted structured logs |

## ADR-001: Rust core boundaries {#adr-001}

- **Context.** The core must give identical behavior on Android and iOS. A core that owns sockets would need its own cookie jar and TLS state, which can disagree with the WebView.
- **Options.** (1) Pure logic core: events in, actions out. (2) Core owns HTTP. (3) Hybrid: core owns HTTP only for realtime channels.
- **Recommendation.** Option 1 now. Option 3 is an experiment after cookie/auth correctness is proven.
- **Verification.** Core builds with no platform dependency; all core tests run on a host machine with fake transports.

## ADR-002: FFI technology {#adr-002}

- **Context.** UniFFI generates Kotlin and Swift bindings from Rust. Large byte buffers across UniFFI can be copied on each call.
- **Options.** (1) UniFFI only. (2) Hand-written C ABI + JNI + Swift wrapper. (3) UniFFI for control plane, C ABI for the HTML byte stream.
- **Recommendation.** Option 3 if the WP-04 FFI benchmark shows more than the agreed copy cost; otherwise option 1.
- **Verification.** Microbenchmark: 1 MiB document in 16 KiB chunks, measure latency and copied bytes on a mid-range Android device and an iPhone.

## ADR-003: iOS navigation strategy {#adr-003}

- **Context.** iOS has no public API to intercept standard HTTPS requests of `WKWebView` like Android's `shouldInterceptRequest`. `WKURLSchemeHandler` works only for custom schemes. <Badge type="tip" text="FACT" />
- **Options.** IOS-P1 early fetch + `loadHTMLString`/`load(_:mimeType:…baseURL:)`; IOS-P2 normal navigation + JS diff; IOS-P3 custom scheme for trusted content; plus `loadSimulatedRequest` if the spike confirms it.
- **Recommendation.** Decide after the spike matrix in WP-04. Prefer real navigation (P2) for authenticated pages.
- **Verification.** `ios-parity-matrix.md` with supported / conditional / impossible per feature.

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
- **Decision:** blocks are marked with the `data-rq-block` attribute on normal HTML elements; the server publishes a page manifest with revisions and SHA-256 block hashes; headers use the `Rq-` prefix and Structured Field Values (RFC 9651). Bodies are JSON with JSON Schemas. A delta response uses its own media type and `Vary`.
- **Why:** the legacy comment markers were matched differently by five implementations (see [divergences](/engineering/protocol/legacy-divergences)); attributes are parsed by real HTML tokenizers and survive minifiers.
- **Verification:** interop tests across Rust, Node.js, Java and PHP parsers; intermediary tests through CDN and proxies.

## ADR-007: WebSocket vs WebTransport {#adr-007}

- **Recommendation.** WebSocket (WSS) for push invalidation in 1.x (P8). WebTransport behind a feature flag after device capability tests.
- **Verification.** Runtime feature probes on target WebViews; chaos tests.

## ADR-008: Security and privacy model {#adr-008}

- **Recommendation.** Adopt the [threat model](/engineering/security/threat-model) and [controls](/engineering/security/controls) as the baseline.

## ADR-009: Streaming ABI {#adr-009}

- **Recommendation.** Each chunk has one owner and a bounded size. The adapter pulls chunks (pull model) to get natural backpressure.

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

- **Options.** Platform `HttpEngine` (API 34+) / Cronet, OkHttp, `HttpURLConnection`.
- **Recommendation.** Define an adapter interface. Default to the stack the host app already uses for cookies and auth. Do not ship two stacks without a measured benefit.

## ADR-015: Minimum platform versions {#adr-015}

- **Recommendation.** Set the minimum versions at the start of WP-10/WP-11 from current store requirements and market data. See [Support matrix](/guide/support-matrix).

## ADR-016: Native protocol first; legacy mode end of life {#adr-016}

- **Decision direction (owner approved, 2026-10-09):** the Rivqen protocol (RQP) is the default from 1.0. Compatibility with the VasSonic legacy protocol is temporary.
- **Packaging:** legacy support ships as separate optional modules (Cargo feature, Android artifact, Swift product, npm/Maven/Composer packages), so that its removal does not break SemVer of the core.
- **Timeline:** deprecated from 1.0; security fixes only from 1.1; last shipped in 1.4.x; not published from 1.5.
- **Naming:** the word "Sonic" appears only in attribution and in literal legacy wire tokens (`sonicdiff`, header names).
- **Details:** [Modes and legacy deprecation](/engineering/protocol/versioning).

## ADR-017: Error handling and logging {#adr-017}

- **Decision direction:** no error is ignored; every error is typed, carries context and a stable code, and is propagated to a place that decides; logs are structured and redacted; release builds never crash on recoverable errors and never log secrets or page content.
- **Details:** [Error handling and logging](/engineering/architecture/errors-logging).
