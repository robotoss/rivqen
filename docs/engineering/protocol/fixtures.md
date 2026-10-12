# Golden fixtures

A golden fixture is a set of files that defines one exact behavior: the input, the state, and the expected output. Every Rivqen implementation (Rust core, servers in Node.js, Java, PHP, the reference server) runs the same fixtures.

**Status:** <Badge type="info" text="DESIGN" /> Fixtures are created in WP-02. The catalog below is the minimum set.

[[toc]]

## 1. Fixture layout

```text
fixtures/legacy/FX-LEG-304/
├── fixture.json          # metadata: id, profile, upstream SHA, sources, description
├── cache-before/         # client cache state (template, data, meta) or empty
├── request.http          # raw HTTP request the client sends
├── origin.html           # what the origin renders for this request
├── response.http         # expected raw HTTP response from the server SDK
├── expected.template     # expected template after split (if any)
├── expected.data.json    # expected data blocks (if any)
├── expected.render.html  # expected final document shown to the user
└── expected.actions.json # expected core actions, in order
```

`fixture.json` example:

```json
{
  "id": "FX-LEG-304",
  "profile": "legacy-canonical",
  "upstream": { "repo": "Tencent/VasSonic", "sha": "59936beff656d4b5718ff6444d6c5e001a2c5231" },
  "sources": ["upstream:<path>:<line>"],
  "description": "Cached page, server confirms no change"
}
```

## 2. Rules

1. Files use UTF-8 and LF line ends, except where the fixture tests CRLF or encodings.
2. A fixture changes only through a `spec` commit with protocol-owner review.
3. Never change a fixture to make code pass. Fix the code, or open an ADR if the fixture is wrong.
4. Each fixture names its profile. Divergent upstream behaviors get separate fixtures per profile.
5. Fixtures never contain real user data, real tokens or real domains (use `example.com`).

## 3. legacy protocol catalog (minimum)

| ID | Scenario |
|---|---|
| `FX-LEG-FIRST-LOAD` | No cache; full HTML; template and data stored |
| `FX-LEG-304` | Cache present; `etag` matches; `304` |
| `FX-LEG-DATA-CHANGE` | Same template, data changed; data-only response |
| `FX-LEG-TEMPLATE-CHANGE` | Template changed; full HTML; `template-change: true` |
| `FX-LEG-OFFLINE-TRUE` | `cache-offline: true` |
| `FX-LEG-OFFLINE-FALSE` | `cache-offline: false` |
| `FX-LEG-OFFLINE-STORE` | `cache-offline: store` |
| `FX-LEG-OFFLINE-HTTP` | `cache-offline: http` (disable acceleration for a period) |
| `FX-LEG-MALFORMED-MARKERS` | Unclosed, mismatched or broken markers |
| `FX-LEG-DUPLICATE-BLOCKS` | Same block name twice |
| `FX-LEG-TITLE` | Special handling of the `<title>` element (see [markers grammar](/engineering/protocol/legacy-markers)) |
| `FX-LEG-UNICODE` | Non-ASCII content and block names |
| `FX-LEG-COMPRESSED` | gzip / br response |
| `FX-LEG-INTERRUPTED` | Stream cut in the middle |
| `FX-LEG-CONCURRENT` | Two sessions for the same URL |
| `FX-LEG-AUTH-CHANGE` | Partition change during a session |
| `FX-LEG-REDIRECT` | 301/302 on the main document |
| `FX-LEG-COOKIE-VARIANCE` | Different cookies, same URL |
| `FX-LEG-NON-SONIC-CLIENT` | Browser without legacy headers gets unchanged HTML |

The exact expected outputs are derived from the upstream audit and confirmed with captured traces (WP-01).

## 4. Rivqen protocol catalog

### 4.1 Protocol fixtures (initial)

<Badge type="info" text="DESIGN" /> Request and response fixtures for RQP. Sprint WP-17 S2 revises this list for the human decisions H-7 (client block digests) and H-9 (no `page_id` and `sequence` in HTTP patches).

| ID | Scenario |
|---|---|
| `FX-RQ-NEGOTIATE` | Client and server agree on RQP |
| `FX-RQ-UNSUPPORTED` | Server ignores RQP headers |
| `FX-RQ-PATCH-OK` | Valid patch applied |
| `FX-RQ-PATCH-BASE-MISMATCH` | Stale base revision |
| `FX-RQ-PATCH-TEMPLATE-MISMATCH` | Template revision changed |
| `FX-RQ-PATCH-SEQUENCE-GAP` | Missing sequence number |
| `FX-RQ-PATCH-SCRIPT` | Patch with a `<script>` tag is rejected |
| `FX-RQ-PATCH-HASH` | Wrong block hash |
| `FX-RQ-DOWNGRADE-DENIED` | Downgrade not allowed by policy |

### 4.2 Markup fixtures (`FX-RQ-MARKUP-*`)

<Badge type="tip" text="FACT" /> 197 fixtures (72 valid, 125 invalid) in `fixtures/rqp/markup/`. All are normative. The 18 fixtures `EDGE-*` cover the edge cases E1–E8, their combinations and the case of the CDATA opener ([markup §2.11](/engineering/protocol/markup#_2-11-decided-edge-cases)), decided by the human (H-21, H-22). The fixtures `FORMATTING-*`, `NOSCRIPT-BALANCED`, `NOSCRIPT-CROSSING`, `NOSCRIPT-EOF-UNBALANCED`, `NOSCRIPT-STRAY-END`, `NOSCRIPT-UNBALANCED*`, `*-NON-ASCII*`, `TABLE-STRAY-*`, `TABLE-IMPLIED-PARTS` and `LIMIT-DEPTH-*` cover the rules of the security review (H-25 to H-38, [markup §2.12](/engineering/protocol/markup#_2-12-rules-from-the-security-review)). The fixtures `TEMPLATE-FOR*` cover H-43 from the conformance study ([edge cases §8](/engineering/protocol/markup-edge-cases#_8-whatwg-conformance-table-round-3)). Each fixture tests the rules of [RQP markup](/engineering/protocol/markup) §2 that it lists. Layout and `fixture.json` format: `fixtures/rqp/markup/README.md`. Result format: `tools/rqp-markup/CONTRACT.md` §3.

| File | Written by |
|---|---|
| `input.html` | A person (exact bytes); generated by a recipe in `tools/rqp-markup/gen.py` for `LIMIT-256`, `LIMIT-257`, `LIMIT-BLOCK-SIZE`, `LIMIT-INPUT-SIZE` and the depth fixtures `LIMIT-DEPTH-DIRTY*`, `LIMIT-DEPTH-IGNORED-END*`, `LIMIT-DEPTH-MIXED`, `LIMIT-DEPTH-NOSCRIPT*`, `LIMIT-DEPTH-TABLE*`, `FORM-NESTED-DEPTH`, `TABLE-FOSTER-MARKER-DEPTH` |
| `fixture.json` | A person; generated by the recipe for `LIMIT-256` (256 block entries) |
| `expected.json` | `tools/rqp-markup/gen.py` only |

An invalid fixture has exactly one error code. Some invalid fixtures break more than one rule with the same code; the column "Rules" lists them.

To add a fixture:

1. Make the directory `fixtures/rqp/markup/FX-RQ-MARKUP-<NAME>/`.
2. Write `input.html` with exact bytes. Do not let the editor add a final line feed or change line ends.
3. Write `fixture.json` with the expected blocks or the expected error.
4. Run `python3 tools/rqp-markup/gen.py`.
5. Run `python3 tools/rqp-markup/gen.py --check`.
6. Add a row to the table below.

In the table, the prefix `FX-RQ-MARKUP-` is not shown.

| Fixture | Result | Rules | Scenario |
|---|---|---|---|
| `AFTER-HTML-END` | valid, 1 block | M-19, M-27 | A block after &lt;/html> is a block |
| `ATTR-DUPLICATE` | valid, 2 blocks | M-03, M-14 | Duplicate data-rq-block attributes on one tag: the first one wins |
| `ATTR-GT` | valid, 1 block | M-03, M-14 | The character > inside quoted attribute values does not end the tag |
| `ATTR-QUOTES` | valid, 4 blocks | M-03, M-14, M-15 | Double-quoted, single-quoted and unquoted attribute values |
| `BAD-JSON` | `BAD_JSON` | M-25 | A trailing comma is not valid JSON |
| `BAD-JSON-DEPTH-65` | `BAD_JSON` | M-25 | Nesting depth 65 is more than the maximum (64) |
| `BAD-JSON-EMPTY` | `BAD_JSON` | M-25 | Empty content is not a JSON text |
| `BAD-JSON-SURROGATE` | `BAD_JSON` | M-25 | A lone surrogate escape is invalid |
| `BAD-JSON-WHITESPACE` | `BAD_JSON` | M-25 | A no-break space is not JSON white space |
| `BOGUS-COMMENT` | valid, 1 block | M-03, M-14 | Bogus comments (&lt;!x and &lt;/ followed by a space) end at the first > |
| `BOM` | valid, 1 block | M-02, M-27 | A UTF-8 byte order mark at offset 0 counts in the offsets and the template |
| `CDATA-HTML` | `STRUCTURE` | M-12 | A CDATA section with > inside, in HTML content |
| `CDATA-SVG` | valid, 1 block | M-07, M-12 | A CDATA section in svg without > inside |
| `CHARREF` | valid, 1 block | M-14, M-27 | Character references stay as raw bytes; escaped markup is not a block |
| `COL-IN-CELL` | `STRUCTURE` | M-09 | A col start tag inside a td |
| `COMMENT-FORMS` | valid, 2 blocks | M-03, M-14 | Tag-like text in a comment is not a block; &lt;!--> and --!> end comments early |
| `CONTENT-DOCTYPE` | `STRUCTURE` | M-22 | A DOCTYPE token in the content |
| `CONTENT-OPTION` | `STRUCTURE` | M-22 | An option element in the content |
| `CONTENT-SCRIPT` | `STRUCTURE` | M-22 | A script element in the content |
| `CONTENT-SELF-CLOSING` | `STRUCTURE` | M-22 | A self-closing flag on a non-void element in the content |
| `CONTENT-SVG` | `STRUCTURE` | M-22 | An svg element in the content |
| `CONTENT-VOID-END` | `STRUCTURE` | M-22 | An end tag of a void element (&lt;/br>) in the content |
| `CONTEXT-TABLE` | `STRUCTURE` | M-19 | A block directly in a table (it would be foster-parented) |
| `CONTEXT-TEMPLATE` | `STRUCTURE` | M-19 | A block inside a template element |
| `CRLF` | valid, 1 block | M-03, M-27 | CR LF line ends; offsets count both bytes and the content keeps CR |
| `CROSSING` | `STRUCTURE` | M-06 | An end tag crosses an open button |
| `DIV-AFTER-OPEN-P` | valid, 1 block | M-19, M-24 | A div block after an open p: the p closes before the div |
| `DUPLICATE` | `DUPLICATE` | M-21 | Two blocks with the same id |
| `EDGE-CDATA-LOWERCASE-FOREIGN` | `STRUCTURE` | M-04, M-07, M-12 | &lt;![cdata[ in svg desc is a bogus comment, not a CDATA section |
| `EDGE-CDATA-LOWERCASE-HTML` | valid, 1 block | M-04, M-12 | &lt;![cdata[a>b]]> in HTML context is a bogus comment; M-12 does not apply |
| `EDGE-COMBO-DESC-CDATA-EOF` | valid, 1 block | M-07, M-12, M-20 | E2 + E4 + E5: the input ends inside a CDATA section in svg desc |
| `EDGE-COMBO-DESC-EMPTY-END-TAG` | valid, 1 block | M-07 | E1: &lt;/> in svg desc gives no token |
| `EDGE-COMBO-NOSCRIPT-MANIFEST-EOF` | `RESERVED` | M-11, M-13 | E3 + E8: a closed manifest script in noscript without an end tag |
| `EDGE-COMBO-NOSCRIPT-STYLE-SCRIPT` | valid, 1 block | M-11, M-13 | E8: a manifest script start tag inside style text in noscript is not a tag |
| `EDGE-COMBO-STYLE-CDATA-EOF` | valid, 1 block | M-07, M-12, M-20 | E2 + E5: the input ends inside svg style after a CDATA section |
| `EDGE-E1-FOREIGN-LT` | valid, 1 block | M-07 | E1: a &lt; emitted as a character in svg desc, svg style and math mi |
| `EDGE-E2-FOREIGN-EOF` | valid, 1 block | M-07, M-20 | E2: the input ends inside svg title, after all blocks |
| `EDGE-E3-NOSCRIPT-EOF` | valid, 1 block | M-11 | E3: noscript without an end tag; the rest of the input passes M-11 |
| `EDGE-E3-NOSCRIPT-EOF-BLOCK` | `STRUCTURE` | M-11 | E3: noscript without an end tag; the rest of the input has block markup |
| `EDGE-E4-CDATA-EOF` | valid, 1 block | M-12 | E4: &lt;![CDATA[ without > before the end of the input |
| `EDGE-E5-FOREIGN-CDATA` | valid, 1 block | M-07, M-12 | E5: CDATA sections in svg style and svg desc |
| `EDGE-E5-FOREIGN-CDATA-GT` | `STRUCTURE` | M-07, M-12 | E5: a CDATA section with > inside, in svg style |
| `EDGE-E6-PI-EOF` | `STRUCTURE` | M-03, M-07 | E6: &lt;? to the end of the input inside svg desc (a comment) |
| `EDGE-E6-PI-EOF-HTML` | valid, 1 block | M-03 | E6: &lt;? to the end of the input in HTML context |
| `EDGE-E7-FOREIGN-FRAMESET` | `STRUCTURE` | M-10 | E7: a frameset start tag inside svg |
| `EDGE-E8-NOSCRIPT-MANIFEST` | `RESERVED` | M-11, M-13 | E8: a manifest script inside noscript |
| `EMPTY` | valid, 2 blocks | M-20, M-27 | Blocks with empty content |
| `ENCODING-INVALID` | `ENCODING` | M-02 | Invalid UTF-8 sequence (C3 28) |
| `ENCODING-OVERLONG` | `ENCODING` | M-02 | Overlong UTF-8 form (C0 AF) |
| `ENCODING-SURROGATE` | `ENCODING` | M-02 | UTF-8 encoded surrogate code point (ED A0 80) |
| `ENCODING-TRUNCATED` | `ENCODING` | M-02 | The input ends in a truncated UTF-8 sequence (E2 82) |
| `EOF` | `STRUCTURE` | M-20 | The input ends in the content of an html block |
| `EOF-JSON` | `STRUCTURE` | M-20 | The input ends in the script data of a json block |
| `EOF-TITLE` | `STRUCTURE` | M-20 | The input ends in the RCDATA content of a title block |
| `FORBIDDEN-OTHER` | `FORBIDDEN_ELEMENT` | M-16 | An element outside the allowlist (li) cannot be a block |
| `FORBIDDEN-RAWTEXT` | `FORBIDDEN_ELEMENT` | M-16 | A RAWTEXT element (style) cannot be a block |
| `FORBIDDEN-RCDATA` | `FORBIDDEN_ELEMENT` | M-16 | An RCDATA element (textarea) cannot be a block |
| `FORBIDDEN-SCRIPT-JS` | `FORBIDDEN_ELEMENT` | M-16 | A script without type (JavaScript) cannot be a block |
| `FORBIDDEN-SCRIPT-TYPE` | `FORBIDDEN_ELEMENT` | M-16 | A script with another JSON type (application/ld+json) cannot be a block |
| `FORBIDDEN-SCRIPT-TYPE-CASE` | `FORBIDDEN_ELEMENT` | M-16 | The json script type is case-sensitive (Application/JSON) |
| `FORBIDDEN-SVG` | `FORBIDDEN_ELEMENT` | M-16 | An svg title is not the HTML title and cannot be a block |
| `FORBIDDEN-TEMPLATE` | `FORBIDDEN_ELEMENT` | M-16 | A template element cannot be a block |
| `FORBIDDEN-VOID` | `FORBIDDEN_ELEMENT` | M-16 | A void element (img) cannot be a block |
| `FOREIGN-BREAKOUT` | `STRUCTURE` | M-07 | A p start tag inside svg (breaks out of foreign content) |
| `FOREIGN-INTEGRATION` | `STRUCTURE` | M-07 | HTML markup inside svg foreignObject |
| `FOREIGN-TEXT-ELEMENT` | `STRUCTURE` | M-07 | Markup inside an svg style element (RAWTEXT in HTML, markup in svg) |
| `FORM-NESTED` | `STRUCTURE` | M-10 | Nine form elements with a nested form in object; the outer forms stay open and a later a cuts a block (H-36, review repro) |
| `FORM-NESTED-DEPTH` | `STRUCTURE` | M-10, M-26 | 600 nested forms in object: D stays 3, the WHATWG stack grows to 603 (H-36; input generated by gen.py) |
| `FORM-SEQUENTIAL` | valid, 1 block | M-10 | Forms one after the other, also after a div end tag that closed a form (H-36: valid) |
| `FORMATTING-CROSS-FOREIGN` | `STRUCTURE` | M-06, M-07 | An a end tag in svg crosses an svg text element (H-26) |
| `FORMATTING-CROSS-ONE` | `STRUCTURE` | M-06 | A b end tag crosses one div (H-26) |
| `FORMATTING-CROSS-PHRASING` | valid, 1 block | M-06, M-24 | Formatting end tags over phrasing elements, &lt;/p> over b, 8 div elements closed in order in an a (H-26: valid) |
| `FORMATTING-CROSS-SPECIAL` | `STRUCTURE` | M-06 | An a end tag crosses 8 div elements; a later a cuts a span block (H-26, review repro) |
| `FORMATTING-CROSS-SPECIAL-NOBR` | `STRUCTURE` | M-06 | A nobr end tag crosses 8 div elements (H-26) |
| `FORMATTING-MARKED-LIST` | `STRUCTURE` | M-06 | Four li end tags cross a ul; the mark passes down to the a; the a end tag pops it (H-32) |
| `FORMATTING-MARKED-NOBR` | `STRUCTURE` | M-06 | A span end tag crosses 8 div elements and marks the nobr; the nobr end tag pops it (H-32) |
| `FORMATTING-MARKED-PHRASING` | valid, 1 block | M-06, M-24 | Mis-nesting of phrasing elements sets no special-crossed mark (H-32: valid) |
| `FORMATTING-MARKED-SPAN` | `STRUCTURE` | M-06 | A span end tag crosses 8 div elements and marks the a; the a end tag pops it; a later a cuts a block (H-32) |
| `FORMATTING-START-AAA` | valid, 1 block | M-06, M-24 | a and nobr start tags run the adoption agency outside blocks; the later block is not changed |
| `FRAMESET` | `STRUCTURE` | M-10 | A frameset start tag in the document |
| `GUARD-A` | `STRUCTURE` | M-24 | An a start tag in the content while an a is open |
| `GUARD-BUTTON` | `STRUCTURE` | M-24 | A button start tag in the content while a button is open |
| `GUARD-DD` | `STRUCTURE` | M-24 | A dt start tag in the content without a dl opened in the content |
| `GUARD-DD-NESTED` | `STRUCTURE` | M-24 | A dt start tag in the content while a dd opened in the content is above the nearest dl (item j) |
| `GUARD-HEADING` | `STRUCTURE` | M-24 | A heading block contains a heading start tag |
| `GUARD-HEADING-NESTED` | `STRUCTURE` | M-24 | Nested headings in the content of a div block inside a heading |
| `GUARD-LI` | `STRUCTURE` | M-24 | An li start tag in the content without a list opened in the content |
| `GUARD-LI-NESTED` | `STRUCTURE` | M-24 | An li start tag in the content while an li opened in the content is above the nearest list (item j) |
| `GUARD-NOBR` | `STRUCTURE` | M-24 | A nobr start tag in the content while a nobr is open |
| `GUARD-P-BLOCK` | `STRUCTURE` | M-24 | A p block contains a start tag that closes a p (ul) |
| `GUARD-P-SPAN` | `STRUCTURE` | M-24 | A span block contains a start tag that closes a p (div) |
| `GUARD-RUBY` | `STRUCTURE` | M-24 | An rt start tag in a p block inside a ruby, without a ruby opened in the content |
| `GUARD-RUBY-RB` | `STRUCTURE` | M-24 | An rb start tag in the content while an rt is open |
| `GUARD-TABLE-PART` | `STRUCTURE` | M-24 | Table part start tags in the content without a table opened in the content |
| `GUARD-TABLE-PART-CELL` | `STRUCTURE` | M-09, M-24 | A td start tag in the content of a block inside a td |
| `ID-CHARREF` | `INVALID_ID` | M-15 | A character reference in the raw value (decodes to a valid id) |
| `ID-EDGE` | valid, 3 blocks | M-15 | Shortest id, id with _ and -, and an id of 64 characters |
| `ID-EMPTY` | `INVALID_ID` | M-15 | Empty id |
| `ID-LEADING-HYPHEN` | `INVALID_ID` | M-15 | Id starts with a hyphen |
| `ID-LEADING-UNDERSCORE` | `INVALID_ID` | M-15 | Id starts with an underscore |
| `ID-NO-VALUE` | `INVALID_ID` | M-15 | Attribute without a value (empty id) |
| `ID-NON-ASCII` | `INVALID_ID` | M-15 | Non-ASCII letters in the id |
| `ID-PUNCTUATION` | `INVALID_ID` | M-15 | A dot in the id |
| `ID-TOO-LONG` | `INVALID_ID` | M-15 | Id of 65 characters |
| `ID-UPPERCASE` | `INVALID_ID` | M-15 | Upper-case letter in the id |
| `ID-WHITESPACE` | `INVALID_ID` | M-15 | White space in the id |
| `JSON` | valid, 1 block | M-16, M-20, M-25 | A json block in a script element with type application/json |
| `JSON-DEPTH-64` | valid, 1 block | M-25 | JSON with nesting depth 64 (the maximum) |
| `JSON-EDGE` | valid, 1 block | M-25 | JSON with CR LF white space, a huge number, duplicate names, a surrogate pair and an escaped slash |
| `LIMIT-256` | valid, 256 blocks | M-26, M-27 | 256 blocks: the maximum number (generated by gen.py) |
| `LIMIT-257` | `LIMIT` | M-26 | 257 blocks: one more than the maximum (input generated by gen.py) |
| `LIMIT-BLOCK-SIZE` | `LIMIT` | M-26 | One block with 1 048 577 content bytes: one byte more than the maximum (input generated by gen.py) |
| `LIMIT-DEPTH-256` | valid, 1 block | M-05, M-26 | 255 nested div elements and a div block inside them: D = 256 (the maximum) |
| `LIMIT-DEPTH-257` | `LIMIT` | M-26 | 256 nested div elements and a div block inside them: D = 257 |
| `LIMIT-DEPTH-DIRTY` | `LIMIT` | M-26 | 70 x &lt;span>&lt;span>&lt;span>&lt;div>&lt;/span>&lt;/span>&lt;/span>: pops of dirty elements do not decrease D (input generated by gen.py) |
| `LIMIT-DEPTH-DIRTY-CLEAN` | valid, 1 block | M-26 | The same elements closed in order: D goes back to 0 (input generated by gen.py) |
| `LIMIT-DEPTH-FOREIGN` | `LIMIT` | M-05, M-26 | 257 nested elements in svg, outside blocks: D = 257 |
| `LIMIT-DEPTH-IGNORED-END` | `LIMIT` | M-26 | 300 x &lt;span>&lt;div>&lt;/span> before a block: D = 600 (review repro; input generated by gen.py) |
| `LIMIT-DEPTH-IGNORED-END-127` | valid, 1 block | M-26 | 127 x &lt;span>&lt;div>&lt;/span>, a div and a div block: D = 256 (input generated by gen.py) |
| `LIMIT-DEPTH-MIXED` | `LIMIT` | M-26 | 29 x the review shapes with h1/h2, li/ul, body and form end tags (input generated by gen.py) |
| `LIMIT-DEPTH-NOSCRIPT` | valid, 1 block | M-11, M-26 | 255 div elements, a noscript element with 300 nested elements in its content, a block: C does not count, D = 256 (input generated by gen.py) |
| `LIMIT-DEPTH-NOSCRIPT-ELEMENT` | `LIMIT` | M-11, M-26 | 256 div elements and a noscript element: D = 257 (input generated by gen.py) |
| `LIMIT-DEPTH-OUTSIDE` | `LIMIT` | M-26 | 257 nested b elements before the block, closed by matching end tags: D = 257 |
| `LIMIT-DEPTH-TABLE` | `LIMIT` | M-09, M-26 | 64 levels of &lt;table>&lt;td> (4 each with the implied row group and row): D = 257 (input generated by gen.py) |
| `LIMIT-DEPTH-TABLE-63` | valid, 1 block | M-09, M-19, M-26 | 63 levels of &lt;table>&lt;td> and a block in the deepest cell: D = 253 (input generated by gen.py) |
| `LIMIT-DEPTH-VOID` | valid, 1 block | M-05, M-26 | 256 nested div elements with void elements and self-closing svg and math inside: they do not change D |
| `LIMIT-INPUT-SIZE` | `LIMIT` | M-01 | Input of 5 242 881 bytes: one byte more than the maximum (input generated by gen.py) |
| `LIST-NESTED` | valid, 1 block | M-23, M-24 | Nested ul, dl and ol lists in the content; each li, dd or dt has its own list (item j) |
| `MATH` | valid, 1 block | M-07 | MathML with text in mi, mo, mn |
| `MULTI-ORDER` | valid, 5 blocks | M-16, M-21, M-27 | Several blocks of all kinds, reported in document order |
| `NESTED` | `NESTED` | M-17 | A block start tag in the content of another block |
| `NESTED-MARKUP` | valid, 1 block | M-22, M-23, M-24 | Balanced content with lists, a table, links, a button and ruby |
| `NO-END-TAG` | `STRUCTURE` | M-20, M-24 | A p block closed by an implied end tag (next p): no explicit end tag |
| `NOSCRIPT` | valid, 1 block | M-04, M-11 | Typical noscript fallbacks (iframe, img) are valid |
| `NOSCRIPT-BALANCED` | valid, 1 block | M-11 | Balanced noscript content with a link, an image, a table and a paragraph (H-27: valid) |
| `NOSCRIPT-BLOCK` | `STRUCTURE` | M-11 | Block markup inside noscript (text with scripting, a block without) |
| `NOSCRIPT-COMMENT` | `STRUCTURE` | M-11 | noscript content that ends inside a comment when parsed as markup |
| `NOSCRIPT-CROSSING` | `STRUCTURE` | M-11 | noscript content closes elements out of order (H-27) |
| `NOSCRIPT-EOF-UNBALANCED` | `STRUCTURE` | M-11 | noscript without an end tag; the rest of the input leaves a div open (E3, H-27) |
| `NOSCRIPT-RESERVED-NON-ASCII` | `RESERVED` | M-11, M-13 | A script type with a dotless i in noscript content (H-28) |
| `NOSCRIPT-STRAY-END` | `STRUCTURE` | M-11 | A stray end tag in noscript content (H-27) |
| `NOSCRIPT-TABLE-IN-CELL` | valid, 1 block | M-11, M-19 | noscript content in a cell with its own table (H-38: valid) |
| `NOSCRIPT-TABLE-PART` | `STRUCTURE` | M-11 | A td in noscript content in a cell without a table in C (H-38, review repro) |
| `NOSCRIPT-TABLE-PART-CAPTION` | `STRUCTURE` | M-11 | A td in noscript content in a caption (H-38) |
| `NOSCRIPT-TABLE-PART-COL` | `STRUCTURE` | M-11 | A col in noscript content in a cell (H-38) |
| `NOSCRIPT-TABLE-PART-TH` | `STRUCTURE` | M-11 | A tr in noscript content in a th cell (H-38) |
| `NOSCRIPT-UNBALANCED` | `STRUCTURE` | M-11 | noscript content leaves a button open (H-27, review repro) |
| `NOSCRIPT-UNBALANCED-TABLE` | `STRUCTURE` | M-11 | noscript content leaves a table open (H-27) |
| `NOT-A-MARKER` | valid, 1 block | M-14 | An end tag with the attribute and similar attribute names are not blocks |
| `NUL-CONTROL` | valid, 1 block | M-02, M-03 | U+0000 and other control characters are valid content bytes |
| `OPTIONAL-END-OUTSIDE` | valid, 1 block | M-06, M-23 | Omitted optional end tags (li, p) outside blocks are allowed |
| `PI-TEXT` | valid, 1 block | M-03, M-14 | A processing instruction or bogus comment &lt;?...> ends at the first > |
| `RESERVED` | `RESERVED` | M-13 | The document already contains a manifest script |
| `RESERVED-CASE` | `RESERVED` | M-13 | A manifest script type with other case and white space |
| `RESERVED-CHARREF` | `RESERVED` | M-13 | A script type with a character reference that could decode to the manifest type |
| `RESERVED-NON-ASCII` | `RESERVED` | M-13 | A script type with a dotless i (U+0131) (H-28) |
| `RESERVED-NON-ASCII-LONG-S` | `RESERVED` | M-13 | A script type with a long s (U+017F) (H-28) |
| `RESERVED-NON-ASCII-OTHER` | `RESERVED` | M-13 | Any non-ASCII character in a script type (text/jävascript) (H-28) |
| `SCRIPT-NON-ASCII` | valid, 1 block | M-13, M-16 | Non-ASCII text in script content and in other script attributes (H-28: valid) |
| `SCRIPT-TEXT` | valid, 1 block | M-04, M-14 | Tag-like text in script data, also in the double-escaped state, is not a block |
| `SELECT` | valid, 1 block | M-08 | A select with option, optgroup and explicit end tags |
| `SELECT-CONTENT` | `STRUCTURE` | M-08 | A b element inside select (parsers differ for select content) |
| `SELF-CLOSING-BLOCK` | `STRUCTURE` | M-18 | A block start tag with the self-closing flag |
| `SIMPLE` | valid, 1 block | M-14, M-15, M-16, M-20, M-27 | One html block in the body |
| `SPAN-IN-P` | valid, 1 block | M-24 | A span block inside a paragraph |
| `STYLE-TEXT` | valid, 1 block | M-04, M-14 | Tag-like text in a style element is not a block |
| `SVG-CROSSING` | valid, 1 block | M-06, M-07 | Inside svg an end tag can cross other svg elements |
| `SVG-SELF-CLOSING` | valid, 1 block | M-05, M-16 | A self-closing svg start tag opens no foreign region |
| `SVG-TITLE` | valid, 1 block | M-05, M-07, M-16 | An svg title and svg style are not the page title and not RCDATA/RAWTEXT |
| `TABLE-CELL` | valid, 3 blocks | M-09, M-19 | Blocks inside a caption, a td and a th |
| `TABLE-CURRENT` | valid, 1 block | M-09, M-19 | Normal tables: each table part, col and table start tag has F as the current node (H-37: valid) |
| `TABLE-FOSTER` | `STRUCTURE` | M-09, M-24 | A div and a table start tag directly in a table in the content (item k: foster parenting); the table start tag also breaks M-09 condition 2 |
| `TABLE-FOSTER-MARKER` | `STRUCTURE` | M-09 | A table start tag while a foster-parented marquee is the current node; its marker stays and a later a cuts a block (H-37, review repro) |
| `TABLE-FOSTER-MARKER-DEPTH` | `STRUCTURE` | M-09, M-26 | 600 foster-parented marquee tables: D stays small, the WHATWG stack grows to 605 (H-37; input generated by gen.py) |
| `TABLE-IMPLIED-PARTS` | valid, 1 block | M-06, M-09, M-19 | Implied tbody, tr, colgroup and stray td, th, caption, colgroup, col end tags (H-30: valid) |
| `TABLE-NESTING` | `STRUCTURE` | M-09 | A td start tag while a td is open (implied end of the cell) |
| `TABLE-STRAY-TABLE` | `STRUCTURE` | M-06 | A table end tag with no table on the token stack (H-30) |
| `TABLE-STRAY-TBODY` | `STRUCTURE` | M-06 | A stray tbody end tag after an implied tbody (H-30, review repro) |
| `TABLE-STRAY-THEAD` | `STRUCTURE` | M-06 | A thead end tag with no thead on the token stack (H-30) |
| `TABLE-STRAY-TR` | `STRUCTURE` | M-06 | A stray tr end tag after an implied tr (H-30, review repro) |
| `TABLE-TEXT` | valid, 1 block | M-09, M-24 | Text and a comment directly in a table in the content, and elements in a cell (item k) |
| `TEMPLATE-COL-TEXTAREA` | `STRUCTURE` | M-09 | A col directly in a template in the head; the ignored textarea hides a manifest script (H-35, review repro) |
| `TEMPLATE-COL-TITLE` | `STRUCTURE` | M-09 | A col directly in a template; the ignored title makes a fake block (H-35, review repro) |
| `TEMPLATE-FOR` | `STRUCTURE` | M-10 | A template with a for attribute after a block with start and end markers: content patching replaces the block content (H-43, Chrome 154 repro) |
| `TEMPLATE-FOR-EMPTY` | `STRUCTURE` | M-10 | A template with a for attribute without a value, in mixed case (H-43: the value does not matter) |
| `TEMPLATE-FOR-SVG` | valid, 1 block | M-07, M-10 | A template with a for attribute in svg is a foreign element and does not patch (H-43: valid) |
| `TEMPLATE-OUTSIDE` | valid, 1 block | M-05, M-19 | A template element without blocks, then a block outside it |
| `TEMPLATE-TABLE-PARTS` | valid, 1 block | M-09, M-19 | tr and td in a template, col in a table in a template (H-35: valid) |
| `TEXTAREA-TEXT` | valid, 1 block | M-04, M-14 | Tag-like text in a textarea element (RCDATA) is not a block |
| `TITLE` | valid, 1 block | M-04, M-16, M-20 | The page title is a block; its RCDATA content is kept as raw bytes |
| `TITLE-TEXT` | valid, 1 block | M-04, M-14 | Tag-like text in a title element that is not a block |
| `UNBALANCED` | `STRUCTURE` | M-23 | The block end tag comes while an element of the content is open |
| `UNBALANCED-STRAY` | `STRUCTURE` | M-23 | A stray end tag in the content |
| `UNICODE-ASTRAL` | valid, 1 block | M-02, M-27 | Non-BMP characters (4 byte UTF-8, 2 UTF-16 units) before and in a block |
| `UNICODE-BMP` | valid, 1 block | M-02, M-27 | Cyrillic, CJK and symbols (2 and 3 byte UTF-8) before and in a block |
| `UPPERCASE` | valid, 4 blocks | M-03, M-14, M-16 | Upper-case and mixed-case tag and attribute names |
| `ZERO-BLOCKS` | valid, 0 blocks | M-27 | A document without blocks is valid and has an empty block list |

## 5. Conformance runner

The runner (`server/conformance`) is language-neutral. It starts the server under test, sends `request.http`, and compares the response with `response.http` using these rules:

| Part | Comparison |
|---|---|
| Status | Exact |
| Protocol headers | Exact names (case-insensitive) and values |
| Other headers | Presence of required headers (`Content-Type`, `Cache-Control`, `Vary`) |
| HTML body | Byte-exact (after decoding transfer and content encodings) |
| JSON body | Semantic equality (key order ignored), unless the fixture marks it byte-exact |

## Related

- [Test strategy](/engineering/quality/testing)
- [Server conformance](/engineering/server/conformance)
