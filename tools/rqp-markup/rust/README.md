# RQP markup parser candidate — Rust

<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- SPDX-FileCopyrightText: The Rivqen Authors -->

The Rust candidate of WP-17 S1 (task T-03). It implements the rules M-01…M-27 of `docs/engineering/protocol/markup.md` (rqp/1) and the interface of `tools/rqp-markup/CONTRACT.md`. It is a tool for the differential test, not the production crate (`rivqen-proto`, P3).

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

stderr names paths and error kinds only. It never contains document content.

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
| `src/*.rs` (`mod tests`) | Tokenizer states and spans, name sets, JSON grammar, hashes |
| `tests/rules.rs` | One or more tests for each rule M-01…M-27 and each error code; each invalid case checks the code and the rule |
| `tests/fixtures.rs` | Every fixture of `fixtures/rqp/markup/` against its `expected.json` |
| `tests/cli.rs` | Command line: modes, exit codes, batch order, skipped directories, size limit, no content on stderr |
| `tests/props.rs` | proptest: no panic on arbitrary bytes and text; for valid results the offsets ascend and stay in the input, content hash = SHA-256 of `input[start..end]`, tags around the content; a markup "soup" generator reaches valid documents with blocks |

`tests/fixtures.rs` reads `$RQP_MARKUP_FIXTURES` when it is set, else `../../../fixtures/rqp/markup`.

### Mutation testing

`cargo mutants` builds a copy of the package, so the relative fixture path does not exist there. Set the fixture directory:

```sh
RQP_MARKUP_FIXTURES="$(git rev-parse --show-toplevel)/fixtures/rqp/markup" \
  cargo mutants --no-shuffle -j 3 --timeout 90 --output /tmp/rqp-markup-mutants
```

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
| `names.rs` | Name normalization (ASCII lower case, U+0000 → U+FFFD), the name sets of `markup.md` section 2.1, the per-name pointer to the nearest open element |
| `rules.rs` | One pass: token stack (M-05), document rules (M-06…M-13), block rules (M-14…M-26) |
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

### Resource limits and failure modes

| Item | Bound |
|---|---|
| Input | M-01 is checked first. The command reads at most 5 MiB + 1 byte of a file. |
| Time | Linear in the input size. The nearest open element with a name is found through a per-name pointer (O(1)); each element is popped once; raw text end tags scan each letter once. `tests/rules.rs` runs near-limit adversarial inputs (deep stacks, many names, many failed end tags, comment and script patterns). |
| Memory | Linear in the input size: token stack, interned names, at most 257 block ids, one copy of each `noscript` content (M-11), JSON depth stack ≤ 64 |
| Recursion | None |
| Panics | `unwrap`, `expect`, `panic`, indexing and unchecked arithmetic are denied by lints outside tests. `#![forbid(unsafe_code)]`. |
| Errors | `analyze()` returns the code and the rule (`M-xx`) that failed first. The command prints only the code. |

### How the rules are read

These places needed a choice. Each one follows the text of `markup.md`; where the text leaves room, the choice is the conservative one or the one that cannot change a block.

| Rule | Reading |
|---|---|
| M-07.3, M-07.4 | "Character tokens only" is checked on tokens. `a < b` inside `<svg><desc>` is valid (the `<` is a character token), although the rule adds "the content contains no `<`". `</>` emits no token. |
| M-07.3, M-07.4 | EOF before the end tag of a foreign text-only element (`<svg><title>x` at EOF) is not an error. EOF in a foreign region is not an error. |
| M-07, M-10, M-13 | `frameset` and the reserved `script` type are checked in every context, also in a foreign region. `BREAKOUT` includes `font` with or without attributes. `INTEGRATION` names apply in `svg` and in `math` alike. |
| M-08 | A `<![CDATA[` in `select` content is a bogus comment (HTML context), so it is allowed as a comment. |
| M-11 | When `noscript` has no end tag, C is the rest of the input. The second tokenization runs in HTML context; `<![CDATA[` there is a bogus comment and M-12 does not apply to it. |
| M-12 | `<![CDATA[` with no `>` after it is not an error: a CDATA section and a bogus comment both end at EOF. |
| M-19 | F is computed from elements pushed in HTML context. A block start tag is always in HTML context when M-19 is checked (M-16 first). |
| M-26 | The block count is checked when the 257th block start tag is read; the size when the block end tag is read. |

## Dependencies

| Crate | Version | License | Use |
|---|---|---|---|
| `sha2` | 0.11.0 | MIT OR Apache-2.0 | SHA-256 (runtime) |
| `base64` | 0.23.1 | MIT OR Apache-2.0 | base64url without padding (runtime; default features off, no SIMD `unsafe`) |
| `proptest` | 1.11.0 | MIT OR Apache-2.0 | Property tests (dev; default features off, `std` only) |
| `serde_json` | 1.0.151 | MIT OR Apache-2.0 | Reading `expected.json` and result lines in tests (dev) |

Transitive runtime crates: `digest`, `block-buffer`, `crypto-common`, `hybrid-array`, `typenum`, `cpufeatures`, `cfg-if` (all MIT OR Apache-2.0). Versions are pinned with `=` in `Cargo.toml` and locked in `Cargo.lock`.
