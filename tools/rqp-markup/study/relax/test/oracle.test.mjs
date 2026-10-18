// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// The parse5 oracle finds each kind of disagreement. The tests give it forged results
// (a result that a wrong rule could give) and check the finding kinds.

import assert from 'node:assert/strict';
import { test } from 'node:test';
import { analyzeRelaxed, flagsOf } from '../src/markup-relax.mjs';
import { Oracle, isInside, treeIndex } from '../src/oracle.mjs';

const kinds = (html, result) => new Oracle(Buffer.from(html, 'utf8')).check(result).map((f) => f.kind);
const block = (id, start, end) => ({ id, format: 'html', start, end, sha256: 'x' });
const forged = (blocks, content, noscripts = []) => ({ valid: true, error: null, blocks, trace: { content, noscripts, blockNames: blocks.map(() => 'div') } });

test('a valid result of the study rule has no finding', () => {
  for (const html of [
    '<div data-rq-block="a">x<svg><path/></svg>y</div>',
    '<p>x</p><noscript><svg><g></g></svg></noscript><span data-rq-block="b">y</span>',
  ]) {
    const r = analyzeRelaxed(Buffer.from(html), flagsOf('r1+r2+r3'), { trace: true });
    assert.equal(r.valid, true);
    assert.deepEqual(kinds(html, r), [], html);
  }
});

test('an invalid result has no finding (the rules are stricter than the tree on purpose)', () => {
  assert.deepEqual(kinds('<b><div>x</b>', { valid: false, error: 'RQP_MARKUP_STRUCTURE', blocks: [] }), []);
});

test('a breakout tag accepted in svg: wrong block end and wrong namespace', () => {
  const html = '<div data-rq-block="a"><svg><div>x</div></svg></div>';
  const end = html.indexOf('</div></svg>');
  const k = kinds(html, forged([block('a', 23, end)], [[23, false, 28, 'svg'], [28, true, 33, 'div']]));
  assert.ok(k.includes('parse5-enabled-blocks'));
  assert.ok(k.includes('parse5-disabled-blocks'));
  assert.ok(k.includes('tree-enabled-namespace'));
});

test('a start tag that the tree builder moves out of the block is "outside"', () => {
  // In no-quirks mode the table start tag closes the p block in the tree (M-24 a), so the
  // table is not inside it.
  const html = '<!DOCTYPE html><p data-rq-block="a">x<table></table>y</p>';
  const at = html.indexOf('<table>');
  const k = kinds(html, forged([block('a', at - 1, html.indexOf('</p>'))], [[at, false, at + 7, 'table']]));
  assert.ok(k.includes('tree-enabled-outside'), k.join());
});

test('a start tag that makes no element is "missing"', () => {
  // After text the frameset-ok flag is "not ok", and the tree builder ignores the tag.
  const html = '<div data-rq-block="a">x<frameset></div>';
  const k = kinds(html, forged([block('a', 23, html.indexOf('</div>'))], [[24, false, 34, 'frameset']]));
  assert.ok(k.includes('tree-enabled-missing'), k.join());
});

test('noscript content that leaves the noscript element: end and outside findings, with the head flag', () => {
  // Scripting disabled: <div> in C closes the open p and the noscript element with it.
  const body = '<body><p><noscript><div>x</div></noscript>';
  const ns = { tag: 9, start: 19, end: body.indexOf('</noscript>'), tags: [[19, false, 'div']] };
  const k = kinds(body, forged([], [], [ns]));
  assert.ok(k.includes('noscript-end'), k.join());
  assert.ok(k.includes('noscript-outside'), k.join());
  // In head ("in head noscript"), an img pops the noscript and the head.
  const head = '<head><noscript><img src=x></noscript></head>';
  const nh = { tag: 6, start: 16, end: head.indexOf('</noscript>'), tags: [[16, false, 'img']] };
  const kh = kinds(head, forged([], [], [nh]));
  assert.ok(kh.includes('noscript-end-head'), kh.join());
  assert.ok(kh.includes('noscript-outside-head'), kh.join());
});

test('a foreign flag that the tree does not have is a namespace finding in noscript content', () => {
  const html = '<body><noscript><svg></svg><b>x</b></noscript>';
  const ns = { tag: 6, start: 16, end: html.indexOf('</noscript>'), tags: [[16, false, 'svg'], [27, true, 'b']] };
  assert.ok(kinds(html, forged([], [], [ns])).includes('noscript-namespace'));
});

test('treeIndex maps UTF-8 byte offsets (non-BMP text before the tag) and isInside follows parents', () => {
  const html = '\u{1F600}<div><svg><path/></svg></div>';
  const index = treeIndex(Buffer.from(html, 'utf8'), true);
  const div = index.byStart.get(4)[0];
  const path = index.byStart.get(14)[0];
  assert.equal(div.tagName, 'div');
  assert.equal(path.tagName, 'path');
  assert.ok(isInside(path, div));
  assert.ok(!isInside(div, path));
  assert.equal(treeIndex(Buffer.from([0xff]), true), null);
});
