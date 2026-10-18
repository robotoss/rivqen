#!/usr/bin/env node
// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// Agreement run of the H-40 study (WP-17 S3 T-01, report section 3.1).
//
// For each document and each combination of relaxations (COMBOS), the study rule
// gives a result. For each valid result the parse5 oracle (src/oracle.mjs) checks
// both scripting views, the depth claim and the tree checks of r1 and r2. The run
// also checks:
//   monotonic  a document that a smaller combination accepts gets the same result
//              from every larger combination (a relaxation only adds documents)
//   attr-walk  for a valid result with r1: the walk of the attribute rule
//              (manifest-patch section 4.2) reads exactly the start tags of the
//              block content that the token pass reads, also in a foreign region
//
// Sets: the golden fixtures, the cross-check cases, the generator profiles of the
// cross-check tool and the relaxation generator (src/gen.mjs).
//
// Usage: node src/agree.mjs [--relax n] [--profile-count n] [--seed n] [--keep n] [--out file]
// The output file has numbers, rule ids, fixture names and synthetic example
// documents only.

import { existsSync, readFileSync, readdirSync, writeFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { docSeed, generateDocument } from '../../../crosscheck/src/generator.mjs';
import { walkStartTags } from './attrs.mjs';
import { resultKey, subsetsOf } from './combos.mjs';
import { generateRelaxDocument } from './gen.mjs';
import { COMBOS, analyzeRelaxed, flagsOf } from './markup-relax.mjs';
import { Oracle } from './oracle.mjs';

const HERE = path.dirname(fileURLToPath(import.meta.url));
const ROOT = path.resolve(HERE, '..', '..', '..', '..', '..');
const FIXTURES = path.join(ROOT, 'fixtures', 'rqp', 'markup');
const CASES = path.join(ROOT, 'tools', 'rqp-markup', 'crosscheck', 'cases');
const FLAGS = new Map(COMBOS.map((c) => [c, flagsOf(c)]));

export function parseArgs(argv) {
  const o = { relax: 200_000, profileCount: 20_000, seed: 40, keep: 3, out: null };
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i];
    const v = argv[++i];
    if (v === undefined) throw new Error(`option ${a} needs a value`);
    const n = Number(v);
    if (a === '--out') o.out = path.resolve(v);
    else if (!Number.isSafeInteger(n) || n < 0) throw new Error(`option ${a} needs a non-negative integer`);
    else if (a === '--relax') o.relax = n;
    else if (a === '--profile-count') o.profileCount = n;
    else if (a === '--seed') o.seed = n;
    else if (a === '--keep') o.keep = n;
    else throw new Error(`unknown option ${a}`);
  }
  return o;
}

/** r1: the walk of section 4.2 reads the same start tags of each html block as the token pass. */
export function attrWalkFinding(bytes, result) {
  for (const [i, b] of result.blocks.entries()) {
    // A title block has RCDATA content and a json block script data: no start tags.
    if (b.format !== 'html' || result.trace.blockNames[i] === 'title') continue;
    const walk = walkStartTags(bytes.subarray(b.start, b.end)).map((t) => t.start + b.start);
    const pass = result.trace.content.filter(([at]) => at >= b.start && at < b.end).map(([at]) => at);
    if (walk.length !== pass.length || walk.some((x, i) => x !== pass[i])) {
      return `block ${b.id}: walk ${walk.length} start tags, token pass ${pass.length}`;
    }
  }
  return null;
}

function newStats() {
  // findings: on documents that base accepts too (the same result in every
  // combination, so not caused by a relaxation); gainedFindings: on documents
  // that only the relaxation accepts.
  // docs*: documents with at least one finding; "NotHead": a finding other than
  // noscript-*-head (the head part of H-42, study T-02).
  const row = () => ({ valid: 0, gained: 0, findings: {}, gainedFindings: {}, docsWithFinding: 0, gainedDocsWithFinding: 0, gainedDocsNotHead: 0, examples: [] });
  return Object.fromEntries(COMBOS.map((c) => [c, row()]));
}

/**
 * Evaluate one document for every combination.
 * @param {{ label: string, bytes: Buffer, show: string | null }} doc show: the text for an example (synthetic only)
 */
export function evaluate(doc, stats, totals, keep) {
  const results = new Map();
  for (const c of COMBOS) results.set(c, analyzeRelaxed(doc.bytes, FLAGS.get(c), { trace: true }));
  const oracle = new Oracle(doc.bytes);
  const checked = new Map(); // result key -> findings
  const base = results.get('base');
  for (const c of COMBOS) {
    const r = results.get(c);
    const s = stats[c];
    const findings = [];
    if (r.valid) {
      s.valid++;
      if (!base.valid) s.gained++;
      const key = resultKey(r);
      if (!checked.has(key)) checked.set(key, oracle.check(r));
      findings.push(...checked.get(key));
      if (FLAGS.get(c).r1) {
        const w = attrWalkFinding(doc.bytes, r);
        if (w !== null) findings.push({ kind: 'attr-walk', detail: w });
      }
    }
    for (const sub of subsetsOf(c)) {
      const rs = results.get(sub);
      if (rs.valid && resultKey(rs) !== resultKey(r)) findings.push({ kind: 'monotonic', detail: `${sub} valid, ${c} ${r.valid ? 'other blocks' : r.why}` });
    }
    if (findings.length > 0) {
      if (base.valid) s.docsWithFinding++;
      else {
        s.gainedDocsWithFinding++;
        if (findings.some((f) => !/^noscript-.*-head$/.test(f.kind))) s.gainedDocsNotHead++;
      }
    }
    for (const f of findings) {
      const bucket = base.valid ? s.findings : s.gainedFindings;
      bucket[f.kind] = (bucket[f.kind] ?? 0) + 1;
      const kind = base.valid ? f.kind : `gained:${f.kind}`;
      totals[kind] = (totals[kind] ?? 0) + 1;
      if (s.examples.filter((e) => e.kind === kind).length < keep) s.examples.push({ doc: doc.show ?? doc.label, kind, detail: f.detail });
    }
  }
}

function fixtureDocs() {
  return readdirSync(FIXTURES)
    .filter((n) => existsSync(path.join(FIXTURES, n, 'input.html')))
    .sort()
    .map((n) => ({ label: n, bytes: readFileSync(path.join(FIXTURES, n, 'input.html')), show: null }));
}

function caseDocs() {
  const out = [];
  for (const file of readdirSync(CASES).filter((f) => f.endsWith('.json')).sort()) {
    for (const c of JSON.parse(readFileSync(path.join(CASES, file), 'utf8'))) {
      out.push({ label: `${file}:${c.id}`, bytes: Buffer.from(c.input, 'utf8'), show: null });
    }
  }
  return out;
}

function* profileDocs(profile, seed, count) {
  for (let i = 0; i < count; i++) {
    const text = generateDocument(docSeed(seed, i), { maxTokens: 20, profile });
    yield { label: `${profile} seed=${seed} index=${i}`, bytes: Buffer.from(text, 'utf8'), show: text };
  }
}

function* relaxDocs(seed, count) {
  for (let i = 0; i < count; i++) {
    const text = generateRelaxDocument(docSeed(seed, i));
    yield { label: `relax seed=${seed} index=${i}`, bytes: Buffer.from(text, 'utf8'), show: text };
  }
}

export function run(o, log = (s) => process.stderr.write(`${s}\n`)) {
  const sets = [
    ['fixtures', fixtureDocs()],
    ['cases', caseDocs()],
    ...['classic', 'review', 'review2', 'review3'].map((p) => [`profile-${p}`, profileDocs(p, o.seed, o.profileCount)]),
    ['relax', relaxDocs(o.seed, o.relax)],
  ];
  const out = { date: new Date().toISOString().slice(0, 10), seed: o.seed, combos: COMBOS, sets: {}, totals: {} };
  for (const [name, docs] of sets) {
    const stats = newStats();
    const totals = {};
    let n = 0;
    const t0 = Date.now();
    for (const doc of docs) {
      evaluate(doc, stats, totals, o.keep);
      n++;
    }
    out.sets[name] = { documents: n, combos: stats, findings: totals };
    for (const [k, v] of Object.entries(totals)) out.totals[k] = (out.totals[k] ?? 0) + v;
    log(`${name}: ${n} documents in ${((Date.now() - t0) / 1000).toFixed(1)} s; findings ${JSON.stringify(totals)}`);
  }
  return out;
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    const o = parseArgs(process.argv.slice(2));
    const out = run(o);
    const text = `${JSON.stringify(out, null, 1)}\n`;
    if (o.out === null) process.stdout.write(text);
    else writeFileSync(o.out, text);
  } catch (err) {
    process.stderr.write(`agree: ${err instanceof Error ? err.message : String(err)}\n`);
    process.exit(2);
  }
}
