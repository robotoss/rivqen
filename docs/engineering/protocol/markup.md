# RQP markup

This page specifies how a page marks its data blocks in the Rivqen protocol (RQP), when a document is valid for RQP, and which bytes are the content of each block. The markup is valid HTML. It replaces the legacy comment markers.

**Status:** <Badge type="info" text="DESIGN" /> Normative for `rqp/1` from WP-17 S1. Decisions: attribute markup and inline manifest ([ADR-006](/engineering/architecture/adr/#adr-006)); a block without an explicit end tag is invalid (human decision H-12, [sprint record WP-17 S1](/engineering/plan/sprints/WP-17-S1)). Parser interface, hashes and revisions: `tools/rqp-markup/CONTRACT.md`. Golden fixtures: [`FX-RQ-MARKUP-*`](/engineering/protocol/fixtures#_4-2-markup-fixtures-fx-rq-markup).

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

<Badge type="tip" text="FACT" /> `PCLOSE`, `BREAKOUT` and the void handling of `VOID` come from the WHATWG HTML Living Standard, §13.2.6.4.7 "The rules for parsing tokens in body" and §13.2.6.5 "The rules for parsing tokens in foreign content" (checked against the `whatwg/html` source on 2026-10-09). `INTEGRATION` is the union of the SVG HTML integration points (`foreignObject`, `desc`, `title`) and the MathML text integration points and `annotation-xml`.

### 2.2 Input

**M-01 Input size.** The input MUST NOT be larger than 5 MiB (5 242 880 bytes). Check this first, before any other rule. Error: `RQP_MARKUP_LIMIT`.

**M-02 Encoding.** The input MUST be valid UTF-8 (RFC 3629: no overlong forms, no surrogate code points, no code point above U+10FFFF, no truncated sequence). A UTF-8 BOM (`EF BB BF`) at offset 0 is allowed. The tokenizer does not see it, but it stays in the offsets and in the template (contract §4). U+FEFF at any other offset is a normal character. U+0000 and other control characters are valid characters. Error: `RQP_MARKUP_ENCODING`.

The parser decodes the input as UTF-8 always. It ignores `<meta charset>` and the `http-equiv` charset declaration. <Badge type="info" text="DESIGN" /> The RQP server sends `Content-Type: text/html; charset=utf-8`; a transport-layer charset has priority over `<meta>` in the WHATWG encoding sniffing algorithm, so the browser also decodes UTF-8 (negotiation text: S2).

### 2.3 Tokenization and token stack

**M-03 Tokenizer.** The parser tokenizes the input with the WHATWG tokenizer (HTML §13.2.5), from the data state, after the input stream preprocessing (CR and CR LF become LF). Preprocessing does not change offsets: offsets always refer to the input bytes. The tree builder is **not** used. The tokenizer state changes that the tree builder would make are defined by M-04 only.

- A start tag ends at its `>`. A `>` inside a quoted attribute value does not end the tag.
- Comments follow the WHATWG comment states, including `<!-->`, `<!--->` and the end `--!>`.
- `<!` followed by anything other than `--`, `DOCTYPE` (ASCII case-insensitive) or `[CDATA[` starts a bogus comment. `</` followed by a character that is not an ASCII letter and not `>` starts a bogus comment. A bogus comment ends at the first `>`.
- `<?` starts a bogus comment in older parsers and a processing instruction in newer ones. <Badge type="tip" text="FACT" /> Both end at the first `>` (WHATWG "processing instruction data state" and "bogus comment state"). RQP treats them the same.
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

The token stack is not the DOM. It does not do implied end tags, foster parenting or the adoption agency algorithm. The rules M-06 to M-24 make sure that, for a valid document, the differences do not change which elements are blocks or where their content ends.

### 2.4 Document rules

These rules apply to the whole document, also when it has no block. They remove the cases where the token stack and the WHATWG tree disagree about an element that matters for blocks. Error for all of them, except M-13: `RQP_MARKUP_STRUCTURE`.

**M-06 Crossing.** An end tag MUST NOT cross (M-05, step 3) an element that was pushed in HTML context and whose name is in `GUARDED`. (Elements pushed in a foreign region, for example an SVG `a`, are not guarded; M-07 applies to them.) Exception: an end tag with a name in `TABLE_FAMILY` can cross elements with a name in `TABLE_PART`.

- Allowed: `<div><span>a</div>` (crosses `span`), `<ul><li>a</ul>` (crosses `li`), `<tr><td>a</tr>` (crosses `td`).
- Invalid: `<div><button>a</div>` (crosses `button`), `<td><a href="#">a</td>` (crosses `a`).

**M-07 Foreign regions.** In a foreign region:

1. A start tag MUST NOT have a name in `BREAKOUT`.
2. An end tag MUST match the `svg` or `math` element of the region or an element pushed inside the region. It can cross other elements of the region. A stray end tag (no match) is not allowed. A region ends only with the end tag of its own `svg` or `math` element.
3. After the start tag of an element with a name in `INTEGRATION` (without the self-closing flag), the tokens up to the end tag of that element MUST be character tokens only. Thus, the content contains no `<`: no tags, no comments, no CDATA.
4. A start tag with a name in `TEXT` MUST NOT have the self-closing flag. Its content (up to its end tag) MUST be character tokens only. The name `plaintext` is not allowed.

**M-08 Select.** Inside a `select` element (HTML context, from its start tag to its end tag), the only allowed tokens are characters, comments, the start tags `option`, `optgroup`, `hr`, and the end tags `option`, `optgroup`, `select`. The `select` element MUST have an explicit end tag `</select>`.

**M-09 Table parts.** Let F be the nearest element on the token stack with a name in `TABLE_FAMILY`; the search stops at a `template` element. A start tag in HTML context with one of these names is allowed only when F is none or is in the list:

| Start tag | Allowed F |
|---|---|
| `caption`, `colgroup`, `tbody`, `thead`, `tfoot` | `table` |
| `col` | `table`, `colgroup` |
| `tr` | `table`, `tbody`, `thead`, `tfoot` |
| `td`, `th` | `table`, `tbody`, `thead`, `tfoot`, `tr` |

So a cell, a row or a row group must be closed explicitly before the next one starts: `<td>a</td><td>b</td>` is valid, `<td>a<td>b` is not.

**M-10 Frameset.** The document MUST NOT contain a `frameset` start tag.

**M-11 Noscript.** Let C be the RAWTEXT content of a `noscript` element in HTML context (the bytes from the end of its start tag to the start of its end tag). Tokenize the bytes of C followed by `</noscript>` alone, from the data state, with M-03 and M-04 (HTML context). Then:

1. The first end tag `noscript` MUST start exactly at the end of C. (C must not end inside a tag, a comment, a DOCTYPE or a RAWTEXT, RCDATA, script data or PLAINTEXT section.)
2. C MUST NOT contain a start tag with a `data-rq-block` attribute.
3. C MUST NOT contain a start tag named `frameset`, `math`, `noscript`, `plaintext`, `select`, `svg` or `template`.

Typical tracking fallbacks such as `<noscript><iframe src="…"></iframe></noscript>` and `<noscript><img src="…"></noscript>` are valid.

**M-12 CDATA.** For every `<![CDATA[` that the tokenizer reads in the data state, the first `>` after it MUST be the last character of a `]]>` that starts after `<![CDATA[`. Then a CDATA section (ends at the first `]]>`) and a bogus comment (ends at the first `>`) end at the same byte.

**M-13 Reserved manifest.** A start tag `script` (in any context, outside raw text) MUST NOT have a first `type` attribute whose raw value contains `rivqen-manifest` (ASCII case-insensitive) or contains `&`. The server SDK adds the manifest; a document that already has one, or a script type that could decode to it, is not accepted. Error: `RQP_MARKUP_RESERVED`. The `&` part fails closed, so no character reference decoder is needed (decided by the human, 2026-10-09; [WP-17 S1](/engineering/plan/sprints/WP-17-S1) H-17).

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

Comments, bogus comments, character references and all other elements are allowed. Attributes are not checked: event handler attributes and `javascript:` URLs are a client patch rule ([manifest and patch §4](/engineering/protocol/manifest-patch#_4-content-rules)), not a markup rule.

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

**M-26 Block limits.** A document MUST NOT have more than 256 blocks. The content of one block MUST NOT be larger than 1 MiB (1 048 576 bytes). Error: `RQP_MARKUP_LIMIT`. Values: [resource limits](/engineering/core/resource-limits) (`maxBlocks`, `maxBlockBytes`, `maxDocumentBytes`).

### 2.9 Result

**M-27 Result.** For a valid document the result is the list of blocks in document order. Each block has `id`, `format`, `start` (offset of the first content byte), `end` (offset of the `<` of the block end tag) and `sha256`. The template and the revisions follow `tools/rqp-markup/CONTRACT.md` §4. The template keeps the start and end tags of every block.

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
| `RQP_MARKUP_RESERVED` | M-13 |
| `RQP_MARKUP_STRUCTURE` | M-06 to M-12, M-18 to M-20, M-22 to M-24 |

`RQP_MARKUP_STRUCTURE` is the one code for all structure failures: no explicit end tag, implied end tag, unbalanced content, forbidden content element, block context, and the document rules.

**Recommended check order.** M-01, M-02, then one pass over the tokens. For a block start tag: M-17, M-15, M-16, M-18, M-19, M-21, M-26 (count). At the block end: M-26 (size), M-25. A fixture never depends on this order, because a fixture with an error has exactly one error.

## 3. Implementation notes

<Badge type="info" text="DESIGN" /> Four parser candidates implement section 2 in WP-17: Rust, Node.js, Java and PHP (`tools/rqp-markup/CONTRACT.md` §1). All of them use the same token stack (M-05). A tree is never the source of truth for validity.

### 3.1 Parsers with a tokenizer only (Rust, PHP)

The tokenizer must implement these WHATWG states exactly:

| States | Why |
|---|---|
| Data, tag open, end tag open, tag name | Find tags and their byte spans |
| Before/after attribute name, attribute name, attribute value (double-quoted, single-quoted, unquoted), after attribute value (quoted), self-closing start tag | `>` inside quoted values; duplicate attributes; raw values (M-15, M-16, M-13); self-closing flag (M-05, M-18, M-22) |
| Markup declaration open, comment start, comment, comment end dash, comment end, comment end bang, and the comment less-than-sign states | Comments, including `<!-->`, `<!--->` and `--!>` |
| Bogus comment; processing instruction states (or bogus comment for `<?`) | `<!x …>`, `</ …>`, `<?…>`: all end at the first `>` |
| DOCTYPE states | A DOCTYPE ends at the first `>` |
| RCDATA, RAWTEXT, script data with the escaped and double-escaped states, PLAINTEXT, and their end tag states | Raw text (M-04). The script data states decide where `</script>` ends a script that contains `<!--<script>` |
| CDATA section (optional) | M-12 lets the tokenizer treat `<![CDATA[` as a bogus comment |

Character references need not be decoded: no rule depends on decoded text. The state after a start tag comes from M-04 and the foreign-region flag of the token stack (M-05), not from a tree builder.

<Badge type="warning" text="RESEARCH" /> `lol_html` gives byte spans of tags (research R-1). Whether its own text-type logic agrees with M-04 in every case (foreign regions, `noscript`, `plaintext`) is NOT FOUND. The Rust lane checks this with the fixtures; where it differs, the candidate must follow M-04.

### 3.2 Parsers with a tree builder (Node.js, Java)

- Get start and end offsets from source locations (parse5 `sourceCodeLocation`; jsoup `sourceRange()` and `endSourceRange()`), and convert UTF-16 offsets to UTF-8 byte offsets exactly (non-BMP characters are 2 UTF-16 units and 4 bytes; CR LF stays 2 bytes).
- The rules need tokens that the tree does not keep: stray end tags, start tags that the tree builder ignores, raw attribute values, self-closing flags. Use the library tokenizer if it is public, or the parse error positions and the source text. A candidate must not decide validity from the tree alone.
- Use scripting enabled. <Badge type="tip" text="FACT" /> parse5 has the option `scriptingEnabled` (content of `noscript` is text when it is true). jsoup (current `master`) inserts the children of `noscript` as elements (`HtmlTreeBuilder.startNoscript`); a jsoup candidate must treat the content of `noscript` as text and rely on M-11.
- <Badge type="tip" text="FACT" /> The WHATWG parser changed in 2025–2026: `select` content is parsed in the "in body" mode (customizable `select`) and `<?target …>` gives a processing instruction. Parsers that implement an older version give a different tree for such input. M-08 and M-03 make the result the same for valid documents.

### 3.3 Order of work in one pass

1. Check M-01 and M-02 on the bytes.
2. Tokenize (M-03, M-04). Keep the token stack (M-05) and check M-06 to M-13 for each token.
3. At a block start tag, check M-14 to M-19 and M-21, and remember the offset after `>`.
4. In the content, check M-22 to M-24. At the block end tag, record `end`, then check M-26 and M-25.
5. At EOF, check M-20 for an open block. Then compute the result (M-27).

All loops run once per input byte or per token; all memory is bounded by the input size (M-01).

## 4. Why these restrictions

<Badge type="tip" text="FACT" /> Each restriction removes one WHATWG tree-construction behavior that a token-only parser cannot see (HTML §13.2.6):

| Rule | WHATWG behavior that it removes |
|---|---|
| M-04 | The tree builder, not the tokenizer, switches to RCDATA, RAWTEXT, script data and PLAINTEXT. Inside `svg` and `math` it does not switch. |
| M-06 | An end tag is ignored when a scope boundary (`table`, `td`, `th`, `caption`, `template`, `object`, `applet`, `marquee`, `button` for button scope) is in the way, or when a special element stops the walk. The token stack would pop and disagree with the tree. |
| M-07 | `BREAKOUT` start tags pop out of `svg`/`math`. Integration points parse their children as HTML. |
| M-08 | `select` parsing differs between parser versions (customizable `select`). |
| M-09, M-19 | Table parts close open cells and rows implicitly. Content directly in a table is moved before the table (foster parenting). |
| M-10 | After `frameset`, the tree builder ignores most tags. |
| M-11 | With scripting disabled, `noscript` content is markup, not text. |
| M-12 | `<![CDATA[` is a CDATA section only in foreign content. |
| M-20, M-23 | An element closed by an implied end tag or at EOF has no end tag position; parsers report different ends (research R-1). |
| M-24 a | "Close a `p` element": a start tag in `PCLOSE` pops the open `p` and every element above it. |
| M-24 b | A heading start tag pops a current heading. A heading end tag pops to any open heading. |
| M-24 c | `li`, `dd`, `dt` pop an earlier `li`, `dd`, `dt` up to the first special element other than `address`, `div`, `p`. |
| M-24 d, e, f | `button`, `a` and `nobr` close an open element of the same name (`a` and `nobr` with the adoption agency algorithm). |
| M-24 h, i | Ruby start tags generate implied end tags when a `ruby` is in scope. |
| M-22 (`option`, `optgroup`) | `option` and `optgroup` pop a current `option`. |

## 5. Known limits

<Badge type="info" text="DESIGN" />

1. **Reconstructed formatting.** When a formatting element (`b`, `i`, `a`, …) was closed implicitly before a block (for example `<p><b>x<div data-rq-block="d">y</div>`), the WHATWG parser re-opens it inside the block element when it reads text. The DOM of the block then contains an extra `b` that is not in the content bytes. The content bytes and the offsets are still the same in all parsers. When a patch replaces the content, the extra element is not created again. This is a rendering difference, not a parser ambiguity. Follow-up: client patch rules (S2).
2. **Scripting disabled.** RQP uses the scripting-enabled view (M-04). M-11 makes the block result the same in both views.
3. **Strictness.** The rules reject some documents that browsers render well (for example unclosed `td` before a block in the same table, or an `svg` with HTML in `foreignObject`). Such pages fall back to the normal response. Relaxing a rule later is compatible; making a rule stricter is not.

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

To regenerate or check the expected results:

1. Run `python3 tools/rqp-markup/gen.py` to write `expected.json` files.
2. Run `python3 tools/rqp-markup/gen.py --check` to make sure that no `expected.json` is stale.

## Related

- [RQP negotiation](/engineering/protocol/negotiation)
- [RQP manifest and patch](/engineering/protocol/manifest-patch)
- [Golden fixtures](/engineering/protocol/fixtures)
- [Legacy markers grammar](/engineering/protocol/legacy-markers) (deprecated)
