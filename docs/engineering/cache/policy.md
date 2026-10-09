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

## 3. Sensitive content

1. The app can mark routes as **sensitive**. Sensitive routes never show stale content without a visible stale state.
2. `TLS_ERROR` never falls back to stale sensitive content.
3. Authenticated pages follow `R-CACHE-03`: off by default.

## 4. Invalidation triggers

| Trigger | Scope |
|---|---|
| Logout, account switch, tenant change | Whole partition (keys revoked) |
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
