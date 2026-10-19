// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// The study tools: the seeded generator, argument checks, the corpus row (numbers and
// labels only, no page text) and the corpus totals.

import { test } from 'node:test';
import assert from 'node:assert/strict';
import { docSeed } from '../../../crosscheck/src/generator.mjs';
import { generateS5Document } from '../src/gen-s5.mjs';
import { parseArgs as sweepArgs } from '../src/sweep.mjs';
import { aggregate, parseArgs as corpusArgs } from '../src/corpus.mjs';
import { census, measurePage } from '../src/corpus-page.mjs';
import { analyzeS5 } from '../src/markup-s5.mjs';
import { summarize } from '../src/summary.mjs';

test('the generator is deterministic for a seed and a profile, and the profiles differ', () => {
  for (const p of ['tpl', 'nos', 'mix']) assert.equal(generateS5Document(docSeed(1, 7), p), generateS5Document(docSeed(1, 7), p));
  assert.notEqual(generateS5Document(docSeed(1, 7), 'tpl'), generateS5Document(docSeed(1, 8), 'tpl'));
  assert.throws(() => generateS5Document(1, 'x'), /unknown profile/);
  let templates = 0;
  let noscripts = 0;
  for (let i = 0; i < 500; i++) {
    templates += generateS5Document(docSeed(2, i), 'tpl').includes('<template') ? 1 : 0;
    noscripts += generateS5Document(docSeed(2, i), 'nos').includes('<noscript') ? 1 : 0;
  }
  assert.ok(templates > 300 && noscripts > 300, `${templates} ${noscripts}`);
});

test('the sweep and the corpus run reject bad arguments and counts above 500 000', () => {
  assert.throws(() => sweepArgs(['--source', 'x', '--out', 'o.json']), /--source/);
  assert.throws(() => sweepArgs(['--source', 'tpl']), /--out/);
  assert.throws(() => sweepArgs(['--source', 'tpl', '--count', '500001', '--out', 'o.json']), /--count/);
  assert.throws(() => sweepArgs(['--source', 'tpl', '--count']), /needs a value/);
  assert.throws(() => sweepArgs(['--source', 'tpl', '--count', '-1', '--out', 'o.json']), /non-negative/);
  assert.equal(sweepArgs(['--source', 'mix', '--count', '10', '--out', 'o.json']).count, 10);
  assert.throws(() => corpusArgs(['--out', 'e']), /--data or --aggregate/);
  assert.throws(() => corpusArgs(['--out', 'e', '--data', 'd', '--jobs', '0']), /--jobs/);
});

test('the corpus page row has numbers, option names and labels only', () => {
  const page = Buffer.from('<!DOCTYPE html><html><head><title>secret one</title></head><body><main id="content"><h1 class="title">secret two</h1>'
    + '<template id="row"><div>secret three</div></template><noscript><img src="/secret4.gif"></noscript></main></body></html>', 'utf8');
  const row = measurePage(page);
  const text = JSON.stringify(row);
  assert.equal(text.includes('secret'), false);
  assert.equal(row.combos.base.valid, true);
  assert.equal(row.variants.main.combos.base.valid, false);
  assert.equal(row.variants.main.combos.t1n1.valid, true);
  assert.equal(row.variants.main.combos.t1n1.templates, 1);
  assert.equal(row.variants.main.combos.t1n1.noscripts, 1);
  assert.equal(row.variants.main.combos.t1n1.patchBlocked, 1); // p1: noscript in PATCH_FORBIDDEN
  assert.deepEqual(row.variants.main.census.cKinds, { void: 1 });
});

test('the census counts shadowrootmode and templates that start with a table part', () => {
  const html = '<div data-rq-block="a"><template shadowrootmode="open">x</template><table><tr><td><template> <tr><td>r</td></tr></template></td></tr></table></div>';
  const bytes = Buffer.from(html, 'utf8');
  const c = census(bytes, analyzeS5(bytes, { template: 'tu', noscript: 'nu', trace: true }));
  assert.deepEqual({ t: c.templates, s: c.shadowroot, f: c.tableFirst }, { t: 2, s: 1, f: 1 });
});

test('the corpus totals count variants per combination of the pages that it accepts', () => {
  const combos = (valid) => Object.fromEntries(['base', 't1', 't2', 'tu', 'n1', 'n2', 'nu', 't1n1', 't2n2'].map((c) => [c, valid(c)]));
  const v = (ok) => combos((c) => (ok(c) ? { valid: true, templates: c.includes('t') ? 1 : 0, noscripts: 0, patchBlocked: 0 } : { valid: false, why: 'M-22.2-template' }));
  const rows = [
    { id: 'a', charset: 'utf-8', combos: combos(() => ({ valid: true })), variants: { main: { picks: 1, combos: v((c) => c !== 'base' && c[0] === 't'), census: { templates: 1, noscripts: 0, shadowroot: 0, tableFirst: 0, cKinds: {} } } } },
    { id: 'b', charset: 'utf-8', combos: combos(() => ({ valid: true })), variants: { main: { picks: 0 } } },
    { id: 'c', error: 'timeout' },
  ];
  const t = aggregate(rows);
  assert.equal(t.pages, 3);
  assert.equal(t.errors, 1);
  assert.equal(t.combos.base.valid, 2);
  assert.equal(t.variants.main.documents, 1);
  assert.equal(t.variants.main.noPicks, 1);
  assert.equal(t.variants.main.combos.t1.gained, 1);
  assert.equal(t.variants.main.combos.t1.withTemplate, 1);
  assert.equal(t.variants.main.combos.n1.valid, 0);
  assert.equal(t.variants.main.combos.base.lost, 0);
  assert.equal(JSON.stringify(t).includes('"a"'), false); // no page id or URL in the totals
});

test('the summary adds the sweeps per combination and keeps findings apart from informative kinds', () => {
  const row = (valid, gained, findings, gainedFindings, informative = {}) => ({ valid, gained, findings, gainedFindings, informative, docsWithFinding: 0, gainedDocsWithFinding: Object.keys(gainedFindings).length > 0 ? 1 : 0, patch: { blocks: 0, rejected: {}, findings: {} } });
  const sweep = (source, count) => ({ source, count, monotonic: 0, combos: { base: row(10, 0, {}, {}), t1: row(12, 2, {}, { 'content-spec-missing-tpl': 1 }, { 'known-p-closure': 3 }) } });
  const s = summarize([sweep('tpl', 100), sweep('nos', 50)]);
  assert.equal(s.documents, 150);
  assert.equal(s.generated, 150);
  assert.equal(s.combos.t1.valid, 24);
  assert.equal(s.combos.t1.gained, 4);
  assert.deepEqual(s.combos.t1.findings, { 'content-spec-missing-tpl': 2 });
  assert.deepEqual(s.combos.t1.informative, { 'known-p-closure': 6 });
  assert.equal(s.combos.base.findingDocuments, 0);
});
