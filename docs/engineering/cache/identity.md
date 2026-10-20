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

## 2. URL canonicalization

1. Lowercase the scheme and the host. Apply IDNA to the host.
2. Remove the default port of the scheme (`443` for `https`, `80` for `http` in debug builds).
3. Remove the fragment (`#…`).
4. Normalize percent-encoding: decode unreserved characters, uppercase hex digits.
5. Keep **all** query parameters by default. Do not sort them unless the app declares that order is not significant.
6. Remove only parameters that the app lists in `cache.ignoredQueryParams` (for example tracking parameters). Never remove a parameter that changes content or authorization.

<Badge type="info" text="DESIGN" /> **Origin allowlist (DL-038).** The RQP origin allowlist uses the origin of steps 1 and 2: the scheme, the host in lower case and in its IDNA form, and the port only when it is not the default port. This is the ASCII serialization of the origin in the WHATWG URL Standard. The client compares it with each allowlist entry by exact match, with no wildcard and no suffix match ([negotiation §6.1](/engineering/protocol/negotiation#_6-1-where-the-client-sends-rqp-fields), rule 1). The SDK rejects a configuration with an entry that is not equal to its own serialized origin, with a clear error (H-69). It takes the origin from the URL object that its network stack used, never from a string that another parser reads again (DL-040, L-3).

<Badge type="info" text="DESIGN" /> **User information (H-68).** A user name or a password of a URL (`https://user:secret@host/`) never enters a cache key. The client uses no RQP for such a URL: it sends no RQP field and stores no snapshot ([negotiation §6.1](/engineering/protocol/negotiation#_6-1-where-the-client-sends-rqp-fields), rule 6). So no key, file name or log line holds the password (rule 1 below).

<Badge type="warning" text="RESEARCH" /> Upstream legacy derives its session id from the URL with its own rules for query parameters. legacy mode must reproduce that id for compatibility with legacy servers. See [legacy client behavior](/engineering/protocol/legacy-client). The Rivqen `cache_key` still includes the partition, so legacy mode stays isolated per account.

## 3. Rules

1. Cookies and `Authorization` values are never part of a file name, log line or metric.
2. The partition is **required** for every session. There is no shared default partition for authenticated content.
3. Public pages can use a `public` partition that is shared by all profiles. **An SDK uses `public` only when the app turns it on (H-81); it is off by default**, and then every entry goes to the partition of the request. **With the setting off, the SDK also MUST NOT read, offer (`If-None-Match`, `Rq-Have`), revalidate or show an entry of the `public` partition**, for example an entry that it stored while the setting was on, and SHOULD delete such entries (S4 security review L1, DL-052). It handles a `304` or a delta for such a request as for a request without an entry. With the setting on, an entry goes to `public` only when all of these are true (H-61): the app declares the origin public; the response did not come through a redirect, also not a redirect to the same URL (H-67); the native SDK saw all header fields of the response (H-64); the response has no `Cache-Control` directive `private`; the response has no `Set-Cookie` or `Set-Cookie2` field (H-63, DL-038); the response has an explicit `Cache-Control: public`, **or** the request had no credentials. For an RQP page the manifest also has `"mode": "public"` ([manifest and patch §1.4](/engineering/protocol/manifest-patch#_1-4-policy)). Else the entry goes to the partition of the request.
   - **Credentials (H-62).** The client treats a request as credentialed unless it knows that no cookie (also no cookie that the WebView cookie store adds), no `Authorization` field, no HTTP authentication and no client certificate was used. A `Set-Cookie` field in the response also counts as credentials. Unknown means credentialed. **A snapshot of a response with a `Set-Cookie` field never goes to the `public` partition, also with an explicit `Cache-Control: public` (H-63).** Reason: the page may set a session cookie, so the rule fails closed. It is stricter than RFC 9111 on purpose. The cost is a lost shared snapshot only.
   - **Fields not seen (H-64).** The client can apply the `Set-Cookie` rule only to a response that the native SDK fetched itself and saw in full. For any other response (for example a response that the WebView loaded, or one that page script observed: the Fetch Standard hides `Set-Cookie` and `Set-Cookie2` from script), the client acts as if the response had a `Set-Cookie` field: the entry goes to the partition of the request.
   - **Set-Cookie2 and names (DL-038).** `Set-Cookie2` counts as `Set-Cookie`. Field names match ASCII case-insensitively.
   - **Redirects (H-67).** A redirect response can set a cookie that the network stack adds to the next request, and the client sees only the final response. So an entry from a response that came through a redirect (a full document or the result of a delta) goes to the partition of the request.
   - **Delta on a `public` snapshot (H-65).** The result of a delta on a snapshot in `public` stays in `public` only when the conditions above hold for the delta response and the client knows that the request had no credentials (an explicit `Cache-Control: public` does not count for a delta, which has `no-store`). Else the result is a new entry in the partition of the request, and the entry in `public` does not change. A `304` on an entry in `public` keeps the entry as it is: it has no content. A `304` that does not meet the conditions above does not update the stored metadata of the `public` entry (its freshness), so the next navigation revalidates it again (DL-040, I-3). When the partition of the request and `public` both have an entry for the cache key, the client uses the entry of the partition of the request for the next navigation (its `If-None-Match` and `Rq-Have`); the `public` entry is used only when the partition of the request has none **and the setting is on** (DL-039, H-81, DL-052).
4. A partition change cancels all sessions in the old partition.
5. When the user or the app clears the cookies or the site data of a site, the client deletes the snapshots of that site in all partitions, also in `public` (H-60). The snapshots of a site are those of every origin whose host is the cleared domain or a subdomain of it. When all cookies or all site data are cleared, the client deletes all snapshots. A snapshot holds content and revisions that the server gave for the old cookies; it must not outlive them.

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
