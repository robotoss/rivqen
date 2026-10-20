// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// The parse5 oracle and the patch apply oracle of the S5 study see the shapes that the
// option rules remove (controls), see nothing on the shapes that the rules accept, and
// report the parse5 8.0.1 table-scope deviation apart from the WHATWG view.

import { test } from 'node:test';
import assert from 'node:assert/strict';
import { OpenElementStack } from '../../../crosscheck/node_modules/parse5/dist/parser/open-element-stack.js';
import { analyzeS5 } from '../src/markup-s5.mjs';
import { COMBOS } from '../src/options.mjs';
import { Oracle, isInformative, withSpecTableScope } from '../src/oracle-s5.mjs';
import { patchFindings } from '../src/patch-s5.mjs';

const P = '<span data-rq-block="z">y<a href="#">z</a>w</span>';

function kinds(html, combo) {
  const bytes = Buffer.from(html, 'utf8');
  const r = analyzeS5(bytes, { ...COMBOS[combo], trace: true });
  assert.equal(r.valid, true, `${combo} must accept the document`);
  return new Oracle(bytes).check(r).map((f) => f.kind);
}
const findings = (html, combo) => kinds(html, combo).filter((k) => !isInformative(k));

test('control tu: H-35 in a template of block content cuts the block in parse5 (both views)', () => {
  const html = `<div data-rq-block="a"><table><tr><td><template><col><style></template></td></tr></table></div><p data-rq-block="b">f</p></style></template></td></tr></table></div>${P}`;
  const k = findings(html, 'tu');
  assert.ok(k.includes('parse5-enabled-blocks'), k.join());
  assert.ok(k.includes('parse5-disabled-blocks'), k.join());
});

test('control nu: a PCLOSE start tag in C closes a p block in the scripting-disabled view', () => {
  const k = findings(`<p data-rq-block="a">x<noscript><div>y</div></noscript>z</p>${P}`, 'nu');
  assert.ok(k.includes('parse5-disabled-blocks'), k.join());
  assert.ok(!k.includes('parse5-enabled-blocks'));
});

test('accepted shapes of t2 and n1 have no finding in any view, fragment or depth check', () => {
  for (const [html, combo] of [
    [`<div data-rq-block="a">x<template id="row"><div class="r"><b>n</b></div></template>z</div>${P}`, 't2'],
    [`<div data-rq-block="a"><template><table><tr><td>x</td></tr></table><script>s()</script></template></div>${P}`, 't2'],
    [`<div data-rq-block="a">x<noscript><img src="/p.gif"><iframe src="/f"></iframe></noscript>y</div>${P}`, 'n1'],
    [`<div data-rq-block="a"><noscript><svg><path d="M0"></svg><p>t</p></noscript></div>${P}`, 'n1'],
    [`<div data-rq-block="a"><template><noscript><img src=x></noscript></template></div>${P}`, 't2n2'],
    [`<p><b>x</p><div data-rq-block="a">y<template><i>t</i></template>z</div>${P}`, 't2'],
  ]) {
    assert.deepEqual(findings(html, combo), [], `${combo} ${html}`);
  }
});

test('t1: table modes in a template give elements that the tree does not have; parse5 8.0.1 also leaves the template', () => {
  const html = `<section data-rq-block="a"><table><tr><td><template><tfoot></tfoot><table><tr><td>c</td></tr></table></template></td></tr></table>z</section>${P}`;
  const k = findings(html, 't1');
  // parse5 8.0.1: its table scope does not stop at template, so <table> pops out of the template.
  assert.ok(k.includes('template-enabled-end'), k.join());
  // The WHATWG table scope stops at template: the tree has no element for <table>, the template ends in place.
  assert.ok(k.includes('content-spec-missing-tpl'), k.join());
  assert.ok(!k.includes('template-spec-end'), k.join());
  // Block boundaries agree in every view.
  assert.ok(!k.some((x) => x.startsWith('parse5-')), k.join());
  assert.equal(analyzeS5(Buffer.from(html, 'utf8'), COMBOS.t2).why, 'S5-template-table-part');
});

test('withSpecTableScope restores the parse5 prototype, also after an exception', () => {
  const before = OpenElementStack.prototype.hasInTableScope;
  assert.throws(() => withSpecTableScope(() => {
    assert.notEqual(OpenElementStack.prototype.hasInTableScope, before);
    throw new Error('x');
  }), /x/);
  assert.equal(OpenElementStack.prototype.hasInTableScope, before);
});

test('t1: implied table parts in template contents exceed D + 2 there (informative); t2 rejects the shape', () => {
  const html = `<div data-rq-block="a"><table><tr><td><template><caption></caption><td>x</td></template></td></tr></table></div>${P}`;
  const k = kinds(html, 't1');
  assert.ok(k.includes('depth-template-enabled'), k.join());
  assert.ok(!k.includes('depth-enabled'), k.join()); // the claim of M-26 (no template open) holds
  assert.equal(analyzeS5(Buffer.from(html, 'utf8'), COMBOS.t2).valid, false);
});

test('known classes are counted apart: p closure around a noscript in C (n1), head noscript', () => {
  const k = kinds(`<div data-rq-block="a"><p>x<noscript><div>y</div></noscript>z</p></div>${P}`, 'n1');
  assert.ok(k.includes('known-p-closure'), k.join());
  assert.deepEqual(k.filter((x) => !isInformative(x)), []);
  assert.equal(analyzeS5(Buffer.from(`<div data-rq-block="a"><p>x<noscript><div>y</div></noscript>z</p></div>${P}`, 'utf8'), COMBOS.n2).valid, false);
});

test('patch apply: the shipped rule misses C under a scripting-disabled parse (noscript mXSS); p1 and p2 do not', () => {
  const html = `<div data-rq-block="a"><noscript><img src=x onerror=alert(1)></noscript><b>t</b></div>${P}`;
  const bytes = Buffer.from(html, 'utf8');
  const r = analyzeS5(bytes, { ...COMBOS.n1, trace: true });
  const p = patchFindings(bytes, r);
  const a = p.blocks.find((b) => b.id === 'a');
  assert.deepEqual(a.rules, { shipped: null, p1: 'noscript', p2: 'attribute' });
  const unseen = p.findings.filter((f) => f.kind === 'patch-unseen-disabled');
  assert.ok(unseen.length > 0 && unseen.every((f) => f.rule === 'shipped'));
  assert.ok(!p.findings.some((f) => f.kind === 'patch-unseen-inert'));
});

test('patch apply: p2 accepts a clean C; its two parse modes differ (a rendering difference)', () => {
  const html = `<div data-rq-block="a"><noscript><img src="/p.gif"></noscript></div>${P}`;
  const bytes = Buffer.from(html, 'utf8');
  const r = analyzeS5(bytes, { ...COMBOS.n1, trace: true });
  const p = patchFindings(bytes, r);
  assert.equal(p.blocks[0].rules.p2, null);
  assert.ok(p.findings.some((f) => f.rule === 'p2' && f.kind === 'patch-mode-diff'));
  assert.ok(!p.findings.some((f) => f.rule === 'p2' && f.kind.startsWith('patch-unseen')));
});

test('patch apply: template contents are read by every rule, and both parse modes agree without noscript', () => {
  const html = `<div data-rq-block="a"><template><b>t</b><img src="/i.png"></template></div>${P}`;
  const bytes = Buffer.from(html, 'utf8');
  const r = analyzeS5(bytes, { ...COMBOS.t2, trace: true });
  assert.deepEqual(patchFindings(bytes, r).findings, []);
});
