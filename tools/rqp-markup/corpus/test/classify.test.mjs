// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// Tests of the all-rules classifier (classify.mjs). The golden fixtures are the reference:
// the classifier finds no violation exactly for the valid fixtures, and for each invalid
// fixture it finds a class of one of the fixture's rules.

import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync, readdirSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { classify, ruleOf, choosePicks, insertBlocks, cdataOk, scriptTypeItem } from "../classify.mjs";

const HERE = path.dirname(fileURLToPath(import.meta.url));
const FIXTURES = path.resolve(HERE, "..", "..", "..", "..", "fixtures", "rqp", "markup");

// Rules that the classifier does not check (block limits and JSON): fixtures of these rules
// may be valid for the classifier.
const NOT_CHECKED = new Set(["M-25", "M-26-blocks", "M-01"]);

const all = (r) => [...Object.keys(r.classes), ...Object.keys(r.blockRules?.classes ?? {})];
const doc = (s, blocks = false) => classify(Buffer.from(s, "utf8"), { blocks });

test("golden fixtures: no violation exactly for valid fixtures; the fixture rule is found", () => {
  const mismatches = [];
  const names = readdirSync(FIXTURES, { withFileTypes: true }).filter((d) => d.isDirectory()).map((d) => d.name);
  assert.ok(names.length > 100);
  for (const name of names.sort()) {
    const dir = path.join(FIXTURES, name);
    const expected = JSON.parse(readFileSync(path.join(dir, "expected.json"), "utf8"));
    const fixture = JSON.parse(readFileSync(path.join(dir, "fixture.json"), "utf8"));
    const r = classify(readFileSync(path.join(dir, "input.html")), { blocks: true });
    const cls = all(r);
    const rules = new Set(cls.map(ruleOf));
    if (expected.valid) {
      if (cls.length > 0) mismatches.push(`${name}: valid, got ${cls.join(",")}`);
    } else if (!fixture.rules.some((x) => rules.has(x))) {
      const skip = fixture.rules.some((x) => NOT_CHECKED.has(x)) || expected.error === "RQP_MARKUP_LIMIT" || expected.error === "RQP_MARKUP_BAD_JSON";
      if (!skip) mismatches.push(`${name}: ${expected.error} (${fixture.rules.join(",")}), got ${cls.join(",") || "valid"}`);
    }
  }
  assert.deepEqual(mismatches, []);
});

test("document rules with items", () => {
  assert.deepEqual(Object.keys(doc("<div><button>a</div>").classes), ["M-06.1"]);
  assert.deepEqual(Object.keys(doc("<b><div>a</b>").classes), ["M-06.2"]);
  assert.deepEqual(Object.keys(doc("<table><td></tbody>").classes), ["M-06.3-row"]);
  assert.deepEqual(Object.keys(doc("<div></table>").classes), ["M-06.3-table"]);
  assert.deepEqual(Object.keys(doc("<a><span><div>a</span></a>").classes), ["M-06.4-special"]);
  // H-75: the end tag of the region root sets no mark; an inner crossing in the region does.
  assert.deepEqual(Object.keys(doc('<a href="#"><svg><path d="M0"></svg></a>').classes), []);
  assert.deepEqual(Object.keys(doc('<a href="#"><svg><g><path d="M0"></g></svg></a>').classes), ["M-06.4-foreign"]);
  assert.deepEqual(Object.keys(doc('<a href="#"><svg><svg><path d="M0"></svg></svg></a>').classes), ["M-06.4-foreign"]);
  assert.deepEqual(Object.keys(doc("<svg><div></div></svg>").classes), ["M-07.1"]);
  assert.deepEqual(Object.keys(doc("<svg></p></svg>").classes), ["M-07.2"]);
  // An end tag in the region that matches an element below the region (also crosses svg: M-06.1).
  assert.deepEqual(Object.keys(doc("<div><svg></div>").classes), ["M-06.1", "M-07.2"]);
  assert.deepEqual(Object.keys(doc("<svg><desc><b>x</b></desc></svg>").classes), ["M-07.1", "M-07.3"]);
  assert.deepEqual(Object.keys(doc("<svg><title/></svg>").classes), ["M-07.4"]);
  assert.deepEqual(Object.keys(doc("<select><div></div></select>").classes), ["M-08"]);
  assert.deepEqual(Object.keys(doc("<select><option>a").classes), ["M-08"]);
  assert.deepEqual(Object.keys(doc("<table><tr><td>a<td>b</td></tr></table>").classes), ["M-09"]);
  assert.deepEqual(Object.keys(doc("<frameset>").classes), ["M-10"]);
  assert.deepEqual(Object.keys(doc("<noscript><img src=\"a</noscript>").classes), ["M-11.1"]);
  assert.deepEqual(Object.keys(doc("<noscript><math></math></noscript>").classes), ["M-11.3"]);
  assert.deepEqual(Object.keys(doc('<noscript><script type="a&b"></script></noscript>').classes), ["M-11.4"]);
  assert.deepEqual(Object.keys(doc("<noscript><button></noscript>").classes), ["M-11.5-other"]);
  assert.deepEqual(Object.keys(doc("<noscript><p>a<li>b</noscript>").classes), ["M-11.5-pli"]);
  assert.deepEqual(Object.keys(doc("<noscript><div><p>a</div></noscript>").classes), ["M-11.5-pli"]);
  assert.deepEqual(Object.keys(doc("<![CDATA[a>b]]>").classes), ["M-12"]);
  assert.deepEqual(Object.keys(doc('<script type="application/rivqen-manifest+json"></script>').classes), ["M-13.1"]);
  assert.deepEqual(Object.keys(doc('<script type="text/j&#97;vascript"></script>').classes), ["M-13.2"]);
  assert.deepEqual(Object.keys(doc('<script type="text/jävascript"></script>').classes), ["M-13.3"]);
  assert.deepEqual(Object.keys(classify(new Uint8Array([0x3c, 0x70, 0x3e, 0xe9])).classes), ["M-02"]);
  assert.deepEqual(Object.keys(classify(new Uint8Array(5 * 1024 * 1024 + 1)).classes), ["M-01"]);
});

test("the classifier continues after a violation", () => {
  const r = doc("<div><button>a</div><b><div>x</b><table><td></tbody>");
  assert.deepEqual(Object.keys(r.classes), ["M-06.1", "M-06.2", "M-06.3-row"]);
  assert.equal(r.classes["M-06.1"], 1);
});

test("M-26: D with the dirty mark, and the units of the stack", () => {
  const dirty = doc("<span><div></span>".repeat(130));
  assert.deepEqual(Object.keys(dirty.classes), ["M-26-dirty"]);
  assert.equal(dirty.maxD, 260);
  assert.equal(dirty.maxUnits, 2);
  const deep = doc("<div>".repeat(257));
  assert.deepEqual(Object.keys(deep.classes), ["M-26-stack"]);
  const ok = doc("<div>".repeat(256));
  assert.deepEqual(Object.keys(ok.classes), []);
});

test("rules of review round 2 and H-43 (classes from depth.mjs, details from the pass)", () => {
  const cls = (h) => doc(h).classes;
  assert.deepEqual(cls("<template><col></template>"), { "M-09.1": 1 });
  assert.deepEqual(cls("<template><div><col></div></template>"), { "M-09.1": 1 });
  assert.deepEqual(cls("<template><table><col></table></template>"), {});
  assert.deepEqual(cls("<table><colgroup><col></colgroup></table>"), {});
  assert.deepEqual(cls("<form><div><form></form></div></form>"), { "M-10.2": 1 });
  assert.deepEqual(cls("<form></form><form></form>"), {});
  assert.deepEqual(cls("<svg><form></form></svg>"), {});
  const form = doc("<table><form><tr><td>a</td></tr></form></table>");
  assert.deepEqual(form.classes, { "M-09.2": 1 });
  assert.deepEqual(form.details["M-09.2"], { "<form>": 1 });
  const div = doc("<table><div><tr><td>a</td></tr></div></table>");
  assert.deepEqual(div.details["M-09.2"], { "<div>": 1 });
  assert.deepEqual(cls("<table><div></div><tr><td>a</td></tr></table>"), {});
  assert.deepEqual(cls("<table><tr><td>a</td></tr></table>"), {});
  assert.deepEqual(cls('<template for="p"></template>'), { "M-10.3": 1 });
  assert.deepEqual(cls("<template FOR></template>"), { "M-10.3": 1 });
  assert.deepEqual(cls('<svg><template for="p"></template></svg>'), {});
  // With H-71, M-09 also applies to the tags of C: the tr has the cell around the noscript as F.
  const cell = doc("<table><tr><td><noscript><tr></tr></noscript></td></tr></table>");
  assert.deepEqual(cell.classes, { "M-09": 1, "M-11.6": 1 });
  assert.deepEqual(cell.details["M-09"], { "<tr> in <td> in noscript": 1 });
  assert.deepEqual(cell.details["M-11.6"], { "noscript in a cell or caption": 1 });
  assert.deepEqual(cls("<noscript><table><tr><td>a</td></tr></table></noscript>"), {});
  assert.deepEqual(doc("<noscript><td>a</td></noscript>").details["M-11.6"], { "noscript elsewhere": 1 });
});

test("H-71: the rules of the main pass on the noscript content (M-11 items 7 and 8)", () => {
  const cls = (h) => doc(h).classes;
  const k = '<span data-rq-block="k">y<a href="2">z</a>w</span>';
  const t16 = doc(`<object><a href="1"><noscript><table><marquee><table></table></marquee></table></noscript>x</a></object>${k}`);
  assert.deepEqual(t16.classes, { "M-09.2": 1 });
  assert.deepEqual(t16.details["M-09.2"], { "<marquee> in noscript": 1 });
  assert.deepEqual(cls("<table><noscript><table></table></noscript></table>"), { "M-09.2": 1 });
  assert.deepEqual(cls("<form><noscript><form>x</form></noscript></form>"), { "M-10.2": 1 });
  assert.deepEqual(cls(`<a href="1"><noscript>${"<form><object><form></form></object></form>".repeat(9)}</noscript>x</a>`), { "M-10.2": 1 });
  assert.deepEqual(cls("<noscript><table><caption><td>a</td></caption></table></noscript>"), { "M-09": 1 });
  assert.deepEqual(cls("<noscript><![CDATA[a>b]]></noscript>"), { "M-12": 1 });
  assert.deepEqual(cls("<noscript><body></body></noscript>"), { "M-11.7": 2 });
  assert.deepEqual(doc('<noscript><html lang="en"></html></noscript>').details["M-11.7"], { "<html>": 1, "</html>": 1 });
  const deep = doc(`${"<div>".repeat(156)}<noscript>${"<span>".repeat(100)}x${"</span>".repeat(100)}</noscript>`);
  assert.deepEqual(deep.classes, { "M-26-noscript": 1 });
  assert.deepEqual(cls(`${"<div>".repeat(155)}<noscript>${"<span>".repeat(100)}x${"</span>".repeat(100)}</noscript>`), {});
  // Controls: the same tags without the elements around the noscript element.
  assert.deepEqual(cls("<form></form><noscript><form>x</form><table><tr><td>t</td></tr></table></noscript>"), {});
  assert.deepEqual(cls("<table><tr><td><noscript><table></table></noscript></td></tr></table>"), {});
});

test("H-71: M-09 on the table parts of C, with F in C or around the noscript element", () => {
  const cls = (h) => doc(h).classes;
  assert.deepEqual(cls("<noscript><table><tr><td><col></td></tr></table></noscript>"), { "M-09": 1 });
  assert.deepEqual(cls("<noscript><table><caption><tr></tr></caption></table></noscript>"), { "M-09": 1 });
  assert.deepEqual(cls("<noscript><table><tr><caption></caption></tr></table></noscript>"), { "M-09": 1 });
  assert.deepEqual(cls("<noscript><table><tbody><th>a</th></tbody><colgroup><col></colgroup></table></noscript>"), {});
  assert.deepEqual(cls("<noscript><table><thead><tr><td>a</td></tr></thead><tfoot><tr><th>b</th></tr></tfoot><tr><td>c</td></tr></table></noscript>"), {});
  assert.deepEqual(cls("<noscript><table><tbody><tr><td><tr></tr></td></tr></tbody></table></noscript>"), { "M-09": 1 });
  // A template in C stops the search for F (M-09); the template itself is M-11 item 3.
  assert.deepEqual(cls("<table><tr><td><noscript><template><tr></tr></template></noscript></td></tr></table>"), { "M-11.3": 1, "M-11.6": 1 });
});

test("H-74: M-07 and M-12 in the foreign region of C, and the end of the region", () => {
  const cls = (h) => doc(h).classes;
  // After the integration point and after the region, C is in the right context again.
  assert.deepEqual(cls("<noscript><svg><title>t</title><g></g></svg><p>x</p></noscript>"), {});
  assert.deepEqual(cls("<noscript><svg><style/></svg></noscript>"), { "M-07.4": 1 });
  assert.deepEqual(cls("<noscript><svg><desc><!--c--></desc></svg></noscript>"), { "M-07.3": 1 });
  assert.deepEqual(cls("<noscript><svg><desc><!DOCTYPE html></desc></svg></noscript>"), { "M-07.3": 1 });
  assert.deepEqual(cls("<noscript><!-- a > b --></noscript>"), {});
  const out = doc("<noscript><div><svg></div></noscript>");
  assert.deepEqual(Object.keys(out.classes), ["M-07.2", "M-11.5-other"]);
  // math in C: M-11 item 3; its end tag closes the region like the end tag of an svg root.
  assert.deepEqual(cls("<noscript><math><mi>x</mi><mrow><mn>1</mn></math></noscript>"), { "M-11.3": 1 });
});

test("M-11 item 5: only unclosed p or li, also with a foreign region and void elements in C", () => {
  const cls = (h) => doc(h).classes;
  assert.deepEqual(cls('<noscript><p><svg><path d="M0"></svg>x</noscript>'), { "M-11.5-pli": 1 });
  assert.deepEqual(cls('<noscript><p><svg><path d="M0"></svg><i><b><br></b></i></noscript>'), { "M-11.5-pli": 1 });
  assert.deepEqual(cls("<noscript><p><br><svg/></noscript>"), { "M-11.5-pli": 1 });
});

test("H-74 and H-75: svg in the noscript content (M-11 items 5 and 9)", () => {
  const cls = (h) => doc(h).classes;
  assert.deepEqual(cls('<noscript><img src="p.gif"><svg width="1"><path d="M0"></path></svg><svg/></noscript>'), {});
  assert.deepEqual(cls('<noscript><a href="/"><svg><path d="M0"></svg>Home</a></noscript>'), {});
  assert.deepEqual(cls('<noscript><svg><g><path d="M0"></g></svg></noscript>'), { "M-11.5-other": 1 });
  assert.deepEqual(cls("<noscript><svg><p>x</p></svg></noscript>"), { "M-07.1": 1 });
  // No state change in the region: the g in svg title is a tag (M-07 item 3), and the svg end tag
  // after an svg style start tag is a tag (so M-07 item 4: the style content is text only).
  assert.deepEqual(cls("<noscript><svg><title><g></g></title></svg></noscript>"), { "M-07.3": 2 });
  assert.deepEqual(cls("<noscript><svg><style></svg></noscript>"), { "M-07.4": 1 });
  assert.deepEqual(cls("<noscript><svg><tr></tr></svg></noscript>"), { "M-11.6": 1 });
  assert.deepEqual(cls("<noscript><svg><style><![CDATA[a>b]]></style></svg></noscript>"), { "M-12": 1 });
});

test("block rules on simulated blocks", () => {
  const ok = doc('<main data-rq-block="m">x<p>a</p></main>', true);
  assert.deepEqual(all(ok), []);
  assert.deepEqual(ok.blocks.length, 1);
  assert.deepEqual(all(doc('<main data-rq-block="m"><script></script></main>', true)), ["M-22.2:script"]);
  // H-73: svg and math in the content, with FOREIGN_ALLOWED (M-22 item 5).
  assert.deepEqual(all(doc('<main data-rq-block="m"><svg><path/><title>t</title></svg><math><mi>x</mi></math><svg/></main>', true)), []);
  const a = doc('<main data-rq-block="m"><svg><a href="#x"><text>t</text></a><script></script></svg></main>', true);
  assert.deepEqual(all(a), ["M-22.5"]);
  assert.deepEqual(a.blockRules?.details["M-22.5"], { "<a>": 1, "<script>": 1 });
  assert.deepEqual(all(doc('<main data-rq-block="m"><svg><foreignObject>t</foreignObject><x-y.z/></svg></main>', true)), ["M-22.5"]);
  // The tokenizer lowercases tag names: the detail has "?", not the name.
  const odd = doc('<main data-rq-block="m"><svg><x.SECRET/></svg></main>', true);
  assert.deepEqual(odd.blockRules?.details["M-22.5"], { "<?>": 1 });
  assert.equal(JSON.stringify(odd).toLowerCase().includes("secret"), false);
  // H-75 in the content: only the end tag of the region root can cross.
  assert.deepEqual(all(doc('<main data-rq-block="m"><a href="#"><svg><path d="M0"></svg></a></main>', true)), []);
  assert.deepEqual(all(doc('<main data-rq-block="m"><svg><g><path d="M0"></g></svg></main>', true)), ["M-23"]);
  assert.deepEqual(all(doc('<main data-rq-block="m"><svg><path d="M0"/>', true)), ["M-20"]);
  assert.deepEqual(all(doc('<main data-rq-block="m"><span/></span></main>', true)), ["M-22.3"]);
  assert.deepEqual(all(doc('<main data-rq-block="m"><br></br></main>', true)), ["M-22.4"]);
  assert.deepEqual(all(doc('<main data-rq-block="m"><p>a</main>', true)), ["M-20", "M-23"]);
  assert.deepEqual(all(doc('<p data-rq-block="m">a<div>b</div></p>', true)), ["M-24.a"]);
  assert.deepEqual(all(doc('<div data-rq-block="m"><li>a</li></div>', true)), ["M-24.c"]);
  assert.deepEqual(all(doc('<a href="#"><div data-rq-block="m"><a>b</a></div></a>', true)), ["M-24.e"]);
  assert.deepEqual(all(doc('<table><tr><div data-rq-block="m">a</div></tr></table>', true)), ["M-19.2"]);
  assert.deepEqual(all(doc('<template><div data-rq-block="m">a</div></template>', true)), ["M-19.1"]);
  assert.deepEqual(all(doc('<div data-rq-block="m">a', true)), ["M-20"]);
  assert.deepEqual(all(doc('<div data-rq-block="m"><div data-rq-block="n">a</div></div>', true)), ["M-17"]);
  assert.deepEqual(all(doc('<td data-rq-block="m">a</td>', true)), ["M-16"]);
});

test("simulated block picks: h1 and id/class matches without overlap; insertion after the tag name", () => {
  const html = '<main><h1>T</h1><div class="price">1</div><span id="title-x">t</span><p class="content">c</p></main>';
  const r = doc(html);
  const { small, main } = choosePicks(r.picks);
  assert.deepEqual(small.map((p) => p.name), ["h1", "div", "span"]);
  assert.deepEqual(main.map((p) => p.name), ["main"]);
  const bytes = Buffer.from(html, "utf8");
  const out = insertBlocks(bytes, small, "s");
  assert.ok(out !== null);
  const text = out.toString("utf8");
  assert.ok(text.includes('<h1 data-rq-block="s1">T</h1>'));
  assert.ok(text.includes('<div data-rq-block="s2" class="price">'));
  assert.ok(text.includes('<span data-rq-block="s3" id="title-x">'));
  assert.deepEqual(all(doc(text, true)), []);
  // A nested match is skipped.
  const nested = doc('<h1><span class="title">a</span></h1><div class="content">b</div>');
  assert.deepEqual(choosePicks(nested.picks).small.map((p) => p.name), ["h1", "div"]);
  // Non-ASCII text before the pick: byte offsets.
  const u = 'é😀<h1 class="x">a</h1>';
  const ur = doc(u);
  const ins = insertBlocks(Buffer.from(u, "utf8"), choosePicks(ur.picks).small, "s");
  assert.equal(ins?.toString("utf8"), 'é😀<h1 data-rq-block="s1" class="x">a</h1>');
  // A wrong offset is refused.
  assert.equal(insertBlocks(Buffer.from("<h1>a</h1>"), [{ kind: "h1", name: "h1", insertAt: 1, start: 0, end: 9 }], "s"), null);
});

test("ruleOf", () => {
  assert.equal(ruleOf("M-06.1"), "M-06");
  assert.equal(ruleOf("M-26-dirty"), "M-26");
  assert.equal(ruleOf("M-22.2:svg"), "M-22");
  assert.equal(ruleOf("M-24.k"), "M-24");
  assert.equal(ruleOf("H-37"), "H-37");
});

test("helpers", () => {
  assert.equal(cdataOk("<![CDATA[a]]>", 0), true);
  assert.equal(cdataOk("<![CDATA[a>b]]>", 0), false);
  assert.equal(cdataOk("<![CDATA[>", 0), false);
  assert.equal(cdataOk("<![CDATA[]]>", 0), true);
  assert.equal(cdataOk("<![CDATA[ no end", 0), true);
  assert.equal(scriptTypeItem(null), null);
  assert.equal(scriptTypeItem("text/javascript"), null);
  assert.equal(scriptTypeItem("Application/Rivqen-Manifest+json"), "1");
  assert.equal(scriptTypeItem("a&b"), "2");
  assert.equal(scriptTypeItem("ı"), "3");
});

test("the output has no document text", () => {
  const secret = "SECRET-TEXT-1234";
  const r = doc(`<div class="${secret}"><p>${secret}</p><noscript>${secret}</noscript><svg><desc><b>${secret}</b></desc></svg></div>`);
  assert.ok(!JSON.stringify(r).includes(secret));
});
