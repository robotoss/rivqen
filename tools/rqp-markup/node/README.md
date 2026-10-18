# RQP markup parser candidate: Node.js

<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- SPDX-FileCopyrightText: The Rivqen Authors -->

The Node.js candidate of WP-17 S1 (tasks T-04, T-08, T-09, T-13 and T-16). It implements the RQP markup rules rqp/1 (`docs/engineering/protocol/markup.md` section 2, rules M-01 to M-27, with M-24 items j and k of decision H-18) and the interface of `../CONTRACT.md`. It reads the edge cases E1 to E8 as the human decided them (H-21, H-22; see [Edge cases E1 to E8](#edge-cases-e1-to-e8)). It implements the rules of the security review (H-26 to H-28, H-30 to H-32) and of its round 2 (H-35 to H-38); see [Rules of the security review](#rules-of-the-security-review). It is a conformance tool, not the server SDK.

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
| 2 | Usage error or I/O error. The reason is on stderr. An input path that is not a regular file after the open (a FIFO, a device, a directory, a socket) is an I/O error. |
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
| `test/rules.test.mjs` | One group for each rule M-01 to M-27 and each error code; one group for the edge cases E1 to E8; one group for each rule item of the security review (M-06 items 2 to 4, M-11 item 5, M-13 item 3, the depth counter D of M-26) and of round 2 (M-09 conditions 1 and 2, M-10 item 2, M-11 item 6), with the review repros, the review controls and a valid neighbor for each invalid case |
| `test/name-sets.test.mjs` | The name sets of `markup.md` section 2.1 by behavior, with lists copied from `markup.md` as an independent oracle; list items (M-24 j), table context (M-24 k), `FORMATTING` and `SPECIAL` (M-06 items 2 and 4) |
| `test/json.test.mjs` | M-25 JSON grammar, surrogate escapes, depth limit |
| `test/fixtures.test.mjs` | All `FX-RQ-MARKUP-*` fixtures, in process |
| `test/tokenizer.test.mjs` | Tokenizer states; differential test against the parse5 8.0.1 tokenizer (4000 random inputs) |
| `test/tree-agreement.test.mjs` | For each valid random document, in both scripting views of parse5 8.0.1: the same blocks and offsets as the tree builder, no `script` whose decoded type contains `rivqen-manifest` after Unicode upper-case mapping (M-13), and no element deeper than 259 (M-26). The generator also makes the shapes of the security review and of its round 2 |
| `test/property.test.mjs` | No exception on random markup and random bytes; result invariants (content bytes equal input bytes `[start, end)`, template, revisions); a prefix whose elements are all closed exactly does not change the result of the rest (D, marks, name counts and the count of open forms come back to their start values) |
| `test/cli.test.mjs` | Single and batch mode, byte order, exit codes, no content on stderr, bounded read, only regular files (a FIFO without a writer gives exit 2 at once) |
| `test/s4-rules.test.mjs` | Rules of WP-17 S4 (H-78, H-84, H-86): patch-safety elements and raw text in the content, character references in the `script` type, JSON-LD blocks. Each case checks the error code |
| `test/s3-rules.test.mjs` | Rules of WP-17 S3 (H-71, H-73, H-74, H-75): the name sets and the region of block content (item 5 `FOREIGN_ALLOWED` left markup in H-78), M-23 with the region root, M-06 item 4 with the region root, M-11 items 5 and 7 to 9. Each case is an input that only the named rule rejects |
| `test/perf.test.mjs` | `[perf]` 39 adversarial inputs of about 5 MiB finish in less than 10 s each (measured: less than 0.25 s each). Each input has an expected result. Eight go beyond the depth limit and must give `RQP_MARKUP_LIMIT`. Others keep D at 256 again and again (top level, block content, svg, unmatched end tags, one crossing at the limit, implied table parts) and must be valid. Five use the `noscript` content of 5 MiB: nested (`RQP_MARKUP_LIMIT`, D counts C), siblings balanced (valid) and unbalanced at the end (`RQP_MARKUP_STRUCTURE`), and svg icons (valid). Five use the round 2 rules: forms one after another, closed elements before table parts, `col` in a table in a template, tables nested in the `noscript` content, and table parts after the tables of the content are closed (`RQP_MARKUP_STRUCTURE`) |

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

T-09 (WP-17 S3, H-71 to H-75; changed lines of `markup.mjs` and `names.mjs` against `7f36912`, without `--incremental`): 148 of 218 mutants killed before the new tests (68.4 %). The 59 survivors of `names.mjs` were string literals in `FOREIGN_ALLOWED` and `NOSCRIPT_FORBIDDEN`: the old tests iterated the set, so a removed name was not tested. Three tests in `test/s3-rules.test.mjs` fix this (the exact lists of `markup.md` section 2.1, a doctype token in C, an end tag that is not the current node in a `noscript` element at the root of the stack). After them: 210 of 218 (96.3 %). The 8 survivors are equivalent:

| Location | Mutant | Reason |
|---|---|---|
| `contentEnd()` | `this.region >= 0` → `> 0` | A region root is inside a block element, so its stack index is at least 1 |
| `noscriptEnd()` | `this.region >= 0` → `> 0` | A region root in C is above the `noscript` element, so its index is at least 1 |
| `noscriptRejects()` | `<= this.cBase` → `< this.cBase` | The nearest `table` cannot be the `noscript` element itself |
| `scanNoscriptTokens()` | `t.start === contentLength` → `true` | The outer tokenizer ends C at the first end tag `noscript`, so no other one is in C |
| `scanNoscriptTokens()` | `this.charsOnly === null` → `true` | A text-only element takes `</noscript>` as text, so the end tag is never reached while one is open |
| `noscriptEnd()` | `DOCUMENT_TAGS.has(name)` → `false` | Item 7 is also item 5: a document tag is never the current node of C |
| `noscriptEnd()` | `top <= this.cBase` → `top < this.cBase` and → `false` | At `top === cBase` the current node is the `noscript` element, whose name is never the name of this end tag |

T-08 (M-24 j, k, E5, E8; changed lines against `4c90861`): 53 of 54 mutants killed (98.1 %). The survivor is equivalent: it puts a string at the bottom of the list-mark array (`this.listMark = ["Stryker was here"]`). That value is never a list item, and M-24 j reads the array only after M-24 c has found a list opened in the content above it.

With the command runner, Stryker has no coverage for each test. An `--incremental` run after a change of the tests only can keep old "Survived" results. Run without `--incremental` to check new tests.

T-13 (rules of the security review, F-4, F-7; changed lines against `4fff64f`, without `--incremental`): 358 killed and 4 timeouts of 369 mutants (98.1 %). The timeouts are real hangs (for example, without `O_NONBLOCK` the FIFO test waits for its 20 s limit). The 7 survivors:

| Location | Mutant | Status | Reason |
|---|---|---|---|
| `markup.mjs` `isReservedType()` | `i < span.end` → `i <= span.end` | Equivalent | The byte at `span.end` is a quote, white space or `>`: none of them is `&` or ≥ 0x80 |
| `markup.mjs` `isReservedType()` | `>= 0x80` → `> 0x80` | Equivalent | The input is valid UTF-8 (M-02) and the span starts at an ASCII boundary, so the first non-ASCII byte of a value is a lead byte ≥ 0xC2 |
| `markup.mjs` `scanNoscript()` | `if (mode !== undefined)` → `if (true)` | Equivalent | `switchTo(undefined)` gives the default branch of `next()`: the data state |
| `markup.mjs` `close()` | `if (current >= 0)` → `if (true)` | Equivalent | With an empty stack it writes the property `"-1"` of the state array, which is never read |
| `names.mjs` `KNOWN_NAMES` (2) | the interning list → `[]` | Equivalent | Interning changes only which string object is kept, never a result |
| `cli.mjs` `readBounded()` | `if (cause instanceof CliError)` → `if (true)` | Accepted | Reached only by a read error on an open regular file (for example `EIO`); a test needs fault injection |

T-16 (rules of round 2, H-35 to H-38; changed lines against `5224828`, without `--incremental`): 65 of 65 mutants killed (100 %). The first run had one survivor in `scanNoscript()`: `if (name === 'table') tables++` → `if (true) tables++` (every element of the content counts as a table). A behavior test now covers it: `<td><noscript><div><td></td></div></noscript>` is invalid, because with scripting disabled that `td` closes the cell around the `noscript`.

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
| Count of open elements for each name (HTML context) | M-08, M-10 item 2 (`form`), M-19, M-24 d, e, f, h, i |
| Count of elements opened in the block content | M-24 b, c, g, h, i |
| Nearest list (`ul`, `ol`, `menu`, `dl`) or list item (`li`, `dd`, `dt`) for each entry | M-24 j |
| Stack index of the nearest table-family entry for each entry, or `NO_FAMILY` (-1), or `IN_TEMPLATE` (-2) when a `template` entry is nearer | F of M-09 (with conditions 1 and 2), M-19, M-24 k and M-26 item 3 |
| State bits for each entry: dirty, special-crossed mark, implied row group, row and column group | M-06 item 4, M-26 |
| Depth counter D | M-26 |
| Stack index of the foreign region root | M-05 rule 4, M-07 |

Every element is pushed and popped once, so the crossing checks of M-06 and the pops are linear in total. A pop takes constant time: it removes the last entry of each per-entry array and of the stack indexes for its name, and it changes two counters. The nearest match of an end tag is the last entry of the stack indexes for its name, so no end tag searches the stack. A name map removes a name when its last element is popped, so each map has at most one key for each element on the token stack. The first rule violation ends the pass. The check order follows `markup.md` section 2.10.

All checks of an end tag (`close()` in `src/markup.mjs`) are in one loop over the entries that the end tag pops: M-06 item 1 (guarded), item 2 (a `FORMATTING` end tag over a special element pushed in HTML context or over any element pushed in a foreign region) and item 4 (a `FORMATTING` end tag that pops a marked entry). Only after all checks, the entries are popped and D and the marks change. In block content, M-23 allows only exact pops, so this loop has no crossed entry there.

M-11 (H-71, H-74, H-75) scans the `noscript` content C again with a second tokenizer, from the data state, on the bytes of C followed by `</noscript>` (`scanNoscript()`). Each byte is in at most one `noscript` content, so this is linear too. The scan runs **in place**: it uses the same `Scanner` methods (`startTag()`, `endTag()`, `comment()`, `doctype()`) and the same token stack, above the `noscript` element, which is the current node. So M-05 to M-10, M-12, M-13 and the depth counter D apply to the tokens of C without a second implementation (item 8). `cBase`, the stack index of the `noscript` element, marks the scan. It adds only the rules of C:

- `noscriptRejects()` for a start tag: item 2 (`data-rq-block`), item 3 (`NOSCRIPT_FORBIDDEN`, without `svg`), item 7 (`DOCUMENT_TAGS`: `html`, `body`, `head`), and item 6 (a `TABLE_PART` or `col` start tag needs `nearest('table') > cBase`, a table of C that is still open, also in a region). M-13 (item 4) is the check of the main pass.
- `noscriptEnd()` for an end tag: item 7, and item 5: the end tag has the name of the current node, which is an element above `cBase`. The exception of H-75 is an end tag whose nearest element with its name is the region root: it closes the region.
- The first end tag `noscript` ends the scan (item 1): it must start at the end of C, the current node must be the `noscript` element (nothing of C is open), and no text-only element of a region may be open. The end of the input before it, for example in a RCDATA `title` of C, is an error.
- An `svg` start tag in HTML context starts a region by the normal rule of M-05 item 4 (item 9). The tokenizer state does not change in the region (M-04), and the start tag of a self-closing `svg` opens none.

`scanNoscript()` saves D and the state bits of the `noscript` element, and restores them (and the main tokenizer and bytes) at the end, so the main pass continues with the state it had after the `noscript` push. The stack itself is back at the `noscript` element, because C is balanced.

M-24 j is checked after M-24 c. When c holds, a list opened in the content is open, so the nearest list or list item from the top of the stack is always an element opened in the content. The index for j therefore needs no separate "opened in the content" flag.

`src/json.mjs` checks M-25 without recursion: the nesting depth is a counter with the limit 64.

### Limits and memory

1. M-01 is checked first, on the byte length. The CLI reads at most 5 MiB + 1 bytes of a file, so memory stays bounded for any file.
2. M-02 uses `isUtf8()` of `node:buffer` (strict: no overlong forms, no surrogates, no code point above U+10FFFF, no truncated sequence).
3. M-26 depth counter D (H-31, which amends H-25): `addDepth()` adds 1 for each push and 1 for each new implied part of F, and checks `D <= 256` (`LIMITS.maxOpenElements`) before it changes any state. An end tag gives units back only when it pops exactly one clean element that is not `html` or `body`; else the new current node gets the dirty bit. The limit applies in HTML context and in a foreign region, inside and outside blocks. Elements that M-05 does not push (void elements, self-closing `svg` and `math` in HTML context, self-closing elements in a foreign region) do not count, but an implied column group of a `col` does.
4. Each entry of the token stack holds at least 1 of D, so the token stack, its per-entry arrays and its name maps have at most 256 entries. The tags of the `noscript` content count in D (M-26 item 7, H-71): C is scanned on the same stack, so its elements are bounded by 256 too (5 MiB of `<em>` in C gives `RQP_MARKUP_LIMIT`). The scan gives D back at the end of C.
5. The names of the sets of `markup.md` section 2.1 are interned (`internName()` in `src/names.mjs`): for such a name, a stack entry keeps a constant string, not one string per tag (security review F-4). Interning does not change a result.
6. Memory is linear in the input size. The largest measured time at 5 MiB is less than 0.25 s (`test/perf.test.mjs`).

### Edge cases E1 to E8

The human decided these cases (H-21, H-22 in the sprint record `docs/engineering/plan/sprints/WP-17-S1.md`; study: `docs/engineering/protocol/markup-edge-cases.md`). The fixtures `FX-RQ-MARKUP-EDGE-*` and the group "edge cases E1 to E8" of `test/rules.test.mjs` check each one.

| Case | Input | Result | Code |
|---|---|---|---|
| E1 | `<` as a character in foreign text content (`<svg><desc>a < b`) | Valid | The tokenizer emits no token for it |
| E2 | End of input in a foreign region, after all blocks | Valid | `eof()` checks only `noscript`, an open block and `select` |
| E3 | `noscript` without an end tag | C is the rest of the input; M-11 applies | `eof()` calls `scanNoscript()` |
| E4 | `<![CDATA[` without `>` before the end of input | Valid; inside a block, M-20 fails | `isCdataSafe()` |
| E5 | CDATA in a foreign `TEXT` element or integration point | Valid when M-12 holds | `comment()` |
| E6 | `<?` that reaches the end of input | A comment to the end of input (invalid in foreign text content) | Tokenizer, `comment()` |
| E7 | `frameset` start tag in a foreign region | Invalid (`RQP_MARKUP_STRUCTURE`) | `startTag()`, before the context test |
| E8 | Manifest `script` in `noscript` content | Invalid (`RQP_MARKUP_RESERVED`): M-13 applies to the start tags of C | `scanNoscript()` |

An E5 CDATA section that runs to the end of input in foreign text content is valid (E2 and E4 together). No fixture covers this combination; the rules test does.

### Rules of the security review

The human decided these rules after the security review of 2026-10-11 (H-25 to H-32 in the sprint record; rule text: `markup.md` sections 2.4 and 2.8; study: `markup-edge-cases.md` section 6) and after its round 2 (H-35 to H-38; rule text: `markup.md` M-09, M-10, M-11; study: `markup-edge-cases.md` section 7). Task T-13 implemented the first rules, task T-16 the rules of round 2.

| Decision | Rule | Code |
|---|---|---|
| H-26 | M-06 item 2: a `FORMATTING` end tag does not cross a `SPECIAL` element pushed in HTML context or any element pushed in a foreign region | `close()` |
| H-30 | M-06 item 3: in HTML context, `</tbody>`, `</thead>`, `</tfoot>`, `</tr>`, `</table>` must match an element | `endTag()` (`STRAY_TABLE_END`) |
| H-32 | M-06 item 4: special-crossed mark on the new current node; a `FORMATTING` end tag does not pop a marked entry | `close()` (`MARKED` bit) |
| H-27 | M-11 item 5: the `noscript` content is balanced, also when it runs to the end of the input (E3) | `scanNoscript()` |
| H-28 | M-13 item 3: a byte ≥ 0x80 in the raw `type` value of a `script` start tag gives `RQP_MARKUP_RESERVED`, in the main pass and in the `noscript` content | `isReservedType()` |
| H-78 | M-22: `FORBIDDEN_CONTENT` has no patch-safety element (`base`, `basefont`, `bgsound`, `embed`, `iframe`, `link`, `meta`, `object`, `script`, `style`); no `FOREIGN_ALLOWED` check in a region of the content. M-04 raw text works in the content through the tokenizer state of `htmlStart()` (script data with the escaped and double-escaped states, RAWTEXT); the appropriate end tag is the current node (M-23). Raw text is linear: the tokenizer reads each byte once | `names.mjs`, `foreignStart()`, `contentStart()` |
| H-84 | M-13 item 2: a `&` followed by an ASCII letter, or a numeric reference (`&#digits`, `&#x`/`&#X` hex, raw bytes, `;` optional, value capped at 0x110000) with a value 00 to 20, a letter, a digit, `/`, `+`, `-`, `.` or ≥ 0x80. `&#;` and `&#x;` are not references (DL-048 a) | `isReservedType()`, `numericReference()`, `isRiskyCode()` |
| H-86 | M-16: json type is exactly `application/json` or `application/ld+json` (case-sensitive); M-25 applies to both | `isJsonType()` (`JSON_TYPES`) |
| H-31 | M-26: the depth counter D with the dirty bit and the implied parts; it replaces the stack-size limit of H-25 | `addDepth()`, `htmlStart()`, `close()` |
| H-35 | M-09 condition 1: no `col` start tag when a `template` is nearer than every `TABLE_FAMILY` element | `htmlStart()` (`IN_TEMPLATE`) |
| H-36 | M-10 item 2: no `form` start tag in HTML context while an HTML `form` is on the token stack. A crossing that pops the form also decrements its count | `htmlStart()` |
| H-37 | M-09 condition 2: a start tag in `TABLE_PART`, `col` or `table` while F is `table`, `tbody`, `thead`, `tfoot`, `tr` or `colgroup` needs F as the current node | `htmlStart()` |
| H-38 | M-11 item 6: a start tag in `TABLE_PART` or `col` in the `noscript` content needs an open `table` of the content | `scanNoscript()` |
| H-43 | M-10 item 3: no `template` start tag with an attribute named `for` (ASCII case-insensitive, any value, any occurrence) in HTML context. A `template` in a foreign region is not affected. In the `noscript` content `template` is already forbidden (M-11 item 3) | `htmlStart()`, tokenizer flag `hasFor` |

`FORMATTING` and `SPECIAL` are fixed sets in `src/names.mjs`. Not every member of `SPECIAL` changes a result today: void elements are never pushed in HTML context, raw text elements have no end tag in their content, the other `GUARDED` elements and `table` cannot be crossed (M-06 item 1), and `frameset` is rejected (M-10). `test/name-sets.test.mjs` checks the other members by behavior and the full set against the list of `markup.md`.

Choices of this candidate inside the rules:

| Case | Choice | Reason |
|---|---|---|
| End tag in block content | The same `close()` as outside: checks of M-06 items 2 and 4, D, marks | M-23 allows only exact pops of elements that were pushed in the content, so the checks never fail there and D always gives the units back. One code path for all pops. |
| M-11 item 5, start tag `svg` with the self-closing flag | Not handled separately | M-05 item 1 pushes nothing and opens no region; `math` is rejected by item 3. |
| Implied parts and M-09 | M-09 with conditions 1 and 2, and M-10 item 2, are checked first; the implied parts of a rejected start tag are not counted, and it is not pushed | A rejected start tag ends the pass, so the order changes only which error code a document with two errors gets (`CONTRACT.md` section 5). |
| M-09 condition 1 and the F index | The per-entry F index has a second negative value, `IN_TEMPLATE`, instead of a walk down the stack | It keeps the check O(1). All other users of F read only indexes ≥ 0, so they see `IN_TEMPLATE` as "no F", as before. A `template` pushed in a foreign region also sets it; that entry is popped with its region before the next start tag in HTML context, so it never changes a result. |
| M-10 item 2 in a `template` | A `form` start tag in a `template` under an open HTML `form` is rejected | The rule text has no exception for template contents (the tree builder does not use the form element pointer there). Stricter than needed; the rule text decides. |
| M-11 item 6, order in a start tag of the content | Items 4, 2 and 3 first, then item 6, then the push of item 5 | One error ends the check, so the order changes only the error code of a document with two errors. |

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

`typed-rest-client` 2.3.1 (a Stryker dependency for the dashboard reporter) pins `qs` 6.15.1, which has moderate advisories. `package.json` overrides it to `qs` 6.16.0 (BSD-3-Clause; decision H-23, DL-017). `npm audit` reports no advisory. Remove the override when Stryker depends on a fixed `qs`.

## Security

- No `eval`, no `new Function`, no dynamic import.
- Untrusted input never throws: `analyze()` returns a result for every byte sequence (property tests).
- Every loop advances the input position or pops the stack; no recursion.
- The CLI never prints document content to stderr.
- The CLI reads only regular files. It opens the input with `O_NONBLOCK` and checks `fstat` on the descriptor, so a FIFO without a writer cannot make it wait (security review F-7).
- The token stack and its name maps have at most 256 entries (D ≤ 256), so tag names from the input cannot fill a map (hash flooding, security review F-4). The `noscript` content is scanned on the same stack, so it adds no map.
