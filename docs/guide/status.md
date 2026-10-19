# Project status

**Current stage:** Phase P0 (Foundation and evidence) is **closed**: milestone **M1 "init docs"** is delivered and gate **G0** is passed. **Now:** Phase P1 — Contracts. WP-17 (RQP specification) sprint S1 is closed: the RQP markup rules are normative and four parser candidates agree on all golden markup fixtures. Sprint S2 is closed: negotiation, manifest and patch of `rqp/1` are normative, with JSON Schemas, 174 protocol fixtures (78 server, 96 client) and a server demo that passes them. Sprint S3 is closed: the `noscript` content model (H-71), the manifest after the head start tag (H-72) and `svg`/`math` in blocks (H-73…H-75) are normative and in all four parser candidates; 820 of 882 corpus pages are valid. Sprint S4 is closed: the audit follow-up relaxed the over-tight rules (H-77, H-78: patch-safety checks moved from the markup to the patch content rule; H-79a implied head; H-84; H-85a implied table closure; H-86 JSON-LD), so 91.5 % of the small and 66.5 % of the main block variants of the corpus are valid (S3: 81.8 % and 35.1 %); 813 of 868 corpus pages are valid. Sprint S5 is closed: `template` and `noscript` are allowed in blocks (H-94…H-97, DL-056; main block variants of the corpus 80.2 %), the patch apply step is normative (H-102), and WebView checks on iOS and Android confirm the scripting-disabled parse and the delivery rules (Q-19, Q-20). The legacy byte comparison shows that the upstream server sends fewer bytes than RQP; sprint S6 studies this. WP-17 stays open for further work; WP-02 and WP-03 follow.

## What exists

| Item | State |
|---|---|
| Documentation site (this site) | ✅ Available |
| Development plan with work packages | ✅ Available |
| Upstream VasSonic behavioral audit (summary) | ✅ Available — see [Research](/research/upstream-vassonic) |
| Upstream server traces (Node.js, PHP, Java) | ✅ Available — see [server traces](/engineering/protocol/legacy-traces) |
| Rivqen protocol (RQP) specification | 🔄 In progress — markup, negotiation, manifest and patch normative ([RQP markup](/engineering/protocol/markup), [negotiation](/engineering/protocol/negotiation), [manifest and patch](/engineering/protocol/manifest-patch)); 222 protocol fixtures (89 server, 133 client) and 362 markup fixtures; S1–S5 closed, WP-17 open |
| Server demo (RQP reference behavior) | ✅ Runs — see [Examples: Server](/guide/examples/server) |
| Golden fixtures | 🔄 RQP markup: 247 fixtures (`fixtures/rqp/markup/`); RQP protocol: 174 fixtures (`fixtures/rqp/protocol/`); legacy fixtures planned |
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
