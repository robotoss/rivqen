# RQP markup parser candidate (PHP)

<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- SPDX-FileCopyrightText: The Rivqen Authors -->

The PHP candidate of WP-17 (decision H-13). It has its own subset of the WHATWG HTML tokenizer with byte offsets and no tree builder. It implements the rules of `docs/engineering/protocol/markup.md` section 2 (rqp/1) and the interface of `../CONTRACT.md`. It has no runtime dependency.

## Run

The command needs PHP 8.3 or later with the `json` and `pcre` extensions (both are always present). It needs no build step and no `composer install`.

```sh
php bin/rqp-markup <input.html>
php bin/rqp-markup --batch <fixtures-dir>
```

| Exit code | Meaning |
|---|---|
| 0 | Results printed (valid and invalid documents) |
| 2 | Usage error or I/O error. The reason is on stderr. |
| 70 | Internal error (a defect). stderr has the exception class and the source line only. |

The command never prints document content to stderr. It reads at most 5 MiB + 1 byte of a file, so rule M-01 applies to any file size.

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

`vendor/bin/phpunit --exclude-group limits` skips the 5 MiB resource tests (about 25 s).

| Test file | What it checks |
|---|---|
| `tests/TokenizerTest.php` | Token spans in each tokenizer state |
| `tests/ReferenceTokenizerTest.php` | Differential test against `tests/Fuzz/ReferenceTokenizer.php`, a literal one-character-at-a-time transcription of the WHATWG states (with CR/LF preprocessing, all DOCTYPE states and the processing instruction states): exhaustive short comment bodies, dense inputs per state, 100 000 random inputs |
| `tests/RulesTest.php` | Each rule M-01 to M-27 and each error code (error code and rule ID) |
| `tests/JsonValidatorTest.php` | RFC 8259 rules of M-25 |
| `tests/RevisionTest.php` | Hashes and revisions of `CONTRACT.md` section 4 |
| `tests/FixturesTest.php` | Every fixture of `fixtures/rqp/markup/` against `expected.json` |
| `tests/CliTest.php` | Command line, exit codes, batch order, bounded read |
| `tests/PropertyTest.php` | Seeded random documents: invariants, and an oracle for generated valid documents |
| `tests/LimitsTest.php` | Linear time and bounded memory on adversarial 5 MiB inputs |
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
| CDATA (M-12) | One token from `<![CDATA[` to the first `>`; a flag says if that `>` ends a `]]>` |
| RCDATA, RAWTEXT, script data with escape and double-escape states, PLAINTEXT, and their end tag states | Set by the parser after a start tag (M-04) |

Equivalences that the code uses (each one keeps the token boundaries):

- CR is white space. The WHATWG input preprocessing turns CR and CR LF into LF; in every state that reads white space, the LF after a CR is white space too.
- Character tokens are not emitted. No rule depends on them.
- Character references are not decoded. No character reference state reads `<`, `>`, a quote or white space.
- Only `data-rq-block` and `type` attribute values are kept. Memory per token is constant.

`tests/ReferenceTokenizerTest.php` checks these equivalences: the reference tokenizer does not use them, and both must give the same tokens.

### Parser (`src/Parser.php`)

One pass over the tokens with the token stack of M-05. The stack is two lists (names, flags). These counters make the work for each token constant:

| Structure | Use |
|---|---|
| Count of open elements by name | Ignore an end tag without a match in O(1) (M-05 step 3); "is open" checks of M-24 |
| Count of elements opened in the content, by name | "Opened in the content" checks of M-24 |
| Stack of indexes of `table`, table parts and `template` | F of M-09 and M-19 |
| Index of the foreign region element | M-05 step 4, M-07 |
| Index of a foreign INTEGRATION or TEXT element | M-07 items 3 and 4 |

An end tag that matches an element pops the elements above it. Each element is pushed once and popped once, so all end tags together cost O(number of start tags). The M-11 check tokenizes the content of each `noscript` once more. The JSON validator (`src/JsonValidator.php`) is iterative and builds no value. UTF-8 validity (M-02) is one PCRE check (`preg_match('//u')`) that cannot backtrack.

### Limits

Measured on PHP 8.3 with 5 MiB inputs (`tests/LimitsTest.php` checks that each one is below 30 s and 256 MiB):

| Input | Time | Peak memory |
|---|---|---|
| 1.7 million nested `<b>` | 3.5 s | 64 MiB |
| 0.6 million nested elements with different names | 1.0 s | 108 MiB |
| 1 million elements nested as `<table><tr><td>` groups | 2.4 s | 80 MiB |
| One end tag that pops 1 million elements | 2.3 s | 64 MiB |
| 2.6 million attributes in one tag | 1.1 s | 5 MiB |

`bin/rqp-markup` sets `memory_limit` to 512 MiB.

### Choices where `markup.md` leaves room

| Case | Choice | Reason |
|---|---|---|
| `<![CDATA[` without a `>` before EOF (M-12) | Invalid (M-12) | Fail closed: "the first `>`" does not exist |
| `<?` that reaches EOF | A comment token to EOF | The current WHATWG text emits no token, older parsers emit a comment. A comment is the stricter choice: every rule that accepts a comment also accepts no token. |
| CDATA in the content of a foreign INTEGRATION or TEXT element (M-07 items 3 and 4) | Invalid | M-04 lets a tokenizer read CDATA as a bogus comment, and M-07 item 3 says "no CDATA". The same reading is used for item 4. Example: `<svg><style><![CDATA[a{}]]></style></svg>` is invalid. |
| `frameset` start tag in a foreign region (M-10) | Invalid | M-10 says "the document" without a context |
| `select` in M-19 | Kept, but not reachable | M-08 rejects a block start tag inside `select` first |

## Related

- `../CONTRACT.md`: command line and result object
- `../README.md`: differential runner
- `docs/engineering/protocol/markup.md`: normative rules
