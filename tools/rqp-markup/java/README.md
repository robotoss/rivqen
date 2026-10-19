# RQP markup parser candidate: Java

<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- SPDX-FileCopyrightText: The Rivqen Authors -->

The Java 21 candidate of WP-17 S1 and S3 (tasks T-05, T-08, T-11, T-13, T-16 and, in S3, T-06; in S4, T-06: H-78, H-84, H-86; in S4, T-10d: H-85a). It also implements the rules of WP-17 S3 (H-71, H-73, H-74, H-75): the main-pass rules on the `noscript` content (M-11 items 7 to 9, M-26 item 7), `svg` and `math` in `html` block content with `FOREIGN_ALLOWED` (M-22 item 5, M-23), `svg` in the `noscript` content, and no mark for the end tag of a region root (M-06 item 4). It implements the RQP markup rules of `docs/engineering/protocol/markup.md` §2 (rqp/1, rules M-01 to M-27, with M-24 items j and k), the human decisions on the edge cases E1 to E8 (H-21, H-22; `docs/engineering/protocol/markup-edge-cases.md`), the rules of the security review (H-26 to H-28, H-30 to H-32: M-06 items 2 to 4, M-11 item 5, M-13 item 3, and the depth counter D of M-26), the rules of round 2 of the security review (H-35 to H-38: M-09 conditions 1 and 2, M-10 item 2, M-11 item 6) and the command line of `tools/rqp-markup/CONTRACT.md`.

**Status:** DESIGN (tool for the differential test). WP-17 S5 T-06d: all 355 golden fixtures pass (`diff.mjs --only java`: 355/355 OK) after H-94 to H-97 and DL-055 (section 4.5: M-22 items 2 and 6, M-24 item g, M-11 item 10). The cross-check cases `s5-cases.json`, `s4-cases.json` and `s4b-cases.json` (both scripting views) show no document that the candidate accepts and a rule class rejects. Changed: `Name` (`template` and `noscript` leave `FORBIDDEN_CONTENT`, which has 14 names), `Tokenizer.hasShadowRootModeAttribute()` and `attributeName()`, `Analyzer.contentStartTag()`. S5 T-07 mutation on the S5 lines: 7/7 killed (section "Mutation"). Before S5: all 311 golden fixtures passed (`diff.mjs --only java`: 311/311 OK, WP-17 S4 T-10d). T-10d adds the implied table closure of H-85a (M-09, DL-050): `Analyzer.impliedClosure()` runs after the M-10 checks and before M-09 condition 1, not in block content and not in the C pass. The cross-check cases of `cases/s4b-cases.json` (68 cases, both scripting views) all hold. The cross-check cases of S3 and S4 (`cases/s3-cases.json`, `cases/s4-cases.json`, both scripting views) show no finding for this candidate (S4: 126 cases). The S3 and S4 diffs are mutation-tested in T-09. S4 changed `Name` (the `FORBIDDEN_CONTENT` flags; `FOREIGN_ALLOWED` is removed) and `Analyzer.startTag()`, `blockStart()`, `reservedType()` and the new `rejectedReference()` (H-78, H-84, H-86).

## 1. Build

Requirements: JDK 21 and Gradle (`gradle` on `PATH`). T-13 and T-16 used Gradle 9.7.1; T-05 to T-11 used Gradle 8.14. There is no Gradle wrapper.

1. Go to `tools/rqp-markup/java`.
2. Run `gradle --quiet installDist`.
3. The command is `build/install/rqp-markup/bin/rqp-markup`.

The differential runner (`tools/rqp-markup/diff.mjs`) runs the same two steps. A clean build with a cold Gradle daemon takes about 46 s (most of it is Error Prone); the runner allows 120 s.

When Maven Central rejects the requests of your host (HTTP 429), set a mirror of Maven Central for one command. The build uses Maven Central and the Gradle Plugin Portal when the property is not set.

```sh
ORG_GRADLE_PROJECT_rqpMavenMirror=https://maven-central.storage-download.googleapis.com/maven2 gradle --quiet installDist
```

## 2. Run

```text
rqp-markup <input.html>
rqp-markup --batch <fixtures-dir>
```

| Exit code | Meaning |
|---|---|
| 0 | Results are printed, for valid and invalid documents |
| 2 | Usage error or I/O error. stderr has the reason (the exception class, never a path or document content). |
| 3 | Internal error (a defect). For a defect in the analysis of a document, the command still prints a fail-closed result for it (`RQP_MARKUP_STRUCTURE`, `CONTRACT.md` §5), prints the results of the other documents of a batch, and stderr has the number of such documents. For other defects, stderr has the exception class only. |

The command reads at most 5 MiB + 1 byte of a file. The start script sets `-Xmx512m`.

The command reads only regular files (`CONTRACT.md` §2 item 7). It follows symbolic links. If the path of single mode, or the `input.html` of a batch fixture, exists but is not a regular file (a FIFO, a device, a directory, a socket), the exit code is 2. The command checks the file type before it opens the file, so it never waits on a FIFO. A subdirectory of a batch without `input.html` is skipped.

FACT. Remaining window: Java has no portable way to open a file without blocking and then check its type. The check and the open are two steps. A process that has write access to the directory can replace a regular file by a FIFO between them, and the open then waits. Use input directories that only trusted users can write to.

## 3. Test

Run these commands in `tools/rqp-markup/java`:

| Check | Command |
|---|---|
| Format (google-java-format through Spotless) | `gradle spotlessCheck` |
| Unit, property and fixture tests (Error Prone and NullAway run in the compile) | `gradle test` |
| All fixtures through the command line | `node tools/rqp-markup/diff.mjs --only java` (from the repository root) |
| Cross-check against parse5 | `tools/rqp-markup/crosscheck/README.md` (section 3.1) |
| Mutation testing (PIT) | `gradle pitest -Ppitest.targetClasses="$(node <root>/tools/mutation/diff-ranges.mjs --base main --ext .java --format pit-classes)"` |

Until WP-17 is merged, use `--base wp/WP-17`: against `main` the whole package is new. The PIT report is in `build/reports/pitest/index.html`. The PIT run leaves out `LinearTimeTest`: it checks time on 5 MiB inputs, not behavior.

Mutation results of T-07 (WP-17 S5; base `a56b45d`; PIT 1.30.0; classes `Analyzer`, `Name`, `Tokenizer`; 166 s): the whole classes have 754 of 788 mutants killed (95.7 %). On the lines that S5 changed (`Analyzer` 404 to 406 and 424, `Name` 89 and 121, `Tokenizer` 42, 105, 246 to 253, 282 and 426 to 428), 7 of 7 mutants are killed (100 %). No test was added and no survivor is on these lines.

Mutation results of T-09 (WP-17 S4; base `9bd8c32`; PIT 1.30.0; classes `Analyzer` and `Name`; 61 s of analysis, 66 s with the build): the whole classes have 454 of 474 mutants killed (95.8 %) after the new tests (451 before). On the lines that S4 changed, 85 of 92 mutants are killed (92.4 %); before the tests it was 82 of 92 (89.1 %). The new cases in `RulesTest` are the letters `Z`, `a`, `z` after `&`, the references `&#65;`, `&#90;`, `&#97;`, `&#122;`, `&#57;`, `&#xA;`, `&#xf;` (reserved) and `&#64;`, `&#91;`, `&#96;`, `&#123;`, `&#127;`, `&#33a;`, `&#a;` (not reserved). The 7 others are equivalent:

| Location | Mutant | Reason |
|---|---|---|
| `startTag()` | `noscriptBase < 0` → `<= 0` | `noscriptBase` is the stack size after the push of `noscript`, so it is at least 1 |
| `impliedClosure()` | `n < MAX_CLOSURES` → `<=` | The longest chain below `table` has three elements (`tbody`, `tr`, `td`), so the loop ends by F that allows the tag, before a 4th closure |
| `impliedClosure()` | `f < 0` → `<= 0` | A table family element is above `html`: its index is at least 1 |
| `rejectedReference()` | `at >= to` → `>`; `i < to` → `<=` (twice) | The byte at `to` is a quote, white space or `>`: it is not a letter, `#`, `x`, `X` or a digit |
| `rejectedReference()` | `digits++` → `digits--` | `digits` is only compared with 0 |

Mutation results of T-09 (WP-17 S3; base `7f36912`; PIT 1.30.0; classes `Analyzer`, `Name`, `TokenStack`): the whole classes have 473 of 487 mutants killed (97 %). On the lines that S3 changed, 68 of 72 mutants are killed (94.4 %), and no test was added. The 4 others are equivalent:

| Location | Mutant | Reason |
|---|---|---|
| `Analyzer.startTag()`, `endTag()` | `noscriptBase >= 0` → `> 0` | `noscriptBase` is the stack size after the push of the `noscript` element, so it is at least 1 |
| `Analyzer.contentEndTag()` | `root >= 0` → `> 0` | A region root is inside the block element, so its index is at least 1 |
| `Analyzer.noscript()` | call of `doctype()` removed | In C, `doctype()` cannot throw: a text-only element takes all tokens as text, `select` and block content do not hold a `noscript` element (M-08, M-22 item 2) |

Mutation results of T-16 (2026-10-11; base `5224828`; PIT 1.30.0):

| Scope | Detected (killed, time-out or memory error) | Rate | Not detected |
|---|---|---|---|
| T-16 changed lines (`Analyzer`, `TokenStack`) | 26 of 26 | 100 % | — |
| T-16 changed classes (`Analyzer`, `TokenStack`) | 430 of 440 | 97.7 % | 10 equivalent: the same mutants as items 1 and 2 of the T-13 list below |

Mutation results of T-13 (2026-10-11; base `4fff64f`; PIT 1.30.0):

| Scope | Detected (killed, time-out or memory error) | Rate | Not detected |
|---|---|---|---|
| T-13 changed lines | 109 of 110 | 99.1 % | 1 equivalent (`TokenStack.tableFamilyIndex`, below) |
| T-13 changed classes (`Analyzer`, `Main`, `Name`, `NameTable`, `Result`, `TokenStack`) | 460 of 473 | 97.3 % | 10 equivalent, 3 accepted |

The not-detected mutants of T-13:

1. `TokenStack.tableFamilyIndex`: `return -1` → `return 0` for an empty stack. Equivalent: a pop clears `known[0]`, so `tableFamily()` gives null and `impliedParts` gives no part for index 0 of an empty stack, as for -1.
2. The 9 equivalent mutants of T-08 and T-11 in `Analyzer` (below): the `&` scan of M-13 now also finds a byte of 0x80 or more; the byte at `to` is still ASCII, so `i < to` → `i <= to` stays equivalent.
3. The 3 accepted mutants in `Main.main`, which calls `System.exit`.

Two survivors of the first T-13 run were fixed: a test for ASCII control characters in a script `type` (M-13 item 3 is about bytes of 0x80 or more only), and the `NameTable` class with its own tests (the interning of F-2 has no effect on results, so only a unit test sees it).

Mutation results of T-11 (2026-10-11; base `edab313`):

| Scope | Detected (killed, time-out or memory error) | Rate | Not detected |
|---|---|---|---|
| T-11 changed lines (`Analyzer`, `TokenStack`) | 6 of 6 | 100 % | — |
| T-11 changed classes (`Analyzer`, `TokenStack`) | 331 of 340 | 97.4 % | 9 equivalent (below) |
| All classes of the package | 901 of 935 | 96.4 % | 31 equivalent, 3 accepted |

T-11 adds no surviving mutant: the not-detected mutants are the same as in the T-08 runs. Mutation results of T-08 (2026-10-09):

| Scope | Detected (killed, time-out or memory error) | Rate | Not detected |
|---|---|---|---|
| T-08 changed lines (`Analyzer`, `TokenStack`) | 37 of 39 | 94.9 % | 2 equivalent (below) |
| T-08 changed classes (`Analyzer`, `TokenStack`) | 328 of 337 | 97.3 % | 9 equivalent |
| All classes of the package | 898 of 932 | 96.4 % | 31 equivalent, 3 accepted |

In the whole-package run, 29 of the 31 equivalent mutants are in code that T-08 did not change. They are the same mutants as in a run on the T-07 code (T-05 triage). The 3 accepted mutants are in `Main.main`, which calls `System.exit`; the differential runner runs it on every fixture.

The 2 equivalent mutants on the changed lines are both in `Analyzer.listItemImplied`:

1. `list < 0` → `list <= 0`. A list at stack index 0 is never in the content (the block element is below every content element), so `!stack.inContent(list)` gives the same result.
2. `nearestListItem() > list` → `>=`. One stack entry is never both a list and a list item, so the two indexes are never equal when `list >= 0`.

The other 7 equivalent mutants in the two classes are `x >= 0` → `x > 0` on `textOnly` and `noscriptContent` (a foreign text element is always above its `svg` or `math` element, so its index is at least 1; a `noscript` content offset is at least 10) and `i < to` → `i <= to` in the `&` scan of M-13 (the byte at `to` ends the value or the attribute name: a quote, white space, `=`, `/` or `>`, never `&`).

| Test class | What it checks |
|---|---|
| `RulesTest` | One group of tests for each rule M-03 to M-27, on small documents, including M-24 items j and k, the decided edge cases E1 to E8 and the rules of the security review: M-06 items 2 to 4 (every FORMATTING name, every pushed SPECIAL name, foreign elements, stray table end tags, the mark and how it passes down), M-11 item 5 (unclosed elements, stray end tags, crossings, E3, names after the intern table is full) and M-13 item 3 (dotless i, long s, other non-ASCII values, also in `noscript` content and in a foreign region). Round 2 (H-35 to H-38): M-09 condition 1 (`col` in a template, also in a table or a cell around the template), M-09 condition 2 (every table part, `col` and `table` with an element above F in table context; the check comes before the implied parts of M-26), M-10 item 3 (H-43: `for` in any case, empty, unquoted, duplicated, with other attributes; `data-for`, `forx`, `for` on other elements and a foreign `template` are valid) and M-10 item 2 (nested forms, also in `object`, `template`, a table cell and block content; a form popped by a crossing is gone; a foreign `form` does not count) and M-11 item 6 (each table part and `col` in C without a table of C, a table of C that is closed again, E3, the cell and caption repros). Each rule item has a valid neighbor and the review repros. Each invalid case checks the error code and the rule ID. |
| `NameTest` | The name sets `SPECIAL`, `FORBIDDEN_CONTENT` (H-94, H-95: 14 names) and `FORMATTING` of the `Name` enum equal the lists of `markup.md` §2.1 (copied into the test), and `svg` and `math` are not in `FORBIDDEN_CONTENT` (H-73) |
| `RawTextInContentTest` | M-04 in the content of an html block (H-78): a block end tag or block start tag in `script` or `style` text is text, the escaped and double-escaped states, `</SCRIPT >`, `</script foo>`, `</scripty>`, raw text without an end (`M-20`), M-13 on a `script` in the content, linear time |
| `ForeignContentTest` | WP-17 S3 rules, each test checks the rule that fails: M-22 in a region (valid icons and math; since H-78 `a`, SMIL, `foreignObject`, `script`, `style`, `image` and unknown names are valid; `<svg><script>a<b</script>` is M-07; a breakout name is M-07), M-23 in a region of the content (root end tag, inner end tag, stray end tag, nested `svg`, end of input: M-20, D in a region), M-06 item 4 with the region root (H-75), M-11 items 7 to 9 (`html`, `body`, `head`, region in C, tokenizer state in the region, rules of the main pass on C: M-09, M-10, M-12, M-26) |
| `NameTableTest` | F-2: a known name is the enum constant, equal unknown names share one string, the table keeps at most 1 024 names |
| `LimitsTest` | M-01, M-02 and M-26 at their limits. Depth counter D (M-26, H-25, H-31): D = 256 is valid and 257 is `RQP_MARKUP_LIMIT` in HTML context, in a foreign region, outside blocks and in block content. Void elements, self-closing `svg` and `math`, self-closing foreign elements, raw text and `noscript` content do not count; the `noscript` element, a self-closing flag on another HTML name and the TEXT elements count. D comes back after exact pops (matching end tags, the end of a foreign region, a block). A crossing keeps its units (`<span><div></span>` keeps 2, the review repro with 300 of them is `LIMIT`), the dirty mark passes down on each pop, and the pop of `html` or `body` gives nothing back. Implied parts: the row group, row and column group count once for each F and come back with F; a `template` hides the table; an implied part above the limit fails before the push; 64 levels of `<table><td>` and a block are `LIMIT`. The other rules of a start tag come before the D check. |
| `TokenStackTest` | The fixed capacity of 256 (the stack never has more elements than D): the stack is full after 256 pushes, a push on a full stack throws `IllegalStateException` (a defect of the caller), a pop makes room |
| `TokenizerTest` | Token boundaries of every tokenizer state, attribute raw values, self-closing flags |
| `JsonTextTest`, `Utf8Test` | M-25 and M-02 on their own |
| `FixturesTest` | Every fixture equals its `expected.json`; for an invalid fixture, the reported rule is in `fixture.json` |
| `MainTest` | Command line, exit codes, batch order, JSON escaping, no document content in messages. F-4: an unexpected exception in the analysis gives a fail-closed invalid result with the exception class only; the command prints it, keeps the other results of a batch and exits with code 3 |
| `PropertyTest` | Random input with fixed seeds (own generator, no extra dependency): no exception for any bytes; for valid results, ordered blocks inside the input, and `sha256`, `template_revision`, `page_revision` equal an independent computation; replacing each block content with plain text keeps the document valid and keeps the template revision. M-26 depth against a model stack: 3 000 random walks with exact pops around the limit in HTML context, block content and a foreign region (473 go above 256, 315 stay exactly at 256). M-06 items 1 to 4, M-09 conditions 1 and 2, M-10 item 2 and D (dirty mark, implied parts) against `StackModel`, a plain implementation of the rule text with lists: 12 000 random walks with crossings, stray end tags, table parts (also `colgroup`), templates, forms, `html` and `body` in HTML context, exact pops in block content, and crossings in a foreign region (seed 32: 7 107 valid, 538 of them with D of 248 or more; errors: 619 `LIMIT`, M-06 item 1 2 048, item 2 306, item 3 1 239, item 4 192, M-09 condition 1 151, condition 2 187, M-10 item 2 151). The analyzer must give the same rule as the model at the same token, and each outcome must occur more than 100 times. The generator also makes the shapes that M-24 j and k, E1 and E5 keep valid (lists in list items, text and comments directly in a table, CDATA in SVG `style` and `desc`). |
| `ImpliedClosurePropertyTest` | H-85a: random table trees with every end tag (D') and the same documents without the end tags that the closure of the next table part start tag supplies (D). Both are valid and have the same blocks (4 000 documents, fixed seed) |
| `LinearTimeTest` | 46 worst-case inputs of almost 5 MiB finish quickly (a quadratic step would take hours). Eleven of them are valid and keep D and the token stack at or near 256 for the whole input: matching end tags in HTML context, pushes and pops on a full stack, stray end tags over a full stack, one end tag that pops 255 elements (once: a crossing keeps its units, so a valid document crosses at most 256 elements in total), nested lists in block content (also over lists opened before the block, M-24 c, j), an `svg` region at the limit, unique names, and 2^16 tag names that have the same hash code. They make sure that a push, a pop or a stack query stays O(1). Four inputs put the stack of the `noscript` content (M-11 item 5) through up to 1.7 million elements (balanced, and open to the end of the input), unique names and names with one hash code. Twelve deep inputs without the limit must give `RQP_MARKUP_LIMIT` (M-26) quickly, also repeated crossings and tables in cells. |

### 3.1 Cross-check against parse5

Run the commands of `tools/rqp-markup/crosscheck/README.md` §3 with `--only java` and a memory file system for `TMPDIR` (`/dev/shm` on Linux). Results on 2026-10-11 (T-16, after the rules of round 2 of the security review; macOS, `TMPDIR` in a temporary directory):

| Run | Documents | Valid (valid with blocks) | Findings |
|---|---|---|---|
| `check --scripting both` (all fixtures) | 194 | 71 | 0 |
| `cases --cases cases/review2-cases.json` | 21 | 7 | 0: each repro is `RQP_MARKUP_STRUCTURE`, each control is valid with the parse5 blocks |
| `sweep --seed 63 --count 20000 --chunk 10000 --max-tokens 20 --profile review2 --scripting both` | 20 000 | 2 969 (1 375) | 0 |

In the seed 63 sweep, the candidate accepts no document that only one rule class rejects. The depth invariant fails on 1 096 documents; the candidate accepts none of them. The sweep took 1.5 minutes.

Results on 2026-10-11 (T-13, after the rules of the security review):

| Run | Documents | Valid (valid with blocks) | Findings |
|---|---|---|---|
| `check --scripting both` (all fixtures) | 180 | 67 | 0 |
| `cases --cases cases/review-cases.json` | 30 | 9 | 0: each case gives the result of its rule class |
| `sweep --seed 33 --count 20000 --chunk 10000 --max-tokens 20 --profile review --scripting both` | 20 000 | 3 441 (1 886) | 0 |
| `sweep --seed 5 --count 50000 --chunk 10000 --scripting both` (classic profile) | 50 000 | 20 573 (6 167) | 0 |

In the seed 33 sweep, the rule classes of the cross-check reject 3 123 (`M-05-stray-table-end`), 5 372 (`M-06-formatting`), 4 284 (`M-06-formatting-marked`), 5 800 (`M-11-balance`), 3 508 (`M-13-non-ascii`) and 1 499 (`M-26-depth`) documents. The candidate accepts no document that only one class rejects. The depth invariant fails on 122 documents; the candidate accepts none of them.

Results on 2026-10-09 (T-08):

| Run | Documents | Valid with blocks | Findings |
|---|---|---|---|
| `check --scripting both` (all fixtures) | 133 | — | 0 |
| `cases` (edge-case inputs) | 34 | — | Each result as decided in H-21 and H-22 |
| `sweep --seed 4 --count 200000 --max-tokens 20` | 200 000 | 23 785 | 0 |
| `sweep --seed 5 --count 100000 --max-tokens 20 --scripting both` | 100 000 | 11 921 | 0 |
| `sweep --seed 6` and `--seed 7`, `--count 200000 --max-tokens 40 --scripting both` | 400 000 | 27 692 | 0 |

Before T-08, the seed 5 sweep gave 2 block-byte disagreements (M-24 j shapes) and 2 222 manifests in the scripting-disabled view (E8), summed over three candidates.

## 4. Design

### 4.1 Why an own tokenizer and not jsoup

DL-012 allows an own tokenizer. This candidate uses one, and no runtime dependency. Reasons:

1. jsoup's tokenizer (`org.jsoup.parser.Tokeniser`, `Token`) is package-private (checked in the jsoup `master` source on 2026-10-09). The public API gives a tree, not tokens.
2. The rules need what a tree does not keep: stray end tags, start tags that the tree builder ignores, the self-closing flag, raw (undecoded) attribute values and the first of two duplicate attributes.
3. jsoup works on a Java `String` (UTF-16). The offsets must be UTF-8 byte offsets.
4. jsoup inserts the children of `noscript` as elements (scripting-disabled view). RQP uses the scripting-enabled view (M-04, DL-009).

A tree may not be the source of truth for validity (DL-008). A tree library would only add a second view that the candidate must not use.

### 4.2 Structure

| Class | Role |
|---|---|
| `Utf8` | M-02: strict RFC 3629 validation |
| `Tokenizer` | WHATWG tokenizer subset over UTF-8 bytes (section 4.3). Reports start tags, end tags, comments (also bogus comments, processing instructions and CDATA) and DOCTYPE with byte offsets. It does not report character tokens: no rule needs them. |
| `TokenStack` | The token stack of M-05 with O(1) queries: nearest element with a name (a chain of same-name entries); F of M-09, the nearest table-family or `template` element of M-09 condition 1, and the nearest list and list item of M-24 c and j (each entry keeps the index of the nearest such entry at or below it); the foreign region; open-element counters. Per entry also the state of M-26 (dirty flag, implied parts) and M-06 item 4 (special-crossed mark). Fixed capacity of 256 entries (M-26): the arrays do not grow. |
| `Analyzer` | One pass over the tokens: M-01 to M-27 with the depth counter D, then the hashes and revisions of `CONTRACT.md` §4. Its `analyze` method is the fail-closed boundary (F-4). |
| `NameTable` | Tag names of one document, interned (F-2) |
| `JsonText` | M-25: RFC 8259 validation without recursion, depth at most 64 |
| `Name` | The name sets of `markup.md` §2.1 as flags, also `SPECIAL` and `FORMATTING` (no hash keyed by input for the sets) |
| `Result`, `ResultJson`, `ErrorCode` | Result object and its JSON form |
| `Main` | Command line |

### 4.3 Tokenizer states

All structural characters are ASCII, so the tokenizer reads the bytes of valid UTF-8 directly. CR counts as white space (preprocessing changes CR LF to LF; token boundaries do not change). A byte order mark at offset 0 is not skipped: its bytes are not ASCII, so they are characters in the data state and cannot start or end a token. The tokens are the same as when the tokenizer starts after the mark (M-02).

| States | Notes |
|---|---|
| Data, tag open, end tag open, tag name | ASCII upper case to lower case; U+0000 in a name becomes U+FFFD, as WHATWG does |
| Attribute states (all quoting forms), self-closing start tag | Only the first `data-rq-block` and the first `type` attribute are kept, as raw byte ranges; a flag records any `for` attribute |
| Comment states | The comment less-than sign states only add parse errors, so they are folded into the comment state |
| Bogus comment, `<?` | End at the first `>` |
| DOCTYPE | Ends at the first `>` |
| `<![CDATA[` | Read as a bogus comment. The token tells whether the first `>` ends a `]]>` that starts after `<![CDATA[` (M-12). |
| RCDATA, RAWTEXT, PLAINTEXT | Appropriate end tag: the name (ASCII case-insensitive), then white space, `/` or `>` |
| Script data | All escaped and double-escaped states |

A tag that the input ends inside is dropped, as in WHATWG ("EOF in tag").

### 4.4 Invariants

1. Every loop advances. The tokenizer reads each byte a bounded number of times. Each stack entry is pushed once and popped once, and a push, a pop and each stack query are O(1) (`TokenStack`). The index of the nearest element by name is a `HashMap` with at most 256 keys (M-26). Names with the same hash code make a lookup O(log 256) at most, because `HashMap` keeps a large bucket of `String` keys as a tree. One end tag pops at most 256 elements. M-06 items 1, 2 and 4 and the marks of item 4 look only at the elements that the end tag pops, in the same loop. The rules of round 2 add O(1) work for each start tag: M-09 conditions 1 and 2 read the index chain of F and compare F with the current node, M-10 item 2 reads the counter of open HTML `form` elements, and M-11 items 6 to 9 read one counter and the stack size. The total work is O(n) for an input of n bytes.
2. D (M-26) is checked after each push and after each implied part; D > 256 rejects the document (`RQP_MARKUP_LIMIT`, rule M-26). Each element on the token stack holds at least 1 of D (its push), and only an exact pop of that element gives it back, so the stack never has more elements than D. The token stack has a fixed capacity of 256 entries: 2 reference arrays, 4 int arrays, 4 boolean arrays and 1 byte array of 256 entries each, and at most 256 name references, for every input. A push on a full stack is a defect and throws `IllegalStateException`.
3. Tag names are interned for each document (`NameTable`, F-2): a known name is the constant of `Name`, an unknown name is one string while the table has fewer than 1 024 names. So the token stack keeps at most 256 name references and the table at most 1 024 strings. The elements of a `noscript` content C (M-11 items 5 and 8, H-71) are on the token stack of the main pass, above the `noscript` element, so they count in D and the stack stays bounded by D (256 entries) also inside C. The pass on C adds three scalars: the stack size at the start of C, the number of `table` elements of C (item 6), and the values of D and of the dirty mark of the `noscript` element, which the pass restores after the end tag `noscript` (DL-044 c).
4. The other memory is O(n): the copy of each `noscript` content (the contents do not overlap) and the blocks. Measured in T-13 with the installed command (`JAVA_OPTS=-Xmx…`, 5 MiB inputs): unique names in the main pass need less than 24 MiB. (T-13 also measured 700 000 unknown names and 1.7 million `<b>` left open in `noscript` content. Since S3 the elements of C are bounded by D, so such an input ends with `RQP_MARKUP_LIMIT` after 256 elements and needs no extra memory.) The start script allows 512 MiB.
5. M-01 is checked before anything else; the command reads at most 5 MiB + 1 byte.
6. No recursion (JSON uses a fixed array of 64 entries).
7. A failed rule throws an internal `Reject` that the analysis catches. No other exception is expected. If one occurs (a defect), `Analyzer.analyze` catches every `RuntimeException` and returns an invalid result (`RQP_MARKUP_STRUCTURE`) whose rule is `internal-error:` and the exception class name (F-4): a caller never uses blocks of a document that was not checked to the end. `Main` prints this fail-closed result and exits with code 3. `Main.main` catches every other `Throwable` (for example `OutOfMemoryError`), prints its class name only, and exits with code 3.
8. Messages never contain document content or paths.

### 4.5 How the rules are read

These points are where the text of `markup.md` needed an interpretation, or where the human decided an edge case (E1 to E8: H-21, H-22). Each one is covered by a test.

| Rule | Reading in this candidate |
|---|---|
| M-05, M-08, M-19, M-24 | "Open" counts elements pushed in HTML context. Elements pushed in a foreign region are never `select`, `template`, `a`, … for these rules. |
| M-03 (E6) | A `<?` without `>` is a comment that ends at the end of the input. In foreign text content it is invalid (M-07). |
| M-07 rule 3 (E1) | "Character tokens only" is the normative part. A `<` that the tokenizer emits as a character (`a < b`) is allowed. |
| M-07 rules 3 and 4 (E2) | The end of input inside an integration point or a TEXT element of a foreign region is not an error by itself. |
| M-07 rules 3 and 4 (E5) | A CDATA section (`<![CDATA[`, case-sensitive) that satisfies M-12 is allowed in text-only content. A lower-case `<![cdata[` is a bogus comment and stays invalid there. |
| M-07 rule 4 | `noscript` is a TEXT name, so `<svg><noscript>` content is text only. `<svg><title/>` is invalid (TEXT and self-closing). |
| M-10 (E7) | A `frameset` start tag is invalid also in a foreign region. |
| M-09 implied closure (H-85a, DL-050) | `Analyzer.impliedClosure()` runs in `startTag()` after the M-10 item 2 check and before `tablePart()` (M-09 condition 1, the list and condition 2), only when no html block is open and the pass is not the C pass. For a start tag with a name in `TABLE_PART` or `col`, it loops at most 3 times (`MAX_CLOSURES`): find F (the search stops at a `template`); stop when F is none or the list of the name allows it (`allowedIn`); when F is `tr`, `tbody`, `thead`, `tfoot` or `colgroup` and is not the current node, the document is invalid (`M-09`, condition 2); else run the end tag steps for F: the M-06 item 1 guard (`guardCrossing`, with the table-part exception of an end tag of the family) and `popTo` (M-06 item 4 mark, the special-crossed mark as `</td>` passes it, M-26 items 4 and 5). A table is never closed, and the closure releases units of D only as an exact pop does. Condition 1 runs after the closure, so `<template><tr><td>a<col>` is invalid. |
| M-09 condition 1 (H-35) | The search walks the same index chain as the search for F: the nearest element pushed in HTML context with a name in `TABLE_FAMILY` or the name `template`. A `col` start tag is invalid when it is a `template`, also when a table or a cell is open below the template (`<table><template><col>`). A `col` in a foreign region is a foreign element and is not checked. |
| M-09 condition 2 (H-37) | Checked after the table of M-09 and before M-10 item 2, the implied parts and the push of M-26. A `table` start tag passes the table of M-09 for every F; condition 2 applies to it as to the table parts and `col`. A `table` start tag while F is `table` and the current node is accepted (`<table><table></table></table>`), as the text says. |
| M-10 item 3 (H-43) | A `template` start tag in HTML context with an attribute named `for` is invalid (`RQP_MARKUP_STRUCTURE`, `M-10`): ASCII case-insensitive name, any value or none, any occurrence (a duplicate that the tokenizer drops counts too). The tokenizer records every `for` name, not only the first. The check comes right after M-07 text-only content and `frameset`, before M-13, M-08, M-09 and the other M-10 items. A `template` in a foreign region (`<svg><template for>`) is a foreign element and is valid. The content C of a `noscript` rejects every `template` start tag (M-11 item 3), so C needs no extra check. |
| M-10 item 2 (H-36) | The count of open `form` elements pushed in HTML context goes down on every pop, also on a crossing (`<div><form></div><form>` is valid). The rule applies also in a template, in a table cell and in block content: the text has no exception for template contents. The tags of a `noscript` content C are not in the main pass, so a `form` in C is not checked (section 6, item 4). |
| M-11 item 6 (H-38) | A start tag in C with a name in `TABLE_PART` or the name `col` needs at least one `table` of C. A counter goes up at a `table` start tag in C and down when the end tag `table` closes the top element of C. The check comes after items 2, 3 and 7 and before the rules of the main pass. M-09 now applies to C as well (item 8). |
| M-11 item 8 (H-71, DL-044 c) | The pass on C runs the main-pass handlers (`startTag`, `endTag`, `comment`, `doctype`) in place, on the token stack, D and the marks of the main pass, with a second tokenizer on the bytes of C followed by `</noscript>`. `noscriptStartTag` and `noscriptEndTag` add the checks of items 2, 3, 5, 6 and 7 before the handler. The end tag `noscript` of the pass is not given to `endTag`: it must start at the end of C (item 1) and the stack must be back at the size it had after the `noscript` push (item 5); then D and the dirty mark of the `noscript` element are restored. The error is the one of the rule that fails first in token order; for a C that is both unbalanced and deeper than D, the D limit (`RQP_MARKUP_LIMIT`) comes first. |
| M-11 item 3, 9 (H-74) | `svg` is not in the item 3 list; `math` is. An `svg` start tag without the self-closing flag in HTML context of C pushes a region root by the main-pass handler. A start tag in the region never changes the tokenizer state. |
| M-11 item 5 exception (H-75) | In C, an end tag is valid when its name is the name of the top element above the `noscript` element, or when the nearest element with its name is the region root. The second case is `popTo` with a crossing: it sets no mark and makes the new current node dirty. |
| M-11 item 7 | A start or end tag `html`, `body` or `head` in C, also in a region of C, is `RQP_MARKUP_STRUCTURE` with rule `M-11`. |
| M-11 (E3) | A `noscript` without an end tag: C is the rest of the input. |
| M-11, M-13 (E8) | The pass over C applies M-13 to its `script` start tags: error `RQP_MARKUP_RESERVED`, rule `M-13`. The check comes before items 2, 3 and 7. Start tags in raw text inside C (for example in a `style`) are not checked, as in the main pass. |
| M-22 items 2 and 6, M-24 item g (H-94, H-97) | `template` and `noscript` are not in `FORBIDDEN_CONTENT` (14 names). `contentStartTag` rejects a `template` start tag in HTML context with any attribute named `shadowrootmode` (`M-22`; the tokenizer records every occurrence, also a dropped duplicate, as it does for `for`). Item g: a table part or `col` is invalid (`M-24`) when no `table` opened in the content is open, or when F is none (`tableFamily()` stops at a `template`). A `template` is a normal M-23 element (exact end tag). |
| M-11 item 10 (H-95, DL-055) | No extra code: the pass on C already runs the main-pass handlers in place with `blockIndex` set, so `contentStartTag` (M-22 items 2, 3 and 6, M-24) runs on each start tag of C, `doctype()` rejects a DOCTYPE of C with `M-22`, and the end tags of C are M-23 through `noscriptEndTag` (item 5). A `template` in C stays invalid by M-11 item 3. |
| M-22, M-23 (H-73, H-75, H-78) | `contentStartTag` (items 2 to 4 and M-24) runs only in HTML context. A start tag in a foreign region of the content is checked only by the foreign handler (M-07); there is no `FOREIGN_ALLOWED` check since H-78. The block-start check (M-17) comes first for a start tag with `data-rq-block`. In a region, `contentEndTag` accepts an end tag with the name of the current node, or an end tag whose nearest element is the region root. Item 4 of M-22 (void end tag) is for HTML context only. The end of the input inside the region leaves the block pending: `M-20`. |
| M-04, M-22 raw text in the content (H-78) | No special code: the tokenizer state change of M-04 does not depend on the block, so the content tokens of a `script`, `style` or `iframe` are character tokens up to the appropriate end tag (script data with the escaped and double-escaped states). The end tag token is the current node, so M-23 pops it. `script`, `style`, `iframe`, `object`, `embed`, `link`, `meta`, `base`, `basefont` and `bgsound` are not in `FORBIDDEN_CONTENT`; `embed`, `link`, `meta`, `base`, `basefont` and `bgsound` stay in `VOID` (item 4 rejects their end tags), `object` is pushed and `GUARDED`. Raw text without an end leaves the block pending (`M-20`). Time and memory stay linear. |
| M-13 item 2 (H-84, DL-048 a) | `rejectedReference` runs at each `&` of the raw first `type` value: the next byte is an ASCII letter, or a numeric reference (`&#` digits, `&#x`/`&#X` hex digits, leading zeros, optional `;`, value capped at 0x110000) with a value 00 to 20, a letter, a digit, `/`, `+`, `-`, `.` or 0x80 or more. `&#;`, `&#x;`, a trailing `&` and `& ` are not rejected. Each digit is read at most twice. |
| M-16 (H-86) | `typeEquals` is tried for `application/json` and for `application/ld+json`; both give format `json` and M-25. The match is exact and case-sensitive on the raw value. |
| M-12 (E4) | A `<![CDATA[` without `>` before the end of input satisfies M-12: both readings end at the end of the input. |
| M-13 | Applies also to a `script` start tag in a foreign region. |
| M-19 rule 1 (`select`) | Not reachable: inside `select`, M-08 allows only `option`, `optgroup` and `hr`, and none of them can be a block. The check stays for clarity. |
| M-24 a | Only the block element (`p` or `span`) is checked, as the table says. |
| M-24 c, j | One check: let L be the nearest `ul`, `ol`, `menu` or `dl` on the stack. Invalid when there is no L, when L was opened before the block (c), or when an `li`, `dd` or `dt` is above L (j). An item above L was opened in the content, because L was. |
| M-26 (D) | The D checks are the last checks of a start tag: first the implied parts, then the push (markup.md §2.10: "on each push to the token stack and each implied part"). A start tag that breaks another rule (M-07 to M-10, M-13, M-15 to M-22, M-24) reports that rule, also when D is 256. Elements that M-05 does not push do not count: `VOID` names and self-closing `svg` and `math` in HTML context, self-closing elements in a foreign region. A self-closing flag on another HTML name is ignored, so `<span/>` counts. A `col` is not pushed, but its column group counts. The tags of the `noscript` content C do not count (item 7). |
| M-26 items 4, 5 (`html`, `body`) | By the name of the popped element, also for an element pushed in a foreign region (a foreign `html` element). This is the literal text and is stricter than needed. |
| M-26 items 4, 5 (pops) | Every pop path uses the same step: an end tag in HTML context, in a foreign region, in block content (M-23) and the end tag of a foreign text-only element (M-07). |
| M-06 item 2 | The crossed elements of an end tag in a foreign region are all foreign, so a `FORMATTING` end tag there (in practice `</a>`; the other names are in `BREAKOUT`) must pop exactly the current node. |
| M-06 item 3 | Checked only in HTML context and outside block content. In a foreign region a stray end tag is M-07; in block content it is M-23. |
| M-06 item 4 | The mark goes to the new current node after every end tag that pops, also after the exact pop of a marked element. An end tag that matches nothing and a start tag do not change it. A crossed foreign element sets the mark only when the matching element is not the region root (H-75); `popTo` reads the root index before the pops. |
| M-11 item 5 | A start tag of C opens an element by the push rule of the main pass (M-05 items 1 and 2). Names are compared after the lower-case step of the tokenizer. Items 2, 3 and 7 are checked at each start tag; item 5 at each end tag and at the end tag `noscript` (after item 1), so a document with an unclosed element and a manifest script in C reports `RQP_MARKUP_RESERVED`. |
| M-13 item 3 | A byte of 0x80 or more in the raw first `type` value of a `script` start tag. ASCII control characters (U+0000 to U+001F, U+007F) are not in item 3. |
| M-24 k | Applies to every start tag in the content, also to names that no set of §2.1 contains (`<x-y>`). Table parts and `col` pass item k; M-09 then checks them. Text and comments are not checked. Item k does not apply outside block content. |

## 5. Dependencies

There is no runtime dependency. Build and test dependencies (exact versions):

| Component | Version | License | Use |
|---|---|---|---|
| JUnit Jupiter (BOM) | 5.14.4 | EPL-2.0 (test only) | Tests |
| JSpecify | 1.0.1 | Apache-2.0 | `@NullMarked` (compile only) |
| Error Prone | 2.50.0 | Apache-2.0 | Compiler checks |
| NullAway | 0.14.2 | MIT | Null checks |
| Gradle plugin `net.ltgt.errorprone` | 5.1.1 | Apache-2.0 | Error Prone in Gradle |
| Gradle plugin `com.diffplug.spotless` | 8.10.4 | Apache-2.0 | Format check |
| google-java-format | 1.35.0 | Apache-2.0 | Format (1.37.0 fails in Spotless 8.10.4, section 6) |
| PIT | 1.30.0 | Apache-2.0 | Mutation testing |
| pitest-junit5-plugin | 1.2.3 | Apache-2.0 | PIT with JUnit 5 |
| Gradle plugin `info.solidsoft.pitest` | 1.19.0 | Apache-2.0 | PIT in Gradle |

## 6. Known problems

1. The two rule gaps that T-05 found with parse5 (nested list items, foster parenting in the content) are closed by M-24 items j and k (H-18). The candidate implements both (T-08).
2. google-java-format 1.37.0 (the version in `docs/engineering/standards/java.md`) fails inside Spotless 8.10.4 with `InvocationTargetException`. The build uses 1.35.0.
3. Batch mode reads directory names with the platform file name encoding of the JVM. With a non-UTF-8 locale, a non-ASCII directory name gives an I/O error (exit code 2). Use a UTF-8 locale.
4. Closed in WP-17 S3 (T-06): the rule gap of T-16 (M-09 condition 2 and M-10 item 2 did not apply to the tags of a `noscript` content C) is closed by M-11 item 8 (H-71). The pass on C runs the main-pass rules in place. The three shapes (`S3-H71-T16`, `S3-H71-H36-in-C-a`, `S3-H71-H36-in-C`) are invalid in the cross-check cases.
