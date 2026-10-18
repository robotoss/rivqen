#!/usr/bin/env node
// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// One corpus page of the H-40 study (run by src/corpus.mjs in a child process with a
// time limit). Measurement tool only.
//
// Usage: node src/corpus-page.mjs <input.html> [--charset label] [--variants-dir dir --id id]
//
// For the page and for its simulated blocks (variants "small" and "main" of the S1
// method, tools/rqp-markup/corpus/classify.mjs choosePicks and insertBlocks), it runs
// the study rule with every combination of relaxations, the parse5 oracle on each valid
// result, and the attribute walk on the block content. With --variants-dir it writes
// the variant documents to <dir>/<variant>/<id>/input.html for the four candidates.
//
// Prints one JSON object. The output never has document text: only numbers, rule ids,
// and tag and attribute names.

import { mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { analyze } from '../../../node/src/markup.mjs';
import { choosePicks, classify, insertBlocks } from '../../../corpus/classify.mjs';
import { blockAttributeStats } from './attrs.mjs';
import { contractFields, resultKey } from './combos.mjs';
import { COMBOS, analyzeRelaxed, flagsOf } from './markup-relax.mjs';
import { Oracle } from './oracle.mjs';

const FLAGS = new Map(COMBOS.map((c) => [c, flagsOf(c)]));
const NAME = /^[a-z][a-z0-9:_-]{0,40}$/;
const sameJson = (a, b) => JSON.stringify(a) === JSON.stringify(b);

function mapToObject(m) {
  const o = {};
  for (const [k, v] of m) o[NAME.test(k) ? k : 'other'] = (o[NAME.test(k) ? k : 'other'] ?? 0) + v;
  return o;
}

/**
 * Every combination on one document: valid, rule of the first violation, oracle findings,
 * and for the combinations with r1 the attribute statistics of the blocks.
 */
export function measure(bytes) {
  const oracle = new Oracle(bytes);
  const checked = new Map();
  const combos = {};
  let attrs = null;
  for (const c of COMBOS) {
    const r = analyzeRelaxed(bytes, FLAGS.get(c), { trace: true });
    const row = { valid: r.valid, why: r.valid ? null : r.why, blocks: r.blocks.length };
    if (r.valid) {
      const key = resultKey(r);
      if (!checked.has(key)) checked.set(key, oracle.check(r).map((f) => f.kind));
      const kinds = checked.get(key);
      if (kinds.length > 0) row.findings = [...new Set(kinds)];
      row.foreignTags = r.trace.content.filter(([at, inRegion, , name]) => (inRegion || name === 'svg' || name === 'math') && r.blocks.some((b) => at >= b.start && at < b.end)).length;
      row.noscriptSvg = r.trace.noscripts.some((n) => n.tags.some(([, , name]) => name === 'svg'));
      if (c === 'r1+r2+r3' && r.blocks.length > 0) {
        const s = blockAttributeStats(bytes, r);
        attrs = { ...s, foreignElements: mapToObject(s.foreignElements), foreignAttributes: mapToObject(s.foreignAttributes) };
      }
    }
    combos[c] = row;
  }
  return { combos, attrs };
}

export function measurePage(bytes, { charset = null, variantsDir = null, id = null } = {}) {
  const shipped = analyze(bytes);
  const base = analyzeRelaxed(bytes, FLAGS.get('base'));
  const out = {
    size: bytes.length,
    shipped: { valid: shipped.valid, error: shipped.error },
    baseEqualsShipped: sameJson(contractFields(base), shipped),
    page: measure(bytes).combos,
  };
  const cl = classify(bytes, { charset });
  out.utf8 = cl.utf8;
  out.classes = cl.classes;
  out.details = {};
  for (const k of ['M-11.3', 'M-11.5-other', 'M-06.4-foreign', 'M-06.1']) if (cl.details[k]) out.details[k] = cl.details[k];
  out.variants = {};
  const anyValid = Object.values(out.page).some((x) => x.valid);
  if (!anyValid) return out;
  const picks = choosePicks(cl.picks);
  for (const name of ['small', 'main']) {
    const list = picks[name];
    if (list.length === 0) {
      out.variants[name] = { picks: 0 };
      continue;
    }
    const doc = insertBlocks(bytes, list, name === 'small' ? 's' : 'm');
    if (doc === null) {
      out.variants[name] = { picks: list.length, error: 'insert refused' };
      continue;
    }
    if (variantsDir !== null) {
      const dir = path.join(variantsDir, name, id);
      mkdirSync(dir, { recursive: true });
      writeFileSync(path.join(dir, 'input.html'), doc);
    }
    const m = measure(doc);
    const shippedV = analyze(doc);
    out.variants[name] = {
      picks: list.length,
      shipped: { valid: shippedV.valid, error: shippedV.error },
      baseEqualsShipped: sameJson(contractFields(analyzeRelaxed(doc, FLAGS.get('base'))), shippedV),
      combos: m.combos,
      attrs: m.attrs,
    };
  }
  return out;
}

function main(argv) {
  let file = null;
  let charset = null;
  let variantsDir = null;
  let id = null;
  for (let i = 0; i < argv.length; i++) {
    if (argv[i] === '--charset') charset = argv[++i] ?? null;
    else if (argv[i] === '--variants-dir') variantsDir = argv[++i] ?? null;
    else if (argv[i] === '--id') id = argv[++i] ?? null;
    else file = argv[i];
  }
  if (file === null || (variantsDir !== null && (id === null || !/^[A-Za-z0-9_-]{1,32}$/.test(id)))) {
    process.stderr.write('usage: corpus-page.mjs <file> [--charset label] [--variants-dir dir --id id]\n');
    process.exit(2);
  }
  const bytes = readFileSync(file);
  process.stdout.write(`${JSON.stringify(measurePage(bytes, { charset, variantsDir, id }))}\n`);
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    main(process.argv.slice(2));
  } catch (err) {
    process.stderr.write(`corpus-page: ${err instanceof Error ? err.message : String(err)}\n`);
    process.exit(2);
  }
}
