# S5 study: `template` and `noscript` in block content (T-02)

<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- SPDX-FileCopyrightText: The Rivqen Authors -->

Evidence for WP-17 S5, task T-02 (sprint record `docs/engineering/plan/sprints/WP-17-S5.md`). Measured on 2026-10-19 on base `270278c`. Tools: `tools/rqp-markup/study/s5/` (README with the commands). Machine results: `tools/rqp-markup/study/s5/results/` (aggregates, rule labels, synthetic examples, fixture and case names only). The study lane could not write this file; the architect wrote it from the lane's report text.

**Marks.** FACT: measured, or read in the cited source on that date. DESIGN: a proposal or an argument. RESEARCH: an inference. NOT FOUND. No rule changed in this study; the human decides from the options of §6.

## 1. Question

M-22 item 2 rejects `template` and `noscript` in `html` block content (H-78, "until a study"). Which rules make each element safe there for the token stack (M-05), the state changes (M-04) and the depth counter D (M-26)? Do the results agree with parse5 8.0.1 in both scripting views and in a fragment parse with the block element as the context? What must the patch content rule add? What is the corpus cost and gain?

## 2. Method

Invariants, checked for each valid result of each option:

- I1. With all option flags off, the study copy gives the results of the pre-S5 candidate (`baseline/baseline-s4.json`, recorded at `270278c`) on the 311 golden fixtures, the 441 cases of `270278c`, 1 600 documents of the four cross-check profiles and 6 000 documents of the S5 generator profiles.
- I2. Monotonic: an option accepts every document that a smaller one accepts, with the same blocks.
- I3. parse5 agrees in the views `enabled` and `disabled` (scripting on and off) and in the view `spec` (scripting on, WHATWG table scope, §5): the same blocks; no manifest-like script; each start tag of the content makes an element in the block, in the namespace and with the parent of the token stack (through template contents to the template); each template ends at the end tag that the token pass found; with scripting off, each `noscript` ends at its end tag and the tags of C are inside it.
- I4. Fragment parse (parse5 `parseFragment`, the block element as the context, scripting on): the same start tags make elements with the same namespace and parent.
- I5. The M-26 claim (stack ≤ D+2, open depth ≤ D+2, closed depth ≤ D+3) after each token while no template is open, in both views. The same bounds inside open templates are reported apart.
- I6. Patch apply: each html block whose content a patch-rule variant accepts is parsed as a fragment in the mode "inert" (`innerHTML`, `insertAdjacentHTML`, `setHTMLUnsafe` in a scripting-on document; `noscript` content is text) and in the mode "disabled" (`DOMParser`, `parseHTMLUnsafe`, or `innerHTML` with scripting off; `noscript` content is markup). Every element of either parse must come from a start tag that the rule read.

Known classes are counted apart (`known-*`): the shipped rules accept them and they do not change block bytes (a `p` closed by a `PCLOSE` start tag, M-24 item a covers only `p` and `span` blocks; a formatting element reconstructed from before the block, markup §5 item 1; a head `noscript` with scripting off, H-72). Controls show that the oracle reaches the shapes that the rules remove: `tu` (T1 without the `shadowrootmode` rule and without M-09 condition 1 in the content) and `nu` (N1 without the content rules on C).

Limits of the method: parse5 is one implementation; it has no declarative shadow DOM (DSD), no `template for`, and the table-scope difference of §5. The generators reach only the shapes that they contain. No browser was run (NOT FOUND; the WebView lab, §7).

## 3. Token model

### 3.1 `template` in block content

- **M-04.** In template contents the state changes are those of the body. "in template" sends `script`, `style`, `link`, `meta`, `base` and `template` to "in head"; other start tags go to "in body" or to a table mode. Exception: `<template><col>` ("in column group" with the template as the current node ignores later start tags); M-09 condition 1 already rejects it, also in content. FACT: WHATWG "in template", "in table", "in column group", fetched 2026-10-19 ("Last Updated 19 October 2026").
- **M-05 and M-23.** The template is pushed and M-23 requires an exact `</template>`. The tree builder's `</template>` generates implied end tags thoroughly, pops to the template, clears the active formatting elements to the marker and resets the insertion mode. So nothing inside a template can move a block boundary. `template` is a boundary of every scope and its marker hides outer formatting elements, so M-24 items a to f are stricter than needed inside a template (DESIGN; example: `<p data-rq-block><template><div>` is rejected by M-24 a, but the tree builder does not close the `p`).
- **Table modes in a template.** The F search of M-09 stops at `template`. A table part at the top of template contents switches the template insertion mode to a table mode, and the tree then differs from the token stack: implied `tbody`/`tr` are not counted by D (example `<template><caption></caption><td>`: stack 11 > D 7 + 2); a `<table>` start tag is ignored (no table in table scope); a `<form>` is inserted and popped at once (parse5 ignores it). Block boundaries do not change, but elements and parents in the contents do (T1 findings, §4). Option T2 removes table modes in templates of block content.
- **M-26.** No change: the template counts 1 and an exact pop releases it. With T2, the bounds also hold inside templates of block content in the sweep.
- **M-10 item 3 (`for`), M-19, M-17:** no change; a block start tag in template contents is NESTED.
- **`shadowrootmode` (DSD).** FACT (T-01 §2): the full-document parser attaches a shadow root to the host and the template becomes the shadow root; `innerHTML` attaches none; `setHTMLUnsafe` and `parseHTMLUnsafe` do. RESEARCH (spec reasoning; parse5 has no DSD): (1) a block element that is a DSD host keeps its shadow root when `innerHTML` replaces the light DOM, so a later patch is not rendered; (2) a DSD host inside the content is attached in the full document and in the next cold load of the stored snapshot, but not after an `innerHTML` patch: the same bytes give two renderings. So the rule must be a markup rule (full and patched document), not only a patch rule.

### 3.2 `noscript` in block content

- **Scripting on** (WebView: Normal; `innerHTML`: Inert): C is RAWTEXT up to `</noscript`, as M-04 says; the `noscript` element is an exact M-23 element.
- **Scripting off** (Disabled: crawlers, `DOMParser`, a WebView with JavaScript off, Q-19): C is markup at its place in the block content. M-11 items 1 to 9 make C end exactly at `</noscript>`, balanced, without block tags and without `template`, `select`, `math`, `noscript`, `plaintext`, `frameset`, `html`, `body`, `head`; table parts in C need a table of C; D counts C (item 8). New in a block: the tree builder applies the content behaviors to C, so M-22 to M-24 must hold for the start tags of C (N1). Without this (control `nu`), `<p data-rq-block><noscript><div>` closes the block in the scripting-off view: 462 block disagreements.
- **Known class.** With N1, and outside blocks today: a `PCLOSE` start tag in C closes a `p` around the `noscript` in the scripting-off view; parents change only. N2 removes it in block content.
- RESEARCH: the scripting-off `noscript` start tag reconstructs formatting elements and the raw-text path does not, so the parent of a `noscript` can differ between views after a reconstruction (markup §5 item 1).

### 3.3 Combinations

`noscript` in a template in block content is valid with T2 + N1; its scripting-off tree is in the inert template contents. `template` in C stays invalid (M-11 item 3).

## 4. Results

### 4.1 Sweeps

FACT: 500 789 documents (311 fixtures, 478 cases including the 37 of `crosscheck/cases/s5-cases.json`, 500 000 generated: classic, review, review2, review3 at 50 000 each; tpl, nos, mix at 100 000 each; seed 5); 0 monotonic violations.

| Combination | Valid | Gained | Documents with a finding | Finding kinds |
|---|--:|--:|--:|---|
| base | 63 995 | 0 | 0 | — |
| T1 | 71 709 | 7 714 | 9 | In template contents: missing `table`/`form` elements, parent differences, template end in parse5 |
| T2 | 71 675 | 7 680 | 0 | — |
| N1 | 79 078 | 15 083 | 0 | — |
| N2 | 78 708 | 14 713 | 0 | — |
| T1 + N1 | 93 030 | 29 035 | 15 | As T1 |
| **T2 + N1** | **92 973** | **28 978** | **0** | — |
| T2 + N2 | 92 452 | 28 457 | 0 | — |
| `tu` (control) | 74 073 | 10 078 | 1 619 | 1 609 block disagreements (H-35 in a template of block content) |
| `nu` (control) | 81 412 | 17 417 | 475 | 462 block disagreements with scripting off |

FACT: the M-26 claim had 0 violations in both views on all valid documents of all combinations. Inside open templates (outside the claim) the bound was exceeded in 2 base documents and 2 to 3 documents of T2, T2 + N1, T2 + N2, all in templates outside block content (shipped behavior); T1 had 7 and T1 + N1 10, of which 4 and 6 are in block content. Fragment parse: 0 findings for T2, N1, N2, T2 + N1, T2 + N2. Patch apply: with the shipped patch rule and N1, the disabled parse makes 25 009 elements that the rule never read (the `noscript` mutation XSS class of T-01 §6); with P1 or P2, 0 unread elements in both modes; P1 gives the same elements in both modes, P2 gives 6 673 mode differences (C is text in one mode and elements in the other).

### 4.2 Corpus

FACT (864 pages fetched for S5 under H-92, read from the scratchpad, deleted after S5): 809 pages valid in every combination (block content options do not change pages without blocks).

| Variant | base | T1 = T2 | N1 = N2 | T2 + N1 (= T1 + N1 = T2 + N2) | Lost |
|---|--:|--:|--:|--:|--:|
| small (752) | 687 (91.4 %) | 699 | 689 | 701 (93.2 %) | 0 |
| main (490) | 332 (67.8 %) | 358 | 367 | 393 (80.2 %) | 0 |

- 0 findings on all gained variants (enabled, disabled and spec views, fragment, depth). With the shipped patch rule, C is unread in 8 gained main variants; P1 and P2 give 0.
- Census (start tags that the blocks reach before the first violation, no S5 rule): main: 37 variants with 205 `template` tags (61 with `shadowrootmode`, 0 with a table part as the first child) and 53 variants with 577 `noscript` tags (C: 556 only void-like elements such as `img` and `iframe`, 9 text, 12 other markup); small: 12 variants with 30 `template` tags, 4 variants with 60 `noscript` tags. So T2 costs 0 variants against T1, and N2 costs 0 against N1.
- The `shadowrootmode` rule costs 2 main variants (`tu` 360 against T1 358).
- Remaining failures of main variants under T2 + N1: M-22 item 2 (other names) 33, M-23 29, LIMIT 14, `shadowrootmode` 5, M-24 c 4, M-24 d/e/f 4, M-24 j 3, M-22 item 3 2, M-22 item 1 2, M-24 k 1.
- Blocks that a patch cannot carry, with P1: small 98 of 687 (14.3 %) → 106 of 701 (15.1 %); main 178 of 332 (53.6 %) → 228 of 393 (58.0 %). With P2: 105 (small) and 220 (main).

### 4.3 Fixtures and cases that change

Under T2 and N1 these become valid: `FX-RQ-MARKUP-CONTENT-FORBIDDEN-TEMPLATE`, `FX-RQ-MARKUP-CONTENT-FORBIDDEN-NOSCRIPT`, and the cases `S4-H78-kept-template` and `S4-H78-kept-noscript`. No other golden fixture or case changes.

## 5. Sources

- FACT: WHATWG HTML, Parsing (`https://html.spec.whatwg.org/multipage/parsing.html`, "Last Updated 19 October 2026", fetched 2026-10-19 through a summarizing fetch tool): "has an element in table scope" stops at `html`, `table` and `template`; "in table", start tag `table`: ignored when no `table` is in table scope; "in table", start tag `form`: inserted and popped when the parser is parsing template contents; "in template": `caption`, `colgroup`, `tbody`, `tfoot`, `thead` → "in table", `col` → "in column group", `tr` → "in table body", `td`, `th` → "in row".
- FACT (code read): parse5 8.0.1 `dist/parser/open-element-stack.js`, `hasInTableScope` and `hasTableBodyContextInTableScope` stop only at `table` and `html`, not at `template`. In `<td><template><tfoot></tfoot><table>…</template>`, parse5 pops out of both templates to the outer table; with the WHATWG scope (study view `spec`) the `<table>` is ignored and the templates end in place. parse5 `formStartTagInTable` ignores `<form>` when a template is on the stack. `dist/` has no `shadowrootmode` (0 matches): parse5 has no DSD.
- FACT (T-01, `research-t01.md`): DSD in the full document and in `setHTMLUnsafe`/`parseHTMLUnsafe`, not in `innerHTML`; `DOMParser` and `parseHTMLUnsafe` parse `noscript` content as markup; the `noscript` mutation XSS class.
- NOT FOUND: a browser or WebView run of these shapes; the DSD effects are spec reasoning only.

## 6. Options

| Option | Rule text proposal (DESIGN) | Corpus gain | parse5 findings (500 789 documents) | Patch content rule | Risks |
|---|---|---|---|---|---|
| T0 | Keep `template` in the forbidden names of M-22 item 2 | — | 0 | none | 26 main and 12 small variants stay invalid |
| T1 | Remove `template` from M-22 item 2. New M-22 item 6: "a start tag `template` in HTML context with an attribute named `shadowrootmode` (ASCII case-insensitive; any value or none; every attribute counts, also a duplicate that the tokenizer drops)" is invalid | +26 main, +12 small | 9 (15 with N1), in template contents | none: the rule already reads template contents as tokens; the patched-document analysis (step 8) rejects `shadowrootmode`; M-10 item 3 rejects `for` | Table modes in templates are where parse5 differs from WHATWG |
| **T2 (recommended)** | T1, and M-24 new item g: a start tag with a name in `TABLE_PART` or `col` is invalid in the content when no `table` opened in the content is open, **or when F (M-09) is none**; the F search stops at `template`, so a table part in a template needs a `table` opened in that template | as T1 (0 corpus templates start with a table part) | **0** | as T1 | Rejects row templates `<template><tr>…` in blocks (0 in the corpus); M-24 a–f stay stricter than needed in templates (a later relaxation is compatible) |
| N0 | Keep `noscript` in the forbidden names of M-22 item 2 | — | 0 | none | 35 main and 2 small variants stay invalid |
| **N1 (recommended)** | Remove `noscript` from M-22 item 2. New M-11 item 10: "When the `noscript` element is in the content of an `html` block, the pass of item 8 also applies M-22 to M-24 to the start tags of C, as if they came in the content at the place of C. Item 5 is M-23 for C." M-26 item 7 unchanged | +35 main, +2 small | **0** (control without item 10: 475 documents, 462 block cuts with scripting off) | P1 | Known class: a `PCLOSE` tag in C closes a `p` around the `noscript` with scripting off (parents only, as outside blocks today) |
| N2 | N1, and: in C of a `noscript` in block content, a start tag in `PCLOSE` is invalid while an HTML `p` is on the token stack below the `noscript` element and above the nearest `template` | as N1 | 0 | P1 | Removes the known class; costs 270 generated documents and 0 corpus variants; stricter than C outside blocks |
| **P1 (recommended with N1 or N2)** | Manifest-patch §4 item 1.1: `PATCH_FORBIDDEN` adds `noscript`. A full document can have it; a patch cannot | — | Patch apply: 0 unread elements; both modes give the same elements | — | 8 main variants could carry patches with P2 |
| P2 | `noscript` allowed in patches: the patch rule also reads the start tags of C (tokenized as M-11 does) with items 1.1, 1.2 and §4.2 | +8 main variants patchable | 0 unread elements; 6 673 mode differences | — | The page depends on the apply method (`DOMParser` makes C live: images load); more client code |

**Recommendation (DESIGN):** T2 + N1 + P1. 0 findings in 500 789 documents (both scripting views, the WHATWG table-scope view, the fragment parse, the depth claim); 0 lost variants; main variants 332 → 393 (67.8 % → 80.2 %), small 687 → 701; patch apply is the same under `innerHTML` and `DOMParser`/`parseHTMLUnsafe`. T2 costs nothing against T1 in the corpus and removes all T1 findings. N2 is a free stricter alternative if the scripting-off tree of block content must match exactly. The DSD rule: only `shadowrootmode` (the only trigger in the WHATWG "in head" template step); the prefix form `shadowroot*` is fail-closed for future DSD attributes (its corpus cost is NOT FOUND: not measured).

## 7. Limits and follow-ups

- parse5 8.0.1 is the only tree builder; a browser check is NOT FOUND. Proposed WebView lab cases (T-03, T-04): (1) `<div data-rq-block="a"><template shadowrootmode="open"><slot></slot></template>light</div>`: does the full document attach a shadow root, and does the block element after `innerHTML` keep it; (2) the `noscript` shapes "p around" and "mutation XSS" with scripting off; (3) the table-scope shape (the browser keeps the templates, parse5 does not).
- The table-scope difference of parse5 also affects templates in table modes outside blocks, which the shipped rules allow; the cross-check found no block disagreement there. The study view `spec` (`withSpecTableScope` in `oracle-s5.mjs`) can be reused.
- The H-35 claim exclusion (no template open) still matters for templates outside blocks: 2 generated documents of the shipped rules exceed D+2 inside template contents.
