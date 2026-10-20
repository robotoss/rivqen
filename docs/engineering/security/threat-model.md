# Threat model

This page lists what Rivqen protects, who can attack it, where the trust boundaries are, and which threats the design must stop.

**Status:** <Badge type="info" text="DESIGN" /> Owner: security engineer (WP-18). Review at each phase gate.

[[toc]]

## 1. Assets

| Asset | Why it matters |
|---|---|
| User session (cookies, tokens) | Account takeover |
| Cached pages of a user | Private data disclosure |
| Page integrity (what the user sees) | Fraud, phishing, wrong prices or balances |
| Native capabilities exposed by the JS bridge | Code execution or data access from web content |
| Server origin (for server SDKs) | SSRF, cache poisoning of shared caches |
| Device resources (memory, CPU, battery, data) | Denial of service; user cost |
| Release artifacts | Supply-chain compromise |

## 2. Attackers

| Attacker | Capabilities |
|---|---|
| Network attacker | Observes and modifies traffic on untrusted networks; cannot break TLS |
| Malicious web content | Controls an iframe, a third-party script, a redirect target, or an HTML fragment |
| Malicious or compromised upstream | Returns crafted HTML, headers, diffs or push messages |
| Second user on the same device | Logs in after the first user on a shared device |
| Local attacker with file access | Reads app files on a rooted/jailbroken or backed-up device |
| Supply-chain attacker | Compromises a dependency or a build step |

## 3. Trust boundaries

```mermaid
flowchart TD
  subgraph Device
    App[Host app] --- SDK[Rivqen SDK + core]
    SDK --- Store[(Private cache)]
    SDK --- WV[WebView]
    WV --- Frames[Page and iframes]
  end
  SDK -->|TLS| Net[Network]
  Net --> Srv[Server SDK]
  Srv --> Origin[Origin app]
  Frames -.->|B1 bridge| SDK
  Net -.->|B2 network| SDK
  Store -.->|B3 storage| SDK
  Srv -.->|B4 request| Origin
```

| Boundary | Untrusted side | Main checks |
|---|---|---|
| B1 JS bridge | Web content in any frame | Real frame origin (from the platform API), message schema, allowlist of commands |
| B2 Network | All responses and push messages | TLS (system validation), size limits, schema, revision, integrity |
| B3 Storage | Files on disk | Integrity on read, AEAD for private entries, partition check |
| B4 Server request | Client requests to the server SDK | No fetch of client-supplied URLs; tenant context; header injection checks |
| B5 Partition | Another account on the same device | Partition in every key; revoke on logout |

## 4. Threats and mitigations

| # | Threat | Boundary | Mitigation (controls) |
|---|---|---|---|
| T-01 | XSS through a data block or patch | B2 | The client patch content rule is the only gate (H-78: these elements are valid markup in a full document): no `script`, `style`, `link`, `meta`, `iframe`, `object`, `embed`, `base`, `basefont` or `bgsound` element, no SVG or MathML element outside `FOREIGN_ALLOWED`, no inline event handlers and no attribute value that the URL parser reads as a `javascript:`, `vbscript:` or `data:` URL after character reference decoding (H-77), except a `data:` image whose type is in an allow-list of raster types, in `img`, `source` and `video` (H-83, H-83a, DL-052), in patched content, also in a foreign region (H-59, DL-036, H-73); the server SHOULD send the full document instead. Not covered by this rule: later URLs in `ping`, `url()` in `style` and in SVG presentation attributes, app-specific schemes, DOM clobbering and SVG definitions with an `id` that the page uses (server SHOULD use unique ids, H-76), `use` copies, render cost, CSS overlays, full documents ([manifest and patch §4](/engineering/protocol/manifest-patch#_4-content-rules)). Context-aware insertion; CSP for script gadgets and CSS (SEC-03, SEC-04, SEC-17); no bridge through URL-scheme navigation (SEC-02) |
| T-02 | Bridge abuse from a third-party iframe | B1 | Frame-origin check by native API; command allowlist (SEC-02, SEC-16) |
| T-03 | Cross-account cache read | B5 | Partitioned keys, revoke on logout (SEC-05, SEC-18). The shared `public` partition only for a response that the native SDK saw in full, with no `Set-Cookie` or `Set-Cookie2` and known absent credentials or `Cache-Control: public` (H-61 to H-64, DL-038); the result of a delta on a `public` snapshot stays public only with known absent credentials (H-65); a result that comes through a redirect never goes to `public`, because a redirect can set a cookie that the next request carries (H-67) |
| T-04 | Replay of an old patch | B2 | HTTP: `binding` check (H-107, H-108): the client computes the binding of the canonical final URL, the page revision of its snapshot, the nonce that its request sent and the next page revision R′, so a replayed patch applies only as the answer to the request that sent its nonce: a rollback (n → n+1 on n+2, and A → B → A), a stale replay on its own target, a patch of another base or URL, and a patch for another request with the same base and URL give `INTEGRITY_ERROR`. Assumptions: a nonce from a CSPRNG that is never used again, TLS, an honest server (manifest and patch §2.2, "Replay"). `Cache-Control: no-store` on every delta, navigation generation (SEC-09, SEC-20); no sequence over HTTP (H-9). Push channels (WP-16): sequence. Correction: under H-104 alone (2026-10-20, before H-107) this row said that a replayed patch passes only when its result is a page revision of the server; the S6 security review (F1) showed that this allowed a rollback and stale pinning. Under H-107 alone this row said that every rollback gives `INTEGRITY_ERROR`; the S6 security re-review (B2) showed that A → B → A (the same base again) was accepted; H-108 closes it |
| T-05 | Cache poisoning via shared CDN | B4 | `Cache-Control: private`, `Vary`, no 304 on identity mismatch (SEC-06). A delta has `no-store`; for a cache that ignores it, the delta `binding` (H-107, H-108) stops a delta for any other request: another user's base, another URL, and also another request with the same base and URL (a later visit, A → B → A, or two users with the same `public` snapshot, where a user can choose the base). A stored delta has no value for a cache. Remaining risk: none from a stored delta while the nonce comes from a CSPRNG and is never used again; a TLS-terminating intermediary that can read the request and write the response can build any delta (the binding is not a MAC; manifest and patch §2.2, "Replay"). Correction: under H-107 alone this row named a delta for another request with the same base and URL as the remaining risk, and H-65 was named as a defense for it; H-65 is not one (S6 security re-review B1) |
| T-06 | Decompression bomb / huge document | B2 | Size and ratio limits (SEC-07, SEC-23) |
| T-07 | Slow-loris stream | B2 | Deadlines and backpressure (SEC-07) |
| T-08 | TLS bypass "for speed" | B2 | System validation only; no custom trust (SEC-01, SEC-21) |
| T-09 | Downgrade to weaker legacy behavior | B2 | Downgrade only by app policy (SEC-10) |
| T-10 | Secrets in logs | all | Redaction; log privacy tests (SEC-08, SEC-25) |
| T-11 | Read of cache files from backup or rooted device | B3 | AEAD + Keystore/Keychain keys; no backup (SEC-13, SEC-19) |
| T-12 | SSRF through server middleware | B4 | Never fetch URLs from the request (SEC-14) |
| T-13 | Memory corruption in FFI | B1/B2 | Safe Rust, minimal `unsafe`, fuzzing, Miri (SEC-22) |
| T-14 | Dependency compromise | build | Lockfiles, SBOM, `cargo-deny`, signed releases (SEC-12, SEC-24) |
| T-15 | Stale private page shown after TLS error | B2 | No sensitive stale on `TLS_ERROR` (SEC-06) |
| T-16 | Encoding confusion: the WebView decodes a full document or a snapshot with another encoding than UTF-8 and sees other blocks or another manifest position; or a string API keeps a byte order mark as U+FEFF, which moves the head into the body | B2 | UTF-8 only (H-62); every full document has exactly `Content-Type: text/html; charset=utf-8` (H-66); the client gives every document to the WebView as UTF-8 bytes (H-66), and removes a leading byte order mark before a string API (DL-040) ([negotiation §4, §6.3](/engineering/protocol/negotiation#_6-3-response); SEC-17) |
| T-17 | Allowlist bypass by a look-alike origin (`shop.example.com.attacker.net`, a subdomain) after an open redirect, or by a URL that two parsers read in two ways (`https://attacker.example\@shop.example.com/`) | B2 | Exact match of the serialized origin, no wildcard, no suffix match (DL-038); the client uses a response only from an allowlisted final request (H-62); the origin comes from the URL object of the network stack (DL-040); an invalid allowlist entry stops the SDK (H-69) (SEC-02) |
| T-18 | A password in a URL (`https://user:secret@host/`) passes the allowlist and goes into a cache key, a file name or a log line | B3 | No RQP and no snapshot for a URL with a user name or a password (H-68); cookies and credentials never in keys or logs ([cache identity](/engineering/cache/identity) rule 1) (SEC-02, SEC-08) |

## 5. Out of scope

- A fully compromised device (root/jailbreak with code injection) can read process memory. Rivqen limits secret lifetime but cannot stop this.
- Bugs in the WebView engine itself.
- Malicious host apps.

## Related

- [Security controls](/engineering/security/controls)
- [Research: WebView security literature](/research/literature#webview-and-hybrid-app-security)
