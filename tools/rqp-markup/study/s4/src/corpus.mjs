#!/usr/bin/env node
// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// Corpus run of the S4 T-03 studies (H-79, H-85). Same corpus method as S1 T-15
// (tools/rqp-markup/corpus): the pages of <data>/pages.jsonl with corpus "yes".
//
// Usage: node src/corpus.mjs --data <corpus-data> --evidence <dir> [--jobs 3] [--timeout-s 600]
//        node src/corpus.mjs --aggregate <corpus-pages.jsonl> --evidence <dir>
//
// 1. src/corpus-page.mjs on each page (child process, time limit).
// 2. <evidence>/corpus-pages.jsonl: one line per page (URL redacted as in
//    ../../../corpus/run.mjs, numbers, rule labels, tag names).
// 3. <evidence>/corpus.json: the totals.
//
// The data directory is read only. No page bytes go to the evidence.

import { readFileSync, writeFileSync, mkdirSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { runProcess } from '../../../diff.mjs';
import { redactUrl } from '../../../corpus/run.mjs';

const HERE = path.dirname(fileURLToPath(import.meta.url));
const WORKER = path.join(HERE, 'corpus-page.mjs');
const log = (s) => process.stderr.write(`${s}\n`);

export function parseArgs(argv) {
  const o = { data: null, evidence: null, aggregate: null, jobs: 3, timeoutS: 600 };
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i];
    const v = argv[++i];
    if (v === undefined) throw new Error(`option ${a} needs a value`);
    if (a === '--data') o.data = path.resolve(v);
    else if (a === '--evidence') o.evidence = path.resolve(v);
    else if (a === '--aggregate') o.aggregate = path.resolve(v);
    else if (a === '--jobs') o.jobs = Number(v);
    else if (a === '--timeout-s') o.timeoutS = Number(v);
    else throw new Error(`unknown option ${a}`);
  }
  if (o.evidence === null || (o.data === null && o.aggregate === null)) throw new Error('--evidence and one of --data or --aggregate are required');
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

/** Totals from the page rows. */
export function aggregate(rows) {
  const t = { pages: rows.length, errors: 0, valid: 0, validUtf8: 0, head: {}, close: {}, variants: {} };
  for (const r of rows) {
    if (r.error) {
      t.errors++;
      continue;
    }
    const valid = r.shipped.valid;
    const ok = valid && utf8Label(r.charset);
    if (valid) t.valid++;
    if (ok) t.validUtf8++;
    for (const [rule, h] of Object.entries(r.head)) {
      const s = (t.head[rule] ??= { explicit: 0, implied: 0, none: 0, rqpPages: 0, gained: [], lost: [], failures: {}, stops: {}, noneStops: {} });
      if (!ok) continue;
      s[h.kind]++;
      if (h.kind !== 'none') s.rqpPages++;
      if (h.kind === 'implied') bump(s.stops, h.stop);
      if (h.kind === 'none') bump(s.noneStops, h.stop);
      const before = r.head.shipped.kind !== 'none';
      if (h.kind !== 'none' && !before) s.gained.push(r.url);
      if (h.kind === 'none' && before) s.lost.push(r.url);
      for (const f of h.failures ?? []) bump(s.failures, f);
    }
    for (const [mode, c] of Object.entries(r.close)) {
      const s = (t.close[mode] ??= { valid: 0, validUtf8: 0, gained: [], lost: 0, why: {}, findings: {}, closures: {} });
      if (c.valid) s.valid++;
      if (c.valid && utf8Label(r.charset)) s.validUtf8++;
      if (c.valid && !r.close.off.valid && utf8Label(r.charset)) s.gained.push(r.url);
      if (!c.valid && r.close.off.valid) s.lost++;
      if (!c.valid) bump(s.why, c.why);
      for (const f of c.findings ?? []) bump(s.findings, f);
      for (const [k, n] of Object.entries(c.closures ?? {})) bump(s.closures, k, n);
    }
    for (const [name, v] of Object.entries(r.variants ?? {})) {
      if (!v.close) continue;
      for (const [mode, c] of Object.entries(v.close)) {
        const s = ((t.variants[name] ??= {})[mode] ??= { pages: 0, valid: 0, gained: 0, findings: {} });
        if (!r.close[mode].valid) continue; // variants of pages that this mode accepts
        s.pages++;
        if (c.valid) s.valid++;
        if (c.valid && !v.close.off.valid) s.gained++;
        for (const f of c.findings ?? []) bump(s.findings, f);
      }
    }
  }
  for (const s of Object.values(t.head)) {
    s.gained.sort();
    s.lost.sort();
  }
  for (const s of Object.values(t.close)) s.gained.sort();
  return t;
}

async function main(argv) {
  const o = parseArgs(argv);
  mkdirSync(o.evidence, { recursive: true });
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
    writeFileSync(path.join(o.evidence, 'corpus-pages.jsonl'), rows.map((r) => JSON.stringify(r)).join('\n') + '\n');
  }
  const totals = aggregate(rows);
  writeFileSync(path.join(o.evidence, 'corpus.json'), `${JSON.stringify(totals, null, 2)}\n`);
  log(`valid ${totals.valid}, valid UTF-8 ${totals.validUtf8}; head gained ${totals.head.h79?.gained.length}; close b2 gained ${totals.close.b2?.gained.length}`);
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  main(process.argv.slice(2)).catch((err) => {
    process.stderr.write(`corpus: ${err instanceof Error ? err.message : String(err)}\n`);
    process.exit(2);
  });
}
