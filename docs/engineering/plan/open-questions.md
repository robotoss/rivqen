# Open questions

This page lists questions that block or shape decisions. Each question has a way to close it and an owner work package. Closed questions move to the "Closed" table with the answer and the evidence.

**Status:** <Badge type="warning" text="RESEARCH" />

## 1. Open

| ID | Question | How to close | Blocks | Owner |
|---|---|---|---|---|
| Q-02 | Full FSM difference between Quick and Standard modes | Golden traces on a test bench | Parity | WP-01 |
| Q-03 | How the legacy iOS SDK connected native network results to the WebView | Code audit (M1) + run on an old device if possible | iOS design | WP-01 |
| Q-04 | Which WKWebView APIs allow an early display without losing origin, cookies, CSP and Service Worker behavior | IOS-P1/P2/P3 experiments | iOS release | WP-04 |
| Q-05 | Which Android System WebView / iOS versions support the needed WebTransport features | Feature probes on a device farm | WT opt-in only | WP-16 |
| Q-06 | Cost of UniFFI for large HTML payloads | Microbenchmark + mobile memory profiles | Streaming FFI choice | WP-04 |
| Q-07 | Shared Rust server implementation vs native-language SDKs | Conformance + ops benchmark | Not the mobile core | WP-13…15 |
| Q-08 | Do offline packages need manifest signatures, or is TLS + integrity enough? | Threat model, key management cost | Signed bundles | WP-18 |
| Q-09 | Exact supported versions of OS, React, Java, PHP, Node.js | Market data + EOL policy + CI cost | Support matrix | WP-21 |
| Q-10 | What does "exact copy" mean where upstream behavior is unsafe? | Compatibility/security review; exception policy | legacy release | WP-18, ADR-005 |
| Q-11 | How CDN compression and caching interact with legacy/RQP headers | Proxy conformance lab | Server release | WP-13…15 |
| Q-12 | Who owns cookies and auth during native early fetch on Android and iOS? | Cookie isolation tests; API mapping | Sensitive pages | WP-04 |
| Q-13 | Can Compression Dictionary Transport replace or complement block patches in RQP? | Benchmark on real pages; platform support check | Nothing in P1 (human, 2026-10-09: moved to P8; RQP 1 reserves the capability token `dict-compression`) | WP-16 |
| Q-16 | Use the Arcmutate plugins (line-level git scope for PIT, Kotlin-aware mutation, Android)? They are commercial; a free licence for open-source projects is announced but not verified | Confirm licence terms in writing; compare with class-scoped PIT on the first JVM/Kotlin WP | JVM/Kotlin mutation gate | WP-14, WP-10 |
| Q-17 | Does an optional Chrome oracle in `tools/rqp-markup/crosscheck` find gaps that parse5 8.0.1 cannot (processing instructions, content patching, new `select` modes)? ([edge cases §8.6](/engineering/protocol/markup-edge-cases#_8-6-follow-ups), item 1) | Architect decision to add the oracle, then a run of the `cases` command | Confidence in M-rules after the 2025 WHATWG changes | WP-19 (human decision H-98, WP-17 S5) |

## 2. Closed

| ID | Question | Answer | Evidence |
|---|---|---|---|
| Q-21 | A repeatable sweep for H-72 (the manifest position) | `node src/cli.mjs head` in `tools/rqp-markup/crosscheck` (WP-17 S5 T-09, human decision H-100): the shipped insertion point (H-72, H-79a) and client rule C2, parse5 8.0.1 in both scripting views, the four candidates. Run of 2026-10-19: 201 005 documents (fixtures, protocol pages, cases, 200 000 generated) and 789 465 hostile positions, 0 findings | `tools/rqp-markup/crosscheck/README.md` (command `head`) |
| Q-18 | How many corpus pages hold `template for`, `<?marker` and `<?start` | Counted on the S5 corpus (2026-10-19, 864 pages): 1 page has an HTML `<template for=…>` in markup (rejected by M-10 item 3); 0 pages have `<?marker`, `<?start` or `<?end`; 13 pages have another `<?` token. M-10 item 3 costs at most 1 page | [WP-17 S5](/engineering/plan/sprints/WP-17-S5) progress log (counts only; no page bytes) |
| Q-20 | Do real WebViews follow H-66 (exact `Content-Type`, UTF-8 delivery) and the byte order mark rule (DL-040) | Yes, with one exact `Content-Type` (WP-17 S5 T-03, T-04; iOS 26.5 WebKit 8624.2.5.10.4, Android 17 WebView 155.0.8059.43): the byte order mark wins over a Latin-1 header charset; the header charset wins over `meta`. Without a charset WebKit uses windows-1252 and Chromium UTF-8 (in this run); with two `Content-Type` lines the last one wins; a UTF-8 page with a byte order mark and no `Content-Type` is shown as `text/plain` by both. So the SDK must send the one exact `Content-Type` of H-66 | `tools/webview-lab/results/ios/`, `tools/webview-lab/results/android/` (`summary.md`, `run.json`) |
| Q-19 | What does a browser with scripting disabled do with the `noscript` shapes of M-11 | A WebView with content JavaScript off parses with scripting disabled, as parse5 with `scriptingEnabled: false` (WP-17 S5 T-03, T-04): iOS 42 of 47 equal, Android 45 of 47. Differences: both engines cap the parser tree depth at 512 (only the `LIMIT` fixtures, which RQP rejects); WebKit with content JavaScript off puts no `script` element in the DOM and drops `on*` attributes, Chromium keeps `script` | `tools/webview-lab/results/*/summary.md`; WebKit `HTMLConstructionSite.cpp`, `SettingsBase.h`; Chromium `html_construction_site.cc` |
| Q-22 | The legacy byte comparison (RQP against the upstream VasSonic server on the same page) | Measured again on 2026-10-19 (WP-17 S5 T-05). The upstream server sends fewer bytes than RQP: cold 2,244 vs 2,707 (gzip), data change 294 vs 526 (gzip). RQP claims no byte advantage over legacy | `evidence/benchmarks/rqp-bytes-2026-10-19.json`; [performance](/guide/performance#legacy-column) |
| Q-15 | RQP patch overhead from 64-hex SHA-256 per block | Full SHA-256 in base64url without padding (43 characters); no truncation. Decided by the human, 2026-10-09 | [ADR-006](/engineering/architecture/adr/#adr-006), [WP-17 S1](/engineering/plan/sprints/WP-17-S1) |
| Q-14 | Canonical legacy server profile | "Rivqen canonical" column + profiles `legacy-java`, `legacy-node`, `legacy-php`. Decided by the human, 2026-10-09 | [ADR-005](/engineering/architecture/adr/#adr-005) |
| Q-01 | Exact byte-level rules of legacy template/data split and hashing in each upstream **server** | Confirmed by code audit + 115 trace checks; documented with divergences D-01…D-13 | [Server trace report](/engineering/protocol/legacy-traces) |
| Q-00 | Which upstream commit is the reference? | `Tencent/VasSonic@59936beff656d4b5718ff6444d6c5e001a2c5231` (2019-04-15), the last commit on `master` | [Upstream audit](/research/upstream-vassonic) |
