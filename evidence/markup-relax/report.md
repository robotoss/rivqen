# H-40 study: three relaxations of the RQP markup rules

<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- SPDX-FileCopyrightText: The Rivqen Authors -->

Evidence for WP-17 S3, task T-01 (sprint record `docs/engineering/plan/sprints/WP-17-S3.md`). Measured on 2026-10-18. Tools: `tools/rqp-markup/study/relax/` (README with the commands). Machine results: `agreement.json`, `corpus.jsonl`, `corpus-summary.json` in this directory.

**Marks.** FACT: measured, or read in the cited source on the given date. DESIGN: a proposal or an estimate. RESEARCH: not verified; an open question. This study changes no rule. The rule texts in section 5 are proposals for the human decision.

**Decisions (added by the architect, 2026-10-18).** The human chose R1 option A (H-73), R2 option A (H-74) and R3 option A (H-75). See the sprint record `docs/engineering/plan/sprints/WP-17-S3.md` §1.

## 1. Question

S1 (decision H-40) asked for evidence on three relaxations before the human decides:

| ID | Relaxation | Rule that rejects today |
|---|---|---|
| R1 | A balanced `svg` or `math` subtree in the content of an `html` block | M-22 item 2 (`svg`, `math` in `FORBIDDEN_CONTENT`) and item 3 (`<path/>`) |
| R2 | `svg` in the `noscript` content C | M-11 item 3 |
| R3 | No H-32 mark for a crossing that ends at the region's own `</svg>` (icon shape `<a href="#"><svg><path d="M0"></svg></a>`) | M-06 item 4 |

The study answers these questions for each relaxation:

- How many corpus pages and simulated blocks become valid?
- Do the four candidates and parse5 8.0.1 (scripting enabled and disabled) still agree?
- For R1: what must the attribute rule (`manifest-patch.md` §4.2) check in `svg` and `math` content?
- What rule text, and which options for the human?

## 2. Method

### 2.1 Study rule

**DESIGN.** `src/markup-relax.mjs` is a copy of the Node.js candidate (`tools/rqp-markup/node/src/markup.mjs`) with flags. It imports the shipped tokenizer, name sets, JSON check and result object unchanged. Only the token-stack pass and the `noscript` check are copied. The shipped candidate is not changed.

| Flag | Meaning |
|---|---|
| `r1` | R1. In the content, an `svg` or `math` start tag (also with the self-closing flag) starts a foreign region. M-07 applies in it. M-23 applies to every element of the region. `script` and `style` stay forbidden in the region |
| `r1a` | R1 with an element allowlist in the foreign region of block content (`FOREIGN_CONTENT_ALLOWED`, section 5.1) |
| `r2` | R2. In C, an `svg` start tag starts a foreign region of C. In it: M-05 item 2 (self-closing tags are not pushed), no tokenizer state change, M-07 items 1 to 5, M-12 |
| `r3` | R3. The end tag of the region root does not set the mark for its crossing. With `r1`, M-23 allows that crossing in block content; with `r2`, M-11 item 5 allows it in C |
| `r3w` | R3 for every end tag in a foreign region (scope "region"), for comparison |

Combinations: `base` (the shipped rules), each flag alone, and `r1+r3`, `r2+r3`, `r1+r2`, `r1+r2+r3`, `r1a+r2+r3`, `r1+r2+r3w`.

### 2.2 Oracle

**DESIGN.** For each result that the study rule says is valid, `src/oracle.mjs` reports a finding when parse5 8.0.1 disagrees:

| Kind | Check |
|---|---|
| `parse5-<view>-*` | The checks of `tools/rqp-markup/crosscheck` (`view.mjs`, `compare.mjs`): same blocks, ids, HTML namespace, start and end offsets, explicit end tags, no manifest-like script. Both views: scripting enabled and disabled |
| `depth-invariant` | The claim of M-26 (`crosscheck/src/depth.mjs`) |
| `tree-<view>-*` | New for R1. Each start tag of block content makes an element inside the block element, in the namespace that the token stack says |
| `fragment-*` | New for R1. The block content is parsed as a fragment, with the block element as the context (HTML fragment parsing algorithm, as a client that sets the content of the element does). The elements and namespaces must be the same |
| `noscript-*[-head]` | New for R2 (scripting disabled). The `noscript` element ends at the end tag that the token pass found. Each start tag of C makes an element inside it, in the namespace that the C pass says. `-head`: the `noscript` element is a child of `head` |
| `monotonic` | A document that a smaller combination accepts gets the same result from every larger combination |
| `attr-walk` | For R1: the walk of the attribute rule (§4.2, the reference `passesAttributeRule`) reads exactly the start tags of the block content that the token pass reads, also in a foreign region |

The tree checks are stricter than the cross-check tool. The `noscript-*` check also finds the known H-42 gap of `markup.md` §5 item 2: C closes an element around the `noscript`, and the head case. Section 3.1 separates these findings from findings that a relaxation causes.

### 2.3 Inputs

| Set | Documents |
|---|---|
| Golden fixtures | 197 (`fixtures/rqp/markup`) |
| Cross-check cases | 162 (`crosscheck/cases/*.json`) |
| Generator profiles `classic`, `review`, `review2`, `review3` | 50 000 each, seed 40, 20 pieces (`crosscheck/src/generator.mjs`) |
| Relaxation sweep | 300 000, seed 40 (`src/gen.mjs`). Three shapes: block content with foreign subtrees, `noscript` with `svg` in body and in head, and icon shapes outside blocks. The subtrees contain crossings, unclosed foreign children, integration points with text and with tags, breakout tags, `foreignObject` with HTML, nested `svg` and `math`, CDATA, stray end tags, and the attribute shapes of section 4 |
| Corpus | 882 pages of the S3 fetch (session scratchpad, deleted at sprint close). Same method as S1 T-15: pages without blocks, and simulated blocks. Variant "small": the first `h1` and up to two `price`/`title`/`content` elements. Variant "main": the first `main` |

The four shipped candidates ran in batch mode on the corpus pages and variants. They also ran through the cross-check tool on 20 000 relaxation sweep documents.

### 2.4 Invariants, failure modes and limits of the study tools

- **Invariant.** With every flag off, the study rule gives the result object of the shipped candidate. FACT: the results are equal on these inputs:
  - the 197 golden fixtures and their `expected.json`;
  - the 162 cases;
  - 7 500 generated documents (`test/equivalence.test.mjs`);
  - all 882 corpus pages and all 1 222 variants (`baseEqualsShipped`).
- **Invariant.** A relaxation only adds documents (monotonic check, 0 violations).
- **Failure modes.** A corpus page that times out or crashes a worker is recorded as an error (0 errors). A candidate that does not run stops the corpus run.
- **Limits.**
  - The study rule keeps the limits of the candidate (5 MiB, D ≤ 256).
  - The corpus worker runs one page per child process with a 600 s limit.
  - The oracle walks trees iteratively.
  - Page bytes go only to the scratch directory. The evidence has URLs, numbers, rule IDs, tag and attribute names and synthetic documents.
  - A test checks that page text does not appear in the worker output.

## 3. Results

### 3.1 Agreement with parse5 and between candidates

**FACT.** For every set and every combination, there is no finding of these kinds: `parse5-*`, `tree-*`, `fragment-*`, `depth-invariant`, `monotonic`, `attr-walk`. The only findings are `noscript-*` findings of the scripting-disabled tree check.

The table counts documents with a finding (`agreement.json`). "Base docs" are documents that `base` accepts; their result is the same in every combination.

| Set | Documents | Valid base | Base docs with a finding | Gained docs (all three relaxations, `r1+r2+r3`) | Gained docs with a finding | Of them not `-head` |
|---|--:|--:|--:|--:|--:|--:|
| Fixtures | 197 | 72 | 0 | 1 | 0 | 0 |
| Cases | 162 | 88 | 3 | 0 | 0 | 0 |
| `classic` | 50 000 | 17 518 | 139 | 60 | 0 | 0 |
| `review` | 50 000 | 8 623 | 184 | 18 | 1 | 0 |
| `review2` | 50 000 | 7 506 | 111 | 25 | 1 | 0 |
| `review3` | 50 000 | 7 367 | 99 | 20 | 1 | 0 |
| Relaxation sweep | 300 000 | 8 819 | 0 | 78 666 | 9 871 | 0 |

Gained documents per relaxation in the relaxation sweep:

| `r1` | `r1a` | `r2` | `r3` | `r3w` | All three |
|--:|--:|--:|--:|--:|--:|
| 39 736 | 31 289 | 27 369 | 4 798 | 5 681 | 78 666 |

- The findings on base documents are the open H-42 gap (study T-02). Kinds: `noscript-end` and `noscript-outside`, with and without `-head`. With scripting disabled, a `div` or `p` in C closes an open `p` and the `noscript` element with it. A non-head tag in a head `noscript` ends the head. No relaxation causes these findings.
- Every finding on a gained document is `noscript-*-head`. There are two causes:
  - an `svg` in a `noscript` in the head (R2);
  - a head `noscript` with other content, in a document that base rejected for another reason.

  "In head noscript" closes the `noscript` and the head at the `svg` start tag. The blocks still agree (no `parse5-*` finding). The open question is the head end tag and the manifest position (T-02, `manifest-patch.md` §1.7).
- **FACT.** The four shipped candidates on 20 000 relaxation sweep documents: 535 valid each, 0 cross-check findings (both views), no candidate disagreement.

### 3.2 Corpus: pages without blocks

**FACT.** This fetch has 882 pages. The four candidates accept 798 (90.5 %; S1: 786 of 867, 90.7 %). Candidate disagreements: 0. With flags off, the study rule equals the shipped Node.js candidate on 882 of 882 pages. It also equals the four-candidate validity on 882 of 882.

| Combination | Valid pages | % | Gained |
|---|--:|--:|--:|
| base | 798 | 90.5 % | 0 |
| R1 (`r1`, `r1a`) | 798 | 90.5 % | 0 (R1 changes block content only) |
| R2 | 813 | 92.2 % | 15 |
| R3 | 805 | 91.3 % | 7 |
| R3 scope region (`r3w`) | 805 | 91.3 % | 7 |
| R2 + R3 | 820 | 93.0 % | 22 |
| R1 + R2 + R3 (also `r1a`, `r3w`) | 820 | 93.0 % | 22 |

Pages that R2 makes valid (15):

- `https://www.bbc.com/news`
- `https://www.bbc.com/future/article/20260128-how-to-make-google-put-trusted-sources-up-top-when-you-search`
- `https://www.bbc.com/persian`
- `https://www.bbc.com/persian/iran-features-53961947`
- `https://www.mk.ru/`
- `https://www.mk.ru/politics/2026/10/18/posledniy-shans-es-sankcii-protiv-rf-na-grani-sryva.html`
- `https://www.bbc.com/russian`
- `https://www.bbc.com/russian/articles/ckly02022p6go`
- `https://blogs.microsoft.com/`
- `https://www.chase.com/`
- `https://www.shopify.com/`
- `https://www.shopify.com/blog`
- `https://www.shopify.com/blog/13975985-how-to-find-a-manufacturer-or-supplier-for-your-product-idea`
- `https://discuss.python.org/`
- `https://eda.ru/`

Of them, `https://eda.ru/` has the `svg` in a head `noscript` (the `-head` finding of section 3.1).

Pages that R3 makes valid (7):

- `https://www.folha.uol.com.br/`
- `https://www.elegantthemes.com/blog/`
- `https://www.elegantthemes.com/blog/divi-resources/exploring-slots-for-components-in-divi-5`
- `https://colourpop.com/`
- `https://colourpop.com/products/daybreak-and-moonlight-fresh-kiss-trio`
- `https://skillbox.ru/course/profession-project-manager-pro/`
- `http://government.ru/news/60193/`

Two more pages have the M-06 item 4 shape (`deathwishcoffee.com`). They stay invalid because of `select` in `noscript` (M-11 item 3).

Pages that stay invalid with all three relaxations, by first rule:

| Rule | Pages |
|---|--:|
| M-02 | 18 |
| M-06.1 | 17 |
| M-09 | 6 |
| M-13 | 4 |
| M-11.3 (`select`) | 3 |
| M-07.3 | 3 |
| M-01 | 2 |
| M-11.5 | 2 |
| M-10.2 | 2 |
| M-07.5 | 2 |
| M-10.3 | 1 |
| M-07.1 | 1 |

**FACT.** The R2 crossing exception (R3 inside C) gains no corpus page: the R2 + R3 gain is the R2 gain plus the R3 gain. The S1 detail "`</svg>` over `<path>`" came from `<path/>` elements, which M-11 item 5 pushes in HTML context. The foreign region of R2 does not push them.

### 3.3 Corpus: simulated blocks

Two denominators:

- **A**: the variants of the 798 pages that the four candidates accept (S1 method).
- **B**: the variants of the pages that the combination accepts.

**Variant small** (742 variants in A; 56 pages have no usable element):

| Combination | Valid (A) | % (A) | Gained | Valid (B) | % (B) |
|---|--:|--:|--:|--:|--:|
| base (four candidates) | 416 | 56.1 % | 0 | 416 / 742 | 56.1 % |
| R1 (`r1`) | 611 | 82.3 % | 195 | 611 / 742 | 82.3 % |
| R1 allowlist (`r1a`) | 607 | 81.8 % | 191 | 607 / 742 | 81.8 % |
| R2 | 416 | 56.1 % | 0 | 421 / 755 | 55.8 % |
| R3 | 416 | 56.1 % | 0 | 420 / 749 | 56.1 % |
| R1 + R3 | 611 | 82.3 % | 195 | 616 / 749 | 82.2 % |
| R1 + R2 + R3 | 611 | 82.3 % | 195 | 627 / 762 | 82.3 % |
| R1 allowlist + R2 + R3 | 607 | 81.8 % | 191 | 623 / 762 | 81.8 % |

**Variant main** (480 variants in A; 318 pages have no `main`):

| Combination | Valid (A) | % (A) | Gained | Valid (B) | % (B) |
|---|--:|--:|--:|--:|--:|
| base (four candidates) | 67 | 14.0 % | 0 | 67 / 480 | 14.0 % |
| R1 (`r1`) | 175 | 36.5 % | 108 | 175 / 480 | 36.5 % |
| R1 allowlist (`r1a`) | 168 | 35.0 % | 101 | 168 / 480 | 35.0 % |
| R1 + R2 + R3 | 175 | 36.5 % | 108 | 181 / 496 | 36.5 % |
| R1 allowlist + R2 + R3 | 168 | 35.0 % | 101 | 174 / 496 | 35.1 % |

**FACT.**

- Candidate disagreements on the variants: 0.
- With flags off, the study rule equals the shipped candidate on all 1 222 variants.
- Oracle findings on gained variants: 152 (variant, combination) pairs. All of them are `noscript-*-head`, and in every pair the page itself has the same finding without blocks (section 3.1).

Other results:

- The R1 crossing exception (R3 in block content) gains no variant on this corpus: on denominator A, R1 + R3 gives the same result as R1.
- The allowlist (`r1a`) costs:
  - 4 small variants: `animate` 2, `animateTransform` 2;
  - 7 main variants: `animateTransform` 4, `animate` 2, `image` 1.
- `style` in a foreign region is forbidden in both R1 options. It rejects 3 small and 3 main variants.
- After R1, the next costs are HTML elements in the content:

  | Element | Small | Main |
  |---|--:|--:|
  | `script` | 46 | 122 |
  | `style` | 18 | 51 |
  | `template` | 11 | 17 |

  `link`, `meta`, `noscript` and `iframe` follow.
- D > 256 (M-26) rejects 4 variants with large `svg` subtrees (`latimes.com`, `dw.com`, `tagesschau.de` twice).

### 3.4 Foreign content in the gained blocks

**FACT** (`corpus-summary.json`, `variants.*.attrs`; R1 + R2 + R3 on denominator A).

| Variant | Variants with foreign content | Foreign start tags | Most frequent elements (variants) |
|---|--:|--:|---|
| small | 195 | 5 636 | `svg` 195, `path` 158, `g` 54, `use` 43, `title` 34, `defs` 18, `circle` 18, `clippath` 14, `rect` 14 |
| main | 108 | 22 665 | `svg` 108, `path` 89, `g` 33, `use` 24, `circle` 23, `title` 23, `defs` 18, `rect` 16, `clippath` 14 |

- SMIL elements occur in few variants: `animate` (2 small, 2 main) and `animateTransform` (2 small, 4 main). No `attributeName` value names `href`, so the proposed item 6 rejects 0.
- `image` occurs in 1 main variant. No gained variant has `a`, `foreignObject`, `script`, `feImage` or `set`.
- URL attributes in foreign content, mostly on `use`:

  | Attribute | Small | Main |
  |---|--:|--:|
  | `xlink:href` | 165 | 675 |
  | `href` | 93 | 504 |
  | `style` | 96 | 382 |

- The current attribute rule (§4.2 items 1 to 4) rejects 0 foreign attributes. The proposed items 5 and 6 (section 5.2) also reject 0.
- The current rule rejects HTML attributes in 100 of 611 small and 76 of 175 main block contents, mostly by item 2 (a raw `&` in a URL). This is a cost of H-59 for patches, not of R1.

## 4. Security analysis (R1)

R1 puts SVG and MathML into block content, so also into patch content. The client checks patch content in two steps:

- the markup analysis of the patched document (`manifest-patch.md` §4.1: M-22 to M-24);
- the attribute rule (§4.2).

R2 and R3 change no patch content. C is never in a block (`noscript` is in `FORBIDDEN_CONTENT`), and R3 changes only a mark of the token stack. For R2 and R3, the only risk is parser disagreement (section 3.1).

### 4.1 Ways to script execution or navigation from `svg` and `math` content

| # | Vector | Source (section 7) | Option A (allowlist) | Option B (R1 with items 5, 6) |
|---|---|---|---|---|
| 1 | SVG `script` element (also `<script/>`, which the tree builder runs at once) | SVG 2 §15.9; WHATWG §13.2.6.5 | Not in the allowlist | M-22 item 5 |
| 2 | Event handler attributes on SVG and MathML elements (`onload`, `onclick`, `onbegin`, `onend`, `onrepeat`, …) | SVG 2 §15.8; MathML Core §2.1.4 | §4.2 item 1 | §4.2 item 1 |
| 3 | `href` or `xlink:href` with a `javascript:` URL on SVG `a` (activation uses the HTML navigate algorithm) | SVG 2 §16.2, §16.1.7.1, §16.1.6 | `a` is not in the allowlist; item 4 applies to every attribute | §4.2 items 2 to 4 |
| 4 | SMIL animation of a URL attribute: `<a><animate attributeName="href" values="#a;javascript:x"/>`. The `href` of `a` is animatable, and `animate` and `set` animate URL values | SVG 2 §16.2 (href "Animatable: yes"); SVG Animations Level 2 §2.6, §2.10 and its data-type table | Not in the allowlist | Items 5 and 6 (section 5.2): item 5 rejects an `attributeName` with `href` or `on…`; item 6 checks each part of `values`. Item 4 alone catches `to` and `from`, but not a later part of `values` |
| 5 | The SMIL `href` of an animation element targets another element by id, also outside the block (it can change attributes of page SVG) | SVG Animations Level 2 §2.5 | Not in the allowlist | Item 5 (no `href`/`xlink:href` on SMIL elements) |
| 6 | `discard` removes an element | RESEARCH: definition NOT FOUND in the checked text | Not in the allowlist | Item 5 (fails closed) |
| 7 | HTML integration points parse their children as HTML (`<img onerror>`, `<iframe>`): `foreignObject`, `desc`, `title`, MathML `mi`, `mo`, `mn`, `ms`, `mtext`, and `annotation-xml` with an HTML encoding | WHATWG §13.2.6 (dispatcher, integration points) | M-07 item 3: text and CDATA only; `foreignObject` and `annotation-xml` are not in the allowlist | M-07 item 3 |
| 8 | Breakout tags leave the foreign region (`div`, `p`, `img`, `font` with `color`/`face`/`size`, `</br>`, `</p>`) | WHATWG §13.2.6.5 | M-07 items 1 and 2 (`BREAKOUT` always includes `font`) | Same |
| 9 | CDATA sections in foreign content end at `]]>`; a token reader can end them at `>` | WHATWG tokenizer; `markup.md` M-12 | M-12, M-07 item 5 | Same |
| 10 | `use href` loads or clones content | SVG 2 §5.5: no cross-origin and no `data:` URL requests | Item 4 (`data:`); browsers allow same-origin only | Same |
| 11 | `image` and `feImage` `href` load a resource (tracking, as an HTML `img` does) | SVG 2 §16.1.5 | Not in the allowlist | Not covered (same as an HTML `img`) |
| 12 | An SVG `style` element applies a style sheet to the whole document (overlay, `@import`, data leaks through attribute selectors) | SVG 2 §6.2, §6.4 | Not in the allowlist | M-22 item 5 |
| 13 | MathML `href` (MathML 3; not in MathML Core) | MathML Core: NOT FOUND (no `href` defined) | §4.2 item 4 | §4.2 item 4 |
| 14 | `maction` `actiontype` | MathML Core §3.6: no observable behavior defined | Not in the allowlist | No script; allowed |

**FACT.** The attribute walk of §4.2 reads every start tag of a foreign region in block content:

- `passesAttributeRule` changes the tokenizer state after `title`, `style` and similar tags, also in a foreign region (M-04 does not).
- But M-07 items 3 and 4 allow only text in those elements, so the walk reads the same start tags.
- Evidence: `attr-walk` found 0 mismatches on all valid R1 results of section 3.1.

### 4.2 What the rules do not cover

These stay outside the rules in both options, as for HTML content (`manifest-patch.md` §4 "Not covered"):

- the `style` attribute and presentation attributes with `url(…)` (`fill`, `filter`, `mask`, `clip-path`, `marker-*`): CSS and resource references, no script;
- `use href="#id"` and paint servers that reference page elements by id: they render page content inside the block (DOM-clobbering class);
- same-origin external `use` resources. RESEARCH: whether a referenced resource document can run script is NOT FOUND. SVG 2 §5.5 forbids cross-origin and `data:` requests;
- `xml:base`. RESEARCH: browser support NOT FOUND. It can change how a URL resolves, but not the scheme check of item 4;
- new SVG or MathML features after 2026-10. Option B needs rule updates for them; option A fails closed.

The Content Security Policy of the page (SEC-04) is the second line of defense for all of them.

## 5. Proposed rule text (not applied)

These are diffs of `docs/engineering/protocol/markup.md`, and for R1 also of §4 of `manifest-patch.md`. They are proposals for the decision. The architect assigns the decision IDs (H-71 and later).

### 5.1 R1, option A (recommended): balanced subtree with an element allowlist

```diff
 **Name sets.** The rules use these sets of names:
-- `FORBIDDEN_CONTENT`: `base`, `basefont`, `bgsound`, `body`, `embed`, `frame`, `frameset`, `head`, `html`, `iframe`, `link`, `math`, `meta`, `noembed`, `noframes`, `noscript`, `object`, `optgroup`, `option`, `plaintext`, `script`, `select`, `style`, `svg`, `template`, `textarea`, `title`, `xmp`.
+- `FORBIDDEN_CONTENT`: `base`, `basefont`, `bgsound`, `body`, `embed`, `frame`, `frameset`, `head`, `html`, `iframe`, `link`, `meta`, `noembed`, `noframes`, `noscript`, `object`, `optgroup`, `option`, `plaintext`, `script`, `select`, `style`, `template`, `textarea`, `title`, `xmp`.
+- `FOREIGN_ALLOWED` (names as the tokenizer gives them, in lower case): `svg`, `g`, `path`, `circle`, `ellipse`, `line`, `polyline`, `polygon`, `rect`, `defs`, `symbol`, `use`, `title`, `desc`, `lineargradient`, `radialgradient`, `stop`, `clippath`, `mask`, `pattern`, `marker`, `text`, `tspan`, `filter`, `feblend`, `fecolormatrix`, `fecomponenttransfer`, `fecomposite`, `fedisplacementmap`, `fedropshadow`, `feflood`, `fefunca`, `fefuncb`, `fefuncg`, `fefuncr`, `fegaussianblur`, `femerge`, `femergenode`, `femorphology`, `feoffset`, `feturbulence`, `math`, `mrow`, `mi`, `mo`, `mn`, `ms`, `mtext`, `mspace`, `msub`, `msup`, `msubsup`, `mfrac`, `msqrt`, `mroot`, `mstyle`, `munder`, `mover`, `munderover`, `mmultiscripts`, `mprescripts`, `none`, `mtable`, `mtr`, `mtd`, `mpadded`, `mphantom`, `merror`, `semantics`, `annotation`.
```

```diff
 **M-07 Foreign regions.** In a foreign region:
 …
-6. **End of input.** … A block cannot be open at that point: a block start tag in a foreign region is invalid (M-16), and the content of an `html` block cannot contain `svg` or `math` (M-22).
+6. **End of input.** … A block start tag in a foreign region is invalid (M-16). When the region started in the content of a block, the block has no block end tag (M-20).
```

```diff
 **M-22 Allowed tokens.** The content MUST NOT contain:
 
 1. a DOCTYPE token;
-2. a start tag with a name in `FORBIDDEN_CONTENT`;
-3. a start tag with the self-closing flag whose name is not in `VOID` (`<span/>`);
-4. an end tag with a name in `VOID` (`</br>`, `</img>`).
+2. a start tag in HTML context with a name in `FORBIDDEN_CONTENT`;
+3. a start tag in HTML context with the self-closing flag whose name is not in `VOID` and is not `svg` or `math` (`<span/>`);
+4. an end tag in HTML context with a name in `VOID` (`</br>`, `</img>`);
+5. a start tag in a foreign region whose name is not in `FOREIGN_ALLOWED`.
+
+An `svg` or `math` start tag in the content starts a foreign region (M-05 item 4). M-07 applies in it, and the region ends in the content (M-23). (Human decision R1.)
 
-Comments, bogus comments, character references and all other elements are allowed. Attributes are not checked: …
+Comments, bogus comments, character references and all other elements are allowed. Attributes are not checked: … The client attribute rule reads the start tags of a foreign region too ([manifest and patch §4.2](/engineering/protocol/manifest-patch#_4-2-attribute-rule)).
```

```diff
-**M-23 Balance.** In the content, a start tag with a name not in `VOID` pushes an element (M-05). Every end tag MUST have the name of the current node, and pops only that node.
+**M-23 Balance.** In the content, a start tag pushes an element as M-05 items 1 and 2 say (in a foreign region, every start tag without the self-closing flag). Every end tag MUST have the name of the current node, and pops only that node.
```

```diff
 **M-24 No implied end tags.** The WHATWG tree builder closes some elements implicitly when it reads some start tags. In the content, these start tags MUST NOT be used …
+These items apply to start tags in HTML context. A start tag in a foreign region makes a foreign element; it closes no element (M-07 item 1 keeps the breakout tags out).
```

`manifest-patch.md` §4:

```diff
-1. `html` content MUST NOT contain `<script>`, `<iframe>`, `<object>`, `<embed>`, inline event handlers (`on*` attributes), or an attribute value that starts with `javascript:`, `vbscript:` or `data:` after URL normalization (section 4.2; H-59, DL-036).
+1. `html` content MUST NOT contain `<script>`, `<iframe>`, `<object>`, `<embed>`, an SVG or MathML element outside `FOREIGN_ALLOWED` (markup M-22 item 5), inline event handlers (`on*` attributes), or an attribute value that starts with `javascript:`, `vbscript:` or `data:` after URL normalization (section 4.2; H-59, DL-036).
```

```diff
 ### 4.2 Attribute rule
-… The client MUST apply this rule to every start tag in the content of each patched `html` block, as the tokenizer of the analysis of section 4.1 reads it …
+… The client MUST apply this rule to every start tag in the content of each patched `html` block, also in a foreign region (`svg`, `math`), as the tokenizer of the analysis of section 4.1 reads it …
```

In `markup.md` §5 item 3, add the measured gain of R1. Keep the example "an `svg` with HTML in `foreignObject`": this study does not propose to allow HTML in integration points.

### 5.2 R1, option B: balanced subtree, `script` and `style` forbidden, two more attribute items

As option A, with these changes:

```diff
-5. a start tag in a foreign region whose name is not in `FOREIGN_ALLOWED`.
+5. a start tag in a foreign region with the name `script` or `style`.
```

There is no `FOREIGN_ALLOWED` set. `manifest-patch.md` §4.2 gets two items after item 4:

```diff
+5. In a foreign region (markup M-05 item 4), on a start tag `animate`, `animatemotion`, `animatetransform`, `animatecolor`, `set` or `discard`: if the tag has an attribute `href` or `xlink:href`, reject (the animation would target an element outside the block). If the tag has an attribute `attributename` whose copy (item 3), with trailing bytes `00` to `20` removed and in ASCII lower case, contains `href` or starts with `on`, reject.
+6. For an attribute `values` in a foreign region: split the copy (item 3) at each `;`. Apply item 3 to each part and reject when a part starts with a scheme of item 4.
```

### 5.3 R2: `svg` in `noscript` content

```diff
 **M-11 Noscript.** …
-3. C MUST NOT contain a start tag named `frameset`, `math`, `noscript`, `plaintext`, `select`, `svg` or `template`.
+3. C MUST NOT contain a start tag named `frameset`, `math`, `noscript`, `plaintext`, `select` or `template`.
 4. …
-5. C MUST be balanced (human decision H-27). Use a separate stack for C, which starts empty. A start tag that M-05 item 1 would push opens an element of C. Each end tag in C …
+5. C MUST be balanced (human decision H-27). Use a separate stack for C, which starts empty. A start tag that M-05 item 1 (HTML context of C) or item 2 (foreign region of C, item 7) would push opens an element of C. Each end tag in C …
 6. …
+7. **Foreign region of C** (human decision R2). An `svg` start tag in HTML context of C without the self-closing flag starts a foreign region of C. The region ends when that `svg` element is closed. In it, a start tag never changes the tokenizer state (M-04), and M-07 items 1 to 5 and M-12 apply to the tokens of C. Items 2 to 4 and 6 apply in it too.
```

In `markup.md` §5 item 2:

- Old: "Items 2 and 3 keep blocks and the tags that change the parser mode (`frameset`, `math`, `noscript`, `plaintext`, `select`, `svg`, `template`) out of C".
- New: "… (`frameset`, `math`, `noscript`, `plaintext`, `select`, `template`) out of C; an `svg` subtree in C follows the foreign-region rules (item 7)".

The head case goes with the head rule of T-02: with scripting disabled, an `svg` in a head `noscript` ends the head.

### 5.4 R3: no mark at the region's own end tag

```diff
 4. **Special-crossed elements.** …
-   - After an end tag pops elements, the new current node (if there is one) gets the mark when (a) an element that the end tag crosses was pushed in HTML context and has a name in `SPECIAL`, or was pushed in a foreign region, or (b) a popped element (matching or crossed) has the mark. …
+   - After an end tag pops elements, the new current node (if there is one) gets the mark when (a) an element that the end tag crosses was pushed in HTML context and has a name in `SPECIAL`, or was pushed in a foreign region and the end tag is not the end tag of the `svg` or `math` element of that region (human decision R3), or (b) a popped element (matching or crossed) has the mark. …
```

In the FACT note of item 4, change the last sentences to this text:

> The mark also covers crossings over foreign elements by an end tag other than the end tag of the region root. The tree builder pops these elements too (M-07). That part is stricter than needed and was decided by the human (H-32). The end tag of the region root pops every element of the region in the tree builder ("any other end tag" in foreign content). So its crossing leaves nothing open and does not mark (R3).

The depth counter does not change: the crossing still makes the new current node dirty (M-26 item 5).

If R1 is also adopted, add to M-23: "Exception (R3): in a foreign region, the end tag of the `svg` or `math` element of the region can cross elements of the region." If R2 is also adopted, add the same exception to M-11 item 5, for the foreign region of C.

In `markup.md` §5 item 3:

- Old: "so a common icon shape … is invalid; `<path/>` or `<path></path>` is valid … after a parse5 study".
- New: "the icon shape `<a href="#"><svg><path d="M0"></svg></a>` is valid (R3)".

## 6. Options for the human

R1 is a security decision. R2 and R3 are protocol decisions. All three belong to the human.

### 6.1 R1 (security)

| Option | Gain (small / main variants, denominator A) | Risk | Cost |
|---|---|---|---|
| **A. Adopt with limits: element allowlist (recommended)** | +191 / +101 (56.1 % → 81.8 %, 14.0 % → 35.0 %) | Smallest surface: patch content gets no `a`, SMIL, `foreignObject`, `image`, `script` or `style`. Items 1 to 4 of §4.2 cover the attributes. New SVG features fail closed | One name set in the four candidates, the reference and the demo; golden fixtures. Compared to B, it loses 4 small and 7 main variants (spinner animations, one `image`) |
| B. Adopt: `script` and `style` forbidden, attribute items 5 and 6 | +195 / +108 (82.3 %, 36.5 %) | SVG navigation (`a`) and SMIL stay. Safety depends on items 5 and 6 being complete. Every new SVG feature needs a review | Two attribute items in the client, the server and the reference; fixtures |
| C. Reject | 0 | None new | 195 small and 108 main variants stay invalid (26 % and 22.5 % of the variants) |

### 6.2 R2 (protocol)

| Option | Gain | Risk | Cost |
|---|---|---|---|
| **A. Adopt, with the head case decided by T-02 (recommended)** | +15 pages (90.5 % → 92.2 %) | None for the client view: with scripting enabled, C is text, never in a block, never patched. With scripting disabled: 0 disagreements in body. In head, the `svg` ends the head, as any non-head tag does today (H-42 head, T-02) | Foreign-region tracking on the C stack in the four candidates; fixtures. If T-02 makes other content of a head `noscript` invalid, `https://eda.ru/` stays invalid (14 pages gained) |
| B. Adopt for body only, with a new head/body rule in this item | +14 pages | As A | Needs a head tracker in the markup rules; duplicates T-02 |
| C. Reject | 0 | None | 15 pages stay invalid |

### 6.3 R3 (protocol)

| Option | Gain | Risk | Cost |
|---|---|---|---|
| **A. Adopt for the region root's end tag (recommended)** | +7 pages (90.5 % → 91.3 %); with R2, +22 pages (93.0 %) | None measured: the tree builder pops the same elements (WHATWG §13.2.6.5 "any other end tag"); 0 findings | One condition in the mark rule of the four candidates. With R1 or R2, one exception in M-23 or M-11 item 5. Fixtures |
| B. Adopt for every end tag in a foreign region (scope "region") | +7 pages (no more than A); more generated shapes | Same argument as A; a wider exception in M-23 and M-11 item 5 | As A |
| C. Reject | 0 | None | 7 pages stay invalid |

Not proposed: a change of the depth counter for these crossings. **FACT.** Each icon crossing still leaves its units in D. In `test/relax.test.mjs`, 130 × `<span><svg><path></svg></span>` gives `RQP_MARKUP_LIMIT` in every combination. But M-26 rejects no corpus page or variant because of icon crossings.

### 6.4 Combined

With option A for all three relaxations:

| Measure | Before | After |
|---|--:|--:|
| Pages | 798 | 820 (93.0 %) |
| Small variants | 416 | 607 (81.8 %) |
| Main variants | 67 | 168 (35.0 %) |

The parse5 cross-check, the tree checks and the fragment check find no disagreement on any gained document. The only findings are the H-42 head findings that T-02 studies.

## 7. Facts and sources

| Fact | Source | Checked |
|---|---|---|
| The breakout start tags of foreign content are `b`, `big`, `blockquote`, `body`, `br`, `center`, `code`, `dd`, `div`, `dl`, `dt`, `em`, `embed`, `h1`–`h6`, `head`, `hr`, `i`, `img`, `li`, `listing`, `menu`, `meta`, `nobr`, `ol`, `p`, `pre`, `ruby`, `s`, `small`, `span`, `strong`, `strike`, `sub`, `sup`, `table`, `tt`, `u`, `ul`, `var`, and `font` with `color`, `face` or `size`. They pop to an HTML element or integration point, and the token is processed again | WHATWG HTML Living Standard §13.2.6.5 "The rules for parsing tokens in foreign content" (page "Last Updated 16 October 2026") | 2026-10-18 |
| An end tag `br` or `p` in foreign content pops to an HTML element or integration point and is processed again | WHATWG §13.2.6.5 | 2026-10-18 |
| An end tag `script` with an SVG `script` as the current node processes the SVG script. A self-closing `script` start tag in SVG does the same | WHATWG §13.2.6.5 ("Any other start tag", "An end tag whose tag name is script, if the current node is an SVG script element") | 2026-10-18 |
| "Any other end tag" in foreign content walks down through non-HTML elements and pops up to the element with the same lower-case name. At an HTML element, it uses the HTML insertion mode | WHATWG §13.2.6.5 | 2026-10-18 |
| An `svg` or `math` start tag in body reconstructs the active formatting elements and inserts a foreign element. It pops the element when the self-closing flag is set | WHATWG §13.2.6.4.7 | 2026-10-18 |
| The tree construction dispatcher; MathML text integration points `mi`, `mo`, `mn`, `ms`, `mtext`; HTML integration points: `annotation-xml` with `encoding` `text/html` or `application/xhtml+xml`, and SVG `foreignObject`, `desc`, `title` | WHATWG §13.2.6 "Tree construction" | 2026-10-18 |
| The SVG `script` element "is the place for scripts" | SVG 2, W3C Candidate Recommendation Snapshot 06 October 2026, §15.9 | 2026-10-18 |
| SVG event attributes (`onbegin`, `onend`, `onrepeat`, global `on…`) follow "the same requirements as for event handler content attributes" | SVG 2 §15.8 | 2026-10-18 |
| SVG `a` "forms a link if it has a href or xlink:href attribute". Activation follows the HTML navigate algorithm. `xlink:href` is deprecated and is used when `href` is absent. The `href` of `a` is animatable | SVG 2 §16.2, §16.1.7.1, §16.1.6 | 2026-10-18 |
| On animation elements, `href` allows only same-document URL references | SVG 2 §16.1.7.2 | 2026-10-18 |
| `use`: "this specification does not allow cross-origin and data URL resource requests" | SVG 2 §5.5 | 2026-10-18 |
| The SVG `style` element is processed as the HTML `style` element. In an HTML document, style sheets apply to all HTML and inline SVG content | SVG 2 §6.2, §6.4 | 2026-10-18 |
| The animation elements are `animate`, `animateMotion`, `animateTransform`, `set`. `href` names the target element (default: the parent). `attributeName` names the target attribute or property. `values` is split at `;`. URL values can be animated with `animate` and `set` | SVG Animations Level 2, Editor's Draft 14 October 2026, §2.2, §2.5, §2.6, §2.10 and the data-type table | 2026-10-18 |
| All MathML elements support event handler content attributes | MathML Core, W3C Candidate Recommendation Snapshot 24 June 2025, §2.1.3, §2.1.4 | 2026-10-18 |
| MathML Core "does not define any observable behavior that is specific to the actiontype and selection attributes" of `maction` | MathML Core §3.6 | 2026-10-18 |
| An `href` attribute on MathML elements | NOT FOUND in MathML Core (it is a MathML 3 feature; browser support NOT FOUND) | 2026-10-18 |
| The `discard` element (definition and behavior) | NOT FOUND in the checked text of SVG Animations Level 2 and SVG 2 | 2026-10-18 |
| Browser support of `xml:base`; script in same-origin external `use` resources; SMIL animation of event attributes in browsers | NOT FOUND. The proposed rules fail closed for SMIL; the others are in section 4.2 | 2026-10-18 |
| parse5 8.0.1 results, corpus and sweep numbers | Measured with the tools of `tools/rqp-markup/study/relax/` | 2026-10-18 |
