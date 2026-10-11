# RQP markup: edge cases E1–E8

This page is a short risk study of eight edge cases of the [RQP markup rules](/engineering/protocol/markup) (rqp/1). For each case it gives the evidence, the options, the risk and one recommended reading. The human decided each case (protocol decision; E8 is also a security decision). The decided rule text is in [`markup.md`](/engineering/protocol/markup) §2; the cases are listed in [§2.11](/engineering/protocol/markup#_2-11-decided-edge-cases). Section 6 is the study of the findings of the security review (H-25 to H-31, 2026-10-11).

**Status:** <Badge type="warning" text="RESEARCH" /> Study of 2026-10-09, task WP-17/S1/T-07 ([sprint record](/engineering/plan/sprints/WP-17-S1)). <Badge type="info" text="DESIGN" /> **Decided on 2026-10-09:** E1–E7 as recommended (human decision H-21), E8 option A, reject with `RQP_MARKUP_RESERVED` (human decision H-22). The evidence below is the state before the decision ("Today" = the candidates before task T-08).

[[toc]]

## 1. Summary

Order of preference (from the brief): first **no false accept** (never accept a document where the block bytes can differ between the browser and the server), then **fewer false rejects**.

"Today" is the result of the three built candidates: Rust / Java / PHP. The Node.js candidate (T-04) was not available.

| Case | Today | Can block bytes differ? | Recommendation | Decision |
|---|---|---|---|---|
| E1 `<` as text in a foreign text element or integration point | valid / valid / valid | No | **Valid.** Keep "character tokens only"; delete "no `<`" | A, H-21 |
| E2 End of input in a foreign region, after all blocks | valid / valid / valid | No | **Valid** | A, H-21 |
| E3 `noscript` without an end tag | valid / valid / valid, with the M-11 checks on the rest of the input | No | **Valid**: C is the rest of the input, M-11 applies | A, H-21 |
| E4 `<![CDATA[` without `>` before the end of input | valid / valid / **invalid** | No | **Valid** (PHP changes) | A, H-21 |
| E5 CDATA in a foreign text element or integration point (`<svg><style><![CDATA[…]]>`) | **invalid** / **invalid** / **invalid** | No, when M-12 holds | **Valid** when M-12 holds (all three change) | A, H-21 |
| E6 `<?` that reaches the end of input | comment / comment / comment | No | **Comment to EOF** (no change) | A, H-21 |
| E7 `frameset` start tag in a foreign region | invalid / invalid / invalid | No | **Invalid** in every context (no change) | A, H-21 |
| E8 Manifest script in `noscript` | **valid** / **valid** / **valid** | No, but a scripting-disabled reader sees a second manifest | **Invalid, `RQP_MARKUP_RESERVED`** (all three change). Security decision. | A, H-22 |

In plain words:

- E1, E2, E3, E6 and E7 need no candidate change. Only the text of `markup.md` becomes exact.
- E4: PHP rejects a document that the other two accept. Both readings are safe. Accepting is the smaller rule.
- E5: today all candidates reject inline SVG with `<style><![CDATA[…]]></style>`. The WHATWG parser reads this as text, and M-12 already makes the token boundaries equal. Accepting removes false rejects at no risk.
- E8: today all candidates accept a manifest-like script inside `noscript`. A browser with scripting sees text. A reader with scripting disabled (crawler, server tool, a native HTML parser) sees a second manifest. The fix is one more check in M-11.

## 2. Method and evidence

<Badge type="warning" text="RESEARCH" /> Evidence sources:

1. **The three candidates**, built from `wp/WP-17` at `c8f79ba` (Rust: `cargo build --release`; Java: `gradle installDist`; PHP: `php bin/rqp-markup`).
2. **parse5 8.0.1** (WHATWG tree builder), with scripting enabled and disabled, through the cross-check tool `tools/rqp-markup/crosscheck/` (its `README.md` has the commands).
3. **The WHATWG standard.** The host `html.spec.whatwg.org` was not reachable from the sandbox on 2026-10-09. Exact quotations of the current text: NOT FOUND in this session. The statements below use the rules of `markup.md` §2 (checked against the `whatwg/html` source by T-01), the parse5 8.0.1 source, the html5ever source (`servo/html5ever`, `tree_builder/mod.rs`) and the page of `whatwg/html` pull request #12118.

To repeat the study, run these commands in `tools/rqp-markup/crosscheck`:

1. `npm ci --ignore-scripts`
2. `node src/cli.mjs cases --only rust,java,php` (the inputs are in `cases/edge-cases.json`; one row per input)
3. `node src/cli.mjs check --only rust,java,php --scripting both` (all fixtures)
4. `TMPDIR=/dev/shm node src/cli.mjs sweep --only rust,java,php --seed 5 --count 100000 --chunk 10000 --max-tokens 20 --scripting both`

Sweep results (2026-10-09, before the candidates implement M-24 items j and k):

| Sweep | Candidates | Documents | Valid with blocks | Findings |
|---|---|---|---|---|
| seed 4, max 20 pieces | Rust | 200 000 | 24 468 | 7 block-byte disagreements with parse5: all are M-24 j or k shapes |
| seed 5, max 20 pieces, both views | Rust, Java, PHP | 100 000 | 12 270 (Rust) | 2 block-byte disagreements (M-24 j shapes, the same in both views); manifest in the scripting-disabled tree (E8): 2 222 findings (3 candidates); PHP vs Java: 157 documents, all E4; Rust vs Java: 2 (section 4) |
| all 133 fixtures, both views | Rust, Java, PHP | 133 | — | Only the new M-24 j/k fixtures and the E8 draft. `--expected`: every valid `expected.json` agrees with parse5 in both views |

No sweep found a block-byte disagreement that M-24 j or k does not explain. In the scripting-disabled view, no block disagreement appeared that the scripting-enabled view did not also show. This supported `markup.md` §5 item 2 (M-11 makes both views equal). <Badge type="tip" text="FACT" /> The generator of that day did not reach all shapes: the security review of 2026-10-11 found block disagreements in both views that these sweeps did not show (section 6).

## 3. Cases

### E1 — `<` as text in foreign text content

**Decided: A, H-21.** Normative text: `markup.md` M-07 items 3 and 4 ("character tokens and CDATA sections only"; a `<` that the tokenizer emits as a character is allowed). Fixture: `EDGE-E1-FOREIGN-LT`.

Example: `<svg><desc>a < b</desc></svg>`, `<math><mi>1 <2</mi></math>`.

- **WHATWG:** in the tag open state, `<` followed by a character that is not an ASCII letter, `!`, `/` or `?` is a parse error. The tokenizer emits `<` as a character token and reconsumes in the data state. <Badge type="tip" text="FACT" /> parse5 8.0.1 does this (`_stateTagOpen`, default branch).
- **parse5:** the text `a < b` is a text node of `desc`. The block after the `svg` has the same bytes as in the candidates.
- **Today:** all three candidates accept (case `E1-desc-lt` and four more). A tag (`<b>`) in `desc` is rejected by all three (control case).
- **Options:**
  - A. Token reading: "character tokens only". A `<` that is a character token is allowed.
  - B. Byte reading: no `<` byte at all in the content.
- **Risk:** none for A. The content is characters in every WHATWG tokenizer, and no block can be in a foreign region (M-16). B only adds false rejects (MathML with a literal `<`).
- **How common:** low. Authors usually write `&lt;`.
- **Recommendation: A.** In M-07 item 3, replace the sentence "Thus, the content contains no `<`" with "There is no tag, comment or DOCTYPE token. A `<` that the tokenizer emits as a character (for example `a < b`) is allowed."
- **Draft fixture:** `EDGE-E1-FOREIGN-LT` (valid).

### E2 — End of input in a foreign region

**Decided: A, H-21.** Normative text: `markup.md` M-07 item 6 (the end of the input in a foreign region, also in an element of items 3 and 4, is not an error by itself). Fixture: `EDGE-E2-FOREIGN-EOF`.

Example: `…<p data-rq-block="a">x</p><svg><title>abc` and the input ends.

- **WHATWG:** EOF in foreign content is handled by the "in body" EOF rules: the parser stops. The elements stay open without an end tag.
- **parse5:** the block before the `svg` ends at its explicit end tag.
- **Today:** all three accept (`E2-*`, four inputs).
- **Options:** A. valid; B. invalid (the end tag of M-07 items 3 and 4 is missing).
- **Risk:** none for A. A block cannot be open at that point: a block start tag in a foreign region is invalid (M-16) and block content cannot contain `svg` or `math` (M-22). So every block has already ended at its explicit end tag. B only adds false rejects.
- **How common:** rare (cut output, broken templates).
- **Recommendation: A.** Add to M-07: "The input can end in a foreign region, also inside an element of items 3 and 4. This is not an error."
- **Draft fixture:** `EDGE-E2-FOREIGN-EOF` (valid).

### E3 — `noscript` without an end tag

**Decided: A, H-21.** Normative text: `markup.md` M-11 (C is the rest of the input; all items of M-11 apply). Fixtures: `EDGE-E3-NOSCRIPT-EOF`, `EDGE-E3-NOSCRIPT-EOF-BLOCK`.

Example: `…<noscript><img src="…">` and the input ends.

- **WHATWG:** with scripting enabled, `noscript` is RAWTEXT, so the rest of the input is text. With scripting disabled, the rest is markup.
- **parse5:** scripting on: one text node to EOF. Scripting off: the markup becomes elements (the input `E3-noscript-eof-block` gives two blocks in this view).
- **Today:** all three use C = the rest of the input and apply the M-11 checks to it: `E3-noscript-eof-img` and `E3-noscript-eof-text` are valid; `E3-noscript-eof-block` (block markup) and `E3-noscript-eof-comment` (C ends in a comment) are invalid.
- **Options:** A. C is the rest of the input, M-11 applies; B. invalid (no end tag).
- **Risk:** none for A. With scripting enabled no block can follow. M-11 items 2 and 3 keep blocks and parser-changing tags out of the scripting-disabled view, as for a closed `noscript`.
- **How common:** rare.
- **Recommendation: A.** Add to M-11: "When the `noscript` element has no end tag, C is the rest of the input."
- **Draft fixtures:** `EDGE-E3-NOSCRIPT-EOF` (valid), `EDGE-E3-NOSCRIPT-EOF-BLOCK` (invalid, M-11).

### E4 — `<![CDATA[` without `>`

**Decided: A, H-21.** Normative text: `markup.md` M-12 (with no `>`, both token forms end at the end of the input; the rule holds). Fixture: `EDGE-E4-CDATA-EOF`.

Example: `…<p data-rq-block="a">x</p><![CDATA[abc` and the input ends.

- **WHATWG:** in HTML content, `<![CDATA[` starts a bogus comment ("cdata-in-html-content" parse error). In foreign content it starts a CDATA section. Both end at EOF when there is no `>` (a bogus comment) or no `]]>` (a CDATA section). <Badge type="tip" text="FACT" /> parse5 8.0.1: comment `[CDATA[abc` in HTML content, text `abc` in `svg`.
- **Today:** Rust and Java accept; PHP rejects (it reads "the first `>`" of M-12 as required). This is the only difference between PHP and the others in all sweeps.
- **Options:** A. valid: no `>` means both forms end at EOF; B. invalid: fail closed.
- **Risk:** none for both. After `<![CDATA[` with no `>`, there is no other token. A block that is open there has no end tag and is invalid (M-20; control case `E4-control-cdata-eof-in-block`).
- **How common:** very rare.
- **Recommendation: A** (fewer false rejects, and two of three candidates already do it). Add to M-12: "When there is no `>` after `<![CDATA[`, the rule holds: both end at the end of the input."
- **Draft fixture:** `EDGE-E4-CDATA-EOF` (valid).

### E5 — CDATA in foreign text content

**Decided: A, H-21.** Normative text: `markup.md` M-07 item 5 (a CDATA section that satisfies M-12 is allowed in foreign `TEXT` and `INTEGRATION` content). Fixtures: `EDGE-E5-FOREIGN-CDATA`, `EDGE-E5-FOREIGN-CDATA-GT`.

Example: `<svg><style><![CDATA[.a{fill:red}]]></style></svg>`; also `<svg><script><![CDATA[…]]></script>`, `<svg><desc><![CDATA[…]]></desc>`, `<math><mi><![CDATA[…]]></mi>`.

- **WHATWG:** `<![CDATA[` starts a CDATA section when the adjusted current node is not an HTML element. The section ends at the first `]]>`, and its characters are text.
- **parse5:** in `svg style` and `svg script`: a text node (`a{}`). In the integration points `svg desc` and `math mi`: a **comment** `[CDATA[…]]`. parse5 does not set its foreign flag at integration points (`_setContextModes`). <Badge type="warning" text="RESEARCH" /> The html5ever tree builder has the check `adjusted_current_node_present_but_not_in_html_namespace`, with no exception for integration points; so html5ever probably gives a CDATA section there (not run in this study). So parsers differ in the tree (text or comment), but not in the end byte when M-12 holds.
- **Today:** all three reject all four inputs (M-07: a CDATA token counts as "not a character token").
- **Options:**
  - A. In foreign TEXT elements and integration points, allow a CDATA section that satisfies M-12.
  - B. Allow it only in foreign TEXT elements (`style`, `script`, …), not in integration points.
  - C. No change (invalid).
- **Risk:** none for A and B. With M-12, a CDATA section and a bogus comment end at the same byte, so every parser continues from the same byte in the data state. No block can be in a foreign region (M-16). The only difference is text or comment inside `svg`/`math`. A control input with `>` inside the CDATA section stays invalid by M-12 (`E5-control-cdata-gt`).
- **How common:** CDATA wrappers in SVG `style` and `script` come from XML habits and from SVG editors. Frequency on real pages: NOT FOUND. C would send every such page without RQP.
- **Recommendation: A** (one rule for items 3 and 4; B adds a distinction that buys nothing). Proposed text for M-07 items 3 and 4: "… MUST be character tokens and CDATA sections only. A CDATA section is a `<![CDATA[` that satisfies M-12; a parser that reads it as a bogus comment (M-04) counts that comment as a CDATA section."
- **Draft fixtures:** `EDGE-E5-FOREIGN-CDATA` (valid), `EDGE-E5-FOREIGN-CDATA-GT` (invalid, M-12).

### E6 — `<?` that reaches the end of input

**Decided: A, H-21.** Normative text: `markup.md` M-03 (a `<?` with no `>` is a comment to the end of the input). Fixtures: `EDGE-E6-PI-EOF`, `EDGE-E6-PI-EOF-HTML`.

Example: `…<p data-rq-block="a">x</p><?php echo 1;` and the input ends; `…<svg><desc><?x` and the input ends.

- **WHATWG (old):** `<?` starts a bogus comment; at EOF the comment is emitted. <Badge type="tip" text="FACT" /> parse5 8.0.1 does this (comment `?php echo 1;`).
- **WHATWG (new):** <Badge type="warning" text="RESEARCH" /> pull request whatwg/html #12118 ("Parse XML-style `<?target data?>` processing instructions") was merged on 2026-06-25. The pull request page says that a `>` always ends a processing instruction, and that `<?` followed by EOF is a bogus comment. One of its commits is titled "Drop PIs for eof-in-processing-instruction": a processing instruction that reaches EOF gives no token. The exact text of the merged states: NOT FOUND (spec host not reachable). Browser support: NOT FOUND.
- **Today:** all three read a comment that ends at EOF. `E6-pi-eof` and `E6-qm-eof` (HTML context) are valid; `E6-pi-eof-desc` (in an integration point) is invalid (M-07 item 3: a comment).
- **Options:** A. a comment to EOF (old parsers); B. no token (new parsers); C. both allowed.
- **Risk:** none for block bytes: no token follows, and an open block is invalid by M-20 (control case `E6-control-pi-in-block-eof`). The only visible difference is in foreign text content at EOF. A is the stricter reading: every rule that allows a comment also allows no token, so A never accepts what B rejects. C is not deterministic for fixtures.
- **How common:** very rare.
- **Recommendation: A** (no change; the same reading as a bogus comment at EOF, M-03). Add to M-03: "A `<?` that reaches the end of the input without `>` is a comment that ends there."
- **Draft fixtures:** `EDGE-E6-PI-EOF` (invalid, in `desc`), `EDGE-E6-PI-EOF-HTML` (valid).

### E7 — `frameset` in a foreign region

**Decided: A, H-21.** Normative text: `markup.md` M-10 (no `frameset` start tag in any context, also in a foreign region). Fixture: `EDGE-E7-FOREIGN-FRAMESET`.

Example: `<svg><frameset/></svg>`.

- **WHATWG:** `frameset` is not a breakout name in foreign content, so it is an SVG (or MathML) element with no effect. <Badge type="tip" text="FACT" /> parse5 8.0.1: an element `frameset` in the SVG namespace.
- **Today:** all three reject (M-10: "the document MUST NOT contain a `frameset` start tag").
- **Options:** A. invalid in every context; B. invalid only in HTML context.
- **Risk:** none for both. B would accept a few more documents that no real page contains.
- **How common:** not expected on real pages.
- **Recommendation: A** (no change; simplest text). Add to M-10: "This applies in every context, also in a foreign region."
- **Draft fixture:** `EDGE-E7-FOREIGN-FRAMESET` (invalid).

### E8 — Manifest script inside `noscript`

**Decided: A, H-22.** Normative text: `markup.md` M-11 item 4 and M-13 (a `script` start tag in C that M-13 rejects is invalid; error `RQP_MARKUP_RESERVED`). Fixture: `EDGE-E8-NOSCRIPT-MANIFEST`.

Example: `<noscript><script type="application/rivqen-manifest+json">{…}</script></noscript>`.

- **WHATWG:** with scripting enabled, the `script` tag is text in a RAWTEXT element. With scripting disabled, `noscript` content is parsed as markup and the `script` is an element.
- **parse5:** scripting on: no manifest element. Scripting off: one `script` element with the manifest type, also for the `&#43;` variant (`E8-noscript-manifest-charref`), because the tree decodes character references.
- **Today:** all three accept. M-13 says "outside raw text", and M-11 item 3 does not list `script`.
- **Options:**
  - A. Add to M-11: C MUST NOT contain a `script` start tag that breaks M-13 (raw `type` contains `rivqen-manifest` or `&`). Error: `RQP_MARKUP_RESERVED`.
  - B. Add `script` to the list of M-11 item 3 (no `script` start tag in C at all). Error: `RQP_MARKUP_STRUCTURE`.
  - C. No change. Rely on the position of the real manifest and on clients that use scripting.
- **Risk:** block bytes do not differ (M-11 already keeps blocks out of C). The risk is **manifest confusion**: a reader with scripting disabled (a crawler, a server-side HTML tool such as jsoup, which parses `noscript` content as elements, or a native HTML parser in a client SDK) can see two manifests, and one is controlled by the page content. How the client SDK finds the manifest is not specified yet (S2). C leaves the risk to S2. B also rejects the rare valid `<noscript><script>` pattern (tracking fallbacks do not use it, but some widgets do; frequency NOT FOUND).
- **How common:** rare on honest pages; relevant for pages that place untrusted content in `noscript`.
- **Recommendation: A** (closes the gap with the smallest change, same code as M-13). <Badge type="danger" text="GATE" /> Security decision for the human.
- **Draft fixture:** `EDGE-E8-NOSCRIPT-MANIFEST` (invalid, `RQP_MARKUP_RESERVED`).

## 4. Other findings

1. **Rust candidate, M-08 in a foreign region.** The Rust candidate applies M-08 to a `select` element pushed in a foreign region: `<math><select><th></th></select></math>` and `<svg><select><style></style></select></svg>` are invalid in Rust and valid in Java and PHP. M-08 applies to HTML context only. This is a false reject, not a false accept. Found by the sweep (seed 5, `--max-tokens 20`, indexes 11420 and 60097). Follow-up for the Rust lane.
2. **parse5 is not a perfect oracle.** It gives a comment for CDATA in integration points (E5) and the old `<?` behavior (E6). A false accept that parse5 shares with all candidates is not found by the cross-check.
3. **parse5 time on deep nesting.** 50 000 nested elements take about 25 s in parse5 8.0.1. The cross-check is a test tool; this does not affect the candidates.

## 5. Fixtures

<Badge type="info" text="DESIGN" /> The fixtures `FX-RQ-MARKUP-EDGE-*` were drafts that follow the recommendations above. All decisions took the recommended option, so every fixture stays with the same input and the same result. They are normative now. The catalog lists them in [Golden fixtures §4.2](/engineering/protocol/fixtures#_4-2-markup-fixtures-fx-rq-markup).

| Fixture | Result | Rules | Decision |
|---|---|---|---|
| `EDGE-E1-FOREIGN-LT` | valid | M-07 | E1: A, H-21 |
| `EDGE-E2-FOREIGN-EOF` | valid | M-07, M-20 | E2: A, H-21 |
| `EDGE-E3-NOSCRIPT-EOF` | valid | M-11 | E3: A, H-21 |
| `EDGE-E3-NOSCRIPT-EOF-BLOCK` | `STRUCTURE` | M-11 | E3: A, H-21 |
| `EDGE-E4-CDATA-EOF` | valid | M-12 | E4: A, H-21 |
| `EDGE-E5-FOREIGN-CDATA` | valid | M-07, M-12 | E5: A, H-21 |
| `EDGE-E5-FOREIGN-CDATA-GT` | `STRUCTURE` | M-07, M-12 | E5: A, H-21 |
| `EDGE-E6-PI-EOF` | `STRUCTURE` | M-03, M-07 | E6: A, H-21 |
| `EDGE-E6-PI-EOF-HTML` | valid | M-03 | E6: A, H-21 |
| `EDGE-E7-FOREIGN-FRAMESET` | `STRUCTURE` | M-10 | E7: A, H-21 |
| `EDGE-E8-NOSCRIPT-MANIFEST` | `RESERVED` | M-11, M-13 | E8: A, H-22 |

## 6. Security review (H-25 to H-31)

**Status:** <Badge type="warning" text="RESEARCH" /> Study of 2026-10-11, task WP-17/S1/T-12-spec. The human decided H-25 to H-31 before this study ([sprint record](/engineering/plan/sprints/WP-17-S1)). The rule text is in [`markup.md`](/engineering/protocol/markup) §2 (summary in §2.12). This section gives the WHATWG reasoning, the parse5 evidence, the refinements of the depth counter D, and one open question (§6.1).

Sources: the WHATWG HTML Living Standard (`html.spec.whatwg.org`, multipage `parsing.html`, read on 2026-10-11), parse5 8.0.1 through `tools/rqp-markup/crosscheck` (`cases/review-cases.json`, `src/depth.mjs`), and the Rust, Node.js and PHP candidates at `27ae1b5` (they implement H-25, not the rules of this section). The Java candidate was not run: Gradle is not installed in this sandbox.

### 6.1 M-06 item 2: formatting end tags (H-26)

<Badge type="tip" text="FACT" /> WHATWG §13.2.6.4.7 "in body": "An end tag whose tag name is one of: a, b, big, code, em, font, i, nobr, s, small, strike, strong, tt, u — Run the adoption agency algorithm for the token." The algorithm (same section):

1. If the current node has the tag name and is not in the list of active formatting elements, pop it and return.
2. The outer loop runs at most 8 times ("If outerLoopCounter is greater than or equal to 8, then return").
3. formattingElement is the last element with the tag name in the list of active formatting elements after the last marker. If there is none: "any other end tag". If it is not on the stack: remove it from the list and return. If it is not in scope: ignore the token.
4. furthestBlock is "the topmost node in the stack of open elements that is lower in the stack than formattingElement, and is an element in the special category".
5. "If there is no furthestBlock, then the UA must first pop all the nodes from the bottom of the stack of open elements, from the current node up to and including formattingElement, then remove formattingElement from the list of active formatting elements, and finally return."

With a furthest block, the algorithm moves the furthest block to the common ancestor and puts a new formatting element below it. One pass handles one special element. With 8 or more nested special elements, the loop stops and a formatting element stays open in the browser.

<Badge type="info" text="DESIGN" /> Argument for item 2. Let m be the element that the end tag matches on the token stack. Item 2 forbids special and foreign elements between m and the current node of the token stack. When the WHATWG stack above m holds only the elements of the token stack and reconstructed formatting elements (which are not special), there is no furthest block, step 5 applies, and the tree builder pops the same elements as the token stack. Scope (step 3): the scope boundaries `applet`, `caption`, `table`, `td`, `th`, `marquee`, `object` and `template` are in `GUARDED` (M-06 item 1), `html` is in `SPECIAL` (item 2), and the foreign integration points are in a foreign region (M-07, item 2). So the token stack never pops a formatting element over a scope boundary.

<Badge type="tip" text="FACT" /> parse5 8.0.1 (`cases/review-cases.json`, both scripting views):

| Case | parse5 block end | Rust, Node.js, PHP today | Rule |
|---|---|---|---|
| `H26-a-div8`: `<a>`, 8 × `<div>`, `x</a>`, block with `<a>` | none (cut) | valid | M-06 item 2 |
| `H26-nobr-div8`: the same with `nobr` | none (cut) | valid | M-06 item 2 |
| `H26-a-div7`: 7 × `<div>` | same as candidates | valid | M-06 item 2 (one special element is enough) |
| `H26-control-phrasing`: `</a>` over `span`, `</b>` over `i`, `</p>` over `b` | same | valid | valid |

**Formatting start tags.** <Badge type="tip" text="FACT" /> Two start tags also run the adoption agency: `a` "if the list of active formatting elements contains an a element between the end of the list and the last marker", and `nobr` "if the stack of open elements has a nobr element in scope" (§13.2.6.4.7). These runs can have a furthest block. <Badge type="info" text="DESIGN" /> In block content, M-24 items e and f reject these start tags whenever an `a` or `nobr` is open on the token stack. The token stack never closes an `a` or `nobr` by a crossing (both are in `GUARDED`), so outside the cases of the open question below, an `a` or `nobr` in the list of the tree builder is also on the token stack. Outside block content, no block is open: the algorithm moves the furthest block with its whole subtree, so the content bytes of an earlier block do not change. The algorithm does not make the WHATWG stack larger and does not make an element deeper: the furthest block moves below the common ancestor, at most as deep as before; the new formatting element takes the place of the removed one. Evidence: `H26-start-a-aaa`, `H26-start-nobr-aaa`, `H26-start-a-div9-aaa` and the fixture `FORMATTING-START-AAA` agree with parse5. No new rule is needed for the start tag paths.

**Open question Q-T12-1 (gap in H-26).** <Badge type="tip" text="FACT" /> The WHATWG stack can hold special elements that the token stack has already popped. Example: `<a><span>`, 8 × `<div>`, `x</span></a><span data-rq-block="x">y<a>z</a>w</span>`. The `</span>` crosses the `div` elements (allowed: `span` is not in `FORMATTING`). The tree builder ignores it ("any other end tag" stops at the special `div`). Then `</a>` is an exact pop on the token stack and crosses nothing, so item 2 does not apply. The tree builder runs the adoption agency with 8 furthest blocks, stops, and the later `<a>` cuts the block (parse5: no end tag for the block, cases `Q1-span-div8-a`, `Q1-span-div8-nobr`). The same works with `</li>` over `ul` (`Q1-li-ul-a`). All three candidates accept these documents. In each case the formatting element is **dirty** in the sense of M-26 (a crossing happened above it). Options for the human:

| Option | Rule | Cost |
|---|---|---|
| A (recommended) | M-06 item 2 also: an end tag with a name in `FORMATTING` MUST NOT pop a dirty element (M-26). `STRUCTURE`. | Same state as D; one check. Also rejects harmless shapes such as `<a href="#"><b><span><i>x</span></b></a>` (`Q1-control-b-span-i`). |
| B | A second mark, set only by a crossing over an element in `SPECIAL` or a foreign element, passed down like the dirty mark; a formatting end tag MUST NOT pop an element with this mark. | Fewer false rejects; one more flag per element in each candidate. |
| C | No change | Leaves a false accept with the same impact as H-26. |

This is a protocol and security decision. The fixtures do not cover it yet. The cross-check marks the shape as the class `M-06-formatting-dirty`.

### 6.2 M-11 item 5: balanced noscript content (H-27)

<Badge type="tip" text="FACT" /> With scripting disabled, `<noscript>` in body is an ordinary element and `</noscript>` is "any other end tag" (§13.2.6.4.7), which ignores the token when a special element is open above the `noscript`. parse5: `<noscript><button></noscript><div data-rq-block="a">x<button>y</button>z</div>` has no end tag for the block in the scripting-disabled view (case `H27-button`). For `<noscript><table></noscript>` (case `H27-table`) parse5 gives the same block bytes in both views; H-27 rejects it because C is not balanced.

<Badge type="info" text="DESIGN" /> Why item 5 makes `markup.md` §5 item 2 true again: see that item. Additional parse5 checks with balanced C, both views equal: `H27-balanced`, `H27-head-noscript` (the "in head noscript" mode leaves the head at a `div`), `H27-p-closed-by-c` (a `div` in C closes the open `p` in the scripting-disabled view only). The content C is not counted in D (architect decision, `markup.md` M-26 item 7); fixture `LIMIT-DEPTH-NOSCRIPT` has 300 nested elements in C after 255 `div` elements. With scripting disabled such a document is deeper than 512 inside the `noscript`; the flattening of the browser happens only there, and the stack of open elements stays the same, so the later block is not changed.

### 6.3 M-13 item 3: non-ASCII script types (H-28)

<Badge type="tip" text="FACT" /> In JavaScript, `"ı".toUpperCase()` is `"I"` and `"ſ".toUpperCase()` is `"S"` (Unicode case mapping). The cross-check now counts a script type as manifest-like also after this mapping (`src/view.mjs`, `looksLikeManifest`): the cases `H28-dotless-i` and `H28-long-s` give one manifest-like element in both views, and all three candidates accept them today.

### 6.4 M-06 item 3: stray table end tags (H-30)

<Badge type="tip" text="FACT" /> WHATWG §13.2.6.4.13 to §13.2.6.4.15: in "in cell", `</tbody>`, `</tfoot>`, `</thead>`, `</tr>` and `</table>` "close the cell" when an element with that name is in table scope; then "in row" closes the `tr` and "in table body" closes the row group. An implied `tbody` or `tr` is in table scope, but not on the token stack. parse5: `H30-tbody` has no end tag for the block (it is moved before the table and its inner `table` closes it); `H30-tr` moves the block before the table (same bytes).

The other table-family end tags, when they match nothing on the token stack:

| End tag | WHATWG behavior | Effect |
|---|---|---|
| `</td>`, `</th>` | "in cell": ignored unless a `td`/`th` is in table scope; "in row", "in table body", "in table", "in caption": "Parse error. Ignore the token." | None. The tree builder has a `td` or `th` open only when the token stack has it: cells are never implied, and the token stack pops a cell only by its own end tag or by a table-family end tag that also closes the cell in the tree builder (M-06 item 1). |
| `</caption>` | "in caption": ignored unless a `caption` is in table scope; other table modes: ignored | None, for the same reason (captions are never implied). |
| `</col>` | "in column group" and the table modes: "Parse error. Ignore the token." | None. |
| `</colgroup>` | "in column group": pops the current `colgroup` | Closes an implied `colgroup` early. The next token other than `col` closes it anyway ("in column group", "anything else"); only white space and comments directly in the table get another parent. No block can be there (M-19). |

So H-30 needs no more names. Evidence: case `H30-control-td-th-caption-colgroup-col` and fixture `TABLE-IMPLIED-PARTS` agree with parse5.

### 6.5 M-26: depth counter D (H-31)

**Refinements of the H-31 draft** (each one is in the rule text):

| # | Draft (H-31) | Rule text | Why |
|---|---|---|---|
| R1 | −1 when an end tag pops exactly the current node | Only when that element is not **dirty**; a crossing (and a pop of a dirty element, `html` or `body`) makes the new current node dirty | <Badge type="tip" text="FACT" /> Counterexample: 5 × (10 × `<span>`, `<div>`, 10 × `</span>`). The first `</span>` crosses the `div`; the other 9 pop exactly. The tree builder ignores all of them. Draft D = 10, WHATWG stack 57 (parse5). With the dirty mark D = 55. |
| R2 | + the implied elements for each start tag | Each implied part (row group, row, column group) counts once per F element, and comes back when F is popped exactly | The draft adds 2 for every `td` directly in a `table` and gives back 1: a table with 128 cells would reach the limit, and every table would keep units. The tree builder has at most one open row group per table and one open row per row group. |
| R3 | `col` directly in `table`: +1 | The same, although `col` is not pushed | — |
| R4 | `head`: undecided | `head` is not excluded | In body, `</head>` is ignored only when the tree builder has already closed the head ("in head", "anything else" pops it), so the head is not an ancestor of open elements. "after head" pushes the head back only for one token. |
| R5 | — | The tags of the `noscript` content C do not count; the `noscript` element counts | Architect decision: with scripting enabled C is text. |
| R6 | DOM depth ≤ D + 2 | (a) WHATWG stack ≤ D + 2; (b) open elements ≤ D + 2; (c) elements that the tree builder closes at once ≤ D + 3 | A `<br>` at the top level has depth 3 with D = 0. Blink and Gecko flatten by the stack position, so (a) is the bound that matters. |

The false text "1.5 × 256 + 2 = 386" of H-25 is removed: the token stack is not a bound of the WHATWG stack.

<Badge type="info" text="DESIGN" /> **Argument.** Each +1 of D is a unit that belongs to an element of the token stack (its push, or an implied part of its F). A unit comes back only by item 4 of M-26. The claim holds when each element of the WHATWG stack (other than `html`, `head`, `body`), each DOM ancestor of such an element, and each entry of the list of active formatting elements whose element is not on the stack, has its own live unit.

| Tree-builder behavior (WHATWG §13.2.6) | Effect | Why the claim holds |
|---|---|---|
| Insert an element for a start tag that M-05 pushes | +1 element | +1 unit (item 2) |
| Void elements, self-closing `svg`/`math`, `br` for `</br>`, the empty `p` for `</p>`, `input type=hidden` and `form` in a table | inserted and popped at once | Leaves: claim (c) |
| Implied `html`, `head`, `body` | +2 | The 2 of the claim. `head` and `body` are never both ancestors. |
| Implied `tbody`, `tr` ("clear the stack back to a table (body) context", then insert), `colgroup` | at most one of each kind open per F | Implied parts (item 3), given back with F |
| Reconstruct the active formatting elements (§13.2.4.3) | clones of entries whose element is not on the stack | The entry comes from a formatting start tag (a unit). The tree builder popped its element without its own end tag; the token stack crossed it (the unit stays, item 5) or still has it (the unit is live) |
| Implied end tags, "close a p element", "pop until", the adoption agency without a furthest block | fewer elements | No element gets deeper |
| An end tag that the tree builder ignores or uses only in part: "any other end tag" at a special element, `</li>` outside list item scope, a heading end tag that stops at another heading, `</form>` (removes only the form), `</body>`, `</html>` | elements stay open that the token stack popped | The token stack popped them by a crossing (no decrease) or they are `html`/`body`. After that the elements below are dirty, so their later exact pops give nothing back |
| Exact pop of a clean element e | — | No crossing happened above e since its push, so above e the tree builder has only elements that are already closed and reconstructed formatting elements (not special): the end tag of e pops them too |
| The adoption agency with a furthest block | moves elements | The WHATWG stack does not grow, no element gets deeper |
| Foster parenting | the element goes before the table | Not deeper than the table |
| Template contents | a separate fragment; the `template` stays on the stack | Counted through the stack (claim a) |
| `noscript` with scripting enabled | one element with text | +1 (item 7) |

<Badge type="warning" text="RESEARCH" /> This is an argument, not a machine-checked proof. Two paths are argued only in outline: a `</form>` exact pop while reconstructed formatting elements are above the form (the form stays their ancestor), and a formatting end tag whose adoption agency takes another entry of the same name (after the Noah's Ark clause removed the entry of the token stack element). In both paths the unit of the element that left the stack without its own end tag pays the extra level. The cross-check checks the claim after every token.

<Badge type="tip" text="FACT" /> Measurements with parse5 8.0.1 (`src/depth.mjs`):

| Input | max D | WHATWG stack | max DOM depth |
|---|---|---|---|
| 300 × `<span><div></span>`, block (review repro) | 601 | 603 | 603 |
| 100 × `<h1><span><h2>x</h1>`, block | 301 | 203 | 203 |
| 130 × `<li><ul></li>`, block | 261 | 263 | 263 |
| 130 × `<body><div></body>`, block | 261 | 133 | 133 |
| 130 × `<form><div></form>`, block | 261 | 133 | 263 |
| 130 × `<p><b>x</p>y`, block (each `y` reconstructs a `b` inside the last one) | 261 | 133 | 133 |
| 70 × `<span><span><span><div></span></span></span>`, block | 281 | 283 | 283 |
| 64 × `<table><td>`, block | 257 | 259 | 259 |

The invariant check on the fixtures: 0 violations on the 66 valid fixtures (`check --expected`). The sweeps are in §6.6.

**Cost.** <Badge type="info" text="DESIGN" /> D is stricter than the H-25 stack limit. Each crossing keeps its units: `<p><b>x</p>` costs 2 for the rest of the document (the tree builder really nests the reconstructed `b`, see the table), so about 128 such errors make a page invalid. A table that a crossing closes keeps its units (`<tr><td>a</tr>` inside it makes the table dirty). A `select` with options that have no end tag keeps one unit per option until `</select>`, as with H-25. Relaxing D later (for example, no dirty mark after a crossing that the tree builder mirrors) is compatible.

### 6.6 Sweeps

<Badge type="tip" text="FACT" /> Sweeps of 2026-10-11 with the Rust, Node.js and PHP candidates at `27ae1b5`, both scripting views. The candidates do not implement the rules of this section yet, so findings are expected; each finding is counted once per candidate (the three candidates gave the same results). "Without a class" means that no rule class of the cross-check (§6, `src/depth.mjs`) explains the finding.

| Sweep | Documents | Valid (each candidate) | Findings | Without a class | Only the open question | Depth invariant: violations on valid documents |
|---|---|---|---|---|---|---|
| seed 21, `review`, 20 pieces | 50 000 | 15 591 | 6 762 | 0 | 30 (5 documents × 3 candidates × 2 views) | 0 (313 on invalid documents) |
| seed 22, `review`, 40 pieces | 50 000 | 8 216 | 4 290 | 0 | 0 | 0 (522 on invalid documents) |
| seed 23, `review`, 80 pieces | 20 000 | 1 697 | 816 | 0 | 6 (1 document) | 0 (354 on invalid documents) |
| seed 24, `classic`, 20 pieces | 100 000 | 37 318 | 0 | — | — | 0 (222 on invalid documents) |

Findings by kind in seed 21 (per candidate × view): block disagreements 102 (scripting enabled) and 252 (disabled), manifest-like scripts (all `M-13-non-ascii`) 2 634 and 3 735, both kinds in one document 9 and 30. The 5 documents that only the open question explains are all of the form `<a>` or `<nobr>`, a crossing by `span` or `q`, then 8 or more `div` or `ul` elements (seed 21, indexes 6906, 17594, 24445, 35189, 49124). Of the 2 486 documents of seed 21 whose only class is the open question, the Rust candidate accepts 711; option A of §6.1 would reject them all, and 5 of them are real cuts. The depth invariant fails only on documents that the candidates reject (tags in SVG integration points, M-07; table parts after an explicit `colgroup`, M-09). The classic profile gives 0 findings: the change of the manifest check (Unicode look-alikes) adds no finding there.

To repeat a sweep, run in `tools/rqp-markup/crosscheck`: `node src/cli.mjs sweep --only rust,node,php --seed 21 --count 50000 --chunk 10000 --max-tokens 20 --profile review --scripting both`.

## Related

- [RQP markup](/engineering/protocol/markup)
- [Golden fixtures](/engineering/protocol/fixtures)
- [Sprint record WP-17 S1](/engineering/plan/sprints/WP-17-S1)
