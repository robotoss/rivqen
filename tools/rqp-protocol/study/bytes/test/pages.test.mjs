// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

import assert from 'node:assert/strict';
import { test } from 'node:test';
import { analyze } from '../../../src/markup.mjs';
import { nextData, renderCatalog } from '../../../../../examples/server-demo/src/pages.js';
import { LARGE_BYTES, SMALL_BYTES, blockContent, demoPage, gridId, gridPage, stripMarkup, toLegacy } from '../src/pages.mjs';

const count = (buf, s) => buf.toString('utf8').split(s).length - 1;

test('block contents have exactly 20 and 2048 bytes and depend on index and version', () => {
  for (let i = 0; i < 64; i++) {
    assert.equal(Buffer.byteLength(blockContent('small', i, 0)), SMALL_BYTES);
    assert.equal(Buffer.byteLength(blockContent('large', i, 1)), LARGE_BYTES);
  }
  assert.notEqual(blockContent('small', 3, 0), blockContent('small', 3, 1));
  assert.notEqual(blockContent('large', 3, 0), blockContent('large', 4, 0));
  assert.equal(blockContent('large', 5, 1), blockContent('large', 5, 1));
});

test('a synthetic page is valid for RQP and changes one block or all blocks', () => {
  for (const blocks of [1, 4, 16, 64]) {
    const a = analyze(gridPage({ blocks, size: 'small' }));
    const one = analyze(gridPage({ blocks, size: 'small', version: 1, changed: 'one' }));
    const all = analyze(gridPage({ blocks, size: 'small', version: 1, changed: 'all' }));
    assert.equal(a.valid && one.valid && all.valid, true);
    assert.deepEqual(a.blocks.map((b) => b.id), Array.from({ length: blocks }, (_, i) => gridId(i)));
    assert.equal(a.template_revision, one.template_revision);
    const changedOne = a.blocks.filter((b, i) => b.sha256 !== one.blocks[i].sha256).length;
    const changedAll = a.blocks.filter((b, i) => b.sha256 !== all.blocks[i].sha256).length;
    assert.equal(changedOne, 1);
    assert.equal(changedAll, blocks);
  }
});

test('the legacy form has a marker pair for each block except the first title, and no data-rq-block', () => {
  const page = demoPage(0);
  const legacy = toLegacy(page);
  assert.equal(count(legacy, 'data-rq-block'), 0);
  assert.equal(count(legacy, '<!--sonicdiff-'), 6); // price, stock, cart: start and end markers
  assert.equal(count(legacy, '<title>Shop — Catalog (120.00)</title>'), 1);
  const h82 = toLegacy(page, { titleMarkers: true });
  assert.equal(count(h82, '<!--sonicdiff-title--><title>'), 1);
  // Without the markers, the legacy form is the page without block markup.
  const unmarked = Buffer.from(legacy.toString('utf8').replace(/<!--sonicdiff-\w+?(?:-end)?-->/g, ''), 'utf8');
  assert.ok(unmarked.equals(stripMarkup(page)));
  assert.equal(page.length - stripMarkup(page).length, [' data-rq-block="title"', ' data-rq-block="price"', ' data-rq-block="stock"', ' data-rq-block="cart"'].join('').length);
});

test('the demo page of a data step is the page that the demo server renders at that step', () => {
  const step0 = demoPage(0);
  const step1 = demoPage(1);
  assert.ok(step0.equals(Buffer.from(renderCatalog(), 'utf8')));
  nextData(); // changes the module state of the demo: keep this test last in the file (each test file runs in its own process)
  assert.ok(step1.equals(Buffer.from(renderCatalog(), 'utf8')));
  assert.throws(() => demoPage(0), { code: 'STUDY_PAGE' });
});
