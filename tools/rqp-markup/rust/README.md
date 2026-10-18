# RQP markup parser candidate — Rust

<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- SPDX-FileCopyrightText: The Rivqen Authors -->

The Rust candidate of WP-17 S1 (tasks T-03, T-08, T-09, T-11, T-13 and T-16) and S3 (task T-04: the markup rules H-71, H-73, H-74 and H-75). It implements the rules M-01…M-27 of `docs/engineering/protocol/markup.md` (rqp/1), with M-24 items j and k (human decision H-18), the decided edge cases E1…E8 (H-21, H-22), the rules of the security review (M-06 items 2 to 4: H-26, H-30, H-32; M-11 item 5: H-27; M-13 item 3: H-28), the rules of security review round 2 (M-09 condition 1: H-35; M-10 item 2: H-36; M-09 condition 2: H-37; M-11 item 6: H-38), the depth counter D of M-26 (H-25, H-31), and the interface of `tools/rqp-markup/CONTRACT.md`. It is a tool for the differential test, not the production crate (`rivqen-proto`, P3).

## Build

Run these commands in `tools/rqp-markup/rust`:

1. Install Rust stable 1.85 or later.
2. Run `cargo build --release`.
3. The command is `target/release/rqp-markup`.

## Run

```sh
target/release/rqp-markup <input.html>          # one JSON line
target/release/rqp-markup --batch <fixtures-dir> # one JSON line per fixture, with "fixture"
```

| Exit code | Meaning |
|---|---|
| 0 | Results are printed (valid and invalid documents) |
| 2 | Usage error or I/O error; the reason is on stderr |
| 70 | Internal defect: for at least one input the analysis found an internal error (for example an offset outside the input). The result line for that input fails closed (`RQP_MARKUP_STRUCTURE`); stderr names the path or the fixture. The other results are printed as usual. `CONTRACT.md` section 5, note |

stderr names paths, fixture names and error kinds only. It never contains document content. Arguments are read as OS strings: a path that is not UTF-8 is opened like any other path, so it gives a result or exit code 2, never a panic (L-2 of the security review). Single mode reads only a regular file (also through a symbolic link). It checks the file type before the open and again on the open file. A directory, a FIFO, a socket or a device is an I/O error (exit code 2, "not a regular file"), so the command never waits for a FIFO writer (L-2 of security review round 2). One window stays: when the path changes to a FIFO between the check and the open, the open can still wait. Batch mode applies the same rule to each `input.html` (CONTRACT.md section 2, item 7): a subdirectory without `input.html` is skipped, but an `input.html` that exists and is not a regular file (FIFO, device, directory, socket; symbolic links are followed) is an I/O error with exit code 2 and no wait. The same check-to-open window applies.

To compare the candidate with the golden fixtures, run this from the repository root:

```sh
node tools/rqp-markup/diff.mjs --only rust
```

## Test

Run these commands in `tools/rqp-markup/rust`:

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

| Test file | What it checks |
|---|---|
| `src/*.rs` (`mod tests`) | Tokenizer states and spans, name sets, JSON grammar, hashes; spans outside the input fail closed (`lib.rs`, `rules.rs`); the name set `FOREIGN_ALLOWED`; the report of an internal defect (`main.rs`) |
| `tests/rules.rs` | One or more tests for each rule M-01…M-27 and each error code; each invalid case checks the code and the rule. Separate tests for M-24 items j and k, for M-08 with a foreign `select`, and for the edge cases E1…E8. The security review: one test for each of M-06 items 2, 3 and 4, M-11 item 5 and M-13 item 3, each with the review repros and valid neighbors. Round 2: one test for each of M-09 conditions 1 and 2 (`m09_1_*`, `m09_2_*`), M-10 item 2 (`m10_2_*`) and M-11 item 6 (`m11_6_*`), each with the review repros and valid neighbors; M-09 condition 2 is checked before the implied parts of M-26. M-26 depth counter D (`m26_depth_*`, `m26_d_*`): D = 256 is valid and 257 is `RQP_MARKUP_LIMIT` in HTML context, in a foreign region, for a block element and in block content; elements that M-05 does not push do not count; exact pops give units back; crossings, dirty elements, `html` and `body` keep their units; implied table parts count once per F; the place of the check; `noscript` content. WP-17 S3 tests: `m22_5_*` and `m23_*` (regions in block content, with the reason checked), `m26_pushes_*`, `m26_region_root_*`, `m26_depth_counts_noscript_*`, `m11_7_*`, `m11_8_*` and `m11_9_*` (the pass over `noscript` content, with the restore of D and of the dirty mark). Two tests run near-limit inputs that take minutes when the work grows with the input history (`large_adversarial_inputs_finish`, `large_nested_inputs_closed_by_end_tags_finish`) |
| `tests/fixtures.rs` | Every fixture of `fixtures/rqp/markup/` against its `expected.json` |
| `tests/cli.rs` | Command line: modes, exit codes, batch order, skipped directories, size limit, no content on stderr, arguments that are not UTF-8, inputs that are not regular files (a FIFO and a symbolic link to it give exit code 2 without a wait; in batch mode a FIFO, a symbolic link to a FIFO and a directory named `input.html` give exit code 2; Unix only, skipped when `mkfifo` is not available) |
| `tests/props.rs` | proptest: no panic on arbitrary bytes and text; for valid results the offsets ascend and stay in the input, content hash = SHA-256 of `input[start..end]`, tags around the content; a markup "soup" generator reaches valid documents with blocks; well-nested documents with 240 to 271 elements (HTML context, foreign region, block content, with tokens that are not pushed) are valid when the token stack has at most 256 elements, else `RQP_MARKUP_LIMIT` (M-26); 120 to 279 shapes with known costs of D (exact pops, implied parts, crossings, dirty elements, `body`, `html`) give the result of a small model of M-26; balanced `noscript` content of any depth is valid and is `RQP_MARKUP_STRUCTURE` (M-11) without one of its end tags; well-nested `form`, `div`, `b` and `span` elements are valid exactly when no `form` is inside a `form` (M-10 item 2); well-nested `noscript` content in a cell is valid exactly when every `td`, `tr` or `caption` has a `table` of the content open (M-11 item 6); elements that the tree builder foster-parents in a table are valid when they are closed before the next table part, `col` or `table`, else `RQP_MARKUP_STRUCTURE` (M-09 condition 2). A guard test checks that the M-10 and M-11 generators reach both results |

`tests/fixtures.rs` reads `$RQP_MARKUP_FIXTURES` when it is set, else `../../../fixtures/rqp/markup`.

### Mutation testing

`cargo mutants` builds a copy of the package, so the relative fixture path does not exist there. Set the fixture directory:

```sh
RQP_MARKUP_FIXTURES="$(git rev-parse --show-toplevel)/fixtures/rqp/markup" \
  cargo mutants --no-shuffle -j 3 --timeout 90 --output /tmp/rqp-markup-mutants
```

Each job needs about 450 MB of disk space for its copy of the package and its build.

Result of T-09 (WP-17 S3; cargo-mutants 27.1.0, `--in-diff` of the changed lines of `src/` against `7f36912`, `-j 1 --timeout 60`): 65 mutants, 3 unviable, 60 caught, 2 missed. Kill rate 60/62 = 96.8 %. No test was added. The 2 missed mutants are equivalent:

| Mutant | Why equivalent |
|---|---|
| `==` → `!=` in `Pass::content_end` (`target == top`) | The two branches pick the name of the current node or the name of the end tag. They differ only when a region root closes with elements above it, and the names allowed in a region (`FOREIGN_ALLOWED`) have neither the FORMATTING nor the TABLE_FAMILY flag that `close()` reads from this name |
| `>` → `>=` in `Pass::noscript_end_rules` (`stack.len() > base`) | At `stack.len() == base` the current node is the `noscript` element. Its name is not the name of this end tag, and a region root around C is already rejected by M-07 before this check, so the error is M-11 in both cases |

Result (cargo-mutants 27.1.0, whole package, 2026-10-09): 402 mutants, 30 unviable, 348 caught, 10 timeouts (infinite loops, counted as detected), 14 missed. Kill rate 358/372 = 96.2 %. All 14 missed mutants are equivalent:

| Mutants | Why equivalent |
|---|---|
| `\|` → `^` in three flag masks (`names.rs`, `rules.rs` ×2) | The flags are different single bits, so OR and XOR give the same mask |
| `>` → `>=` in `open_in_content` | The block element never has a name that is searched there (lists, `table`, `ruby`; a heading block is caught by the clause before) |
| Comment state arms: `<` and `!` in the comment and less-than-sign states, `-` in the comment end bang state | The removed arm reconsumes into a state that makes the same transition; the less-than-sign states only add parse errors |
| Tag state arms: `/` `>` in before attribute name, `>` in before attribute value, `/` `>` and white space in after attribute value (quoted) | The removed arm reconsumes into the before attribute name state, which makes the same transition |
| Escaped less-than sign state: letter guard → `true` | A non-letter then goes to the escaped state in both paths |

To check only the lines that changed since a base commit `BASE`, run this in `tools/rqp-markup/rust`. The package is its own workspace, so the diff paths must be relative to this directory (`--relative`):

```sh
D=$(mktemp)
git -c core.quotePath=false diff --relative --src-prefix=a/ --dst-prefix=b/ \
  --no-ext-diff BASE -- '*.rs' > "$D"
RQP_MARKUP_FIXTURES="$(git rev-parse --show-toplevel)/fixtures/rqp/markup" \
  cargo mutants --in-diff "$D" --no-shuffle -j 3 --timeout 120
```

Result for T-08 (cargo-mutants 27.1.0, `--in-diff` against `4c90861`, 2026-10-09): 27 mutants, 2 unviable, 24 caught, 1 missed. Kill rate 24/25 = 96.0 %. The missed mutant is equivalent:

| Mutant | Why equivalent |
|---|---|
| `>` → `>=` in M-24 item j (`nearest(items) > nearest(lists)`) | Two elements never have the same stack index, so the two sides are equal only when both are `None`. Then no list is open in the content, and item c rejects the same start tag |

Result for T-11 (cargo-mutants 27.1.0, `--in-diff` against `edab313`, 2026-10-11): 8 mutants, 1 unviable, 7 caught, 0 missed. Kill rate 7/7 = 100 %. cargo-mutants does not make the off-by-one mutant `>=` → `>` of the depth check; a manual run of it fails the fixture test, the depth property test and 7 unit tests.

Result for T-13 (cargo-mutants 27.1.0, `--in-diff` against `4fff64f`, `-j 3 --timeout 120`, 2026-10-11): 129 mutants, 10 unviable, 115 caught, 2 timeouts (`end_name_at` mutants that loop forever, counted as detected), 2 missed. Kill rate 117/119 = 98.3 %. Both missed mutants are equivalent:

| Mutant | Why equivalent |
|---|---|
| `\|` → `^` in `ROW_GROUP \| ROW` (`implied_parts`) | The two parts are different single bits, so OR and XOR give the same mask |
| `Tokenizer::defect` → `false` | The defect flag is set only when an internal offset is outside the input. The callers never pass such an offset (`from` and the attribute start are at most the current position), so no input sets it. It is a fail-closed guard for a future defect |

The guard `index >= self.depth` in `Pass::push` is of the same kind: D is never smaller than the stack size, so the guard never fires. cargo-mutants makes only `>=` → `<` there (caught).

Result for T-16 (cargo-mutants 27.1.0, `--in-diff` against `5224828`, `-j 3 --timeout 120`, 2026-10-11): 38 mutants, 3 unviable, 35 caught, 0 missed. Kill rate 35/35 = 100 %. cargo-mutants does not mutate the table count of the `noscript` content (no operator in the `if` of the `table` name). Two manual mutants (no subtraction at `</table>`; no addition at `<table>`) each fail `m11_6_table_parts_in_noscript`. The second type check of `read_input` (on the open file) has no test: a test cannot change the path between the two checks.

## Design

### Library choice

The candidate has its own tokenizer for the WHATWG states that `markup.md` section 3.1 lists (DL-012). No HTML library is used.

| Option | Result |
|---|---|
| `lol_html` 3.0.1 (BSD-3-Clause) | Rejected. It is a streaming rewriter with its own text-type logic. The rules need every end tag (also stray ones), the raw attribute value spans, the M-04 state changes controlled by the token stack (foreign regions, `noscript`), and a second tokenization of `noscript` content (M-11). Agreement of its text-type logic with M-04 is NOT FOUND (`markup.md` section 3.1). |
| `html5gum` 0.8.4 (MIT) | Rejected. Byte spans of tokens and raw attribute values are NOT FOUND in its public documentation; attribute values are decoded. Control of the tokenizer state after a start tag is shown only in an example. |
| Own tokenizer | Chosen. About 780 lines with comments, exact byte spans, raw attribute values, the state after a start tag is set by the caller (M-04), no dependency. |

The tokenizer follows the WHATWG state descriptions (HTML Living Standard §13.2.5). No code was copied from an HTML parser.

### Modules

| Module | Content |
|---|---|
| `tokenizer.rs` | Data, tag open, end tag open, tag name, attribute states, self-closing, markup declaration open, comment states (with the less-than-sign states), bogus comment, `<?`, DOCTYPE (ends at the first `>`), `<![CDATA[` (M-12 check), RCDATA, RAWTEXT, script data with the escaped and double-escaped states, PLAINTEXT |
| `names.rs` | Name normalization (ASCII lower case, U+0000 → U+FFFD), the name sets of `markup.md` section 2.1 (with `SPECIAL` and `FORMATTING` as fixed lists), the per-name pointer to the nearest open element |
| `rules.rs` | One pass: token stack (M-05) with the state per entry (dirty mark, special-crossed mark, implied parts), the depth counter D (M-26), the count of HTML `form` elements on the stack (M-10 item 2), document rules (M-06…M-13), block rules (M-14…M-26), the pass of M-11 item 8 over the content C of a `noscript` element (the same rules on the same state, with the checks of M-11 items 1 to 3 and 5 to 7 in `noscript_start_rules` and `noscript_end_rules`) |
| `json.rs` | RFC 8259 JSON text check (M-25), depth ≤ 64, without recursion |
| `lib.rs` | `analyze()` (M-01, M-02, the pass, M-27 hashes and revisions), `result_json()` |
| `main.rs` | Command line (`CONTRACT.md` section 2) |

### Invariants

- Validity is decided on tokens only (DL-008). There is no tree builder.
- Offsets are byte offsets into the input. A byte order mark at offset 0 is skipped by the tokenizer and stays in the offsets and the template.
- Input preprocessing (CR LF → LF) is modelled by treating CR as white space. CR LF then gives the same token boundaries as LF. Offsets never change.
- Character references are never decoded. Raw attribute values are used (DL-010).
- The first attribute with a name wins (`data-rq-block`, `type`).
- A valid result has blocks in document order; for each block `input[start - 1] == '>'`, `input[end..end + 2] == "</"`, and `sha256` is the hash of `input[start..end]` (property tests).
- The depth counter D (M-26) is at most 256 after every token. Each element on the token stack holds at least one unit of D (its push; an exact pop gives back only its own units), so the stack never has more than D elements. A push checks this too and fails closed when it does not hold.
- Each token-stack entry has three parts of state besides its name: the dirty mark (M-26 item 5), the special-crossed mark (M-06 item 4) and its implied parts (M-26 item 3, three bits). Marks are set only on the new current node after an end tag pops; they leave with their element.
- The count of HTML forms is the number of elements on the token stack with the name `form` that were pushed in HTML context. `Pass::push` and `Pass::pop` are the only places that push and pop, and they change the count, so a pop by a crossing changes it too. A count below 0 is an internal defect.
- The stack of C is the part of the token stack above the `noscript` element (M-11 items 5 and 6): the pass of M-11 item 8 pushes the elements of C on the token stack. The index of the `noscript` element plus 1 is `base`; an end tag of C needs a stack larger than `base`, and a table of C is a `table` element at an index from `base`. After the end tag `noscript`, `Pass::check_noscript` restores D, and the dirty and special-crossed marks of the `noscript` element (DL-044 c); C is balanced, so the stack, the form count and the table marks are those of before C.
- An offset outside the input is an internal defect, never a reason to use empty bytes. The pass, the result and the tokenizer return an internal error (`MarkupError::is_internal`); the document fails closed (L-3 of the security review).

### Resource limits and failure modes

| Item | Bound |
|---|---|
| Input | M-01 is checked first. The command reads at most 5 MiB + 1 byte of a file. |
| Time | Linear in the input size. The nearest open element with a name is found through a per-name pointer (O(1)); M-24 items j and k use the same pointers and the table-family marks (O(1) per start tag); each element is popped once, and the pops of an end tag also check M-06 items 1, 2 and 4 and set the marks (O(1) per popped element); D and the implied parts are O(1) per token; raw text end tags scan each letter once; the `noscript` content is tokenized once more, and an end tag in it compares only with the top of the token stack. The checks of round 2 are O(1) per start tag: M-09 condition 1 reads the last table mark, condition 2 compares the index of F with the stack size, M-10 item 2 reads the form count, and M-11 item 6 reads the pointer of the nearest `table`. D ≤ 256 (M-26) bounds the token stack, so work per token that grows with the stack depth is bounded too. A crossing keeps its units of D, so all crossings of one document pop at most 256 elements together. `tests/rules.rs` runs near-limit adversarial inputs: many failed end tags on a full stack, more than 250 000 names in groups of 256 nested names closed by exact end tags, more than 500 000 names of self-closing elements in a foreign region on a full stack, comment and script patterns, lists and tables at the maximum depth in five blocks, many scripts in `noscript`. `large_nested_inputs_closed_by_end_tags_finish` (T-09, T-11, T-13) fills the stack to the maximum depth and closes it with matching end tags, again and again up to 5 MiB: one name, two names, short elements on a full stack, one end tag that pops 254 elements followed by short elements up to 5 MiB (the same crossing twice is `RQP_MARKUP_LIMIT`), `template` (table marks), `svg` (foreign root, one region and many regions), five blocks just under 1 MiB, and the shapes of round 2 (T-16): `<form></form>` at the maximum depth, rows of one table, `col` in a table in a template, and rows of a table in `noscript` content, each up to 5 MiB. For each shape, the old deep input (up to 1.7 million elements) is now `RQP_MARKUP_LIMIT` at the 257th unit of D. The two tests take about 5 s and 6 s in a debug build and 0.4 s and 0.3 s in a release build (T-13). A cost that grows with the number of earlier tokens (for example a scan of all interned names at each push or pop) makes `large_adversarial_inputs_finish` take 90 s in a release build. A pop cost that grows with the stack depth now costs at most 256 steps (0.7 s). |
| Memory | Linear in the input size: interned names, at most 257 block ids, one copy of each `noscript` content (the copies are disjoint, so the total is at most the input size). The stack of C is the token stack above the `noscript` element, so it is bounded by D (M-26 item 7, H-71): token stack and table marks ≤ 256 elements (D ≤ 256), JSON depth stack ≤ 64 |
| Recursion | None |
| Panics | `unwrap`, `expect`, `panic`, indexing and unchecked arithmetic are denied by lints outside tests. `#![forbid(unsafe_code)]`. Command line arguments are OS strings (no panic on non-UTF-8 arguments). |
| Errors | `analyze()` returns the code and the rule (`M-xx`) that failed first. The command prints only the code. An internal defect has the rule `internal` and the code `RQP_MARKUP_STRUCTURE`; the command then exits with 70 after all results. No input is known to cause one. |
| Hash flooding | Tag names map to ids through the standard `HashMap` (SipHash with a random key per process). The name sets, also `SPECIAL` and `FORMATTING`, are fixed lists; their flags are bits on the interned id. An end tag in C compares only with the top of the token stack. |

### How the rules are read

These places needed a choice. Each one follows the text of `markup.md`. Where the human decided an edge case of `markup-edge-cases.md` (H-21, H-22), the table names the case. Elsewhere, where the text leaves room, the choice is the conservative one or the one that cannot change a block.

| Rule | Reading |
|---|---|
| M-07.3, M-07.4 (E1) | "Character tokens only" is checked on tokens. `a < b` inside `<svg><desc>` is valid (the `<` is a character token). `</>` emits no token. |
| M-07.3, M-07.4 (E2) | EOF before the end tag of a foreign text-only element (`<svg><title>x` at EOF) is not an error. EOF in a foreign region is not an error, also with a foreign `select` open. |
| M-07.3, M-07.4, M-12 (E5) | A `<![CDATA[` that satisfies M-12 is allowed in foreign TEXT content and in integration points (`<svg><style><![CDATA[a{}]]></style>`). M-12 still applies there. Comments, DOCTYPE tokens and tags stay invalid. |
| M-03, M-07 (E6) | `<?` without `>` is a comment that ends at EOF. In a foreign text-only element it is therefore invalid. |
| M-07, M-10, M-13 (E7) | `frameset` and the reserved `script` type are checked in every context, also in a foreign region. `BREAKOUT` includes `font` with or without attributes. `INTEGRATION` names apply in `svg` and in `math` alike. |
| M-08 | M-08 applies only to a `select` pushed in HTML context. A `select` in a foreign region is a foreign element: M-07 applies to it (`<math><select><th></th></select></math>` is valid). The nearest open `select` decides, because an HTML `select` cannot contain `svg` or `math`, and a foreign region ends with all its elements. A `<![CDATA[` in HTML `select` content is a bogus comment, so it is allowed as a comment. |
| M-11 (E3) | When `noscript` has no end tag, C is the rest of the input, and M-11 applies to it. The pass over C tokenizes `C</noscript>` from the data state, in HTML context; `<![CDATA[` there is a bogus comment that ends at the first `>`, the same byte as the token of this tokenizer. |
| M-11, M-13 (E8) | M-13 also applies to the start tags of C: a `script` whose first `type` raw value contains `rivqen-manifest` (ASCII case-insensitive), `&` or a byte ≥ 0x80 (H-28) gives `RQP_MARKUP_RESERVED`. M-13 is a rule of the pass of M-11 item 8, so a `script` start tag with a reserved type and a `data-rq-block` attribute gives M-11 (item 2) first. Text in raw text sections of C (`<style>`, `<script>`) and in comments is not checked. |
| M-12 (E4) | `<![CDATA[` with no `>` after it is not an error: a CDATA section and a bogus comment both end at EOF. With E5, this is also valid in a foreign integration point (`<svg><desc><![CDATA[x` at EOF). |
| M-19 | F is computed from elements pushed in HTML context. A block start tag is always in HTML context when M-19 is checked (M-16 first). |
| M-24 j | Checked with the per-name pointers: the stack index of the nearest open `li`, `dd` or `dt` against the index of the nearest open `ul`, `ol`, `menu` or `dl`. When the nearest list was opened before the block, item c rejects the start tag, so item j needs no "opened in the content" test of its own. All combinations count (for example `dd` above `ol`), as the rule says. |
| M-24 k | Checked on every start tag in the content, also void names (`<br>`, `<img>`) and `table`. F is the M-09 value. At the block start F is none, `td`, `th` or `caption` (M-19), so a table context in the content is always opened in the content. Outside block content, k does not apply. |
| M-06.1, M-06.2, M-06.4 | Checked in one loop over the popped elements of an end tag (`Pass::close`), in pop order from the current node down to the matching element; the first failure returns. Item 4 checks the matching element too. In block content every end tag pops exactly the current node (M-23); the same function runs, so item 4 and D apply there in the same way. |
| M-06.2 | `SPECIAL` applies to elements pushed in HTML context; every element pushed in a foreign region counts, whatever its name (`<svg><a><g></a>` is invalid). |
| M-06.3 | Applies only when no element matches and the position is in HTML context. In a foreign region M-07.2 rejects every stray end tag first; in block content M-23 rejects it first. ASCII case does not matter (`</TBODY>`). |
| M-06.4 | Item (a) and item (b) set the same flag on the new current node after the pops. A no-match end tag returns before the pops, so it does not change a mark. |
| M-11.5 (H-27, H-75) | An end tag in C needs a token stack larger than `base`, and its name must be the name of the current node. Names compare as the tokenizer emits them (the name table). Exception: an `svg` end tag when the open region root is the nearest open `svg` closes the root and the elements above it. The end tag `noscript` of item 1 also needs a stack of size `base`. A start tag that M-05 pushes (a self-closing non-void start tag too, `<noscript><span/></noscript>` is invalid) is an element of C by the normal push. An end tag in a raw text section of C is text. |
| M-09.1 (H-35) | Checked for each `col` start tag in HTML context: the last table mark is the nearest element in `TABLE_FAMILY` or `template` that was pushed in HTML context. A `col` in a foreign region is a foreign element. Condition 1 comes before the table of M-09, in the same check. |
| M-09.2 (H-37) | Checked for each start tag in HTML context with a name in `TABLE_PART`, `col` or `table`, before the table of M-09, the implied parts of M-26 and the push. The current node is the top of the token stack, also an element that is not in `TABLE_FAMILY`. Elements that are not pushed (void elements, comments) are not on the stack, so `<table><br><tr>` is valid. |
| M-10.2 (H-36) | A `form` start tag in HTML context fails while the form count is above 0. The count includes only forms pushed in HTML context: a foreign `form` (`<svg><form>`) is not counted, also when a crossing pops it. The tags of the `noscript` content C run in the pass of M-11 item 8, so the count includes the forms around the `noscript` element and the forms of C (H-71). |
| M-10.3 (H-43) | A `template` start tag in HTML context fails when it has an attribute named `for` (ASCII case-insensitive; any value, also none; any occurrence, also a duplicate): `RQP_MARKUP_STRUCTURE`, rule `M-10`. The tokenizer sets `Tag::has_for` when it leaves the name state of any attribute named `for` (O(1) per attribute, no hashing). The check runs next to M-10 item 2, before the block and content rules, so it also applies to a `template` that carries `data-rq-block`. A `template` in a foreign region is not affected. The `noscript` content C already fails on every `template` start tag (M-11 item 3), so C needs no extra check. |
| M-11.6 (H-38) | A start tag in C with a name in `TABLE_PART` or `col` fails with M-11 unless the nearest open `table` is at an index from `base` (a table of C). The check comes before the push; M-09 then applies as in the main pass (H-71), so a table part above an open `table` with another element between them is `M-09`. |
| M-11.8, M-11.9 (H-71, H-74, DL-044) | `Pass::check_noscript` runs at the next end tag or at EOF: it keeps D, the dirty mark and the special-crossed mark of the `noscript` element, tokenizes `C</noscript>`, runs `start_tag` and `end_tag_rules` on its tokens with the checks of items 1 to 3 and 5 to 7 first, and restores D and the marks of the `noscript` element. An `svg` start tag in HTML context of C without the self-closing flag is a region root in the normal push; items 2 to 4, 6 and 7 apply in the region; a start tag there never changes the tokenizer state. `math` is forbidden in C, also in a region. |
| M-22.5, M-23 (H-73, H-75) | `FOREIGN_ALLOWED` is a flag (`flag::FOREIGN_ALLOWED`); `content_start` checks it for a start tag in a foreign region and returns before M-22 items 2 to 4 and M-24. `svg` and `math` are not in `FORBIDDEN_CONTENT`; a self-closing region root is exempt from item 3. `content_end` lets the end tag whose nearest element is the region root cross the elements of its region; M-22 item 4 (void end tags) applies in HTML context only. EOF in a region of the content is M-20. |
| M-06.4 (H-75) | `Pass::close` does not set the mark (a) for the foreign elements that an end tag crosses when its matching element is the region root; the mark (b) is passed as before. |
| M-13.3 (H-28) | Any byte ≥ 0x80 in the first `type` raw value of a `script` start tag, quoted or unquoted, in the main pass and in C. A non-ASCII value in another attribute, or in a second `type` attribute, is not checked. M-13 comes before M-16, so `<script type="application/jsön" data-rq-block="j">` is `RQP_MARKUP_RESERVED`. |
| M-26 | The block count is checked when the 257th block start tag is read; the size when the block end tag is read. D is checked at each unit (`Pass::raise`): implied parts after M-09 and before the push of the start tag; the push after all other checks of the same start tag: for a block start tag after M-15…M-21 and the block count, in block content after M-22 and M-24, in a foreign region after M-07, in HTML context after M-09. Implied parts use F of M-09 (HTML elements only, the search stops at `template`), so table parts in a foreign region have none. The exception of item 4 for `html` and `body` uses the name of the popped element, in both contexts, as the rule text and the cross-check do. The tags of `noscript` content (M-11) count in the pass of item 8 (item 7, H-71); D is restored after the pass, and the `noscript` element itself is pushed and counts. |

### Cross-check against parse5

Run the sweeps of `tools/rqp-markup/crosscheck/README.md` with `--only rust` and `TMPDIR=/dev/shm`. Result (T-08, 2026-10-09, release build):

| Sweep | Documents | Valid with blocks | Findings |
|---|---|---|---|
| seed 4, `--max-tokens 20`, scripting enabled | 200 000 | 23 785 | 0 |
| seed 5, `--max-tokens 20`, both views | 100 000 | 11 921 | 0 |
| seed 6, `--max-tokens 40`, both views | 100 000 | 7 036 | 0 |
| All 133 fixtures, both views (`check`) | 133 | — | 0 |

Before T-08, seeds 4 and 5 gave 9 block-byte disagreements (all M-24 j or k shapes) and manifest findings in the scripting-disabled view (E8).

Result for T-13 (2026-10-11, release build, both views, `TMPDIR` on disk). "With a class" counts the documents that a rule class of the cross-check (`src/depth.mjs`: M-06 items 2 to 4, M-11 item 5, M-13 item 3, M-26) rejects and that the candidate accepts; it must be 0. "Depth invariant" counts the documents where parse5 is deeper than the claim of M-26 allows, and how many of them the candidate accepts.

| Sweep | Documents | Valid | Valid with blocks | Findings | Accepted with a class | Depth invariant |
|---|---|---|---|---|---|---|
| seed 31, `review` (16 pieces) | 20 000 | 4 203 | 2 221 | 0 | 0 | 97, none accepted |
| seed 21, `review`, `--max-tokens 20` | 50 000 | 8 645 | 4 635 | 0 | 0 | 313, none accepted |
| seed 23, `review`, `--max-tokens 80` | 20 000 | 938 | 514 | 0 | 0 | 354, none accepted |
| seed 24, `classic`, `--max-tokens 20` | 100 000 | 35 242 | 11 126 | 0 | 0 | 222, none accepted |
| All 180 fixtures (`check`) | 180 | 67 | — | 0 | — | — |

Result for T-16 (2026-10-11, release build, both views, `TMPDIR` on disk). The rule classes of round 2 (`M-09-template-col`, `M-09-table-current`, `M-10-form`, `M-11-table-part`) are in the class count.

| Sweep | Documents | Valid | Valid with blocks | Findings | Accepted with a class | Depth invariant |
|---|---|---|---|---|---|---|
| seed 61, `review2`, `--max-tokens 20` | 20 000 | 2 945 | 1 384 | 0 | 0 | 1 003, none accepted |
| seed 62, `review`, `--max-tokens 20` | 20 000 | 3 485 | 1 866 | 0 | 0 | 117, none accepted |
| seed 63, `classic` (16 pieces) | 20 000 | 8 266 | 2 533 | 0 | 0 | 39, none accepted |
| All 194 fixtures (`check`) | 194 | 71 | — | 0 | — | — |

The round 2 cases (`cases/review2-cases.json`): every `H35-*`, `H36-*`, `H37-*` and `H38-*` repro, and `H37-object-td` and `H37-form-in-table` (the cost of condition 2), is `STRUCTURE`; `H37-end-table-over-div-a` is `STRUCTURE` by M-06 item 4; every control case is valid with the parse5 blocks. Before T-16 (study at `65d4b98`, edge cases §7.5), seed 51 gave 3 180 findings for each candidate and view.

The review cases (`cases/review-cases.json`): every repro is rejected with the code of its rule (`H26-*`, `Q1-*`, `H27-*`: `STRUCTURE`; `H28-*`: `RESERVED`; `H30-*`: `STRUCTURE`; `H31-*`: `LIMIT`), and every control case is valid with the parse5 blocks. Before T-13 (study at `27ae1b5`, edge cases §6.6), seed 21 gave 15 591 valid documents for each candidate and 6 762 findings for the Rust, Node.js and PHP candidates together.

## Dependencies

| Crate | Version | License | Use |
|---|---|---|---|
| `sha2` | 0.11.0 | MIT OR Apache-2.0 | SHA-256 (runtime) |
| `base64` | 0.23.1 | MIT OR Apache-2.0 | base64url without padding (runtime; default features off, no SIMD `unsafe`) |
| `proptest` | 1.11.0 | MIT OR Apache-2.0 | Property tests (dev; default features off, `std` only) |
| `serde_json` | 1.0.151 | MIT OR Apache-2.0 | Reading `expected.json` and result lines in tests (dev) |

Transitive runtime crates: `digest`, `block-buffer`, `crypto-common`, `hybrid-array`, `typenum`, `cpufeatures`, `cfg-if` (all MIT OR Apache-2.0). Versions are pinned with `=` in `Cargo.toml` and locked in `Cargo.lock`.
