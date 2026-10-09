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
| The tree has no `script` element whose `type` contains `rivqen-manifest` (case-insensitive, after character reference decoding) | `parse5-<view>-manifest` |

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

# A seeded generator sweep
TMPDIR=/dev/shm node src/cli.mjs sweep --only rust,java,php --seed 5 --count 100000 --chunk 10000 --scripting both
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
| `--chunk n` | `sweep` | Documents per batch run | 2000 |
| `--out dir` | `sweep` | Where to save the inputs of findings | `$TMPDIR/rqp-crosscheck-sweep-<seed>` |
| `--keep n` | `sweep` | Saved and printed findings for each kind | 20 |

### Output and exit code

- `check` prints one line for each finding, then one summary line for each candidate.
- `cases` prints a table: case, parse5 with scripting on, parse5 with scripting off, one cell for each candidate (`valid <n>` or the error code, plus `DISAGREE(<views>)`).
- `sweep` prints the first findings of each kind, a table (documents, valid, valid with blocks, findings for each candidate) and the total for each finding kind.

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
3. The generator (`src/generator.mjs`) is tag soup with list and table patterns. It does not reach every shape. A sweep without findings is evidence, not a proof.

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
| `test/view.test.mjs` | parse5 view: byte offsets (non-BMP, CR LF, BOM), implied ends, template and foreign content, manifests, scripting views, limits |
| `test/compare.test.mjs` | Each kind of disagreement, result shapes, candidate comparison |
| `test/generator.test.mjs` | Deterministic PRNG (known answer), bounded documents, coverage of the needed shapes |
| `test/cli.test.mjs` | Commands, options, exit codes, saved findings, and that the output never contains document text |

## 7. Dependencies

| Package | Version | License | Use |
|---|---|---|---|
| `parse5` | 8.0.1 (exact) | MIT | WHATWG tree builder with source locations |
| `entities` | 8.1.0 (through `parse5`) | BSD-2-Clause | Character references (used by parse5) |

The lock file is `package-lock.json`. Install with `npm ci --ignore-scripts`.
