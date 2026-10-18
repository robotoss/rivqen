#!/usr/bin/env node
// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// Agreement run of the H-85 study (WP-17 S4 T-03): the study rule with each close
// mode on a source of documents, the parse5 oracle (src/table-oracle.mjs) on each
// valid result that the shipped rule does not have, and the counts.
//
// Usage: node src/table-sweep.mjs --source fixtures|cases|tables|classic|review|review2|review3
//          [--seed 85] [--count 100000] [--examples 3] [--control 50] --out <file.json>
//
// fixtures and cases: every valid result of every mode is checked (also the shipped
// ones). Generated sources: every gained result (valid in the mode, invalid with
// close 'off'), and every --control-th document that close 'off' accepts. Output:
// numbers, rule labels, finding kinds and up to --examples synthetic documents for
// each group. Bounded: --count is at most 500 000 documents per run.

import { writeFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { generateDocument } from '../../../crosscheck/src/generator.mjs';
import { docSeed } from '../../../crosscheck/src/generator.mjs';
import { analyze } from '../../../node/src/markup.mjs';
import { CLOSE_MODES, analyzeS4, contractFields } from './markup-s4.mjs';
import { checkResult } from './table-oracle.mjs';
import { generateTableDocument } from './table-gen.mjs';
import { crosscheckCases, markupFixtures } from './inputs.mjs';

export const SOURCES = Object.freeze(['fixtures', 'cases', 'tables', 'classic', 'review', 'review2', 'review3']);
const MAX_COUNT = 500_000;

export function parseArgs(argv) {
  const o = { source: null, seed: 85, count: 100_000, examples: 3, control: 50, out: null };
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i];
    const v = argv[++i];
    if (v === undefined) throw new Error(`option ${a} needs a value`);
    if (a === '--source') o.source = v;
    else if (a === '--seed') o.seed = Number(v);
    else if (a === '--count') o.count = Number(v);
    else if (a === '--examples') o.examples = Number(v);
    else if (a === '--control') o.control = Number(v);
    else if (a === '--out') o.out = path.resolve(v);
    else throw new Error(`unknown option ${a}`);
  }
  if (!SOURCES.includes(o.source)) throw new Error(`--source must be one of ${SOURCES.join(', ')}`);
  if (o.out === null) throw new Error('--out is required');
  for (const k of ['seed', 'count', 'examples', 'control']) if (!Number.isInteger(o[k]) || o[k] < 0) throw new Error(`--${k} must be an integer >= 0`);
  if (o.count < 1 || o.count > MAX_COUNT) throw new Error(`--count must be 1..${MAX_COUNT}`);
  if (o.control < 1) throw new Error('--control must be >= 1');
  return o;
}

/** The documents of a source as a generator of [name, bytes]. */
export function* documents(o) {
  if (o.source === 'fixtures') yield* markupFixtures();
  else if (o.source === 'cases') yield* crosscheckCases();
  else if (o.source === 'tables') {
    for (let i = 0; i < o.count; i++) yield [`tables ${i}`, Buffer.from(generateTableDocument(o.seed, i), 'utf8')];
  } else {
    for (let i = 0; i < o.count; i++) {
      yield [`${o.source} ${i}`, Buffer.from(generateDocument(docSeed(o.seed, i), { maxTokens: 24, profile: o.source }), 'utf8')];
    }
  }
}

const key = (r) => (r.valid ? r.blocks.map((b) => `${b.id}/${b.start}/${b.end}`).join(',') : 'invalid');
const sameContract = (a, b) => JSON.stringify(contractFields(a)) === JSON.stringify(b);

/**
 * Run one source. Returns the summary object (see the header).
 * @param {ReturnType<typeof parseArgs>} o
 */
export function sweep(o) {
  const all = o.source === 'fixtures' || o.source === 'cases';
  const modes = {};
  for (const m of CLOSE_MODES) {
    modes[m] = { valid: 0, gained: 0, checked: 0, withFindings: 0, findings: {}, why: {}, closures: { cell: 0, row: 0, group: 0, caption: 0, colgroup: 0 }, examples: {} };
  }
  let docs = 0;
  let offNotShipped = 0;
  let controls = 0;
  for (const [name, bytes] of documents(o)) {
    docs++;
    const shipped = analyze(bytes);
    const results = {};
    for (const m of CLOSE_MODES) results[m] = analyzeS4(bytes, { close: m, trace: true });
    if (!sameContract(results.off, shipped)) offNotShipped++;
    const control = !all && results.off.valid && docs % o.control === 0;
    if (control) controls++;
    const checked = new Map(); // result key -> finding kinds
    for (const m of CLOSE_MODES) {
      const r = results[m];
      const s = modes[m];
      if (!r.valid) {
        s.why[r.why ?? r.error] = (s.why[r.why ?? r.error] ?? 0) + 1;
        continue;
      }
      s.valid++;
      const gained = !results.off.valid;
      if (gained) s.gained++;
      for (const [k, n] of Object.entries(r.closures)) s.closures[k] += n;
      // b3 changes D also for explicit cell end tags: every result that it accepts is checked.
      if (!(all || gained || control || m === 'b3')) continue;
      // b3 has its own D rule (depth claim and rewrite differ), so its findings are its own.
      const k = m === 'b3' ? `b3 ${key(r)}` : key(r);
      if (!checked.has(k)) checked.set(k, [...new Set(checkResult(bytes, r).map((f) => f.kind))]);
      const kinds = checked.get(k);
      s.checked++;
      if (kinds.length > 0) s.withFindings++;
      for (const kind of kinds) {
        s.findings[kind] = (s.findings[kind] ?? 0) + 1;
        const ex = (s.examples[kind] ??= []);
        if (ex.length < o.examples) ex.push(all ? name : bytes.toString('utf8'));
      }
    }
  }
  return { source: o.source, seed: all ? null : o.seed, docs, controls, offNotShipped, modes };
}

function main(argv) {
  const o = parseArgs(argv);
  const t0 = Date.now();
  const out = sweep(o);
  out.seconds = Math.round((Date.now() - t0) / 1000);
  writeFileSync(o.out, `${JSON.stringify(out, null, 2)}\n`);
  const line = CLOSE_MODES.map((m) => `${m}: valid ${out.modes[m].valid}, gained ${out.modes[m].gained}, findings ${out.modes[m].withFindings}/${out.modes[m].checked}`).join('; ');
  process.stderr.write(`${o.source}: ${out.docs} documents, ${out.seconds} s; ${line}\n`);
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    main(process.argv.slice(2));
  } catch (err) {
    process.stderr.write(`table-sweep: ${err instanceof Error ? err.message : String(err)}\n`);
    process.exit(2);
  }
}
