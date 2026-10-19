// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

import assert from "node:assert/strict";
import { test } from "node:test";
import { ALLOWED_F, CLASSES, DepthCounter, MAX_CLOSURES, MAX_D, analyzeDepth, checkNoscriptContent, rawAttrValue } from "../src/depth.mjs";

const b = (s) => Buffer.from(s, "utf8");
const run = (s) => analyzeDepth(b(s));

/** Feed a token list to a counter: "<x>", "<x/>", "</x>". Returns D after each token. */
function feed(tokens, options) {
  const c = new DepthCounter(options);
  const ds = [];
  for (const t of tokens) {
    const m = /^<(\/?)([a-z0-9]+)(\/?)>$/.exec(t);
    assert.ok(m, t);
    if (m[1] === "/") c.endTag(m[2]);
    else c.startTag(m[2], m[3] === "/");
    ds.push(c.d);
  }
  return { c, ds };
}

test("D counts pushes and an exact pop of a clean current node gives the unit back", () => {
  assert.deepEqual(feed(["<div>", "<span>", "</span>", "</div>"]).ds, [1, 2, 1, 0]);
});

test("void elements and self-closing svg and math are not pushed; a self-closing div is", () => {
  assert.deepEqual(feed(["<br>", "<img>", "<svg/>", "<math/>", "<div/>"]).ds, [0, 0, 0, 0, 1]);
});

test("a crossing does not decrease D and marks the new current node dirty", () => {
  const { c, ds } = feed(["<p>", "<span>", "<div>", "</span>", "</p>"]);
  assert.deepEqual(ds, [1, 2, 3, 3, 3]);
  assert.equal(c.stack.length, 0);
});

test("the draft counter fails on span^k div; the dirty mark keeps D at the WHATWG stack size", () => {
  // Draft: D would go back to 2 per repetition, while the browser keeps all 11 elements open.
  const one = ["<span>", "<span>", "<span>", "<div>", "</span>", "</span>", "</span>"];
  assert.deepEqual(feed(one).ds.at(-1), 4);
  const doc = `${"<span>".repeat(10)}<div>${"</span>".repeat(10)}`.repeat(5);
  const a = run(doc);
  assert.deepEqual([a.maxD, a.maxStack, a.violations], [55, 57, []]);
});

test("the dirty mark passes down when a dirty element is popped", () => {
  const { c } = feed(["<div>", "<p>", "<span>", "<i>", "</span>", "</p>"]);
  assert.equal(c.stack.length, 1);
  assert.equal(c.stack[0].dirty, true);
  assert.equal(c.d, 4);
});

test("end tags of html and body never decrease D", () => {
  assert.deepEqual(feed(["<html>", "<body>", "</body>", "</html>"]).ds, [1, 2, 2, 2]);
});

test("an end tag without a match changes nothing", () => {
  assert.deepEqual(feed(["<div>", "</span>", "</div>"]).ds, [1, 1, 0]);
});

test("implied tbody and tr count once per table and come back with the table", () => {
  const cells = ["<table>", ...Array(10).fill(["<td>", "</td>"]).flat(), "</table>"];
  const { ds } = feed(cells);
  assert.equal(Math.max(...ds), 4);
  assert.equal(ds.at(-1), 0);
  assert.deepEqual(feed(["<table>", "<tr>", "</tr>", "<tr>", "</tr>", "</table>"]).ds, [1, 3, 2, 3, 2, 0]);
  assert.deepEqual(feed(["<table>", "<tbody>", "<td>", "</td>", "</tbody>", "</table>"]).ds, [1, 2, 4, 3, 1, 0]);
  assert.deepEqual(feed(["<table>", "<col>", "<col>", "</table>"]).ds, [1, 2, 2, 0]);
});

test("H-85a: the implied closure applies the end tag of a cell, a row and a row group, at most three", () => {
  // Exact pops give the units back: the second td closes the first, the tr closes the cell and the row.
  const { c, ds } = feed(["<table>", "<tr>", "<td>", "<td>", "<tr>", "<td>"]);
  assert.deepEqual(ds, [1, 3, 4, 4, 3, 4]);
  assert.deepEqual(c.stack.map((e) => e.name), ["table", "tr", "td"]);
  assert.equal(c.closures, 3);
  // A tbody start tag closes the cell, the row and the row group (three closures), never the table.
  const g = feed(["<table>", "<thead>", "<tr>", "<th>", "<tbody>"]);
  assert.deepEqual(g.c.stack.map((e) => e.name), ["table", "tbody"]);
  assert.equal(g.c.closures, MAX_CLOSURES);
  // caption and colgroup close; col keeps a colgroup open (an allowed F of col).
  assert.deepEqual(feed(["<table>", "<caption>", "<tr>"]).c.stack.map((e) => e.name), ["table", "tr"]);
  assert.deepEqual(feed(["<table>", "<colgroup>", "<col>", "<col>", "<td>"]).c.stack.map((e) => e.name), ["table", "td"]);
  // Without the closure (the classifier mode) the cells nest.
  assert.deepEqual(feed(["<table>", "<tr>", "<td>", "<td>"], { closure: false }).c.stack.map((e) => e.name), ["table", "tr", "td", "td"]);
});

test("H-85a: the closure happens exactly when F is not in the list of M-09 for the start tag", () => {
  // The table of markup.md M-09, written out here (independent of ALLOWED_F).
  const list = {
    caption: ["table"],
    colgroup: ["table"],
    tbody: ["table"],
    thead: ["table"],
    tfoot: ["table"],
    col: ["table", "colgroup"],
    tr: ["table", "tbody", "thead", "tfoot"],
    td: ["table", "tbody", "thead", "tfoot", "tr"],
    th: ["table", "tbody", "thead", "tfoot", "tr"],
  };
  // A context in which F is the given element and is the current node.
  const context = {
    table: ["<table>"],
    caption: ["<table>", "<caption>"],
    colgroup: ["<table>", "<colgroup>"],
    tbody: ["<table>", "<tbody>"],
    thead: ["<table>", "<thead>"],
    tfoot: ["<table>", "<tfoot>"],
    tr: ["<table>", "<tr>"],
    td: ["<table>", "<tr>", "<td>"],
    th: ["<table>", "<tr>", "<th>"],
  };
  for (const [name, allowed] of Object.entries(list)) {
    for (const [f, tokens] of Object.entries(context)) {
      const { c } = feed([...tokens, `<${name}>`]);
      assert.equal(c.closures > 0, !allowed.includes(f), `<${name}> with F ${f}`);
    }
  }
});

test("H-85a: a closure over other elements is a crossing: D stays, the row is dirty and gets the mark", () => {
  const { c, ds } = feed(["<table>", "<tr>", "<td>", "<div>", "<td>", "</td>", "</tr>"]);
  assert.deepEqual(ds, [1, 3, 4, 5, 6, 5, 5]);
  assert.equal(c.stack.length, 1);
  assert.equal(c.stack[0].dirty, true);
  const m = feed(["<table>", "<tr>", "<td>", "<div>", "<td>"]).c;
  assert.equal(m.stack[1].marked, true);
  assert.equal(m.stack[1].dirty, true);
});

test("H-85a: the closure of a row that is not the current node sets the class of H-37", () => {
  assert.deepEqual(run("<table><tr><td>a</td><b>x<tr><td>c</td></tr></table>").classes, [CLASSES.tableCurrent]);
  assert.deepEqual(run("<table><tr><td>a<b>x<tr><td>c</td></tr></table>").classes, []);
  assert.deepEqual(run("<table><tr><td>a<td>b<tr><td>c</table>").classes, []);
});

test("H-85a: no closure in a template boundary, outside a table family element, or in a foreign region", () => {
  assert.deepEqual(feed(["<table>", "<tr>", "<td>", "<template>", "<td>"]).c.stack.map((e) => e.name), ["table", "tr", "td", "template", "td"]);
  assert.deepEqual(feed(["<div>", "<td>"]).c.closures, 0);
  assert.deepEqual(feed(["<table>", "<tr>", "<td>", "<svg>", "<td>"]).c.stack.map((e) => e.name), ["table", "tr", "td", "svg", "td"]);
  assert.deepEqual(feed(["<table>", "<td>", "<table>"]).c.stack.map((e) => e.name), ["table", "td", "table"]);
  // Every allowed F set has table: the loop never closes a table.
  for (const [name, set] of ALLOWED_F) assert.ok(set.has("table"), name);
});

test("H-85a: the claim of M-26 holds with the closure in both views; 300 rows of <tr><td>a<td>b keep D at 5", () => {
  const rows = `<table>${"<tr><td>a<td>b".repeat(300)}</table><div data-rq-block="x">y</div>`;
  for (const scripting of [true, false]) {
    const a = analyzeDepth(b(rows), { scripting });
    assert.deepEqual([a.maxD, a.violations, a.classes], [5, [], []]);
  }
  const shapes = [
    "<table><thead><tr><th>h<tbody><tr><td>a<tfoot><tr><td>f</table>",
    "<table><caption><i>x<col></table><p>y</p>",
    "<table><colgroup><col><colgroup><col><tr><td>a</table>",
    "<table><tr><td><div>a<td><b>b<td><p>c</p></table>",
    "<td>a<td><p>b</p></td>",
    "<table><tr><td>a<col><tr><td>b</table>",
  ];
  for (const s of shapes) {
    for (const scripting of [true, false]) assert.deepEqual(analyzeDepth(b(s), { scripting }).violations, [], s);
  }
});

test("H-85a: no closure in the noscript content C, in the copy of M-11 item 8 and with scripting disabled", () => {
  const outer = new DepthCounter();
  for (const t of ["table", "tr", "td", "noscript"]) outer.startTag(t, false);
  const r = checkNoscriptContent("<table><tr><td>a<td>b</td></td></tr></table>", outer);
  assert.equal(r.maxD, 5 + 5); // the C pass nests the cells: table, implied row group, tr, td, td
  assert.equal(outer.closure, true);
  const doc = "<noscript><table><tr><td>a<td>b</td></td></tr></table></noscript><table><tr><td>x<td>y<td>z<td>v<td>w</table>";
  const off = analyzeDepth(b(doc), { scripting: false });
  // The C pass nests the cells (5 with a closure); after </noscript> the closure applies again (8 without).
  assert.equal(off.maxD, 6);
  assert.deepEqual(analyzeDepth(b(doc), { scripting: true }).maxD, 6);
});

test("implied parts are not counted inside a template in a table (M-09 stops at template)", () => {
  assert.deepEqual(feed(["<table>", "<template>", "<td>"]).ds, [1, 2, 3]);
});

test("the invariant check is exact: a WHATWG stack of D + 3 is reported, D + 2 is not", () => {
  // Tags in an svg integration point (invalid by M-07): parse5 inserts an implied tbody that D does not see.
  const a = run("<svg><desc><table><tr>x");
  assert.ok(a.violations.includes("stack 7 > D 4 + 2 after <tr> at 18"), a.violations.join("; "));
  assert.ok(a.violations.some((v) => v.startsWith("open <tr> depth 7 > D 4 + 2")));
  assert.ok(a.violations.some((v) => v.startsWith("<tr> depth 7 > D 4 + 2")));
  assert.deepEqual(run("<svg><desc><table>x").violations, []);
});

test("a table popped by a crossing keeps its implied units", () => {
  assert.deepEqual(feed(["<div>", "<table>", "<td>", "</div>"]).ds, [1, 2, 5, 5]);
});

test("elements in a foreign region count; self-closing foreign elements do not", () => {
  assert.deepEqual(feed(["<svg>", "<g>", "<path/>", "<g>", "</g>", "</g>", "</svg>"]).ds, [1, 2, 2, 3, 2, 1, 0]);
});

test("the depth class is set when D is larger than 256", () => {
  assert.deepEqual(run(`${"<div>".repeat(MAX_D)}x`).classes, []);
  assert.deepEqual(run(`${"<div>".repeat(MAX_D + 1)}x`).classes, [CLASSES.depth]);
});

test("review shape: 300 x <span><div></span> gives D 600 and a WHATWG stack of 603", () => {
  const a = run(`${"<span><div></span>".repeat(300)}<div data-rq-block="x">y</div>`);
  assert.deepEqual([a.maxD, a.maxStack, a.maxDepth, a.violations], [601, 603, 603, []]);
  assert.deepEqual(a.classes, [CLASSES.depth]);
});

test("review shapes with ignored or partial end tags keep the invariant", () => {
  for (const shape of ["<h1><span><h2>x</h1>", "<li><ul></li>", "<body><div></body>", "<form><div></form>", "<p><b>x</p>y"]) {
    const a = run(shape.repeat(20));
    assert.deepEqual(a.violations, [], shape);
    assert.ok(a.maxStack <= a.maxD + 2, shape);
  }
});

test("</form> removes only the form: the DOM depth is larger than the stack, and D still bounds it", () => {
  const a = run("<form><div></form>".repeat(5));
  assert.deepEqual([a.maxD, a.maxStack, a.maxDepth, a.violations], [10, 8, 12, []]);
});

test("reconstructed formatting elements and a stolen formatting end tag keep the invariant", () => {
  assert.deepEqual(run(`<p><b class=z>x</p>${"<b><b><b><b></b></b></b>y</b>".repeat(5)}`).violations, []);
  assert.deepEqual(run("<p><b>x</p><form>y</form>z<div>w</div>").violations, []);
});

test("nested tables in cells: 64 levels make D 257", () => {
  const doc = (n) => `${"<table><td>".repeat(n)}<div data-rq-block="x">y</div>${"</td></table>".repeat(n)}`;
  assert.deepEqual([run(doc(63)).maxD, run(doc(63)).classes], [253, []]);
  assert.deepEqual([run(doc(64)).maxD, run(doc(64)).classes], [257, [CLASSES.depth]]);
});

test("H-71: the tags of the noscript content count in D (M-11 item 8, M-26 item 7)", () => {
  const nested = (divs, spans) =>
    `${"<div>".repeat(divs)}<noscript>${"<span>".repeat(spans)}x${"</span>".repeat(spans)}</noscript><div data-rq-block="x">y</div>`;
  // Before H-71 this document was valid with max D 256 (architect decision R5).
  assert.deepEqual([run(nested(255, 300)).maxD, run(nested(255, 300)).classes], [556, [CLASSES.depth]]);
  assert.deepEqual([run(nested(155, 100)).maxD, run(nested(155, 100)).classes], [256, []]);
  assert.deepEqual(run(nested(156, 100)).classes, [CLASSES.depth]);
  assert.deepEqual(run(`${"<div>".repeat(256)}<noscript></noscript>`).classes, [CLASSES.depth]);
});

test("H-71: after the noscript end tag the main pass continues with D as before C", () => {
  // C leaves its units in its own pass (the H-75 crossing makes the noscript element dirty there);
  // the main pass pops the noscript element exactly.
  const doc = `<noscript><svg><path d="M0"></svg></noscript>${"<div>".repeat(5)}`;
  assert.equal(run(doc).maxD, 5);
  assert.equal(analyzeDepth(b(doc), { scripting: false }).maxD, 5);
});

test("H-71: the main-pass rule classes apply to the tokens of C (M-11 item 8)", () => {
  const k = '<span data-rq-block="k">y<a href="2">z</a>w</span>';
  // The T-16 shape (M-09 condition 2 inside C) and the H-36 shape inside C (M-10 item 2).
  assert.deepEqual(run(`<object><a href="1"><noscript><table><marquee><table></table></marquee></table></noscript>x</a></object>${k}`).classes, [CLASSES.tableCurrent]);
  assert.deepEqual(run(`<a href="1"><noscript>${"<form><object><form></form></object></form>".repeat(9)}</noscript>x</a>${k}`).classes, [CLASSES.form]);
  // The element around the noscript element counts: a form outside, a table outside.
  assert.deepEqual(run("<form><noscript><form>x</form></noscript></form>").classes, [CLASSES.form]);
  assert.deepEqual(run("<table><noscript><table></table></noscript></table>").classes, [CLASSES.tableCurrent]);
  // Controls: the same tags in C without the outer element, and a table of C in a cell.
  assert.deepEqual(run("<noscript><form>x</form><table><tr><td>t</td></tr></table></noscript>").classes, []);
  assert.deepEqual(run("<table><tr><td><noscript><table></table></noscript></td></tr></table>").classes, []);
});

test("H-71: M-11 item 7, no html, body or head tag in C", () => {
  for (const c of ["<body></body>", '<html lang="en"></html>', "<head></head>", "</head>"]) {
    assert.ok(run(`<noscript>${c}</noscript>`).classes.includes(CLASSES.noscriptDocumentTag), c);
    assert.equal(checkNoscriptContent(c).documentTag, true, c);
  }
  assert.equal(checkNoscriptContent("<header></header><bodyx></bodyx>").documentTag, false);
});

test("checkNoscriptContent with the main-pass counter: classes go to its set, the counter does not change", () => {
  const outer = new DepthCounter();
  outer.startTag("form", false);
  outer.startTag("noscript", false);
  const before = JSON.stringify({ stack: outer.stack.map((e) => ({ ...e, implied: [...e.implied] })), d: outer.d });
  const r = checkNoscriptContent(`${"<i>".repeat(10)}<form></form>${"</i>".repeat(10)}`, outer);
  assert.equal(r.maxD, 13);
  assert.deepEqual([...outer.classes], [CLASSES.form]);
  assert.equal(JSON.stringify({ stack: outer.stack.map((e) => ({ ...e, implied: [...e.implied] })), d: outer.d }), before);
  assert.equal(outer.maxD, 2);
});

test("H-74: a foreign region of C has no tokenizer state change and follows M-05 item 2", () => {
  // No RAWTEXT for style in svg: the svg end tag is a tag (with H-75 it closes the style too).
  assert.equal(checkNoscriptContent("<svg><style></svg>").balanced, true);
  // No RCDATA for title in svg: the g start tag in it is a tag, and it is closed.
  assert.equal(checkNoscriptContent("<svg><title><g></g></title></svg>").balanced, true);
  // In HTML context of C, title is still RCDATA: the "tags" in it are text.
  assert.equal(checkNoscriptContent("<title><g></title>").balanced, true);
  // Self-closing tags in the region are not pushed; a self-closing svg opens no region.
  assert.equal(checkNoscriptContent('<svg><path d="M0"/><g/></svg><svg/>').balanced, true);
  // Item 6 also in the region.
  assert.equal(checkNoscriptContent("<svg><tr></tr></svg>").tablePart, true);
});

test("H-75 in C: only the end tag of the region root can cross elements of the region", () => {
  assert.equal(checkNoscriptContent('<a href="/"><svg><path d="M0"></svg></a>').balanced, true);
  assert.equal(checkNoscriptContent("<svg><g><path></svg>").balanced, true);
  assert.equal(checkNoscriptContent("<svg><g><path></g></svg>").balanced, false);
  // A nested svg is pushed in the region: its end tag is not the end tag of the region root.
  assert.equal(checkNoscriptContent("<svg><svg><path></svg></svg>").balanced, false);
  assert.equal(checkNoscriptContent("<svg><path>").balanced, false);
  assert.equal(checkNoscriptContent("<div><svg><path></div>").balanced, false);
});

test("H-75: the end tag of the region root sets no special-crossed mark; D does not change", () => {
  const block = '<span data-rq-block="x">y<a>z</a>w</span>';
  assert.deepEqual(run(`<a href="#"><svg><path d="M0"></svg></a>${block}`).classes, []);
  assert.deepEqual(run(`<b><math><mrow><mi>x</mi></math></b>${block}`).classes, []);
  // Other crossings in the region still mark (H-32): an inner end tag, and a nested svg.
  assert.deepEqual(run(`<a href="#"><svg><g><path d="M0"></g></svg></a>${block}`).classes, [CLASSES.formattingMarked]);
  assert.deepEqual(run(`<a href="#"><svg><svg><path d="M0"></svg></svg></a>${block}`).classes, [CLASSES.formattingMarked]);
  // The crossing keeps its units: 3 for each icon.
  const icons = (n) => `${'<span><svg><path d="M0"></svg></span>'.repeat(n)}<div data-rq-block="d">x</div>`;
  assert.deepEqual([run(icons(85)).maxD, run(icons(85)).classes], [256, []]);
  assert.deepEqual(run(icons(86)).classes, [CLASSES.depth]);
  const { ds } = feed(["<span>", "<svg>", "<path>", "</svg>", "</span>"]);
  assert.deepEqual(ds, [1, 2, 3, 3, 3]);
});

test("H-75: the end tag of the region root still passes a mark of a popped element (M-06 item 4 (b))", () => {
  // </g> crosses path and marks svg; </svg> crosses rect (no mark by (a)) but pops the marked svg.
  const block = '<span data-rq-block="x">y<a>z</a>w</span>';
  assert.deepEqual(run(`<a href="#"><svg><g><path d="M0"></g><rect></svg></a>${block}`).classes, [CLASSES.formattingMarked]);
});

test("checkNoscriptContent: item 7 for start tags alone, M-13 for script only, raw text states in HTML context of C", () => {
  assert.equal(checkNoscriptContent("<body>").documentTag, true);
  assert.equal(checkNoscriptContent('<input type="jä"><img type="ı">').nonAsciiType, false);
  // Script data and PLAINTEXT in HTML context of C: the "tags" after them are text.
  assert.equal(checkNoscriptContent("<script>if (a</b) x</script>").balanced, true);
  assert.equal(checkNoscriptContent("<plaintext></plaintext>").balanced, false);
  // math also starts a region (item 3 forbids it; the tokenization follows M-05 item 4).
  assert.equal(checkNoscriptContent("<math><mi></math>").balanced, true);
});

test("checkNoscriptContent: the region of C ends with its root, and only the root end tag crosses", () => {
  // Self-closing tags in the region are not pushed.
  assert.equal(checkNoscriptContent("<svg><g><path/></g></svg>").balanced, true);
  // The region stays open after an inner end tag.
  assert.equal(checkNoscriptContent("<svg><g></g><path></svg>").balanced, true);
  // After the root end tag, C is HTML context again: void elements, nesting.
  assert.equal(checkNoscriptContent("<svg></svg><div><p><br></p></div>").balanced, true);
  assert.equal(checkNoscriptContent("<svg></svg><div><div><p><br></p></div></div>").balanced, true);
  // The end tag of a nested svg is not the end tag of the region root.
  assert.equal(checkNoscriptContent("<svg><svg><path></svg>").balanced, false);
});

test("H-71: the pass on C sees attribute names (M-10 item 3 class in C)", () => {
  assert.ok(run('<noscript><template for="p"></template></noscript>').classes.includes(CLASSES.templateFor));
});

test("maxD keeps an earlier maximum when a later noscript content is flat; classes are sorted", () => {
  assert.equal(run("<div><div><div></div></div></div><noscript><i></i></noscript>").maxD, 3);
  assert.deepEqual(run("<noscript><body></body></noscript><table><td></tbody></table>").classes, [CLASSES.strayTable, CLASSES.noscriptDocumentTag]);
});

test("scripting disabled: a nested noscript start tag in C does not start a new C (the outer C is checked)", () => {
  const a = analyzeDepth(b("<div><noscript><noscript><span></span></noscript>x</noscript></div>"), { scripting: false });
  assert.deepEqual(a.classes, [CLASSES.noscriptBalance]);
});

test("scripting disabled: the claim of M-26 holds with D counted in C, at the bound", () => {
  // WHATWG stack in C: html, body, 155 div, noscript, 100 span = 258 = D + 2.
  const doc = `${"<div>".repeat(155)}<noscript>${"<span>".repeat(100)}x${"</span>".repeat(100)}</noscript><div data-rq-block="x">y</div>`;
  const off = analyzeDepth(b(doc), { scripting: false });
  assert.deepEqual([off.maxD, off.maxStack, off.violations], [256, 258, []]);
  const on = analyzeDepth(b(doc));
  assert.deepEqual([on.maxD, on.maxStack, on.violations], [256, 158, []]);
  // Tags in an svg integration point inside C (invalid by M-07): only the disabled view sees them.
  const bad = `<noscript><svg><desc>${"<table><td>".repeat(3)}x</desc></svg></noscript>`;
  assert.deepEqual(run(bad).violations, []);
  assert.ok(analyzeDepth(b(bad), { scripting: false }).violations.length > 0);
});

test("scripting disabled: classes of C come from the main-pass state at the noscript start tag", () => {
  const doc = "<form><noscript><form>x</form></noscript></form>";
  assert.deepEqual(analyzeDepth(b(doc), { scripting: false }).classes, [CLASSES.form]);
  assert.deepEqual(analyzeDepth(b("<noscript><div><img src=x></div></noscript><p>x</p>"), { scripting: false }).classes, []);
  // E3 in the disabled view: C is the rest of the input.
  assert.deepEqual(analyzeDepth(b("<noscript><div>x"), { scripting: false }).classes, [CLASSES.noscriptBalance]);
});

test("the invariant check reports a violation (tags in an svg integration point: invalid by M-07)", () => {
  const a = run(`<svg><desc>${"<table><td>".repeat(3)}x`);
  assert.ok(a.violations.length > 0);
  assert.match(a.violations[0], /> D \d+ \+ 2/);
});

test("class H-26: a formatting end tag over a special or foreign element", () => {
  assert.deepEqual(run('<a><div><div><div><div><div><div><div><div>x</a><span data-rq-block="x">y<a>z</a>w</span>').classes, [CLASSES.formatting]);
  assert.deepEqual(run("<b><div>x</b></div>").classes, [CLASSES.formatting]);
  assert.deepEqual(run("<svg><a><text>t</a></svg>").classes, [CLASSES.formatting]);
  assert.deepEqual(run("<a href=x><span>x</a><b><i>y</b><p><b>z</p>").classes, []);
  // The matching element itself may be foreign: an svg a closed in order is valid.
  assert.deepEqual(run("<svg><a>t</a></svg>").classes, []);
});

test("rule classes for HTML context only: a stray tr end tag and a noscript element in svg", () => {
  assert.deepEqual(run("<svg></tr></svg>").classes, []);
  assert.deepEqual(run("<svg><noscript><g></noscript></svg>").classes, []);
});

test("class H-32: a formatting end tag pops an element that an earlier crossing over a special element marked", () => {
  const D8 = "<div>".repeat(8);
  const block = (f) => `<span data-rq-block="x">y<${f}>z</${f}>w</span>`;
  assert.deepEqual(run(`<a><span>${D8}x</span></a>${block("a")}`).classes, [CLASSES.formattingMarked]);
  assert.deepEqual(run(`<nobr><span>${D8}x</span></nobr>${block("nobr")}`).classes, [CLASSES.formattingMarked]);
  assert.deepEqual(run(`<a>${"<li><ul>".repeat(4)}${"</li>".repeat(4)}x</a>${block("a")}`).classes, [CLASSES.formattingMarked]);
});

test("class H-32: a crossing over phrasing elements only does not mark (harmless mis-nesting stays valid)", () => {
  assert.deepEqual(run('<a href="#"><b><span><i>x</span></b></a>').classes, []);
  const { c } = feed(["<a>", "<b>", "<span>", "<i>", "</span>"]);
  assert.deepEqual(c.stack.map((e) => [e.name, e.dirty, e.marked]), [["a", false, false], ["b", true, false]]);
});

test("the special-crossed mark passes down when a marked element is popped, also by an exact pop", () => {
  const { c } = feed(["<a>", "<em>", "<span>", "<div>", "</span>", "</em>"]);
  assert.deepEqual(c.stack.map((e) => [e.name, e.marked]), [["a", true]]);
  assert.deepEqual(feed(["<p>", "<a>", "<span>", "<div>", "</span>", "</a>"]).c.classes, new Set([CLASSES.formattingMarked]));
  // A formatting end tag that crosses a marked element (not only the matching one) is caught.
  assert.deepEqual(run("<a><em><span><div></span></a>").classes, [CLASSES.formattingMarked]);
  // A crossing over a foreign element marks too (H-32).
  assert.deepEqual(run("<a><svg><g><text></g></svg></a>").classes, [CLASSES.formattingMarked]);
});

test("class H-30: a stray tbody, thead, tfoot, tr or table end tag in HTML context", () => {
  assert.deepEqual(run('<table><td></tbody><div data-rq-block="x"><table></table>y</div></td></table>').classes, [CLASSES.strayTable]);
  assert.deepEqual(run("<p>a</table>b</p>").classes, [CLASSES.strayTable]);
  assert.deepEqual(run("<table><tr><td>a</td></td></tr></table><table><col></colgroup><caption>c</caption></caption></table>").classes, []);
});

test("an end tag that parse5 reprocesses counts once (text in a table, then </table>)", () => {
  const a = run('<div data-rq-block="box"><table><div><table>a</table></div></table></div>');
  // The inner table start tag also breaks H-37 (F is the outer table, the current node is the div);
  // without the dedupe, the second </table> would pop the outer table and add M-05-stray-table-end.
  assert.deepEqual([a.classes, a.violations], [[CLASSES.tableCurrent], []]);
});

test("class H-27: noscript content that is not balanced, also at the end of the input", () => {
  assert.deepEqual(run('<noscript><button></noscript><div data-rq-block="a">x<button>y</button>z</div>').classes, [CLASSES.noscriptBalance]);
  assert.deepEqual(run("<noscript><div>x").classes, [CLASSES.noscriptBalance]);
  assert.deepEqual(run("<noscript><div><img src=x><table><tr><td>a</td></tr></table></div></noscript>").classes, []);
});

test("checkNoscriptContent: order, stray end tags, raw text and void elements", () => {
  assert.deepEqual(checkNoscriptContent("<a><span>x</a></span>"), { balanced: false, nonAsciiType: false, tablePart: false, documentTag: false, maxD: 0 });
  assert.deepEqual(checkNoscriptContent("</div>"), { balanced: false, nonAsciiType: false, tablePart: false, documentTag: false, maxD: 0 });
  assert.deepEqual(checkNoscriptContent("<br><img><style><p></style><title><b></title>"), { balanced: true, nonAsciiType: false, tablePart: false, documentTag: false, maxD: 0 });
  assert.deepEqual(checkNoscriptContent('<script type="text/jävascript"></script>'), { balanced: true, nonAsciiType: true, tablePart: false, documentTag: false, maxD: 0 });
});

test("class H-28: non-ASCII characters in the raw value of a script type, also in noscript", () => {
  assert.deepEqual(run('<script type="application/rıvqen-manifest+json">{}</script>').classes, [CLASSES.typeNonAscii]);
  assert.deepEqual(run("<noscript><script type=application/rivqen-manifeſt+json></script></noscript>").classes, [CLASSES.typeNonAscii]);
  assert.deepEqual(run('<script type="application/json" data-x="é">"é"</script>').classes, []);
});

test("class H-35: col whose nearest table-family-or-template element is template", () => {
  assert.deepEqual(run('<p data-rq-block="z">real</p><template><col><title></template><p data-rq-block="z">fake</p></title></template>').classes, [CLASSES.templateCol]);
  assert.deepEqual(run("<template><div><col></div></template>").classes, [CLASSES.templateCol]);
  assert.deepEqual(run("<template><tr><td>x</td></tr></template><template><table><col></table></template>").classes, []);
});

test("class H-43: an HTML template start tag with a for attribute, any value and any case", () => {
  const cls = [CLASSES.templateFor];
  assert.deepEqual(run('<div data-rq-block="a"><?start name="p">old<?end></div><template for="p"><b>new</b></template>').classes, cls);
  assert.deepEqual(run("<template FOR>y</template>").classes, cls);
  assert.deepEqual(run('<template for="">y</template>').classes, cls);
  assert.deepEqual(run('<table><template for="m"></template></table>').classes, cls);
  assert.deepEqual(run('<template><template id="t" for="m"></template></template>').classes, cls);
  assert.deepEqual(run('<template shadowrootmode="open" for="m"></template>').classes, cls);
});

test("class H-43 does not apply to foreign templates, other attributes or other elements", () => {
  assert.deepEqual(run('<svg><template for="m">Z</template></svg>').classes, []);
  assert.deepEqual(run('<math><template for="m"></template></math>').classes, []);
  assert.deepEqual(run('<template id="for" data-for="m" form="x"></template><label for="m">x</label>').classes, []);
});

test("startTag without attribute names keeps the old behavior", () => {
  const c = new DepthCounter();
  c.startTag("template", false);
  assert.deepEqual([...c.classes], []);
  c.startTag("template", false, ["for"]);
  assert.deepEqual([...c.classes], [CLASSES.templateFor]);
});

test("the depth invariant excludes template contents (H-35): implied parts there are not counted", () => {
  // In "in table" mode inside a template, <tr> inserts a tbody; M-09 F stops at template, so D does not count it.
  const a = run(`<template><caption></caption>${"<tr><td>".repeat(5)}</template><p>x</p>`);
  assert.deepEqual(a.violations, []);
  // Only an HTML template switches the check off: an SVG template does not.
  assert.ok(run("<svg><template><desc><table><tr>x").violations.length > 0);
});

test("class H-36: a form start tag while an HTML form is on the token stack", () => {
  assert.deepEqual(run(`<a href="1">${"<form><object><form></form></object></form>".repeat(9)}x</a>`).classes, [CLASSES.form]);
  assert.deepEqual(run("<form><input></form><form><input></form><div><form></div><form>x</form>").classes, []);
  assert.deepEqual(run("<form><svg><form></form></svg></form>").classes, []);
});

test("class H-37: a table part, col or table start tag while F is not the current node", () => {
  assert.deepEqual(run("<object><a href=1><table><marquee><table></table></marquee></table>x</a></object>").classes, [CLASSES.tableCurrent]);
  assert.deepEqual(run("<table><tr><object><td>x</td></object></tr></table>").classes, [CLASSES.tableCurrent]);
  assert.deepEqual(run("<table><form><tr><td>x</td></tr></form></table>").classes, [CLASSES.tableCurrent]);
  assert.deepEqual(run("<table><div><col></div></table>").classes, [CLASSES.tableCurrent]);
  assert.deepEqual(run("<table><div></div><tr><td><table><tr><td>x</td></tr></table></td></tr></table><table><col></table>").classes, []);
});

test("class H-38: a table part or col in noscript content without a table opened in C", () => {
  for (const tag of ["<td></td>", "<col>", "<tr></tr>", "<caption></caption>"]) {
    assert.deepEqual(run(`<table><tr><td><noscript>${tag}</noscript>x</td></tr></table>`).classes, [CLASSES.noscriptTablePart], tag);
  }
  assert.deepEqual(run("<noscript><table></table><td></td></noscript>").classes, [CLASSES.noscriptTablePart]);
  assert.deepEqual(run("<table><tr><td><noscript><table><tr><td>x</td></tr></table></noscript>x</td></tr></table>").classes, []);
});

test("rawAttrValue reads quoted, unquoted and empty values without decoding", () => {
  const text = `<x a="1&amp;" b='2' c=3 d>`;
  const loc = (name, value) => ({ attrs: { [name]: { startOffset: text.indexOf(value), endOffset: text.indexOf(value) + value.length } } });
  assert.equal(rawAttrValue(text, loc("a", 'a="1&amp;"'), "a"), "1&amp;");
  assert.equal(rawAttrValue(text, loc("b", "b='2'"), "b"), "2");
  assert.equal(rawAttrValue(text, loc("c", "c=3"), "c"), "3");
  assert.equal(rawAttrValue(text, loc("d", "d"), "d"), "");
  assert.equal(rawAttrValue(text, undefined, "a"), null);
});

test("invalid UTF-8 gives no analysis", () => {
  assert.equal(analyzeDepth(Buffer.from([0x3c, 0xc3, 0x28])).ok, false);
});
