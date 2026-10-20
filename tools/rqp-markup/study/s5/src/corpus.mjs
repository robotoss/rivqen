#!/usr/bin/env node
// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// Corpus run of the S5 study (template and noscript in block content). Same corpus
// method as S1 T-15 and S4 T-03 (tools/rqp-markup/corpus): the pages of
// <data>/pages.jsonl with corpus "yes".
//
// Usage: node src/corpus.mjs --data <corpus-data> --out <dir> [--jobs 3] [--timeout-s 600]
//        node src/corpus.mjs --aggregate <corpus-pages.jsonl> --out <dir>
//
// 1. src/corpus-page.mjs on each page (child process, time limit).
// 2. <out>/corpus-pages.jsonl: one line per page (URL redacted as in
//    ../../../corpus/run.mjs, numbers, option names, rule labels, tag names). This file
//    stays outside the repository (the scratchpad): the evidence keeps aggregates only.
// 3. <out>/corpus.json: the totals (aggregate numbers only; no URL).
//
// The data directory is read only. No page bytes go to the outputs.

import { mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { runProcess } from '../../../diff.mjs';
import { redactUrl } from '../../../corpus/run.mjs';
import { COMBO_NAMES } from './options.mjs';

const HERE = path.dirname(fileURLToPath(import.meta.url));
const WORKER = path.join(HERE, 'corpus-page.mjs');
const log = (s) => process.stderr.write(`${s}\n`);

export function parseArgs(argv) {
  const o = { data: null, out: null, aggregate: null, jobs: 3, timeoutS: 600 };
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i];
    const v = argv[++i];
    if (v === undefined) throw new Error(`option ${a} needs a value`);
    if (a === '--data') o.data = path.resolve(v);
    else if (a === '--out') o.out = path.resolve(v);
    else if (a === '--aggregate') o.aggregate = path.resolve(v);
    else if (a === '--jobs') o.jobs = Number(v);
    else if (a === '--timeout-s') o.timeoutS = Number(v);
    else throw new Error(`unknown option ${a}`);
  }
  if (o.out === null || (o.data === null && o.aggregate === null)) throw new Error('--out and one of --data or --aggregate are required');
  if (!Number.isInteger(o.jobs) || o.jobs < 1 || o.jobs > 16) throw new Error('--jobs must be 1..16');
  if (!Number.isInteger(o.timeoutS) || o.timeoutS < 1) throw new Error('--timeout-s must be a positive integer');
  return o;
}

async function pool(items, jobs, fn) {
  let next = 0;
  const worker = async () => {
    while (next < items.length) {
      const i = next++;
      await fn(items[i]);
    }
  };
  await Promise.all(Array.from({ length: Math.min(jobs, items.length) }, worker));
}

const bump = (o, k, n = 1) => {
  o[k] = (o[k] ?? 0) + n;
};

/** UTF-8 for RQP (H-62): no charset label, or a UTF-8 label. */
const utf8Label = (c) => c === null || c === undefined || /^utf-?8$/i.test(c);

function addCensus(t, c) {
  if (!c) return;
  t.variantsWithTemplate += c.templates > 0 ? 1 : 0;
  t.variantsWithNoscript += c.noscripts > 0 ? 1 : 0;
  t.templates += c.templates;
  t.noscripts += c.noscripts;
  t.shadowroot += c.shadowroot;
  t.tableFirst += c.tableFirst;
  for (const [k, n] of Object.entries(c.cKinds ?? {})) bump(t.cKinds, k, n);
}

/** Totals from the page rows (aggregates only: no URL, no page data). */
export function aggregate(rows) {
  const t = { pages: rows.length, errors: 0, combos: {}, variants: {} };
  for (const c of COMBO_NAMES) t.combos[c] = { valid: 0, validUtf8: 0 };
  for (const r of rows) {
    if (r.error) {
      t.errors++;
      continue;
    }
    for (const c of COMBO_NAMES) {
      if (r.combos[c].valid) t.combos[c].valid++;
      if (r.combos[c].valid && utf8Label(r.charset)) t.combos[c].validUtf8++;
    }
    for (const [name, v] of Object.entries(r.variants ?? {})) {
      const vt = (t.variants[name] ??= {
        documents: 0, noPicks: 0, errors: 0,
        census: { variantsWithTemplate: 0, variantsWithNoscript: 0, templates: 0, noscripts: 0, shadowroot: 0, tableFirst: 0, cKinds: {} },
        combos: {},
      });
      if (v.picks === 0) {
        vt.noPicks++;
        continue;
      }
      if (!v.combos) {
        vt.errors++;
        continue;
      }
      vt.documents++;
      addCensus(vt.census, v.census);
      for (const c of COMBO_NAMES) {
        if (!r.combos[c].valid) continue; // variants of pages that this combination accepts
        const s = (vt.combos[c] ??= { pages: 0, valid: 0, gained: 0, lost: 0, withTemplate: 0, withNoscript: 0, patchBlocked: 0, patchBlockedP2: 0, why: {}, findings: {}, informative: {}, patchFindings: {} });
        const x = v.combos[c];
        s.pages++;
        if (!x.valid) {
          if (v.combos.base.valid) s.lost++;
          bump(s.why, x.why);
          continue;
        }
        s.valid++;
        if (!v.combos.base.valid) s.gained++;
        if (x.templates > 0) s.withTemplate++;
        if (x.noscripts > 0) s.withNoscript++;
        if (x.patchBlocked > 0) s.patchBlocked++;
        if (x.patchBlockedP2 > 0) s.patchBlockedP2++;
        for (const f of x.findings ?? []) bump(s.findings, f);
        for (const f of x.informative ?? []) bump(s.informative, f);
        for (const f of x.patchFindings ?? []) bump(s.patchFindings, f);
      }
    }
  }
  return t;
}

async function main(argv) {
  const o = parseArgs(argv);
  mkdirSync(o.out, { recursive: true });
  let rows;
  if (o.aggregate !== null) {
    rows = readFileSync(o.aggregate, 'utf8').split('\n').filter((l) => l !== '').map((l) => JSON.parse(l));
  } else {
    const pages = readFileSync(path.join(o.data, 'pages.jsonl'), 'utf8')
      .split('\n')
      .filter((l) => l !== '')
      .map((l) => JSON.parse(l))
      .filter((p) => p.corpus === 'yes');
    log(`pages: ${pages.length}`);
    const byId = new Map();
    let done = 0;
    const t0 = Date.now();
    await pool(pages, o.jobs, async (p) => {
      const args = [process.execPath, WORKER, path.join(o.data, 'pages', p.id, 'input.html')];
      if (p.charset) args.push('--charset', p.charset);
      const r = await runProcess(args, HERE, o.timeoutS * 1000, 64 * 1024 * 1024);
      let row;
      if (r.kind !== 'exit' || r.code !== 0) row = { error: r.kind === 'exit' ? `exit ${r.code}` : r.kind };
      else {
        try {
          row = JSON.parse(r.stdout);
        } catch {
          row = { error: 'bad output' };
        }
      }
      byId.set(p.id, { id: p.id, url: redactUrl(p.final_url ?? p.url), charset: p.charset ?? null, ...row });
      done++;
      if (done % 50 === 0) log(`  ${done}/${pages.length} pages, ${((Date.now() - t0) / 1000).toFixed(0)} s`);
    });
    rows = pages.map((p) => byId.get(p.id));
    writeFileSync(path.join(o.out, 'corpus-pages.jsonl'), `${rows.map((r) => JSON.stringify(r)).join('\n')}\n`);
  }
  const totals = aggregate(rows);
  writeFileSync(path.join(o.out, 'corpus.json'), `${JSON.stringify(totals, null, 1)}\n`);
  log(`pages ${totals.pages}, errors ${totals.errors}, base valid ${totals.combos.base.valid}`);
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  main(process.argv.slice(2)).catch((err) => {
    process.stderr.write(`corpus: ${err instanceof Error ? err.message : String(err)}\n`);
    process.exit(2);
  });
}
