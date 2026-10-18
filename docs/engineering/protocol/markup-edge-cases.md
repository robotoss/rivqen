# RQP markup: edge cases E1–E8

This page is a short risk study of eight edge cases of the [RQP markup rules](/engineering/protocol/markup) (rqp/1). For each case it gives the evidence, the options, the risk and one recommended reading. The human decided each case (protocol decision; E8 is also a security decision). The decided rule text is in [`markup.md`](/engineering/protocol/markup) §2; the cases are listed in [§2.11](/engineering/protocol/markup#_2-11-decided-edge-cases). Section 6 is the study of the findings of the security review (H-25 to H-31, 2026-10-11). Section 7 is round 2 of the review (H-35 to H-38). Section 8 is the WHATWG conformance study (round 3, H-43). Section 9 is the evidence for the `noscript` content model and the relaxations of WP-17 S3 (H-71, H-73 to H-75).

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

**Status:** <Badge type="warning" text="RESEARCH" /> Study of 2026-10-11, task WP-17/S1/T-12-spec. The human decided H-25 to H-31 before this study, and H-32 (option B of the question in §6.1) after it ([sprint record](/engineering/plan/sprints/WP-17-S1)). The rule text is in [`markup.md`](/engineering/protocol/markup) §2 (summary in §2.12). This section gives the WHATWG reasoning, the parse5 evidence, the refinements of the depth counter D, and the question Q-T12-1 with its decision (§6.1).

Sources: the WHATWG HTML Living Standard (`html.spec.whatwg.org`, multipage `parsing.html`, read on 2026-10-11), parse5 8.0.1 through `tools/rqp-markup/crosscheck` (`cases/review-cases.json`, `src/depth.mjs`), and the Rust, Node.js and PHP candidates at `27ae1b5` (they implemented H-25, not the rules of this section; the Java candidate was not run in that study). All four candidates implement the rules since T-13 (§6.6, last paragraph).

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

**Formatting start tags.** <Badge type="tip" text="FACT" /> Two start tags also run the adoption agency: `a` "if the list of active formatting elements contains an a element between the end of the list and the last marker", and `nobr` "if the stack of open elements has a nobr element in scope" (§13.2.6.4.7). These runs can have a furthest block. <Badge type="info" text="DESIGN" /> In block content, M-24 items e and f reject these start tags whenever an `a` or `nobr` is open on the token stack. The token stack never closes an `a` or `nobr` by a crossing (both are in `GUARDED`), so with M-06 item 4 (H-32, below), an `a` or `nobr` in the list of the tree builder is also on the token stack. Outside block content, no block is open: the algorithm moves the furthest block with its whole subtree, so the content bytes of an earlier block do not change. The algorithm does not make the WHATWG stack larger and does not make an element deeper: the furthest block moves below the common ancestor, at most as deep as before; the new formatting element takes the place of the removed one. Evidence: `H26-start-a-aaa`, `H26-start-nobr-aaa`, `H26-start-a-div9-aaa` and the fixture `FORMATTING-START-AAA` agree with parse5. No new rule is needed for the start tag paths.

**Question Q-T12-1 (gap in H-26), decided: option B, H-32.** <Badge type="tip" text="FACT" /> The WHATWG stack can hold special elements that the token stack has already popped. Example: `<a><span>`, 8 × `<div>`, `x</span></a><span data-rq-block="x">y<a>z</a>w</span>`. The `</span>` crosses the `div` elements (allowed: `span` is not in `FORMATTING`). The tree builder ignores it ("any other end tag" stops at the special `div`). Then `</a>` is an exact pop on the token stack and crosses nothing, so item 2 does not apply. The tree builder runs the adoption agency with 8 furthest blocks, stops, and the later `<a>` cuts the block (parse5: no end tag for the block, cases `Q1-span-div8-a`, `Q1-span-div8-nobr`). The same works with `</li>` over `ul` (`Q1-li-ul-a`). All three candidates at `27ae1b5` accept these documents. Options that were given to the human:

| Option | Rule | Cost |
|---|---|---|
| A | An end tag with a name in `FORMATTING` MUST NOT pop a dirty element (M-26) | Same state as D. Also rejects harmless shapes such as `<a href="#"><b><span><i>x</span></b></a>` |
| B (decided, H-32) | A second mark, set only by a crossing over an element in `SPECIAL` or a foreign element, passed down like the dirty mark; a formatting end tag MUST NOT pop an element with this mark | Fewer false rejects; one more flag per element in each candidate |
| C | No change | Leaves a false accept with the same impact as H-26 |

<Badge type="info" text="DESIGN" /> **Rule (H-32):** `markup.md` M-06 item 4, the **special-crossed mark**. After an end tag pops elements, the new current node gets the mark when a crossed element is in `SPECIAL` (pushed in HTML context) or was pushed in a foreign region, or when a popped element has the mark. An end tag with a name in `FORMATTING` MUST NOT pop a marked element. Error: `RQP_MARKUP_STRUCTURE`.

How it covers the three routes (the WHATWG reasoning is in `markup.md` M-06, item 4 note):

| Case | Crossing that sets the mark | Why the tree builder keeps elements open (WHATWG §13.2.6.4.7) | Fixture |
|---|---|---|---|
| `Q1-span-div8-a` | `</span>` crosses 8 `div` (special) → `a` marked | "any other end tag": "if node is in the special category, then this is a parse error; ignore the token" | `FORMATTING-MARKED-SPAN` |
| `Q1-span-div8-nobr` | the same → `nobr` marked | the same | `FORMATTING-MARKED-NOBR` |
| `Q1-li-ul-a` | each `</li>` crosses a `ul` (special); the mark passes down through the next `ul` to `a` | `</li>`: "If the stack of open elements does not have an li element in list item scope, … ignore the token"; `ul` ends list item scope | `FORMATTING-MARKED-LIST` |
| `Q1-control-b-span-i` | `</span>` crosses only `i` (not special): no mark | "any other end tag" passes `i` and pops `i` and `span`: the tree builder keeps nothing open | `FORMATTING-MARKED-PHRASING` (valid) |

The cross-check reports the shape as the class `M-06-formatting-marked`.

### 6.2 M-11 item 5: balanced noscript content (H-27)

<Badge type="tip" text="FACT" /> With scripting disabled, `<noscript>` in body is an ordinary element and `</noscript>` is "any other end tag" (§13.2.6.4.7), which ignores the token when a special element is open above the `noscript`. parse5: `<noscript><button></noscript><div data-rq-block="a">x<button>y</button>z</div>` has no end tag for the block in the scripting-disabled view (case `H27-button`). For `<noscript><table></noscript>` (case `H27-table`) parse5 gives the same block bytes in both views; H-27 rejects it because C is not balanced.

<Badge type="info" text="DESIGN" /> Why item 5 makes `markup.md` §5 item 2 true again: see that item. Additional parse5 checks with balanced C, both views equal: `H27-balanced`, `H27-head-noscript` (the "in head noscript" mode leaves the head at a `div`), `H27-p-closed-by-c` (a `div` in C closes the open `p` in the scripting-disabled view only). The content C was not counted in D at first (architect decision R5); fixture `LIMIT-DEPTH-NOSCRIPT` has 300 nested elements in C after 255 `div` elements. With scripting disabled such a document is deeper than 512 inside the `noscript`. <Badge type="info" text="DESIGN" /> Since H-71 (WP-17 S3), C counts in D in the pass of M-11 item 8, so this fixture is invalid (`RQP_MARKUP_LIMIT`), and D bounds the scripting-disabled tree in C too (§9).

<Badge type="tip" text="FACT" /> Correction (round 2, H-38): item 5 alone did not make both views equal. A balanced C with a table part and no table of its own, in a cell or a caption, closes that cell or caption in the scripting-disabled view (`<td><noscript><td></td></noscript>`). M-11 item 6 closes this gap (§7.4).

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
| R5 | — | Replaced by H-71: the tags of the `noscript` content C count in the pass of M-11 item 8; after the end tag `noscript`, D is as before C. The `noscript` element counts | The earlier architect decision (C does not count, because with scripting enabled C is text) left the scripting-disabled tree in C without a bound (H-42). The human decided H-71 (WP-17 S3, option A1): fixture `LIMIT-DEPTH-NOSCRIPT` is now invalid. |
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
| An end tag that the tree builder ignores or uses only in part: "any other end tag" at a special element, `</li>` outside list item scope, a heading end tag that stops at another heading, `</form>` (ignored when the form element pointer is null or its form is not in scope, else it removes only that form; M-10 item 2 allows one form on the token stack), `</body>`, `</html>` | elements stay open that the token stack popped | The token stack popped them by a crossing (no decrease) or they are `html`/`body`. After that the elements below are dirty, so their later exact pops give nothing back |
| Exact pop of a clean element e | — | No crossing happened above e since its push, so above e the tree builder has only elements that are already closed and reconstructed formatting elements (not special): the end tag of e pops them too |
| The adoption agency with a furthest block | moves elements | The WHATWG stack does not grow, no element gets deeper |
| Foster parenting | the element goes before the table | Not deeper than the table |
| Template contents | a separate fragment; the `template` stays on the stack | Counted through the stack (claim a) |
| `noscript` with scripting enabled | one element with text | +1 (item 7) |
| `noscript` with scripting disabled | the tokens of C are inserted as in the main stream | The pass of M-11 item 8 counts them (item 7, H-71); C closes every element that it opens (M-11 items 5 and 7), so after `</noscript>` the main pass continues with its own D |

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

| Sweep | Documents | Valid (each candidate) | Findings | Without a class | Only Q-T12-1 (now H-32) | Depth invariant: violations on valid documents |
|---|---|---|---|---|---|---|
| seed 21, `review`, 20 pieces | 50 000 | 15 591 | 6 762 | 0 | 30 (5 documents × 3 candidates × 2 views) | 0 (313 on invalid documents) |
| seed 22, `review`, 40 pieces | 50 000 | 8 216 | 4 290 | 0 | 0 | 0 (522 on invalid documents) |
| seed 23, `review`, 80 pieces | 20 000 | 1 697 | 816 | 0 | 6 (1 document) | 0 (354 on invalid documents) |
| seed 24, `classic`, 20 pieces | 100 000 | 37 318 | 0 | — | — | 0 (222 on invalid documents) |

Findings by kind in seed 21 (per candidate × view): block disagreements 102 (scripting enabled) and 252 (disabled), manifest-like scripts (all `M-13-non-ascii`) 2 634 and 3 735, both kinds in one document 9 and 30. The 5 documents that only Q-T12-1 explains are all of the form `<a>` or `<nobr>`, a crossing by `span` or `q`, then 8 or more `div` or `ul` elements (seed 21, indexes 6906, 17594, 24445, 35189, 49124). Of the 2 486 documents of seed 21 whose only class was the dirty-mark class of option A, the Rust candidate accepts 711; option A would reject them all, and 5 of them are real cuts.

<Badge type="tip" text="FACT" /> **Re-run with the H-32 class** (`M-06-formatting-marked`, option B; same candidates, same seeds):

| Sweep | Findings | Without a class | Only H-32 | Documents that a candidate accepts and only H-32 rejects |
|---|---|---|---|---|
| seed 21, `review`, 20 pieces | 6 762 | 0 | 30 (the same 5 documents) | 681 (option A: 711) |
| seed 23, `review`, 80 pieces | 816 | 0 | 6 (1 document) | 73 |

For comparison, the accepted documents that only one other class rejects in seed 21: M-06 item 2 1 060, M-11 item 5 888, M-13 item 3 933, M-06 item 3 670, M-26 229. The generator makes the H-32 shape on purpose (a crossing below a formatting element over special elements), so these numbers do not estimate real pages. The depth invariant fails only on documents that the candidates reject (tags in SVG integration points, M-07; table parts after an explicit `colgroup`, M-09). The classic profile gives 0 findings: the change of the manifest check (Unicode look-alikes) adds no finding there.

<Badge type="tip" text="FACT" /> **After T-13** (all four candidates implement H-25…H-32, wp/WP-17 at `cf88837`; both scripting views): every candidate gives 180/180 on the golden fixtures and 0 parse5 findings on them. Review sweeps per lane: Rust seed 31, Node.js seed 32, Java seed 33, PHP seed 34 (20 000 documents each): 0 findings, no accepted document that only one rule class rejects, 0 depth-invariant violations on valid documents. Joint sweep of the four candidates, seed 41 (`review`, 20 pieces, 20 000 documents): 3 539 valid documents (1 886 with blocks) in each candidate, identical results, 0 findings, 0 depth-invariant violations on valid documents.

To repeat a sweep, run in `tools/rqp-markup/crosscheck`: `node src/cli.mjs sweep --only rust,node,php --seed 21 --count 50000 --chunk 10000 --max-tokens 20 --profile review --scripting both`.

## 7. Security review round 2 (H-35 to H-38)

**Status:** <Badge type="warning" text="RESEARCH" /> Study of 2026-10-11, task WP-17/S1/T-14-spec. The human decided H-35 to H-38 before this study ([sprint record](/engineering/plan/sprints/WP-17-S1)). Rule text: `markup.md` M-09 conditions 1 and 2, M-10 item 2, M-11 item 6, and the M-26 claim. Sources: the WHATWG text as in §6, parse5 8.0.1 through `tools/rqp-markup/crosscheck` (`cases/review2-cases.json`, `src/depth.mjs`), and the four candidates at `65d4b98` (they implement H-25 to H-32, not the rules of this section). The Java candidate was built with a cached Gradle 9.5.1.

| Decision | Repro (case) | parse5 today | Candidates today | Rule |
|---|---|---|---|---|
| H-35 | `<template><col><title>` … fake block (`H35-template-col-title`) | 2 blocks | valid, 1 block | M-09 condition 1 |
| H-35 | `<template><col><textarea>` … manifest script (`H35-template-col-textarea`) | 1 manifest | valid | M-09 condition 1 |
| H-36 | `<a>`, 9 × `<form><object><form></form></object></form>`, block with `<a>` (`H36-form-object-a`) | no end tag for the block; WHATWG stack 13 with D = 4 | valid | M-10 item 2 |
| H-36 | 600 × the same unit, block (`H36-form-object-depth`) | WHATWG stack 603 with D = 3 | valid | M-10 item 2 |
| H-37 | `<object><a><table><marquee><table></table></marquee></table>x</a></object>`, block with `<a>` (`H37-marquee-table`) | no end tag for the block | valid | M-09 condition 2 |
| H-37 | 600 × `<object><b><table><marquee><table></table></marquee></table></b></object>`, block (`H37-foster-depth`) | WHATWG stack 605 with D = 5 | valid | M-09 condition 2 |
| H-38 | `<td><noscript><td></td></noscript>`, block with a `table` (`H38-td`; also `col`, `th`, `caption` variants) | scripting disabled: no end tag for the block | valid | M-11 item 6 |

### 7.1 M-09: col in a template (H-35)

<Badge type="tip" text="FACT" /> "in template" (§13.2.6.4.16) sends `col` to "in column group", and there "Anything else: If the current node is not a colgroup element, then this is a parse error; ignore the token" (§13.2.6.4.12). In a template the current node is the `template`, so the tree builder ignores the later start tags, also the raw-text ones.

**Check of the other paths.** In a template, "in template" sends `caption`, `colgroup`, `tbody`, `tfoot`, `thead` to "in table", `tr` to "in table body", `td` and `th` to "in row", and every other start tag to "in body". In "in table" (§13.2.6.4.9), `style`, `script` and `template` go to "in head", and every other start tag is foster-parented "using the rules for the "in body" insertion mode"; "in table body" and "in row" send other start tags to "in table"; "in caption" uses "in body"; "in column group" with a `colgroup` as the current node pops it and reprocesses the token in "in table". So `title`, `textarea`, `style`, `script`, `xmp`, `iframe`, `noembed`, `noframes`, `noscript` and `plaintext` are inserted (and switch the tokenizer) in all these paths. "in column group" with a `template` as the current node is the only path in a template that ignores them. Outside a template, "in column group" always has a `colgroup` as the current node (except in the fragment case). Two other modes ignore start tags: "in select" (older parsers; M-08 allows only `option`, `optgroup` and `hr` in a `select`) and "in frameset" / "after frameset" (M-10 item 1). Control: `<template><colgroup><title>…</title></colgroup></template>` gives the same result in parse5 and the candidates (`H35-control-template-colgroup-title`).

The M-26 claim now excludes template contents. Inside a template, "in table" inserts an implied `tbody` for `tr` ("Clear the stack back to a table context", then insert a `tbody`), while the M-09 search for F stops at the `template`, so D does not count it (low finding of round 2). Template contents are inert and hold no block (M-19). After the `template` closes, the WHATWG stack holds none of their elements. The cross-check does not check the invariant while a `template` element is open.

### 7.2 M-10 item 2: nested forms (H-36)

<Badge type="tip" text="FACT" /> "in body", `form` start tag: "If the form element pointer is not null, and the parser is not parsing template contents, then this is a parse error; ignore the token." `form` end tag: the pointer is set to null, and the token is ignored when "node is null or … the stack of open elements does not have node in scope"; else only node is removed (§13.2.6.4.7). The earlier FACT text ("`</form>` removes only the form") left out the two ignore cases; `markup.md` M-06 item 4 and §6.5 are corrected.

<Badge type="info" text="DESIGN" /> **Re-check of the H-32 argument with item 2.** With at most one HTML `form` on the token stack:

| Situation | Tree builder | Token stack | Result |
|---|---|---|---|
| The tree builder inserted the form; `</form>` pops it exactly | The pointer names it, and it is in scope (a scope boundary above it needs a `GUARDED` crossing, M-06 item 1) → removed | exact pop | Mirrored |
| The same, but `</form>` crosses elements | Removes only the form; the crossed elements stay open | Crossing: D keeps its units; a crossed special element sets the special-crossed mark | Covered by M-26 and M-06 item 4 |
| An earlier crossing closed the form (`<div><form></div>`), the pointer still names it; then a new `<form>` | Ignored: no element | Pushed (no form on the token stack) | Nothing stays open. The next `</form>` is ignored (node not in scope), and the token stack pops an element that the tree builder never had |
| A `template` is open | The pointer is not used; `</form>` pops "until a form element has been popped" | as usual | Mirrored |

So `</form>` is mirrored when the token stack pops the form exactly, and no `form` can stay open in the tree builder after an exact pop. `H36-control-stale-pointer` and the fixture `FORM-SEQUENTIAL` agree with parse5.

### 7.3 M-09: F is the current node (H-37)

<Badge type="tip" text="FACT" /> "Clear the stack back to a table context": "while the current node is not a table, template, or html element, pop elements from the stack of open elements". "… a table body context" stops at `tbody`, `tfoot`, `thead`, `template`, `html`; "… a table row context" stops at `tr`, `template`, `html` (§13.2.6.4.9, §13.2.6.4.13, §13.2.6.4.14). Start tags that run these steps: `caption`, `colgroup`, `col`, `tbody`, `tfoot`, `thead`, `td`, `th`, `tr` (in the three modes), and `table` ("Pop elements from this stack until a table element has been popped"). Condition 2 covers all of them: F must be the current node, so the steps pop nothing.

**End tags.** The end tags that pop down to a table element are `</table>` ("pop elements from this stack until a table element has been popped"), `</tbody>`, `</tfoot>`, `</thead>` (clear back to a table body context, then pop) and `</tr>` (clear back to a table row context, then pop). On the token stack, such an end tag crosses the foster-parented elements above F:

| Foster-parented element above F | Covered by |
|---|---|
| `applet`, `marquee`, `object`, `template` (the elements that insert a marker) | M-06 item 1: they are in `GUARDED`, and a `TABLE_FAMILY` end tag may cross only `TABLE_PART` elements |
| `button`, `a`, `nobr`, `select`, `svg`, `math` | M-06 item 1 (`GUARDED`) |
| `form` | The pop closes it in both. The form element pointer stays set: §7.2 covers that path |
| Other elements (`div`, `b`, `span`, …) | Popped in both (mirrored). Formatting elements stay in the list of active formatting elements and are reconstructed later (`markup.md` §5 item 1). D keeps their units (crossing). A crossed special element sets the special-crossed mark (M-06 item 4) |

`H37-end-table-over-b` agrees with parse5; `H37-end-table-over-div-a` is rejected by M-06 item 4. No further rule is proposed for end tags.

**Cost.** Condition 2 also rejects shapes that parse5 parses the same as the token stack, for example `<table><tr><object><td>x</td></object></tr></table>` and the old pattern `<table><form><tr>…</tr></form></table>` (`H37-object-td`, `H37-form-in-table`). The corpus measurement (H-39) shows how often real pages have them.

### 7.4 M-11 item 6: table parts in noscript content (H-38)

<Badge type="tip" text="FACT" /> With scripting disabled, a `noscript` element in a cell or a caption does not change the insertion mode ("in cell" or "in caption"). "in cell" (§13.2.6.4.15): "A start tag whose tag name is one of: "caption", "col", "colgroup", "tbody", "td", "tfoot", "th", "thead", "tr"" → "Close the cell (see below) and reprocess the token." "in caption" (§13.2.6.4.11) closes the caption for the same names. So a table part in C closes the cell or the caption around the `noscript`, and the rest is parsed in another cell or row; the block of `H38-td` is cut in the scripting-disabled view. With a `table` opened in C, the table part is parsed in the modes of that table. Both views agree for `H38-control-table-in-c` and the fixture `NOSCRIPT-TABLE-IN-CELL`.

### 7.5 Sweeps

<Badge type="tip" text="FACT" /> Sweeps of 2026-10-11 with the four candidates at `65d4b98` (Rust, Node.js, Java, PHP), profile `review2` (the `review` motifs plus form pointer cycles, template with `col` and raw text, foster-parented elements in tables, and `noscript` in cells and captions with table parts), both scripting views. The candidates do not implement H-35 to H-38 yet; the four gave identical results.

| Sweep | Documents | Valid (each) | Findings (per candidate × view) | Without a class | Depth invariant: accepted documents with a violation |
|---|---|---|---|---|---|
| seed 51, 20 pieces | 30 000 | 5 728 | 3 180 | 0 | 165, all with `M-10-form` |
| seed 52, 60 pieces | 10 000 | 671 | 312 | 0 | 18, all with `M-10-form` |

Documents that a candidate accepts and that only one rule class rejects (seed 51): `M-09-table-current` 443, `M-11-table-part` 349, `M-09-template-col` 141, `M-10-form` 131. Findings by class (seed 51; a finding counts once for each class of its document): block disagreements in the scripting-enabled view 372 (`M-09-template-col` 364, `M-09-table-current` 48, `M-11-table-part` 28, `M-10-form` 20) and in the scripting-disabled view 2 148 (`M-11-table-part` 1 804, `M-09-template-col` 436, `M-09-table-current` 316, `M-10-form` 108). The generator makes these shapes on purpose; the numbers do not estimate real pages (H-39).

To repeat a sweep, run in `tools/rqp-markup/crosscheck`: `node src/cli.mjs sweep --only rust,node,java,php --seed 51 --count 30000 --chunk 10000 --max-tokens 20 --profile review2 --scripting both`.

## 8. WHATWG conformance table (round 3)

**Status:** <Badge type="warning" text="RESEARCH" /> Study of 2026-10-12, task WP-17/S1/T-17-conformance ([sprint record](/engineering/plan/sprints/WP-17-S1)). It found one gap (GAP-1, §8.5). <Badge type="info" text="DESIGN" /> **Closed on 2026-10-12:** the human decided option A (H-43). The rule is `markup.md` M-10 item 3; fixtures `TEMPLATE-FOR`, `TEMPLATE-FOR-EMPTY`, `TEMPLATE-FOR-SVG`; cross-check class `M-10-template-for` (task T-19). The candidate lanes of T-19 implement the rule. The study text below is the state before the decision, except where it says H-43.

Sources:

- The WHATWG HTML Living Standard, read on 2026-10-12 (`html.spec.whatwg.org`, multipage `parsing.html` and `scripting.html`): §13.2.4 "Parse state", §13.2.5 "Tokenization", §13.2.6 "Tree construction", §4.12.3 "The template element".
- parse5 8.0.1 through `tools/rqp-markup/crosscheck`, both scripting views (`cases/conformance-cases.json`, 77 cases).
- Headless Chrome 154.0.8037.99 (`--headless --dump-dom`, scripting enabled) on the same 77 inputs.
- The four candidates (Rust, Node.js, Java, PHP) at `cf56d4d`.

### 8.1 Method

<Badge type="info" text="DESIGN" /> The token model (`markup.md` §2) is correct when, for every document that the rules accept, the tree builder with scripting enabled gives:

1. the same block elements, with the same start tag and end tag (M-14, M-20, M-27);
2. a block subtree that comes only from the content bytes of the block;
3. no script element with a manifest type (M-13);
4. a WHATWG stack of at most D + 2 elements (M-26 claim).

The table lists each step of §13.2.6 that does one of these things. The column "Kind" gives the letters:

| Kind | Meaning |
|---|---|
| a | Ignores a token |
| b | Pops elements, but not by a matching end tag |
| c | Inserts an element that has no start tag |
| d | Moves or removes nodes |
| e | Changes the tokenizer state |

The table also has the tokenizer states that the tree builder controls (§8.2.1) and the template content patching of §4.12.3, because the "in head" rules call it. "Covered" means that a rule of `markup.md` makes the token model agree, or that the step cannot change the four items above (the reason is in the row or in §8.3). Each row that is not obvious has cases in §8.4. A case id in the cases file starts with `C3-`; the tables below leave out this prefix.

### 8.2 Step table

#### 8.2.1 Tokenizer and dispatcher

| ID | Step | Kind | Rule |
|---|---|---|---|
| K-1 | RCDATA, RAWTEXT, script data, PLAINTEXT after a start tag | e | M-04 |
| K-2 | Start tags of K-1 names that the tree builder ignores | a, e | M-09 cond. 1, M-10 item 1, M-04 |
| K-3 | CDATA section when the adjusted current node is not HTML | e | M-12, M-07 item 5; §8.3 item 6 |
| K-4 | Processing instruction states: end at the first `>` | e | M-03 |

#### 8.2.2 Before body (§13.2.6.4.1 to §13.2.6.4.6)

| ID | Step | Kind | Rule |
|---|---|---|---|
| IN-1 | "initial": white space ignored; quirks mode set | a | No element; quirks: SA-11 |
| BH-1 | "before html": DOCTYPE, white space, most end tags ignored | a | Token stack is empty here |
| BH-2 | "before html": implied `html` | c | The 2 of the M-26 claim |
| BD-1 | "before head": DOCTYPE, white space, most end tags ignored | a | Token stack has at most `html` |
| BD-2 | "before head": implied `head` | c | The 2 of the M-26 claim |
| IH-1 | "in head": DOCTYPE, `head`, other end tags ignored | a | M-26 (a 2nd `head` costs D only) |
| IH-2 | "in head": anything else pops `head` | b | No block is `head`; M-26 |
| IH-3 | `</template>`: implied end tags, pop to `template` | b | M-06 item 1, M-19 |
| IH-4 | `meta` charset can change the encoding | e | M-02 (UTF-8 transport) |
| IH-5 | `template` with `for`: content patching | d | M-10 item 3 (H-43; was GAP-1) |
| IH-6 | `template` with `shadowrootmode`: shadow root | d | M-19, M-22 (rendering only) |
| HN-1 | "in head noscript" | a, b | Scripting disabled only: blocks M-11; manifest position H-72 (manifest and patch §1.7) |
| AH-1 | "after head": `head` pushed again for one token | c | M-04, M-20 |
| AH-2 | "after head": `head`, DOCTYPE, end tags ignored | a | As IH-1 |
| AH-3 | "after head": implied `body` | c | The 2 of the M-26 claim |
| AH-4 | `frameset` | a, d | M-10 item 1 |

#### 8.2.3 In body (§13.2.6.4.7)

| ID | Step | Kind | Rule |
|---|---|---|---|
| IB-1 | U+0000 ignored | a | Text only; offsets are input bytes |
| IB-2 | Reconstruct the active formatting elements | c | §5 item 1 of `markup.md`; M-26 |
| IB-3 | `html` start tag: attributes merged | a | M-26 items 4, 5; M-06 item 4 |
| IB-4 | `body` start tag: attributes merged | a | As IB-3 |
| IB-5 | `frameset`: removes `body` | b, d | M-10 item 1 |
| IB-6 | `</body>`, `</html>`: mode change, no pop | a | M-26 item 5, M-06 items 1, 4 |
| IB-7 | `PCLOSE` start tags: close a `p` | b | M-24 a |
| IB-8 | `h1`–`h6` pop a current heading | b | M-24 b |
| IB-9 | `pre`, `listing`: next LF ignored | a | §8.3 item 7 |
| IB-10 | `form` ignored while the pointer is set | a | M-10 item 2 |
| IB-11 | `li`, `dd`, `dt` close earlier items | b | M-24 c, j; §8.3 item 1 |
| IB-12 | `plaintext` | e | M-04, M-22 |
| IB-13 | `button` closes an open `button` | b | M-24 d |
| IB-14 | Block end tags (`div`, `ul`, `select`, …) | b | M-05 step 3; M-06 item 1 |
| IB-15 | `</form>` | a, b | M-10 item 2 |
| IB-16 | `</p>` without `p`: empty `p` | c | M-23; claim (c) |
| IB-17 | `</li>` outside list item scope | a | M-06 item 4, M-26 |
| IB-18 | `</dd>`, `</dt>` | b | As IB-14 |
| IB-19 | Heading end tag pops any heading | a, b | M-23; M-06 item 4, M-26 |
| IB-20 | `a` with an `a` in the list | b, d | M-24 e; §6.1 |
| IB-21 | `nobr` with `nobr` in scope | b, d | M-24 f; §6.1 |
| IB-22 | Formatting end tags: adoption agency | b, d | M-06 items 2, 4 |
| IB-23 | `applet`, `marquee`, `object`: markers | b | M-06 item 1 |
| IB-24 | `table` closes `p` (no-quirks only) | b | M-24 a; SA-11 |
| IB-25 | `</br>`: a `br` element | c | M-22 item 4; claim (c) |
| IB-26 | `input` with `select` in scope pops it | b | M-08 |
| IB-27 | `hr`: close `p`; in `select`, implied ends | b | M-24 a, M-08, M-26 |
| IB-28 | `image` becomes `img` | a | `VOID` has `image` |
| IB-29 | `textarea`: RCDATA, next LF ignored | a, e | M-04, M-22 |
| IB-30 | `xmp`: close `p`, RAWTEXT | b, e | M-04, M-24 a |
| IB-31 | `iframe`, `noembed`, `noscript` | e | M-04 |
| IB-32 | `select` in `select`: ignored, pops | a, b | M-08 |
| IB-33 | `option`, `optgroup`: implied ends | b | M-22, M-08, M-26 |
| IB-34 | `rb`, `rtc`, `rp`, `rt`: implied ends | b | M-24 h, i |
| IB-35 | `math`, `svg` | — | M-05, M-07 |
| IB-36 | Table parts, `frame`, `head` ignored | a | M-09, M-19, M-24 g |
| IB-37 | Any other end tag stops at a special element | a | M-06 item 4, M-26 |
| IB-38 | `title`, `style`, `script`, `template`, … | e | Rules of "in head" (IH rows) |
| IB-39 | End of file: elements stay open | — | M-20 |

#### 8.2.4 Text and table modes (§13.2.6.4.8 to §13.2.6.4.15)

| ID | Step | Kind | Rule |
|---|---|---|---|
| TX-1 | "text": end of file pops the element | b | M-20, M-11 (E3) |
| TB-1 | Characters go to "in table text" | — | TT rows |
| TB-2 | `caption`, `colgroup`, row groups: clear the stack | b | M-09 cond. 2 |
| TB-3 | Implied `colgroup`, `tbody` | c | M-26 implied parts |
| TB-4 | `table` in a table: pop to `table` | b | M-09 cond. 2, M-24 k |
| TB-5 | `</table>` | b | M-06 items 1, 3 |
| TB-6 | `</body>`, `</td>`, `</col>`, … ignored | a | M-06 items 1, 3 (§6.4) |
| TB-7 | Hidden `input`, `form`: inserted and popped | c | Claim (c); M-10 item 2 |
| TB-8 | `form` ignored while the pointer is set | a | M-10 item 2 |
| TB-9 | Anything else: foster parenting | d | M-19, M-24 k, M-09 cond. 2 |
| TT-1 | U+0000 ignored | a | Text only |
| TT-2 | Other text foster-parented | c, d | M-24 k; §5 item 1 |
| CA-1 | `</caption>`: implied ends, pop | b | M-05 step 3; M-06 item 1 |
| CA-2 | Table parts, `</table>` close the caption | b | M-09 table; M-06 item 1 |
| CA-3 | `</body>`, `</td>`, `</tr>`, … ignored | a | M-06 items 1, 3 |
| CG-1 | `</colgroup>` (no `colgroup`), `</col>` ignored | a | §6.4 |
| CG-2 | Anything else pops `colgroup` | b | M-09 table; M-19 |
| CG-3 | Anything else under `template`: ignored | a, e | M-09 cond. 1 (H-35) |
| TBO-1 | `tr`: clear to a table body context | b | M-09 cond. 2 |
| TBO-2 | `td`, `th`: implied `tr` | c | M-26 implied parts; M-09 |
| TBO-3 | `</tbody>`, `</tfoot>`, `</thead>` | b | M-06 items 1, 3 |
| TBO-4 | `caption`, `col`, row groups, `</table>` | b | M-09 table; M-06 item 1 |
| TBO-5 | `</body>`, `</td>`, `</tr>`, … ignored | a | M-06 items 1, 3 |
| TR-1 | `td`, `th`: clear to a table row context | b | M-09 cond. 2 |
| TR-2 | `</tr>` | b | M-06 items 1, 3 |
| TR-3 | Other table parts, `</table>` pop the row | b | M-09 table; M-06 item 1 |
| TR-4 | `</tbody>`, `</tfoot>`, `</thead>` | b | M-06 item 3 (H-30) |
| TR-5 | `</body>`, `</td>`, `</caption>`, … ignored | a | M-06 items 1, 3 |
| TD-1 | `</td>`, `</th>`: implied ends, pop | b | M-05 step 3; M-06 item 1 |
| TD-2 | Table-part start tags close the cell | b | M-09 table |
| TD-3 | `</table>`, `</tbody>`, `</tr>`, … close it | b | M-06 items 1, 3 |
| TD-4 | `</body>`, `</html>`, `</col>`, … ignored | a | M-06 item 1 |

#### 8.2.5 Template, after body, frameset (§13.2.6.4.16 to §13.2.6.4.21)

| ID | Step | Kind | Rule |
|---|---|---|---|
| TM-1 | Table parts switch the template mode | — | M-09 cond. 1, M-19 |
| TM-2 | Any other end tag ignored | a | M-06 item 1 |
| TM-3 | End of file: pop to `template` | b | M-19, M-20 |
| AB-1 | "after body": comment goes to `html` | d | No block effect |
| AB-2 | "after body": other tokens back to "in body" | — | IB-6 |
| AAB-1 | "after after body": comments to the document | d | IB-6 |
| FS-1 | Frameset modes ignore most tokens | a | Not reachable: M-10 item 1 |

#### 8.2.6 Foreign content (§13.2.6.5)

| ID | Step | Kind | Rule |
|---|---|---|---|
| F-1 | U+0000 becomes U+FFFD | — | Text only |
| F-2 | Breakout start tags, `font` with attributes | b | M-07 item 1 |
| F-3 | `</br>`, `</p>` break out | b | M-07 item 2 |
| F-4 | Any other end tag; HTML rules at an HTML node | b | M-07 item 2 |
| F-5 | Self-closing start tags pop | — | M-05 item 2 |
| F-6 | SVG `script` end tag | — | M-07 item 4 |
| F-7 | Integration points use HTML rules | — | M-07 item 3 |
| F-8 | Text in an integration point reconstructs | c | §8.3 item 6 |
| F-9 | `annotation-xml` with `svg`: HTML rules | — | M-07 item 3 |
| F-10 | `mglyph`, `malignmark` stay foreign | — | M-07 item 3 |

#### 8.2.7 Shared algorithms and parser state

| ID | Step | Kind | Rule |
|---|---|---|---|
| SA-1 | Reset the insertion mode appropriately | — | M-06 item 1, M-09, M-19 |
| SA-2 | Reconstruct the active formatting elements | c | IB-2 |
| SA-3 | Adoption agency algorithm | b, d | IB-20 to IB-22 |
| SA-4 | Close a `p` element | b | M-24 a |
| SA-5 | Generate implied end tags (also thoroughly) | b | M-22, M-23, M-24 c, h, i, j |
| SA-6 | Clear the stack back to a table context | b | M-09 cond. 2 |
| SA-7 | Foster parenting | d | M-19, M-24 k, M-09 cond. 2 |
| SA-8 | Head element pointer | c | AH-1 |
| SA-9 | Form element pointer | a | M-10 item 2 |
| SA-10 | Frameset-ok flag | — | M-10 item 1 |
| SA-11 | Quirks: `table` closes `p` only in no-quirks | b | M-24 a |
| SA-12 | Three entries per family in the list | — | M-26 (§6.5) |
| SA-13 | Prepare content patching (§4.12.3) | d | M-10 item 3 (H-43; was GAP-1) |
| SA-14 | Page scripts (`document.write`, custom elements) | d, e | Out of scope: page code |

#### 8.2.8 Count

<Badge type="warning" text="RESEARCH" />

| Result | Rows |
|---|---|
| Steps in the table | 122 |
| Covered by a rule or without effect | 118 before H-43; 120 with H-43 |
| Not covered | 2 before H-43 (IH-5, SA-13: GAP-1); 0 with H-43 |
| Outside the RQP view | 2 (HN-1, SA-14) |

### 8.3 Arguments for the rows that are not obvious

1. <Badge type="info" text="DESIGN" /> **Implied pops (kind b).** These steps close elements that the token stack keeps open. This is the other direction of M-06 item 4: the tree builder has fewer open elements, not more. Such a pop can change a block only while the block is open, so only a start tag in the content can do it. M-22, M-23 and M-24 cover every start tag of "in body" that pops. <Badge type="tip" text="FACT" /> The name sets of `markup.md` §2.1 are equal to the current text: `PCLOSE` is the union of all "in body" start tags that "close a `p` element"; `BREAKOUT` is the breakout list of foreign content plus `font`; `SPECIAL`, `FORMATTING` and `VOID` are as in §13.2.4.2 and §13.2.6.4.7. After the block end tag, the closed elements contain no block bytes, so the block result does not change. D keeps their units (M-26), which is stricter than needed.
2. <Badge type="tip" text="FACT" /> **`</body>` and `</html>` (IB-6, AB-2, AAB-1).** In "in body" these end tags pop nothing; they only change the mode. In "after body" and "after after body", a comment goes to the `html` element or to the document; every other token except `</html>` and the end of file is processed with the "in body" rules. So content after `</body>` or `</html>` is parsed as if the end tags were not there. The token stack pops `body` or `html` by a crossing: D keeps its units (M-26 item 5), and a crossed special element sets the special-crossed mark (M-06 item 4). In "in table", "in caption", "in cell" and "in template" these end tags are ignored; there the token stack crosses a `GUARDED` element, and M-06 item 1 rejects the document (`body-end-in-cell`). An `</html>` that matches nothing is ignored by both (`html-end-in-cell`).
3. <Badge type="tip" text="FACT" /> **`select` (customizable `select`).** The current text has no "in select" and no "in select in table" mode. `select` content goes through "in body", and `select` is in the "has an element in scope" list. `input`, `select`, `hr`, `option` and `optgroup` (IB-26, IB-32, IB-27, IB-33) have new steps for a `select` in scope. M-08 allows only `option`, `optgroup`, `hr` and their end tags in a `select`. For these tokens, the old and the new text give the same block result: parse5 8.0.1 (old text) and Chrome 154 (new text) agree on `select-hr`, `select-optgroup`, `select-in-table` and `select-in-cell`. `hr` pops an open `option` in the tree builder, not on the token stack; D keeps the unit.
4. <Badge type="tip" text="FACT" /> **`table` in a table (TB-4).** "in table": "A start tag whose tag name is "table"": "Pop elements from this stack until a table element has been popped", then reset the insertion mode and reprocess the token. M-09 condition 2 allows `<table><table>`, because F is the current node. The tree builder closes the outer table, and the new table is its sibling; the token stack nests them. A block in the inner table is in a cell (M-19) of a table that both models have open. After the inner `</table>`, the token stack still has the outer table: M-19 rejects a block there unless F is a cell, and a cell start tag in the closed table is ignored by the tree builder, so the block goes to the body with the same tags (`table-in-row`). D counts more than the tree builder.
5. <Badge type="tip" text="FACT" /> **Quirks mode (SA-11).** In tree construction, quirks mode changes one step only: "A start tag whose tag name is "table"" closes an open `p` only "If the Document is not set to quirks mode". `table` is in `PCLOSE`, so M-24 a rejects a `table` in the content of a `p` or `span` block in both modes. In other places the `p` closes earlier or later, but its end tag `</p>` is mirrored or gives an empty `p` (IB-16). A document without DOCTYPE and a document with a quirks public identifier give the same blocks in parse5 and Chrome (`quirks-*`, `noquirks-*`). The fragment parser takes the quirks mode of the document, so a patch gives the same tree.
6. <Badge type="info" text="DESIGN" /> **Foreign content (K-3, F-8).** `svg` and `math` are in `GUARDED` (M-06 item 1), so a crossing never closes them, and M-07 item 1 forbids breakouts. So the foreign-region flag of the token stack is equal to "the adjusted current node is not HTML", which selects the CDATA section state; inside an integration point M-12 and M-07 item 5 make both readings end at the same byte. Characters in an integration point run "reconstruct the active formatting elements" (they use the "in body" rules). This reconstructs nothing: the `svg` or `math` start tag reconstructs first, and no token in the region adds an entry to the list or pops an HTML element (M-07). Evidence: `reconstruct-mi`, `reconstruct-annot`, `reconstruct-desc`.
7. <Badge type="tip" text="FACT" /> **Leading LF (IB-9, IB-29).** After `pre`, `listing` and `textarea`, the tree builder ignores the next token when it is an LF. The tokenizer state does not change. The content bytes keep the LF; the DOM does not. No block element is `pre`, `listing` or `textarea` (M-16), so the block start and end tags do not move. The fragment parser of a patch also drops the LF after `pre` or `listing`. CR LF is one LF after preprocessing; offsets count input bytes (M-03), and the candidates and parse5 agree (`listing-crlf`).
8. <Badge type="tip" text="FACT" /> **U+0000 and CR (IB-1, TT-1, TT-2).** "in body" and "in table text" ignore U+0000; foreign content inserts U+FFFD. These are text changes only. Text in table context is foster-parented, but it stays inside the parent of the table, so inside the block when the table is in the block (`table-text-nul`, `table-text-cr`).
9. <Badge type="tip" text="FACT" /> **`template shadowrootmode` (IH-6).** The template becomes the shadow root of the adjusted current node, and it is not inserted in the DOM. `template` is in `FORBIDDEN_CONTENT`, so a block element is never the host. The block stays in the light DOM of its parent (Chrome: `template-shadowroot`). Whether it is shown depends on slots; that is rendering, not parsing.
10. <Badge type="info" text="DESIGN" /> **Stricter than the tree builder.** M-07 is stricter in three places: `font` without attributes breaks out only in M-07 (`font-plain`); `annotation-xml` without `encoding` is not an integration point, but M-07 item 3 treats it as one (`annot-noenc-mi`); `mglyph` and `malignmark` in a MathML text integration point stay foreign (`mglyph`). These are false rejects, not false accepts.

### 8.4 Cases and results

<Badge type="tip" text="FACT" /> Run in `tools/rqp-markup/crosscheck`: `node src/cli.mjs cases --cases cases/conformance-cases.json --only rust,node,java,php --scripting both`. Each case in the file has the fields `step` (row of §8.2) and `expected` (the result by the rules, derived by hand before the run). All four candidates gave the expected result for all 77 cases (rules before H-43, candidates at `cf56d4d`). With H-43 the expected result of 7 cases is `STRUCTURE` (table below); the candidates give it when they implement M-10 item 3 (T-19). parse5 8.0.1 agreed with every valid result in both scripting views (no finding, no depth-invariant violation). Chrome 154 had the same block elements as the candidates for every valid case except the GAP-1 cases.

| Cases | Rows | Result |
|---|---|---|
| `quirks-p-table`, `noquirks-p-table`, `quirks-div-block-p-table`, `noquirks-div-block-p-table`, `quirks-public-id` | SA-11 | valid |
| `quirks-p-block-table` | SA-11 | `STRUCTURE` (M-24 a) |
| `head-twice`, `title-after-head` | IH-1, AH-1 | valid, 2 blocks |
| `html-in-body`, `body-in-body` | IB-3, IB-4 | valid |
| `head-in-body`, `frame-in-body`, `stray-td`, `stray-caption` | IB-36 | valid |
| `stray-tr-block` | IB-36 | `STRUCTURE` (M-19) |
| `image`, `svg-image` | IB-28 | valid |
| `after-body-block`, `after-body-p-open`, `after-body-ws-comment` | AB-2, AAB-1 | valid |
| `after-body-b-div` | AB-2 | `STRUCTURE` (M-06 item 2) |
| `body-end-in-cell` | IB-6 | `STRUCTURE` (M-06 item 1) |
| `html-end-in-cell`, `html-end-crosses-div` | IB-6 | valid |
| `select-in-table`, `select-in-cell`, `select-hr`, `select-optgroup`, `option-outside` | TB-9, IB-27, IB-33 | valid |
| `select-td-inside`, `select-nested`, `select-input`, `select-keygen` | IB-26, IB-32 | `STRUCTURE` (M-08) |
| `frameset-after-content` | IB-5 | `STRUCTURE` (M-10) |
| `plaintext-after-block` | IB-12 | valid, 1 block |
| `pre-lf`, `listing-crlf`, `pre-lf-before-block`, `textarea-lf`, `xmp` | IB-9, IB-29, IB-30 | valid |
| `table-text-nul`, `table-text-cr`, `body-nul` | TT-1, TT-2, IB-1 | valid |
| `font-color`, `font-plain` | F-2 | `STRUCTURE` (M-07 item 1) |
| `foreign-end-p`, `foreign-end-br` | F-3 | `STRUCTURE` (M-07 item 2) |
| `annot-html-block` | F-7 | `STRUCTURE` (M-07, M-16) |
| `annot-html-text` | F-7 | valid |
| `annot-noenc-mi`, `mglyph` | F-7, F-10 | `STRUCTURE` (stricter) |
| `reconstruct-mi`, `reconstruct-annot`, `reconstruct-desc` | F-8 | valid |
| `ruby-outside`, `ruby-content`, `ruby-p-in-content` | IB-34 | valid |
| `ruby-outer-p-block` | IB-34 | `STRUCTURE` (M-24 i) |
| `li-outside`, `dl-outside`, `li-after-block` | IB-11 | valid |
| `table-in-table`, `table-in-row` | TB-4 | valid |
| `form-stale-pointer` | TB-7 | valid |
| `heading-end-other` | IB-19 | valid |
| `eof-title`, `eof-script` | TX-1 | valid |
| `template-shadowroot` | IH-6 | valid |
| `GAP1-replace-content`, `GAP1-remove-block`, `GAP1-title-in-head`, `GAP1-start-no-end`, `GAP1-ancestor-scope` | IH-5 | Before H-43: valid (**GAP-1**). With H-43: `STRUCTURE` (M-10 item 3) |
| `GAP1-control-table-scope`, `GAP1-control-empty` | IH-5 | Before H-43: valid (no patching). With H-43: `STRUCTURE` (fail closed) |
| `GAP1-control-foreign`, `xml-pi` | IH-5 | valid (no patching) |

To repeat the Chrome check, write each input to a file and run `"/Applications/Google Chrome.app/Contents/MacOS/Google Chrome" --headless --dump-dom file:///…/case.html`. The tool does not run Chrome.

### 8.5 GAP-1: template content patching

<Badge type="tip" text="FACT" /> The current WHATWG text has **content patching**. §4.12.3: the `template` element has the content attribute `for` ("Updates existing content"). §13.2.6.4.4, "in head", "A start tag whose tag name is "template"": when `shadowrootmode` is not set and "templateStartTag's for attribute is specified", the tree builder runs "prepare content patching". The steps are:

1. The scope is "the node that contains the adjusted insertion location"; when that is the `body`, the scope is its parent, so a `template` in the body can patch the `head`.
2. "find markers" walks the descendants of the scope in tree order. It takes the first processing instruction `<?marker name="…">`, or `<?start name="…">` with its matching `<?end>` among the next siblings. A `<?start>` without `<?end>` is "a range from start to the end of its parent".
3. The nodes between `<?start>` and `<?end>` are removed.
4. The content of the template is not inert: "the appropriate place for inserting a node" sends it to the parent of the marker, before the end marker. `</template>` then removes the markers.

The `template` start tag reaches this path from "in head" and from every mode that sends it there: "after head", "in body", "in table", "in column group", "in template", and the modes that use their rules ("in caption", "in cell", "in table body", "in row"). It does not apply to an SVG or MathML `template`, to an empty `for` value, or when no marker is found in the scope. The tokenizer reads `<?start …>` as a processing instruction; RQP reads it as a comment (M-03), and M-22 allows comments in block content.

<Badge type="tip" text="FACT" /> Evidence. The four candidates say valid, 1 block, for each input below. parse5 8.0.1 has no content patching and no processing instructions, so its tree has the block as in the bytes (no finding: the cross-check cannot see this gap). Chrome 154 (`--dump-dom`, scripting enabled) gives this DOM. Each input starts with `<!DOCTYPE html>`; the full inputs are in the cases file.

- `GAP1-replace-content`: `<div data-rq-block="a"><?start name="p">old<?end></div>` and then `<template for="p"><b>new</b></template>`. The block content is `<b>new</b>`, not `old`.
- `GAP1-remove-block`: `<body><?start name="q"><div data-rq-block="a">x</div><?end>` and then `<template for="q">gone</template>`. The block element is not in the DOM; the body has the text `gone`.
- `GAP1-title-in-head`: `<head><?start name="t"><title data-rq-block="t">T</title><?end></head>` and then, in the body, `<template for="t"><title>X</title></template>`. The block element is not in the DOM; the head has `<title>X</title>`.
- `GAP1-start-no-end`: `<p data-rq-block="a">v<?start name="m">w</p>` and then `<template for="m">Z</template>`. The block content is `vZ`, not `v` followed by the marker and `w`.
- `GAP1-ancestor-scope`: `<div><div data-rq-block="a"><?start name="m">x<?end></div>` and then `<template for="m">y</template></div>`. The block content is `y`.

Impact: the block element is not in the DOM, or its subtree does not come from its content bytes (items 1 and 2 of §8.1). This is the same class as H-26 and H-30 (a block that the client cannot find or that is not what the server hashed). The patched content cannot add a manifest script or a block: M-13 applies in every context, and M-19 rejects a block start tag while a `template` is on the token stack. <Badge type="warning" text="RESEARCH" /> The depth of patched content is not measured: it is inserted under the marker, and the M-26 claim does not check while a `template` is open. Use of `template for` in the T-15 corpus: NOT MEASURED (the corpus statistics do not count it).

**Question for the human (protocol and security decision).** Decided on 2026-10-12: option A (H-43). The options were:

| Option | Rule | Cost |
|---|---|---|
| A (recommended) | A `template` start tag in HTML context MUST NOT have an attribute named `for` (ASCII case-insensitive, as the tokenizer lowercases names). Error: `RQP_MARKUP_STRUCTURE` | One attribute check on a tag that every candidate already finds (M-19). Fail closed: also rejects an empty `for` and a `for` that matches no marker. Pages that use content patching fall back to normal HTML |
| B | A comment that starts with `<?` and whose target is `marker`, `start` or `end` MUST NOT appear | Each candidate must read the processing instruction target (case-sensitive, ends at white space, `?` or `>`); older parsers read a bogus comment, so the rule must not depend on the tokenizer of the client. It leaves `template for` valid, so a later change of the marker syntax in the WHATWG text can open the gap again |
| C | A and B | The cost of both; no extra safety over A for the parser view |

Recommendation: **A**. <Badge type="info" text="DESIGN" /> **Done (T-19):** `markup.md` M-10 item 3 with rows in §2.12, §3.3 and §4; fixtures `TEMPLATE-FOR` (invalid, the first Chrome repro), `TEMPLATE-FOR-EMPTY` (invalid: `<template For>` without a value) and `TEMPLATE-FOR-SVG` (valid control); the class `M-10-template-for` in `src/depth.mjs`; the sweep profile `review3`. M-11 item 3 already forbids `template` in the `noscript` content C. <Badge type="tip" text="FACT" /> A `review3` sweep (seed 81, 5 000 documents, Rust and Node.js candidates before H-43, both views) gave 0 parse5 findings and 39 accepted documents that only `M-10-template-for` rejects: parse5 cannot see the class, as expected.

### 8.6 Follow-ups

1. <Badge type="info" text="DESIGN" /> parse5 8.0.1 implements an older text: no processing instructions, no content patching, the old `select` modes. A cross-check with parse5 alone does not find gaps from these changes. Proposal for the architect: an optional Chrome oracle in `tools/rqp-markup/crosscheck` for the `cases` command.
2. Done (T-19): `markup.md` §3.2 (the list of WHATWG changes since 2025) names content patching.
3. Count `template for`, `<?marker` and `<?start` in the T-15 corpus.

## 9. noscript content model and relaxations (WP-17 S3)

<Badge type="info" text="DESIGN" /> **Decided on 2026-10-18** ([sprint record WP-17 S3](/engineering/plan/sprints/WP-17-S3) §1): H-71 (option A1: the main-pass rules on the `noscript` content C, D counted in C), H-73 (option A: `svg` and `math` in `html` block content with `FOREIGN_ALLOWED`), H-74 (option A: `svg` in C), H-75 (option A: no mark for the crossing by the end tag of the region root). Studies: `evidence/markup-noscript/report.md` (T-02) and `evidence/markup-relax/report.md` (T-01). Rule text: `markup.md` M-05 item 4, M-06 item 4, M-07 item 6, M-11 items 3 and 5 to 9, M-22 item 5 and `FOREIGN_ALLOWED`, M-23, M-24, M-26 item 7, [§2.13](/engineering/protocol/markup#_2-13-rules-from-the-wp-17-s3-studies). Task T-03 wrote the rule text, the golden fixtures, the cross-check cases `cases/s3-cases.json` and the changes of the cross-check oracle and the corpus classifier.

### 9.1 How the four decisions fit together

<Badge type="info" text="DESIGN" />

| Pair | Where they meet | Rule text |
|---|---|---|
| H-71 and H-74 | The foreign region of C (item 9) is also a foreign region in the pass of item 8: the `noscript` element is in HTML context, so the `svg` start tag in C pushes a region root by M-05 item 4. | Item 9 gives only the tokenization (no state change) and the stack of item 5 (M-05 item 2). M-07 and M-12 in the region come from item 8. Items 2 to 4, 6 and 7 also apply in the region. |
| H-71 and H-75 | The exception of item 5 in C. In the pass of item 8 the crossing sets no mark (M-06 item 4), but it makes the new current node dirty (M-26 item 5), so D of that pass stays larger. | After the end tag `noscript`, the main pass continues with its own state (item 8). The pass of item 8 can only reject more. |
| H-73 and H-75 | The exception of M-23 in block content. The region root starts in the content (M-16, M-07 item 2), so the crossing never reaches an element outside the block. | M-23 uses the same condition as M-06 item 4: the matching element is the region root. The block element can become dirty; that changes only D. |
| H-73 and M-24 | A start tag in a foreign region closes no element. | The items of M-24 apply in HTML context only. |
| H-73 and H-71, H-74 | C is never in block content (`noscript` is in `FORBIDDEN_CONTENT` and not in `FOREIGN_ALLOWED`), and with scripting enabled C is never parsed as markup. | `FOREIGN_ALLOWED` applies only to block content, not to the foreign region of C. |
| H-71 and H-38 | For C in a cell or a caption, item 8 (M-09) and item 6 both reject a table part without a table of C. | Both give `RQP_MARKUP_STRUCTURE`. Item 6 stays for C outside a table (`<noscript><td></td></noscript>`), where M-09 has no F. |

Choices of T-03 that the studies left open (each one is stricter than the other reading, so a later relaxation stays compatible):

1. Item 6 also applies in the foreign region of C (the proposal of T-01). The model (a) variant of T-02 did not apply it there. Fixture `NOSCRIPT-SVG-TABLE-PART`.
2. Item 7 also applies in the foreign region of C (as in the T-02 study code).
3. `math` stays in item 3 (H-74 names `svg` only), so only `svg` starts a foreign region of C. Fixture `NOSCRIPT-MATH`.
4. A self-closing `<svg/>` or `<math/>` is allowed in block content and opens no region (M-22 item 3, the proposal of T-01). Fixture `CONTENT-SVG-SELF-CLOSING-ROOT`.

### 9.2 Fixtures

<Badge type="tip" text="FACT" /> 2026-10-18: 247 golden fixtures (86 valid, 161 invalid). Two existing results change by decision:

| Fixture | Before | After | Decision |
|---|---|---|---|
| `LIMIT-DEPTH-NOSCRIPT` | valid, 1 block | `LIMIT` | H-71 (replaces R5) |
| `CONTENT-SVG` | `STRUCTURE` | valid, 1 block | H-73 |

New fixtures (50):

| Decision | Valid | Invalid |
|---|---|---|
| H-71 | `NOSCRIPT-MAIN-VALID`, `LIMIT-DEPTH-NOSCRIPT-CONTENT-256` | `NOSCRIPT-MAIN-TABLE-CURRENT` (T-16), `NOSCRIPT-MAIN-TABLE-IN-TABLE`, `NOSCRIPT-MAIN-FORM-OBJECT` (H-36 in C), `NOSCRIPT-MAIN-FORM-OUTSIDE`, `NOSCRIPT-MAIN-CDATA`, `NOSCRIPT-DOCUMENT-TAG-BODY`, `-HTML`, `-HEAD`, `LIMIT-DEPTH-NOSCRIPT-CONTENT-257` |
| H-73 | `CONTENT-SVG-ICON`, `CONTENT-MATH`, `CONTENT-SVG-SELF-CLOSING-ROOT`, `LIMIT-DEPTH-CONTENT-FOREIGN-256` | `CONTENT-FOREIGN-A`, `-ANIMATE`, `-SET`, `-ANIMATEMOTION`, `-ANIMATETRANSFORM`, `-FOREIGNOBJECT`, `-IMAGE`, `-SCRIPT`, `-STYLE`, `-FEIMAGE`, `-MACTION`, `-ANNOTATION-XML`, `-INTEGRATION`, `-INTEGRATION-MATH`, `-CROSSING`, `-EOF`, `LIMIT-DEPTH-CONTENT-FOREIGN-257` |
| H-74 | `NOSCRIPT-SVG`, `NOSCRIPT-SVG-HEAD` | `NOSCRIPT-SVG-CROSSING`, `-UNCLOSED`, `-BREAKOUT`, `-TITLE-MARKUP`, `-TABLE-PART`, `NOSCRIPT-MATH` |
| H-75 | `ICON-ROOT-CROSSING`, `CONTENT-SVG-ICON-CROSSING`, `NOSCRIPT-SVG-ICON`, `LIMIT-DEPTH-ICON-85` | `ICON-INNER-CROSSING`, `ICON-NESTED-SVG-CROSSING`, `LIMIT-DEPTH-ICON` |
| H-71 with H-75 | `LIMIT-DEPTH-NOSCRIPT-ICON-RESTORE` (after `</noscript>` the main pass has D as before C), `LIMIT-DEPTH-NOSCRIPT-ICONS-127` | `LIMIT-DEPTH-NOSCRIPT-ICONS` (each icon in C keeps 2 units in the pass of item 8) |

Each new invalid fixture breaks one rule item (corpus classifier, `classify.mjs`). The fixtures `NOSCRIPT-TABLE-PART`, `-CAPTION`, `-COL` and `-TH` now break M-11 item 6 and item 8 (M-09); the result is the same (`STRUCTURE`).

### 9.3 Cross-check

<Badge type="tip" text="FACT" /> parse5 8.0.1, 2026-10-18, `tools/rqp-markup/crosscheck`:

| Input | Result |
|---|---|
| Golden fixtures, `check --expected --scripting both` | 247 documents, 86 valid; 0 findings (blocks in both views, depth claim in both views, rule classes) |
| `cases/s3-cases.json` (78 cases) and the earlier cases files (162 cases) | Each `expected` field holds for a reference of the decided rules; 0 disagreements in both views and 0 depth-claim violations on the valid cases. The depth claim fails in the scripting-disabled view only on rejected shapes (T-16, H-36 in C, an unclosed `svg` in C) |
| Sweeps, profiles `classic`, `review`, `review2`, `review3`, seed 71, 100 000 documents each, both views | 81 848 valid documents; 0 findings; 0 depth-claim violations on valid documents in either view (violations on rejected documents: 208 / 4 627, 659 / 8 921, 5 353 / 12 090, 5 130 / 11 721 with scripting enabled / disabled) |
| The generators of the studies: `relax` (seed 40, 300 000), `body` (seed 42, 1 000 000), `head` (seed 42, 300 000), both views | 578 734 valid documents; 0 findings |
| Control: the `body` generator with the rules before S3 | 24 block disagreements with scripting disabled (the H-37 class of the T-02 study) and 6 114 depth-claim violations with scripting disabled on documents that those rules accept |
| T-01 tree, fragment and `noscript` tree checks on the valid fixtures | Only `noscript-*-head` findings for `NOSCRIPT-SVG-HEAD`: "in head noscript" ends the head at the `svg` start tag. The blocks agree; the manifest position is H-72 |

The reference of the decided rules is the T-01 study copy (`study/relax/src/markup-relax.mjs`, flags `r1a`, `r2`, `r3`) with the model (a) pass of the T-02 study copy added. It was used only to check the rule text and the fixtures; it is not committed. <Badge type="info" text="DESIGN" /> At integration, run the cases and a sweep again with the four candidates of T-04…T-07: `node src/cli.mjs cases --cases cases/s3-cases.json --only rust,node,java,php` and `node src/cli.mjs sweep --only rust,node,java,php --seed 71 --count 100000 --chunk 10000 --max-tokens 20 --profile review2 --scripting both`.

<Badge type="tip" text="FACT" /> Corpus (882 pages of the S3 fetch, 2026-10-18), the same reference: 820 pages valid (798 with the rules before S3); H-71 alone rejects none of the 798 pages. The corpus run with the four candidates follows at the sprint close.

## Related

- [RQP markup](/engineering/protocol/markup)
- [Golden fixtures](/engineering/protocol/fixtures)
- [Sprint record WP-17 S1](/engineering/plan/sprints/WP-17-S1)
- [Sprint record WP-17 S3](/engineering/plan/sprints/WP-17-S3)
