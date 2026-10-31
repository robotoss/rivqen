# Malformed fixtures

<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- SPDX-FileCopyrightText: The Rivqen Authors -->

Negative cases and fuzz seeds for the server side of the legacy protocol (WP-02 S2, DL-080). Every case must get pass-through or a defined legacy response by `docs/engineering/protocol/legacy-wire.md` §10 (rules C-01 to C-16); no case may crash a server or exceed the runner timeout. The RQP negative cases are in `fixtures/rqp` (markup and protocol suites).

## Layout

```text
fixtures/malformed/
├── legacy/
│   ├── pages/<page-id>.html          # application body bytes, exact
│   └── FX-LEG-MAL-<NAME>/
│       ├── case.json                 # format of fixtures/legacy/README.md, profile legacy-canonical, evidence code-reading
│       └── expected.json             # generated: node tools/legacy-fixtures/gen.mjs --dir fixtures/malformed/legacy
└── seeds/
    ├── pages/<name>.html             # raw pages for the fuzz targets of the legacy split (WP-05, WP-18)
    └── fields/<name>.txt             # raw request field values (one value per file, no line end)
```

Run: `node server/conformance/e2e/run.mjs --module tools/legacy-fixtures/src/server.mjs --suite legacy -- --fixtures fixtures/malformed/legacy`. The rule coverage and the catalog size of `gen.mjs` apply only to `fixtures/legacy`.

## Cases

Default page: `b` (a byte copy of `fixtures/legacy/pages/b.html` in `pages/`). Default request: `accept-encoding: gzip`, `accept-diff: true`, `if-none-match:` 40 zeros, `template-tag: {{tag:<page>}}` (a data request for the page of the case). Each row changes only what it names. "Expected" is for the reviewer; the generator computes the exact response.

| ID | Page or request | Rule | Expected |
|---|---|---|---|
| `FX-LEG-MAL-OPENERS-1MIB` | page `b` with 1 MiB of alternating `<title` and `<!--sonicdiff-a-->x` in the footer text, no LF | C-06, C-07, C-16 | data response; in less than 1 s |
| `FX-LEG-MAL-NESTED-1000` | 1000 start markers `<!--sonicdiff-n-->` then one end marker | C-07 | one block from the first start marker to the first end marker |
| `FX-LEG-MAL-HUGE-NAME` | a start marker with a 60 KiB name and its end marker | C-07 | no key; the match stays in the template |
| `FX-LEG-MAL-EMPTY-CONTENT` | `<!--sonicdiff-a--><!--sonicdiff-a-end-->` before the price block | C-07 | the lazy content needs one character: as the expression gives |
| `FX-LEG-MAL-END-NAME-UPPER` | the price end marker as `<!--SONICDIFF-price-END-->` | C-07 | `{price}` key (case-insensitive) |
| `FX-LEG-MAL-BLOCK-NAMED-TITLE` | a block named `title` | C-07 | pass-through (DL-076) |
| `FX-LEG-MAL-MARKER-IN-SCRIPT` | the price block inside `<script>` and a stock block inside a comment | C-07 | both are blocks (textual split, markers §4) |
| `FX-LEG-MAL-PLACEHOLDER-TEXT` | the page text has a literal `{price}` before the price block | C-07, C-08 | data response; `split.rebuild_sha1` differs from the etag (the client check of `html-sha1` fails and the client loads normally) |
| `FX-LEG-MAL-TITLE-UNCLOSED` | `<title>Shop` without `</title>` | C-06 | `{title}` = `""` |
| `FX-LEG-MAL-CLOSE-BEFORE-OPEN` | `</title>` before `<title>`, no later `</title>` | C-06 | `{title}` = `""` |
| `FX-LEG-MAL-TITLE-CR` | `<title>Shop\rNew</title>` (a CR byte, no LF) | C-06 | the title matches |
| `FX-LEG-MAL-NUL-BYTES` | page `b` with NUL bytes in the footer text | C-03 | data response (NUL is valid UTF-8) |
| `FX-LEG-MAL-SURROGATE` | the bytes `ED A0 80` (an encoded surrogate) in the footer | C-03 | pass-through |
| `FX-LEG-MAL-OVERLONG` | the bytes `C0 AF` in the footer | C-03 | pass-through |
| `FX-LEG-MAL-TRUNCATED-UTF8` | the page ends with the byte `E2` | C-03 | pass-through |
| `FX-LEG-MAL-BOM` | a UTF-8 byte order mark at offset 0 | C-03, C-04 | data response; the etag covers the BOM bytes |
| `FX-LEG-MAL-CONTENT-TYPE-TWO` | the application sends two `content-type` lines | C-01 | pass-through (DL-076) |
| `FX-LEG-MAL-ACCEPT-DIFF-SPACES` | `accept-diff:` value `   true   ` | C-02 | legacy (values are trimmed) |
| `FX-LEG-MAL-ACCEPT-DIFF-UPPER` | `accept-diff: TRUE` | C-02 | pass-through |
| `FX-LEG-MAL-INM-LIST` | `if-none-match: "x", "{{etag:b}}"` | C-05 | no `304` |
| `FX-LEG-MAL-INM-6KIB` | `if-none-match` of 6 KiB of `a` (the field section stays under the 8 KiB that a server under test must accept, CONTRACT.md §3) | C-05 | no `304`; data response |
| `FX-LEG-MAL-TEMPLATE-TAG-100` | 100 `template-tag` lines with the right value | C-11 | full response (not exactly one line) |

## Seeds

The pages of the cases, plus at least 10 more raw pages and 10 request field values. Each seed file has one row in a table in this section (name, what it stresses). Seeds have no expected result; a fuzz target asserts only "no crash, bounded time and memory, result equal to the regular expressions of C-06 and C-07".

## Rules

1. Files are exact bytes (`.gitattributes`: `fixtures/** -text`).
2. No real user data, tokens or domains. Use `example.com`.
3. A case that shows a gap in the canonical rules is not changed to pass: it goes to the architect.
