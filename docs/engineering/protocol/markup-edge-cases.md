# RQP markup: edge cases E1–E8

This page is a short risk study of eight edge cases of the [RQP markup rules](/engineering/protocol/markup) (rqp/1). For each case it gives the evidence, the options, the risk and one recommended reading. The human decided each case (protocol decision; E8 is also a security decision). The decided rule text is in [`markup.md`](/engineering/protocol/markup) §2; the cases are listed in [§2.11](/engineering/protocol/markup#_2-11-decided-edge-cases).

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

No sweep found a block-byte disagreement that M-24 j or k does not explain. In the scripting-disabled view, no block disagreement appeared that the scripting-enabled view did not also show. This supports `markup.md` §5 item 2 (M-11 makes both views equal).

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

## Related

- [RQP markup](/engineering/protocol/markup)
- [Golden fixtures](/engineering/protocol/fixtures)
- [Sprint record WP-17 S1](/engineering/plan/sprints/WP-17-S1)
