#!/usr/bin/env node
// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// Agreement runs of the H-42 study (WP-17 S3 T-02) on synthetic documents and the
// repository inputs: the golden markup fixtures, the cross-check cases, the
// generated noscript sweeps (profiles body and head) and the review2 profile of the
// cross-check generator. One process, one document at a time.
//
// Usage:
//   node src/sweep.mjs --source fixtures|cases|body|head|review2 [--seed n] [--count n]
//                      [--examples n] [--out file.json]
//
// The output has counts only, plus at most --examples synthetic inputs for each
// disagreement group. No corpus page is read here.

import { readFileSync, readdirSync, writeFileSync, existsSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { generateDocument } from '../../../crosscheck/src/generator.mjs';
import { docSeed, generateNoscriptDocument } from './generator.mjs';
import { HEAD_VARIANTS, MARKUP_VARIANTS, evaluateDocument } from './evaluate.mjs';

const HERE = path.dirname(fileURLToPath(import.meta.url));
const MARKUP_ROOT = path.resolve(HERE, '..', '..', '..');
const REPO_ROOT = path.resolve(MARKUP_ROOT, '..', '..');
export const SOURCES = Object.freeze(['fixtures', 'cases', 'body', 'head', 'review2']);
const MAX_COUNT = 2_000_000;
const MAX_SIGNATURES = 40;
const MAX_LOST_NAMES = 60;

export function parseArgs(argv) {
  const o = { source: null, seed: 1, count: 1000, examples: 3, out: null };
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i];
    const v = argv[++i];
    if (v === undefined) throw new Error(`option ${a} needs a value`);
    if (a === '--source') o.source = v;
    else if (a === '--seed') o.seed = Number(v);
    else if (a === '--count') o.count = Number(v);
    else if (a === '--examples') o.examples = Number(v);
    else if (a === '--out') o.out = path.resolve(v);
    else throw new Error(`unknown option ${a}`);
  }
  if (!SOURCES.includes(o.source)) throw new Error(`--source must be one of ${SOURCES.join(', ')}`);
  if (!Number.isInteger(o.seed) || o.seed < 0 || o.seed > 0xffffffff) throw new Error('--seed must be 0 … 2^32 − 1');
  if (!Number.isInteger(o.count) || o.count < 1 || o.count > MAX_COUNT) throw new Error(`--count must be 1 … ${MAX_COUNT}`);
  if (!Number.isInteger(o.examples) || o.examples < 0 || o.examples > 20) throw new Error('--examples must be 0 … 20');
  return o;
}

/** The documents of a source: { name, input: Buffer, signature }. A generator, so memory stays small. */
export function* documents(o) {
  if (o.source === 'fixtures') {
    const dir = path.join(REPO_ROOT, 'fixtures', 'rqp', 'markup');
    for (const name of readdirSync(dir).sort()) {
      const f = path.join(dir, name, 'input.html');
      if (existsSync(f)) yield { name, input: readFileSync(f), signature: name };
    }
  } else if (o.source === 'cases') {
    const dir = path.join(MARKUP_ROOT, 'crosscheck', 'cases');
    for (const file of readdirSync(dir).filter((n) => n.endsWith('.json')).sort()) {
      const list = JSON.parse(readFileSync(path.join(dir, file), 'utf8'));
      for (const c of list) yield { name: `${file}:${c.id}`, input: Buffer.from(c.input, 'utf8'), signature: c.id };
    }
  } else if (o.source === 'review2') {
    for (let i = 0; i < o.count; i++) {
      const input = generateDocument(docSeed(o.seed, i), { maxTokens: 20, profile: 'review2' });
      yield { name: `review2-${o.seed}-${i}`, input: Buffer.from(input, 'utf8'), signature: 'review2' };
    }
  } else {
    for (let i = 0; i < o.count; i++) {
      const d = generateNoscriptDocument(o.seed, i, o.source);
      yield { name: `${o.source}-${o.seed}-${i}`, input: Buffer.from(d.input, 'utf8'), signature: d.signature };
    }
  }
}

function newGroup() {
  return { documents: 0, valid: 0, validWithBlocks: 0, disagree: { enabled: 0, disabled: 0 }, signatures: {}, examples: [] };
}

function newHeadGroup() {
  return {
    documents: 0, found: 0, foundMarkupValid: 0,
    disagree: { enabled: 0, disabled: 0 }, disagreeMarkupValid: { enabled: 0, disabled: 0 },
    signatures: {}, examples: [],
  };
}

function note(group, doc, examples, detail) {
  group.signatures[doc.signature] = (group.signatures[doc.signature] ?? 0) + 1;
  if (group.examples.length < examples) group.examples.push({ name: doc.name, input: doc.input.toString('utf8'), detail });
}

function topSignatures(sig) {
  return Object.fromEntries(Object.entries(sig).sort((a, b) => b[1] - a[1] || (a[0] < b[0] ? -1 : 1)).slice(0, MAX_SIGNATURES));
}

/** Run the evaluation over the documents of a source and return the aggregate. */
export function sweep(o) {
  const markup = Object.fromEntries(MARKUP_VARIANTS.map((v) => [v.name, newGroup()]));
  const head = Object.fromEntries(HEAD_VARIANTS.map((r) => [r, newHeadGroup()]));
  const reasons = Object.fromEntries(MARKUP_VARIANTS.map((v) => [v.name, {}]));
  const headFailures = { h1: {}, h2: {} };
  let documentsSeen = 0;
  let candidateMismatch = 0;
  let divergences = 0;
  // Newly lost: valid with the shipped rule, invalid with the variant.
  const lost = Object.fromEntries(MARKUP_VARIANTS.map((v) => [v.name, 0]));
  const lostNames = Object.fromEntries(MARKUP_VARIANTS.map((v) => [v.name, []]));
  for (const doc of documents(o)) {
    documentsSeen++;
    const e = evaluateDocument(doc.input);
    if (!e.candidateEqual) candidateMismatch++;
    if (e.divergences > 0) divergences++;
    const shippedValid = e.markup.shipped.valid;
    for (const v of MARKUP_VARIANTS) {
      const g = markup[v.name];
      const r = e.markup[v.name];
      g.documents++;
      if (!r.valid) {
        const k = r.why ?? '?';
        reasons[v.name][k] = (reasons[v.name][k] ?? 0) + 1;
        if (shippedValid) {
          lost[v.name]++;
          if (lostNames[v.name].length < MAX_LOST_NAMES) lostNames[v.name].push(`${doc.name} ${k}`);
        }
        continue;
      }
      g.valid++;
      if (r.blocks > 0) g.validWithBlocks++;
      let any = false;
      for (const view of ['enabled', 'disabled']) {
        if (r[view] !== 'agree') {
          g.disagree[view]++;
          any = true;
        }
      }
      if (any) note(g, doc, o.examples, { enabled: r.enabledDetails ?? null, disabled: r.disabledDetails ?? null });
    }
    for (const rule of HEAD_VARIANTS) {
      const g = head[rule];
      const r = e.head[rule];
      g.documents++;
      if (r.failure !== null && headFailures[rule] !== undefined) {
        headFailures[rule][r.failure] = (headFailures[rule][r.failure] ?? 0) + 1;
      }
      if (r.at < 0) continue;
      g.found++;
      if (shippedValid) g.foundMarkupValid++;
      let any = false;
      for (const view of ['enabled', 'disabled']) {
        if (r[view] === 'disagree') {
          g.disagree[view]++;
          if (shippedValid) g.disagreeMarkupValid[view]++;
          any = true;
        }
      }
      if (any) note(g, doc, o.examples, { enabled: r.enabledDetails ?? null, disabled: r.disabledDetails ?? null });
    }
  }
  for (const g of [...Object.values(markup), ...Object.values(head)]) g.signatures = topSignatures(g.signatures);
  return {
    source: o.source,
    seed: ['body', 'head', 'review2'].includes(o.source) ? o.seed : null,
    count: documentsSeen,
    candidateMismatch,
    modelADivergences: divergences,
    markup,
    lostAgainstShipped: lost,
    lostNames,
    rejectReasons: Object.fromEntries(Object.entries(reasons).map(([k, v]) => [k, topSignatures(v)])),
    head,
    headFailures: Object.fromEntries(Object.entries(headFailures).map(([k, v]) => [k, topSignatures(v)])),
  };
}

function main(argv) {
  const o = parseArgs(argv);
  const t0 = Date.now();
  const result = sweep(o);
  result.seconds = Math.round((Date.now() - t0) / 100) / 10;
  const text = `${JSON.stringify(result, null, 1)}\n`;
  if (o.out !== null) writeFileSync(o.out, text);
  else process.stdout.write(text);
  const line = MARKUP_VARIANTS.map((v) => {
    const g = result.markup[v.name];
    return `${v.name} valid ${g.valid} (blocks ${g.validWithBlocks}) dis-en ${g.disagree.enabled} dis-dis ${g.disagree.disabled}`;
  }).join('; ');
  const hl = HEAD_VARIANTS.map((r) => {
    const g = result.head[r];
    return `${r} found ${g.found} dis-en ${g.disagree.enabled} dis-dis ${g.disagree.disabled}`;
  }).join('; ');
  process.stderr.write(`${o.source}: ${result.count} documents in ${result.seconds} s; candidate mismatch ${result.candidateMismatch}\n  markup: ${line}\n  head: ${hl}\n`);
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    main(process.argv.slice(2));
  } catch (err) {
    process.stderr.write(`sweep: ${err instanceof Error ? err.message : String(err)}\n`);
    process.exit(2);
  }
}
