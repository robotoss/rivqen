# WP-17 S5 T-02 study: `template` and `noscript` in block content

<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- SPDX-FileCopyrightText: The Rivqen Authors -->

**Status:** DESIGN (study tool). It is not a candidate, not part of the differential runner and not part of CI. It changes no rule: the options exist only behind flags in a study copy. Report: `evidence/markup-s5/` (written by the architect from the lane report).

## 1. Question

Today `markup.md` M-22 item 2 rejects a `template` or `noscript` start tag in the content of an `html` block (`FORBIDDEN_CONTENT`, H-78 "until a study"). Which rules make them safe in block content, with the token stack of M-05, the state changes of M-04 and the depth counter D of M-26? What must the patch content rule (`manifest-patch.md` §4 item 1) add? What is the corpus cost and gain?

## 2. Options (flags of `src/markup-s5.mjs`)

| Flag | Value | Rule |
|---|---|---|
| `template` | `off` | Shipped (pre-S5) |
| | `t1` | `template` allowed in HTML context of the content; an attribute `shadowrootmode` (any value, any case, also a duplicate) on it rejects; all other rules apply in the template contents |
| | `t2` | `t1`, and a start tag in `TABLE_PART` or `col` in the content needs an F (M-09) that is not none: in a template, a table part needs a table opened in that template |
| | `tu` | Control, not a proposal: `t1` without the `shadowrootmode` rule and without M-09 condition 1 in the content |
| `noscript` | `off` | Shipped (pre-S5) |
| | `n1` | `noscript` allowed in HTML context of the content; its content C follows M-11, and the content rules M-22 to M-24 apply to the start tags of C (the pass of M-11 item 8 runs inside the block) |
| | `n2` | `n1`, and in C a start tag in `PCLOSE` rejects while an HTML `p` is open around the `noscript` element (above the nearest `template`) |
| | `nu` | Control, not a proposal: `n1` without M-22 to M-24 on the start tags of C |

Combinations (`src/options.mjs`): `base`, `t1`, `t2`, `tu`, `n1`, `n2`, `nu`, `t1n1`, `t2n2`.

Patch rule variants (`src/patch-s5.mjs`): `shipped` (the rule of `tools/rqp-protocol`), `p1` (`noscript` added to `PATCH_FORBIDDEN`), `p2` (`noscript` allowed in a patch; the rule also reads the start tags of C).

## 3. Files

| File | Use |
|---|---|
| `baseline/names-s4.mjs` | Frozen copy of the name sets of the Node.js candidate at commit `270278c` (pre-S5) |
| `baseline/make-s4.mjs`, `baseline/groups-s4.mjs`, `baseline/baseline-s4.json` | The results of the pre-S5 candidate on the 311 golden fixtures, the case files of `270278c` and generated groups; recorded once (see the comment in `make-s4.mjs`) |
| `src/markup-s5.mjs` | Study copy of `../../node/src/markup.mjs` with the flags; `why` labels and a trace (content start tags with parents, C tags, templates) |
| `src/oracle-s5.mjs` | parse5 8.0.1 oracle: block comparison, content elements (inside, namespace, parent), template ends, `noscript` in the scripting-disabled view, fragment parse with the block element as context (scripting on), views `enabled`, `disabled` and `spec` (the WHATWG table scope, see §6) |
| `src/depth-s5.mjs` | The depth claim of M-26 in both views (copy of `crosscheck/src/depth.mjs` `analyzeDepth`); the same bounds while a template is open, reported apart |
| `src/patch-s5.mjs` | Patch rule variants and the apply oracle: fragment parse in the modes "inert" (innerHTML) and "disabled" (DOMParser, parseHTMLUnsafe) |
| `src/gen-s5.mjs` | Generator profiles `tpl`, `nos`, `mix` |
| `src/sweep.mjs`, `src/summary.mjs` | Sweep of one source; totals of the sweeps |
| `src/corpus-page.mjs`, `src/corpus.mjs` | Corpus run (S1 method, simulated blocks `small` and `main`) |
| `src/inputs.mjs`, `src/options.mjs` | Inputs, combinations |
| `test/` | `node --test "test/*.test.mjs"` |
| `results/` | Machine results (aggregate numbers, rule labels, synthetic example documents, fixture and case names only) |

## 4. Procedure

Do these steps in `tools/rqp-markup/study/s5`. `DATA` is the corpus data directory (`../../corpus/README.md` §3, steps 4 and 5), outside the repository. `OUT` is a new directory outside the repository.

1. In `tools/rqp-markup/crosscheck` and in `tools/rqp-markup/node`, run `npm ci --ignore-scripts`.
2. Run the tests: `node --test "test/*.test.mjs"`.
3. Run the sweeps, one process at a time:

   ```sh
   node src/sweep.mjs --source fixtures --out results/sweep-fixtures.json
   node src/sweep.mjs --source cases --out results/sweep-cases.json
   node src/sweep.mjs --source classic --seed 5 --count 50000 --out results/sweep-classic.json
   node src/sweep.mjs --source tpl --seed 5 --count 100000 --out results/sweep-tpl.json
   ```

   Run the third command also with `review`, `review2`, `review3`, and the last command also with `nos` and `mix`.
4. Run `node src/summary.mjs --out results/summary.json results/sweep-*.json`.
5. Run the corpus: `node src/corpus.mjs --data "$DATA" --out "$OUT" --jobs 4`. Copy `$OUT/corpus.json` (totals only) to `results/corpus.json`. Do not copy `corpus-pages.jsonl`: it has page URLs.
6. Run the cross-check cases: in `tools/rqp-markup/crosscheck`, `node src/cli.mjs cases --cases cases/s5-cases.json --only node --scripting both`.
7. Delete `DATA` and `OUT`.

Measured on 2026-10-19 (Apple silicon, Node.js 26): all sweeps together take about 1 minute, the corpus about 30 s with 4 jobs.

## 5. Safety of the data

- The corpus pages are untrusted, copyrighted data. The scripts read them only, never write them, and never execute them: they tokenize with the candidate tokenizer and parse with parse5 (no scripts run in parse5).
- Rule labels from a page are kept only when they look like labels (`^[A-Za-z0-9:._-]{1,40}$`); else `?`. The evidence keeps aggregate numbers only.

## 6. Limits

- parse5 8.0.1 is one implementation of the WHATWG parser. It has no declarative shadow DOM (`shadowrootmode`) and no content patching (`template for`). Its "has an element in table scope" does not stop at `template` (WHATWG: `html`, `table`, `template`); the view `spec` patches that one function for the study (`withSpecTableScope`).
- The generators reach the shapes that they contain. A sweep without findings is evidence, not a proof.
- Only the Node.js candidate is copied. The four candidates give the same result on all golden fixtures (`../../diff.mjs`).
- The tree checks skip known classes of the shipped rules: a `p` closed by a start tag in `PCLOSE` (M-24 item a covers only `p` and `span` blocks), a formatting element reconstructed from before the block (`markup.md` §5 item 1), and a `noscript` in the head with scripting disabled (H-72). They are counted as `known-*`.
