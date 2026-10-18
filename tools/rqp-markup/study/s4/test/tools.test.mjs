// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// The study tools: seeded generators, argument checks, bounded sweeps, the corpus
// totals and the labels of the evidence (no page text).

import { test } from 'node:test';
import assert from 'node:assert/strict';
import { generateHeadDocument } from '../src/head-gen.mjs';
import { generateTableDocument } from '../src/table-gen.mjs';
import { parseArgs as headArgs, sweep as headSweep } from '../src/head-sweep.mjs';
import { parseArgs as tableArgs, sweep as tableSweep } from '../src/table-sweep.mjs';
import { aggregate, parseArgs as corpusArgs } from '../src/corpus.mjs';
import { headStudy, measurePage } from '../src/corpus-page.mjs';

test('the generators are deterministic for a seed and an index, and differ between indices', () => {
  assert.equal(generateHeadDocument(1, 7), generateHeadDocument(1, 7));
  assert.equal(generateTableDocument(1, 7), generateTableDocument(1, 7));
  assert.notEqual(generateTableDocument(1, 7), generateTableDocument(1, 8));
  assert.notEqual(generateHeadDocument(1, 7), generateHeadDocument(2, 7));
});

test('the sweeps reject unknown sources, missing outputs and counts above 500 000', () => {
  assert.throws(() => headArgs(['--source', 'x', '--out', 'o.json']), /--source/);
  assert.throws(() => tableArgs(['--source', 'tables']), /--out/);
  assert.throws(() => tableArgs(['--source', 'tables', '--count', '500001', '--out', 'o.json']), /--count/);
  assert.throws(() => headArgs(['--source', 'gen', '--count']), /needs a value/);
  assert.throws(() => corpusArgs(['--evidence', 'e']), /--data or --aggregate/);
  assert.equal(tableArgs(['--source', 'review2', '--count', '10', '--out', 'o.json']).count, 10);
});

test('a small table sweep: no finding for b1, b2 and b3, gains for every closure mode', () => {
  const out = tableSweep({ source: 'tables', seed: 85, count: 3000, examples: 1, control: 10, out: null });
  assert.equal(out.docs, 3000);
  assert.equal(out.offNotShipped, 0);
  for (const m of ['b1', 'b2', 'b3']) {
    assert.ok(out.modes[m].gained > 50, `${m} gained ${out.modes[m].gained}`);
    assert.deepEqual(out.modes[m].findings, {}, m);
  }
  assert.ok(out.modes.b2.gained >= out.modes.b1.gained);
});

test('a small head sweep: the h79 rule has no failure and the hostile client c2 accepts only the server position', () => {
  const gen = headSweep({ source: 'gen', seed: 79, count: 1500, examples: 1, out: null });
  assert.ok(gen.rules.h79.implied > 500);
  assert.deepEqual(gen.rules.h79.failures.counts, {});
  const hostile = headSweep({ source: 'hostile', seed: 79, count: 500, examples: 1, out: null });
  assert.deepEqual(hostile.hostile.c2Failures.counts, {});
  assert.ok(hostile.hostile.c1Only > 0);
});

test('fixture sweeps report fixture names, not document text', () => {
  const out = tableSweep({ source: 'cases', seed: 0, count: 1, examples: 3, control: 1, out: null });
  for (const m of Object.values(out.modes)) {
    for (const list of Object.values(m.examples)) for (const e of list) assert.match(e, /^[a-z0-9-]+\.json:[A-Za-z0-9-]+$/);
  }
});

test('the corpus page row has numbers and tag names only', () => {
  const row = measurePage(Buffer.from('<meta charset="utf-8"><title>secret text</title><p>x</p>', 'utf8'));
  assert.equal(JSON.stringify(row).includes('secret'), false);
  assert.equal(row.head.h79.kind, 'implied');
  assert.equal(row.head.h79.pass, true);
  assert.equal(row.head.shipped.kind, 'none');
  const table = measurePage(Buffer.from('<title>secret text</title><table><tr><td>a<td>b</table>', 'utf8'));
  assert.equal(JSON.stringify(table).includes('secret'), false);
  assert.equal(table.close.off.valid, false);
  assert.equal(table.close.b2.valid, true);
  assert.equal(table.head.h79.pass, undefined, 'no parse5 check for a page that the shipped rules reject');
  assert.equal(headStudy(Buffer.from('<x\u0001y>'), true).h79.stop, '?');
});

test('the corpus totals count gained and lost pages per rule and mode', () => {
  const page = (url, headKind, offValid, b2Valid) => ({
    url,
    charset: 'utf-8',
    shipped: { valid: true },
    head: { shipped: { kind: headKind === 'implied' ? 'none' : headKind, stop: '<meta>' }, h79: { kind: headKind, stop: '<meta>', pass: true } },
    close: { off: { valid: offValid }, b2: { valid: b2Valid } },
    variants: {},
  });
  const t = aggregate([page('https://a/', 'implied', false, true), page('https://b/', 'explicit', true, true), { error: 'timeout' }]);
  assert.equal(t.errors, 1);
  assert.deepEqual(t.head.h79.gained, ['https://a/']);
  assert.deepEqual(t.head.h79.lost, []);
  assert.deepEqual(t.close.b2.gained, ['https://a/']);
  assert.equal(t.close.b2.lost, 0);
});
