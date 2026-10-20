# WP-17 S4 T-03 study: implied head (H-79) and implied table closure (H-85)

<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- SPDX-FileCopyrightText: The Rivqen Authors -->

**Status:** DESIGN (study tool). It is not a candidate, not part of the differential runner and not part of CI. It changes no rule: the proposals exist only behind flags in study copies. Results: `evidence/markup-s4/`.

## 1. Questions

| Study | Decision | Question |
|---|---|---|
| A | H-79 | When item 1 of the head walk (manifest-patch §1.7, "Document head") stops at a start tag `base`, `basefont`, `bgsound`, `link`, `meta`, `noframes`, `script`, `style` or `title` before any head start tag, can the manifest go at the `<` of that tag? Which client rule finds the same position? |
| B | H-85 | Can the token stack model the implied closure of a cell, a row, a row group, a caption and a column group (markup M-09) so that the blocks stay equal to parse5 in both scripting views and the depth claim of M-26 holds? |

## 2. Files

| File | Use |
|---|---|
| `src/head.mjs` | Study A: the head walk with the rules `shipped` (H-72), `h79`, `l1` (also `noscript`, `template`), `l2` (every start tag); the client rules `c1` (walk on F) and `c2` (walk on the original document O) |
| `src/head-views.mjs` | Study A: parse5 checks of F in one view (manifest is the first child of the head, the only manifest, the tree of F without it equals the tree of O) |
| `src/head-gen.mjs`, `src/head-sweep.mjs` | Study A: generator of prefixes; sweep (fixtures, protocol documents, cross-check cases, generated, hostile client positions) |
| `src/markup-s4.mjs` | Study B: copy of `../../node/src/markup.mjs` with the flag `close`: `off` (shipped), `b1` (exact closure), `b2` (closure as the end tag of F), `b3` (b2 and a release of D for cells; rejected by the study), `u` (control without guards) |
| `src/depth-s4.mjs` | Study B: the depth analysis of `../../crosscheck/src/depth.mjs` with the closure (and the b3 release) |
| `src/table-oracle.mjs` | Study B: parse5 oracle: cross-check comparison and depth claim in both views, table parent check, rewrite check (the closures written as end tags) |
| `src/table-gen.mjs`, `src/table-sweep.mjs` | Study B: generator of table shapes; sweep (fixtures, cases, tables, the four cross-check generator profiles) |
| `src/corpus-page.mjs`, `src/corpus.mjs` | Corpus run of both studies (S1 method, simulated blocks of `../../corpus/classify.mjs`) |
| `src/inputs.mjs` | Fixtures, cross-check cases and protocol documents of the repository |
| `test/` | Tests (`node --test`); `test/equality.test.mjs`: with the flags off the study code equals the pre-S4 Node.js candidate (commit `9bd8c32`, recorded in `../baseline/baseline-s3.json`: 247 golden fixtures, the S3 cases, generated documents) and the shipped head walk of the reference and the demo. The study copy imports the frozen pre-S4 name sets `../baseline/names-s3.mjs`: the shipped candidate follows H-78, H-84, H-86 and H-85a |

## 3. Procedure

Do these steps in `tools/rqp-markup/study/s4`. `E` is `../../../../evidence/markup-s4`. `DATA` is the corpus data directory (`../../corpus/README.md` §3, steps 4 and 5), outside the repository.

1. In `tools/rqp-markup/crosscheck` and in `tools/rqp-markup/node`, run `npm ci --ignore-scripts`.
2. Run the tests: `node --test "test/*.test.mjs"`.
3. Run the sweeps, one process at a time:

   ```sh
   node src/head-sweep.mjs --source fixtures --out $E/head-fixtures.json
   node src/head-sweep.mjs --source protocol --out $E/head-protocol.json
   node src/head-sweep.mjs --source cases --out $E/head-cases.json
   node src/head-sweep.mjs --source gen --seed 79 --count 500000 --out $E/head-gen.json
   node src/head-sweep.mjs --source hostile --seed 79 --count 200000 --out $E/head-hostile.json
   node src/table-sweep.mjs --source fixtures --out $E/table-fixtures.json
   node src/table-sweep.mjs --source cases --out $E/table-cases.json
   node src/table-sweep.mjs --source tables --seed 85 --count 500000 --control 1 --out $E/table-tables.json
   node src/table-sweep.mjs --source classic --seed 85 --count 200000 --control 1 --out $E/table-classic.json
   ```

   Run the last command also with `--source review`, `review2` and `review3` (outputs `table-<source>.json`).
4. Run the corpus: `node src/corpus.mjs --data "$DATA" --evidence $E --jobs 3`. To compute `corpus.json` again from the page rows, run `node src/corpus.mjs --aggregate $E/corpus-pages.jsonl --evidence $E`.
5. Delete `DATA`.

Measured on 2026-10-18 (Apple silicon, Node.js 26): each sweep takes 15 s to 2 minutes, the corpus about 4 minutes with 3 jobs.

## 4. Outputs

| File | Content |
|---|---|
| `head-*.json` | For each head rule: insertion points by kind, stop tags, failures on valid pages (`failures`) and on other documents (`failuresAnyPage`); `hostile`: client rule counts; `full`: client rules on the protocol full documents |
| `table-*.json` | For each close mode: valid, gained, checked, findings by kind, rejection labels, closures by kind, up to 3 synthetic examples (fixture or case names for repository inputs) |
| `corpus-pages.jsonl`, `corpus.json` | One line per corpus page and the totals: head rule results, close mode results, variants |

The outputs have URLs (redacted as in `../../corpus/run.mjs`), numbers, rule labels, tag names and synthetic documents only.

## 5. Safety of the data

- The corpus pages are untrusted, copyrighted data. The scripts read them only, never write them, and never execute them: they tokenize with the candidate tokenizer and parse with parse5 (no scripts run in parse5).
- Labels from a page are kept only when they look like tag names (`^</?[a-z][a-z0-9-]{0,31}>$`); else `?`.

## 6. Limits

- parse5 8.0.1 is one implementation of the WHATWG parser. A disagreement that parse5 shares with a model is not found.
- The generators reach the shapes that they contain. A sweep without findings is evidence, not a proof.
- Only the Node.js candidate is copied. The four candidates give the same result on all golden fixtures (`../../diff.mjs`).
- The tree checks compare structure and offsets, not the text inside a block after a reconstruction of formatting elements (the shipped rules accept such shapes too).
