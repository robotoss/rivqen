# Legacy protocol: wire contract

::: warning Legacy mode — temporary
This page describes the **legacy mode** (compatibility with the VasSonic legacy protocol). It ships as an optional module, is **deprecated from 1.0** and is **removed in 1.5**. New integrations use the [Rivqen protocol](/engineering/protocol/markup). See [Modes and legacy deprecation](/engineering/protocol/versioning).
:::

This page specifies the legacy protocol as observed in the upstream implementations. Rivqen clients and servers MUST follow it in legacy protocol mode. Where upstream implementations disagree, this page states the **canonical** behavior and links to [Divergences](/engineering/protocol/legacy-divergences).

**Status:** <Badge type="tip" text="FACT" /> derived from upstream code at `59936bef` and **confirmed for the three servers by captured HTTP traces** (2026-10-09, 115 checks; see [server trace report](/engineering/protocol/legacy-traces)). Client-side items still need Android traces (WP-01).

Evidence prefix: `upstream:` = `Tencent/VasSonic@59936beff656d4b5718ff6444d6c5e001a2c5231`.

[[toc]]

## 1. Overview

```mermaid
sequenceDiagram
  participant C as Client
  participant S as Server
  C->>S: GET + accept-diff, If-None-Match, template-tag
  S->>S: render HTML, SHA-1 of HTML
  alt SHA-1 equals If-None-Match
    S-->>C: 304, cache-offline: store
  else template SHA-1 equals template-tag
    S-->>C: 200, JSON data, template-change: false
  else
    S-->>C: 200, full HTML, template-change: true
  end
```

## 2. Request headers (client → server)

| Header | Value | When sent | Evidence |
|---|---|---|---|
| `accept-diff` | `true` or `false` | Always. `true` when the session accepts data updates (default). | `upstream:sonic-android/sdk/src/main/java/com/tencent/sonic/sdk/SonicSessionConnection.java:347` |
| `If-None-Match` | Cached `etag` value (lowercase hex SHA-1, **unquoted**) | When the client has a cache entry | `…/SonicSessionConnection.java:350-352` |
| `template-tag` | Cached template SHA-1 (lowercase hex) | When the client has a cache entry | `…/SonicSessionConnection.java:354-356` |
| `sonic-sdk-version` | `Sonic/<version>` (for example `Sonic/2.0.0`) | Always (Android, iOS) | `…/SonicSessionConnection.java:361`; `upstream:sonic-iOS/Sonic/SonicConstants.h:127-132` |
| `Cookie` | From the WebView cookie store | When cookies exist | `…/SonicSessionConnection.java:370-372` |
| `User-Agent` | The WebView user agent | Always | `…/SonicSessionConnection.java:377` |
| `Accept-Encoding` | `gzip` | Always (Android) | `…/SonicSessionConnection.java:359` |
| `Host` + `sonic-dns-prefetch` | Original host | Only for IP-direct connections (optional feature) | `…/SonicSessionConnection.java:301-305` |

::: info Rivqen deviation (privacy)
`sonic-sdk-version` identifies the SDK on the network. In Rivqen, this header is **off by default** (`identity.sdkHeader = false`) because no upstream server reads it. The compatibility profile `legacy-strict-headers` sends it. <Badge type="info" text="DESIGN" />
:::

## 3. Server detection of a legacy request

| Implementation | Rule | Evidence |
|---|---|---|
| Java | `accept-diff == "true"` enables only the `304` check and the `Cache-Control`/`Cache-Offline` headers. The **data-only response is chosen by the `template-tag` request header alone**, even with `accept-diff: false` (trace S08). | `upstream:sonic-java/src/main/java/com/github/tencent/SonicFilter.java:69-77, 105-144` |
| PHP | Header `accept-diff` is identical to `"true"`. The header **name** lookup is case-sensitive: `Accept-Diff: true` is not detected on the PHP built-in server (trace S10). | `upstream:sonic-php/sdk/sonic.php:59` |
| Node.js | Header `accept-diff` is present and non-empty (any value, also `false`) **and** the client accepts `gzip` or `deflate`; with `Accept-Encoding: identity` the middleware skips legacy processing (traces S08, S09, S11). | `upstream:sonic-nodejs/middleware/compress.js:26-28, 78` |

**Canonical (MUST):** a server treats a request as a legacy request only when `accept-diff` equals `true` (case-insensitive header name, exact lowercase value).

## 4. Hashes

| Value | Algorithm | Input | Encoding | Evidence |
|---|---|---|---|---|
| `etag` (a.k.a. `html-sha1`) | SHA-1 | The full HTML response body **as rendered by the origin**, UTF-8 bytes, before compression | Lowercase hex, 40 characters, no quotes | Java `SonicFilter.java:92-93` + `SonicUtil.java:21-54`; Node `common/diff.js:15`; PHP `sdk/sonic.php:73` |
| `template-tag` | SHA-1 | The template string (section 5 of [markers grammar](/engineering/protocol/legacy-markers)), UTF-8 bytes | Lowercase hex | Java `SonicFilter.java:119`; Node `common/diff.js:66`; PHP `sdk/sonic.php:109` |

::: warning SHA-1 is not a security mechanism
SHA-1 is used only as a version identifier. Rivqen never uses these values for authentication or integrity against attackers. See [Cryptography](/engineering/security/crypto).
:::

## 5. Response cases

### 5.1 Not modified

Condition: the request is a legacy request, `If-None-Match` is present, and it equals the SHA-1 of the rendered HTML.

| Field | Value |
|---|---|
| Status | `304 Not Modified` |
| `Cache-Offline` | `store` |
| `Content-Length` | `0` |
| Body | none |

Evidence: Java `SonicFilter.java:96-101`; Node `common/diff.js:26-33`; PHP `sdk/sonic.php:75-80`. Comparison is case-insensitive in Java and case-sensitive in Node.js and PHP (traces S04). A quoted value (`"<sha1>"`) is never recognized (trace S05).

<Badge type="tip" text="FACT" /> **Trace finding:** the Java filter sends **two** `Cache-Offline` header fields on `304` — `true` (added for every legacy request) and `store` (trace S03). A client that joins repeated fields sees `true, store`.

### 5.2 Data update (template unchanged)

Condition: not 304, and the SHA-1 of the computed template equals the request `template-tag`.

| Field | Value |
|---|---|
| Status | `200` |
| `etag` (header name may be overridden, section 6) | SHA-1 of the full HTML |
| `template-tag` | SHA-1 of the template |
| `template-change` | `false` |
| `Cache-Offline` | `true` |
| Body | JSON object (section 5.4) |
| `Content-Type` | Unchanged from the origin (typically `text/html`) — the body is JSON nevertheless |

### 5.3 Template change or first load

Condition: not 304, and the template SHA-1 differs from the request `template-tag` (or the client sent none).

| Field | Value |
|---|---|
| Status | `200` |
| `etag`, `template-tag` | As in 5.2 |
| `template-change` | `true` |
| `Cache-Offline` | `true` |
| Body | The full HTML |

### 5.4 Data body format

```json
{
  "data": {
    "{title}": "<title>Shop</title>",
    "{price}": "<!--sonicdiff-price--><p class=\"price\">120.00</p><!--sonicdiff-price-end-->"
  },
  "template-tag": "<40 hex>",
  "html-sha1": "<40 hex>",
  "diff": ""
}
```

| Key | Meaning |
|---|---|
| `data` | Map of placeholder → content. Keys include the braces. Values include the **marker comments** themselves. `{title}` is always present (empty string if no title). |
| `template-tag` | Same as the header |
| `html-sha1` | SHA-1 of the full HTML; the client uses it to verify the rebuilt page |
| `diff` | Always the empty string in upstream servers |

The server sends **all** data blocks, not only changed ones. The client computes the difference.

## 6. Custom etag header name

A server MAY rename the `etag` header. It then sends `sonic-etag-key: <name>` and puts the value in `<name>`. The Android client reads `sonic-etag-key` and falls back to `eTag` (`SonicSessionConnection.java:519-521`).

<Badge type="tip" text="FACT" /> **Trace finding:** the Node.js demo does **not** use `common/diff.js`. Its middleware loads the published npm package `sonic_differ` (latest 1.0.7, MIT), and that package always writes `Etag` and never sends `sonic-etag-key` (trace S02; package source compared with `common/diff.js`). The repository file `common/diff.js` is a newer, unused variant.

## 7. `cache-offline` {#cache-offline}

`cache-offline` tells the client how to use and store the response.

| Value | Client behavior | Evidence |
|---|---|---|
| `true` | Store the new data **and** refresh the page | `upstream:sonic-android/…/SonicSession.java:146-152`; `SonicUtils.java:575-603` |
| `false` | Refresh the page, do **not** store; remove the existing cache | `SonicSession.java:155-161`; flow `SonicSession.java:754-757` |
| `store` | Store the new data, do **not** refresh the current page | `SonicSession.java:136-144` |
| `http` | Legacy mode is unavailable for this URL: load normally and do not use legacy mode for a period | `SonicSession.java:125-133`; flow `SonicSession.java:730-745` |
| absent | Treated like `false` on non-first loads (cache removed) | `SonicSession.java:754-757` |

The `http` period is **6 hours** by default: Android `SONIC_UNAVAILABLE_TIME = 6 * 60 * 60 * 1000` ms (`upstream:sonic-android/…/SonicConfig.java:30`), iOS `cacheOfflineDisableTime = 21600` s (`upstream:sonic-iOS/Sonic/Engine/SonicConfiguration.m:31`).

Rivqen rules:

1. In legacy mode, Rivqen implements all four values.
2. `Cache-Control: no-store` **overrides** `cache-offline: true` and `store`. (Upstream Android checks `Cache-Control` only when `SUPPORT_CACHE_CONTROL` is on, default off: `SonicSessionConfig.java:72`, `SonicUtils.java:575-596`.)
3. The `http` period is configurable; default 6 hours for compatibility.

## 8. Other response headers

| Header | Use | Evidence |
|---|---|---|
| `sonic-link` | `;`-separated list of subresource URLs to prefetch (VasSonic 3.0). A `max-age` query parameter on each URL sets its cache lifetime. | `upstream:assets/VasSonic3.0_preload.md`; `SonicSession.java:705-708` |
| `Set-Cookie` | Copied into the WebView cookie store by the client | `SonicSession.java:1250` |
| `Content-Security-Policy`, `Content-Security-Policy-Report-Only` | Read by the client and passed to the WebView where the API allows | `SonicSessionConnection.java:110-119` |
| `Cache-Control`, `Expires`, `Pragma` | Used for cache lifetime only when `SUPPORT_CACHE_CONTROL` is on; lifetime capped at 5 minutes (`SONIC_CACHE_MAX_AGE`) | `SonicUtils.java:154-205`; `SonicConfig.java:55` |
| `Cache-Control: no-cache` | Added by Java and PHP servers to legacy responses | `SonicFilter.java:71`; `sonic.php:60` |

## 9. Non-legacy clients

**Canonical (MUST):** a server does not change the response body or add legacy headers for requests that are not legacy requests.

Upstream Java violates this: it adds `Etag`, `template-tag` and `template-change` to every text response (`SonicFilter.java:103-144`). PHP and Node.js do not. See [Divergences](/engineering/protocol/legacy-divergences).

## 10. Canonical server algorithm {#canonical-server}

<Badge type="info" text="DESIGN" /> This section is the exact behavior of a server in the `legacy-canonical` profile (ADR-005). It joins the canonical rules of this page, of the [markers grammar](/engineering/protocol/legacy-markers#_4-canonical-legacy-grammar-for-rivqen) §4 and of the canonical column of [Divergences](/engineering/protocol/legacy-divergences), and closes their gaps (WP-02 kickoff: DL-070, DL-072 to DL-075). The `fixtures/legacy` generator and every server SDK implement it. "Pass-through" means: the application response goes out unchanged (status, every field line, body), with no legacy field added.

Input: the request (method, field lines), the application response A (status, field lines, body bytes) and the configuration. A field name compares ASCII case-insensitively; a field value is trimmed of spaces and tabs at both ends.

| Rule | Behavior |
|---|---|
| C-01 Scope | Only a `GET` request with A status `200` and exactly one A `content-type` field line with the media type `text/html` (ASCII case-insensitive; parameters ignored) can get legacy processing. Any other request or response (`HEAD`, `POST`, 3xx, 404, other media types): pass-through (DL-074) |
| C-02 Legacy request | The request has exactly one `accept-diff` field line and its value is exactly `true`. Otherwise: pass-through (§3, §9, D-01, D-02) |
| C-03 Text | The `charset` parameter of A, if present, is `utf-8` (ASCII case-insensitive, also as a quoted string), and the body is valid UTF-8. Otherwise: pass-through (DL-074; replaces the "decode other charsets" row of the markers grammar §4) |
| C-04 `etag` | The lowercase hex SHA-1 of the body bytes of A, without quotes (§4) |
| C-05 Not modified | The request has exactly one `If-None-Match` field line; its value without one leading `W/` and without one pair of enclosing `"` equals C-04 (ASCII case-insensitive, D-03). Then: status `304`, no body, `cache-offline: store`, `cache-control` of C-09; no `etag`, `template-tag` or `template-change` field (all three upstream servers send none; trace S03) |
| C-06 Title | In the body text, find the first match of `<title([^\n]*?)</title>` (ASCII case-insensitive). Replace it with `{title}`. The data value `{title}` is the whole match, or the empty string when there is no match (the key is always present; markers §2, D-04, Node.js rule) |
| C-07 Blocks | In the result of C-06, find the matches of `<!--sonicdiff-?(\w*)-->([\s\S]+?)<!--sonicdiff-?(\w*)-end-->` from left to right, without overlap (ASCII case-insensitive; `\w` = `[A-Za-z0-9_]`; the lazy match ends at the first end marker; the end name is not checked). For each match: if the name (group 1) is empty or longer than 128 characters, the match stays in the template and has no data key (D-05); else it is replaced with `{name}` and the data value `{name}` is the whole match. If a name occurs twice (case-sensitive), or a block is named `title` (its key would be `{title}`), the split fails: pass-through (DL-075, DL-076). The split runs before C-05: a page whose split fails gets pass-through also when `If-None-Match` matches |
| C-08 Template | The template is the text after C-07; `template-tag` is the lowercase hex SHA-1 of its UTF-8 bytes. The data map has `{title}` first, then the blocks in document order |
| C-09 `cache-control` | `no-cache, private`, which replaces the field lines of A; but if a field line of A has the directive `no-store`, the field lines of A stay (D-09) |
| C-10 `cache-offline` | One field line. Default: `true` on `200`, `store` on `304`. On `200`, an A value `true`, `false`, `store` or `http` (exactly, lowercase) replaces the default; any other A value is ignored (DL-070) |
| C-11 Data response | When the request has exactly one `template-tag` field line and its value equals C-08 exactly: status `200`, body = the UTF-8 JSON object with the members `data` (C-08), `template-tag`, `html-sha1` (= C-04) and `diff` (`""`), without HTML escaping; `template-change: false` |
| C-12 Full response | Otherwise: status `200`, the body of A unchanged, `template-change: true` |
| C-13 Fields | C-05, C-11 and C-12 keep the field lines of A except `etag`, `template-tag`, `template-change`, `cache-offline`, `sonic-etag-key`, `cache-control` (C-09), `cdn-cache-control`, `surrogate-control` (H-111) and `content-length`, then add `etag` (not on `304`), `template-tag` (not on `304`), `template-change` (not on `304`), `cache-offline`, `cache-control` and C-15. `content-type` stays the value of A (DL-072). `Content-Length` is the length of the sent body |
| C-14 Content coding | The server does not use the request `Accept-Encoding` to decide (D-10). A legacy response has no content coding or `gzip` (DL-073) |
| C-15 `Vary` | C-05, C-11 and C-12 keep the `vary` field lines of A and add one field line `vary: template-tag` after them, unless a `vary` line of A already lists `template-tag` or `*` (ASCII case-insensitive members). The same URL gives the full document or the data response by the `template-tag` request field, so a cache must not give one for the other (H-111, U-12) |
| C-16 Linear time | A server MUST compute C-06 and C-07 in time linear in the body length. The regular expressions above define the result; a backtracking regular expression engine can take quadratic time on many unclosed `<title` or `<!--sonicdiff` openers, so an implementation scans forward: C-06 tries each `<title` opener against the next `</title>` and the next LF after it; C-07 stops when no end marker follows an opener (DL-077) |

Request fields `sonic-sdk-version`, `Cookie`, `User-Agent` and `Accept-Encoding` do not change the result.

## 11. Verification

Fixtures `FX-LEG-*` in [Golden fixtures](/engineering/protocol/fixtures) cover every section of this page; every rule C-01 to C-16 has at least one fixture (`fixtures/legacy/README.md`).
