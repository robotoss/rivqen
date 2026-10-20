#!/usr/bin/env node
// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// One corpus page of the S4 T-03 studies (run by src/corpus.mjs in a child process
// with a time limit). Measurement tool only.
//
// Usage: node src/corpus-page.mjs <input.html>
//
// Study A (H-79): the insertion point under each head rule; for a page that the
// shipped candidate accepts, the parse5 checks of the full document in both views
// and the client rules. Study B (H-85): the study rule with each close mode on the
// page; the oracle on each gained result; and the simulated blocks of the S1 method
// (tools/rqp-markup/corpus/classify.mjs choosePicks and insertBlocks) on pages that
// a mode accepts.
//
// Prints one JSON object with numbers, rule labels and tag names only. Tag names that
// do not look like tag names are "?". The page bytes are read, never written.

import { readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { analyze } from '../../../node/src/markup.mjs';
import { choosePicks, classify, insertBlocks } from '../../../corpus/classify.mjs';
import { HEAD_RULES, clientAccepts, fullDocument, headPosition } from './head.mjs';
import { checkBothViews } from './head-views.mjs';
import { analyzeS4 } from './markup-s4.mjs';
import { checkResult } from './table-oracle.mjs';

const MODES = ['off', 'b1', 'b2', 'b3', 'u'];
const TAG = /^<\/?[a-z][a-z0-9-]{0,31}>$/;
const label = (s) => (s === null || s === undefined || TAG.test(s) || /^[a-z]{2,8}$/.test(s) ? s : '?');

/** Study A on one page. */
export function headStudy(bytes, valid) {
  const out = {};
  const views = new Map(); // insertion point -> parse5 result (the rules often give the same point)
  for (const r of HEAD_RULES) {
    const p = headPosition(bytes, r);
    const row = { kind: p.kind, stop: label(p.stop) };
    if (valid && p.at >= 0) {
      const { full, length } = fullDocument(bytes, p.at);
      if (!views.has(p.at)) views.set(p.at, checkBothViews(bytes, full));
      const v = views.get(p.at);
      const c = clientAccepts(full, p.at, p.at + length, r);
      row.pass = v.pass && c.c1 && c.c2;
      if (!row.pass) row.failures = [...v.failures, ...(c.c1 ? [] : ['client-c1']), ...(c.c2 ? [] : ['client-c2'])];
    }
    out[r] = row;
  }
  return out;
}

function closeRow(bytes, mode, off) {
  const r = analyzeS4(bytes, { close: mode, trace: true });
  const row = { valid: r.valid, why: r.valid ? null : (r.why ?? r.error), blocks: r.blocks.length };
  const n = r.closures === null ? 0 : Object.values(r.closures).reduce((a, b) => a + b, 0);
  if (n > 0) row.closures = r.closures;
  if (r.valid && (off === null || !off.valid)) {
    const kinds = [...new Set(checkResult(bytes, r).map((f) => f.kind))];
    if (kinds.length > 0) row.findings = kinds;
  }
  return { row, result: r };
}

/** Study B on one document (a page or a variant). */
export function closeStudy(bytes) {
  const out = {};
  const off = closeRow(bytes, 'off', null);
  out.off = off.row;
  for (const m of MODES.slice(1)) out[m] = closeRow(bytes, m, off.result).row;
  return out;
}

export function measurePage(bytes, { charset = null } = {}) {
  const shipped = analyze(bytes);
  const out = { size: bytes.length, shipped: { valid: shipped.valid, error: shipped.error } };
  out.head = headStudy(bytes, shipped.valid);
  out.close = closeStudy(bytes);
  out.variants = {};
  if (!MODES.some((m) => out.close[m].valid)) return out;
  const cl = classify(bytes, { charset });
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
    out.variants[name] = { picks: list.length, close: closeStudy(doc) };
  }
  return out;
}

function main(argv) {
  let file = null;
  let charset = null;
  for (let i = 0; i < argv.length; i++) {
    if (argv[i] === '--charset') charset = argv[++i] ?? null;
    else file = argv[i];
  }
  if (file === null) {
    process.stderr.write('usage: corpus-page.mjs <file> [--charset label]\n');
    process.exit(2);
  }
  process.stdout.write(`${JSON.stringify(measurePage(readFileSync(file), { charset }))}\n`);
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    main(process.argv.slice(2));
  } catch (err) {
    process.stderr.write(`corpus-page: ${err instanceof Error ? err.message : String(err)}\n`);
    process.exit(2);
  }
}
