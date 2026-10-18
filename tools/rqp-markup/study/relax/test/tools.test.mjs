// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// The generator and the drivers of the study (agree.mjs, corpus-page.mjs, corpus.mjs).

import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { test } from 'node:test';
import { docSeed } from '../../../crosscheck/src/generator.mjs';
import { attrWalkFinding, evaluate, parseArgs as agreeArgs } from '../src/agree.mjs';
import { subsetsOf } from '../src/combos.mjs';
import { measurePage } from '../src/corpus-page.mjs';
import { parseArgs as corpusArgs, summarize } from '../src/corpus.mjs';
import { generateRelaxDocument } from '../src/gen.mjs';
import { COMBOS, analyzeRelaxed, flagsOf } from '../src/markup-relax.mjs';

test('generator: the same document for the same seed (known answer)', () => {
  let h = createHash('sha256');
  for (let i = 0; i < 200; i++) h.update(generateRelaxDocument(docSeed(5, i)));
  const first = h.digest('hex');
  h = createHash('sha256');
  for (let i = 0; i < 200; i++) h.update(generateRelaxDocument(docSeed(5, i)));
  assert.equal(h.digest('hex'), first);
  assert.equal(generateRelaxDocument(docSeed(5, 0)), generateRelaxDocument(docSeed(5, 0)));
  assert.notEqual(generateRelaxDocument(docSeed(5, 0)), generateRelaxDocument(docSeed(5, 1)));
});

test('generator: bounded documents that reach the shapes of the three relaxations', () => {
  const seen = { block: 0, noscriptBody: 0, noscriptHead: 0, integrationTag: 0, breakout: 0, math: 0, gainedR1: 0, gainedR2: 0, gainedR3: 0 };
  let max = 0;
  for (let i = 0; i < 5000; i++) {
    const d = generateRelaxDocument(docSeed(6, i));
    max = Math.max(max, d.length);
    if (/data-rq-block="b">[^]*<(svg|math)/.test(d)) seen.block++;
    if (/^(x|<p>x<\/p>|<body>)<noscript><[^]*svg/.test(d)) seen.noscriptBody++;
    if (d.startsWith('<!DOCTYPE html><html><head>') && d.includes('<noscript>')) seen.noscriptHead++;
    const bytes = Buffer.from(d, 'utf8');
    if (!analyzeRelaxed(bytes, flagsOf('base')).valid) {
      if (analyzeRelaxed(bytes, flagsOf('r1')).valid) seen.gainedR1++;
      if (analyzeRelaxed(bytes, flagsOf('r2')).valid) seen.gainedR2++;
      if (analyzeRelaxed(bytes, flagsOf('r3')).valid) seen.gainedR3++; // the icon shape: unclosed foreign children
    }
    if (/<(desc|mi|foreignObject)[^>]*><(b|div|p|span|img)/.test(d)) seen.integrationTag++;
    if (/<svg[^>]*>[^]*<(div|p|b|br|table)>/.test(d)) seen.breakout++;
    if (d.includes('<math')) seen.math++;
  }
  for (const [k, n] of Object.entries(seen)) assert.ok(n > 20, `shape ${k}: ${n}`);
  assert.ok(max < 16 * 1024, `max ${max}`);
});

test('subsetsOf: base is in every set; r3 is a subset of r3w; no combination is its own subset', () => {
  assert.deepEqual(subsetsOf('base'), []);
  assert.deepEqual(subsetsOf('r1+r3'), ['base', 'r1', 'r3']);
  assert.ok(subsetsOf('r1+r2+r3w').includes('r1+r2+r3'));
  for (const c of COMBOS) assert.ok(!subsetsOf(c).includes(c));
});

test('agree: options, and one document evaluated for every combination', () => {
  assert.deepEqual(agreeArgs(['--relax', '5', '--seed', '2']), { relax: 5, profileCount: 20000, seed: 2, keep: 3, out: null });
  assert.throws(() => agreeArgs(['--relax', '-1']));
  assert.throws(() => agreeArgs(['--bad', '1']));
  const stats = Object.fromEntries(COMBOS.map((c) => [c, { valid: 0, gained: 0, findings: {}, gainedFindings: {}, docsWithFinding: 0, gainedDocsWithFinding: 0, gainedDocsNotHead: 0, examples: [] }]));
  const totals = {};
  const text = '<div data-rq-block="a"><svg><path/></svg></div>';
  evaluate({ label: 't', bytes: Buffer.from(text), show: text }, stats, totals, 2);
  assert.equal(stats.base.valid, 0);
  assert.equal(stats.r1.valid, 1);
  assert.equal(stats.r1.gained, 1);
  assert.deepEqual(totals, {});
});

test('agree: the attribute walk check skips title blocks and finds a different start tag list', () => {
  const html = '<title data-rq-block="t"><b>x</b></title><div data-rq-block="a"><svg><path/></svg></div>';
  const r = analyzeRelaxed(Buffer.from(html), flagsOf('r1'), { trace: true });
  assert.equal(r.valid, true);
  assert.equal(attrWalkFinding(Buffer.from(html), r), null);
  const forged = { ...r, trace: { ...r.trace, content: r.trace.content.slice(1) } };
  assert.match(attrWalkFinding(Buffer.from(html), forged), /block a: walk 2 start tags, token pass 1/);
});

test('corpus-page: the output has numbers and names only, never page text', () => {
  const marker = 'SECRETPAGETEXT';
  const page = `<!DOCTYPE html><html><head><title>${marker}</title></head><body><h1 class="title">${marker}</h1><main><p>${marker}</p><svg><path d="${marker}"/></svg></main><noscript><svg></svg></noscript></body></html>`;
  const out = measurePage(Buffer.from(page));
  const text = JSON.stringify(out);
  assert.ok(!text.includes(marker));
  assert.equal(out.baseEqualsShipped, true);
  assert.equal(out.page.base.why, 'M-11.3:svg');
  assert.equal(out.page.r2.valid, true);
  assert.equal(out.variants.main.combos['r1+r2'].valid, true);
  assert.equal(out.variants.main.combos.r2.why, 'M-22.2:svg');
});

test('corpus: options and the summary of page lines', () => {
  assert.throws(() => corpusArgs(['--data', 'x']));
  assert.equal(corpusArgs(['--data', 'a', '--scratch', 'b', '--evidence', 'c']).jobs, 6);
  const row = (valid, why) => Object.fromEntries(COMBOS.map((c) => [c, { valid: valid(c), why: valid(c) ? null : why, blocks: 0 }]));
  const lines = [
    { id: 'p1', url: 'https://a.example/', four: { allValid: true, agree: true }, baseEqualsShipped: true, page: row(() => true, null), variants: {} },
    { id: 'p2', url: 'https://b.example/', four: { allValid: false, agree: true }, baseEqualsShipped: true, page: row((c) => c.includes('r2'), 'M-11.3:svg'), variants: {} },
    { id: 'p3', error: 'timeout' },
  ];
  const s = summarize(lines);
  assert.equal(s.pages, 3);
  assert.deepEqual(s.workerErrors, [{ id: 'p3', error: 'timeout' }]);
  assert.equal(s.fourAllValid, 1);
  assert.equal(s.page.base.valid, 1);
  assert.equal(s.page.r2.valid, 2);
  assert.deepEqual(s.page.r2.gainedUrls, ['https://b.example/']);
  assert.deepEqual(s.page.r1.why, { 'M-11.3:svg': 1 });
});
