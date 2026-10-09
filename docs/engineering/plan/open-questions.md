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
| Q-13 | Can Compression Dictionary Transport replace or complement block patches in RQP? | Benchmark on real pages; platform support check | RQP wire format | WP-17 |
| Q-14 | Which legacy server behavior is the "canonical profile" when Java, Node.js and PHP differ? | Divergence analysis + usage evidence | ADR-005 | WP-01 |
| Q-15 | RQP patch overhead: per-block SHA-256 (64 hex) and envelope fields make a patch of four tiny blocks larger than the legacy data body (869 B vs 512 B raw, measured). Use truncated hashes, hashes only in the manifest, or a compact encoding? | Measure variants with `examples/server-demo/scripts/measure.js` on several real pages | RQP wire format | WP-17 |

## 2. Closed

| ID | Question | Answer | Evidence |
|---|---|---|---|
| Q-01 | Exact byte-level rules of legacy template/data split and hashing in each upstream **server** | Confirmed by code audit + 115 trace checks; documented with divergences D-01…D-13 | [Server trace report](/engineering/protocol/legacy-traces) |
| Q-00 | Which upstream commit is the reference? | `Tencent/VasSonic@59936beff656d4b5718ff6444d6c5e001a2c5231` (2019-04-15), the last commit on `master` | [Upstream audit](/research/upstream-vassonic) |
