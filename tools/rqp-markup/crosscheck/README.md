# RQP markup cross-check against parse5

<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- SPDX-FileCopyrightText: The Rivqen Authors -->

A differential check of the RQP markup parser candidates (`../CONTRACT.md`) against a WHATWG tree builder: `parse5` 8.0.1. It finds documents that a candidate accepts but where the browser tree has other blocks or other block bytes. Task WP-17/S1/T-07.

**Status:** DESIGN (test tool). It is not a candidate and not part of the differential runner (`../diff.mjs`).

## 1. What it checks

For each document and each candidate, the tool parses the document with parse5 (`sourceCodeLocationInfo: true`) and compares the trees with the candidate result.

When the candidate says **valid**, these must be true. Else the tool reports a finding.

| Check | Finding kind |
|---|---|
| The tree has the same number of elements with a `data-rq-block` attribute as the result has blocks | `parse5-<view>-blocks` |
| Each element has the same id, is in the HTML namespace, and has the same format (`script` for `json`) | `parse5-<view>-blocks` |
| `start` equals the end of the start tag, and `end` equals the start of an explicit end tag (UTF-8 byte offsets) | `parse5-<view>-blocks` |
| The tree has no `script` element whose `type` contains `rivqen-manifest` (case-insensitive, after character reference decoding; also after Unicode upper-case mapping, so `ı` and `ſ` count as `i` and `s`, `markup.md` M-13 item 3) | `parse5-<view>-manifest` |
| The depth claim of `markup.md` M-26 holds after every token (scripting enabled): the WHATWG stack has at most D + 2 elements, each open element has a DOM depth of at most D + 2, each element that the tree builder closes at once at most D + 3 | `depth-invariant` |
| With the view `disabled`: the same claim in the scripting-disabled tree. The counter runs on the tokens of the `noscript` content C as they come (the pass of `markup.md` M-11 item 8, H-71), and after the end tag `noscript` it continues with the main-pass state | `depth-invariant-disabled` |
| With `--expected` only: the document has none of the rule classes below (the golden fixtures implement the rules of the security review) | `new-rule` |

`src/depth.mjs` computes the depth counter D of M-26 from the parse5 tokens (a `Parser` subclass that sees each tokenizer token once), and it classifies each document by the rules of the security review. The counter applies the implied closure of `markup.md` M-09 (H-85a): before a table part or `col` start tag in HTML context, while F is not an allowed F, it applies the end tag of F, at most three times; not in the `noscript` content C (the copy of M-11 item 8 and the scripting-disabled view of C). The counter does not know blocks; a valid document has no closure shape in block content, so the counter has the D of the rule on every valid document. A closure of a row, a row group or a column group while F is not the current node sets the class `M-09-table-current`. The counter follows H-75: the end tag of the region root (the `svg` or `math` element pushed in HTML context) sets no special-crossed mark for its crossing. For the `noscript` content C, `checkNoscriptContent` also runs the main-pass counter on a copy (M-11 item 8, H-71), so the classes below and D come also from the tokens of C, with the foreign region of C (item 9, H-74):

| Class | Rule | Shape |
|---|---|---|
| `M-06-formatting` | M-06 item 2 (H-26) | A formatting end tag crosses a special or foreign element |
| `M-06-formatting-marked` | M-06 item 4 (H-32) | A formatting end tag pops an element with the special-crossed mark |
| `M-05-stray-table-end` | M-06 item 3 (H-30) | A `tbody`, `thead`, `tfoot`, `tr` or `table` end tag matches nothing |
| `M-11-balance` | M-11 item 5 (H-27, H-75) | The `noscript` content is not balanced (the end tag of a region root of C may cross elements of its region) |
| `M-13-non-ascii` | M-13 item 3 (H-28) | A script `type` value has a non-ASCII character |
| `M-26-depth` | M-26 (H-31) | D is larger than 256 |
| `M-09-template-col` | M-09 condition 1 (H-35) | A `col` whose nearest table-family or `template` element is a `template` |
| `M-09-table-current` | M-09 condition 2 (H-37) | A table part, `col` or `table` start tag while F (a table context) is not the current node |
| `M-10-form` | M-10 item 2 (H-36) | A `form` start tag while an HTML `form` is on the token stack |
| `M-11-table-part` | M-11 item 6 (H-38) | A table part or `col` in the `noscript` content without a `table` opened in it |
| `M-10-template-for` | M-10 item 3 (H-43) | A `template` start tag in HTML context with a `for` attribute (any value) |
| `M-11-document-tag` | M-11 item 7 (H-71) | An `html`, `body` or `head` start or end tag in the `noscript` content |

The classifier is a test oracle for these rules only. It does not decide validity.

parse5 8.0.1 does not implement content patching (`template for`) and reads `<?…>` as a bogus comment, so it never disagrees with a candidate on the shape of `M-10-template-for`. For this class the sweep only counts the documents that a candidate accepts; the evidence for the rule is Chrome 154 (`docs/engineering/protocol/markup-edge-cases.md` §8.5).

`<view>` is `enabled` (scripting enabled, the RQP view of `markup.md` M-04) or `disabled` (scripting disabled: `noscript` content is markup). The default is `enabled`. Use `--scripting both` to check the claim of `markup.md` §5 item 2 too.

The tool also compares the candidates with each other (finding kind `candidates`): validity and blocks must be the same. Error codes are not compared (a document with two errors can give either code, `CONTRACT.md` §5).

When a candidate says **invalid**, the tool does not check it: the rules are stricter than the WHATWG parser on purpose. The tool does not measure false rejects, except through the `candidates` comparison.

## 2. Install

Do these steps once, in `tools/rqp-markup/crosscheck`:

1. Install Node.js 22 or later.
2. Run `npm ci --ignore-scripts`.
3. Build the candidates that you want to check (see the `README.md` of each candidate). The tool does not build them, unless you give `--build`.

## 3. Commands

Run the commands in `tools/rqp-markup/crosscheck`. The candidates come from `../candidates.json`.

```sh
# All golden fixtures, three candidates, both scripting views
node src/cli.mjs check --only rust,java,php --scripting both

# One document: prints the parse5 views and the result of each candidate
node src/cli.mjs check --input path/to/input.html --only rust

# The golden fixtures themselves: every valid expected.json against parse5
node src/cli.mjs check --expected --scripting both

# Results that you already have (one JSON line per fixture, with the field "fixture")
node src/cli.mjs check --fixtures path/to/dir --results results.jsonl

# The edge-case study inputs (cases/edge-cases.json): one row per case
node src/cli.mjs cases --only rust,java,php

# The inputs of the security review (repro inputs, controls, open question, depth shapes)
node src/cli.mjs cases --cases cases/review-cases.json --only rust,node,java,php
node src/cli.mjs cases --cases cases/review2-cases.json --only rust,node,java,php

# The conformance study (round 3): WHATWG steps, cases with the fields step and expected
node src/cli.mjs cases --cases cases/conformance-cases.json --only rust,node,java,php

# The shapes of the WP-17 S3 decisions H-71, H-73, H-74 and H-75 (fields decision and expected)
node src/cli.mjs cases --cases cases/s3-cases.json --only rust,node,java,php

# The shapes of the WP-17 S4 decisions H-78, H-84 and H-86 (fields decision and expected)
node src/cli.mjs cases --cases cases/s4-cases.json --only rust,node,java,php

# The shapes of the WP-17 S4 decision H-85a, the implied closure of M-09 (fields decision and expected)
node src/cli.mjs cases --cases cases/s4b-cases.json --only rust,node,java,php

# A seeded generator sweep
TMPDIR=/dev/shm node src/cli.mjs sweep --only rust,java,php --seed 5 --count 100000 --chunk 10000 --scripting both

# A sweep with the shapes of the security review
TMPDIR=/dev/shm node src/cli.mjs sweep --only rust,node,java,php --seed 21 --count 50000 --chunk 10000 --max-tokens 20 --profile review --scripting both
```

| Option | Commands | Meaning | Default |
|---|---|---|---|
| `--only a,b` | all | Candidates to run | all in `candidates.json` |
| `--candidates file` | all | Candidate list | `../candidates.json` |
| `--timeout s` | all | Limit for each build and each batch run | 300 |
| `--build` | all | Run the `build` command of each candidate first | off |
| `--scripting v` | `check`, `sweep` | `enabled`, `disabled` or `both` | `enabled` (`cases` always uses both) |
| `--fixtures dir` | `check` | Directory with `<name>/input.html` | `fixtures/rqp/markup` |
| `--input file` | `check` | One document | — |
| `--results file` | `check` | Use these results instead of running candidates | — |
| `--expected` | `check` | Use the `expected.json` of each fixture as the result: checks the golden fixtures themselves | — |
| `--cases file` | `cases` | JSON list of `{ "id", "input" }` | `cases/edge-cases.json` |
| `--seed n` | `sweep` | Seed of the sweep (0 … 2^32 − 1) | 1 |
| `--count n` | `sweep` | Number of documents (at most 10 000 000) | 1000 |
| `--max-tokens n` | `sweep` | Pieces per document (1 … 200) | 16 |
| `--profile p` | `sweep` | `classic` (the documents of earlier sweeps, unchanged), `review` (adds the shapes of the security review, unchanged since round 1), `review2` (adds the shapes of round 2: form pointer cycles, template with `col` and raw text, foster-parented elements in tables, `noscript` in cells with table parts) or `review3` (adds the `template for` motif of H-43: processing-instruction markers in or around a block, then a `template` with `for`, or a control; `review` and `review2` documents do not change) | `classic` |
| `--chunk n` | `sweep` | Documents per batch run | 2000 |
| `--out dir` | `sweep` | Where to save the inputs of findings | `$TMPDIR/rqp-crosscheck-sweep-<seed>` |
| `--keep n` | `sweep` | Saved and printed findings for each kind | 20 |

### Output and exit code

- `check` prints one line for each finding, then one summary line for each candidate.
- `check --input` also prints a line `depth:` (max D, WHATWG stack, DOM depth, rule classes).
- `cases` prints a table: case, parse5 with scripting on, parse5 with scripting off, depth (max D, WHATWG stack, DOM depth, rule classes), depth with scripting off, one cell for each candidate (`valid <n>` or the error code, plus `DISAGREE(<views>)`; `depth` or `depth-disabled` for a depth-invariant finding).
- `sweep` prints the first findings of each kind, a table (documents, valid, valid with blocks, findings for each candidate), the number of documents in each rule class, one line for each finding kind (findings without a rule class, findings that only the open question explains, findings by class), the depth invariant line (documents with a violation, and how many of them a candidate accepts; a second line for the scripting-disabled view when it is checked), and the total for each finding kind.

| Exit code | Meaning |
|---|---|
| 0 | No finding, and every candidate ran |
| 1 | At least one finding, or a candidate did not run (`cases`: only the second) |
| 2 | Usage error or input error |

### Reproduce a sweep finding

The sweep saves `<out>/<kind>-<candidate>-<seed>-<index>/input.html` and `finding.json` (seed, index, `maxTokens`, details). Document `index` of a sweep is `generateDocument(docSeed(seed, index), { maxTokens })` from `src/generator.mjs`; it is the same on every run. To check it again:

1. Run `node src/cli.mjs check --input <out>/<name>/input.html --scripting both`.
2. Look at the lines `parse5:` and `<candidate>:`.

### Speed

The batch runs write one file for each document. On a slow temporary file system this is the slowest step: set `TMPDIR` to a memory file system (`/dev/shm` on Linux). Measured in the WP-17 sandbox (2026-10-09) with `TMPDIR=/dev/shm`: 200 000 documents with the Rust candidate in about 1.5 minutes; 100 000 documents with the Rust, Java and PHP candidates and both views in about 1.5 minutes.

## 4. Limits

1. parse5 is one implementation of the WHATWG parser, not the specification. A false accept that parse5 shares with a candidate is not found. Known differences of parse5 8.0.1 (RESEARCH, see `docs/engineering/protocol/markup-edge-cases.md`):
   - `<![CDATA[` in an SVG or MathML integration point (`desc`, `mi`, …) gives a comment in parse5. The WHATWG check is on the namespace of the adjusted current node, which gives a CDATA section there. The end byte is the same when `markup.md` M-12 holds.
   - `<?` gives a bogus comment in parse5. The WHATWG standard parses processing instructions since whatwg/html PR #12118 (merged 2026-06-25).
2. parse5 needs time that grows faster than the nesting depth (about 25 s for 50 000 nested elements). Sweep documents are small; do not run the tool on large untrusted documents in a time-limited job.
3. The generator (`src/generator.mjs`) is tag soup with list and table patterns. It does not reach every shape. A sweep without findings is evidence, not a proof. The `classic` profile did not reach the shapes of the security review (2026-10-11); the `review` profile adds them.
4. The depth pass uses the parse5 token stream. For a valid document it is the token stream of `markup.md` M-03/M-04. For a document that breaks M-07 or M-09 (for example tags in an SVG integration point), parse5 builds elements that the token stack does not see, and the invariant can fail. So the `depth-invariant` finding applies only to results that say valid; the sweep also counts the violations on all documents, for information. The check is skipped while a `template` element is open: the claim excludes template contents (H-35). While the WHATWG stack has more than 1024 elements, the check covers only new elements and the current node (bounded time).

## 5. Security

- The tool never prints a document. It prints fixture names, case ids, block ids, offsets and short values (at most 80 characters).
- It starts candidates with argument lists, never through a shell (`../diff.mjs` `runProcess`). Each run has a timeout and an output limit.
- It writes documents only to its temporary directory and to `--out`.
- The generated documents and the cases are synthetic. They contain no real data.

## 6. Tests

Run `node --test` in this directory. The tests use a fake candidate (`test-support/fake-candidate.mjs`), not the real candidates.

| Test file | What it checks |
|---|---|
| `test/utf8.test.mjs` | Strict UTF-8, BOM, UTF-16 to UTF-8 offsets |
| `test/view.test.mjs` | parse5 view: byte offsets (non-BMP, CR LF, BOM), implied ends, template and foreign content, manifests (also Unicode look-alikes), scripting views, limits |
| `test/depth.test.mjs` | Depth counter D (pushes, exact pops, dirty mark, implied parts, `noscript` content in D), the invariant on the review shapes and in the scripting-disabled view, the rule classes, the pass on C (H-71), the foreign region of C (H-74), the crossing of the region root (H-75) |
| `test/compare.test.mjs` | Each kind of disagreement, result shapes, candidate comparison |
| `test/generator.test.mjs` | Deterministic PRNG (known answer), bounded documents, coverage of the needed shapes |
| `test/cli.test.mjs` | Commands, options, exit codes, saved findings, and that the output never contains document text |

## 7. Dependencies

| Package | Version | License | Use |
|---|---|---|---|
| `parse5` | 8.0.1 (exact) | MIT | WHATWG tree builder with source locations |
| `entities` | 8.1.0 (through `parse5`) | BSD-2-Clause | Character references (used by parse5) |

The lock file is `package-lock.json`. Install with `npm ci --ignore-scripts`.
