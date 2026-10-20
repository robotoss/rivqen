# H-42 study: `noscript` content model and the head rule with scripting disabled

<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- SPDX-FileCopyrightText: The Rivqen Authors -->

Evidence for WP-17 S3, task T-02 (open item H-42). Measured on 2026-10-18. Scripts: `tools/rqp-markup/study/noscript/` (its `README.md` has the commands). Machine results: `sweep-*.json`, `corpus.json` and `corpus-pages.jsonl` in this directory.

**Marks.** FACT: measured or quoted, with the date. DESIGN: a proposal or an argument. RESEARCH: not checked. GATE: a decision of the human. This is a study: nothing in `markup.md`, `manifest-patch.md`, the candidates, the fixtures, the reference or the demo is changed.

**Decisions (added by the architect, 2026-10-18).** The human chose option A1 for the body (H-71) and option HC for the head (H-72). See the sprint record `docs/engineering/plan/sprints/WP-17-S3.md` §1.

## 1. Question

With scripting disabled, the HTML parser reads the `noscript` content C as markup. Readers in that view are crawlers, server-side HTML tools and native HTML parsers. The RQP client and the WebView use the scripting-enabled view (markup M-04), so they are not affected. Two parts of the RQP result can differ between the views:

1. **Body (H-42, security review T-16 Java, MEDIUM).** The rules of the main pass (M-09 conditions 1 and 2, M-10 item 2) do not apply to the tags of C. Their shapes come back inside C in the scripting-disabled view. Example (`markup.md` §5 item 2): `<object><a href="1"><noscript><table><marquee><table></table></marquee></table></noscript>x</a></object>` before a `span` block. parse5 then has no end tag for the block.
2. **Head (H-42 head, S2 code review round 2).** With scripting disabled, a `noscript` start tag in the head switches the parser to "in head noscript". A token in C that this mode does not keep closes the `noscript` element and the head. The head walk of `manifest-patch.md` §1.7 (DL-031, DL-034) does not model that mode. So the manifest that the server inserts before `</head>` is in the body in that view.

Goal of the brief: a rule set for C (and for `noscript` in the head) that gives the same blocks, the same block offsets and the same manifest position in both views.

## 2. Method

### 2.1 Variants

All variants are flags in study copies of the shipped code:

- `src/markup-study.mjs` is a copy of the Node.js candidate `tools/rqp-markup/node/src/markup.mjs`.
- `src/head-study.mjs` is a copy of `documentHeadEnd` of `tools/rqp-protocol/src/markup.mjs`.

With the flags off, the copies give the shipped results (§2.4).

| Variant | Body rule for C |
|---|---|
| `shipped` | M-11 items 1 to 6 as shipped |
| `a` | Model (a): items 1 to 6, plus item 7 (no `html`, `body` or `head` tag in C), plus item 8: the main-pass document rules M-05 … M-10, M-12, M-13 and the M-26 limit on D run on the tokens of C and the end tag `noscript`, on a copy of the token stack at the `noscript` start tag |
| `b0` | Model (b): items 1 to 6, and every start tag in C has a name in the list of the brief: `a`, `div`, `iframe`, `img`, `link`, `meta`, `p`, `span`, `style` |
| `b1` | Model (b) with a longer list: b0 and `b`, `br`, `em`, `i`, `strong`, `small`, `u`, `font`, `s`, `picture`, `source`, `video`, `audio`, `label`, `center`, `h1`–`h6`, `ul`, `li`, `ol`, `section`, `article`, `nav`, `header`, `footer`, `hr`, `input` |
| `a-fx` | Model (a), and item 3 allows `svg` and `math`; the item 5 stack follows M-05 in a foreign region of C. Input for H-40 relaxation 2 (lane T-01); not a T-02 option |

| Variant | Head rule |
|---|---|
| `shipped` | The walk as shipped: a `noscript` in the head is head content, and C is not checked |
| `h0` | A `noscript` start tag while the document head is the current node means: no head end tag |
| `h1` | C (tokenized as in M-11) has only gaps of white space, comments, DOCTYPE tokens, the start tags `basefont`, `bgsound`, `link`, `meta`, `noframes`, `style` (with their raw text) and ignored end tags (DL-034). Else: no head end tag |
| `h2` | As h1, and also `base`, `script` and `title` (the head content of item 2 without `noscript` and `template`) |
| `hc` | A protocol change, for comparison: the manifest element goes immediately after the head start tag that item 1 accepts. No head end tag is needed |

### 2.2 Oracle and agreement

The oracle is parse5 8.0.1 (`tools/rqp-markup/crosscheck`), with option `scriptingEnabled` true and false.

- **Blocks.** For each variant that accepts a document, `compareWithView` of `crosscheck/src/compare.mjs` must hold: the same block elements in the HTML namespace, each with an explicit end tag, the same content bytes, and no manifest-like script.
- **Head (walk rules).** For each rule that finds a head end tag at offset P, in each view: the end tag of the document head element starts at P, and a manifest element inserted at P is a child of the head, which then ends right after it.
- **Head (hc).** The manifest element inserted after the head start tag is a child of the head in each view.
- A variant that rejects a document is not compared, because rejection is the safe fallback (H-12). Its cost is counted as pages lost.

### 2.3 Inputs

| Input | Documents | Notes |
|---|--:|---|
| Golden markup fixtures | 197 | `fixtures/rqp/markup/*/input.html` |
| Cross-check cases | 162 | `crosscheck/cases/*.json`: edge cases, security review rounds 1 and 2, conformance |
| Profile `body`, seed 42 | 2 000 000 | `src/generator.mjs`. Up to 3 contexts: `p`, cells, captions, the `table`/`tbody`/`tr`/`colgroup` modes, formatting elements, `object`, `applet`, `marquee`, `button`, `form`, lists, `template`, `select`, `svg`, `math`. Then a `noscript` with generated C: pairs, void and raw text elements, the H-26/H-36/H-37 motifs, foreign motifs and some unbalanced pieces. Then blocks inside or after the contexts |
| Profile `head`, seed 42 | 500 000 | A head with a `noscript`. C comes from head content, text, body tags, document tags, comments and stray end tags |
| Profile `review2`, seed 42 | 300 000 | The cross-check generator of S1 round 2 |
| Corpus | 882 pages | The S3 fetch (session scratchpad, deleted at sprint close). Both the pages and the simulated blocks of the S1 method (`corpus/classify.mjs`): variant `small` 742 documents, variant `main` 480 |

### 2.4 Self-checks

- `test/equality.test.mjs`: with the flags off, the study analysis equals the Node.js candidate on all 197 golden fixtures. The study head walk equals the reference and the demo head walk on the 197 golden inputs and on 102 protocol documents (20 pages, 79 response bodies, 3 spec examples). Every sweep and the corpus run also compare the study analysis with the candidate: 0 differences (`candidateMismatch` in each JSON file).
- Model (a) compares the state after the C pass with the main state after the end tag `noscript` (token stack names, marks, D): 0 differences in all runs (`modelADivergences`).
- `node tools/rqp-markup/diff.mjs`: 788/788 OK (the four candidates are unchanged).

### 2.5 Limits

- parse5 is one implementation of the WHATWG parser. This study did not run Chrome with scripting disabled. A disagreement that parse5 shares with a variant is not found.
- A sweep without findings is evidence, not a proof. The generators reach only the shapes they contain.
- Only the Node.js candidate is copied. The four candidates agree on the golden fixtures.
- The corpus is the hand-made URL list of S1 (popular sites), fetched again on 2026-10-18. It has 882 pages, not the 867 pages of S1.

## 3. Results

### 3.1 Known shapes

<Badge type="tip" text="FACT" /> parse5 8.0.1, 2026-10-18. A cell is either `valid/<status in the scripting-disabled view>` or `invalid <rule label>`. Where the shape has no block, the block is `<span data-rq-block="k">y<a href="2">z</a>w</span>`.

| Shape | shipped | a | b0 | b1 |
|---|---|---|---|---|
| T-16 (H-37 in C), `markup.md` §5 item 2 | valid/**disagree** | invalid A:M-09.2 | invalid B:table | invalid B:table |
| H-37: `object` in a table in C, in `<a>` | valid/agree | invalid A:M-09.2 | invalid B:table | invalid B:table |
| H-36: 9 × `<form><object><form></form></object></form>` in C, in `<a>` | valid/**disagree** | invalid A:M-10.2 | invalid B:form | invalid B:form |
| H-36: `<form><noscript><form>x</form></noscript></form>` | valid/agree | invalid A:M-10.2 | invalid B:form | invalid B:form |
| H-27: `<noscript><button></noscript>` before a block | invalid M-11.5 | same | same | same |
| H-38: `<td><noscript><td></td></noscript>` before a block | invalid M-11.6 | same | same | same |
| H-35: `template`, `col`, `title` in C | invalid M-11.3 | same | same | same |
| E8: manifest script in C | invalid M-11.4 | same | same | same |
| Pixel `<img>` or GTM-style `<iframe>` in C | valid/agree | valid/agree | valid/agree | valid/agree |
| `<p><noscript><div>q</div></noscript><span block>` (C closes the `p`) | valid/agree | valid/agree | valid/agree | valid/agree |

| Head shape (then `</head>`) | shipped | h0 | h1 | h2 | hc |
|---|---|---|---|---|---|
| `<noscript><img src=x></noscript>` | found/**disagree** | none | none | none | found/agree |
| `<noscript><link rel=stylesheet></noscript>` | found/agree | none | found/agree | found/agree | found/agree |
| `<noscript>Enable JS</noscript>` | found/**disagree** | none | none | none | found/agree |
| `<noscript><script>1</script></noscript>` | found/agree | none | none | found/agree | found/agree |
| `H27-head-noscript` (a `div` in C) | found/**disagree** | none | none | none | found/agree |

### 3.2 Sweeps

<Badge type="tip" text="FACT" /> 2026-10-18. In the scripting-enabled view there are 0 disagreements for every variant and every input. The table gives the scripting-disabled view.

| Input | Variant | Valid | With blocks | Disagree (disabled) |
|---|---|--:|--:|--:|
| body 2 000 000 | shipped | 804 382 | 766 793 | **45** |
| | a | 675 249 | 643 872 | 0 |
| | a-fx | 698 040 | 665 617 | 0 |
| | b0 | 328 708 | 313 351 | 0 |
| | b1 | 383 696 | 365 752 | 0 |
| head 500 000 | shipped | 244 492 | 235 301 | 0 |
| | a | 243 449 | 234 302 | 0 |
| | b0 / b1 | 147 726 / 165 200 | — | 0 / 0 |
| review2 300 000 | shipped | 44 855 | 21 323 | 0 |
| | a | 44 849 | 21 321 | 0 |
| | b0 / b1 | 42 622 / 43 175 | — | 0 / 0 |
| fixtures 197, cases 162 | all | — | — | 0 |

- The file keeps the 40 largest groups of the 45 body disagreements. All 40 listed signatures have a `table` in C together with `marquee`, `object` or `applet`: the H-37 class. `sweep-body.json` has five examples.
- Model (a) loses 6 review2 documents that the shipped rule accepts. All 6 are `A:M-12`: a `<![CDATA[` in C that M-12 rejects. In HTML context the WHATWG tokenizer reads it as a bogus comment in both views, so this rejection is not needed. It keeps the rule "the main-pass rules apply" without an exception.
- The generated documents over-represent table parts, `select` and `template` in C, so the "valid" counts of the sweeps are not coverage estimates. The coverage numbers come from the corpus (§3.3).

<Badge type="tip" text="FACT" /> Head sweep: 500 000 documents. Of these, 200 446 are valid markup with a head end tag by the shipped walk.

| Rule | Head end found | Of these, markup valid | Disagree (disabled) |
|---|--:|--:|--:|
| shipped | 410 183 | 200 446 | **243 729** (87 424 markup valid) |
| h0 | 0 | 0 | 0 |
| h1 | 110 594 | 99 121 | 0 |
| h2 | 122 721 | 109 141 | 0 |
| hc | 449 921 | 219 994 | 0 |

### 3.3 Corpus

<Badge type="tip" text="FACT" /> 882 pages, fetched on 2026-10-18 (S1 URL list). 862 pages are UTF-8 and at most 5 MiB. With the shipped rules, 798 pages are valid for RQP (90.5 %).

**noscript in real pages.**

| Item | Pages | Elements |
|---|--:|--:|
| Any `noscript` in HTML context | 567 | 2 583 |
| In the document head | 225 | 527 |
| Elsewhere (body, template, after the head) | 464 | 2 056 |
| Before the head start tag | 0 | 0 |

Start tags in C, as pages with the tag (a page counts once for each tag):

| Location | Tags (pages) |
|---|---|
| Head | `link` 69, `img` 56, `style` 31, `div` 25, `meta` 10, `iframe` 9, `picture` 2, `br` 1; text 6; comments 2; empty only 62 |
| Elsewhere | `img` 248, `iframe` 185, `div` 143, `a` 40, `p` 33, `style` 29, `svg` 21, `span` 19, `link` 18, `path` 17, `li` 15, `ul` 15, `h2` 8, `meta` 8, `nav` 8, `button` 7, `footer` 7, `header` 7, `strong` 7; text 95 |

**Body models: pages valid.**

| Variant | Valid pages | Lost (shipped valid, variant invalid) | Disagree (either view) |
|---|--:|---|--:|
| shipped | 798 | — | 0 |
| a | 798 | **0** | 0 |
| a-fx | 813 | 0 (+15) | 0 |
| b0 | 767 | 31: `header` 6, `picture` 6, `h1` 3, `details` 2, `figure` 2, `h2` 2, `i` 2, `script` 2; `br`, `button`, `dl`, `input`, `section`, `strong` 1 each | 0 |
| b1 | 785 | 13: `details` 2, `figure` 2, `script` 2, `table` 2, `time` 2; `aside`, `button`, `dl` 1 each | 0 |

Model (a) loses no page for item 7 (`html`, `body` or `head` in C) and no page for M-26 D in C.

**Simulated blocks** (DESIGN estimate, S1 method; only pages that the shipped rules accept):

| Variant | `small`: 742 documents | `main`: 480 documents |
|---|--:|--:|
| shipped | 416 valid | 67 valid |
| a, a-fx | 416 (0 lost) | 67 (0 lost) |
| b0 | 403 (13 lost) | 65 (2 lost) |
| b1 | 411 (5 lost) | 67 (0 lost) |

Disagreements in either view: 0 for every variant.

**Head rules.** Of the 798 valid pages, 773 have a head end tag by the shipped walk.

| Rule | Valid pages with RQP | Lost | Gained | Disagree (disabled) |
|---|--:|--:|--:|--:|
| shipped | 773 | — | — | **54** (the manifest is in the body) |
| h0 | 579 | 194 | 0 | 0 |
| h1 | 719 | 54 | 0 | 0 |
| h2 | 719 | 54 | 0 | 0 |
| hc | 786 | 0 | 13 | 0 |

- h1 loses exactly the 54 pages that disagree. The first failing token in their head `noscript` is `img` on 27 pages, `div` on 22, `iframe` on 4, text on 3 and `picture` on 2 (a page can have more than one). These are tracking pixels and tag-manager fallbacks in the head. `corpus.json`, field `head.h1.lostPages`, has the URLs.
- No corpus page has `base`, `script` or `title` in a head `noscript`, so h1 and h2 give the same pages.
- hc gains 13 valid pages that have a head start tag but no head end tag by the walk (DL-034), and loses none.

**Combined.** These are the pages that a server sends with RQP: valid markup and a manifest position.

| Body + head | Pages |
|---|--:|
| shipped + shipped (today) | 773 |
| a + h1 | 719 |
| a + hc | 786 |
| b0 + h1 / b1 + h1 | 692 / 706 |
| a-fx + hc (with H-40 relaxation 2) | 801 |

### 3.4 Remaining disagreements for each option

| Option | Body (blocks, offsets) | Head (manifest position) |
|---|---|---|
| a + h1 (or h2) | none found | none found |
| a + hc | none found | none found |
| b0 or b1 + h1 | none found | none found |
| shipped + shipped | 45 per 2 000 000 generated documents (H-37 class); 0 corpus pages | 54 of 773 corpus pages (7.0 %); 87 424 generated documents |

Some known differences stay in every option. None of them changes block bytes or the manifest position:

- C can close elements around the `noscript` early (a `div` in C closes an open `p`).
- The scripting-disabled view reconstructs formatting elements at the `noscript` start tag.
- With h2 only, the scripting-disabled DOM has a `script`, `base` or `title` of C outside the `noscript` element.

## 4. Security analysis

**Who is affected.** Readers with scripting disabled: crawlers, server-side HTML tools, native HTML parsers, and a WebView or browser with JavaScript turned off. The RQP client finds blocks and the manifest with the token rules (markup §2; manifest-patch §1.7, "The client finds the element with the tokenizer"), not with a DOM. The WebView has scripting enabled (M-04, DL-009). So the gap is a parity gap. It becomes a security issue only in two cases: a component reads blocks or the manifest from a scripting-disabled DOM (the spec does not allow this), or a page wants the same content for all readers (a cut block in a crawler view).

**Model (a), argument (DESIGN).**

- With scripting disabled, a `noscript` start tag in body is "any other start tag", that is, an ordinary element (FACT, §7 item 4). So the scripting-disabled tree builder sees the scripting-enabled token stream with the tokens of C inserted after the `noscript` start tag, and the `noscript` element is an ordinary element of the special category.
- M-11 items 1 and 3 make the tokenization of C the same as in that view: C ends in the data state, and there is no `plaintext`, no nested `noscript` and no foreign content. The raw text elements of C switch the tokenizer state by M-04 in both views.
- Item 8 checks the inserted tokens with the main-pass document rules, in the state those tokens would have there.
- Items 5 and 7 make the state after the end tag `noscript` the same as the state before C. C has no crossing, no unclosed element, no stray end tag and no `html`/`body`/`head` tag, so every pop is exact and no mark is set. The study checks this: 0 differences. So the rest of the stream is checked as before.
- C is never in block content (`noscript` is in `FORBIDDEN_CONTENT`) and holds no block (item 2), so the block-content rules M-22 … M-24 are not needed for C.

Result: the scripting-disabled stream satisfies the document rules of section 2, and the principle of section 2 (the token rules and the tree builder give the same blocks) applies to it. Model (a) adds no new argument; it extends the main-pass argument to one more stream. A gap in the main-pass rules is then also a gap in this view, and the same sweeps test both.

**Where the argument needs the head rule.** In the head, the scripting-disabled parser uses "in head noscript", not "in body", so the argument above does not hold there. With h1, a head `noscript` holds only the void head elements and raw text that "in head noscript" processes as head content, so the head and the manifest position stay the same. A document that fails the head rule is sent without RQP, so its body blocks are never used.

**Model (b), argument (DESIGN).**

- The allowed names are void elements (`img`, `link`, `meta`, `br` …), raw text elements (`iframe`, `style`), and elements that "in body" inserts as ordinary or `p`-closing elements.
- None of them is a table part, a `table`, a form, a marker element (`applet`, `marquee`, `object`), a `button`, `select`, `template` or foreign content, so the H-35 … H-38 mechanisms cannot start in C.
- `a` and the formatting names of b1 can still run the adoption agency, for example an `<a>` in C while an `a` outside is open. The main pass has the same behavior outside blocks.
- The safety of (b) depends on a case analysis for each name in each insertion mode. Each new name needs a new analysis.

**Resource limits.**

| Option | Time | Memory |
|---|---|---|
| a | One more pass over the tokens of C (a candidate can merge it with the pass of items 1 to 6): linear in the input | See the notes after the table |
| b | One set lookup for each start tag of C | As today |
| h1 | One more tokenization of the C of each head `noscript`; no stack | Constant |
| hc | Less than today: the walk ends at the head start tag | Constant |

Memory of model (a):

- The C pass uses the token stack. With D counted (A1), that stack has at most 256 entries.
- So the main stack can replace the separate C stack of item 5. Today that stack is bounded only by the size of C (PHP worst case 17 MiB, S1 T-16).
- The study copies the stack (at most 256 entries) for each `noscript`. A candidate can work in place, because C restores the state.

**What the options do not cover.**

- Chrome or another browser with scripting disabled was not run (parse5 only).
- The cross-check tool checks the M-26 depth claim only in the scripting-enabled view. With A1, D also bounds the scripting-disabled token stack (DESIGN, not measured). With A2 (D not counted in C), the scripting-disabled tree can be as deep as C.
- A `<meta charset>` or `<meta http-equiv="refresh">` in a head `noscript` acts only in the scripting-disabled view.
  - The encoding cannot change. RQP needs a UTF-8 document with a byte order mark or a UTF-8 `Content-Type` charset (client item 6, H-62, H-66), and the parser changes the encoding only when "the confidence is currently tentative" (FACT, §7 item 7).
  - A refresh is a navigation for a scripting-disabled reader. It does not change what the RQP client reads.
- hc puts the manifest before the page's `<meta charset>`. When the manifest is long, that element is no longer in the first 1024 bytes.
  - The browser does not use the prescan when the transport layer gives the encoding (FACT, §7 item 8), and the full document always has `Content-Type: text/html; charset=utf-8` (H-66).
  - A conformance checker reports an error for the page.

## 5. Proposed rule text (not applied)

### 5.1 Body: model (a) with D in C (option A1)

```diff
 **M-11 Noscript.** Let C be the RAWTEXT content of a `noscript` element in HTML context …
 …
 6. A start tag in C with a name in `TABLE_PART` or the name `col` MUST have a `table` element on the stack of C (item 5) …
+7. C MUST NOT contain a start tag or an end tag named `html`, `body` or `head` (human decision H-7x).
+8. **Main pass on C** (human decision H-7x). The tokens of C, and then the end tag `noscript` of item 1, MUST satisfy the document rules M-05 to M-10, M-12, M-13 and the limit on D of M-26, as if they came in the main token pass at the place of C. The token stack, its marks and D at the start of C are those after the `noscript` start tag (the `noscript` element is the current node). After the end tag `noscript`, the main pass continues with the state that it has without this item. (Items 5 and 7 make the two states equal.)

-Items 1 to 3, 5 and 6: error `RQP_MARKUP_STRUCTURE`.
+Items 1 to 3 and 5 to 7: error `RQP_MARKUP_STRUCTURE`. Item 8: the error of the rule that fails (for example `RQP_MARKUP_LIMIT` for D).
 …
-The tags in C are not part of the main token pass: they do not change the token stack of M-05 and the depth counter D of M-26. The `noscript` element itself is pushed and counts.
+The tags in C change the main token pass only during the pass of item 8; after the end tag `noscript`, the token stack of M-05 and the depth counter D of M-26 are as before C. The `noscript` element itself is pushed and counts.
+
+<Badge type="tip" text="FACT" /> Item 8: with scripting disabled, a `noscript` start tag in body is "any other start tag", and the element "will be an ordinary element. With one exception: if scripting mode is Disabled, it can also be a noscript element" (WHATWG §13.2.6.4.7). The scripting-disabled tree builder therefore sees the tokens of C as if they came in the main stream. Without item 8, `<object><a href="1"><noscript><table><marquee><table></table></marquee></table></noscript>x</a></object>` before a `span` block (M-09 condition 2 inside C) and nested forms behind an `object` inside C (M-10 item 2) cut a later block in parse5 8.0.1 with scripting disabled (security review T-16; study `evidence/markup-noscript`).
```

```diff
 **Depth counter D** …
-7. The tags in the `noscript` content C (M-11) are not part of the main pass and do not change D. The `noscript` element itself is pushed (item 2).
+7. The tags in the `noscript` content C (M-11) count in the pass of M-11 item 8; a step there that makes D larger than 256 is an error. After the end tag `noscript`, D is as before C. The `noscript` element itself is pushed (item 2).
```

```diff
 ## 5. Known limits
 2. **Scripting disabled.** RQP uses the scripting-enabled view (M-04). M-11 aims to make the block result the same in both views …
-<Badge type="warning" text="RESEARCH" /> **Open (H-42).** The agreement with the scripting-disabled view is not complete yet. …
+Item 8 (H-7x) runs the main-pass rules on C, so the shapes of M-09 and M-10 cannot come back inside C. <Badge type="tip" text="FACT" /> Study of 2026-10-18 (`evidence/markup-noscript`): 0 block disagreements with parse5 8.0.1 in both views on the golden fixtures, the cross-check cases and 2 800 000 generated documents (the shipped M-11: 45 in the scripting-disabled view); 0 corpus pages lost.
```

Fixture effects:

- `FX-RQ-MARKUP-LIMIT-DEPTH-NOSCRIPT` changes from valid to invalid (`RQP_MARKUP_LIMIT`). This replaces architect decision R5 of `markup-edge-cases.md` §6.5.
- New fixtures: the T-16 shape, the H-36 shape in C, `<form><noscript><form>`, `<noscript><body></body></noscript>`, and a deep C.
- The cross-check oracle (`crosscheck/src/depth.mjs`, `checkNoscriptContent`) and the corpus classifier change together with the candidates.

Option A2 (D not counted in C) uses the same text without the sentence about D in item 8 and without the change of M-26 item 7. The fixture then stays valid.

### 5.2 Body: model (b) (option B)

```diff
-3. C MUST NOT contain a start tag named `frameset`, `math`, `noscript`, `plaintext`, `select`, `svg` or `template`.
+3. Every start tag in C MUST have a name in `NOSCRIPT_CONTENT`: `a`, `div`, `iframe`, `img`, `link`, `meta`, `p`, `span`, `style` (b0; or the list b1 of the study).
```

Item 6 becomes redundant, because no table part is in the list. The fixtures with a `table` in C (`NOSCRIPT-BALANCED`, `NOSCRIPT-TABLE-IN-CELL`) change from valid to invalid.

### 5.3 Head: h1 (option H1)

```diff
 2. **In the head.** While the document head is the current node (the top entry of the token stack), only these can come: gaps of white space, comments, DOCTYPE tokens, the start tags `base`, `basefont`, `bgsound`, `link`, `meta`, `noframes`, `noscript`, `script`, `style`, `template` and `title`, ignored end tags …, and the `</head>` end tag. …
+   For a `noscript` start tag here, tokenize its content C as in markup M-11 (C, then `</noscript>`, from the data state). The tokens of C MUST be only gaps of white space, comments, DOCTYPE tokens, the start tags `basefont`, `bgsound`, `link`, `meta`, `noframes` and `style` with their raw text, and ignored end tags. Else the document has no head end tag (human decision H-7x).
```

```diff
-<Badge type="info" text="DESIGN" /> The parity has two conditions. The document is decoded as UTF-8 (client item 6, H-62): with another encoding the browser can see other tokens. Scripting is enabled: with scripting disabled, a `noscript` start tag in the head switches the parser to the "in head noscript" mode, which this walk does not model; this is open item H-42 (WP-17 S3).
+<Badge type="info" text="DESIGN" /> The parity has one condition: the document is decoded as UTF-8 (client item 6, H-62); with another encoding the browser can see other tokens. <Badge type="tip" text="FACT" /> With scripting disabled, a `noscript` start tag in the head switches the parser to "in head noscript" (§13.2.6.4.4). That mode keeps white space, comments, DOCTYPE tokens, `basefont`, `bgsound`, `link`, `meta`, `noframes`, `style` and other end tags than `br` in the head, and for anything else it pops the `noscript` element and reprocesses the token in "in head" (§13.2.6.4.5). The noscript item of item 2 allows only those tokens, so the head ends at the same `</head>` in both views (H-7x; study `evidence/markup-noscript`: 0 disagreements; 54 of 773 valid corpus pages lose RQP).
```

The client uses the same walk (client item 3), so server and client stay equal. New protocol fixtures: a `P39`/`C21` case with `<noscript><img></noscript>` in the head (no head end tag), and a case with `<noscript><link></noscript>` (head end tag found).

### 5.4 Head: hc (option HC, protocol change)

This is an outline only; the full text needs its own review. The normative text is in `docs/engineering/protocol/manifest-patch.md` §1.7, "Document head" (H-72, WP-17 S3 T-08a).

- Server item 2: the insertion point is immediately after the `>` of the head start tag that item 1 accepts (the document head).
- Client item 3: the manifest start tag is the first token after the head start tag of the document (item 1), with no gap.
- Items 2 and 3 of the head walk are then not needed for the manifest. DL-031 and DL-034 are replaced.
- Affected: `manifest-patch.md` §1.7; `negotiation.md` §4 step 2 (a document without a head start tag stays without RQP); `spec/rqp/examples/kat/full*.html`; protocol fixtures `P39`/`C21`; the reference and the demo.

## 6. Options for the human

<Badge type="danger" text="GATE" /> The body rule is a security rule (a finding of the security review), and the head rule is protocol. Both are decisions of the human (AGENTS.md rule 3).

### 6.1 Body (`noscript` content C)

| Option | What it closes | Pages lost (corpus) | Cost | Remaining |
|---|---|--:|---|---|
| **A1. Model (a), D counted in C (recommended)** | The T-16, H-36 and H-37 shapes in C; all 45 sweep disagreements | 0 of 798 (simulated blocks: 0) | M-11 items 7 and 8; M-26 item 7 changes (replaces architect decision R5); `LIMIT-DEPTH-NOSCRIPT` becomes invalid; the four candidates, the cross-check oracle and the classifier change; the C stack is bounded by D | none found; parse5 only |
| A2. Model (a), D not counted in C | As A1 | 0 | As A1, without the M-26 change and the fixture change; the C stack stays bounded only by the size of C | As A1; the scripting-disabled depth is not bounded |
| B. Allowlist (b0 or b1) | As A1 | b0: 31 (3.9 %); b1: 13 (1.6 %). Simulated blocks: b0 −13/−2, b1 −5/0 | Smallest code change (one set); item 6 becomes redundant; two golden fixtures become invalid; the list needs a case analysis for each name and grows with real pages | none found |
| C. Keep M-11; accept the gap | Nothing | 0 | Text change only: §5 item 2 records the gap as a decision | The MEDIUM finding stays: 45 per 2 000 000 generated documents |

Recommendation: **A1.**

- It loses no real page.
- It follows one principle ("the main-pass rules hold in both token streams") instead of a list.
- It bounds the memory of the C check by D.
- Take A2 if the human wants to keep the fixture and R5.

Interaction with H-40 relaxation 2 (`svg` in C, lane T-01): with model (a), that relaxation needs only item 3 without `svg` and `math`, and an item 5 stack that follows M-05 in foreign content. The study variant `a-fx` gains 15 pages (798 → 813), with 0 disagreements in all runs.

### 6.2 Head (`noscript` in the head)

| Option | What it closes | Pages (valid, with RQP) | Cost | Remaining |
|---|---|--:|---|---|
| **HC. Manifest after the head start tag (recommended; protocol change)** | The head gap in both views, for all head content | 786 (+13, 0 lost) | Reverses the insertion point of H-49/DL-031; rewrites server item 2 and client item 3; new KAT full documents and protocol fixtures; the page's `<meta charset>` can move past byte 1024 (no decoding effect under H-66) | none found |
| H1. Keep the insertion point; strict head `noscript` (h1) | The head gap | 719 (−54, 7.0 %) | One more check in the head walk (server and client); protocol fixtures | none found |
| H2. As H1, with `base`, `script`, `title` (h2) | The head gap | 719 (−54) | As H1; the scripting-disabled DOM has the `script`/`title` of C outside the `noscript` | none found |
| H0. No `noscript` in the head | The head gap | 579 (−194) | Simplest rule | none |
| HK. Keep the walk; restrict the parity claim to scripting enabled | Nothing | 773 | Text only (§1.7: the RQP client reads the manifest with the token walk; a scripting-disabled DOM has it in the body) | 54 of 773 pages |

Recommendation: **HC.** It closes the gap, gains pages, and its walk is shorter. If the human keeps the insertion point of H-49 (before `</head>`), take **H1**. H1 gives the same corpus result as H2, and its list is the literal list of "in head noscript", so there is no early close of the `noscript` element to reason about.

Combined: A1 + HC gives 786 pages with RQP (773 today); A1 + H1 gives 719.

## 7. Facts and sources

All WHATWG quotes come from the HTML Living Standard, `https://html.spec.whatwg.org/multipage/parsing.html` (page "Last Updated 16 October 2026"), checked on 2026-10-18.

1. <Badge type="tip" text="FACT" /> §13.2.4.5: "The scripting mode … is initially set to Normal if scripting was enabled for the Document … and Disabled otherwise." The modes are Normal, Disabled, Inert and Fragment.
2. <Badge type="tip" text="FACT" /> §13.2.6.4.4 "in head": "A start tag whose tag name is "noscript", if scripting mode is not Disabled" → "Follow the generic raw text element parsing algorithm". "A start tag whose tag name is "noscript", if scripting mode is Disabled" → "Insert an HTML element for the token. Switch the insertion mode to "in head noscript"."
3. <Badge type="tip" text="FACT" /> §13.2.6.4.5 "in head noscript":
   - A DOCTYPE token: ignore.
   - A start tag `html`: process with the "in body" rules.
   - The end tag `noscript`: pop the `noscript` element and switch to "in head".
   - White space characters, a comment, a processing instruction, and "A start tag whose tag name is one of: "basefont", "bgsound", "link", "meta", "noframes", "style"": process with the "in head" rules.
   - "An end tag whose tag name is "br"": act as "anything else".
   - The start tags `head` and `noscript`, and "Any other end tag": ignore.
   - "Anything else": "Pop the current node (which will be a noscript element) … Switch the insertion mode to "in head". Reprocess the token."
4. <Badge type="tip" text="FACT" /> §13.2.6.4.7 "in body": "A start tag whose tag name is "noscript", if scripting mode is not Disabled" → generic raw text. "Any other start tag": "Reconstruct the active formatting elements, if any. Insert an HTML element for the token. This element will be an ordinary element. With one exception: if scripting mode is Disabled, it can also be a noscript element."
5. <Badge type="tip" text="FACT" /> §13.2.6.4.6 "after head" and §13.2.6.4.16 "in template": the start tags that go to the "in head" rules are `base`, `basefont`, `bgsound`, `link`, `meta`, `noframes`, `script`, `style`, `template` and `title` (no `noscript`). In "in template", "Any other start tag" pushes "in body".
6. <Badge type="tip" text="FACT" /> §13.2.6.4.3 "before head": "A start tag whose tag name is "head"" → insert it, set the head element pointer, switch to "in head". §13.2.6.4.9 "in table", "Anything else": "Enable foster parenting, process the token using the rules for the "in body" insertion mode, and then disable foster parenting." §13.2.4.2: `noscript` is in the special category.
7. <Badge type="tip" text="FACT" /> §13.2.6.4.4, a start tag `meta`: the parser changes the encoding only "if the element has a charset attribute, and getting an encoding from its value results in an encoding, and the confidence is currently tentative".
8. <Badge type="tip" text="FACT" /> §13.2.3.2 (encoding sniffing): "If the result of BOM sniffing is an encoding, return that encoding with confidence certain". "If the transport layer specifies a character encoding, and it is supported, return that encoding with the confidence certain." Both steps come before the prescan.
9. <Badge type="tip" text="FACT" /> The parse5 8.0.1 results of §3 (2026-10-18; scripts in `tools/rqp-markup/study/noscript`).
10. NOT FOUND: the behavior of Chrome (or another browser) with scripting disabled for the shapes of §3.1. Not run in this study.
11. NOT FOUND: a document conformance quote for `<meta charset>` in the first 1024 bytes. Not checked for this report; it matters only for option HC, as a conformance note.
