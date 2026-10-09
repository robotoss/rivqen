# Invariants

An invariant is a rule that every design, every change and every release must keep. A change that breaks an invariant is rejected. Only a maintainers' decision recorded in an ADR can change this list.

**Status:** <Badge type="info" text="DESIGN" /> <Badge type="danger" text="GATE" /> Reviewers check every pull request against this list.

## Product invariants

| ID | Name | Rule |
|---|---|---|
| `INV-01` | Fidelity | While the legacy module exists, it reproduces every documented legacy behavior. Every difference is recorded and tested. |
| `INV-02` | Openness | Rivqen has no hidden dependency on a specific network, CDN, vendor server, or private OS API. |
| `INV-03` | Security first | Acceleration never disables TLS, cookie policy, CSP, account isolation or origin restrictions. |
| `INV-04` | Backward compatibility | Legacy mode and RQP are explicitly different. An old server does not need to know RQP. A new server with the legacy module serves legacy clients without lower security. |
| `INV-05` | Predictable fallback | On any error, Rivqen loads the page with normal HTTPS. Never a white screen. Never an endless wait. |
| `INV-06` | One logic | Protocol, session state, validation, diff, cache choice and serialization are defined by **one** Rust core. Adapters own WebView and OS security. |
| `INV-07` | Bounded resources | Memory, streams, cache and concurrent requests have strict limits. Every operation can be cancelled. Streams have backpressure. |
| `INV-08` | Proof | No optimization is accepted without a correctness test and a measurement. |
| `INV-09` | Honest comparison | An RQP improvement is never presented as equal to a legacy mechanism unless it keeps the same semantics. |
| `INV-10` | No unsafe magic | No HTTPS interception by MITM. No private WKWebView API. No JavaScript injection into third-party sites. |

## Engineering invariants

| ID | Rule |
|---|---|
| `INV-11` | The core has no global mutable state. Engines and sessions are created and destroyed explicitly. |
| `INV-12` | Every event carries a session `generation` and a `request_id`. A stale callback is a no-op. |
| `INV-13` | A diff is validated **before** it is applied. An invalid diff never changes the page or the cache. |
| `INV-14` | A cache commit is atomic: after a crash, the cache holds the old valid snapshot **or** the new valid snapshot, never a mix. |
| `INV-15` | Data of one account never appears in a session of another account, not even for one frame. |
| `INV-16` | Network data is untrusted, on every transport. HTTP/3 or WebTransport does not replace validation or authorization. |
| `INV-17` | Push messages are **signals to revalidate**, not orders to insert HTML. |
| `INV-18` | Logs and metrics never contain cookies, authorization headers, tokens, page HTML or personal data. |
| `INV-19` | A security fix is a patch or a documented mitigation. Hiding a version string is not a fix. |
| `INV-20` | Generated or AI-written code follows the same review, provenance and test rules as human code. |

## How to check an invariant

| Invariant | Primary check |
|---|---|
| `INV-01` | Golden legacy fixtures in CI (`FX-LEG-*`) |
| `INV-03`, `INV-10` | Security test suite; release compliance checklist |
| `INV-05` | Chaos suite: every injected fault ends in a normal HTTPS load |
| `INV-07` | Fuzz and property tests with bounded memory; device memory profiles |
| `INV-12`, `INV-13`, `INV-14` | Property tests on the session state machine and cache transactions |
| `INV-15` | Multi-account concurrency and logout tests |
| `INV-18` | Log privacy test: scan logs and network captures for secrets |

## Related

- [Security controls](/engineering/security/controls)
- [Gates and Definition of Done](/engineering/plan/gates)
