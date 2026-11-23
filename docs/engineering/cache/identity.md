# Cache identity

Cache identity decides which stored response belongs to which request. A wrong identity can show one user's page to another user. This page defines the key.

**Status:** <Badge type="info" text="DESIGN" />

## 1. Key formula

```text
partition = HMAC-SHA256(device_key, app_id ‖ environment_id ‖ profile_id ‖ tenant_id ‖ auth_scope)
cache_key = SHA-256(partition ‖ origin ‖ canonical_url ‖ method ‖ vary_values ‖ variant)
```

`‖` means length-prefixed concatenation, so that `("ab","c")` and `("a","bc")` give different inputs.

| Input | Source | Notes |
|---|---|---|
| `device_key` | Android Keystore / iOS Keychain | Random per install. Makes partition IDs useless outside the device. |
| `app_id` | Host app | Package name / bundle id. |
| `environment_id` | Host app | `prod`, `staging`… prevents mixing environments. |
| `profile_id` | Host app | A **non-secret** stable user ID. Never a token. |
| `tenant_id` | Host app | Organization or workspace, if any. |
| `auth_scope` | Host app | Changes when permissions change. |
| `origin` | URL | Scheme + host + port. |
| `canonical_url` | URL | See section 2. |
| `vary_values` | Response `Vary` | Values of the request headers named in `Vary`. `Vary: *` → do not cache. |
| `variant` | Protocol | `legacy` or `RQP`, plus language/region if the app declares them. |

### Byte layout

<Badge type="tip" text="FACT" /> `rivqen_cache::key::cache_key` gives the 32-byte `CacheKey` (DL-184). It is SHA-256 over a list of fields. Each field is `len ‖ bytes`, where `len` is the number of bytes as an unsigned 64-bit big-endian integer (8 bytes). The fields, in this order:

| # | Bytes of the field |
|---|---|
| 1 | `rivqen-cache-key-v1`: domain tag with the layout version (ASCII) |
| 2 | Partition. Scoped: `01` and the 32 bytes of the `PartitionId` (33 bytes). `public`: `02` only (1 byte) |
| 3 | Origin (UTF-8) |
| 4 | Canonical URL with its query (UTF-8) |
| 5 | `GET` |
| 6 | The number n of `Vary` entries (8 bytes, big-endian) |
| 7 … 6+2n | For each entry two fields: the name (UTF-8); then the value: `00` when the request had no such field, else `01` and the 32-byte digest |
| 7+2n | Protocol: `legacy` or `rqp` (ASCII) |
| 8+2n | Locale: `00` when the app declares none, else `01` and the locale (UTF-8) |

- The function sorts the `Vary` entries: by the bytes of the name, then an absent value first, then by the digest bytes. So the order of the entries does not change the key. It does not change the case of a name: the parser gives lower case.
- Every field has a length, and fields 2, 6 and the values start with a tag. So two different sets of parts give two different byte strings: `("ab","c")` and `("a","bc")` differ, the `public` field (1 byte) is never a scoped field (33 bytes), an absent value differs from any digest, and an absent locale differs from an empty one.
- The inputs come from other parts: the `PartitionId` from the adapter (the HMAC of the first line), the canonical URL from the URL parser of WP-08 (section 2), the `Vary` digests from the key port (HMAC-SHA256, DL-127, H-147). `Vary: *` gives no entry, so it has no key.
- Tests: `crates/rivqen-cache/tests/key_layout.rs` writes the bytes of each case by hand and compares the hash; two known answers are also checked with `shasum -a 256` outside Rust (`key_known_answers`). `key_prop.rs` checks that one changed part changes the key and that two partitions never share a key.

## 2. URL canonicalization

1. Lowercase the scheme and the host. Apply IDNA to the host.
2. Remove the default port of the scheme (`443` for `https`, `80` for `http` in debug builds).
3. Remove the fragment (`#…`).
4. Normalize percent-encoding: decode unreserved characters, uppercase hex digits. A `%` that starts no escape becomes `%25` ([DL-224](/engineering/plan/decision-log)).
5. Keep **all** query parameters by default. Do not sort them unless the app declares that order is not significant.
6. Remove only parameters that the app lists in `cache.ignoredQueryParams` (for example tracking parameters). Never remove a parameter that changes content or authorization.

<Badge type="tip" text="FACT" /> `rivqen_url::parse` (crate `rivqen-url`, on `url` 2.5.9 with `idna_adapter` 1.0.0; H-156, DL-218) reads one absolute `http` or `https` URL of at most 8 192 bytes (`MAX_URL_BYTES`; a longer text is `UrlError::TooLong` before any parse). It gives U of [manifest and patch §2.2](/engineering/protocol/manifest-patch#_2-2-fields) (`ParsedUrl::binding_url`: steps 1 to 3, the path and the query as the WHATWG parser writes them), the serialized origin and the path of [manifest and patch §1.6](/engineering/protocol/manifest-patch#_1-6-origin-and-path) (`origin`, `path`), and the canonical URL of steps 1 to 6 (`ParsedUrl::cache_url`). Step 2 removes the port 80 of `http` in every build; the rule "debug builds only" for `http` is a session rule. Tests: `crates/rivqen-url/tests/url_known.rs`, `url_interop.rs` (the 67 inputs of the [URL interop spike](/research/url-interop) against the stored WHATWG results: 52 equal, 15 listed exceptions), `url_prop.rs`; fuzz target `url_canonical`.

<Badge type="tip" text="FACT" /> **Step 1 and H-156.** The parser has no Unicode data. A host with a byte over `0x7F`, as a character or as an escape `%80` to `%FF`, is `UrlError::Host`: the session is not opened and the web view loads normally. A host in its `xn--` form passes, in lower case (`https://XN--FA-HIA.example/` gives `https://xn--fa-hia.example/`); `https://xn--example-.org/` is rejected. A scan of the input finds a host byte over `0x7F` before the WHATWG parser runs, because `idna_adapter` 1.0.0 has a debug assertion that the host `Aé.example` reaches (a panic in a test or fuzz build). Without Unicode data, the parser does not check the Unicode text of an `xn--` label; it can accept such a host where a browser rejects the URL. The host it gives is then the input host in lower case, never another host.

<Badge type="tip" text="FACT" /> **User information (H-68).** A user name, a password, or an at sign in the authority (also `https://:@a.example/`, which the WHATWG serialization writes without the at sign) is `UrlError::UserInfo`. The backslash ends the authority as in WHATWG: the host of `https://a.example\@b.example/p` is `a.example`. Errors and `Debug` hold no URL byte.

<Badge type="tip" text="FACT" /> **Steps 4 to 6 in `cache_url`.** Step 4 reads the text as escapes (`%` and two hex digits) and other characters: the escape of an unreserved character (RFC 3986 §2.3) becomes the character; every other escape gets upper-case hex digits; a `%` that starts no escape becomes `%25`. Without it, `/a%2%46b` (the text `%2F`) and `/a%2Fb` (the byte `/`) get one key. With it, two URLs get one key only when their path and query decode to the same bytes (`url_prop_cache_key_equal_only_for_equal_bytes`). Step 5 keeps the parameters in their order, also empty ones. Step 6 removes a parameter whose name (the text before its first `=`, after step 4) is equal byte for byte to a listed name; an empty listed name removes nothing. When step 6 removes all parameters, the `?` goes too, so the URL gets the key of the URL without them. The canonical URL can be 3 times as long as U (a `%` becomes `%25`), and U 3 times as long as the input (each byte over `0x7F` becomes an escape).

<Badge type="info" text="DESIGN" /> **Origin allowlist (DL-038).** The RQP origin allowlist uses the origin of steps 1 and 2: the scheme, the host in lower case and in its IDNA form, and the port only when it is not the default port. This is the ASCII serialization of the origin in the WHATWG URL Standard. The client compares it with each allowlist entry by exact match, with no wildcard and no suffix match ([negotiation §6.1](/engineering/protocol/negotiation#_6-1-where-the-client-sends-rqp-fields), rule 1). The SDK rejects a configuration with an entry that is not equal to its own serialized origin, with a clear error (H-69). It takes the origin from the URL object that its network stack used, never from a string that another parser reads again (DL-040, L-3).

<Badge type="info" text="DESIGN" /> **User information (H-68).** A user name or a password of a URL (`https://user:secret@host/`) never enters a cache key. The client uses no RQP for such a URL: it sends no RQP field and stores no snapshot ([negotiation §6.1](/engineering/protocol/negotiation#_6-1-where-the-client-sends-rqp-fields), rule 6). So no key, file name or log line holds the password (rule 1 below).

<Badge type="warning" text="RESEARCH" /> Upstream legacy derives its session id from the URL with its own rules for query parameters. legacy mode must reproduce that id for compatibility with legacy servers. See [legacy client behavior](/engineering/protocol/legacy-client). The Rivqen `cache_key` still includes the partition, so legacy mode stays isolated per account.

## 3. Rules

1. Cookies and `Authorization` values are never part of a file name, log line or metric.
2. The partition is **required** for every session. There is no shared default partition for authenticated content.
3. Public pages can use a `public` partition that is shared by all profiles. **An SDK uses `public` only when the app turns it on (H-81); it is off by default**, and then every entry goes to the partition of the request. **With the setting off, the SDK also MUST NOT read, offer (`If-None-Match`, `Rq-Have`), revalidate or show an entry of the `public` partition**, for example an entry that it stored while the setting was on, and SHOULD delete such entries (S4 security review L1, DL-053). It handles a `304` or a delta for such a request as for a request without an entry. With the setting on, an entry goes to `public` only when all of these are true (H-61): the app declares the origin public; the response did not come through a redirect, also not a redirect to the same URL (H-67); the native SDK saw all header fields of the response (H-64); the response has no `Cache-Control` directive `private`; the response has no `Set-Cookie` or `Set-Cookie2` field (H-63, DL-038); the response has an explicit `Cache-Control: public`, **or** the request had no credentials. For an RQP page the manifest also has `"mode": "public"` ([manifest and patch §1.4](/engineering/protocol/manifest-patch#_1-4-policy)). Else the entry goes to the partition of the request.
   - **Credentials (H-62).** The client treats a request as credentialed unless it knows that no cookie (also no cookie that the WebView cookie store adds), no `Authorization` field, no HTTP authentication and no client certificate was used. A `Set-Cookie` field in the response also counts as credentials. Unknown means credentialed. **A snapshot of a response with a `Set-Cookie` field never goes to the `public` partition, also with an explicit `Cache-Control: public` (H-63).** Reason: the page may set a session cookie, so the rule fails closed. It is stricter than RFC 9111 on purpose. The cost is a lost shared snapshot only.
   - **Fields not seen (H-64).** The client can apply the `Set-Cookie` rule only to a response that the native SDK fetched itself and saw in full. For any other response (for example a response that the WebView loaded, or one that page script observed: the Fetch Standard hides `Set-Cookie` and `Set-Cookie2` from script), the client acts as if the response had a `Set-Cookie` field: the entry goes to the partition of the request.
   - **Set-Cookie2 and names (DL-038).** `Set-Cookie2` counts as `Set-Cookie`. Field names match ASCII case-insensitively.
   - **Redirects (H-67).** A redirect response can set a cookie that the network stack adds to the next request, and the client sees only the final response. So an entry from a response that came through a redirect (a full document or the result of a delta) goes to the partition of the request.
   - **Delta on a `public` snapshot (H-65).** The result of a delta on a snapshot in `public` stays in `public` only when the conditions above hold for the delta response and the client knows that the request had no credentials (an explicit `Cache-Control: public` does not count for a delta, which has `no-store`). Else the result is a new entry in the partition of the request, and the entry in `public` does not change. A `304` on an entry in `public` keeps the entry as it is: it has no content. A `304` that does not meet the conditions above does not update the stored metadata of the `public` entry (its freshness), so the next navigation revalidates it again (DL-040, I-3). When the partition of the request and `public` both have an entry for the cache key, the client uses the entry of the partition of the request for the next navigation (its `If-None-Match` and `Rq-Have`); the `public` entry is used only when the partition of the request has none **and the setting is on** (DL-039, H-81, DL-053).
4. A partition change cancels all sessions in the old partition.
5. When the user or the app clears the cookies or the site data of a site, the client deletes the snapshots of that site in all partitions, also in `public` (H-60). The snapshots of a site are those of every origin whose host is the cleared domain or a subdomain of it. When all cookies or all site data are cleared, the client deletes all snapshots. A snapshot holds content and revisions that the server gave for the old cookies; it must not outlive them.

<Badge type="tip" text="FACT" /> `rivqen_cache::partition::store_partition`, `delta_partition` and `readable` implement rules 2 and 3; `site_matches` implements the host match of rule 5. For a delta, the manifest condition is the policy of the stored snapshot ([DL-196](/engineering/plan/decision-log)). Tests: `crates/rivqen-cache/tests/partition_rules.rs` (one row for each row of section 4 that names a partition) and `partition_prop.rs`.

<Badge type="tip" text="FACT" /> `rivqen_cache::partition::valid_host` accepts only a lower-case ASCII host name (labels of `a`-`z`, `0`-`9`, `-`, 1 to 63 bytes, at most 253 bytes, no empty label, no port, no trailing dot; a dotted IPv4 literal has this form) or an IPv6 literal in brackets in the text form of RFC 5952 §4: lower-case digits, no leading zero in a group, the first longest run of two or more zero groups written as `::`, and no `::` for one group. An embedded IPv4 form and a zone are not valid. A driver refuses a `Replace` with any other host (`Refused::InvalidHost`, DL-205, DL-208, DL-213). `site_matches` compares bytes, so a second text of one address could escape a site clear (rule 5). Tests: `partition_valid_host_*` in `partition_rules.rs` and `partition_prop_valid_host_*` in `partition_prop.rs`.

## 4. Verification

| Test | Expected result |
|---|---|
| Two profiles, same URL, concurrent loads | Two entries, no shared bytes |
| Logout during a revalidation | Revalidation result is discarded; old entry is unreadable |
| Query parameter order change | Different keys (default) / same key (if declared not significant) |
| `Vary: Accept-Language` | Separate entries per language |
| The app turned the `public` partition off; an entry of `public` from a time when the setting was on, and a `304` or a valid delta | The entry is not used: no `If-None-Match`, no `Rq-Have`; the `304` gives a normal load; the delta gives `PATCH_INVALID` (`FX-RQ-C27-PUBLIC-SNAPSHOT-NOT-ENABLED-304`, `-DELTA`; H-81, L1). The deletion of the entry: spec only, the protocol fixtures do not model it |
| The app did not turn the `public` partition on; `"mode": "public"`, public origin, `Cache-Control: public` | Entry in the partition of the request (`FX-RQ-C24-PUBLIC-NOT-ENABLED`, H-81). The rows below have the setting on |
| `"mode": "public"`, public origin, request with cookies, no `Cache-Control: public` | Entry in the partition of the request (protocol fixture `FX-RQ-C24-PUBLIC-NO-DIRECTIVE`) |
| `"mode": "public"`, public origin, no `Cache-Control: public`, the client cannot tell whether credentials were sent | Entry in the partition of the request (`FX-RQ-C24-PUBLIC-CREDENTIALS-UNKNOWN`, H-62) |
| `"mode": "public"`, public origin, no `Cache-Control: public`, a request without credentials and a `Set-Cookie` in the response | Entry in the partition of the request (`FX-RQ-C24-PUBLIC-SET-COOKIE`, H-62) |
| `"mode": "public"`, public origin, `Cache-Control: public` and a `Set-Cookie` in the response | Entry in the partition of the request (`FX-RQ-C24-PUBLIC-EXPLICIT-SET-COOKIE`, H-63) |
| `"mode": "public"`, public origin, `Cache-Control: public`, no credentials, but the native SDK did not see all response fields | Entry in the partition of the request (`FX-RQ-C24-PUBLIC-FIELDS-HIDDEN`, H-64) |
| `"mode": "public"`, public origin, `Cache-Control: public` and a `Set-Cookie2` in the response | Entry in the partition of the request (`FX-RQ-C24-PUBLIC-SET-COOKIE2`, DL-038) |
| A valid delta on an entry in `public`: unknown credentials; a `Set-Cookie`; credentials with `Cache-Control: public` on the delta; no credentials, all fields seen, no `Set-Cookie` | New entry in the partition of the request, the `public` entry unchanged (three cases); the `public` entry is replaced (`FX-RQ-C27-*`, H-65) |
| A valid delta on an entry in `public`, no credentials: the native SDK did not see all fields; the origin is no longer declared public; `Cache-Control: no-store, private`; a `Set-Cookie2` | New entry in the partition of the request, the `public` entry unchanged (`FX-RQ-C27-PUBLIC-SNAPSHOT-DELTA-FIELDS-HIDDEN`, `-NOT-PUBLIC-ORIGIN`, `-PRIVATE`, `-SET-COOKIE2`; DL-040, I-2) |
| `"mode": "public"`, public origin, no credentials, `Cache-Control: public`, but the response came through a redirect (to another URL, or to the same URL) | Entry in the partition of the request (`FX-RQ-C24-PUBLIC-REDIRECT`, `FX-RQ-C24-PUBLIC-REDIRECT-SAME-URL`, H-67) |
| A valid delta on an entry in `public`, no credentials, after a redirect to the same URL or to the same cache key | New entry in the partition of the request (`FX-RQ-C27-PUBLIC-SNAPSHOT-DELTA-REDIRECT`, `-REDIRECT-HOST-CASE`, H-67) |
| A `304` on an entry in `public` | The entry stays in `public`, unchanged (`FX-RQ-C27-PUBLIC-SNAPSHOT-304`, H-65) |
| A `304` on an entry in `public` that does not meet RQP-25 (for example with a `Set-Cookie`) | The stored freshness of the entry does not change (DL-040, I-3). Spec only: the protocol fixtures do not model freshness |
| Entries for one cache key in the partition of the request and in `public` | The next navigation uses the entry of the partition of the request (its `If-None-Match` and `Rq-Have`); the `public` entry only when the partition of the request has none and the setting is on (DL-039, H-81). Spec only: the reference client has one context snapshot, so no protocol fixture checks the choice; an SDK test must |
| Allowlist `https://shop.example.com`; final URL with an upper-case host or the port `443`; final URL `https://shop.example.com.attacker.net/…` | Same origin, RQP is used; another origin, normal page (`FX-RQ-C25-FINAL-*`, DL-038) |
| Request URL or final URL with a user name and a password, origin in the allowlist | No RQP field, no snapshot, normal page (`FX-RQ-C25-USERINFO-REQUEST`, `FX-RQ-C25-USERINFO-FINAL`, `-FINAL-304`, `-FINAL-DELTA`, H-68) |
| Final URL `https://attacker.example\@shop.example.com/…` and `https://shop.example.com\@attacker.example/…`; request URL `https://faß.example/…` with the allowlist `https://xn--fa-hia.example` | The WHATWG hosts `attacker.example` (normal page), `shop.example.com` (stored) and `xn--fa-hia.example` (stored) (`FX-RQ-C25-FINAL-BACKSLASH-*`, `FX-RQ-C25-IDN-HOST`, DL-040, L-3) |
| An allowlist entry that is not its own serialized origin (`https://Shop.example.com`, `https://shop.example.com/`, `*`) | The SDK does not start; the error names the entry by its index and a reason (for example `allowlist[2]: user information`), never by its raw text, which can hold a password (H-69, DL-042; errors and logging) The fixture loader rejects such a case |
| The user clears the site data of one site | No snapshot of that site is left; snapshots of other sites stay |

## Related

- [Freshness and invalidation](/engineering/cache/policy)
- [Security controls SEC-05, SEC-18](/engineering/security/controls)
