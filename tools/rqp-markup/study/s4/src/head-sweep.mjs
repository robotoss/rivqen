#!/usr/bin/env node
// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// Agreement run of the H-79 study (WP-17 S4 T-03).
//
// Usage: node src/head-sweep.mjs --source fixtures|protocol|cases|gen|hostile
//          [--seed 79] [--count 100000] [--examples 3] --out <file.json>
//
// Server sources (fixtures, cases, gen; protocol for documents without a manifest):
// for each head rule (src/head.mjs) and each document O with an insertion point, the
// full document F gets the manifest there; parse5 8.0.1 checks both scripting views
// (src/head-views.mjs: first child of the head, only manifest, the tree of F without
// the manifest equals the tree of O), and the client rules c1 and c2 must find the
// manifest. Protocol documents that already hold one manifest element are full
// documents: only the client rules and the parse5 checks run on them.
//
// hostile: for each generated O, a manifest at every token boundary up to the end of
// the token that ends item 1. A client rule that accepts such an F where the server
// would not put the manifest is measured with the parse5 checks.
//
// Output: numbers, labels and up to --examples synthetic documents (or fixture names)
// for each group. Bounded: --count is at most 500 000 documents per run.

import { writeFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { TokenType, Tokenizer } from '../../../node/src/tokenizer.mjs';
import { findManifestElements } from '../../../../rqp-protocol/src/markup.mjs';
import { analyze } from '../../../node/src/markup.mjs';
import { HEAD_RULES, clientAccepts, fullDocument, headPosition } from './head.mjs';
import { checkBothViews } from './head-views.mjs';
import { generateHeadDocument } from './head-gen.mjs';
import { crosscheckCases, markupFixtures, protocolDocuments } from './inputs.mjs';

export const SOURCES = Object.freeze(['fixtures', 'protocol', 'cases', 'gen', 'hostile']);
const MAX_COUNT = 500_000;

export function parseArgs(argv) {
  const o = { source: null, seed: 79, count: 100_000, examples: 3, out: null };
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
  if (o.out === null) throw new Error('--out is required');
  for (const k of ['seed', 'count', 'examples']) if (!Number.isInteger(o[k]) || o[k] < 0) throw new Error(`--${k} must be an integer >= 0`);
  if (o.count < 1 || o.count > MAX_COUNT) throw new Error(`--count must be 1..${MAX_COUNT}`);
  return o;
}

function* documents(o) {
  if (o.source === 'fixtures') yield* markupFixtures();
  else if (o.source === 'cases') yield* crosscheckCases();
  else if (o.source === 'protocol') yield* protocolDocuments();
  else for (let i = 0; i < o.count; i++) yield [`${o.source} ${i}`, Buffer.from(generateHeadDocument(o.seed, i), 'utf8')];
}

const bump = (obj, k, n = 1) => {
  obj[k] = (obj[k] ?? 0) + n;
};

class Groups {
  constructor(examples) {
    this.max = examples;
    this.counts = {};
    this.examples = {};
  }

  add(kind, example) {
    bump(this.counts, kind);
    const list = (this.examples[kind] ??= []);
    if (list.length < this.max) list.push(example);
  }

  toJSON() {
    return { counts: this.counts, examples: this.examples };
  }
}

/** Server side for one O and one rule: returns the failure labels (empty when all checks pass). */
export function checkServer(original, rule) {
  const p = headPosition(original, rule);
  if (p.at < 0) return { position: p, failures: null };
  const { full, length } = fullDocument(original, p.at);
  const views = checkBothViews(original, full);
  const failures = [...views.failures];
  if (!views.ok) failures.push('no-view');
  const client = clientAccepts(full, p.at, p.at + length, rule);
  if (!client.c1) failures.push('client-c1');
  if (!client.c2) failures.push('client-c2');
  const found = findManifestElements(full);
  if (found === null || found.elements.length !== 1 || found.elements[0].start !== p.at) failures.push('client-elements');
  return { position: p, failures };
}

/** The token boundaries of O up to the end of the token that ends item 1 of the head rule. */
export function boundaries(original) {
  const tok = new Tokenizer(Buffer.from(original.buffer, original.byteOffset, original.byteLength));
  const out = new Set([0]);
  for (let n = 0; n < 64; n++) {
    const t = tok.next();
    out.add(t.start);
    out.add(t.end);
    const before = t.type === TokenType.COMMENT || t.type === TokenType.DOCTYPE
      || (t.type === TokenType.START_TAG && t.name === 'html')
      || (t.type === TokenType.END_TAG && !['head', 'body', 'html', 'br'].includes(t.name));
    if (!before || t.type === TokenType.EOF) break;
  }
  return [...out].sort((a, b) => a - b);
}

export function sweep(o) {
  const named = o.source === 'fixtures' || o.source === 'cases' || o.source === 'protocol';
  const rules = {};
  for (const r of HEAD_RULES) rules[r] = { explicit: 0, implied: 0, none: 0, valid: { explicit: 0, implied: 0, none: 0 }, stops: {}, failures: new Groups(o.examples), failuresAnyPage: new Groups(o.examples) };
  const hostile = { positions: 0, c1: 0, c2: 0, c1Only: 0, c2Only: 0, c1OnlyFailures: new Groups(o.examples), c2Failures: new Groups(o.examples) };
  const full = { documents: 0, failures: new Groups(o.examples) };
  let docs = 0;
  let validDocs = 0;
  for (const [name, bytes] of documents(o)) {
    docs++;
    const example = named ? name : bytes.toString('utf8');
    if (o.source === 'protocol') {
      const found = findManifestElements(bytes);
      if (found !== null && found.elements.length === 1 && found.elements[0].end > 0) {
        // A full document: client rules and parse5 checks on the manifest that is there.
        full.documents++;
        const el = found.elements[0];
        const original = Buffer.concat([bytes.subarray(0, el.start), bytes.subarray(el.end)]);
        const c = clientAccepts(bytes, el.start, el.end, 'h79');
        const v = checkBothViews(original, bytes);
        const shipped = clientAccepts(bytes, el.start, el.end, 'shipped');
        if (shipped.c1 !== c.c1 || shipped.c1 !== c.c2) full.failures.add('client-differs-from-shipped', example);
        if (c.c1 && !v.pass) for (const f of v.failures) full.failures.add(f, example);
        continue;
      }
    }
    if (o.source === 'hostile') {
      const server = headPosition(bytes, 'h79').at;
      for (const q of boundaries(bytes)) {
        const { full: f, length } = fullDocument(bytes, q);
        const c = clientAccepts(f, q, q + length, 'h79');
        hostile.positions++;
        if (c.c1) hostile.c1++;
        if (c.c2) hostile.c2++;
        if (c.c2 && q !== server) hostile.c2Failures.add('c2-not-server-position', example);
        if (!c.c1 && !c.c2) continue;
        const v = checkBothViews(bytes, f);
        if (c.c1 && !c.c2) {
          hostile.c1Only++;
          if (v.pass) bump(hostile, 'c1OnlyPass');
          for (const k of v.failures) hostile.c1OnlyFailures.add(k, `${example} @${q}`);
        }
        if (c.c2) {
          if (!c.c1) hostile.c2Only++;
          for (const k of v.failures) hostile.c2Failures.add(k, `${example} @${q}`);
        }
      }
      continue;
    }
    // The server sends a manifest only for a page that the markup analysis accepts
    // (negotiation section 4); the failures count only on such pages.
    const valid = analyze(bytes).valid;
    if (valid) validDocs++;
    for (const r of HEAD_RULES) {
      const s = rules[r];
      const { position, failures } = checkServer(bytes, r);
      s[position.kind]++;
      if (valid) s.valid[position.kind]++;
      if (position.kind === 'implied') bump(s.stops, position.stop);
      if (failures !== null) for (const f of failures) (valid ? s.failures : s.failuresAnyPage).add(`${position.kind}-${f}`, example);
    }
  }
  const json = (x) => JSON.parse(JSON.stringify(x));
  return { source: o.source, seed: named ? null : o.seed, docs, validDocs, rules: json(rules), hostile: o.source === 'hostile' ? json(hostile) : null, full: o.source === 'protocol' ? json(full) : null };
}

function main(argv) {
  const o = parseArgs(argv);
  const t0 = Date.now();
  const out = sweep(o);
  out.seconds = Math.round((Date.now() - t0) / 1000);
  writeFileSync(o.out, `${JSON.stringify(out, null, 2)}\n`);
  const line = HEAD_RULES.map((r) => `${r}: explicit ${out.rules[r].explicit}, implied ${out.rules[r].implied}, failures ${JSON.stringify(out.rules[r].failures.counts)}`).join('; ');
  process.stderr.write(`${o.source}: ${out.docs} documents, ${out.seconds} s; ${line}\n`);
  if (out.hostile) process.stderr.write(`hostile: ${JSON.stringify({ ...out.hostile, c1OnlyFailures: out.hostile.c1OnlyFailures.counts, c2Failures: out.hostile.c2Failures.counts })}\n`);
  if (out.full) process.stderr.write(`full documents: ${out.full.documents}, failures ${JSON.stringify(out.full.failures.counts)}\n`);
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    main(process.argv.slice(2));
  } catch (err) {
    process.stderr.write(`head-sweep: ${err instanceof Error ? err.message : String(err)}\n`);
    process.exit(2);
  }
}
