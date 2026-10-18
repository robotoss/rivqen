# Freshness and invalidation

This page defines when a cached page is fresh, when Rivqen may show a stale page, and what removes an entry.

**Status:** <Badge type="info" text="DESIGN" />

[[toc]]

## 1. Standards

| Standard | Use in Rivqen |
|---|---|
| [RFC 9111](https://www.rfc-editor.org/rfc/rfc9111) HTTP Caching <Badge type="tip" text="FACT" /> | Base rules: `Cache-Control`, `Expires`, `Age`, validators, `Vary`, `no-store`, `private` |
| [RFC 9110](https://www.rfc-editor.org/rfc/rfc9110) HTTP Semantics | Conditional requests: `If-None-Match`, `304 Not Modified` |
| [RFC 5861](https://www.rfc-editor.org/rfc/rfc5861) | `stale-while-revalidate`, `stale-if-error` |
| Legacy protocol `cache-offline` | Legacy control, **legacy mode only** |

## 2. Decision order

The core applies these rules in order. The first rule that matches decides.

| # | Condition | Decision |
|---|---|---|
| 1 | Response or request has `no-store` | Do not store. Do not serve from cache. |
| 2 | Entry is in another partition | Not visible. |
| 3 | Entry failed integrity | Delete; fetch. |
| 4 | Entry is fresh (RFC 9111 freshness lifetime) | Show; revalidation not needed. |
| 5 | Entry is stale, within `stale-while-revalidate` | Show; revalidate in background. |
| 6 | Network error, within `stale-if-error` | Show with "stale" state to the app. |
| 7 | legacy mode and `cache-offline` rule applies | Follow the [legacy rule](/engineering/protocol/legacy-wire#cache-offline). |
| 8 | Otherwise | Revalidate before showing (conditional request). |

The manifest policy of an RQP page ([manifest and patch §1.4](/engineering/protocol/manifest-patch#_1-4-policy)) never allows what these rules or the app policy deny (H-61). `ttl_seconds` only shortens the freshness lifetime of rule 4. `offline: true` has an effect only inside rule 6 (`stale-if-error`). `"mode": "public"` selects the `public` partition only under the conditions of [cache identity](/engineering/cache/identity) rule 3, also for the result of a delta on a `public` entry (H-65). A result that comes through a redirect never goes to `public` (H-67). A `304` (rule 8) does not change the partition of an entry, and a `304` that does not meet the conditions of `public` (RQP-25) does not update the stored freshness of a `public` entry (DL-040).

## 3. Sensitive content

1. The app can mark routes as **sensitive**. Sensitive routes never show stale content without a visible stale state.
2. `TLS_ERROR` never falls back to stale sensitive content.
3. Authenticated pages follow `R-CACHE-03`: off by default.

## 4. Invalidation triggers

| Trigger | Scope |
|---|---|
| Logout, account switch, tenant change | Whole partition (keys revoked) |
| The user or the app clears cookies or site data of a site (H-60) | All snapshots of that site, in every partition; all snapshots when all cookies or all site data are cleared |
| Language or region change (if part of the variant) | Entries with the old variant |
| App update with a new cache schema | All entries with old `schema_version` |
| Server purge signal (RQP) | Listed keys or path prefixes, after authentication |
| Push invalidation (RQP) | Listed pages → revalidate, not replace |
| `template-change: true` / new template revision | That page |
| Integrity failure | That entry |
| Quota exceeded | LRU entries in the same partition first |

## 5. Stampede control

1. Identical requests in the same partition share one network request.
2. Revalidation of one key runs once at a time.
3. Prefetch never runs on metered networks when data saver is on.

## Related

- [Cache layers](/engineering/cache/)
- [Legacy protocol wire contract](/engineering/protocol/legacy-wire)
