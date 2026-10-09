# Risk register

This register lists known risks, how we detect them, and how we reduce them. Review it at each gate.

**Status:** <Badge type="info" text="DESIGN" />

Likelihood and impact: **H** high, **M** medium, **L** low, **C** critical.

| ID | Risk | Likelihood / impact | Detection | Mitigation | Owner WP |
|---|---|---|---|---|---|
| RSK-01 | iOS cannot repeat Android stream interception with public APIs | H / H | IOS-P1/P2/P3 spike | Functional equivalent + documented difference; no private API | WP-04, WP-11 |
| RSK-02 | Cookies or origin break when HTML is loaded with a base URL on iOS | H / H | Cookie/CSP/JS matrix | Real navigation for sensitive pages | WP-04 |
| RSK-03 | HTTP/3 or WebTransport not available in a target WebView | H / M | Runtime probes | HTTP/2/1.1 fallback; WT optional | WP-16 |
| RSK-04 | Push channel delivers out-of-order patches | M / H | Chaos suite | Sequence + base revision + HTTPS resync | WP-16 |
| RSK-05 | FFI copies too much HTML | M / M | Flamegraphs, copy counters | Chunk API, `bytes`, narrow boundary | WP-09 |
| RSK-06 | Data of another account appears from cache | M / C | Adversarial auth tests | Mandatory partition, encryption, revoke on logout | WP-07 |
| RSK-07 | Marker parser enables XSS | M / C | Fuzz + DOM security suite | Formal grammar, context escaping, narrow trust | WP-06 |
| RSK-08 | Server languages compute different hashes or templates | H / H | Cross-language golden tests | Exact canonicalization spec + oracle | WP-02 |
| RSK-09 | Streaming SSR conflicts with diff | M / M | Partial-block tests | Block-boundary buffering | WP-12, WP-13 |
| RSK-10 | Binary size grows (Rust + two HTTP stacks) | M / M | Size and startup profiling | Platform HTTP first; LTO/strip by measurement | WP-09, WP-10 |
| RSK-11 | Prefetch costs battery and data | H / M | Battery and data-saver tests | Adaptive budgets; trusted routes only | WP-19 |
| RSK-12 | Legacy cache semantics conflict with RFC 9111 or privacy | M / H | Security review | Strict legacy mode; restrictive policy; `no-store` wins | WP-07, WP-18 |
| RSK-13 | Teams start all SDKs before the spec is fixed | H / H | Cross-team failures | Mandatory gates G0–G2 | WP-00 |
| RSK-14 | Upstream behavior unclear or contradictory | H / M | Audit "NOT FOUND" entries | Capture traces from running upstream; profiles per divergence | WP-01 |
| RSK-15 | Clean-room violation (copied code) | L / H | Similarity scan, review | Provenance policy; separate evidence and implementation | WP-21 |
| RSK-16 | Trademark conflict with the chosen name | L / H | Clearance search | `BRAND-001` before publishing packages | WP-21 |
| RSK-17 | AI-generated code with hidden defects or license issues | M / H | Review, tests, similarity scan | Same rules as human code; human gates | WP-00 |
| RSK-18 | Platform policy change (store rules, WebView APIs) | M / M | Release compliance checklist each release | Public APIs only; feature detection | WP-21 |
| RSK-19 | Upstream VasSonic is unmaintained since 2019; old samples do not build | H / L | Lab build attempts | Use code reading + old emulator traces; do not depend on running it | WP-01 |
