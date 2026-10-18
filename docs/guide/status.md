# Project status

**Current stage:** Phase P0 (Foundation and evidence) is **closed**: milestone **M1 "init docs"** is delivered and gate **G0** is passed. **Now:** Phase P1 — Contracts. WP-17 (RQP specification) sprint S1 is closed: the RQP markup rules are normative and four parser candidates agree on all golden markup fixtures. Sprint S2 is closed: negotiation, manifest and patch of `rqp/1` are normative, with JSON Schemas, 174 protocol fixtures (78 server, 96 client) and a server demo that passes them. Sprint S3 is closed: the `noscript` content model (H-71), the manifest after the head start tag (H-72) and `svg`/`math` in blocks (H-73…H-75) are normative and in all four parser candidates; 820 of 882 corpus pages are valid. WP-17 stays open for further work; WP-02 and WP-03 follow.

## What exists

| Item | State |
|---|---|
| Documentation site (this site) | ✅ Available |
| Development plan with work packages | ✅ Available |
| Upstream VasSonic behavioral audit (summary) | ✅ Available — see [Research](/research/upstream-vassonic) |
| Upstream server traces (Node.js, PHP, Java) | ✅ Available — see [server traces](/engineering/protocol/legacy-traces) |
| Rivqen protocol (RQP) specification | 🔄 In progress — markup, negotiation, manifest and patch normative ([RQP markup](/engineering/protocol/markup), [negotiation](/engineering/protocol/negotiation), [manifest and patch](/engineering/protocol/manifest-patch)); 174 protocol fixtures (78 server, 96 client) and 247 markup fixtures; S1–S3 closed, WP-17 open |
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
