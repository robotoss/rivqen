#!/usr/bin/env node
// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// Agreement sweep of the S5 study. For each document and each option combination
// (src/options.mjs) the study rule gives a result. For each valid result the parse5
// oracle (src/oracle-s5.mjs) checks both scripting views, the content, template and
// noscript elements, the fragment parse and the depth claim; the patch oracle
// (src/patch-s5.mjs) checks the patch rule variants under both apply modes. The run
// also checks "monotonic": a document that a smaller combination accepts gets the
// same result from every larger one.
//
// Sources: fixtures (golden markup fixtures), cases (all cross-check case files),
// classic, review, review2, review3 (the cross-check generator profiles, 20 pieces
// at most) and tpl, nos, mix (src/gen-s5.mjs).
//
// Usage: node src/sweep.mjs --source <name> [--seed n] [--count n] [--examples n] --out <file>
// The output has numbers, rule labels, option names, fixture and case names and
// synthetic example documents only.

import { writeFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { docSeed, generateDocument } from '../../../crosscheck/src/generator.mjs';
import { generateS5Document } from './gen-s5.mjs';
import { crosscheckCases, markupFixtures } from './inputs.mjs';
import { analyzeS5 } from './markup-s5.mjs';
import { COMBOS, COMBO_NAMES, SUBSETS, resultKey } from './options.mjs';
import { Oracle, isInformative } from './oracle-s5.mjs';
import { patchFindings } from './patch-s5.mjs';

export const SOURCES = Object.freeze(['fixtures', 'cases', 'classic', 'review', 'review2', 'review3', 'tpl', 'nos', 'mix']);
const MAX_COUNT = 500_000;

export function parseArgs(argv) {
  const o = { source: null, seed: 5, count: 100_000, examples: 3, out: null };
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i];
    const v = argv[++i];
    if (v === undefined) throw new Error(`option ${a} needs a value`);
    if (a === '--source') o.source = v;
    else if (a === '--out') o.out = path.resolve(v);
    else {
      const n = Number(v);
      if (!Number.isSafeInteger(n) || n < 0) throw new Error(`option ${a} needs a non-negative integer`);
      if (a === '--seed') o.seed = n;
      else if (a === '--count') o.count = n;
      else if (a === '--examples') o.examples = n;
      else throw new Error(`unknown option ${a}`);
    }
  }
  if (!SOURCES.includes(o.source)) throw new Error(`--source must be one of ${SOURCES.join(', ')}`);
  if (o.out === null) throw new Error('--out is required');
  if (o.count > MAX_COUNT) throw new Error(`--count must be at most ${MAX_COUNT}`);
  return o;
}

/** The documents of a source: { label, bytes, show } (show: the text of a synthetic document, else null). */
export function* documents(source, seed, count) {
  if (source === 'fixtures') {
    for (const [n, b] of markupFixtures()) yield { label: n, bytes: b, show: null };
    return;
  }
  if (source === 'cases') {
    for (const [n, b] of crosscheckCases()) yield { label: n, bytes: b, show: null };
    return;
  }
  for (let i = 0; i < count; i++) {
    const s = docSeed(seed, i);
    const text = ['tpl', 'nos', 'mix'].includes(source) ? generateS5Document(s, source) : generateDocument(s, { maxTokens: 20, profile: source });
    yield { label: `${source} seed=${seed} index=${i}`, bytes: Buffer.from(text, 'utf8'), show: text };
  }
}

const bump = (o, k, n = 1) => {
  o[k] = (o[k] ?? 0) + n;
};

function newRow() {
  return {
    valid: 0,
    gained: 0,
    withTemplate: 0,
    withNoscript: 0,
    findings: {},
    gainedFindings: {},
    informative: {},
    docsWithFinding: 0,
    gainedDocsWithFinding: 0,
    why: {},
    patch: { blocks: 0, rejected: {}, findings: {} },
    examples: [],
  };
}

export function newStats() {
  return { docs: 0, combos: Object.fromEntries(COMBO_NAMES.map((c) => [c, newRow()])), monotonic: 0, examples: [] };
}

/** True when a block of a valid traced result has a start tag `name` in its content (main pass). */
function blocksHave(result, name) {
  return result.trace.content.some(([at, , , n]) => n === name && result.blocks.some((b) => at >= b.start && at < b.end));
}

/** Evaluate one document for every combination. */
export function evaluate(doc, stats, keep) {
  stats.docs++;
  const results = new Map();
  for (const c of COMBO_NAMES) results.set(c, analyzeS5(doc.bytes, { ...COMBOS[c], trace: true }));
  const base = results.get('base');
  const oracle = new Oracle(doc.bytes);
  const checked = new Map();
  const patched = new Map();
  for (const c of COMBO_NAMES) {
    const r = results.get(c);
    const s = stats.combos[c];
    const example = (kind, detail) => {
      if (s.examples.filter((e) => e.kind === kind).length < keep) s.examples.push({ doc: doc.show ?? doc.label, kind, detail });
    };
    if (!r.valid) {
      bump(s.why, r.why ?? r.error);
      continue;
    }
    s.valid++;
    const gained = !base.valid;
    if (gained) s.gained++;
    if (blocksHave(r, 'template')) s.withTemplate++;
    if (blocksHave(r, 'noscript')) s.withNoscript++;
    const key = resultKey(r);
    if (!checked.has(key)) checked.set(key, oracle.check(r));
    if (!patched.has(key)) patched.set(key, patchFindings(doc.bytes, r));
    let found = false;
    for (const f of checked.get(key)) {
      if (isInformative(f.kind)) {
        bump(s.informative, f.kind);
        continue;
      }
      found = true;
      bump(gained ? s.gainedFindings : s.findings, f.kind);
      example(gained ? `gained:${f.kind}` : f.kind, f.detail);
    }
    if (found) {
      if (gained) s.gainedDocsWithFinding++;
      else s.docsWithFinding++;
    }
    const p = patched.get(key);
    for (const b of p.blocks) {
      s.patch.blocks++;
      for (const [rule, failure] of Object.entries(b.rules)) if (failure !== null) bump(s.patch.rejected, `${rule}:${failure}`);
    }
    for (const f of p.findings) {
      bump(s.patch.findings, `${f.rule}:${f.kind}`);
      example(`patch:${f.rule}:${f.kind}`, f.detail);
    }
  }
  for (const [small, large] of SUBSETS) {
    const a = results.get(small);
    const b = results.get(large);
    if (a.valid && resultKey(a) !== resultKey(b)) {
      stats.monotonic++;
      if (stats.examples.length < keep) stats.examples.push({ doc: doc.show ?? doc.label, kind: 'monotonic', detail: `${small} valid, ${large} ${b.valid ? 'other blocks' : b.why ?? b.error}` });
    }
  }
}

/** Run one source. */
export function sweep(o, log = () => {}) {
  const stats = newStats();
  const t0 = Date.now();
  for (const doc of documents(o.source, o.seed, o.count)) {
    evaluate(doc, stats, o.examples);
    if (stats.docs % 50_000 === 0) log(`  ${stats.docs} documents, ${((Date.now() - t0) / 1000).toFixed(0)} s`);
  }
  return { source: o.source, seed: o.seed, count: stats.docs, date: new Date().toISOString().slice(0, 10), ...stats };
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    const o = parseArgs(process.argv.slice(2));
    const log = (s) => process.stderr.write(`${s}\n`);
    const out = sweep(o, log);
    writeFileSync(o.out, `${JSON.stringify(out, null, 1)}\n`);
    const summary = Object.fromEntries(COMBO_NAMES.map((c) => [c, { valid: out.combos[c].valid, gained: out.combos[c].gained, findings: out.combos[c].gainedDocsWithFinding + out.combos[c].docsWithFinding }]));
    log(`${o.source}: ${out.count} documents, monotonic ${out.monotonic}; ${JSON.stringify(summary)}`);
  } catch (err) {
    process.stderr.write(`sweep: ${err instanceof Error ? err.message : String(err)}\n`);
    process.exit(2);
  }
}
