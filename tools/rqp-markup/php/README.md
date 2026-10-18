# RQP markup parser candidate (PHP)

<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- SPDX-FileCopyrightText: The Rivqen Authors -->

The PHP candidate of WP-17 (decision H-13). It has its own subset of the WHATWG HTML tokenizer with byte offsets and no tree builder. It implements the rules of `docs/engineering/protocol/markup.md` section 2 (rqp/1) and the interface of `../CONTRACT.md`. It has no runtime dependency.

## Run

The command needs PHP 8.3 or later with the `json`, `pcre`, `hash` and `random` extensions (all are always present). It needs no build step and no `composer install`.

```sh
php bin/rqp-markup <input.html>
php bin/rqp-markup --batch <fixtures-dir>
```

| Exit code | Meaning |
|---|---|
| 0 | Results printed (valid and invalid documents) |
| 2 | Usage error or I/O error. The reason is on stderr. |
| 70 | Internal error (a defect). stderr has the exception class and the source line only. |

A defect in the parser for one document fails closed for that document. The command prints the result line `valid: false`, `error: RQP_MARKUP_STRUCTURE`, no blocks and null revisions, reports the defect on stderr (class name and source line, no document content), goes on with the next fixture in batch mode, and exits with 70 at the end. In single mode it prints that line and exits with 70. Only an argument that starts with `--` is an option: `-page.html` is a path.

The command never prints document content to stderr. It reads at most 5 MiB + 1 byte of a file, so rule M-01 applies to any file size.

The command reads only regular files (symbolic links are followed). A single-mode path, or a batch `input.html`, that exists but is not a regular file (FIFO, device, directory, socket) is an I/O error: exit code 2, and the command does not wait on a FIFO. A batch subdirectory without `input.html` is skipped (CONTRACT section 2, item 7).

To compare with the golden fixtures, run this command from the repository root:

```sh
node tools/rqp-markup/diff.mjs --only php
```

## Test

Do these steps in this directory:

1. Install the dev tools: `composer install`.
2. Run the tests: `vendor/bin/phpunit`.
3. Run the static analysis: `vendor/bin/phpstan analyse`.
4. Optional: run the fuzz target for 10 minutes: `php tests/fuzz.php --seconds 600`.
5. Optional: run the mutation test: `vendor/bin/infection --git-diff-lines --git-diff-base=main --min-msi=80 --threads=max`. Infection needs a coverage driver (PCOV, Xdebug or phpdbg).

`vendor/bin/phpunit --exclude-group limits` skips the 5 MiB resource tests (about 45 s).

Mutation result of T-09 (WP-17 S3; Infection 0.35.6, changed lines against `7f36912`, 4 threads, `--min-msi=80`): 108 of 113 mutants detected (95.6 %: 107 killed, 1 time-out). Before two new tests in `tests/ForeignRegionRulesTest.php` it was 106 of 113 (93.8 %). The new tests check the rule ID of an end tag in a region of C whose root is directly above the `noscript` element, and that M-13 comes before the M-11 checks for a reserved `script` in C. The 5 survivors are equivalent:

| Location | Mutant | Reason |
|---|---|---|
| `Parser::nearestIndex()` | `count - 1` → `count - 2` | Both callers call it only when the top element is not the searched name |
| `Parser::noscriptEndTag()` | `throw` removed at the document-tag check | A document tag is never open in C, so the next check throws with the same rule M-11 |
| `Parser::noscriptEndTag()` | `$top <= $noscript` → `<`, and its `throw` removed | At `$top === $noscript` the current node is the `noscript` element, so the name check throws with the same rule M-11 |
| `Parser::noscriptEndTag()` | `$noscript + 1` → `$noscript + 0` | The element at `$noscript` is `noscript`, and its end tag does not reach this check |

| Test file | What it checks |
|---|---|
| `tests/TokenizerTest.php` | Token spans in each tokenizer state |
| `tests/ReferenceTokenizerTest.php` | Differential test against `tests/Fuzz/ReferenceTokenizer.php`, a literal one-character-at-a-time transcription of the WHATWG states (with CR/LF preprocessing, all DOCTYPE states and the processing instruction states): exhaustive short comment bodies, dense inputs per state, 100 000 random inputs |
| `tests/RulesTest.php` | Each rule M-01 to M-27, each item of M-06 (also items 2 to 4), M-09 (also conditions 1 and 2), M-10 (also items 2 and 3), M-11 (also items 5 and 6), M-13 (also item 3) and M-24 (also j and k), each edge case E1 to E8, and each error code (error code and rule ID). Invalid cases and valid neighbors for each rule of the security review, with the review repros. For M-26: pairs of documents at D = 256 (valid) and D = 257 (`LIMIT`) for each step of the depth counter |
| `tests/JsonValidatorTest.php` | RFC 8259 rules of M-25 |
| `tests/RevisionTest.php` | Hashes and revisions of `CONTRACT.md` section 4 |
| `tests/FixturesTest.php` | Every fixture of `fixtures/rqp/markup/` against `expected.json` |
| `tests/CliTest.php` | Command line, exit codes, batch order, bounded read |
| `tests/PropertyTest.php` | Seeded random documents: invariants, and an oracle for generated valid documents. Model tests: D (M-26) for random sequences of shapes with known units, the balance of random `noscript` content (M-11 item 5; an M-09 error of the main rules (item 8) is also accepted), and table parts in random balanced `noscript` content (M-11 item 6) |
| `tests/LimitsTest.php` | Linear time and bounded memory on adversarial 5 MiB inputs within D ≤ 256 (also names with one PHP string hash, and deep `noscript` content); fast `M-26` stop for inputs where D gets larger than 256, also for `noscript` content (M-11 item 8: it counts in D); at most 32 MiB for 5 MiB of `noscript` content that only opens elements |
| `tests/ForeignRegionRulesTest.php` | WP-17 S3 rules (H-71, H-73, H-74, H-75): foreign regions in block content (M-22 item 5, M-23), `svg` in `noscript` content (M-11 items 5 and 9), the main pass on that content (M-11 item 8), the root end tag and M-06 item 4. Each invalid case checks the rule ID |
| `tests/fuzz.php` | Fuzz target (time-limited; writes a failing input to a file) |

### Dev tools

All dev tools are pinned to exact versions in `composer.json` and `composer.lock`. All packages in the lock file have the license MIT, BSD-3-Clause or Apache-2.0 (Allowed class).

| Tool | Version | License |
|---|---|---|
| PHPUnit | 12.5.38 (the newest major that runs on PHP 8.3) | BSD-3-Clause |
| PHPStan | 2.3.1, level max | MIT |
| Infection | 0.35.6 | BSD-3-Clause |

## Design

### Tokenizer (`src/Tokenizer.php`)

The tokenizer follows the state descriptions of the WHATWG HTML Living Standard, section 13.2.5. It works on bytes: every character that changes a state is ASCII. These states are implemented:

| States | Notes |
|---|---|
| Data, tag open, end tag open, tag name | `</>` is dropped. `<` before a non-letter is text. A tag that reaches EOF is dropped. |
| Before/after attribute name, attribute name, before attribute value, attribute value (double-quoted, single-quoted, unquoted), after attribute value (quoted), self-closing start tag | Raw values (no character reference decoding, DL-010). The first attribute of a name wins. |
| Markup declaration open, comment start, comment start dash, comment, comment less-than sign (and bang, bang dash, bang dash dash), comment end dash, comment end, comment end bang | `<!-->`, `<!--->`, `--!>` |
| Bogus comment | `<!x>`, `</ x>` |
| Processing instruction (or bogus comment for `<?`) | Ends at the first `>` |
| DOCTYPE | Ends at the first `>` (every DOCTYPE state does, also in quoted identifiers) |
| CDATA (M-12) | One token from `<![CDATA[` to the first `>`, or to EOF when there is no `>`. The flag `cdataSameEnd` says if a CDATA section and a bogus comment end at the same byte: the `>` ends a `]]>`, or there is no `>` (E4). |
| RCDATA, RAWTEXT, script data with escape and double-escape states, PLAINTEXT, and their end tag states | Set by the parser after a start tag (M-04) |

Equivalences that the code uses (each one keeps the token boundaries):

- CR is white space. The WHATWG input preprocessing turns CR and CR LF into LF; in every state that reads white space, the LF after a CR is white space too.
- Character tokens are not emitted. No rule depends on them.
- Character references are not decoded. No character reference state reads `<`, `>`, a quote or white space.
- Only `data-rq-block` and `type` attribute values are kept. Memory per token is constant.

`tests/ReferenceTokenizerTest.php` checks these equivalences: the reference tokenizer does not use them, and both must give the same tokens.

### Parser (`src/Parser.php`)

One pass over the tokens with the token stack of M-05 and the depth counter D of M-26 (H-31). The stack is a list of names and an array of flags by stack index. The flags of an element are: pushed in a foreign region, pushed in block content, dirty (M-26), special-crossed mark (M-06 item 4), and its implied parts (row group, row, column group; M-26 item 3).

The depth counter D works as `markup.md` M-26 says:

- Each push adds 1. Each implied part that F (M-09) does not have yet adds 1, also for `col`, which is not pushed.
- An end tag that pops exactly the current node, when that element is not dirty and is not `html` or `body`, subtracts 1 and the number of its implied parts.
- Every other pop keeps D and makes the new current node dirty.
- When D gets larger than 256, the result is `RQP_MARKUP_LIMIT` with rule `M-26`. This applies in HTML context and in a foreign region, inside and outside blocks.

The check is done before the element goes on the stack. Units come back only when their own element is popped exactly, so every element on the stack holds at least 1 unit: the stack never has more than 256 elements. Elements that M-05 does not push (void elements, self-closing `svg` and `math` in HTML context, self-closing elements in a foreign region) do not count. The tags in `noscript` content run through the main pass and count (M-26 item 7, H-71); after the end tag `noscript`, D has the value of the main pass before the content (the pass saves and restores D and the dirty mark of the `noscript` element, DL-044 c). The `noscript` element counts.

The start tag rules of round 2 of the security review need no search. They run in HTML context, before the implied parts and the push:

| Rule | Check |
|---|---|
| M-09 condition 1 (H-35) | A `col` start tag while the top entry of the table index stack (`table`, table parts, `template`) is a `template` is an error |
| M-09 condition 2 (H-37) | A start tag in `TABLE_PART`, `col` or `table` while F is `table`, `tbody`, `thead`, `tfoot`, `tr` or `colgroup`: the stack index of F must be the index of the current node |
| M-10 item 2 (H-36) | A count of the `form` elements pushed in HTML context that are on the stack. Every pop changes it, also a pop by a crossing; a `form` pushed in a foreign region does not count. A `form` start tag in HTML context while the count is not 0 is an error |
| M-10 item 3 (H-43) | A `template` start tag in HTML context with an attribute named `for` (ASCII case-insensitive, any value, any occurrence) is an error. The tokenizer keeps the first `for` (WATCHED). A `template` in a foreign region is not checked. A `template` in `noscript` content is already M-11 |

The end tag rules of the security review use the elements that the pop visits anyway:

| Rule | Check |
|---|---|
| M-06 item 2 (H-26) | An end tag with a name in `FORMATTING` must not cross an element in `SPECIAL` (HTML context) or a foreign element |
| M-06 item 3 (H-30) | In HTML context, an end tag `tbody`, `thead`, `tfoot`, `tr` or `table` with no match on the stack is an error |
| M-06 item 4 (H-32, H-75) | After the pop, the new current node gets the mark when a crossed element is special, or foreign and the matching element is not the region root, or a popped element has the mark. An end tag with a name in `FORMATTING` must not pop a marked element |
| M-11 items 5 and 8 (H-27, H-71) | The tokens of C run through the rules of the main pass on the main stack (`Parser::checkNoscript()`). Before the main rules, each end tag of C must have the name of the current node, which must be above the `noscript` element. Exception (H-75, DL-044 d): `svg` whose nearest `svg` is the region root. The stack must be back at the `noscript` element at the end of C. The stack of C is part of the stack of the main pass, so D ≤ 256 bounds it |
| M-11 item 6 (H-38) | The count of `table` elements on the stack minus the count at the `noscript` start tag. A start tag in `TABLE_PART` or `col` in C with a count of 0 is an error |
| M-11 items 2, 3, 7, 9 (H-71, H-74) | `NOSCRIPT_FORBIDDEN` (no `svg`: it starts a region of C, the main rules of M-05 item 4 give the region) and `NOSCRIPT_DOCUMENT_TAGS` (`html`, `body`, `head`: start and end tags) |
| M-22 item 5, M-23 (H-73, H-75) | `FOREIGN_ALLOWED` check on each start tag in a region of the content; the end tag of the region root closes the region: one search for the nearest element with that name (`Parser::nearestIndex()`), then the pops |
| M-13 item 3 (H-28) | `strspn()` with the 128 ASCII bytes finds a byte ≥ 0x80 in the raw `type` value of a `script` start tag. No PCRE: an error of `preg_match()` must not let a value pass |

`FORMATTING`, `SPECIAL` and the stray table end tags are constant arrays in `src/Names.php`.

These structures make the work for each token constant:

| Structure | Use |
|---|---|
| Count of open elements by name key (see "Hash flooding") | Ignore an end tag without a match in O(1) (M-05 step 3) |
| 11 counters by slot for the names that M-19 and M-24 query (`template`, `select`, `button`, `a`, `nobr`, `rb`/`rp`/`rt`, `rtc`, `h1`…`h6`, `ul`/`ol`/`menu`/`dl`, `table`, `ruby`): open elements and elements opened in the content | "Is open" and "opened in the content" checks of M-19 and M-24 |
| Stack of indexes of `table`, table parts and `template` | F of M-09 and M-19 |
| Stack of indexes of the lists (`ul`, `ol`, `menu`, `dl`) and list items (`li`, `dd`, `dt`) opened in the content | M-24 item j: the top entry is a list item |
| Index of the foreign region element | M-05 step 4, M-07 |
| Index of a foreign INTEGRATION or TEXT element | M-07 items 3 and 4 |
| Count of the HTML `form` elements on the stack | M-10 item 2 |
| Flags of each stack element (dirty, mark, implied parts) | M-26 and M-06 item 4: set and checked only on the popped elements and the new current node |

An end tag that matches an element pops the elements above it. Each element is pushed once and popped once (`array_pop()`, O(1); `array_splice()` would copy the stack for each pop), so all end tags together cost O(number of start tags). M-24 item k uses F of M-09 (the top of the table index stack). The M-11 check tokenizes the content of each `noscript` once more, applies M-13 to its start tags (E8), checks the balance (item 5) and the table parts (item 6), and runs the main rules on its tokens (item 8) on the main stack. The JSON validator (`src/JsonValidator.php`) is iterative and builds no value. UTF-8 validity (M-02) is one PCRE check (`preg_match('//u')`) that cannot backtrack.

### Hash flooding

PHP hashes string array keys with DJBX33A. This hash has no seed, so an attacker can make many tag names with the same hash. Example: `a~` and `b]` have the same hash (33 × 97 + 126 = 33 × 98 + 93), so all names made of these two units have the same hash. In a PHP array with such keys, each lookup walks one long collision chain. Before the fix, 131 072 nested names of this kind (4.5 MiB) took 34.8 s; names of the same shape without collisions took 0.34 s.

The parser has no array whose keys are raw tag names from the input:

| Array | Keys | Why the chain is short |
|---|---|---|
| Name index (M-05 step 3) | Name + 8-byte xxh3 hash of the name with a random 192-byte secret, new for each parse | DJBX33A(name + h) = DJBX33A(name) × 33⁸ + (sum over the bytes of h). Names with the same DJBX33A hash get random buckets. The key contains the full name, so the match stays exact. |
| Slot counters | Integers 0 to 10 | No string keys. The slot of a name comes from a constant array: a lookup probes one chain of constant keys. |
| Name sets (`src/Names.php`, `Tokenizer::WATCHED`) | Constant names | Same: one chain of constant keys |
| Block ids | At most 256 ids (M-26) of `[a-z0-9_-]` (M-15) | Bounded |
| Watched attributes of a token | `type`, `data-rq-block` only | Bounded |
| Stack of the `noscript` content (M-11 item 5) | None (the stack of the main pass) | An end tag compares with the top entry only, or finds the region root with one search |

With the fix and the depth limit removed (a test build only), the 131 072 nested names took 0.30 s (0.33 s without collisions). With the depth limit, the same input stops at the 257th start tag. D ≤ 256 also bounds the name index to 256 keys. The xxh3 hash adds about 0.15 µs for each push, pop and end tag lookup.

### Limits

Measured on PHP 8.5.11 with inputs of up to 5 MiB on 2026-10-11 (after the depth counter D) and on 2026-10-12 (the rows of the `noscript` content and of round 2, on a machine with a high load: the times are upper values). `tests/LimitsTest.php` checks that each valid input takes less than 30 s and 256 MiB, and that each input where D gets larger than 256 stops with `M-26` in less than 2 s. A crossing keeps its units of D, so all crossings of one document pop at most 256 elements together; a repeated crossing is a `LIMIT` input.

| Input | Time | Peak memory |
|---|---|---|
| `<b>` × 256, then `</b>` × 256, repeated | 3.3 s | 5 MiB |
| Stacks of 256 elements with different names, repeated | 1.3 s | 5 MiB |
| `<b>` × 256, then `</i>` × 1.3 million (end tags without a match) | 1.8 s | 5 MiB |
| `<html>`, `<div>` × 254, `</html>` (one end tag pops 255 elements), then `<b></b>` at D = 256, repeated | 3.4 s | 5 MiB |
| `<svg>`, then `<g>` × 255 and `</g>` × 255, repeated | 3.3 s | 5 MiB |
| `<table><tr><td>` × 64 (D = 256: table, row group, `tr`, `td`), then `</td></tr></table>` × 64, repeated | 2.4 s | 5 MiB |
| `<div>` × 251, `<table>`, then `<tr><td>a</td></tr><col>` repeated (implied parts once per table) | 2.6 s | 5 MiB |
| `<p><span><div></span>` (marked `p`), then `<b><i></i></b>` repeated | 3.4 s | 5 MiB |
| `<noscript>`, `<ab>` × 255, `</ab>` × 255, repeated (balanced content at D = 256, M-11 item 8) | see `tests/LimitsTest.php` | 5 MiB |
| `<noscript>` with stacks of 255 nested names with one DJBX33A hash, closed in reverse order, repeated | see `tests/LimitsTest.php` | 5 MiB |
| `<noscript><table>`, then `<tr><td></td></tr>` repeated (M-11 item 6) | 2.3 s | 5 MiB |
| `<table>`, then `<tr><td><form></form></td></tr>` repeated (M-09 condition 2, M-10 item 2) | 2.9 s | 5 MiB |
| `<noscript>`, then `<ab>`, `<a>` or `<A>` × 1.3 to 1.7 million and no end tags (`M-26` at the 256th element; the copy of C is the only memory that grows) | < 2 s | < 32 MiB |
| Five blocks of almost 1 MiB with 127 nested `<ul><li>` groups, repeated (M-24 item j) | 3.0 s | 1 MiB |
| Names with one DJBX33A hash in stacks of 256, repeated | 0.3 s | 5 MiB |
| Names with one DJBX33A hash: 256 open, then end tags of other such names | 0.2 s | 5 MiB |
| Names with one DJBX33A hash in five blocks of almost 1 MiB | 0.4 s | 1 MiB |
| 2.6 million attributes in one tag | 0.8 s | 5 MiB |
| 1.7 million nested `<b>`, 131 072 nested names with one hash, `<span><div></span>`, `<body></body>` or `<table><tr><td>a</table>` repeated (`M-26`) | < 0.01 s | < 1 MiB |

The peak memory is the memory that the parse adds to the input string. `bin/rqp-markup` sets `memory_limit` to 512 MiB.

### Edge cases E1 to E8

The human decided these cases (decisions H-21 and H-22 in the WP-17 S1 sprint record; study: `docs/engineering/protocol/markup-edge-cases.md`). The candidate implements the decided reading. The fixtures `FX-RQ-MARKUP-EDGE-*` and `tests/RulesTest.php` check each one.

| Case | Input | Result |
|---|---|---|
| E1 | `<` as text in a foreign text element or integration point (`<svg><desc>a < b</desc>`) | Valid: the tokenizer emits no tag for it |
| E2 | End of input in a foreign region, after all blocks | Valid |
| E3 | `noscript` without an end tag | C of M-11 is the rest of the input; M-11 and M-13 apply to it |
| E4 | `<![CDATA[` without `>` before EOF | Valid: the token ends at EOF and M-12 holds |
| E5 | CDATA section in a foreign INTEGRATION or TEXT element (`<svg><style><![CDATA[a{}]]></style>`) | Valid when M-12 holds. Comments, tags and DOCTYPE stay invalid there (M-07). |
| E6 | `<?` that reaches EOF | A comment token to EOF (invalid in foreign text content, M-07) |
| E7 | `frameset` start tag in a foreign region | Invalid (M-10, every context) |
| E8 | `script` start tag with a reserved `type` in `noscript` content | Invalid, `RQP_MARKUP_RESERVED` (M-13). A `script` tag that is raw text in C (for example inside `style`) is not a start tag and is not checked. |

### Other choices

| Case | Choice | Reason |
|---|---|---|
| `select` in M-19 | Kept, but not reachable | M-08 rejects a block start tag inside `select` first |
| Order of the M-26 depth check | Implied parts and the push, after the other checks of the start tag | `markup.md` section 2.10: "On each push to the token stack and each implied part". A fixture has one error only, so the order does not change a fixture result. |
| Order of the end tag checks | M-07, then M-06 items 1, 2 and 4, then the pop, D and the marks | `markup.md` section 2.10. All items of M-06 give rule ID `M-06`; all items of M-11 give `M-11` |
| Memory of the M-11 item 5 stack | Bounded by D | `markup.md` M-26 item 7 (H-71): C counts in D and runs on the stack of the main pass, so the stack has at most 256 entries. Earlier (S1, S2) the stack of C was a separate packed stack of name offsets with up to 1.7 million entries (17 MiB for 5 MiB of C; `src/OffsetStack.php`, removed in S3). The copy of C for its tokenizer remains: it is at most 5 MiB |
| State of the pass of M-11 item 8 | In place, restored at the end | C closes every element it opens (item 5), so the stack is as before. Only D and the dirty mark of the `noscript` element change; the parser saves and restores both (DL-044 c). No copy of the stack, so the cost is O(tokens of C) |
| Key of the name index | xxh3 with a random secret from `random_bytes()`, new for each parse | `hash` and `random` are always present in PHP 8.3, so there is no new dependency. SipHash (`sodium_crypto_shorthash()`) is faster (0.10 µs against 0.15 µs) but needs the optional `sodium` extension. |

## Related

- `../CONTRACT.md`: command line and result object
- `../README.md`: differential runner
- `docs/engineering/protocol/markup.md`: normative rules
