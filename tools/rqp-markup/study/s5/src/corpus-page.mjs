#!/usr/bin/env node
// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// One corpus page of the S5 study (run by src/corpus.mjs in a child process with a
// time limit). Measurement tool only.
//
// Usage: node src/corpus-page.mjs <input.html> [--charset label]
//
// For the page and for its simulated block variants "small" and "main" (the S1 method,
// ../../../corpus/classify.mjs choosePicks and insertBlocks): the result of each option
// combination; for a valid variant the template and noscript start tags in its blocks,
// the blocks that a patch cannot carry (patch rule p1), and the parse5 oracle and the
// patch apply oracle when the option gains the variant. A census run (template 'tu',
// noscript 'nu': no S5 rule) counts the template and noscript start tags that the
// blocks reach before the first violation, and their shapes.
//
// Prints one JSON object with numbers, option names, rule labels and tag names only.
// The page bytes are read, never written, never executed (parse5 runs no script).

import { readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { analyze } from '../../../node/src/markup.mjs';
import { choosePicks, classify, insertBlocks } from '../../../corpus/classify.mjs';
import { analyzeS5, hasShadowRootMode } from './markup-s5.mjs';
import { COMBOS, COMBO_NAMES } from './options.mjs';
import { Oracle, isInformative } from './oracle-s5.mjs';
import { patchFindings } from './patch-s5.mjs';

const TABLE_STARTS = new Set(['caption', 'colgroup', 'col', 'tbody', 'thead', 'tfoot', 'tr', 'td', 'th']);
const C_VOIDISH = new Set(['img', 'iframe', 'image', 'source', 'picture', 'link', 'meta', 'br', 'input', 'style', 'script']);
const LABEL = /^[A-Za-z0-9:._-]{1,40}$/;
const label = (s) => (s === null || s === undefined ? s : LABEL.test(s) ? s : '?');

/** Shapes of the template and noscript start tags in the blocks of a traced result (census). */
export function census(bytes, r) {
  const out = { templates: 0, noscripts: 0, shadowroot: 0, tableFirst: 0, cKinds: {} };
  if (r.trace === null) return out;
  const content = r.trace.content;
  for (const [i, [at, , end, name]] of content.entries()) {
    if (name !== 'template') continue;
    out.templates++;
    if (hasShadowRootMode(bytes, at, end)) out.shadowroot++;
    // The first start tag of the template contents decides the "in template" mode.
    const next = content[i + 1];
    if (next !== undefined && next[4] === at && TABLE_STARTS.has(next[3])) out.tableFirst++;
  }
  const nos = r.trace.noscripts.filter((ns) => ns.inBlock);
  out.noscripts = r.trace.content.filter(([, , , name]) => name === 'noscript').length;
  for (const ns of nos) {
    const names = ns.tags.filter(([at]) => at < ns.end).map(([, , name]) => name);
    const kind = names.length === 0 ? 'text' : names.every((n) => C_VOIDISH.has(n)) ? 'void' : 'markup';
    out.cKinds[kind] = (out.cKinds[kind] ?? 0) + 1;
  }
  return out;
}

/** One document (a page or a variant): every option combination. */
export function measureDocument(bytes, { oracle = true } = {}) {
  const combos = {};
  const results = new Map();
  for (const c of COMBO_NAMES) {
    const r = analyzeS5(bytes, { ...COMBOS[c], trace: true });
    results.set(c, r);
    combos[c] = { valid: r.valid, why: r.valid ? null : label(r.why ?? r.error), blocks: r.blocks.length };
  }
  const base = results.get('base');
  let o = null;
  const patchCache = new Map();
  for (const c of COMBO_NAMES) {
    const r = results.get(c);
    if (!r.valid) continue;
    const row = combos[c];
    const tags = r.trace.content.map(([, , , name]) => name);
    row.templates = tags.filter((n) => n === 'template').length;
    row.noscripts = tags.filter((n) => n === 'noscript').length;
    const key = r.blocks.map((b) => `${b.start}/${b.end}`).join(',');
    if (!patchCache.has(key)) patchCache.set(key, patchFindings(bytes, r));
    const p = patchCache.get(key);
    row.patchBlocked = p.blocks.filter((b) => b.rules.p1 !== null).length;
    row.patchBlockedP2 = p.blocks.filter((b) => b.rules.p2 !== null).length;
    const pf = [...new Set(p.findings.filter((f) => f.kind.startsWith('patch-unseen')).map((f) => `${f.rule}:${f.kind}`))];
    if (pf.length > 0) row.patchFindings = pf;
    if (oracle && !base.valid) {
      o ??= new Oracle(bytes);
      const kinds = [...new Set(o.check(r).map((f) => f.kind))];
      const findings = kinds.filter((k) => !isInformative(k));
      const info = kinds.filter((k) => isInformative(k));
      if (findings.length > 0) row.findings = findings;
      if (info.length > 0) row.informative = info;
    }
  }
  const all = analyzeS5(bytes, { template: 'tu', noscript: 'nu', trace: true });
  return { combos, census: census(bytes, all) };
}

export function measurePage(bytes, { charset = null } = {}) {
  const shipped = analyze(bytes);
  const out = { size: bytes.length, shipped: { valid: shipped.valid, error: shipped.error } };
  const page = measureDocument(bytes, { oracle: false });
  out.combos = page.combos;
  out.variants = {};
  if (!COMBO_NAMES.some((c) => out.combos[c].valid)) return out;
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
    out.variants[name] = { picks: list.length, ...measureDocument(doc) };
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
