# Security controls

This register lists every security control. Each control has a test that proves it. A control is not "done" until its test passes in CI or on devices.

**Status:** <Badge type="info" text="DESIGN" /> All controls: **planned**.

[[toc]]

## 1. Baseline controls

| ID | Control | Why | Verification |
|---|---|---|---|
| `SEC-01` | TLS everywhere with system chain and hostname validation | Stop MITM | TLS lab tests; static check for custom trust managers |
| `SEC-02` | Origin allowlist and check of the **real** frame origin | Protect the native bridge | E2E with cross-origin iframes and redirects |
| `SEC-03` | Never execute code from a patch or a manifest | Stop XSS/RCE through updates | Fuzz + browser security suite |
| `SEC-04` | CSP and Trusted Types where supported; context-aware escaping | Safe fragment insertion | DOM XSS test corpus |
| `SEC-05` | Cache partitions per app / profile / tenant / environment | No cross-account leaks | Multi-account concurrency tests |
| `SEC-06` | `no-store`, `private` and server TTL beat acceleration | Do not cache forbidden responses | Header matrix tests |
| `SEC-07` | Protocol bounds and timeouts | Stop zip bombs, OOM, slow streams | Fuzz + chaos tests |
| `SEC-08` | No cookies, auth headers, personal data or page HTML in logs | Privacy | Log scanning in CI and on devices |
| `SEC-09` | Base revision check and atomic commit | Stop replay and out-of-order patches | Property tests |
| `SEC-10` | Safe downgrade and fallback only by policy | No silent weakening | Negotiation tests |
| `SEC-11` | OWASP MASVS test suite for WebView | Known mobile risk classes | MASTG test cases |
| `SEC-12` | Dependency audit, SBOM, signed releases | Supply chain | CI gates |
| `SEC-13` | Keys in Android Keystore / iOS Keychain | Key protection | Platform tests |
| `SEC-14` | Server: no fetch of client-supplied URLs; host allowlist | Stop SSRF | Server security tests |

## 2. Security-by-design improvements

::: info How to read "upstream point of comparison"
This column says what the audit must establish about upstream. It is **not** a claim that upstream had a vulnerability. Missing evidence in old docs is not proof of a missing control. See [Upstream audit](/research/upstream-vassonic) for findings with file references.
:::

| ID | Area | Upstream point of comparison | Rivqen design | Verification |
|---|---|---|---|---|
| `SEC-15` | Trust model | Which URLs, redirects, diffs and bridge messages are trusted by default | Explicit trust boundaries per URL/origin/frame/partition; nothing trusted implicitly | Threat model + negative test per boundary |
| `SEC-16` | WebView/JS bridge | How the calling frame and page are identified | Native frame-origin APIs; typed commands; no remote eval | E2E: cross-origin iframe, redirect, malformed message |
| `SEC-17` | Diff / HTML | Where updates become DOM and which trust checks exist | Typed diff schema; context escaping; CSP; no JS in diffs | HTML/patch fuzz + browser suite |
| `SEC-18` | Cache partition | What binds the cache to cookies, session and account | Namespace = app + environment + profile + tenant + origin + auth scope | Multi-account and logout tests |
| `SEC-19` | Cache crypto | Whether private content is protected at rest | AEAD for private entries; platform-wrapped keys | Key rotation, tamper, restore, logout tests |
| `SEC-20` | Freshness / rollback | How revision, template tag, etag and 304 work | Monotonic revision per session; base check; atomic commit | Old patch never changes the page |
| `SEC-21` | Transport | How fallback and redirects work | System TLS; auth/origin policy fixed across H3→H2→H1/WS fallback | TLS/redirect/fallback matrix; fail-closed tests |
| `SEC-22` | Rust + FFI | Bounds and lifetime checks in legacy native code | Checked lengths and ownership; minimal `unsafe`; isolated ABI | Miri/ASan where possible; FFI fuzz; cancel races |
| `SEC-23` | DoS | Limits on streams, HTML size, resources | Memory/time/CPU budgets; backpressure; nesting and decompression limits | Fuzz with big, broken, slow streams |
| `SEC-24` | Supply chain | Old dependencies and their maintenance | SBOM (SPDX/CycloneDX), lockfiles, dependency policy, signed releases, CVE monitoring | CI blocks critical policy violations |
| `SEC-25` | Privacy by default | Versions, headers and URLs sent to network and logs | No global telemetry; no SDK fingerprint header by default; PII redaction | Network capture + log privacy tests |
| `SEC-26` | Platform policy | Use of deprecated or private APIs | Public platform APIs only; transparent behavior for app review | Release compliance checklist |

## 3. Rule for every control ticket

Each `SEC-XX` ticket contains: the upstream point of comparison (with evidence link), the trust boundary, the test, and the source. A ticket that only says "Rust is safer" is rejected.

## Related

- [Threat model](/engineering/security/threat-model)
- [OWASP MASVS](https://mas.owasp.org/MASVS/)
