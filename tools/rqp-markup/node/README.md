# RQP markup parser candidate: Node.js

<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- SPDX-FileCopyrightText: The Rivqen Authors -->

The Node.js candidate of WP-17 S1 (tasks T-04 and T-08). It implements the RQP markup rules rqp/1 (`docs/engineering/protocol/markup.md` section 2, rules M-01 to M-27, with M-24 items j and k of decision H-18) and the interface of `../CONTRACT.md`. It reads the edge cases E1 to E8 as the human decided them (H-21, H-22; see [Edge cases E1 to E8](#edge-cases-e1-to-e8)). It is a conformance tool, not the server SDK.

## Requirements

- Node.js 22 or later. ES modules only.
- No runtime dependency. The development dependencies are only for the tests and the mutation check (see [Dependencies](#dependencies)).

## Build

The differential runner uses this build command. Run it in this directory:

```sh
npm ci --ignore-scripts --silent
```

## Run

```sh
node src/cli.mjs <path-to-input.html>
node src/cli.mjs --batch <fixtures-dir>
```

| Exit code | Meaning |
|---|---|
| 0 | Results are printed (valid and invalid documents) |
| 2 | Usage error or I/O error. The reason is on stderr. |
| 70 | Internal defect: the analyzer threw for an input. The result for that input is printed and fails closed (`RQP_MARKUP_STRUCTURE`). stderr names the input and the error class, never the content. |

To compare the candidate with the golden fixtures, run this command from the repository root:

```sh
node tools/rqp-markup/diff.mjs --only node
```

The library function is `analyze(bytes)` in `src/markup.mjs`. It takes a `Uint8Array` and returns the result object of `CONTRACT.md` section 3. It never throws for a `Uint8Array`.

## Test

1. Run the build command.
2. Run `npm test`.

| File | What it checks |
|---|---|
| `test/rules.test.mjs` | One group for each rule M-01 to M-27 and each error code; one group for the edge cases E1 to E8 |
| `test/name-sets.test.mjs` | The name sets of `markup.md` section 2.1 by behavior, with lists copied from `markup.md` as an independent oracle; list items (M-24 j) and table context (M-24 k) |
| `test/json.test.mjs` | M-25 JSON grammar, surrogate escapes, depth limit |
| `test/fixtures.test.mjs` | All `FX-RQ-MARKUP-*` fixtures, in process |
| `test/tokenizer.test.mjs` | Tokenizer states; differential test against the parse5 8.0.1 tokenizer (4000 random inputs) |
| `test/tree-agreement.test.mjs` | For each valid random document: the same blocks and offsets as the parse5 8.0.1 tree builder (scripting enabled) |
| `test/property.test.mjs` | No exception on random markup and random bytes; result invariants (content bytes equal input bytes `[start, end)`, template, revisions) |
| `test/cli.test.mjs` | Single and batch mode, byte order, exit codes, no content on stderr, bounded read |
| `test/perf.test.mjs` | `[perf]` 14 adversarial inputs of about 5 MiB finish in less than 10 s each (measured: less than 1 s) |

The random tests use fixed seeds. For a deeper run, set more iterations or another seed:

```sh
RQP_FUZZ_ITERATIONS=300000 RQP_FUZZ_SEED=7 npm test        # tokenizer differential
RQP_TREE_ITERATIONS=200000 RQP_TREE_SEED=7 npm test        # tree agreement
RQP_PROP_ITERATIONS=200000 RQP_PROP_SEED=7 npm test        # properties
```

### Mutation check

StrykerJS 10.0.0 with the command runner (`stryker.config.json`). Each mutant runs `npm run test:mutation`: the same tests with fewer random cases, without the `[perf]` tests, and one test file at a time. The mutant timeout is 15 s plus 1.5 times the initial run. With parallel test files and a short timeout, CPU contention made killed mutants show as "Timeout". A full run of all 2097 mutants takes about 1.5 hours on 4 CPUs. Many mutants stop a scanning loop from advancing; they show as "Timeout" and count as detected. Run it in this directory:

```sh
npx stryker run --mutate "$(node ../../mutation/diff-ranges.mjs --base main --ext .mjs --format stryker)" --incremental
```

The reports go to `reports/` (ignored by git).

## Design

### Tokenizer: own, byte level (DL-012)

`src/tokenizer.mjs` implements the WHATWG tokenizer states of `markup.md` section 3.1 on the input bytes. parse5 8.0.1 is used only as a test oracle.

Reasons:

1. **Linear time.** The parse5 8.0.1 tokenizer checks for a duplicate attribute name with a linear search over the earlier attributes of the tag (`getTokenAttr` in `_leaveAttrName`). One start tag with many distinct attributes takes quadratic time: 432 KB took 26 s on the development machine, so a 5 MiB input takes about one hour. The own tokenizer keeps only the first `data-rq-block` and the first `type` attribute and needs no search.
2. **Exact byte offsets.** The input is valid UTF-8 (M-02, checked first). All tokenizer decisions depend on ASCII bytes only, and a UTF-8 multi-byte sequence never contains an ASCII byte. So a scan over bytes finds the same boundaries as a scan over code points, and all offsets are UTF-8 byte offsets without a UTF-16 conversion. A BOM (`EF BB BF`) is not ASCII, so it cannot change a token boundary: offset 0 stays the first byte (M-02).
3. **Raw attribute values (DL-010).** The tokenizer gives the source span of a value. Character references are never decoded.
4. **Control of the state changes (M-04).** The rules code sets RCDATA, RAWTEXT, script data and PLAINTEXT after a start tag. The tokenizer never changes the text mode by itself.

The tokenizer does not emit character tokens, does not decode character references and does not do the CR LF preprocessing (CR counts as white space). None of these change a token boundary (`markup.md` section 3.1). It tokenizes `<![CDATA[` as a bogus comment, which M-04 allows; M-12 is checked on that token. In foreign text content (M-07 rules 3 and 4) such a token counts as a CDATA section, not as a comment (E5).

Evidence that the tokenizer is the WHATWG tokenizer: the differential test against the parse5 tokenizer compares every tag, comment and DOCTYPE token, with names, byte offsets, self-closing flags and attribute values. 300 000 random inputs (3 seeds) gave no difference. The test corrects two parse5 8.0.1 location quirks: the end offset of a comment or DOCTYPE at EOF is one after the input, and the start of a comment or DOCTYPE is one UTF-16 unit late when a non-BMP character follows `</` or `<!`.

### Rules

`src/markup.mjs` makes one pass over the tokens. It keeps the token stack of M-05 as arrays with one entry per element, and these indexes, so that each check is a constant number of map lookups:

| Index | Used by |
|---|---|
| Stack indexes for each name | Nearest match of an end tag (M-05 rule 3) |
| Count of open elements for each name (HTML context) | M-08, M-19, M-24 d, e, f, h, i |
| Count of elements opened in the block content | M-24 b, c, g, h, i |
| Nearest list (`ul`, `ol`, `menu`, `dl`) or list item (`li`, `dd`, `dt`) for each entry | M-24 j |
| Nearest table-family name for each entry, reset at `template` | F of M-09, M-19 and M-24 k |
| Stack index of the foreign region root | M-05 rule 4, M-07 |

Every element is pushed and popped once, so the crossing check of M-06 and the pops are linear in total. The first rule violation ends the pass. The check order follows `markup.md` section 2.10.

M-11 tokenizes the `noscript` content again with a second tokenizer. Each byte is in at most one `noscript` content, so this is linear too. The second pass also checks M-13 on each `script` start tag of the content (E8).

M-24 j is checked after M-24 c. When c holds, a list opened in the content is open, so the nearest list or list item from the top of the stack is always an element opened in the content. The index for j therefore needs no separate "opened in the content" flag.

`src/json.mjs` checks M-25 without recursion: the nesting depth is a counter with the limit 64.

### Limits and memory

1. M-01 is checked first, on the byte length. The CLI reads at most 5 MiB + 1 bytes of a file, so memory stays bounded for any file.
2. M-02 uses `isUtf8()` of `node:buffer` (strict: no overlong forms, no surrogates, no code point above U+10FFFF, no truncated sequence).
3. Memory is linear in the input size: the token stack has at most one entry per start tag. The largest measured time at 5 MiB is less than 1 s (`test/perf.test.mjs`).

### Edge cases E1 to E8

The human decided these cases (H-21, H-22 in the sprint record `docs/engineering/plan/sprints/WP-17-S1.md`; study: `docs/engineering/protocol/markup-edge-cases.md`). The fixtures `FX-RQ-MARKUP-EDGE-*` and the group "edge cases E1 to E8" of `test/rules.test.mjs` check each one.

| Case | Input | Result | Code |
|---|---|---|---|
| E1 | `<` as a character in foreign text content (`<svg><desc>a < b`) | Valid | The tokenizer emits no token for it |
| E2 | End of input in a foreign region, after all blocks | Valid | `eof()` checks only `noscript`, an open block and `select` |
| E3 | `noscript` without an end tag | C is the rest of the input; M-11 applies | `eof()` calls `checkNoscript()` |
| E4 | `<![CDATA[` without `>` before the end of input | Valid; inside a block, M-20 fails | `isCdataSafe()` |
| E5 | CDATA in a foreign `TEXT` element or integration point | Valid when M-12 holds | `comment()` |
| E6 | `<?` that reaches the end of input | A comment to the end of input (invalid in foreign text content) | Tokenizer, `comment()` |
| E7 | `frameset` start tag in a foreign region | Invalid (`RQP_MARKUP_STRUCTURE`) | `startTag()`, before the context test |
| E8 | Manifest `script` in `noscript` content | Invalid (`RQP_MARKUP_RESERVED`): M-13 applies to the start tags of C | `checkNoscript()` |

An E5 CDATA section that runs to the end of input in foreign text content is valid (E2 and E4 together). No fixture covers this combination; the rules test does.

### Other choices

| Case | Choice | Reason |
|---|---|---|
| An internal exception | Fail closed, exit 70 | CONTRACT.md has no code for a defect. |

## Dependencies

All are development dependencies, pinned to exact versions, installed with `--ignore-scripts`. The lock file pins the transitive versions.

| Package | Version | License | Use |
|---|---|---|---|
| `parse5` | 8.0.1 | MIT | Test oracle only (tokenizer and tree builder) |
| `entities` (via parse5) | 8.1.0 | BSD-2-Clause | parse5 dependency |
| `@stryker-mutator/core` | 10.0.0 | Apache-2.0 | Mutation check; resolves `chalk` 5.6.2 |

`npm audit` reports a moderate advisory for `qs` 6.15.1. `typed-rest-client` 2.3.1 (a Stryker dependency for the dashboard reporter) pins this exact version. This package does not use the dashboard reporter, and the tool does not process untrusted input with `qs`.

## Security

- No `eval`, no `new Function`, no dynamic import.
- Untrusted input never throws: `analyze()` returns a result for every byte sequence (property tests).
- Every loop advances the input position or pops the stack; no recursion.
- The CLI never prints document content to stderr.
