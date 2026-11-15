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

### 1.1 Fields that the cache reads {#fields}

<Badge type="tip" text="FACT" /> The parsers are in `rivqen-cache` (`header` module, WP-07 S1 T-02; DL-182). The tests `header_*` of `crates/rivqen-cache/tests/` and the fuzz targets `cache_control` and `http_date` check them. A parser never fails: a value that is not valid gives the stricter result. Field names match ASCII case-insensitively, and every field line of the name counts (in order).

| Field | Function | What it reads |
|---|---|---|
| `Cache-Control` | `parse_cache_control` | The directives below |
| `Expires`, `Date` | `parse_http_date` | One HTTP-date |
| `Age` | `parse_age` | One delta-seconds |
| `Vary` | `parse_vary` | Field names or `*` |
| `cache-offline` (legacy) | `parse_cache_offline` | One of four values |

**`Cache-Control` directives.** The list splits at commas outside quoted strings. An argument is a token or a quoted-string (RFC 9111 §5.2).

| Directive | Result |
|---|---|
| `no-store` | Counts with or without an argument (DL-088 item 3). Also counts when the split of DL-088 item 5 finds it: at every `,` and `;`, quotes not parsed, the name before `=` without spaces, tabs and `"` at its ends. So `private="a, no-store"` is `no-store` |
| `no-cache` | Counts with or without field names (the qualified form counts as the plain form) |
| `private` | Counts with or without field names |
| `must-revalidate` | Counts, also with an argument |
| `public` | Counts only without an argument |
| `max-age`, `stale-while-revalidate`, `stale-if-error` | delta-seconds, as a token or a quoted-string; a value over 2^31 is 2^31 (RFC 9111 §1.2.2) |
| `s-maxage` | Ignored: the cache is a private cache (RFC 9111 §4.2.1) |
| `must-understand` | Ignored, so `no-store` always wins |
| Any other directive | Ignored (RFC 9111 §5.2.3) |

**Values that are not valid** (each gives the stricter result):

| Input | Result |
|---|---|
| `max-age`, `stale-while-revalidate` or `stale-if-error` without a value, with a value that is not digits, with a quoted-pair, or twice with two different values | No number for that directive, and `invalid_freshness`: the response is stale (RFC 9111 §4.2.1). The same value twice counts once |
| A member that does not match the grammar (a name that is not a token, white space around `=`, an unclosed quoted-string, text after the closing quote) | `invalid_freshness`, `public` does not count, and the DL-088 split of that line also counts `no-cache`, `private` and `must-revalidate` |
| An HTTP-date that does not match one of the three forms of RFC 9110 §5.6.7 exactly; a zone other than `GMT`; a day that the month does not have; a time out of range; a year before 1970 | No date |
| `Age` with a first member that is not delta-seconds | No age (RFC 9111 §5.1) |
| A `Vary` member that is not a token | `Vary: *`: the response is not stored |
| A `cache-offline` value other than `true`, `false`, `store` or `http` after ASCII lower case, or no field | `false` (CL-15) |

**Details.**

- HTTP-date: letters match ASCII case-insensitively (RFC 9111 §4.2), also `GMT`. The day name must be one of the seven names; it is not checked against the date. `23:59:60` is `23:59:59`, the nearest second that is not later (RFC 9111 §4.2). The RFC 850 form has a two-digit year: it is the latest year with those digits that is not more than 50 years after 2026-01-01T00:00:00Z (RFC 9110 §5.6.7; the function has no current time).
- `Age`: the first non-empty member of the field lines in order. A value too large for 64 bits is the largest 64-bit value: a larger age is the stricter result.
- `Vary`: `*` in any member gives "any". Else the names in ASCII lower case, sorted, each name once. No field, or only empty members, gives "none". The legacy request fields of CL-12 (`accept-diff`, `if-none-match`, `template-tag`) are removed by the key code, not by the parser.
- `cache-offline`: the value is not trimmed; the field line value has no white space at its ends.
- `Pragma` is not read.
- Work: each parser reads each byte a fixed number of times and does not go back. Only `parse_vary` allocates, at most the size of the `Vary` values. The adapter limits the response head first (`maxHeaderBytes`). A test reads 64 KiB of commas, quotes and backslashes in each parser in less than 50 ms (release build).

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

The manifest policy of an RQP page ([manifest and patch §1.4](/engineering/protocol/manifest-patch#_1-4-policy)) never allows what these rules or the app policy deny (H-61). `ttl_seconds` only shortens the freshness lifetime of rule 4. `offline: true` has an effect only inside rule 6 (`stale-if-error`). `"mode": "public"` selects the `public` partition only under the conditions of [cache identity](/engineering/cache/identity) rule 3, also for the result of a delta on a `public` entry (H-65), and only when the app turned the `public` partition on (H-81; off by default). A result that comes through a redirect never goes to `public` (H-67). A `304` (rule 8) does not change the partition of an entry, and a `304` that does not meet the conditions of `public` (RQP-25) does not update the stored freshness of a `public` entry (DL-040).

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
