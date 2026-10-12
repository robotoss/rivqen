// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

import assert from "node:assert/strict";
import { test } from "node:test";
import { CLASSES, DepthCounter, MAX_D, analyzeDepth, checkNoscriptContent, rawAttrValue } from "../src/depth.mjs";

const b = (s) => Buffer.from(s, "utf8");
const run = (s) => analyzeDepth(b(s));

/** Feed a token list to a counter: "<x>", "<x/>", "</x>". Returns D after each token. */
function feed(tokens) {
  const c = new DepthCounter();
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

test("noscript content does not count in D; the noscript element does", () => {
  const a = run(`${"<div>".repeat(255)}<noscript>${"<span>".repeat(300)}x${"</span>".repeat(300)}</noscript>x`);
  assert.deepEqual([a.maxD, a.classes], [256, []]);
  assert.deepEqual(run(`${"<div>".repeat(256)}<noscript></noscript>`).classes, [CLASSES.depth]);
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
  assert.deepEqual(checkNoscriptContent("<a><span>x</a></span>"), { balanced: false, nonAsciiType: false, tablePart: false });
  assert.deepEqual(checkNoscriptContent("</div>"), { balanced: false, nonAsciiType: false, tablePart: false });
  assert.deepEqual(checkNoscriptContent("<br><img><style><p></style><title><b></title>"), { balanced: true, nonAsciiType: false, tablePart: false });
  assert.deepEqual(checkNoscriptContent('<script type="text/jävascript"></script>'), { balanced: true, nonAsciiType: true, tablePart: false });
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
