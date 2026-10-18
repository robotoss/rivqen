# RQP markup

This page specifies how a page marks its data blocks in the Rivqen protocol (RQP), when a document is valid for RQP, and which bytes are the content of each block. The markup is valid HTML. It replaces the legacy comment markers.

**Status:** <Badge type="info" text="DESIGN" /> Normative for `rqp/1` from WP-17 S1. Decisions: attribute markup and inline manifest ([ADR-006](/engineering/architecture/adr/#adr-006)); a block without an explicit end tag is invalid (human decision H-12, [sprint record WP-17 S1](/engineering/plan/sprints/WP-17-S1)); M-24 items j and k (H-18); edge cases E1–E7 (H-21) and E8 (H-22), section 2.11; rules from the security review: M-06 items 2 to 4 (H-26, H-30, H-32), M-09 conditions 1 and 2 (H-35, H-37), M-10 item 2 (H-36), M-10 item 3 (H-43, from the conformance study T-17), M-11 item 6 (H-38), M-11 item 5 (H-27), M-13 non-ASCII types (H-28) and the depth counter D of M-26 (H-25, amended by H-31), section 2.12. Parser interface, hashes and revisions: `tools/rqp-markup/CONTRACT.md`. Golden fixtures: [`FX-RQ-MARKUP-*`](/engineering/protocol/fixtures#_4-2-markup-fixtures-fx-rq-markup).

[[toc]]

## 1. Example

```html
<!DOCTYPE html>
<html>
<head>
  <title data-rq-block="title">Shop</title>
</head>
<body>
  <header>Static header</header>
  <p class="price" data-rq-block="price">120.00</p>
  <script type="application/json" data-rq-block="cart">{"items":3}</script>
</body>
</html>
```

| Block | Format | Content |
|---|---|---|
| `title` | `html` | `Shop` |
| `price` | `html` | `120.00` |
| `cart` | `json` | `{"items":3}` |

The server SDK finds the blocks with the rules of section 2. Then it adds the page manifest to `<head>` ([manifest and patch](/engineering/protocol/manifest-patch)). The template author does not write the manifest.

## 2. Normative rules (rqp/1)

<Badge type="info" text="DESIGN" /> The key words MUST, MUST NOT and SHOULD are used as in RFC 2119.

**Principle.** The ground truth is the WHATWG HTML parser (tokenization and tree construction). The rules below add conservative restrictions. With these restrictions, a parser that sees only tokens (no tree builder) and a parser with a full tree builder find the same blocks and the same content bytes. A document that does not satisfy a rule is **invalid for RQP**. The server then sends the original document without a manifest (H-12). It is never an error for the page itself.

Each rule has an ID `M-xx` and names the error code of `tools/rqp-markup/CONTRACT.md` §5. When a document breaks more than one rule, a parser can report any of the codes (contract §5).

### 2.1 Terms

| Term | Meaning |
|---|---|
| Input | The bytes of the document, as received |
| Offset | A byte offset into the input. Offset 0 is the first byte, also when it is a byte order mark (BOM). |
| Token | A token of the WHATWG tokenizer: DOCTYPE, start tag, end tag, comment, character, end of file (EOF). In this page a bogus comment and a processing instruction count as a comment. |
| Name | The tag name of a token. The tokenizer converts ASCII upper case to lower case, so names are compared in lower case. |
| Raw value | The source text of an attribute value: the characters between the quotes, or the characters of an unquoted value. Character references are **not** decoded. |
| Token stack | The stack of open elements of rule M-05. It is not the stack of the WHATWG tree builder. |
| Current node | The top element of the token stack |
| Foreign region | The part of the document inside an `svg` or `math` element (M-05) |
| HTML context | Every position that is not in a foreign region |
| D, dirty, implied part | The depth counter of M-26 and its state on the token stack |
| Special-crossed mark | State of a token-stack element for M-06 item 4 |

**Name sets.** The rules use these sets of names:

- `VOID`: `area`, `base`, `basefont`, `bgsound`, `br`, `col`, `embed`, `frame`, `hr`, `image`, `img`, `input`, `keygen`, `link`, `meta`, `param`, `source`, `track`, `wbr`.
- `TEXT`: `title`, `textarea` (RCDATA); `iframe`, `noembed`, `noframes`, `noscript`, `style`, `xmp` (RAWTEXT); `script` (script data); `plaintext` (PLAINTEXT).
- `PCLOSE` (start tags that close an open `p`): `address`, `article`, `aside`, `blockquote`, `center`, `dd`, `details`, `dialog`, `dir`, `div`, `dl`, `dt`, `fieldset`, `figcaption`, `figure`, `footer`, `form`, `h1`–`h6`, `header`, `hgroup`, `hr`, `li`, `listing`, `main`, `menu`, `nav`, `ol`, `p`, `plaintext`, `pre`, `search`, `section`, `summary`, `table`, `ul`, `xmp`.
- `GUARDED`: `a`, `applet`, `button`, `caption`, `colgroup`, `marquee`, `math`, `nobr`, `object`, `select`, `svg`, `table`, `tbody`, `td`, `template`, `tfoot`, `th`, `thead`, `tr`.
- `TABLE_PART`: `caption`, `colgroup`, `tbody`, `td`, `tfoot`, `th`, `thead`, `tr`.
- `TABLE_FAMILY`: `table` and `TABLE_PART`.
- `BREAKOUT`: `b`, `big`, `blockquote`, `body`, `br`, `center`, `code`, `dd`, `div`, `dl`, `dt`, `em`, `embed`, `font`, `h1`–`h6`, `head`, `hr`, `i`, `img`, `li`, `listing`, `menu`, `meta`, `nobr`, `ol`, `p`, `pre`, `ruby`, `s`, `small`, `span`, `strike`, `strong`, `sub`, `sup`, `table`, `tt`, `u`, `ul`, `var`.
- `INTEGRATION`: `annotation-xml`, `desc`, `foreignobject`, `mi`, `mn`, `mo`, `ms`, `mtext`, `title`.
- `BLOCK_HTML`: `article`, `aside`, `div`, `footer`, `h1`–`h6`, `header`, `main`, `nav`, `p`, `section`, `span`, `title`.
- `FORBIDDEN_CONTENT`: `base`, `basefont`, `bgsound`, `body`, `embed`, `frame`, `frameset`, `head`, `html`, `iframe`, `link`, `math`, `meta`, `noembed`, `noframes`, `noscript`, `object`, `optgroup`, `option`, `plaintext`, `script`, `select`, `style`, `svg`, `template`, `textarea`, `title`, `xmp`.
- `FORMATTING`: `a`, `b`, `big`, `code`, `em`, `font`, `i`, `nobr`, `s`, `small`, `strike`, `strong`, `tt`, `u`.
- `SPECIAL` (HTML elements only): `address`, `applet`, `area`, `article`, `aside`, `base`, `basefont`, `bgsound`, `blockquote`, `body`, `br`, `button`, `caption`, `center`, `col`, `colgroup`, `dd`, `details`, `dir`, `div`, `dl`, `dt`, `embed`, `fieldset`, `figcaption`, `figure`, `footer`, `form`, `frame`, `frameset`, `h1`–`h6`, `head`, `header`, `hgroup`, `hr`, `html`, `iframe`, `img`, `input`, `keygen`, `li`, `link`, `listing`, `main`, `marquee`, `menu`, `meta`, `nav`, `noembed`, `noframes`, `noscript`, `object`, `ol`, `p`, `param`, `plaintext`, `pre`, `script`, `search`, `section`, `select`, `source`, `style`, `summary`, `table`, `tbody`, `td`, `template`, `textarea`, `tfoot`, `th`, `thead`, `title`, `tr`, `track`, `ul`, `wbr`, `xmp`.

<Badge type="tip" text="FACT" /> `PCLOSE`, `BREAKOUT` and the void handling of `VOID` come from the WHATWG HTML Living Standard, §13.2.6.4.7 "The rules for parsing tokens in body" and §13.2.6.5 "The rules for parsing tokens in foreign content" (checked against the `whatwg/html` source on 2026-10-09). `INTEGRATION` is the union of the SVG HTML integration points (`foreignObject`, `desc`, `title`) and the MathML text integration points and `annotation-xml`. `FORMATTING` is the "formatting" category and `SPECIAL` is the HTML part of the "special" category of §13.2.4.2 "The stack of open elements" (checked against `html.spec.whatwg.org` on 2026-10-11). The MathML elements `mi`, `mo`, `mn`, `ms`, `mtext`, `annotation-xml` and the SVG elements `foreignObject`, `desc`, `title` are also special; the rules cover them as elements pushed in a foreign region.

### 2.2 Input

**M-01 Input size.** The input MUST NOT be larger than 5 MiB (5 242 880 bytes). Check this first, before any other rule. Error: `RQP_MARKUP_LIMIT`.

**M-02 Encoding.** The input MUST be valid UTF-8 (RFC 3629: no overlong forms, no surrogate code points, no code point above U+10FFFF, no truncated sequence). A UTF-8 BOM (`EF BB BF`) at offset 0 is allowed. The tokenizer does not see it, but it stays in the offsets and in the template (contract §4). U+FEFF at any other offset is a normal character. U+0000 and other control characters are valid characters. Error: `RQP_MARKUP_ENCODING`.

The parser decodes the input as UTF-8 always. It ignores `<meta charset>` and the `http-equiv` charset declaration. <Badge type="info" text="DESIGN" /> RQP is for UTF-8 pages only (H-62, RQP-26): the server passes through a page whose `Content-Type` has another charset, and every F has exactly `Content-Type: text/html; charset=utf-8` (H-66). A transport-layer charset has priority over `<meta>` in the WHATWG encoding sniffing algorithm, so the browser also decodes UTF-8 ([negotiation §4, charset rule](/engineering/protocol/negotiation#_4-response-selection-server)).

### 2.3 Tokenization and token stack

**M-03 Tokenizer.** The parser tokenizes the input with the WHATWG tokenizer (HTML §13.2.5), from the data state, after the input stream preprocessing (CR and CR LF become LF). Preprocessing does not change offsets: offsets always refer to the input bytes. The tree builder is **not** used. The tokenizer state changes that the tree builder would make are defined by M-04 only.

- A start tag ends at its `>`. A `>` inside a quoted attribute value does not end the tag.
- Comments follow the WHATWG comment states, including `<!-->`, `<!--->` and the end `--!>`.
- `<!` followed by anything other than `--`, `DOCTYPE` (ASCII case-insensitive) or `[CDATA[` starts a bogus comment. A CDATA section starts only with the exact, case-sensitive `<![CDATA[`. A `<![cdata[` or any other case is a bogus comment: M-12 does not apply to it, and M-07 item 5 does not allow it. `</` followed by a character that is not an ASCII letter and not `>` starts a bogus comment. A bogus comment ends at the first `>`. If there is no `>`, it ends at the end of the input.
- `<?` starts a bogus comment in older parsers and a processing instruction in newer ones. <Badge type="tip" text="FACT" /> Both end at the first `>` (WHATWG "processing instruction data state" and "bogus comment state"). RQP treats them the same: a comment.
- A `<?` with no `>` before the end of the input is a comment that ends at the end of the input (E6, H-21). <Badge type="warning" text="RESEARCH" /> The current WHATWG text probably emits no token for a processing instruction that reaches the end of the input ([edge cases](/engineering/protocol/markup-edge-cases), E6). The comment reading is the stricter one: every rule that allows a comment also allows no token.
- A DOCTYPE token ends at the first `>`.
- An end tag can have attributes and a self-closing flag. They have no effect.
- When a start tag has two attributes with the same name, the first one is used and the second is dropped (WHATWG "attribute name state").

**M-04 Tokenizer state changes.** Immediately after a start tag token in HTML context, the tokenizer changes state by the name of the tag:

| Name | New state |
|---|---|
| `title`, `textarea` | RCDATA |
| `iframe`, `noembed`, `noframes`, `noscript`, `style`, `xmp` | RAWTEXT |
| `script` | script data |
| `plaintext` | PLAINTEXT |

This applies also when the start tag has the self-closing flag. RCDATA, RAWTEXT and script data end at the "appropriate end tag" (the same name, followed by white space, `/` or `>`). PLAINTEXT never ends. In a foreign region a start tag never changes the state. There is no other state change. A CDATA section (`<![CDATA[`) can be tokenized as a CDATA section or as a bogus comment; M-12 makes both give the same token boundaries.

<Badge type="tip" text="FACT" /> `noscript` is RAWTEXT when scripting is enabled (WHATWG: "A start tag whose tag name is `noscript`, if scripting mode is not Disabled"). A WebView with the Rivqen SDK has scripting enabled, so RQP uses this view. M-11 makes `noscript` content safe for parsers that parse it as markup.

**M-05 Token stack.** The parser keeps a token stack. It starts empty. Characters, comments and DOCTYPE tokens do not change it.

1. A start tag in HTML context:
   - If the name is `svg` or `math` and the self-closing flag is set: do not push.
   - Else, if the name is in `VOID`: do not push.
   - Else: push an element with this name. A self-closing flag on any other name has no effect (it is ignored as in HTML).
2. A start tag in a foreign region: push an element, unless the self-closing flag is set.
3. An end tag: find the nearest element on the stack (from the top) with the same name. If there is none, ignore the end tag. If there is one, pop all elements above it and then pop it. The elements above it are **crossed** (M-06).
4. A position is in a **foreign region** when the stack contains an `svg` or `math` element that was pushed in HTML context. The region starts after that start tag and ends when that element is popped.

The token stack is not the DOM. It does not do implied end tags, foster parenting or the adoption agency algorithm. The rules M-06 to M-24 make sure that, for a valid document, the differences do not change which elements are blocks or where their content ends. The depth counter D of M-26 bounds the depth of the WHATWG tree, which can be larger than the token stack.

### 2.4 Document rules

These rules apply to the whole document, also when it has no block. They remove the cases where the token stack and the WHATWG tree disagree about an element that matters for blocks. Error for all of them, except M-11 item 4 and M-13: `RQP_MARKUP_STRUCTURE`.

**M-06 End tags.**

1. **Guarded elements.** An end tag MUST NOT cross (M-05, step 3) an element that was pushed in HTML context and whose name is in `GUARDED`. (Elements pushed in a foreign region, for example an SVG `a`, are not guarded by this item; M-07 and item 2 apply to them.) Exception: an end tag with a name in `TABLE_FAMILY` can cross elements with a name in `TABLE_PART`.
2. **Formatting end tags.** An end tag with a name in `FORMATTING` MUST NOT cross an element that was pushed in HTML context and whose name is in `SPECIAL`. It MUST NOT cross an element that was pushed in a foreign region. (Human decision H-26.)
3. **Stray table end tags.** In HTML context, an end tag with the name `tbody`, `thead`, `tfoot`, `tr` or `table` MUST match an element on the token stack (M-05, step 3). The elements that the tree builder inserts implicitly (a `tbody` or a `tr`, M-26) stay allowed. (Human decision H-30.)
4. **Special-crossed elements.** An end tag with a name in `FORMATTING` MUST NOT pop (M-05, step 3: the matching element or a crossed element) an element that has the **special-crossed mark**. (Human decision H-32.) The mark is state of the token stack:
   - Each element is pushed without the mark.
   - After an end tag pops elements, the new current node (if there is one) gets the mark when (a) an element that the end tag crosses was pushed in HTML context and has a name in `SPECIAL`, or was pushed in a foreign region, or (b) a popped element (matching or crossed) has the mark. Item (b) passes the mark down to the next element, also when the end tag pops exactly one element.
   - An end tag that matches nothing, and a start tag, do not change the mark. The mark is never removed from an element; it goes away when the element is popped.

Examples:

- Allowed: `<div><span>a</div>` (crosses `span`), `<ul><li>a</ul>` (crosses `li`), `<tr><td>a</tr>` (crosses `td`), `<a href="#"><span>a</a>` and `<b><i>a</b>` (a formatting end tag crosses a phrasing element), `<p><b>a</p>` (`p` is not in `FORMATTING`), `<a href="#"><b><span><i>a</span></b></a>` (`</span>` crosses only `i`, so nothing gets the mark; item 4), `<table><td>a</td></table>` (implied `tbody` and `tr`), a stray `</td>`, `</th>`, `</caption>`, `</colgroup>` or `</col>`.
- Invalid: `<div><button>a</div>` (crosses `button`), `<td><a href="#">a</td>` (crosses `a`), `<b><div>a</b>` (item 2), `<svg><a><text>t</a></svg>` (item 2, a foreign element), `<table><td></tbody>` and `<table><td></tr>` (item 3), `<a><span><div>a</span></a>` (`</span>` crosses `div` and marks `a`; item 4), `<a><li><ul></li></a>` (item 4).

<Badge type="tip" text="FACT" /> Item 2: in the "in body" insertion mode, an end tag with a name in `FORMATTING` runs the adoption agency algorithm (WHATWG §13.2.6.4.7, "An end tag whose tag name is one of: a, b, …, u"; the algorithm is in the same section). The algorithm looks for the "furthest block": the first element in the special category below the formatting element on the stack of open elements. When there is none, it pops the stack "from the current node up to and including formattingElement" and returns: the same elements that the token stack pops. With a furthest block, it moves elements to new parents, and it stops after 8 passes of its outer loop ("If outerLoopCounter is greater than or equal to 8, then return"). Then the formatting element can stay open in the browser while the token stack has closed it. A later `<a>` or `<nobr>` start tag in a block then runs the algorithm again and cuts the block (security review, confirmed in parse5 8.0.1 for `<a><div>`×8`x</a><span data-rq-block="x">y<a>z</a>w</span>`: parse5 has no end tag for the block). Item 2 removes the furthest block when every element above the matching element on the WHATWG stack is an element of the token stack or a reconstructed formatting element. Item 4 makes this condition true; see below.

<Badge type="tip" text="FACT" /> Item 4: after an earlier crossing by another end tag, the WHATWG stack can hold elements that the token stack no longer has. The tree builder keeps an element open that the token stack popped only when one of its end-tag steps stops at an element that it must not pass: "any other end tag" ignores the token at "an element in the special category" (`</span>` over a `div`); `</li>` needs an `li` "in list item scope", and `ol` and `ul` end that scope (`</li>` over a `ul`); a heading end tag pops only "until an HTML element whose tag name is one of h1 … h6 has been popped" (`</h1>` over an `h2`); `</form>` is ignored when the form element pointer is null or its form is not in scope, and else removes only that form (all WHATWG §13.2.6.4.7). In the first three cases the element where the step stops is a crossed element in `SPECIAL`. In the `</form>` case, M-10 item 2 (H-36) allows at most one `form` on the token stack, so the token stack and the pointer name the same form or the tree builder has no form: the pointer is set by the last `form` start tag that the tree builder inserted, and only `</form>` sets it to null. A scope boundary above that form on the WHATWG stack is either on the token stack too, so `</form>` crosses it (M-06 item 1 rejects that: the boundaries are in `GUARDED` or foreign), or it was popped earlier by a crossing over a `GUARDED` element, which M-06 item 1 also rejects. So an exact pop of the form is mirrored; a crossing leaves the crossed elements open, and when one of them is in `SPECIAL` the crossing marks; an element that is not special cannot be a furthest block. A `form` start tag that the tree builder ignores (the pointer still names a form that an earlier crossing closed) creates no element, so nothing stays open. So every special element that stays open in the tree builder after the token stack popped it was crossed by an end tag that set the mark, and the mark stays on the element below it on the token stack until that element is popped. A formatting element m that is not marked, and has no marked element above it, therefore has no such special element above it on the WHATWG stack, the adoption agency finds no furthest block, and it pops the same elements as the token stack. Examples that item 4 rejects (parse5 8.0.1 has no end tag for the block): `<a><span>`, 8 × `<div>`, `x</span></a><span data-rq-block="x">y<a>z</a>w</span>`, the same with `nobr`, and `<a>`, 4 × `<li><ul>`, 4 × `</li>`, `x</a>` before such a block ([edge cases §6.1](/engineering/protocol/markup-edge-cases#_6-1-m-06-item-2-formatting-end-tags-h-26)). Mis-nesting of phrasing elements does not set the mark: in `<a href="#"><b><span><i>x</span></b></a>`, the tree builder pops `i` and `span` for `</span>` ("any other end tag" passes the non-special `i`), so it keeps nothing open, and the token stack agrees. The mark also covers crossings over foreign elements, which the tree builder pops (M-07); that part is stricter than needed and was decided by the human (H-32).

<Badge type="tip" text="FACT" /> Item 3: in the "in table body" and "in row" insertion modes, an end tag `tbody`, `thead`, `tfoot` closes the row group and an end tag `tr` closes the row when the element is in table scope (WHATWG §13.2.6.4.13, §13.2.6.4.14). In the "in cell" mode, `</tbody>`, `</tfoot>`, `</thead>`, `</tr>` and `</table>` first "close the cell" (§13.2.6.4.15). So a stray `</tbody>` or `</tr>` closes an implied `tbody` or `tr` and the open cell, and the next element goes before the table (foster parenting). The token stack has no such element and ignores the end tag. The other table end tags do not change the WHATWG stack when the token stack has no match: `</td>`, `</th>` and `</caption>` need an element in table scope, which the token stack also has, and `</col>` is always ignored. A stray `</colgroup>` closes only an implied `colgroup`, which the next token other than `col` closes anyway ([edge cases §6.4](/engineering/protocol/markup-edge-cases#_6-4-m-06-item-3-stray-table-end-tags-h-30)).

**M-07 Foreign regions.** In a foreign region:

1. A start tag MUST NOT have a name in `BREAKOUT`.
2. An end tag MUST match the `svg` or `math` element of the region or an element pushed inside the region. It can cross other elements of the region. A stray end tag (no match) is not allowed. A region ends only with the end tag of its own `svg` or `math` element, or at the end of the input (item 6).
3. After the start tag of an element with a name in `INTEGRATION` (without the self-closing flag), the tokens up to the end tag of that element MUST be character tokens and CDATA sections (item 5) only. Thus, the content contains no tags, no comments, no DOCTYPE tokens and no CDATA other than the CDATA sections of item 5. A `<` that the tokenizer emits as a character token is allowed, for example `a < b` or `1 <2` (E1, H-21).
4. A start tag with a name in `TEXT` MUST NOT have the self-closing flag. Its content (up to its end tag) MUST be character tokens and CDATA sections (item 5) only, as in item 3. The name `plaintext` is not allowed.
5. **CDATA sections.** In the content of items 3 and 4, a `<![CDATA[` is allowed when M-12 holds for it. It is a CDATA section. A parser that reads it as a bogus comment (M-04) counts that comment as a CDATA section. This includes a `<![CDATA[` with no `>` before the end of the input (M-12). Every other comment, also a `<?` comment (M-03), is not allowed (E5, H-21).
6. **End of input.** The input can end in a foreign region, also in the content of an element of items 3 and 4, before its end tag. This is not an error by itself (E2, H-21). All other rules still apply. A block cannot be open at that point: a block start tag in a foreign region is invalid (M-16), and the content of an `html` block cannot contain `svg` or `math` (M-22).

Examples: `<svg><desc>a < b</desc></svg>` and `<svg><style><![CDATA[.a{fill:red}]]></style></svg>` are valid. `<svg><desc><b>x</b></desc></svg>`, `<svg><desc><!--x--></desc></svg>` and `<svg><style><![CDATA[a>b{}]]></style></svg>` (M-12) are invalid.

**M-08 Select.** Inside a `select` element (HTML context, from its start tag to its end tag), the only allowed tokens are characters, comments, the start tags `option`, `optgroup`, `hr`, and the end tags `option`, `optgroup`, `select`. The `select` element MUST have an explicit end tag `</select>`.

**M-09 Table parts.** Let F be the nearest element on the token stack with a name in `TABLE_FAMILY`; the search stops at a `template` element. A start tag in HTML context with one of these names is allowed only when F is none or is in the list:

| Start tag | Allowed F |
|---|---|
| `caption`, `colgroup`, `tbody`, `thead`, `tfoot` | `table` |
| `col` | `table`, `colgroup` |
| `tr` | `table`, `tbody`, `thead`, `tfoot` |
| `td`, `th` | `table`, `tbody`, `thead`, `tfoot`, `tr` |

So a cell, a row or a row group must be closed explicitly before the next one starts: `<td>a</td><td>b</td>` is valid, `<td>a<td>b` is not.

Two more conditions apply to start tags in HTML context:

1. **`col` in a template** (human decision H-35). A `col` start tag MUST NOT appear when the nearest element on the token stack with a name in `TABLE_FAMILY` or the name `template` is a `template` element. Invalid: `<template><col>`, `<template><div><col>`. Valid: `<template><tr><td>x</td></tr></template>`, `<template><table><col></table></template>`.
2. **F is the current node** (human decision H-37). A start tag with a name in `TABLE_PART`, or the name `col` or `table`, while F is `table`, `tbody`, `thead`, `tfoot`, `tr` or `colgroup`, MUST have F as the current node. So no other element (for example a `div`, `b`, `form`, `object` or `marquee` that the tree builder moves before the table) may be open above F. Invalid: `<table><marquee><table>`, `<table><tr><object><td>`, `<table><form><tr>`. Valid: `<table><div></div><tr>`, a `table` in a cell.

<Badge type="tip" text="FACT" /> Condition 1: in the "in template" insertion mode, "A start tag whose tag name is "col"" pushes "in column group" and reprocesses the token (WHATWG §13.2.6.4.16). In "in column group" (§13.2.6.4.12), "Anything else: If the current node is not a colgroup element, then this is a parse error; ignore the token." The current node is the `template`, so every later start tag other than `col`, `template` and `html` is ignored, also `title`, `textarea`, `style`, `script`, `xmp`, `iframe`, `noembed`, `noframes`, `noscript` and `plaintext`. The tokenizer then does not switch to raw text, while M-04 does: text that the token stack reads as raw text is markup in the browser (security review round 2: a fake block and a manifest script after `<template><col><title>` or `<template><col><textarea>`). This is the only insertion mode that ignores those start tags in a template, see [edge cases §7.1](/engineering/protocol/markup-edge-cases#_7-1-m-09-col-in-a-template-h-35).

<Badge type="tip" text="FACT" /> Condition 2: in "in table", "in table body" and "in row", a table part, `col` or `table` start tag first "clears the stack back to a table context" (or a table body or row context): "while the current node is not a table, template, or html element, pop elements from the stack of open elements" (§13.2.6.4.9, §13.2.6.4.13, §13.2.6.4.14); a `table` start tag pops "until a table element has been popped". These pops close elements that the tree builder foster-parented ("Enable foster parenting, process the token using the rules for the "in body" insertion mode"), without their end tags. An `applet`, `marquee` or `object` popped this way leaves its marker in the list of active formatting elements, so a later formatting end tag or `<a>` does not find the elements before the marker, and a block is cut (security review round 2; also an unbounded depth). Condition 2 removes every such silent pop for start tags. End tags that pop foster-parented elements (`</table>`, `</tbody>`, `</tr>` over them) are crossings: M-06 item 1 rejects them over `applet`, `marquee`, `object` and `template` (all in `GUARDED`), and M-06 item 4 and M-26 cover the others ([edge cases §7.3](/engineering/protocol/markup-edge-cases#_7-3-m-09-f-is-the-current-node-h-37)).

**M-10 Frameset, nested forms and template patching.**

1. The document MUST NOT contain a `frameset` start tag. This applies in every context: in HTML context and in a foreign region, for example `<svg><frameset/></svg>` (E7, H-21). For the content of `noscript`, M-11 item 3 applies.
2. A `form` start tag in HTML context MUST NOT appear while an element `form` that was pushed in HTML context is on the token stack (human decision H-36). Valid: `<form>…</form><form>…</form>`. Invalid: `<form><div><form>`, `<form><object><form></form></object></form>`. A `form` in a foreign region (`<svg><form>`) is a foreign element and is not affected.
3. A `template` start tag in HTML context MUST NOT have an attribute named `for` (human decision H-43). The name match is ASCII case-insensitive (the tokenizer lowercases attribute names). The value does not matter: an empty value or an attribute without a value is also invalid. Every `template` start tag in HTML context counts: in `head`, in `body`, in a table, in another `template`, with or without `shadowrootmode`. Invalid: `<template for="p">`, `<template FOR>`, `<template for="">`. Valid: `<svg><template for="p"></template></svg>` (a foreign element). For the content of `noscript`, M-11 item 3 applies (it forbids `template`).

<Badge type="tip" text="FACT" /> Item 2: the tree builder has a "form element pointer". "in body": "A start tag whose tag name is "form": If the form element pointer is not null, and the parser is not parsing template contents, then this is a parse error; ignore the token." "An end tag whose tag name is "form"" (not in template contents): "Let node be the element that the form element pointer is set to … Set the form element pointer to null. If node is null or if the stack of open elements does not have node in scope, then this is a parse error; return and ignore the token." Otherwise it removes node from the stack, and the elements above it stay open (WHATWG §13.2.6.4.7). In `<form><object><form></form></object></form>` the tree builder ignores the inner `<form>`; the first `</form>` sets the pointer to null and is ignored (the outer form is not in scope: `object` is a scope boundary); the last `</form>` is ignored too (the pointer is null). So the outer form stays open, while the token stack pops it with an exact pop. Nine such forms before a block make the adoption agency of a later `</a>` stop and a later `<a>` cut the block; 600 of them make a WHATWG stack of 603 elements with D = 3 (security review round 2, parse5 8.0.1). With item 2 the token stack has at most one `form`, and `</form>` is mirrored: see the note of M-06 item 4.

<Badge type="tip" text="FACT" /> Item 3: the WHATWG text has **content patching**. The `template` element has the content attribute `for` ("Updates existing content", §4.12.3 "The template element"). In "in head", "A start tag whose tag name is "template"": when `shadowrootmode` is not set and "templateStartTag's for attribute is specified", the tree builder runs "prepare content patching" (§13.2.6.4.4). The scope is the parent of the place where the template goes (the `html` element when that is the `body`). "find markers" takes the first processing instruction `<?marker name="…">` in the scope, or `<?start name="…">` with the next sibling `<?end>` (without `<?end>`, the range runs to the end of the parent). The nodes between the markers are removed, and the content of the template is inserted there as live nodes, not as inert template contents. "after head", "in body", "in table", "in column group" and "in template" send a `template` start tag to these rules. RQP reads `<?…>` as a comment (M-03), and M-22 allows comments in block content. So a later `<template for="p">` replaces the content of `<div data-rq-block="a"><?start name="p">old<?end></div>`, and markers around a block element remove it from the DOM, also a `title` block in `head` (Chrome 154; parse5 8.0.1 has no content patching; [edge cases §8.5](/engineering/protocol/markup-edge-cases#_8-5-gap-1-template-content-patching)). Item 3 removes the step: without a `for` attribute, an HTML `template` start tag never patches. The content of a `template` cannot hold a block (M-19) or a manifest script (M-13). An SVG or MathML `template` is a foreign element and does not patch.

**M-11 Noscript.** Let C be the RAWTEXT content of a `noscript` element in HTML context (the bytes from the end of its start tag to the start of its end tag). If the `noscript` element has no end tag, C is the rest of the input: the bytes from the end of its start tag to the end of the input. All items below apply to this C too (E3, H-21). Tokenize the bytes of C followed by `</noscript>` alone, from the data state, with M-03 and M-04 (HTML context). Then:

1. The first end tag `noscript` MUST start exactly at the end of C. (C must not end inside a tag, a comment, a DOCTYPE or a RAWTEXT, RCDATA, script data or PLAINTEXT section.)
2. C MUST NOT contain a start tag with a `data-rq-block` attribute.
3. C MUST NOT contain a start tag named `frameset`, `math`, `noscript`, `plaintext`, `select`, `svg` or `template`.
4. C MUST NOT contain a start tag `script` that M-13 rejects: a first `type` attribute whose raw value contains `rivqen-manifest` (ASCII case-insensitive), contains `&` or contains a byte ≥ 0x80. Error: `RQP_MARKUP_RESERVED` (E8, H-22; H-28).
5. C MUST be balanced (human decision H-27). Use a separate stack for C, which starts empty. A start tag that M-05 item 1 would push opens an element of C. Each end tag in C (before the end tag `noscript` of item 1) MUST have the name of the last open element of C, and closes it. At the end of C, no element of C is open. So C has no crossing, no unclosed element and no stray end tag.
6. A start tag in C with a name in `TABLE_PART` or the name `col` MUST have a `table` element on the stack of C (item 5): a table opened in C and still open (human decision H-38). Invalid: `<noscript><td></td></noscript>`, `<noscript><col></noscript>`, `<noscript><table></table><tr></tr></noscript>`. Valid: `<noscript><table><tr><td>x</td></tr></table></noscript>`.

Items 1 to 3, 5 and 6: error `RQP_MARKUP_STRUCTURE`. Typical tracking fallbacks such as `<noscript><iframe src="…"></iframe></noscript>` and `<noscript><img src="…"></noscript>` are valid. A `noscript` without an end tag whose rest of the input is `<img src="…">` is valid. `<noscript><script type="application/rivqen-manifest+json">…</script></noscript>` is invalid (item 4): a reader with scripting disabled (a crawler, a server-side HTML tool, a native HTML parser) would see a second manifest. `<noscript><button></noscript>` and `<noscript><table></noscript>` are invalid (item 5). `<td><noscript><td></td></noscript>` is invalid (item 6).

The tags in C are not part of the main token pass: they do not change the token stack of M-05 and the depth counter D of M-26. The `noscript` element itself is pushed and counts.

<Badge type="tip" text="FACT" /> Item 5: with scripting disabled, a `noscript` start tag in body is an ordinary element ("any other start tag"), and its end tag is "any other end tag" (WHATWG §13.2.6.4.7). That step walks down from the current node and ignores the token when it meets a special element first. So when C leaves a `button` or a `table` open, the scripting-disabled tree builder ignores `</noscript>`, and the next block is parsed inside the `button` or the `table` (security review; parse5 8.0.1 has no end tag for the block of `<noscript><button></noscript><div data-rq-block="a">x<button>y</button>z</div>` in the scripting-disabled view).

<Badge type="tip" text="FACT" /> Item 6: with scripting disabled, a `noscript` in a cell or a caption is an ordinary element, and the insertion mode stays "in cell" or "in caption". There, "A start tag whose tag name is one of: "caption", "col", "colgroup", "tbody", "td", "tfoot", "th", "thead", "tr"" closes the cell or the caption ("Close the cell … and reprocess the token", WHATWG §13.2.6.4.15; "in caption", §13.2.6.4.11). So `<td><noscript><td></td></noscript><div data-rq-block="x">a<table></table>b</div>` closes the outer cell with the `noscript` in it; the block goes into the new cell and its inner `table` cuts it (security review round 2; parse5 8.0.1 has no end tag for the block in the scripting-disabled view). A `table` opened in C gives the table part its own table context, as in the token stack.

**M-12 CDATA.** For every `<![CDATA[` that the tokenizer reads in the data state, the first `>` after it MUST be the last character of a `]]>` that starts after `<![CDATA[`. Then a CDATA section (ends at the first `]]>`) and a bogus comment (ends at the first `>`) end at the same byte. If there is no `>` after the `<![CDATA[` before the end of the input, the rule holds: a CDATA section and a bogus comment both end at the end of the input (E4, H-21). M-12 applies in HTML context and in a foreign region. In the content of a foreign `INTEGRATION` or `TEXT` element, M-07 item 5 applies too.

**M-13 Reserved manifest.** A start tag `script` (in any context, outside raw text) MUST NOT have a first `type` attribute whose raw value:

1. contains `rivqen-manifest` (ASCII case-insensitive), or
2. contains `&`, or
3. contains a byte ≥ 0x80 (a non-ASCII character).

The server SDK adds the manifest; a document that already has one, or a script type that could decode or fold to it, is not accepted. Error: `RQP_MARKUP_RESERVED`. Items 2 and 3 fail closed, so no character reference decoder and no Unicode case mapping are needed (decided by the human, 2026-10-09 and 2026-10-11; [WP-17 S1](/engineering/plan/sprints/WP-17-S1) H-17, H-28). Item 3 also rejects harmless values such as `text/jävascript`. The only raw text where M-13 also applies is the `noscript` content C, tokenized as in M-11 (M-11 item 4, H-22).

<Badge type="tip" text="FACT" /> Item 3: in Unicode, `ı` (U+0131, dotless i) maps to the upper case `I`, and `ſ` (U+017F, long s) maps to the upper case `S`. A client that compares the type with Unicode case folding would take `application/rıvqen-manifest+json` or `application/rivqen-manifeſt+json` for the manifest type, but item 1 does not match them (security review). <Badge type="info" text="DESIGN" /> The client SDK compares the type of the manifest script by an ASCII case-insensitive byte match only, never by Unicode case folding (H-28). The client rule is in [manifest and patch §1.7](/engineering/protocol/manifest-patch) (client item 1).

### 2.5 Blocks

**M-14 Block start tag.** A **block start tag** is a start tag token that has an attribute with the name `data-rq-block`, and that is not inside raw text. The attribute name match is ASCII case-insensitive (the tokenizer lowercases attribute names). If the start tag has the attribute more than once, only the first one counts (M-03). Text that only looks like a tag is not a block start tag: text in RAWTEXT, RCDATA, script data or PLAINTEXT, comments, bogus comments, and attribute values. An end tag with a `data-rq-block` attribute is not a block start tag. Attributes such as `data-rq-block-x` are not block markers.

The element of a block start tag is the **block element**. Its **block end tag** is defined by M-20. The **content** of the block is the bytes from the end of the start tag (the byte after `>`) to the start of the block end tag (its `<`).

**M-15 Block id.** The raw value of the `data-rq-block` attribute MUST match `^[a-z0-9][a-z0-9_-]{0,63}$`. The value is the raw value, so a character reference is never allowed: `data-rq-block="&#112;rice"` is invalid, although it decodes to `price`. An attribute without a value has the empty value. Error: `RQP_MARKUP_INVALID_ID`.

**M-16 Block element.** The block start tag MUST be in HTML context and its name MUST be one of:

| Name | Format |
|---|---|
| `BLOCK_HTML` | `html` |
| `script`, with a first `type` attribute whose raw value is exactly `application/json` (case-sensitive, no white space, no parameters) | `json` |

Every other element, also every element in a foreign region (for example `<svg><title data-rq-block="x">`), is not a block element. Error: `RQP_MARKUP_FORBIDDEN_ELEMENT`.

**M-17 Nesting.** A block start tag MUST NOT be in the content of another block. Error: `RQP_MARKUP_NESTED`. Check M-17 before M-15 and M-16 for the inner start tag (recommended order, section 2.10).

**M-18 Start tag form.** A block start tag MUST NOT have the self-closing flag (`<div data-rq-block="x"/>`). Error: `RQP_MARKUP_STRUCTURE`.

**M-19 Block context.** When the block start tag is read:

1. The token stack MUST NOT contain a `template` or a `select` element.
2. Let F be as in M-09. F MUST be none, `td`, `th` or `caption`. (A block MUST NOT be a direct part of a table, a row group, a row or a column group. The WHATWG parser would move it out of the table: "foster parenting".)

Error: `RQP_MARKUP_STRUCTURE`.

**M-20 Explicit end tag.** Every block MUST end with an explicit end tag:

- `title` block: the appropriate end tag `</title>` that ends the RCDATA content.
- `json` block: the appropriate end tag `</script>` that ends the script data content.
- Other `html` blocks: the end tag that pops the block element from the token stack while the block element is the current node (M-23).

If the input ends before the block end tag, the document is invalid. A block that the WHATWG parser would close by an implied end tag (for example `<p data-rq-block="a">one<p>two`) is invalid by M-24. Error: `RQP_MARKUP_STRUCTURE`.

**M-21 Unique id.** Two blocks MUST NOT have the same id. Error: `RQP_MARKUP_DUPLICATE`.

### 2.6 Content of `html` blocks

These rules apply to the content of a block whose element is in `BLOCK_HTML` and is not `title` (the content of a `title` block is RCDATA text). Error for all: `RQP_MARKUP_STRUCTURE`.

**M-22 Allowed tokens.** The content MUST NOT contain:

1. a DOCTYPE token;
2. a start tag with a name in `FORBIDDEN_CONTENT`;
3. a start tag with the self-closing flag whose name is not in `VOID` (`<span/>`);
4. an end tag with a name in `VOID` (`</br>`, `</img>`).

Comments, bogus comments, character references and all other elements are allowed. Attributes are not checked: inline event handlers and `javascript:`, `vbscript:` and `data:` URLs are a client patch rule ([manifest and patch §4](/engineering/protocol/manifest-patch#_4-content-rules)), not a markup rule.

**M-23 Balance.** In the content, a start tag with a name not in `VOID` pushes an element (M-05). Every end tag MUST have the name of the current node, and pops only that node. The end tag that has the name of the block element while the block element is the current node is the block end tag. Thus:

- every element opened in the content is closed in the content, explicitly, also elements with optional end tags (`p`, `li`, `td`, …);
- there is no stray end tag, and no end tag closes an element that was opened before the block.

**M-24 No implied end tags.** The WHATWG tree builder closes some elements implicitly when it reads some start tags. In the content, these start tags MUST NOT be used where they could close the block element or an element around it. "Open" means on the token stack, also outside the block, unless the rule says "opened in the content".

| Item | Start tag in the content | Condition for invalid |
|---|---|---|
| a | name in `PCLOSE` | the block element is `p` or `span` |
| b | `h1`–`h6` | the block element is `h1`–`h6`, or an `h1`–`h6` opened in the content is open |
| c | `li`, `dd`, `dt` | no `ul`, `ol`, `menu` or `dl` opened in the content is open |
| d | `button` | a `button` is open |
| e | `a` | an `a` is open |
| f | `nobr` | a `nobr` is open |
| g | name in `TABLE_PART` or `col` | no `table` opened in the content is open (M-09 also applies) |
| h | `rb`, `rtc` | no `ruby` opened in the content is open, or an `rb`, `rp`, `rt` or `rtc` is open |
| i | `rp`, `rt` | no `ruby` opened in the content is open, or an `rb`, `rp` or `rt` is open |
| j | `li`, `dd`, `dt` | an `li`, `dd` or `dt` opened in the content is on the token stack above the nearest `ul`, `ol`, `menu` or `dl` opened in the content |
| k | every name that is not in `TABLE_PART` and is not `col` | F (M-09) is `table`, `tbody`, `thead`, `tfoot`, `tr` or `colgroup` |

"Above" means nearer to the current node. Items j and k were added by the human decision H-18 ([WP-17 S1](/engineering/plan/sprints/WP-17-S1)), after a differential run against the WHATWG tree builder:

- Item j: in `<ul><li><div><li>`, the WHATWG parser closes the first `li` and the `div` in it. Item c alone accepts this, because a list is open in the content. Item j rejects every `li`, `dd` or `dt` start tag that could find an earlier `li`, `dd` or `dt` before it finds the list.
- Item k: in table context (`table`, `tbody`, `thead`, `tfoot`, `tr`, `colgroup`), the WHATWG parser moves an element out of the table (foster parenting), and a `table` start tag closes the open table. Character tokens and comments stay allowed: moved text does not change the content bytes or the block end tag.

`option` and `optgroup` are in `FORBIDDEN_CONTENT` (M-22) for the same reason.

### 2.7 Content of `json` blocks

**M-25 JSON.** The content of a `json` block MUST be a JSON text by RFC 8259 §2:

- white space is only U+0020, U+0009, U+000A and U+000D; no BOM; the content is not empty;
- numbers follow the grammar; their size is not limited (`1e400` is valid);
- duplicate member names are allowed;
- a `\u` escape of a high surrogate MUST be followed by a `\u` escape of a low surrogate; a lone surrogate escape is invalid;
- the nesting depth (arrays and objects open at the same time) MUST NOT be more than 64 (RFC 8259 §9 allows such a limit). The value 64 was decided by the human (2026-10-09; [WP-17 S1](/engineering/plan/sprints/WP-17-S1) H-16).

Error: `RQP_MARKUP_BAD_JSON`. The hash is over the raw content bytes; RQP does not normalize JSON (H-11).

### 2.8 Limits

**M-26 Limits.** A document MUST NOT have more than 256 blocks. The content of one block MUST NOT be larger than 1 MiB (1 048 576 bytes). The depth counter D (below) MUST NOT be larger than 256 at any point of the document: a step that makes D larger than 256 is an error. This applies in HTML context and in a foreign region, inside and outside blocks. Error: `RQP_MARKUP_LIMIT`. Values: [resource limits](/engineering/core/resource-limits) (`maxBlocks`, `maxBlockBytes`, `maxOpenElements`, `maxDocumentBytes`).

**Depth counter D** (human decision H-31, which amends H-25). D is computed in the main token pass, together with the token stack of M-05. Each element on the token stack has a **dirty** mark (initially not set) and a set of **implied parts** (initially empty).

1. D starts at 0.
2. **Push.** Each push to the token stack (M-05 items 1 and 2) adds 1.
3. **Implied parts.** For a start tag in HTML context, let F be as in M-09. For each implied part in the table below that F does not have yet: add 1, and give the part to F. This applies also to `col`, which is not pushed.

   | Start tag | F | Implied parts |
   |---|---|---|
   | `tr` | `table` | row group |
   | `td`, `th` | `table` | row group, row |
   | `td`, `th` | `tbody`, `thead`, `tfoot` | row |
   | `col` | `table` | column group |

4. **Exact pop.** An end tag that pops exactly one element (M-05 step 3: the current node, no crossing), when that element is not dirty and its name is not `html` or `body`, subtracts 1 and the number of implied parts of that element.
5. **Other pops.** An end tag that pops more than one element (a crossing), or pops a dirty element, or pops `html` or `body`, does not change D. After it, the new current node (if there is one) is dirty.
6. An end tag that matches no element does not change D.
7. The tags in the `noscript` content C (M-11) are not part of the main pass and do not change D. The `noscript` element itself is pushed (item 2).

Check D after each push and after each implied part. Every element on the token stack holds at least 1 of D, so the token stack never has more than D elements: D ≤ 256 also bounds the memory and the per-token work of the token stack in every SDK (H-25).

Examples: `<div><span>a</span></div>` gives D = 2, then 0. `<table><tr><td>a</td><td>b</td></tr></table>` gives at most D = 4 (table, row group, `tr`, `td`), then 0. Each `<span><div></span>` adds 2 and never gives them back. Each `<span><span><div></span></span>` adds 3: the first `</span>` crosses the `div` and makes the outer `span` dirty, so the second `</span>` does not subtract.

<Badge type="tip" text="FACT" /> Browsers limit the depth during parsing to about 512. Blink (Chrome, Android WebView) has `kMaximumHTMLParserDOMTreeDepth` = 512; Gecko attaches nodes deeper than stack position 511 to that node. Beyond the limit a new node goes to the element at the maximum depth, so a deep block element is empty in the DOM and its content becomes its sibling. Then the block bytes and the DOM do not agree.

<Badge type="tip" text="FACT" /> The token stack is not a bound of the WHATWG stack. The tree builder ignores some end tags that the token stack uses: "any other end tag" stops at a special element (`</span>` over a `div`), `</li>` needs an `li` in list item scope (`<li><ul></li>`), a heading end tag pops only to the nearest heading (`<h1><span><h2>x</h1>`), `</form>` is ignored when the form element pointer is null or its form is not in scope and else removes only that form, and `</body>` does not pop (WHATWG §13.2.6.4.7). 300 × `<span><div></span>` before a block gives a WHATWG stack of 603 elements in parse5 8.0.1, while the token stack is empty (security review). D is 600 for this input.

<Badge type="info" text="DESIGN" /> **Claim.** For a document that satisfies the other rules of section 2, after each token at which no `template` element is open (template contents are inert and hold no block, M-19; human decision H-35): (a) the WHATWG stack of open elements has at most D + 2 elements; (b) each element on it has a DOM depth (element ancestors + 1) of at most D + 2; (c) each element that the tree builder inserts and does not keep on the stack (a void element, the empty `p` of `</p>`, the `br` of `</br>`) has a DOM depth of at most D + 3. The 2 are the implied `html` and `body` (or `head`). With D ≤ 256, the WHATWG stack has at most 258 elements and no element is deeper than 259, far below 512. The argument for each tree-builder behavior is in [edge cases §6.5](/engineering/protocol/markup-edge-cases#_6-5-m-26-depth-counter-d-h-31). <Badge type="tip" text="FACT" /> The cross-check tool computes D and checks (a) to (c) after each token of parse5 8.0.1 while no `template` element is open (`tools/rqp-markup/crosscheck`, `src/depth.mjs`): 0 violations on the valid golden fixtures and on all sweep documents that the candidates accept ([edge cases §6.6](/engineering/protocol/markup-edge-cases#_6-6-sweeps)).

### 2.9 Result

**M-27 Result.** For a valid document the result is the list of blocks in document order. Each block has `id`, `format`, `start` (offset of the first content byte), `end` (offset of the `<` of the block end tag) and `sha256`. The template and the revisions follow [manifest and patch §1.3](/engineering/protocol/manifest-patch#_1-3-hashes-and-revisions) (normative). The template keeps the start and end tags of every block.

### 2.10 Error codes

| Code | Rules |
|---|---|
| `RQP_MARKUP_LIMIT` | M-01, M-26 |
| `RQP_MARKUP_ENCODING` | M-02 |
| `RQP_MARKUP_INVALID_ID` | M-15 |
| `RQP_MARKUP_FORBIDDEN_ELEMENT` | M-16 |
| `RQP_MARKUP_NESTED` | M-17 |
| `RQP_MARKUP_DUPLICATE` | M-21 |
| `RQP_MARKUP_BAD_JSON` | M-25 |
| `RQP_MARKUP_RESERVED` | M-11 item 4, M-13 |
| `RQP_MARKUP_STRUCTURE` | M-06 to M-12 (M-11 items 1 to 3, 5 and 6), M-18 to M-20, M-22 to M-24 |

`RQP_MARKUP_STRUCTURE` is the one code for all structure failures: no explicit end tag, implied end tag, unbalanced content, forbidden content element, block context, and the document rules.

**Recommended check order.** M-01, M-02, then one pass over the tokens. On each push to the token stack and each implied part: M-26 (depth counter D). On each start tag in HTML context: M-09 (with conditions 1 and 2), M-10. On each end tag: M-06 (items 1 to 4), then the change of D and of the marks (M-06 item 4, M-26). At the end of a `noscript` content C: M-11. For a block start tag: M-17, M-15, M-16, M-18, M-19, M-21, M-26 (count). At the block end: M-26 (size), M-25. A fixture never depends on this order, because a fixture with an error has exactly one error.

### 2.11 Decided edge cases

<Badge type="info" text="DESIGN" /> The rules above decide the eight edge cases of the [edge case study](/engineering/protocol/markup-edge-cases). The human decided E1–E7 (H-21, all as recommended) and E8 (H-22, reject), [WP-17 S1](/engineering/plan/sprints/WP-17-S1). The fixtures `FX-RQ-MARKUP-EDGE-*` are normative ([catalog](/engineering/protocol/fixtures#_4-2-markup-fixtures-fx-rq-markup)).

| Case | Input | Rule | Result |
|---|---|---|---|
| E1 | `<` as a character in a foreign text element or integration point | M-07 items 3, 4 | Valid |
| E2 | End of input in a foreign region | M-07 item 6 | Not an error by itself |
| E3 | `noscript` without an end tag | M-11 | C is the rest of the input; M-11 applies |
| E4 | `<![CDATA[` without `>` before the end of input | M-12 | Valid |
| E5 | CDATA in a foreign text element or integration point | M-07 item 5, M-12 | Valid when M-12 holds |
| E6 | `<?` that reaches the end of input | M-03 | A comment to the end of the input |
| E7 | `frameset` start tag in a foreign region | M-10 | Invalid (`STRUCTURE`) |
| E8 | Manifest script in `noscript` content | M-11 item 4, M-13 | Invalid (`RESERVED`) |

### 2.12 Rules from the security review

<Badge type="info" text="DESIGN" /> The security review of the four parser candidates (2026-10-11) found shapes where the token stack and the WHATWG tree builder disagree. The human decided H-25 to H-31 ([WP-17 S1](/engineering/plan/sprints/WP-17-S1)). Evidence and analysis: [edge cases §6](/engineering/protocol/markup-edge-cases#_6-security-review-h-25-to-h-31). The conformance study of 2026-10-12 (T-17) added H-43 ([edge cases §8](/engineering/protocol/markup-edge-cases#_8-whatwg-conformance-table-round-3)).

| Decision | Shape | Rule | Result |
|---|---|---|---|
| H-26 | `</a>` or `</nobr>` crosses 8 or more `div` elements; a later `<a>` cuts a block | M-06 item 2 | `STRUCTURE` |
| H-27 | `<noscript><button></noscript>`: `</noscript>` is ignored with scripting disabled | M-11 item 5 | `STRUCTURE` |
| H-28 | `application/rıvqen-manifest+json` (dotless i), long s | M-13 item 3 | `RESERVED` |
| H-30 | `<table><td></tbody>`: the cell closes, the block moves before the table | M-06 item 3 | `STRUCTURE` |
| H-32 | `<a><span>`, 8 × `<div>`, `</span></a>`: the tree builder keeps the `div` elements; a later `<a>` cuts a block | M-06 item 4 | `STRUCTURE` |
| H-35 | `<template><col><title>`: the tree builder ignores the `title`, so its raw text is markup | M-09 condition 1 | `STRUCTURE` |
| H-36 | `<form><object><form></form></object></form>`: the outer form stays open | M-10 item 2 | `STRUCTURE` |
| H-37 | `<table><marquee><table>`: the `marquee` is popped without its end tag, its marker stays | M-09 condition 2 | `STRUCTURE` |
| H-38 | `<td><noscript><td></td></noscript>`: with scripting disabled the outer cell closes | M-11 item 6 | `STRUCTURE` |
| H-25, H-31 | 300 × `<span><div></span>`: WHATWG stack of 603 | M-26 (D) | `LIMIT` |
| H-43 | `<template for="p">` after `<?start name="p">`: content patching replaces or removes a block (conformance study T-17) | M-10 item 3 | `STRUCTURE` |

## 3. Implementation notes

<Badge type="info" text="DESIGN" /> Four parser candidates implement section 2 in WP-17: Rust, Node.js, Java and PHP (`tools/rqp-markup/CONTRACT.md` §1). All of them use the same token stack (M-05). A tree is never the source of truth for validity.

<Badge type="tip" text="FACT" /> All four candidates use an own tokenizer over the input bytes and no HTML library at run time (decision log DL-012…DL-016). No candidate has a tree builder.

| Candidate | Tokenizer | Why not a library | Decision |
|---|---|---|---|
| Rust | Own | `lol_html` cannot be driven by the M-04/M-05 foreign-region flag and cannot tokenize `noscript` content again (M-11); `html5gum` raw attribute spans are not documented | DL-013 |
| Node.js | Own | The parse5 8.0.1 tokenizer is quadratic in the attribute count of one tag (below) | DL-016 |
| Java | Own | jsoup `Tokeniser` and `Token` are package-private; jsoup works on UTF-16 and builds `noscript` content as elements | DL-014 |
| PHP | Own | No PHP library gives byte offsets (research R-1) | H-13, DL-015 |

### 3.1 Tokenizer

The tokenizer must implement these WHATWG states exactly:

| States | Why |
|---|---|
| Data, tag open, end tag open, tag name | Find tags and their byte spans |
| Before/after attribute name, attribute name, attribute value (double-quoted, single-quoted, unquoted), after attribute value (quoted), self-closing start tag | `>` inside quoted values; duplicate attributes; raw values (M-15, M-16, M-13); self-closing flag (M-05, M-18, M-22) |
| Markup declaration open, comment start, comment, comment end dash, comment end, comment end bang, and the comment less-than-sign states | Comments, including `<!-->`, `<!--->` and `--!>` |
| Bogus comment; processing instruction states (or bogus comment for `<?`) | `<!x …>`, `</ …>`, `<?…>`: all end at the first `>`, or at the end of the input (M-03, E6) |
| DOCTYPE states | A DOCTYPE ends at the first `>` |
| RCDATA, RAWTEXT, script data with the escaped and double-escaped states, PLAINTEXT, and their end tag states | Raw text (M-04). The script data states decide where `</script>` ends a script that contains `<!--<script>` |
| CDATA section (optional) | M-12 lets the tokenizer treat `<![CDATA[` as a bogus comment. M-07 item 5 then counts that comment as a CDATA section |

Character references need not be decoded: no rule depends on decoded text. The state after a start tag comes from M-04 and the foreign-region flag of the token stack (M-05), not from a tree builder.

All structural characters of the tokenizer are ASCII, and a byte of a UTF-8 multi-byte sequence is never ASCII. So for valid UTF-8 (M-02), a scan over bytes finds the same token boundaries as a scan over code points. Offsets are then UTF-8 byte offsets without a conversion. The input preprocessing (CR and CR LF become LF) does not change a token boundary when the tokenizer treats CR as white space.

### 3.2 Test oracles with a tree builder

A tree builder is used only to test the candidates, never to decide validity.

- <Badge type="tip" text="FACT" /> parse5 8.0.1 (MIT) is a development dependency of the Node.js candidate (tokenizer differential test and tree agreement test) and of the cross-check tool `tools/rqp-markup/crosscheck/` (DL-016). Use scripting enabled: with the option `scriptingEnabled`, the content of `noscript` is text.
- <Badge type="tip" text="FACT" /> The parse5 8.0.1 tokenizer is quadratic in the attribute count of one tag. For each attribute name it searches the earlier attributes of the same tag linearly (`_leaveAttrName` calls `getTokenAttr`, a loop over `token.attrs`, in `dist/common/token.js`). The Node.js lane measured 26 s for one tag of 432 KB (`tools/rqp-markup/node/README.md`). So parse5 is not used to parse untrusted input of up to 5 MiB (M-01).
- <Badge type="tip" text="FACT" /> jsoup (current `master`) inserts the children of `noscript` as elements (`HtmlTreeBuilder.startNoscript`): this is the scripting-disabled view. Source locations of a tree are in UTF-16 units (parse5 `sourceCodeLocation`; jsoup `sourceRange()` and `endSourceRange()`). A tool that compares them with candidate results converts them to UTF-8 byte offsets exactly (non-BMP characters are 2 UTF-16 units and 4 bytes; CR LF stays 2 bytes).
- <Badge type="tip" text="FACT" /> The WHATWG parser changed in 2025–2026: `select` content is parsed in the "in body" mode (customizable `select`), `<?target …>` gives a processing instruction, and a `template` start tag with a `for` attribute runs content patching (§4.12.3, §13.2.6.4.4). Parsers that implement an older version give a different tree for such input. M-08, M-03 and M-10 item 3 make the result the same for valid documents. parse5 8.0.1 implements none of these three changes, so a cross-check with parse5 cannot find a gap that comes from them; Chrome 154 implements all three ([edge cases §8](/engineering/protocol/markup-edge-cases#_8-whatwg-conformance-table-round-3)).
- <Badge type="warning" text="RESEARCH" /> A tree is not a perfect oracle. parse5 8.0.1 gives a comment for CDATA in an integration point (M-07 item 5) and the older `<?` behavior (M-03). A false accept that the oracle shares with all candidates is not found by a cross-check ([edge cases](/engineering/protocol/markup-edge-cases) §4).

### 3.3 Order of work in one pass

1. Check M-01 and M-02 on the bytes.
2. Tokenize (M-03, M-04). Keep the token stack (M-05) and the depth counter D (M-26), and check M-06 to M-13 for each token.
3. At a block start tag, check M-14 to M-19 and M-21, and remember the offset after `>`.
4. In the content, check M-22 to M-24. At the block end tag, record `end`, then check M-26 and M-25.
5. At EOF, check M-20 for an open block. Then compute the result (M-27).

All loops run once per input byte or per token; all memory is bounded by the input size (M-01).

<Badge type="info" text="DESIGN" /> State for the rules of the security review (section 2.12):

| Rule | State | Cost |
|---|---|---|
| M-06 item 2 | The names of the crossed elements and their foreign flag (already on the token stack) | A check of each crossed element: the pops already visit them |
| M-06 item 3 | None (the end tag search of M-05 step 3 already knows "no match") | O(1) |
| M-06 item 4 | A special-crossed flag per token-stack element. Set it on the new current node after the pops of an end tag when a crossed element is special (HTML context) or foreign, or when a popped element has the flag. Check it on every element that a `FORMATTING` end tag pops | The pops already visit the popped elements |
| M-09 condition 1 | None: for `col`, walk down the token stack to the nearest `template` or `TABLE_FAMILY` element (the M-09 search for F already does this walk and stops at `template`) | The walk of M-09 |
| M-09 condition 2 | None: compare F with the current node | O(1) |
| M-10 item 2 | A count of `form` elements pushed in HTML context that are on the token stack (+1 at the push, −1 when such a `form` is popped, also by a crossing) | O(1) |
| M-10 item 3 | None: look for the attribute name `for` in the attribute list of a `template` start tag in HTML context | The attributes are already scanned (M-14) |
| M-11 item 6 | A count of `table` elements on the stack of C | O(1) |
| M-11 item 5 | A second stack of names for C only; an end tag compares with its top | O(1) per tag of C; C is not counted in D, so this stack is bounded by the size of C only |
| M-13 item 3 | None: scan the raw `type` value for a byte ≥ 0x80 | The value is already scanned for `&` |
| M-26 (D) | An integer D; per token-stack element a dirty flag and 3 implied-part flags | O(1) per token, besides the search of M-05 step 3 |

<Badge type="info" text="DESIGN" /> A map keyed by tag names from the input must resist hash flooding: use fixed slots for fixed name sets, and a keyed or randomized hash for arbitrary names. PHP arrays use DJBX33A without a seed (security review, T-10: 131 072 nested names with one hash took 35 s). With D ≤ 256 (M-26), a name index of the token stack has at most 256 keys, which bounds the remaining effect. The stack of M-11 item 5 needs no name index: an end tag in C compares only with the top.

## 4. Why these restrictions

<Badge type="tip" text="FACT" /> Each restriction removes one WHATWG tree-construction behavior that a token-only parser cannot see (HTML §13.2.6):

| Rule | WHATWG behavior that it removes |
|---|---|
| M-04 | The tree builder, not the tokenizer, switches to RCDATA, RAWTEXT, script data and PLAINTEXT. Inside `svg` and `math` it does not switch. |
| M-06 item 1 | An end tag is ignored when a scope boundary (`table`, `td`, `th`, `caption`, `template`, `object`, `applet`, `marquee`, `button` for button scope) is in the way, or when a special element stops the walk. The token stack would pop and disagree with the tree. |
| M-06 item 2 | A formatting end tag runs the adoption agency algorithm; with a furthest block it moves elements and stops after 8 passes, so the formatting element can stay open. |
| M-06 item 3 | A stray `</tbody>` or `</tr>` closes an implied `tbody` or `tr` and the open cell. |
| M-06 item 4 | An end tag that the tree builder ignores at a special element leaves elements open; a later formatting end tag then meets a furthest block. |
| M-09 condition 1 | In a template, "in column group" ignores every later start tag, also raw-text start tags. |
| M-09 condition 2 | "Clear the stack back to a table context" pops foster-parented elements without their end tags; `applet`, `marquee`, `object` leave a marker. |
| M-10 item 2 | The form element pointer: a nested `form` start tag is ignored, and `</form>` can be ignored. |
| M-10 item 3 | A `template` start tag with a `for` attribute runs content patching: it removes the nodes between `<?start>` and `<?end>` and inserts its content there. |
| M-11 item 6 | With scripting disabled, a table part in C closes the cell or caption around the `noscript`. |
| M-11 item 5 | With scripting disabled, `</noscript>` is ignored when C left a special element open. |
| M-13 item 3 | Unicode case folding on a client maps non-ASCII look-alikes to the manifest type. |
| M-26 (D) | Ignored end tags, implied table parts and reconstructed formatting elements make the tree deeper than the token stack; browsers flatten the tree beyond depth 512. |
| M-07 | `BREAKOUT` start tags pop out of `svg`/`math`. Integration points parse their children as HTML. |
| M-08 | `select` parsing differs between parser versions (customizable `select`). |
| M-09, M-19 | Table parts close open cells and rows implicitly. Content directly in a table is moved before the table (foster parenting). |
| M-10 | After `frameset`, the tree builder ignores most tags. |
| M-11 | With scripting disabled, `noscript` content is markup, not text. |
| M-12 | `<![CDATA[` is a CDATA section only in foreign content. |
| M-20, M-23 | An element closed by an implied end tag or at EOF has no end tag position; parsers report different ends (research R-1). |
| M-24 a | "Close a `p` element": a start tag in `PCLOSE` pops the open `p` and every element above it. |
| M-24 b | A heading start tag pops a current heading. A heading end tag pops to any open heading. |
| M-24 c, j | `li`, `dd`, `dt` pop an earlier `li`, `dd`, `dt` up to the first special element other than `address`, `div`, `p`. |
| M-24 k | In table context, a start tag other than a table part is foster-parented, and a `table` start tag closes the open `table`. |
| M-24 d, e, f | `button`, `a` and `nobr` close an open element of the same name (`a` and `nobr` with the adoption agency algorithm). |
| M-24 h, i | Ruby start tags generate implied end tags when a `ruby` is in scope. |
| M-22 (`option`, `optgroup`) | `option` and `optgroup` pop a current `option`. |

## 5. Known limits

<Badge type="info" text="DESIGN" />

1. **Reconstructed formatting.** When a formatting element (`b`, `i`, `a`, …) was closed implicitly before a block (for example `<p><b>x<div data-rq-block="d">y</div>`), the WHATWG parser re-opens it inside the block element when it reads text. The DOM of the block then contains an extra `b` that is not in the content bytes. The content bytes and the offsets are still the same in all parsers. When a patch replaces the content, the extra element is not created again. This is a rendering difference, not a parser ambiguity. Follow-up: client patch rules (S2).
2. **Scripting disabled.** RQP uses the scripting-enabled view (M-04). M-11 aims to make the block result the same in both views (see the open gap at the end of this item). Why: with scripting disabled, C is markup. Items 2 and 3 keep blocks and the tags that change the parser mode (`frameset`, `math`, `noscript`, `plaintext`, `select`, `svg`, `template`) out of C. Item 1 makes C end in the data state. Item 5 (H-27) makes every element that C opens close by its own end tag inside C, so the tree builder has closed them before `</noscript>`, and `</noscript>` closes the `noscript` element. Item 6 (H-38) keeps table parts in C inside a table of C, so they cannot close a cell or caption around the `noscript`. Effects of C stay outside every block: a block start tag is never in C (item 2), and C is never in the content of a block (`noscript` is in `FORBIDDEN_CONTENT`). C can still close elements around it earlier (a `div` in C closes an open `p`), and with scripting disabled the "in head noscript" mode leaves the head at the first tag of C other than `link`, `meta`, `style`, `noframes`, `basefont` and `bgsound`. That changes the parent of later elements, not the bytes of a later block. <Badge type="tip" text="FACT" /> Before H-27, the scripting-disabled view of `<noscript><button></noscript><div data-rq-block="a">x<button>y</button>z</div>` had no end tag for the block in parse5 8.0.1, so item 2 of this list was false. The sweeps of edge cases §6 found no block disagreement in the scripting-disabled view that M-11 item 5 does not explain. <Badge type="warning" text="RESEARCH" /> **Open (H-42).** The agreement with the scripting-disabled view is not complete yet. The rules of the main pass (M-09 conditions 1 and 2, M-10 item 2) do not apply to the tags of C, so their shapes come back inside C in that view, for example `<object><a href="1"><noscript><table><marquee><table></table></marquee></table></noscript>x</a></object>` before a `span` block (security review, T-16). The scripting-enabled view (M-04, DL-009), which the RQP client uses, is not affected. Sprint S2 studies the content model of `noscript` as a whole.
3. **Strictness.** The rules reject some documents that browsers render well (for example unclosed `td` before a block in the same table, or an `svg` with HTML in `foreignObject`). M-06 item 4 (H-32) also marks crossings over foreign elements, so a common icon shape with an unclosed SVG child, `<a href="#"><svg><path d="M0"></svg></a>`, is invalid; `<path/>` or `<path></path>` is valid. The tree builder pops such foreign elements at `</svg>`, so a later relaxation for foreign crossings that end at the region's own end tag is possible after a parse5 study. Such pages fall back to the normal response. Relaxing a rule later is compatible; making a rule stricter is not. <Badge type="tip" text="FACT" /> On a corpus of 867 real public pages (measured 2026-10-12 with all rules up to H-43), 786 (90.7 %) are valid without blocks; without pages over 5 MiB and pages that are not UTF-8, 92.4 %. The rules H-35…H-38 and H-43 alone reject 3 pages. The rules of the security review (H-26…H-32) alone reject 1 page. The largest costs are `svg` in `noscript` (M-11 item 3, 14 pages alone) and an unclosed `a` crossed by a block end tag (M-06 item 1, 12 pages alone). With simulated blocks, 57 % of the block variants are valid; inline `svg` in block content alone rejects 26 %. Details: `evidence/markup-corpus/summary.md`. Sprint S2 studies relaxations (H-40).

## 6. Why attributes instead of comments

| Problem with legacy comment markers (found in the [audit](/engineering/protocol/legacy-divergences)) | RQP answer |
|---|---|
| Five implementations matched comments differently (title rules, empty names, case) | One grammar, one token-level algorithm, conformance fixtures |
| Markers with spaces silently did not match | Attributes are parsed by the HTML tokenizer; spaces do not matter |
| Nested markers produced broken templates | Nesting is invalid and rejected (M-17) |
| Duplicate names: last value won silently | Duplicates are invalid and reported (M-21) |
| Implicit `<title>` handling corrupted pages with several titles (for example `<svg><title>`) | No implicit rules; the title is a normal block; `svg` titles are never blocks (M-16) |
| Comments are removed by many HTML minifiers | `data-*` attributes survive minifiers |

## 7. Server helpers

Template authors do not write the manifest. Server SDKs compute it:

| SDK | Helper |
|---|---|
| Node.js | `renderWithRivqen()` reads `data-rq-block` from the rendered HTML; React `<RivqenBlock id="price">` emits the attribute |
| Java | `RivqenFilter` + optional Thymeleaf/JSP attribute helpers |
| PHP | `RivqenMiddleware` + optional Twig/Blade helpers |

## 8. Verification

- Golden fixtures `FX-RQ-MARKUP-*` in `fixtures/rqp/markup/`: the [catalog](/engineering/protocol/fixtures#_4-2-markup-fixtures-fx-rq-markup) maps each fixture to its rules. `tools/rqp-markup/gen.py` computes each `expected.json` from the bytes that a person wrote in `fixture.json`; it does not parse HTML.
- Differential test: the four parser candidates must give the same `expected.json` result for every fixture (`tools/rqp-markup/CONTRACT.md` §6).
- Cross-check against the WHATWG tree builder: `tools/rqp-markup/crosscheck/` compares the blocks of each valid candidate result with the parse5 tree (scripting enabled, optionally also disabled), on the fixtures and on a seeded generator sweep. Its `README.md` has the commands. <Badge type="tip" text="FACT" /> On 2026-10-09, every valid `expected.json` agreed with parse5 8.0.1 in both views, and every block-byte disagreement that the sweeps of that day found was a shape that M-24 items j or k reject ([edge cases §2](/engineering/protocol/markup-edge-cases#_2-method-and-evidence)). That generator did not reach all shapes: the security review of 2026-10-11 found more disagreements (deep crossings by formatting end tags, stray table end tags, unbalanced `noscript` content, non-ASCII manifest types, a tree deeper than the token stack). The rules of section 2.12 reject them. The generator profile `review` reaches these shapes; the cross-check tool also computes the depth counter D and checks the claim of M-26 ([edge cases §6](/engineering/protocol/markup-edge-cases#_6-security-review-h-25-to-h-31)).

To regenerate or check the expected results:

1. Run `python3 tools/rqp-markup/gen.py` to write `expected.json` files.
2. Run `python3 tools/rqp-markup/gen.py --check` to make sure that no `expected.json` is stale.

## Related

- [RQP negotiation](/engineering/protocol/negotiation)
- [RQP manifest and patch](/engineering/protocol/manifest-patch)
- [Golden fixtures](/engineering/protocol/fixtures)
- [Legacy markers grammar](/engineering/protocol/legacy-markers) (deprecated)
