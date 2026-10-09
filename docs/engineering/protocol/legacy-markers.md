# Legacy protocol: markers grammar

::: warning Legacy mode — temporary
This page describes the **legacy mode** (compatibility with the VasSonic legacy protocol). It ships as an optional module, is **deprecated from 1.0** and is **removed in 1.5**. New integrations use the [Rivqen protocol](/engineering/protocol/markup). See [Modes and legacy deprecation](/engineering/protocol/versioning).
:::

This page specifies how legacy protocol splits an HTML document into a **template** and **data blocks**. It is the most fragile part of legacy: five upstream implementations use similar but not identical rules.

**Status:** <Badge type="tip" text="FACT" /> from upstream code at `59936bef`; server behavior **confirmed by traces** S12–S21 ([report](/engineering/protocol/legacy-traces)). The canonical rules (section 4) are <Badge type="info" text="DESIGN" /> decisions for ADR-005.

[[toc]]

## 1. The marker pattern

All upstream implementations use the same regular expression for data blocks:

```text
<!--sonicdiff-?(\w*)-->([\s\S]+?)<!--sonicdiff-?(\w*)-end-->
```

| Implementation | Flags | Evidence |
|---|---|---|
| Java server | case-insensitive | `upstream:sonic-java/src/main/java/com/github/tencent/SonicFilter.java:114-116` |
| Node.js server | `i`, `g` | `upstream:sonic-nodejs/common/diff.js:51` |
| PHP server | `i` | `upstream:sonic-php/sdk/sonic.php:106` |
| Android client | `MULTILINE` (case-**sensitive**) | `upstream:sonic-android/sdk/src/main/java/com/tencent/sonic/sdk/SonicUtils.java:74, 484` |
| iOS client | case-insensitive | `upstream:sonic-iOS/Sonic/Util/SonicUtil.m:185` |

Meaning:

- `<!--sonicdiff` then an optional `-` then a name of word characters (`[A-Za-z0-9_]`, possibly empty) then `-->`.
- At least one character of content, matched lazily (shortest match).
- A closing `<!--sonicdiff`, optional `-`, a name, `-end-->`.
- The closing name is **not** checked against the opening name.
- No spaces are allowed inside the markers.

::: warning Documentation vs code
The upstream wiki and comments in the React demo show markers with spaces, such as `<!-- sonicdiff-name -->` (`upstream wiki: VasSonic入门须知.md`; `upstream:sonic-react/server.js:30-40`). **The code does not match such markers.** The React demo's own code emits markers without spaces (`server.js:50`). Rivqen follows the code.
:::

## 2. The title rule

All upstream **servers** also treat the first `<title>` element as a data block named `title`:

| Implementation | Pattern | Occurrences replaced | Case | Evidence |
|---|---|---|---|---|
| Java | `<title(.*?)<\/title>` | **All** (case-sensitive replace) but the value is taken from the first case-insensitive match | Mixed | `SonicFilter.java:110-112` |
| Node.js | `/<title(.*?)<\/title>/i` | **First** only | Insensitive | `common/diff.js:40-43` |
| PHP | `/<title(.*?)<\/title>/i` | **All** | Insensitive | `sonic.php:99-104` |
| Android client (local server mode) | Literal `<title>` … `</title>` (no attributes), searched in the template **after** block extraction | First | Sensitive | `SonicUtils.java:59-63, 515-521` |
| iOS client | No title rule | — | — | `SonicUtil.m:177-208` |

The title value is the **whole element**, including the tags: `<title>Shop</title>`.

The title rule runs **before** the block rule on servers, and **after** it on the Android client. A `<title>` inside a data block is therefore handled differently.

::: danger Trace finding: pages with two `<title>` elements are corrupted (Java, PHP)
A page can contain more than one `<title>` element, for example an SVG icon with `<svg><title>Icon</title></svg>`. Trace S16 shows:

- **Java** takes the value from the first case-insensitive match (`<TITLE>Upper</TITLE>`) but replaces **case-sensitively** — so it replaces the SVG title instead, and leaves the real title in the template.
- **PHP** replaces **every** title with `{title}` but stores only the first value.

When the client rebuilds the page, the page title is copied into the SVG (both servers). Node.js replaces only the first title and is not affected. RQP has no implicit title rule ([RQP markup](/engineering/protocol/markup)).
:::

## 3. Template and data

For each match, upstream servers:

1. Store the **whole match** (including both marker comments) as the block value.
2. Replace the match in the template with `{name}`.
3. Use the key `{name}` in the `data` map.

| Edge case | Java | Node.js | PHP | Android client | iOS client |
|---|---|---|---|---|---|
| Empty name `<!--sonicdiff-->` | Key `{}` (auto-name branch is unreachable) | Key `{auto0}`, `{auto1}`… | Key `{}` | Key `{null}` | Key derived from the end marker |
| Duplicate name | Last value wins in data; template keeps two `{name}` placeholders | Same | Same | Same | Template built by string replace of each value |
| Name source | Opening marker | Opening marker | Opening marker | Opening marker | **Closing** marker |
| No markers in page | Template = HTML with title replaced | Full HTML, `template-change: true` always | Template = HTML with title replaced | Template is **empty** | Template = HTML |

Evidence: Java `SonicFilter.java:20-48, 114-133`; Node.js `common/diff.js:47-60, 75-122`; PHP `sonic.php:29-48, 106-126`; Android `SonicUtils.java:477-530`; iOS `SonicUtil.m:177-208`.

## 4. Canonical legacy grammar for Rivqen

Rivqen implements one **canonical** profile and keeps the other behaviors as named profiles for compatibility tests.

| Rule | Canonical behavior | Reason |
|---|---|---|
| Marker regex | As in section 1, case-insensitive | Matches the three servers and iOS |
| Block name | From the opening marker; `[A-Za-z0-9_]{1,128}` | Empty names are ambiguous across implementations |
| Empty name | Reject the block: treat the page as unmarked for that block (the block stays in the template) | Avoids `{}`, `{null}` and `{auto0}` disagreements |
| Mismatched end name | Accept (upstream compatibility), emit a diagnostic warning | All implementations accept it |
| Title | First `<title…>…</title>` case-insensitive, replaced **once**, before block extraction | Node.js behavior; the simplest unambiguous rule |
| Duplicate names | Reject the page as malformed for legacy split; send full HTML | Last-wins hides data |
| Nested markers | Not supported; the lazy match ends at the first end marker | Upstream behavior |
| Markers inside `<script>`, `<style>`, `<textarea>`, comments | Matched textually (no HTML parsing) | Upstream behavior; documented risk |
| Encoding | UTF-8 only; other charsets are decoded to UTF-8 before hashing | Upstream Java/Node assume UTF-8 |
| Line endings | Bytes as rendered; no normalization | Hash stability |

Profiles: `legacy-canonical` (default), `legacy-java`, `legacy-php`, `legacy-node`. See [Divergences](/engineering/protocol/legacy-divergences).

## 5. Rebuild rule (client)

To rebuild a page, the client replaces each `{key}` in the template with the data value.

<Badge type="tip" text="FACT" /> Upstream Android replaces only the **first** occurrence of each key, in the order of the JSON keys (`SonicUtils.java:339-366`), then verifies the SHA-1 of the result against `html-sha1` if present (`SonicUtils.java:301-334`).

**Canonical (MUST):** replace each placeholder exactly once, in template order; then verify `html-sha1`; on mismatch discard the result and select a full load.

## 6. Security notes

1. The split is textual, not an HTML parse. A data value can contain any HTML, including scripts. In legacy this is accepted because the whole page comes from the same origin over TLS.
2. Rivqen never applies legacy data values with `innerHTML` from untrusted origins. The legacy data path is allowed only for origins in `trust.allowedOrigins`.
3. Fuzz targets: marker parser, title rule, rebuild rule, with limits from [Resource limits](/engineering/core/resource-limits).

## Related

- [Template engine](/engineering/core/template-engine)
- [legacy divergences](/engineering/protocol/legacy-divergences)
