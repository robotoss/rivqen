# S4 study: implied head (H-79) and implied table closure (H-85)

<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- SPDX-FileCopyrightText: The Rivqen Authors -->

Evidence for WP-17 S4, task T-03 (sprint record `docs/engineering/plan/sprints/WP-17-S4.md`). Measured on 2026-10-18 (session clock; the sprint record is dated 2026-10-19), base `9bd8c32`. Tools: `tools/rqp-markup/study/s4/` (README with the commands). Machine results: the JSON files in this directory. The study lane could not write this file (the agent harness refuses report files); the architect wrote it from the lane's report text.

**Marks.** FACT: measured, or read in the cited source on the given date. DESIGN: a proposal or an argument. This is a study: no rule changed in it.

**Decisions (added by the architect).** Under human decision H-87 (night rules: adopt the recommended option when parse5 shows 0 disagreements in both scripting views and no corpus page is lost), both recommended options are adopted: H-79 option A1 with client rule C2, and H-85 option B2. See the sprint record §1.

## 1. Question

**A (H-79).** Today the server needs an explicit `<head>` start tag (manifest-patch §1.7 "Document head", H-72). Proposal: when item 1 of the walk stops at a start tag `base`, `basefont`, `bgsound`, `link`, `meta`, `noframes`, `script`, `style` or `title` before any head start tag, the insertion point is the `<` of that tag. Checks with parse5 8.0.1, both scripting views: the manifest is the first child of the implied head and the only manifest; the page's next tag stays where the parser put it before (head content, body start, `html`/`head`/`body` attributes unchanged); which exact client rule finds the same position? Corpus gain and loss; a sweep of generated prefixes.

**B (H-85).** M-09 rejects a table part that the tree builder closes implicitly ("close the cell", `<tr>` in `<tr>`, `<td>` in `<td>`). Is there a token-stack model of the closure that keeps the block result equal to parse5 in both views, keeps the depth claim of M-26 (`crosscheck/src/depth.mjs`) and leaves M-23 unchanged inside blocks? Which elements pop, which marks and D units change, and which pages gain?

## 2. Method

- Study copies behind flags; flags off equal the shipped code (tested):
  - `src/head.mjs`: head rules `shipped` / `h79` / `l1` (+ `noscript`, `template`) / `l2` (any start tag); client rules `c1` (walk on F) and `c2` (walk on O = F without the manifest element).
  - `src/markup-s4.mjs` (copy of `tools/rqp-markup/node/src/markup.mjs`), flag `close`: `off` (shipped); `b1` (closure only by exact pops: F is the current node); `b2` (closure = the end tag of F through the shipped end-tag routine: M-05 item 3, M-06 items 1 and 4, M-26 items 4 and 5; rows, row groups and colgroups also need M-09 condition 2); `b3` (b2 plus a D release on cell or caption pops); `u` (control: b2 without M-06 item 1 and without condition 2). In block content and in `noscript` content C, no mode uses the closure.
- Oracles (parse5 8.0.1, scripting enabled and disabled):
  - A: in F, the manifest is the first child node of `html > head`; exactly one manifest-type script (template contents included); the serialization of F without the manifest equals that of O; same DOCTYPE and quirks mode; c1, c2 and the reference `findManifestElements` find the element at the insertion point.
  - B: the cross-check block comparison (`crosscheck/src/compare.mjs`); the M-26 depth claim with the closure applied in `DepthCounter` (`src/depth-s4.mjs`, both views); a table parent check (each table part start tag in a table has the parse5 parent F of the token stack; implied `tbody`/`tr`/`colgroup` skipped); a rewrite check: D′ = D with the implied end tags written as bytes, the shipped candidate accepts D′ with the same blocks (offsets mapped) and parse5 builds the same tree for D and D′ in both views. The control mode `u` shows that these oracles find an unsafe closure.
- Inputs: the 247 golden fixtures (the set before S4 T-02), all cross-check cases, 116 protocol documents (48 full documents); A: 500 000 generated prefixes (seed 79) and 200 000 hostile documents (seed 79: a manifest at every token boundary up to the end of the stop token, 856 093 positions); B: 500 000 generated table shapes (seed 85) and 200 000 each of the profiles `classic`, `review`, `review2`, `review3` (seed 85). Every valid result of every mode was checked.
- Corpus: the S1 URL list, fetched on 2026-10-18 (862 pages in the corpus); simulated blocks of the S1 method (`small`, `main`). The data was deleted after the run.

## 3. Results

### A (H-79)

- **Corpus, 802 valid pages** (all UTF-8). Shipped: 793 with an explicit head, 9 without. `h79`: 793 explicit + 9 implied = 802; gained 9, lost 0; 0 parse5 or client failures. Gained: theverge.com (2), vox.com (2), xdaforums.com (2), novayagazeta.eu, sephora.com, linux.org.ru. Stop tags: `<meta>` 6, `<script>` 2, `<link>` 1. `l1` and `l2` gain no extra page. After H-79, every valid corpus page has an insertion point.
- **Generated, 500 000 documents** (438 407 valid markup): `h79` 246 423 implied and 27 088 explicit, 0 failures; `l1` 291 974 implied, 0 failures; `l2` 382 876 implied, 0 failures. The suffixes contain `noscript` (img/link/div/meta/text), `template`, text, comments, `<html attrs>`, a late `<head lang>`, `</head>`, `<body attrs>`, `frameset`, blocks and title blocks.
- **Repository documents:** all failures are on markup-invalid documents only. With `h79`, the protocol page `page-no-head-start` gets an implied head at `<title>` (`FX-RQ-P39-NO-HEAD-START-TAG` changes); 0 golden markup fixtures change; all 48 protocol full documents keep their client answer.
- **Client rule, hostile sweep** (200 000 documents, 856 093 positions): `c2` accepted 109 368 positions, every one the server position; 0 parse5 failures. `c1` accepted 624 186 positions; 514 818 of them are not the server position, and 331 985 of those (64 %) change the tree in both views.

### B (H-85)

- **Golden fixtures:** off 86 valid; b1, b2, b3 87. Gained: `FX-RQ-MARKUP-TABLE-NESTING` (`<td>a<td>b`). `FX-RQ-MARKUP-COL-IN-CELL` stays STRUCTURE, by M-06 item 3 (the stray `</tr>` after the closure) instead of M-09. 0 findings. Cross-check cases: no gain, 0 findings.
- **Generated** (off valid | b1 gained | b2 gained | b2 findings | b3 findings | u findings):

  | Input | off valid | b1 | b2 | b2 findings | b3 findings | u findings |
  |---|--:|--:|--:|--:|--:|--:|
  | tables 500 000 | 9 056 | +22 168 | +39 871 | 0 | 0 | 11 369 rewrite-invalid, 297 block disagreements per view |
  | classic 200 000 | 59 989 | +356 | +438 | 0* | 69 depth | 33 rewrite-invalid |
  | review 200 000 | 29 092 | +65 | +83 | 0* | 84 depth | 4 |
  | review2 200 000 | 25 198 | +46 | +56 | 0* | 47 depth | 4 |
  | review3 200 000 | 24 640 | +53 | +64 | 0* | 41 depth | 5 |

  \* Each profile also has 4/1/1/1 table-parent findings that are the same in `off` (shipped baseline: content in a `colgroup`, see §4).
- b1 and b2: 0 parse5 block disagreements, 0 depth violations in either view, 0 rewrite findings.
- **Corpus:** off: M-09 is the first violation on 6 pages. b1 and b2 gain 4 pages (news.ycombinator.com, cyberforum.ru × 2, yaplakal.com); 0 lost, 0 findings; small variants +4, main +0. nodejs.org/api/fs.html and html.spec.whatwg.org/multipage/parsing.html stay invalid (b2 by D > 256; their D comes mostly from unclosed `p` and similar elements outside tables).
- **Depth:** an exact closure gives units back (300 rows of `<tr><td>a<td>b`: max D 5). The claim holds in both views for all b1/b2-valid documents.

## 4. Security analysis

**A.** In "before head", the manifest `<script>` is "Anything else": the parser inserts a head "for a head start tag token with no attributes", switches to "in head" and reprocesses the token; the script runs to `</script>` and the parser returns to "in head". In O, the stop tag is also "Anything else" in "before head": the same implied head, then the same tag in "in head". Before the stop, nothing depends on scripting, and the tokenizer is in the data state at the stop in both O and F. A later `<head>` is "Parse error. Ignore the token" in "in head", in both O and F.

- `meta charset` after the manifest (6 of 9 gained pages) may end after byte 1024; as documented, this has no effect (`Content-Type: text/html; charset=utf-8`, H-66). `base` and a CSP `meta` after the manifest: the manifest is a data block with no URL.
- **The client rule MUST be C2** (the walk on O). C1 (the walk on F) stops at the manifest itself, because `script` is in the list, and then accepts a manifest before a later `<head lang>`, before white space, comments or body content; the tree changes (head attributes are lost; white space and comments move into the head), and the WebView would show a tree that differs from the O that the client checks.
- Fail closed: an old client rejects an implied F (`MANIFEST_INVALID`, normal page). Cost: linear walk, constant memory, as H-72. Not proposed: `l1`, `l2` (0 extra pages; more shapes).

**B.** In "in caption", "in table body" and "in row", the table part start tags share their steps with the `</table>` end tag; in "in cell" they "close the cell" and reprocess; in "in column group", "Anything else" pops the colgroup and reprocesses. Each step is the end tag of F followed by reprocessing, so D and D′ have the same tree. B2 runs the shipped end-tag routine (M-06 item 1 rejects a guarded crossing; M-06 item 4 marks as for an end tag; M-26 item 5 makes the new current node dirty after a non-exact pop), so B2(D) equals shipped(D′) by construction: 0 rewrite findings.

- Why the guard stays (control `u`): `<table><tr><td><a href="#">y<object><td>x</table>z<span data-rq-block="b">a<a href="#">b</a>c</span>` — "close the cell" clears the formatting list only to the object's marker; the `a` is reconstructed after the table, and the `<a>` in the block runs the adoption agency: parse5 has no end tag for the block in either view.
- Why condition 2 stays for rows and groups: "clear the stack back to a table row/body context" pops foster-parented elements silently (the H-37 class).
- M-23 (content) and M-11 items 5 and 8 (C) are unchanged: patch content stays fully explicit. Bounds: at most 3 pops per start tag; linear time; memory unchanged.
- B3 is unsafe: `<th><span><div></span></th>x` (the tree builder ignores `<th>` and `</th>`; stack 4 > D 0 + 2) and `<span><table><tfoot><b></tfoot></table>z` (the foster-parented `b` is reconstructed after the table; stack 4 > D 1 + 2).
- Observations, not in scope: (1) the shipped token stack accepts content in a `colgroup` other than `col`; the tree builder closes the colgroup, blocks agree. (2) The known shape `<tr><table>` closes the outer table in the tree builder. (3) The shipped D keeps 4 units per table whose last row and cell are open at `</table>`: 64 × `<table><tr><td>x</table>` reach `RQP_MARKUP_LIMIT`.

## 5. Proposed rule text

**manifest-patch §1.7, server item 2:** the insertion point is immediately after the `>` of the head start tag of the document, or, for an **implied head** ("Document head"), the `<` of the start tag that ends item 1. The server inserts no byte between that point and the manifest start tag. A document without a head start tag and without an implied head is sent without RQP.

**Client item 3:** the client computes the insertion point P of server item 2 on the original document O (item 5: F without the bytes of the manifest element), with the walk of "Document head". The `<` of the manifest start tag MUST be at offset P of F; else the document has no valid manifest. For a head start tag, P is the byte after its `>`, and white space before the manifest is a gap. For an implied head, P is the `<` of the start tag that ended item 1 in O; in F that tag starts immediately after the `>` of the manifest end tag. A walk on F alone is not sufficient: it stops at the manifest start tag itself.

**"Document head", item 1:** a start tag `base`, `basefont`, `bgsound`, `link`, `meta`, `noframes`, `script`, `style` or `title` (after a gap of white space) ends item 1 with an implied head: the insertion point is its `<`. Any other token (another start tag, also `noscript` and `template`, the end tag `</head>`, `</body>`, `</html>` or `</br>`, or the end of the input), or a gap with another byte, means: no head start tag and no implied head.

**markup M-09, implied closure (H-85):** outside block content and outside the `noscript` content C, when F is not in the list for the start tag, and F is `td`, `th`, `tr`, `tbody`, `thead`, `tfoot`, `caption` or `colgroup`, the start tag first closes F: apply the end tag of F (M-05 item 3) with the rules of an end tag (M-06 items 1 and 4, M-26 items 4 and 5); then find F again and repeat, at most three times. A `table` is never closed. When F is `tr`, `tbody`, `thead`, `tfoot` or `colgroup`, F MUST be the current node (condition 2). So `<td>a<td>b` is read as `<td>a</td><td>b` and is valid; `<td><a href="#">a<td>` is invalid (M-06 item 1). In block content an omitted end tag stays invalid (M-23), and in C, M-09 rejects as before. Check order (§2.10): on each start tag in HTML context, the implied closure of M-09, then M-09.

## 6. Options

- **H-79:** A1 (recommended, adopted) the H-79 list + client rule C2: +9 corpus pages, every valid page gets RQP, 0 findings. A2/A3 (wider lists): 0 extra pages, more shapes. A4: keep H-72. C1 (walk on F only): rejected.
- **H-85:** B2 (recommended, adopted) closure as the end tag of F: +4 corpus pages, +40 512 generated documents, 0 findings, reduces to the shipped rules by rewrite. B1 (exact pops only): the same 4 pages. B0: keep M-09. B3: unsafe.

## 7. Facts and sources

WHATWG HTML Living Standard, `https://html.spec.whatwg.org/multipage/parsing.html` ("Last Updated 16 October 2026"), checked on 2026-10-18:

- "before head": white space "Ignore the token"; comment "Insert a comment"; a head start tag inserts the head; "Anything else" inserts "an HTML element for a "head" start tag token with no attributes", sets the head element pointer, switches to "in head", reprocesses.
- "in head": white space "Insert the character"; a head start tag "Parse error. Ignore the token".
- "in body": `caption`, `col`, `colgroup`, `frame`, `head`, `tbody`, `td`, `tfoot`, `th`, `thead`, `tr` start tags: "Parse error. Ignore the token".
- "in table": `</table>` pops until a table is popped, then "Reset the insertion mode"; the definitions of clear the stack back to a table / table body / table row context.
- "in column group", "Anything else": ignore if the current node is not `colgroup`; else pop, switch to "in table", reprocess.
- "in caption", "in table body", "in row": the table part start tags share steps with the `</table>` end tag; "in cell": the start tags close the cell and reprocess; "close the cell": generate implied end tags, pop until `td`/`th`, clear the list of active formatting elements to the last marker, switch to "in row".
- parse5 8.0.1 is the only tree builder used. Browser confirmation (Chrome, WebKit): NOT FOUND, not run.
