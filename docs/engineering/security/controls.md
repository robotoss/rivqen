# Security controls

This register lists every security control. Each control has a test that proves it. A control is not "done" until its test passes in CI or on devices.

**Status:** <Badge type="info" text="DESIGN" /> All controls: **planned**.

[[toc]]

## 1. Baseline controls

| ID | Control | Why | Verification |
|---|---|---|---|
| `SEC-01` | TLS everywhere with system chain and hostname validation | Stop MITM | TLS lab tests; static check for custom trust managers |
| `SEC-02` | Origin allowlist and check of the **real** frame origin. The allowlist compares the serialized origin (scheme, lower-case IDNA host, no default port) by exact match, with no wildcard and no suffix match (DL-038); the SDK rejects an entry that is not its own serialized origin (H-69); the origin comes from the URL object of the network stack, never from a second parse (DL-040); no RQP for a URL with a user name or a password (H-68). The native bridge MUST NOT be reachable through URL-scheme navigation (a link, a form or a redirect to a custom scheme such as `myapp://` or `intent:`; DL-036) | Protect the native bridge | E2E with cross-origin iframes and redirects; a link and a redirect to a custom scheme do not reach the bridge; protocol fixtures `FX-RQ-C25-*` (host case, default port, suffix host, user information, backslash, IDN host); a configuration test: each invalid entry of [negotiation §6.1](/engineering/protocol/negotiation#_6-1-where-the-client-sends-rqp-fields), rule 1, stops the SDK with a clear error |
| `SEC-03` | Never execute code from a patch or a manifest: the client applies the patch content rule to every patched `html` block (no `script`, `style`, `link`, `meta`, `iframe`, `object`, `embed`, `base`, `basefont`, `bgsound`, no foreign element outside `FOREIGN_ALLOWED`, the attribute rule; [manifest and patch §4](/engineering/protocol/manifest-patch#_4-content-rules), H-48, H-77, H-78, H-83) | Stop XSS/RCE through updates | Fuzz + browser security suite; protocol fixtures of the patch content rule (WP-17 S4, T-08) |
| `SEC-04` | CSP and Trusted Types where supported; context-aware escaping | Safe fragment insertion | DOM XSS test corpus |
| `SEC-05` | Cache partitions per app / profile / tenant / environment. The shared `public` partition only under RQP-25: also only for a response that the native SDK saw in full (H-64), never with `Set-Cookie` or `Set-Cookie2` (H-63, DL-038), for the result of a delta only with known absent credentials (H-65), and never for a result that comes through a redirect (H-67) | No cross-account leaks | Multi-account concurrency tests; protocol fixtures `FX-RQ-C24-*`, `FX-RQ-C27-*`; a test with a redirect that sets a cookie |
| `SEC-06` | `no-store`, `private` and server TTL beat acceleration | Do not cache forbidden responses | Header matrix tests |
| `SEC-07` | Protocol bounds and timeouts. Byte limits of the RQP request fields before the parse (`Rq-Have` 4096, `Rq-Capabilities` 1024, `Rq-Nonce` 128 bytes) and a parse of every RQP field in time linear in its length (DL-063) | Stop zip bombs, OOM, slow streams, CPU denial of service through a field parse | Fuzz + chaos tests; timing tests of 64 KB adversarial values of every bare item type and of white space in every field (`tools/rqp-protocol/test/sf.test.mjs`, `http.test.mjs`, "M1 (DL-063)"); fixtures `FX-RQ-P16-*`, `FX-RQ-P22-*`, `FX-RQ-P42-NONCE-TOO-LONG`, `-AT-LIMIT` |
| `SEC-08` | No cookies, auth headers, personal data or page HTML in logs | Privacy | Log scanning in CI and on devices |
| `SEC-09` | `binding` check (H-107, H-108: the client computes the binding of the canonical final URL, the page revision of its snapshot, the nonce that its request sent in `Rq-Nonce` (H-109) and the next page revision R′ that it computes from its snapshot and the patch, and it equals `binding`; so a patch applies only to its exact base at its URL, as the answer to the request that sent the nonce) and atomic commit. A new nonce of 16 bytes from a CSPRNG for every network attempt of a delta request, never used again; a release build takes the nonce only from a CSPRNG, and a test input of the nonce exists only in debug or test builds (RQP-29, DL-063). A delta has `X-Content-Type-Options: nosniff` and `Vary: Rq-Nonce` (DL-063) | Stop replay, rollback and out-of-order patches; stop a patch of another base, another URL or another request | Property tests (rollback, A → B → A, other base, other URL, other nonce, stale replay; `tools/rqp-protocol/test/p1.test.mjs`); fixtures `FX-RQ-C31-*`, `FX-RQ-C32-*`, `FX-RQ-P42-*`, `FX-RQ-P43-APP-VARY-DELTA`; an SDK build test: the release artifact has no test input of the nonce |
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
| `SEC-17` | Diff / HTML | Where updates become DOM and which trust checks exist | Typed diff schema; context escaping; CSP; no JS in diffs. UTF-8 only: the full document has exactly `Content-Type: text/html; charset=utf-8`, and the client gives every document to the WebView as UTF-8 bytes; with a string API it removes a leading byte order mark first (H-62, H-66, DL-040) | HTML/patch fuzz + browser suite; protocol fixtures `FX-RQ-P40-*`, `FX-RQ-C26-*`; a WebView test that each platform adapter delivers a snapshot as UTF-8; the same test with a snapshot that starts with `EF BB BF` (`page-bom`): the head elements of the WebView are those of the snapshot, with no text before the head (DL-040) |
| `SEC-18` | Cache partition | What binds the cache to cookies, session and account | Namespace = app + environment + profile + tenant + origin + auth scope | Multi-account and logout tests |
| `SEC-19` | Cache crypto | Whether private content is protected at rest | AEAD for private entries; platform-wrapped keys | Key rotation, tamper, restore, logout tests |
| `SEC-20` | Freshness / rollback | How revision, template tag, etag and 304 work | Monotonic revision per session; `binding` check against the snapshot revision, the final URL, the nonce of the request and the computed R′ (H-107, H-108); atomic commit | Old patch never changes the page: a patch n → n+1 on the snapshot n+2 (rollback), on n+1 (stale replay), or on a snapshot whose page went A → B → A (the patch answers an earlier request, so its nonce differs) is `INTEGRITY_ERROR`. This holds only with a nonce from a CSPRNG that the client never used before, over TLS, with an honest server (manifest and patch §2.2, "Replay"). The binding gives no freshness: a server can still send old content as the answer to a new request. Correction: under H-107 alone this row did not cover A → B → A (S6 security re-review B2) |
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
