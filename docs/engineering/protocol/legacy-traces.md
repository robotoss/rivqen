# Legacy: server trace report

This page reports the **captured behavior** of the three upstream VasSonic servers. The traces confirm or correct the statements in the legacy protocol pages.

::: warning Legacy mode — temporary
This is evidence for the optional legacy module (deprecated from 1.0, removed in 1.5).
:::

**Status:** <Badge type="tip" text="FACT" /> Captured on 2026-10-09 from upstream code at `59936bef`, run unmodified in isolated containers.

[[toc]]

## 1. Setup

| Server | Upstream code used | Runtime |
|---|---|---|
| Java | Binary `sonic-java/lib/VasSonic-1.1.jar` (`SonicFilter`) + Gson 2.8.2 (the version declared in the jar's POM) | Tomcat 9, JDK 11 |
| Node.js | `sonic-nodejs/middleware/compress.js` + its npm dependency `sonic_differ@1.0.7` (Koa 2.16) | Node.js 22 |
| PHP | `sonic-php/sdk/sonic.php` (`util_sonic::start()` / `end()`) | PHP 8.3 built-in server |

The origin pages are **Rivqen test pages** (`tools/upstream-lab/pages/`). A small harness per server serves them through the upstream code. The runner (`tools/upstream-lab/run_traces.py`) sends each request, stores the raw response, and checks it against an **independent model** of the documented rules (SHA-1 values, template, data map).

Raw traces: `evidence/traces/server/<server>/*.http` · Machine-readable results: `evidence/traces/server/results.json` · Source manifest: `evidence/source-manifest.lock`.

## 2. Result

**113 of 115 checks passed.** Both failures are expected upstream behaviors that the documentation now records:

| Failed check | Server | Explanation |
|---|---|---|
| S01 no legacy headers for a non-legacy client | Java | Java adds `Etag`, `template-tag` and `template-change` to every text response (divergence D-02). |
| S03 `304` has `Cache-Offline: store` | Java | Java sends two fields, `true` and `store` (new divergence D-12). |

## 3. Checks per scenario

| Scenario | Check | Java | Node.js | PHP |
|---|---|---|---|---|
| S01 | body unchanged for non-legacy client | ✅ | ✅ | ✅ |
| S01 | no legacy headers for non-legacy client | ❌ | ✅ | ✅ |
| S02 | status 200 + full HTML | ✅ | ✅ | ✅ |
| S02 | etag = lowercase hex SHA-1 of HTML, unquoted | ✅ | ✅ | ✅ |
| S02 | template-tag = SHA-1 of modeled template | ✅ | ✅ | ✅ |
| S02 | template-change: true | ✅ | ✅ | ✅ |
| S02 | Cache-Offline: true | ✅ | ✅ | ✅ |
| S03 | 304 when If-None-Match equals SHA-1 | ✅ | ✅ | ✅ |
| S03 | 304 has Cache-Offline: store | ❌ | ✅ | ✅ |
| S03 | 304 has no body | ✅ | ✅ | ✅ |
| S04 | uppercase etag -> 304 only in Java | ✅ | ✅ | ✅ |
| S05 | quoted etag is NOT recognized (no 304) | ✅ | ✅ | ✅ |
| S06 | JSON data equals modeled data map | ✅ | ✅ | ✅ |
| S06 | html-sha1 = SHA-1 of new HTML | ✅ | ✅ | ✅ |
| S06 | diff is empty string | ✅ | ✅ | ✅ |
| S06 | template-tag in body equals header | ✅ | ✅ | ✅ |
| S06 | body is JSON | ✅ | ✅ | ✅ |
| S06 | template-change: false | ✅ | ✅ | ✅ |
| S07 | template change -> full HTML + template-change: true | ✅ | ✅ | ✅ |
| S12 | nomark: template-tag matches model | ✅ | ✅ | ✅ |
| S13 | emptyname: template-tag matches model | ✅ | ✅ | ✅ |
| S13 | emptyname: data map matches model | ✅ | ✅ | ✅ |
| S14 | dup: template-tag matches model | ✅ | ✅ | ✅ |
| S14 | dup: data map matches model | ✅ | ✅ | ✅ |
| S15 | titleattr: template-tag matches model | ✅ | ✅ | ✅ |
| S15 | titleattr: data map matches model | ✅ | ✅ | ✅ |
| S16 | titles: template-tag matches model | ✅ | ✅ | ✅ |
| S16 | titles: data map matches model | ✅ | ✅ | ✅ |
| S17 | spaces: template-tag matches model | ✅ | ✅ | ✅ |
| S18 | nested: template-tag matches model | ✅ | ✅ | ✅ |
| S18 | nested: data map matches model | ✅ | ✅ | ✅ |
| S19 | mismatch: template-tag matches model | ✅ | ✅ | ✅ |
| S19 | mismatch: data map matches model | ✅ | ✅ | ✅ |
| S20 | uppercase: template-tag matches model | ✅ | ✅ | ✅ |
| S20 | uppercase: data map matches model | ✅ | ✅ | ✅ |
| S21 | unicode: template-tag matches model | ✅ | ✅ | ✅ |
| S21 | unicode: data map matches model | ✅ | ✅ | ✅ |
| S12 | nomark: data map matches model | ✅ | — | ✅ |
| S17 | spaces: data map matches model | ✅ | — | ✅ |

## 4. Request detection and encoding

Response body type for a data-update request (`If-None-Match` = old etag, `template-tag` = current tag):

| Variation | Java | Node.js | PHP |
|---|---|---|---|
| `accept-diff: false` | JSON | JSON | HTML |
| `accept-diff: yes` | JSON | JSON | HTML |
| `Accept-Diff: true` (capitalized name) | JSON | JSON | HTML |
| `Accept-Encoding: identity` | JSON | HTML | JSON |

Findings: Java chooses the data response by `template-tag` alone; Node.js accepts any `accept-diff` value but skips legacy processing for `identity` encoding; PHP needs the exact lowercase header name on its built-in server.

## 5. New findings (not visible from code reading alone)

| # | Finding | Where documented |
|---|---|---|
| T-01 | Java: data-only response does not depend on `accept-diff` | [Legacy wire §3](/engineering/protocol/legacy-wire#_3-server-detection-of-a-legacy-request), D-01 |
| T-02 | Java: two `Cache-Offline` fields on `304` | [Legacy wire §5.1](/engineering/protocol/legacy-wire#_5-1-not-modified), D-12 |
| T-03 | PHP: header name lookup is case-sensitive | Legacy wire §3, D-01 |
| T-04 | Java and PHP corrupt pages with several `<title>` elements | [Markers grammar §2](/engineering/protocol/legacy-markers#_2-the-title-rule), D-13, U-11 |
| T-05 | Published `sonic_differ@1.0.7` has no `sonic-etag-key`; the repository's `common/diff.js` is unused by the demo | [Legacy wire §6](/engineering/protocol/legacy-wire#_6-custom-etag-header-name), D-07 |

## 6. Confirmed statements

SHA-1 of the full HTML as unquoted lowercase hex; SHA-1 of the template as `template-tag`; JSON keys and `{title}` handling; `diff` always empty; `html-sha1`; all-blocks data body; `304` without body; case rules for `If-None-Match`; empty block names (`{auto0}` in Node.js, `{}` in Java and PHP); last duplicate wins; markers with spaces never match; nested markers end at the first end marker; mismatched end names accepted; markers are case-insensitive on all servers; Unicode content preserved; Gson HTML-escaping and PHP slash/Unicode escaping in JSON.

## 7. Reproduce

See `tools/upstream-lab/README.md`. The lab needs Docker, the upstream repository at the pinned commit, and about 1 GB of images.

## 8. Still open

Client-side behavior (Android Quick/Standard timing, result codes in the page) needs traces from the upstream Android sample on an old emulator image (WP-01). The iOS client cannot run on current iOS versions (`UIWebView`).
