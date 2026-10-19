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
