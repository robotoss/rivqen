# RQP markup parser candidate: Java

<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- SPDX-FileCopyrightText: The Rivqen Authors -->

The Java 21 candidate of WP-17 S1 (task T-05). It implements the RQP markup rules of `docs/engineering/protocol/markup.md` §2 (rqp/1, rules M-01 to M-27) and the command line of `tools/rqp-markup/CONTRACT.md`.

**Status:** DESIGN (tool for the differential test). All 117 golden fixtures pass. The candidate follows the text of `markup.md` exactly, also where the text has known gaps (section 6).

## 1. Build

Requirements: JDK 21 and Gradle 8.14 (`gradle` on `PATH`). There is no Gradle wrapper.

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
| 3 | Internal error (a defect). stderr has the exception class only. |

The command reads at most 5 MiB + 1 byte of a file. The start script sets `-Xmx512m`.

## 3. Test

Run these commands in `tools/rqp-markup/java`:

| Check | Command |
|---|---|
| Format (google-java-format through Spotless) | `gradle spotlessCheck` |
| Unit, property and fixture tests (Error Prone and NullAway run in the compile) | `gradle test` |
| All fixtures through the command line | `node tools/rqp-markup/diff.mjs --only java` (from the repository root) |
| Mutation testing (PIT) | `gradle pitest -Ppitest.targetClasses="$(node <root>/tools/mutation/diff-ranges.mjs --base main --ext .java --format pit-classes)"` |

The PIT report is in `build/reports/pitest/index.html`. The PIT run leaves out `LinearTimeTest`: it checks time on 5 MiB inputs, not behavior.

Last mutation result (2026-10-09, all classes of the package): 885 of 916 mutants killed (96.6 %). The other 31: 28 are equivalent (the task report lists each with its reason) and 3 are accepted (`Main.main` calls `System.exit`; the differential runner runs it on every fixture).

| Test class | What it checks |
|---|---|
| `RulesTest` | One group of tests for each rule M-03 to M-27, on small documents. Each invalid case checks the error code and the rule ID. |
| `LimitsTest` | M-01, M-02 and M-26 at their limits |
| `TokenizerTest` | Token boundaries of every tokenizer state, attribute raw values, self-closing flags |
| `JsonTextTest`, `Utf8Test` | M-25 and M-02 on their own |
| `FixturesTest` | Every fixture equals its `expected.json`; for an invalid fixture, the reported rule is in `fixture.json` |
| `MainTest` | Command line, exit codes, batch order, JSON escaping, no document content in messages |
| `PropertyTest` | Random input with fixed seeds (own generator, no extra dependency): no exception for any bytes; for valid results, ordered blocks inside the input, and `sha256`, `template_revision`, `page_revision` equal an independent computation; replacing each block content with plain text keeps the document valid and keeps the template revision |
| `LinearTimeTest` | 21 worst-case inputs of almost 5 MiB finish quickly (a quadratic step would take hours) |

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
| `TokenStack` | The token stack of M-05 with O(1) queries: nearest element with a name (a chain of same-name entries), F of M-09, the foreign region, open-element counters |
| `Analyzer` | One pass over the tokens: M-01 to M-27, then the hashes and revisions of `CONTRACT.md` §4 |
| `JsonText` | M-25: RFC 8259 validation without recursion, depth at most 64 |
| `Name` | The name sets of `markup.md` §2.1 as flags |
| `Result`, `ResultJson`, `ErrorCode` | Result object and its JSON form |
| `Main` | Command line |

### 4.3 Tokenizer states

All structural characters are ASCII, so the tokenizer reads the bytes of valid UTF-8 directly. CR counts as white space (preprocessing changes CR LF to LF; token boundaries do not change). A byte order mark at offset 0 is not skipped: its bytes are not ASCII, so they are characters in the data state and cannot start or end a token. The tokens are the same as when the tokenizer starts after the mark (M-02).

| States | Notes |
|---|---|
| Data, tag open, end tag open, tag name | ASCII upper case to lower case; U+0000 in a name becomes U+FFFD, as WHATWG does |
| Attribute states (all quoting forms), self-closing start tag | Only the first `data-rq-block` and the first `type` attribute are kept, as raw byte ranges |
| Comment states | The comment less-than sign states only add parse errors, so they are folded into the comment state |
| Bogus comment, `<?` | End at the first `>` |
| DOCTYPE | Ends at the first `>` |
| `<![CDATA[` | Read as a bogus comment. The token tells whether the first `>` ends a `]]>` that starts after `<![CDATA[` (M-12). |
| RCDATA, RAWTEXT, PLAINTEXT | Appropriate end tag: the name (ASCII case-insensitive), then white space, `/` or `>` |
| Script data | All escaped and double-escaped states |

A tag that the input ends inside is dropped, as in WHATWG ("EOF in tag").

### 4.4 Invariants

1. Every loop advances. The tokenizer reads each byte a bounded number of times. Each stack entry is pushed once and popped once. The total work is O(n) for an input of n bytes.
2. The memory is O(n): the token stack, one name string per open element, the copy of each `noscript` content (the contents do not overlap) and the blocks. Measured: the worst inputs of 5 MiB need less than 96 MiB of heap.
3. M-01 is checked before anything else; the command reads at most 5 MiB + 1 byte.
4. No recursion (JSON uses a fixed array of 64 entries).
5. A failed rule throws an internal `Reject` that `Analyzer.analyze` catches. No other exception is expected. `Main.main` catches every `Throwable`, prints its class name only, and exits with code 3.
6. Messages never contain document content or paths.

### 4.5 How the rules are read

These points are where the text of `markup.md` needed an interpretation. Each one is covered by a test.

| Rule | Reading in this candidate |
|---|---|
| M-05, M-08, M-19, M-24 | "Open" counts elements pushed in HTML context. Elements pushed in a foreign region are never `select`, `template`, `a`, … for these rules. |
| M-07 rule 3 | "Character tokens only" is the normative part. A `<` that the tokenizer emits as a character (`a < b`) is allowed, although the sentence after it says "no `<`". |
| M-07 rules 3 and 4 | The end of input inside an integration point or a TEXT element of a foreign region is not an error by itself. |
| M-07 rule 4 | `noscript` is a TEXT name, so `<svg><noscript>` content is text only. `<svg><title/>` is invalid (TEXT and self-closing). |
| M-10 | A `frameset` start tag is invalid also in a foreign region. |
| M-11 | A `noscript` without an end tag: C is the rest of the input. |
| M-13 | Applies also to a `script` start tag in a foreign region. |
| M-19 rule 1 (`select`) | Not reachable: inside `select`, M-08 allows only `option`, `optgroup` and `hr`, and none of them can be a block. The check stays for clarity. |
| M-24 a | Only the block element (`p` or `span`) is checked, as the table says. |

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

1. **Gaps in `markup.md` (for the protocol owner).** A differential run against parse5 8.0.1 (WHATWG tree builder, scripting enabled) found valid documents where the WHATWG tree ends a block at a different byte. Two cases:
   - `<div data-rq-block="b"><ul><li><div><li>x</li></div></li></ul></div>`: the second `li` closes the first `li` and the inner `div`, so the inner `</div>` closes the block. M-24 c checks only that a list is open in the content. The same happens with `dd` and `dt`, and with `<dl><dd><dd>…</dd></dd></dl>` inside a block in a `dd`.
   - `<div data-rq-block="b"><table><div><table>x</table></div></table></div>`: content directly in a table is foster-parented, and a `table` start tag in table context closes the open table. No rule covers start tags in the content while F (M-09) is `table`, `tbody`, `thead`, `tfoot`, `tr` or `colgroup`.

   The candidate follows the current text. The task report proposes rule changes.
2. google-java-format 1.37.0 (the version in `docs/engineering/standards/java.md`) fails inside Spotless 8.10.4 with `InvocationTargetException`. The build uses 1.35.0.
3. Batch mode reads directory names with the platform file name encoding of the JVM. With a non-UTF-8 locale, a non-ASCII directory name gives an I/O error (exit code 2). Use a UTF-8 locale.
