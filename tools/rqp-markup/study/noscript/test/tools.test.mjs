// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// The study tools: the generator, the noscript inventory, the corpus aggregate and
// the sweep command line. The corpus tests use synthetic rows and documents only.

import { test } from 'node:test';
import assert from 'node:assert/strict';
import { PROFILES, generateNoscriptDocument } from '../src/generator.mjs';
import { contentSummary, noscriptInventory, safeName } from '../src/inventory.mjs';
import { aggregate, parseArgs as corpusArgs } from '../src/corpus.mjs';
import { evaluatePage, safeWhy } from '../src/corpus-page.mjs';
import { parseArgs as sweepArgs, sweep } from '../src/sweep.mjs';

const bytes = (s) => Buffer.from(s, 'utf8');

test('the generator is deterministic and bounded for each profile', () => {
  for (const profile of PROFILES) {
    for (let i = 0; i < 300; i++) {
      const a = generateNoscriptDocument(5, i, profile);
      assert.deepEqual(generateNoscriptDocument(5, i, profile), a);
      assert.ok(a.input.length < 20000, `${profile} ${i}: ${a.input.length} bytes`);
      assert.ok(a.input.includes('<noscript>'), `${profile} ${i}`);
    }
  }
  assert.notEqual(generateNoscriptDocument(5, 1, 'body').input, generateNoscriptDocument(6, 1, 'body').input);
  assert.throws(() => generateNoscriptDocument(1, 0, 'other'), RangeError);
});

test('the inventory finds noscript in the head, before the head and elsewhere, with tag names only', () => {
  const input = '<noscript>a</noscript><html><head><noscript><link rel=x>Enable</noscript></head>'
    + '<body><noscript><img src=x><my-Widget></my-Widget><x_y></x_y></noscript><svg><noscript>no</noscript></svg></body>';
  const inv = noscriptInventory(bytes(input));
  assert.equal(inv.preHead, 1);
  assert.equal(inv.head, 1);
  assert.equal(inv.other, 1, 'the svg noscript is a foreign element');
  assert.deepEqual(inv.headTags, ['link']);
  assert.equal(inv.headText, 1);
  assert.deepEqual(inv.h1Failures, ['text']);
  assert.deepEqual(inv.otherTags, ['?', 'img', 'my-widget']);
  assert.ok(!JSON.stringify(inv).includes('Enable'), 'no document text');
});

test('safe names keep short lower-case tag names only', () => {
  assert.equal(safeName('div'), 'div');
  assert.equal(safeName('my-widget'), 'my-widget');
  assert.equal(safeName('a"b'), '?');
  assert.equal(safeName('x'.repeat(40)), '?');
  assert.equal(safeWhy('B:<x>'), 'B:?');
  assert.equal(safeWhy('M-11.3:svg'), 'M-11.3:svg');
  assert.equal(safeWhy('A:M-09.2'), 'A:M-09.2');
  assert.equal(safeWhy(null), null);
});

test('contentSummary separates text, comments and raw text', () => {
  const s = contentSummary(bytes('<style>p{}</style> <!--c-->'));
  assert.deepEqual([...s.tags], ['style']);
  assert.equal(s.text, false);
  assert.equal(s.comment, true);
  assert.equal(contentSummary(bytes('')).empty, true);
});

test('evaluatePage gives numbers and labels, and simulated blocks for a valid page', () => {
  const page = '<!DOCTYPE html><html><head><title>T</title></head><body><h1>Title</h1>'
    + '<noscript><img src=x></noscript><main><p class="price">1</p></main></body></html>';
  const r = evaluatePage(bytes(page), 'utf-8');
  assert.equal(r.markup.shipped.valid, true);
  assert.equal(r.head.h1.at, r.head.shipped.at);
  assert.ok(r.variants.small.markup.a.valid);
  assert.ok(r.variants.main.markup.b0.valid);
  assert.ok(!JSON.stringify(r).includes('Title'), 'no page text');
});

test('aggregate counts pages lost by a model and head disagreements', () => {
  const m = (valid, why = null) => ({ valid, blocks: 0, why, enabled: valid ? 'agree' : null, disabled: valid ? 'agree' : null });
  const h = (at, disabled = 'agree') => ({ at, enabled: at >= 0 ? 'agree' : null, disabled: at >= 0 ? disabled : null });
  const inv = { head: 1, preHead: 0, other: 0, headTags: ['img'], otherTags: [], headText: 0, headComment: 0, headEmpty: 0, h1Failures: ['<img>'], h2Failures: ['<img>'], otherText: 0 };
  const rows = [
    {
      id: 'p1', url: 'https://a.example/', result: {
        inventory: inv, candidateEqual: true, divergences: 0,
        markup: { shipped: m(true), a: m(false, 'A:M-09.2'), 'a-fx': m(false, 'A:M-09.2'), b0: m(true), b1: m(true) },
        head: { shipped: h(10, 'disagree'), h0: h(-1), h1: h(-1), h2: h(-1), hc: h(6) },
        variants: {},
      },
    },
    { id: 'p2', url: 'https://b.example/', error: 'timeout' },
  ];
  const a = aggregate(rows);
  assert.equal(a.pages, 2);
  assert.deepEqual(a.errors, [{ id: 'p2', error: 'timeout' }]);
  assert.equal(a.markup.a.lost, 1);
  assert.deepEqual(a.markup.a.lostReasons, { 'A:M-09.2': 1 });
  assert.equal(a.markup.b0.lost, 0);
  assert.deepEqual(a.head.shipped.disagreeShippedValid.disabled, ['p1']);
  assert.equal(a.head.h1.lostShippedValid, 1);
  assert.equal(a.noscript.pagesHead, 1);
  assert.deepEqual(a.headNoscript.h1Failures, { '<img>': 1 });
  assert.equal(a.combined['shipped+shipped'], 1);
  assert.equal(a.combined['a+h1'], 0);
  assert.equal(a.head.hc.found, 1);
  assert.equal(a.head.hc.gainedShippedValid, 0);
});

test('the command lines reject bad options', () => {
  assert.throws(() => sweepArgs(['--source', 'x']), /--source/);
  assert.throws(() => sweepArgs(['--source', 'body', '--count', '0']), /--count/);
  assert.throws(() => sweepArgs(['--source', 'body', '--seed']), /needs a value/);
  assert.throws(() => corpusArgs(['--evidence', 'x']), /required/);
  assert.throws(() => corpusArgs(['--data', 'd', '--evidence', 'e', '--jobs', '99']), /--jobs/);
});

test('a small sweep agrees with the shipped code and counts disagreements by view', () => {
  const r = sweep(sweepArgs(['--source', 'body', '--seed', '11', '--count', '300']));
  assert.equal(r.count, 300);
  assert.equal(r.candidateMismatch, 0);
  assert.equal(r.modelADivergences, 0);
  for (const v of ['a', 'b0', 'b1']) assert.equal(r.markup[v].disagree.disabled, 0, v);
  assert.ok(r.markup.shipped.valid >= r.markup.a.valid);
  assert.equal(r.markup.shipped.disagree.enabled, 0);
});
