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
3. Public pages can use a `public` partition that is shared by all profiles, only if the app declares the origin public.
4. A partition change cancels all sessions in the old partition.

## 4. Verification

| Test | Expected result |
|---|---|
| Two profiles, same URL, concurrent loads | Two entries, no shared bytes |
| Logout during a revalidation | Revalidation result is discarded; old entry is unreadable |
| Query parameter order change | Different keys (default) / same key (if declared not significant) |
| `Vary: Accept-Language` | Separate entries per language |

## Related

- [Freshness and invalidation](/engineering/cache/policy)
- [Security controls SEC-05, SEC-18](/engineering/security/controls)
