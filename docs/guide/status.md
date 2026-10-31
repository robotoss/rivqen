# Project status

**Current stage:** Phase P0 (Foundation and evidence) is **closed**: milestone **M1 "init docs"** is delivered and gate **G0** is passed. **Now:** Phase P1 — Contracts. WP-17 (RQP specification) sprint S1 is closed: the RQP markup rules are normative and four parser candidates agree on all golden markup fixtures. Sprint S2 is closed: negotiation, manifest and patch of `rqp/1` are normative, with JSON Schemas, 174 protocol fixtures (78 server, 96 client) and a server demo that passes them. Sprint S3 is closed: the `noscript` content model (H-71), the manifest after the head start tag (H-72) and `svg`/`math` in blocks (H-73…H-75) are normative and in all four parser candidates; 820 of 882 corpus pages are valid. Sprint S4 is closed: the audit follow-up relaxed the over-tight rules (H-77, H-78: patch-safety checks moved from the markup to the patch content rule; H-79a implied head; H-84; H-85a implied table closure; H-86 JSON-LD), so 91.5 % of the small and 66.5 % of the main block variants of the corpus are valid (S3: 81.8 % and 35.1 %); 813 of 868 corpus pages are valid. Sprint S5 is closed: `template` and `noscript` are allowed in blocks (H-94…H-97, DL-056; main block variants of the corpus 80.2 %), the patch apply step is normative (H-102), and WebView checks on iOS and Android confirm the scripting-disabled parse and the delivery rules (Q-19, Q-20). Sprint S6 is closed: the manifest and the delta carry only what the client cannot compute (H-103, H-104), a delta is bound to its URL, base and a client nonce (H-107…H-109), and field parsing is linear (DL-063); on the demo page RQP now sends 2,357 bytes gzip on a cold visit (legacy 2,232) and 217 on a data change (legacy 316). WP-17 is done and merged into `main` on 2026-10-20 (#3). WP-02 (legacy golden suite and conformance runner) is done; its merge request to `main` is pending: 171 legacy fixtures confirmed by 117 upstream traces, 22 malformed cases, and a runner that tests a server by URL, also against the live upstream servers. WP-03 (Rust reference server, gate G1) follows.

## What exists

| Item | State |
|---|---|
| Documentation site (this site) | ✅ Available |
| Development plan with work packages | ✅ Available |
| Upstream VasSonic behavioral audit (summary) | ✅ Available — see [Research](/research/upstream-vassonic) |
| Upstream server traces (Node.js, PHP, Java) | ✅ Available — see [server traces](/engineering/protocol/legacy-traces) |
| Rivqen protocol (RQP) specification | ✅ Done (merged 2026-10-20, #3) — markup, negotiation, manifest and patch normative ([RQP markup](/engineering/protocol/markup), [negotiation](/engineering/protocol/negotiation), [manifest and patch](/engineering/protocol/manifest-patch)); 265 protocol fixtures (112 server, 153 client) and 362 markup fixtures; S1–S6 closed |
| Server demo (RQP reference behavior) | ✅ Runs — see [Examples: Server](/guide/examples/server) |
| Conformance runner (tests a server by URL) | ✅ Available — see [Server conformance](/engineering/server/conformance); RQP and legacy suites |
| Golden fixtures | ✅ RQP markup: 362 fixtures (`fixtures/rqp/markup/`); RQP protocol: 265 fixtures (112 server, 153 client; `fixtures/rqp/protocol/`); legacy: 171 fixtures (59 canonical, 112 for the upstream profiles; `fixtures/legacy/`); malformed: 22 cases and 54 seeds (`fixtures/malformed/`) |
| Rust core | ⏳ Planned (P3) |
| Android and iOS SDKs | ⏳ Planned (P5) |
| Web and React SDK | ⏳ Planned (P6) |
| Node.js, Java, PHP server SDKs | ⏳ Planned (P4) |

## What "planned API" means

Code samples in the Guide show the **planned** API. Names and signatures can change before the first release. Every sample starts with the comment `Planned API — not released`.

## Follow progress

- [Roadmap and phases](/engineering/plan/roadmap)
- [Work packages](/engineering/plan/work-packages)
- [GitHub repository](https://github.com/robotoss/rivqen)
