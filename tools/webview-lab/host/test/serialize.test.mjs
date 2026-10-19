// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { test } from "node:test";
import { CASES_DIR, REPO_ROOT } from "../lab.mjs";
import { loadParse5 } from "../parse5.mjs";
import { serializeParse5 } from "../tree.mjs";
import { fakeDocument, runSerialize } from "./fake-dom.mjs";

const parse5 = await loadParse5();

/** Both serializers on one document; they must agree. */
function both(html, scripting) {
  const doc = parse5.parse(html, { scriptingEnabled: scripting });
  const a = serializeParse5(doc);
  const b = runSerialize(fakeDocument(doc));
  assert.equal(b, a, "serialize.js and the parse5 serializer differ");
  return a;
}

const lines = (...l) => l.join("\n");

test("html5lib format: doctype, comment, text, sorted attributes", () => {
  const out = both('<!DOCTYPE html><!--c--><p b="2" a="1">x</p>', true);
  assert.equal(
    out,
    lines(
      "| <!DOCTYPE html>",
      "| <!-- c -->",
      "| <html>",
      "|   <head>",
      "|   <body>",
      "|     <p>",
      '|       a="1"',
      '|       b="2"',
      '|       "x"',
    ),
  );
});

test("doctype with public and system id", () => {
  const out = both('<!DOCTYPE html PUBLIC "-//W3C//DTD HTML 4.01//EN" "http://x/y"><p>', true);
  assert.equal(out.split("\n")[0], '| <!DOCTYPE html "-//W3C//DTD HTML 4.01//EN" "http://x/y">');
});

test("noscript with scripting on: the content is one text node", () => {
  const out = both("<body><noscript><p>a</p></noscript>", true);
  assert.equal(out.split("\n").slice(3).join("\n"), lines("|     <noscript>", '|       "<p>a</p>"'));
});

test("noscript with scripting off: the content is elements", () => {
  const out = both("<body><noscript><p>a</p></noscript>", false);
  assert.equal(out.split("\n").slice(3).join("\n"), lines("|     <noscript>", "|       <p>", '|         "a"'));
});

test("svg and math elements carry the namespace word", () => {
  const out = both('<body><svg viewBox="0 0 1 1"><path d="M0"/></svg><math><mi>x</mi></math>', true);
  assert.equal(
    out.split("\n").slice(3).join("\n"),
    lines(
      "|     <svg svg>",
      '|       viewBox="0 0 1 1"',
      "|       <svg path>",
      '|         d="M0"',
      "|     <math math>",
      "|       <math mi>",
      '|         "x"',
    ),
  );
});

test("a foreign attribute with a prefix is shown with a space", () => {
  const out = both('<body><svg><use xlink:href="#a"/></svg>', true);
  assert.ok(out.includes('|         xlink href="#a"'), out);
});

test("template content has its own level", () => {
  const out = both("<body><template><p>t</p></template>", true);
  assert.equal(
    out.split("\n").slice(3).join("\n"),
    lines("|     <template>", "|       content", "|         <p>", '|           "t"'),
  );
});

test("a template with an attribute prints attributes before content", () => {
  const out = both('<body><template id="x"><i></i></template>', false);
  assert.equal(
    out.split("\n").slice(3).join("\n"),
    lines("|     <template>", '|       id="x"', "|       content", "|         <i>"),
  );
});

test("a very deep tree does not overflow the stack", () => {
  const out = both(`<body>${"<div>".repeat(5000)}`, true);
  assert.equal(out.split("\n").length, 3 + 5000);
});

test("serialize.js and parse5 agree on every Q-19 case in both views", () => {
  const ids = JSON.parse(readFileSync(join(CASES_DIR, "index.json"), "utf8"));
  let n = 0;
  for (const id of ids) {
    const c = JSON.parse(readFileSync(join(CASES_DIR, `${id}.json`), "utf8"));
    if (c.question !== "Q-19") continue;
    const html = new TextDecoder().decode(readFileSync(join(REPO_ROOT, c.source)));
    both(html, true);
    both(html, false);
    n += 2;
  }
  assert.ok(n >= 80, `only ${n} runs`);
});

test("a meta element with a content attribute is not a template", () => {
  // The fake DOM gives meta a string property "content", as a real DOM does.
  const out = both('<head><meta name="color-scheme" content="light dark"></head>', true);
  assert.match(out, /<meta>\n\|\s+content="light dark"\n\|\s+name="color-scheme"/);
});

test("text, comment, attribute and doctype strings are escaped the same way in both serializers", () => {
  const out = both('<!DOCTYPE a\\b PUBLIC "p" "s"><body a="q&quot;\\&#10;n" b="x&#13;y"><!--c\\"\n--><p>t"\\\r\nz</p>', true);
  assert.ok(out.includes('a="q\\"\\\\\\nn"'), out);
  assert.ok(out.includes('b="x\\ry"'), out);
  assert.ok(out.includes('<!-- c\\\\\\"\\n -->'), out);
  assert.ok(out.includes('"t\\"\\\\\\nz"'), out);
  assert.ok(out.includes('<!DOCTYPE a\\\\b "p" "s">'), out);
  // Every node is one line: a newline in a value does not add a line.
  assert.ok(out.split("\n").every((l) => l.startsWith("| ")), out);
});

test("two trees that differ only in quotes or newlines give different strings", () => {
  assert.notEqual(both('<p a="x&quot;">', true), both('<p a="x">', true));
  assert.notEqual(both("<p>a\nb</p>", true), both("<p>a</p><p>b</p>", true));
  assert.notEqual(both('<p>a"</p>', true), both("<p>a</p>", true));
  assert.notEqual(both("<p>a\\n</p>", true), both("<p>a\n</p>", true));
});

/** The first element named `name` in a fake tree. */
function find(node, name) {
  if (node.nodeType === 1 && node.localName === name) return node;
  for (const c of node.childNodes) {
    const r = find(c, name);
    if (r) return r;
  }
  return null;
}

test("a form whose named properties hide childNodes, attributes and the others does not change the tree", () => {
  const html = '<body><form id="f"><input name="childNodes"><input name="attributes"><p>kept</p></form><template><i>t</i></template>';
  const doc = parse5.parse(html, { scriptingEnabled: true });
  const expected = serializeParse5(doc);
  const fake = fakeDocument(doc);
  const input = find(fake, "input");
  // LegacyOverrideBuiltIns: the form object answers these names with the named input element.
  const form = find(fake, "form");
  const template = find(fake, "template");
  for (const name of ["childNodes", "attributes", "nodeType", "localName", "namespaceURI", "content", "nodeName", "firstChild"]) {
    Object.defineProperty(form, name, { value: input, configurable: true });
  }
  Object.defineProperty(template, "content", { value: input });
  assert.equal(runSerialize(fake), expected);
});

test("without DOM prototypes the plain properties are read (fallback)", () => {
  const text = { nodeType: 3, data: "x" };
  const el = { nodeType: 1, nodeName: "P", localName: "p", namespaceURI: "http://www.w3.org/1999/xhtml", attributes: [], childNodes: [text] };
  assert.equal(runSerialize({ nodeType: 9, childNodes: [el] }, {}), '| <p>\n|   "x"');
});
