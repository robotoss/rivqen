# Gates and Definition of Done

A gate is a go/no-go decision at the end of a phase. Work cannot pass a gate until every condition is true and has evidence. The architecture lead (or maintainers group) decides each gate.

**Status:** <Badge type="danger" text="GATE" /> <Badge type="info" text="DESIGN" />

[[toc]]

## 1. Gates

| Gate | After | Conditions |
|---|---|---|
| **G0** ✅ passed | P0 | Upstream pinned at a fixed SHA; license inventory done; behavioral audit covers headers, markers, hashing, `cache-offline`, result codes, FSM, cache; divergences listed |
| **G1** | P1 | RQP and legacy golden suites exist **before** SDK code; reference server passes both; each legacy divergence has a profile and fixture |
| **G2** | P2 | Android and iOS feasibility proven with public APIs; incompatibilities documented publicly (ADRs) |
| **G3** | P3 | Core passes unit, property, fuzz and golden tests; `unsafe` reviewed; resource limits enforced |
| **G4-S / G4-M / G4-W** | P4 / P5 / P6 | Servers, mobile SDKs and web SDK pass the same conformance set and their E2E suites |
| **G5** | Beta | `SECURITY-001` passed; benchmarks vs B0 published; signed release pipeline ready |
| **G6** | 1.0 | `LEGAL-001` and `BRAND-001` complete; RQP feature-complete; legacy parity matrix covered (exact, or documented exception); deprecation policy and migration tool published |
| **G7** | P8 (1.x) | No RQP regressions; realtime gated by device capability; ADR-013 decided |
| **G8** | 1.5 | Legacy modules removed; migration complete for known integrators |

## 2. Functional Definition of Done (1.0)

- [ ] Every VasSonic feature is in the [parity matrix](/engineering/protocol/legacy-divergences#parity-matrix) with a source and a test.
- [ ] Quick and Standard behaviors are supported as confirmed by fixtures.
- [ ] The first HTML loads in parallel with the WebView where the platform allows it without policy violations.
- [ ] Cache hit / 304, data diff, template change, full load, offline, and each `cache-offline` value work.
- [ ] Local offline assets and subresource prefetch work in a safe, allowed form.
- [ ] One Rust core serves Android and iOS; bindings are stable and tested.
- [ ] Kotlin and Swift APIs cover lifecycle, cancel, callback order and fallback.
- [ ] React SSR, hydration and the bridge work without conflict with React state.
- [ ] Java, Node.js and PHP pass the same golden conformance suite.
- [ ] RQP is the default; the legacy module is opt-in, deprecated, and degrades to HTTPS by policy.
- [ ] WebSocket and WebTransport are never needed for the first load or for state safety.

## 3. Security Definition of Done

- [ ] No cross-account cache or auth leak, including cold/warm races.
- [ ] No bypass of TLS, ATS or certificate validation; no private WKWebView API.
- [ ] No JS execution or bridge call from a non-allowed origin.
- [ ] `no-store` and invalidation are strictly applied.
- [ ] No unresolved critical/high findings.
- [ ] All artifacts have provenance, lockfiles, SBOM, license notices and signatures.
- [ ] `SEC-15`…`SEC-26` have tests and evidence; `SECURITY-001` passed.
- [ ] Profiles `minimal-exposure` and `hardened-release` pass their checklists.
- [ ] `PATCH-001` process tested with a tabletop exercise.
- [ ] Public name approved under `BRAND-001` before packages are published.

## 4. Engineering Definition of Done

- [ ] Public modules documented with examples.
- [ ] Golden legacy/RQP, chaos, fuzz and device matrices are green.
- [ ] Rust `unsafe` is absent or separately audited.
- [ ] Memory budgets and async cancellation confirmed with instruments.
- [ ] Demos exist for Android, iOS, React and the three servers.
- [ ] Performance confirmed on representative networks and devices, published with method.
- [ ] Docs separate "equivalent to VasSonic", "improved in RQP" and "OS limitation".
- [ ] Kill switch and safe HTTPS fallback tested.

## 5. Definition of Done for a single task

A task is done when:

1. The acceptance criteria in the issue are met.
2. Tests at every required level pass (unit is not a substitute for device tests).
3. Docs for every changed surface are updated.
4. Required reviewers approved.
5. No fixture was changed to make the code pass.

::: info Parity is not identical WebView behavior
Done means 100 % coverage of the agreed matrix: exact compatibility where possible, and documented limits with safe alternatives where not.
:::
