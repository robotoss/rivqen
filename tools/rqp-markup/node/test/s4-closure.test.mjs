// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// H-85a / DL-050: the implied closure of M-09 (docs/engineering/protocol/markup.md).
// Outside the content of a block and outside C, a table part or col start tag
// closes F as the end tag of F does. Each test checks the reason by a neighbor.

import { describe, it } from 'node:test';
import assert from 'node:assert/strict';
import { ErrorCode, analyze } from '../src/markup.mjs';
import { doc, errorOf, rng } from './helpers.mjs';

const { STRUCTURE, LIMIT } = ErrorCode;

describe('implied table closure (M-09, H-85a)', () => {
  const closes = {
    'a cell by td': '<table><tr><td>a<td>b</td></tr></table>',
    'a cell by th': '<table><tr><td>a<th>b</th></tr></table>',
    'cell and row by tr': '<table><tr><td>a<tr><td>b</td></tr></table>',
    'cell, row and group by tbody': '<table><thead><tr><th>h<tbody><tr><td>b</td></tr></table>',
    'a caption by tr': '<table><caption>c<tr><td>b</td></tr></table>',
    'a caption by col': '<table><caption>c<col></table>',
    'a column group by tr': '<table><colgroup><col><tr><td>b</td></tr></table>',
    'a column group by colgroup': '<table><colgroup><col><colgroup></colgroup></table>',
    'a cell over a div (special-crossed row)': '<table><tr><td><div>x<td>y</td></tr></table>',
  };
  for (const [what, markup] of Object.entries(closes)) {
    it(`accepts the closure of ${what}`, () => assert.equal(errorOf(doc(markup)), null, markup));
  }

  it('rejects a closure over a guarded element (M-06 item 1)', () => {
    assert.equal(errorOf(doc('<table><tr><td><a href="#">a<td>b</td></tr></table>')), STRUCTURE);
  });

  it('rejects a row that is not the current node (step 2)', () => {
    assert.equal(errorOf(doc('<table><tr><td>a</td><b>x<tr></table>')), STRUCTURE);
    assert.equal(errorOf(doc('<table><tr><td>a</td><b>x</b><tr></table>')), null);
  });

  it('rejects a col that closes cell and row, so </tr> matches nothing (M-06 item 3)', () => {
    assert.equal(errorOf(doc('<table><tr><td><col></td></tr></table>')), STRUCTURE);
  });

  it('runs the closure before M-09 condition 1 (DL-050 1)', () => {
    assert.equal(errorOf(doc('<template><tr><td>a<col></template>')), STRUCTURE);
    assert.equal(errorOf(doc('<template><tr><td>a<td>b</td></tr></template>')), null);
  });

  it('passes the special-crossed mark as </td> does (DL-050 2)', () => {
    const open = '<a href="#"><table><tr><td><div>x';
    assert.equal(errorOf(doc(`${open}<td>y</td></tr></table></a>`)), STRUCTURE);
    assert.equal(errorOf(doc(`${open}</div></td></tr></table></a>`)), null);
  });

  it('does not close in the content of a block', () => {
    const inner = '<table><tr><td>a<td>b</td></tr></table>';
    assert.equal(errorOf(doc(`<div data-rq-block="t">${inner}</div>`)), STRUCTURE);
  });

  it('does not close in the content C of a noscript', () => {
    assert.equal(errorOf(doc('<table><tr><td><noscript><td>x</td></noscript></td></tr></table>')), STRUCTURE);
  });

  it('does not close in the C pass either: a table of C needs the explicit end tag', () => {
    assert.equal(errorOf(doc('<noscript><table><tr><td>a<td>b</td></tr></table></noscript>')), STRUCTURE);
    assert.equal(errorOf(doc('<noscript><table><tr><td>a</td><td>b</td></tr></table></noscript>')), null);
  });

  it('never closes a table', () => {
    const nested = '<table><tr><td><table><tr><td>a<td>b</td></tr></table></td></tr></table>';
    assert.equal(errorOf(doc(nested)), null);
  });

  it('does not release D: a closure over elements gives no unit back', () => {
    const open = '<div>'.repeat(250);
    assert.equal(errorOf(doc(`<table><tr><td>${open}<td>`)), LIMIT);
  });

  it('property: a document reads as the same document with the closing end tags written out', () => {
    const allowed = {
      caption: ['table'], colgroup: ['table'], tbody: ['table'], thead: ['table'], tfoot: ['table'],
      col: ['table', 'colgroup'], tr: ['table', 'tbody', 'thead', 'tfoot'],
      td: ['table', 'tbody', 'thead', 'tfoot', 'tr'], th: ['table', 'tbody', 'thead', 'tfoot', 'tr'],
    };
    const names = ['table', ...Object.keys(allowed)];
    const r = rng(0x85a);
    let validCount = 0;
    for (let i = 0; i < 1500; i++) {
      const stack = [];
      let implied = '';
      let explicit = '';
      for (let k = r.int(10) + 1; k > 0; k--) {
        const name = r.pick(names);
        let steps = 0;
        while (name !== 'table' && stack.length > 0 && steps < 3 && !allowed[name].includes(stack.at(-1))) {
          explicit += `</${stack.pop()}>`;
          steps++;
        }
        implied += `<${name}>`;
        explicit += `<${name}>`;
        if (name !== 'col') stack.push(name);
      }
      const a = analyze(Buffer.from(doc(implied)));
      const b = analyze(Buffer.from(doc(explicit)));
      assert.equal(a.valid, b.valid, `${implied} vs ${explicit}`);
      if (a.valid) {
        validCount++;
        assert.deepEqual(a.blocks, b.blocks);
      }
    }
    assert.ok(validCount > 50, 'the generator must produce valid documents');
  });
});
