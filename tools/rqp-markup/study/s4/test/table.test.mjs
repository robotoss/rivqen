// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// Behavior of the H-85 study: the implied closure of the study rule (close modes b1,
// b2, b3 and the control u), the depth counter with the closure, and the oracle.

import { test } from 'node:test';
import assert from 'node:assert/strict';
import { analyzeDepth } from '../../../crosscheck/src/depth.mjs';
import { analyzeS4 } from '../src/markup-s4.mjs';
import { analyzeDepthClosure } from '../src/depth-s4.mjs';
import { checkResult, rewrite } from '../src/table-oracle.mjs';

const B = (s) => Buffer.from(s, 'utf8');
const run = (s, close) => analyzeS4(B(s), { close, trace: true });
const AFTER = '<div data-rq-block="a">x</div>';

test('b1 and b2 close a cell, a row, a row group, a caption and a column group as the tree builder does', () => {
  const cases = [
    ['<table><tr><td>a<td>b</table>', { cell: 1 }],
    ['<table><tr><td>a<tr><td>b</table>', { cell: 1, row: 1 }],
    ['<table><thead><tr><th>a<tbody><tr><td>b</table>', { cell: 1, row: 1, group: 1 }],
    ['<table><caption>c<tr><td>x</table>', { caption: 1 }],
    ['<table><colgroup><col><tr><td>x</table>', { colgroup: 1 }],
    ['<table><td>a<td>b<tr><td>c</table>', { cell: 2 }],
  ];
  for (const [s, want] of cases) {
    assert.equal(run(s + AFTER, 'off').valid, false, s);
    for (const close of ['b1', 'b2']) {
      const r = run(s + AFTER, close);
      assert.equal(r.valid, true, `${close} ${s}`);
      for (const [k, n] of Object.entries(want)) assert.equal(r.closures[k], n, `${close} ${s} ${k}`);
      assert.deepEqual(checkResult(B(s + AFTER), r), [], `${close} ${s}`);
    }
  }
});

test('b1 needs F as the current node; b2 crosses other elements as the end tag of F does', () => {
  const s = `<table><tr><td><div>a<td>b</table>${AFTER}`;
  assert.equal(run(s, 'b1').why, 'close-current');
  const r = run(s, 'b2');
  assert.equal(r.valid, true);
  assert.deepEqual(checkResult(B(s), r), []);
});

test('b2 keeps M-06 item 1 (guarded elements) and M-09 condition 2 for the closure', () => {
  assert.equal(run(`<table><tr><td><a href="#">a<td>b</table>${AFTER}`, 'b2').why, 'close-crossing');
  assert.equal(run(`<table><tr><td><object><td>b</table>${AFTER}`, 'b2').why, 'close-crossing');
  assert.equal(run(`<table><tr><object><tr><td>b</table>${AFTER}`, 'b2').why, 'close-context');
});

test('the closure is not used in block content (M-23) or in noscript content (M-11)', () => {
  assert.equal(run('<div data-rq-block="a"><table><tr><td>a<td>b</table></div>', 'b2').why, 'M-09');
  assert.equal(run('<noscript><table><tr><td>a<td>b</table></noscript>', 'b2').why, 'M-09');
  assert.equal(run('<noscript><table><tr><td>a</td><td>b</td></tr></table></noscript>', 'b2').valid, true);
});

test('the control u (no guard, no condition 2) is unsafe: parse5 has no end tag for the block in both views', () => {
  const s = '<table><tr><td><a href="#">y<object><td>x</table>z<span data-rq-block="b">a<a href="#">b</a>c</span>';
  const r = run(s, 'u');
  assert.equal(r.valid, true);
  const kinds = checkResult(B(s), r).map((f) => f.kind).sort();
  assert.deepEqual(kinds, ['parse5-disabled-blocks', 'parse5-enabled-blocks', 'rewrite-invalid']);
  assert.equal(run(s, 'b2').valid, false);
});

test('an exact closure gives its units back: 300 rows of unclosed cells stay far below the limit of D', () => {
  const s = `<table>${'<tr><td>a<td>b'.repeat(300)}</table>${AFTER}`;
  for (const close of ['b1', 'b2']) assert.equal(run(s, close).valid, true, close);
  const d = analyzeDepthClosure(B(s));
  assert.ok(d.maxD <= 5, `max D ${d.maxD}`);
  assert.deepEqual(d.violations, []);
});

test('b2 keeps D of M-26 for a crossing closure; b3 frees the units of a cell or caption pop', () => {
  // Each closure over an open span makes the row dirty in b2, so D does not go down.
  const s = `<table><tr>${'<td><span>a'.repeat(300)}</table>`;
  assert.equal(run(s, 'b2').error, 'RQP_MARKUP_LIMIT');
  assert.equal(run(s, 'b3').valid, true);
  for (const scripting of [true, false]) assert.deepEqual(analyzeDepthClosure(B(s), { scripting, release: true }).violations, []);
  // The end tag </table> over an open row and cell leaks D in the shipped rule; b3 does
  // not free it (a foster-parented formatting element can be reconstructed after it).
  const t = '<table><tr><td>x</table>'.repeat(100);
  assert.equal(run(t, 'off').error, 'RQP_MARKUP_LIMIT');
  assert.equal(run(t, 'b3').error, 'RQP_MARKUP_LIMIT');
});

test('b3 does not release for a row group end tag: the reconstructed formatting element keeps the depth claim', () => {
  // b3 does not release for </tfoot>; with the release, D would be 1 after </table>.
  const s = B('<span><table><tfoot><b></tfoot></table>z');
  const r = run(s.toString(), 'b3');
  assert.equal(r.valid, true);
  for (const scripting of [true, false]) assert.deepEqual(analyzeDepthClosure(s, { scripting, release: true }).violations, []);
});

test('the closure counter equals the cross-check counter on a document without a closure', () => {
  const s = '<table><tr><td>a</td><td><span><div></span></td></tr></table><p>x</p>';
  for (const scripting of [true, false]) {
    const a = analyzeDepth(B(s), { scripting });
    const b = analyzeDepthClosure(B(s), { scripting });
    assert.deepEqual(b, a);
  }
});

test('rewrite inserts the implied end tags and maps offsets after them', () => {
  const s = B(`<table><tr><td>a<td>b</table>${AFTER}`);
  const r = run(s.toString(), 'b2');
  assert.deepEqual(r.virtual, [[16, 'td']]);
  const { bytes, shift } = rewrite(s, r.virtual);
  assert.equal(bytes.toString(), `<table><tr><td>a</td><td>b</table>${AFTER}`);
  assert.equal(shift(15), 15);
  assert.equal(shift(16), 21);
});

test('a table start tag in table context stops the trace (the shipped token stack keeps the outer table)', () => {
  const r = run('<table><tr><table></table><td><div data-rq-block="a">x</div></td></tr></table>', 'off');
  assert.equal(r.valid, true);
  assert.equal(r.traceStopped, true);
  assert.deepEqual(r.trace, [[7, 'tr', 0]]);
});
