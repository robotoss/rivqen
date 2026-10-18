# H-40 relaxation study

<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- SPDX-FileCopyrightText: The Rivqen Authors -->

Study tools of WP-17 S3 task T-01 (H-40): three relaxations of the RQP markup rules (`docs/engineering/protocol/markup.md`), measured with parse5 8.0.1 and on the markup corpus. Report: `evidence/markup-relax/report.md`.

**Status:** DESIGN (study tool). It is not a candidate, not part of the differential runner and not part of CI. It changes no rule: the relaxations exist only behind flags in a study copy of the Node.js candidate.

## 1. Relaxations

| Flag | Relaxation | Rules that change |
|---|---|---|
| `r1` | A balanced `svg` or `math` subtree in the content of an `html` block | M-22 (items 2 and 3 for `svg` and `math`; `script` and `style` stay forbidden in the foreign region) |
| `r2` | `svg` in the `noscript` content C | M-11 item 3 (without `svg`); M-05, M-07 and M-12 in a foreign region of C |
| `r3` | No special-crossed mark for a crossing by the end tag of the region root (`</svg>`, `</math>`) | M-06 item 4; with `r1` also M-23 for that end tag, with `r2` also M-11 item 5 |
| `r3w` | `r3` for every end tag in a foreign region (scope "region") | As `r3` |

A combination name joins flags with `+`, for example `r1+r2+r3`. `base` is the shipped rule set.

## 2. Files

| File | Use |
|---|---|
| `src/markup-relax.mjs` | Study copy of `../../node/src/markup.mjs` with the flags; imports the shipped tokenizer, name sets, JSON check and result object |
| `src/oracle.mjs` | parse5 oracle: the checks of `../../crosscheck` (both scripting views, depth claim) and two tree checks (block content namespace and parent; `noscript` content with scripting disabled) |
| `src/attrs.mjs` | Attribute rule of `manifest-patch.md` §4.2 and the proposed items for foreign content |
| `src/gen.mjs` | Seeded generator of synthetic documents with the shapes of the three relaxations |
| `src/combos.mjs` | Combination helpers |
| `src/agree.mjs` | Agreement run: fixtures, cross-check cases, generator profiles, relaxation sweep |
| `src/corpus-page.mjs`, `src/corpus.mjs` | Corpus run (same method as `../../corpus/run.mjs`) |
| `src/write-relax.mjs` | Writes relaxation sweep documents as a batch directory for the shipped candidates |
| `test/` | Tests (`node --test`) |

## 3. Procedure

Do these steps in the repository root. `DATA` is the corpus data directory (`tools/rqp-markup/corpus/README.md`, steps 4 and 5). `SCRATCH` is a new empty directory outside the repository. Page bytes must not go into the repository.

1. In `tools/rqp-markup/crosscheck` and in `tools/rqp-markup/node`, run `npm ci --ignore-scripts`.
2. Build the four candidates (the `README.md` of each candidate). The Java candidate needs JDK 21 and `gradle` on `PATH`.
3. Run the tests: `node --test "tools/rqp-markup/study/relax/test/*.test.mjs"`.
4. Run the agreement study (about 40 s):
   `node tools/rqp-markup/study/relax/src/agree.mjs --relax 300000 --profile-count 50000 --seed 40 --out evidence/markup-relax/agreement.json`
5. Run the shipped candidates on the relaxation shapes:
   1. `node tools/rqp-markup/study/relax/src/write-relax.mjs --out "$SCRATCH/relax-docs" --seed 40 --count 20000`
   2. In `tools/rqp-markup/crosscheck`, run `node src/cli.mjs check --fixtures "$SCRATCH/relax-docs" --only rust,node,java,php --scripting both`.
6. Run the corpus study (about 2 minutes with 6 jobs):
   `node tools/rqp-markup/study/relax/src/corpus.mjs --data "$DATA" --scratch "$SCRATCH/corpus" --evidence evidence/markup-relax --jobs 6`
7. Delete `$SCRATCH`.

## 4. Outputs

| File | Content |
|---|---|
| `evidence/markup-relax/agreement.json` | For each set and combination: valid, gained, findings by kind (on documents that `base` accepts, and on gained documents), up to 3 synthetic examples |
| `evidence/markup-relax/corpus.jsonl` | One line per corpus page: URL, numbers, the rule ID of the first violation for each combination, oracle finding kinds, attribute name counts |
| `evidence/markup-relax/corpus-summary.json` | Totals of the corpus run |

The outputs have URLs, numbers, rule IDs, tag and attribute names and synthetic documents only.

## 5. Safety of the data

- The scripts never execute page content: they tokenize bytes and parse them with parse5 (no scripting, no network).
- `corpus-page.mjs` writes variant documents only to the directory given with `--variants-dir` (inside `SCRATCH`).
- The test `corpus-page: the output has numbers and names only` checks that page text does not appear in the output.
