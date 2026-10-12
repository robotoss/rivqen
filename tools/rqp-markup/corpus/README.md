# RQP markup corpus

<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- SPDX-FileCopyrightText: The Rivqen Authors -->

Measures the RQP markup rules (`docs/engineering/protocol/markup.md`) on real public pages: how many pages are valid, which rules reject them, and what each rule costs. Task WP-17/S1/T-15, human decision H-39. Results: `evidence/markup-corpus/` (`summary.md`).

**Status:** DESIGN (measurement tool). It is not a candidate, not part of the differential runner and not part of CI. It uses the network once, to fetch the pages.

## 1. Files

| File | Use |
|---|---|
| `url-list.tsv` | The input list: URL, category, kind, language. Written by hand |
| `fetch.mjs` | Fetches the pages (polite, bounded) into a data directory |
| `layout.mjs` | Lays the pages out as `pages/<id>/input.html`; finds bot walls and the platform |
| `classify.mjs` | All-rules classifier: every rule class of a page, with the rule item |
| `depth-cli.mjs` | Runs the depth analysis of `../crosscheck/src/depth.mjs` on one file |
| `rule-probe/` | Small Rust program: the first rule ID of the Rust candidate library |
| `run.mjs` | Runs candidates, probe, classifier, simulated blocks and the parse5 cross-check |
| `summary.mjs` | Writes `summary.md` from the evidence files |
| `test/` | Tests of the classifier, the picks and the helpers |

## 2. Safety of the data

The fetched pages are untrusted data and copyrighted content.

- Keep them outside the repository, in a new empty directory. Never commit them. The scripts write page bytes only to the data directory; the evidence directory gets URLs, numbers, rule IDs and tag names only.
- The scripts never execute page content. They copy bytes, tokenize with the parse5 tokenizer, and do bounded substring searches.
- Run Python tools with `-I` if you read the data with Python. Keep scripts in this directory, not in the data directory.

## 3. Procedure

Do these steps in the repository root. `DATA` is a new empty directory outside the repository.

1. Build the candidates: see the `README.md` of each candidate (`../rust`, `../node`, `../java`, `../php`).
2. Install the cross-check dependencies: in `tools/rqp-markup/crosscheck`, run `npm ci --ignore-scripts`.
3. Build the rule probe: in `tools/rqp-markup/corpus/rule-probe`, run `cargo build --release`.
4. Fetch the pages:
   `node tools/rqp-markup/corpus/fetch.mjs --list tools/rqp-markup/corpus/url-list.tsv --out "$DATA"`
5. Lay them out: `node tools/rqp-markup/corpus/layout.mjs --data "$DATA"`
6. Run the measurement (in `tools/rqp-markup/corpus`):
   `node run.mjs --data "$DATA" --evidence ../../../evidence/markup-corpus`
7. Write the summary: `node summary.mjs --evidence ../../../evidence/markup-corpus`

The fetch takes some minutes. The measurement of about 870 pages takes some minutes (8 jobs).

## 4. Fetch rules

| Rule | Value |
|---|---|
| Requests per host | One at a time, 1 s delay between two requests |
| Global concurrency | 12 |
| Timeout | 15 s per request (headers and body) |
| Size | The read stops after 5 MiB + 1 byte; the page is marked truncated (M-01 rejects it) |
| Body | After HTTP decompression (gzip, deflate, br), before any parsing |
| User-Agent | A browser-like string that says `RivqenMarkupCorpusTest/1.0 (one-off markup test client)` |
| Inner pages | A row with kind `home+X` gives one same-host link of kind X from the home page: no query string, no login, cart or account paths |
| Record | No query string and no cookies are stored; the final URL is kept without query |

## 5. Classifier

`classify.mjs` drives the parse5 8.0.1 tokenizer (the WHATWG tokenizer) without a tree builder. After a start tag it sets the tokenizer state by M-04 and the foreign-region flag of M-05, like a candidate. It does not stop at the first violation: it records each violation and continues with the token-stack behavior of M-05. So a page whose only class is X passes when only X is removed.

Classes (rule and item):

| Class | Rule |
|---|---|
| `M-01`, `M-02` | Size, encoding (other rules are measured on the decoded text) |
| `M-06.1` | End tag crosses a guarded element |
| `M-06.2` | Formatting end tag crosses a special or foreign element (H-26) |
| `M-06.3-table`, `M-06.3-row` | Stray `</table>`; stray `</tbody>`, `</thead>`, `</tfoot>`, `</tr>` (H-30) |
| `M-06.4-special`, `M-06.4-foreign` | Formatting end tag pops a marked element; the marks come from special crossings, or from foreign crossings only (H-32) |
| `M-07.1` … `M-07.4` | Foreign regions |
| `M-08`, `M-09`, `M-10`, `M-12` | Select, table parts, frameset, CDATA |
| `M-09.1`, `M-09.2` | M-09 condition 1 (`col` in a template, H-35) and condition 2 (F is not the current node, H-37; the detail names the current node) |
| `M-10.2`, `M-10.3` | Nested `form` (H-36); `template` with a `for` attribute (H-43) |
| `M-11.6` | Table part or `col` in `noscript` content without a `table` of the content (H-38); the detail says if the `noscript` is in a cell or caption |
| `M-11.1` … `M-11.4` | `noscript` content items 1 to 4 |
| `M-11.5-pli`, `M-11.5-other` | Unbalanced `noscript` content: only unclosed `p` or `li`, or other (H-27) |
| `M-13.1` … `M-13.3` | Script type items (H-28 is item 3) |
| `M-26-dirty`, `M-26-stack` | D > 256 only through the dirty mark (the units of the stack stay ≤ 256), or also without it (H-31) |
| `M-15` … `M-24.k` | Block rules, with `--blocks` only (simulated blocks) |

The classes `M-09.1`, `M-09.2`, `M-10.2`, `M-10.3`, `M-11.6` and `M-26-*` come from the cross-check oracle (`../crosscheck/src/depth.mjs`: `DepthCounter` and `checkNoscriptContent`), driven by the token stream of this pass. The pass adds only the details.

Not checked: M-25 (JSON) and the block limits of M-26.

## 6. Tests

Run `node --test` in this directory. The golden fixtures are the reference: the classifier finds no class exactly for the valid fixtures, and it finds the rule of each invalid fixture.

## 7. Limits

- The URL list is hand-made: popular sites and inner pages, not a random sample.
- A page is the server HTML for a client without JavaScript and cookies.
- The simulated blocks are a heuristic (DESIGN), see `summary.md` section 8.
- `run.mjs` redacts a URL path segment that holds a third-party admin path (the text `admin` and later `edit`): it keeps the first four words and ends with `REDACTED`.
