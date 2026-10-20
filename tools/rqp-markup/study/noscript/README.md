# H-42 study: `noscript` content model and the head rule with scripting disabled

<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- SPDX-FileCopyrightText: The Rivqen Authors -->

**After S3 (2026-10-18).** The decisions H-71…H-75 are now in the shipped candidates. This study models the rules before them: it imports the frozen pre-S3 name sets of `../baseline/names-s2.mjs`, and its tests compare "flags off" with the pre-S3 baseline `../baseline/baseline-s2.json`. Numbers that compare with "the shipped candidate" (`candidateMismatch`, `baseEqualsShipped`) were measured before S3; a run today differs by design.

Study scripts of WP-17 S3 task T-02. Report: `evidence/markup-noscript/report.md`. Machine results: `evidence/markup-noscript/*.json`.

**Status:** DESIGN (study code). It is not a candidate, not in the differential runner (`../../diff.mjs`) and not in CI. It does not change `markup.md`, `manifest-patch.md`, the candidates, the fixtures, the reference or the demo.

## 1. Files

| File | Use |
|---|---|
| `src/markup-study.mjs` | Copy of the Node.js candidate with the option `noscript`: `shipped` (M-11 as shipped), `main` (model (a): main-pass rules on the tokens of C), `allow` (model (b): allowlist for C); option `foreignInContent` (variant `a-fx`, input for H-40 relaxation 2) |
| `src/head-study.mjs` | Copy of the head walk of `tools/rqp-protocol/src/markup.mjs` with the option `rule`: `shipped`, `h0`, `h1`, `h2` |
| `src/views.mjs` | parse5 8.0.1: the end of the head element in each scripting view; where an inserted manifest element lands |
| `src/evaluate.mjs` | One document: every markup variant and head rule against parse5 in both views |
| `src/generator.mjs` | Seeded synthetic documents: profiles `body` and `head` |
| `src/sweep.mjs` | Agreement runs on the golden fixtures, the cross-check cases, the generated profiles and the `review2` profile of `../../crosscheck/src/generator.mjs` |
| `src/inventory.mjs` | `noscript` inventory of a document (location, tag names of C, text): no document text |
| `src/corpus-page.mjs`, `src/corpus.mjs` | Corpus run (S1 method: the pages, and the simulated blocks of `../../corpus/classify.mjs`) |
| `test/` | `node --test` tests; `test/equality.test.mjs` checks that the flags off give the shipped results |

## 2. Install

Do these steps once:

1. Install Node.js 22 or later.
2. In `tools/rqp-markup/crosscheck`, run `npm ci --ignore-scripts` (parse5 8.0.1).

The scripts import the Node.js candidate (`../../node/src`), the cross-check tool (`../../crosscheck/src`), the corpus classifier (`../../corpus/classify.mjs`), the protocol reference and the server demo (tests only). They have no other dependency.

## 3. Commands

Run the commands in `tools/rqp-markup/study/noscript`. `E` is `../../../../evidence/markup-noscript`.

1. Run the tests: `node --test "test/*.test.mjs"`.
2. Run the agreement runs (one process at a time):

   ```sh
   node src/sweep.mjs --source fixtures --out $E/sweep-fixtures.json
   node src/sweep.mjs --source cases --out $E/sweep-cases.json
   node src/sweep.mjs --source body --seed 42 --count 2000000 --examples 5 --out $E/sweep-body.json
   node src/sweep.mjs --source head --seed 42 --count 500000 --examples 5 --out $E/sweep-head.json
   node src/sweep.mjs --source review2 --seed 42 --count 300000 --examples 5 --out $E/sweep-review2.json
   ```

3. Run the corpus (`DATA` is the corpus data directory of `../../corpus/README.md` §3, outside the repository):

   ```sh
   node src/corpus.mjs --data "$DATA" --evidence $E --jobs 3
   ```

   This writes `$E/corpus-pages.jsonl` and `$E/corpus.json`. To compute `corpus.json` again from the page rows, run `node src/corpus.mjs --aggregate $E/corpus-pages.jsonl --evidence $E`.

Measured on 2026-10-18 (Apple silicon, Node.js 26): the body sweep takes about 2.5 minutes, the head sweep 35 s, the review2 sweep 65 s, the corpus some minutes with 3 jobs.

## 4. Safety of the data

- The corpus pages are untrusted, copyrighted data. The scripts read them only. They write numbers, URLs (redacted as in `../../corpus/run.mjs`), rule labels and tag names (only names that match `^[a-z][a-z0-9-]{0,31}$`; other names are `?`). No page text goes to the evidence.
- The scripts never execute page content: they tokenize with the candidate tokenizer and parse with parse5 (no scripts run in parse5).
- The sweep outputs contain synthetic documents only (at most `--examples` for each disagreement group).

## 5. Limits

- parse5 8.0.1 is one implementation of the WHATWG parser. A disagreement that parse5 shares with a model is not found.
- The generators reach the shapes that they contain. A sweep without findings is evidence, not a proof.
- Only the Node.js candidate is copied. The four candidates give the same result on all golden fixtures (`../../diff.mjs`), but the study does not run the other three.
