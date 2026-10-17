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
2. Remove the default port (`443` for `https`).
3. Remove the fragment (`#…`).
4. Normalize percent-encoding: decode unreserved characters, uppercase hex digits.
5. Keep **all** query parameters by default. Do not sort them unless the app declares that order is not significant.
6. Remove only parameters that the app lists in `cache.ignoredQueryParams` (for example tracking parameters). Never remove a parameter that changes content or authorization.

<Badge type="warning" text="RESEARCH" /> Upstream legacy derives its session id from the URL with its own rules for query parameters. legacy mode must reproduce that id for compatibility with legacy servers. See [legacy client behavior](/engineering/protocol/legacy-client). The Rivqen `cache_key` still includes the partition, so legacy mode stays isolated per account.

## 3. Rules

1. Cookies and `Authorization` values are never part of a file name, log line or metric.
2. The partition is **required** for every session. There is no shared default partition for authenticated content.
3. Public pages can use a `public` partition that is shared by all profiles, only when all of these are true (H-61): the app declares the origin public; the response has no `Cache-Control` directive `private`; the response has an explicit `Cache-Control: public`, **or** the request had no credentials. For an RQP page the manifest also has `"mode": "public"` ([manifest and patch §1.4](/engineering/protocol/manifest-patch#_1-4-policy)). Else the entry goes to the partition of the request.
   - **Credentials (H-62).** The client treats a request as credentialed unless it knows that no cookie (also no cookie that the WebView cookie store adds), no `Authorization` field, no HTTP authentication and no client certificate was used. A `Set-Cookie` field in the response also counts as credentials. Unknown means credentialed.
4. A partition change cancels all sessions in the old partition.
5. When the user or the app clears the cookies or the site data of a site, the client deletes the snapshots of that site in all partitions, also in `public` (H-60). The snapshots of a site are those of every origin whose host is the cleared domain or a subdomain of it. When all cookies or all site data are cleared, the client deletes all snapshots. A snapshot holds content and revisions that the server gave for the old cookies; it must not outlive them.

## 4. Verification

| Test | Expected result |
|---|---|
| Two profiles, same URL, concurrent loads | Two entries, no shared bytes |
| Logout during a revalidation | Revalidation result is discarded; old entry is unreadable |
| Query parameter order change | Different keys (default) / same key (if declared not significant) |
| `Vary: Accept-Language` | Separate entries per language |
| `"mode": "public"`, public origin, request with cookies, no `Cache-Control: public` | Entry in the partition of the request (protocol fixture `FX-RQ-C24-PUBLIC-NO-DIRECTIVE`) |
| `"mode": "public"`, public origin, no `Cache-Control: public`, the client cannot tell whether credentials were sent | Entry in the partition of the request (`FX-RQ-C24-PUBLIC-CREDENTIALS-UNKNOWN`, H-62) |
| `"mode": "public"`, public origin, no `Cache-Control: public`, a request without credentials and a `Set-Cookie` in the response | Entry in the partition of the request (`FX-RQ-C24-PUBLIC-SET-COOKIE`, H-62) |
| The user clears the site data of one site | No snapshot of that site is left; snapshots of other sites stay |

## Related

- [Freshness and invalidation](/engineering/cache/policy)
- [Security controls SEC-05, SEC-18](/engineering/security/controls)
