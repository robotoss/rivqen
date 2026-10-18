#!/usr/bin/env node
// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// Corpus run of the H-42 study (WP-17 S3 T-02). Reads the corpus data of the S1
// method (tools/rqp-markup/corpus: pages.jsonl and pages/<id>/input.html, outside
// the repository) and writes only numbers, URLs, rule labels and tag names.
//
// Usage: node src/corpus.mjs --data <data-dir> --evidence <dir> [--jobs 3] [--timeout-s 180]
//        node src/corpus.mjs --aggregate <corpus-pages.jsonl> --evidence <dir>
//
// Output: <evidence>/corpus-pages.jsonl (one line for each page) and
// <evidence>/corpus.json (the aggregate of src/corpus.mjs aggregate()).

import { readFileSync, writeFileSync, mkdirSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { runProcess } from '../../../diff.mjs';
import { redactUrl } from '../../../corpus/run.mjs';
import { HEAD_VARIANTS, MARKUP_VARIANTS } from './evaluate.mjs';

const HERE = path.dirname(fileURLToPath(import.meta.url));
const PAGE_SCRIPT = path.join(HERE, 'corpus-page.mjs');
const MARKUPS = MARKUP_VARIANTS.map((v) => v.name);

export function parseArgs(argv) {
  const o = { data: null, evidence: null, aggregate: null, jobs: 3, timeoutS: 180 };
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
  if (o.evidence === null || (o.data === null && o.aggregate === null)) throw new Error('--evidence and --data (or --aggregate) are required');
  if (!Number.isInteger(o.jobs) || o.jobs < 1 || o.jobs > 16) throw new Error('--jobs must be 1 … 16');
  if (!Number.isInteger(o.timeoutS) || o.timeoutS < 1) throw new Error('--timeout-s must be a positive integer');
  return o;
}

async function pool(items, jobs, fn) {
  const out = new Array(items.length);
  let next = 0;
  const worker = async () => {
    while (next < items.length) {
      const i = next++;
      out[i] = await fn(items[i], i);
    }
  };
  await Promise.all(Array.from({ length: Math.min(jobs, items.length) }, worker));
  return out;
}

const inc = (map, key, n = 1) => {
  map[key] = (map[key] ?? 0) + n;
};

function sorted(map) {
  return Object.fromEntries(Object.entries(map).sort((a, b) => b[1] - a[1] || (a[0] < b[0] ? -1 : 1)));
}

/**
 * The aggregate of the page rows. A row is { id, url, charset, result } or { id, url, error }.
 * @param {object[]} rows
 */
export function aggregate(rows) {
  const ok = rows.filter((r) => r.result !== undefined);
  const a = {
    pages: rows.length,
    errors: rows.filter((r) => r.result === undefined).map((r) => ({ id: r.id, error: r.error })),
    candidateMismatch: ok.filter((r) => !r.result.candidateEqual).map((r) => r.id),
    modelADivergences: ok.filter((r) => r.result.divergences > 0).map((r) => r.id),
    noscript: { pagesAny: 0, pagesHead: 0, pagesPreHead: 0, pagesOther: 0, elementsHead: 0, elementsPreHead: 0, elementsOther: 0 },
    headNoscript: { pagesText: 0, pagesComment: 0, pagesEmptyOnly: 0, pagesPassH1: 0, pagesPassH2: 0, tags: {}, h1Failures: {}, h2Failures: {} },
    otherNoscript: { pagesText: 0, tags: {} },
    markup: {},
    head: {},
    combined: {},
    variants: {},
  };
  for (const r of ok) {
    const inv = r.result.inventory;
    const any = inv.head + inv.preHead + inv.other > 0;
    if (any) a.noscript.pagesAny++;
    if (inv.head > 0) a.noscript.pagesHead++;
    if (inv.preHead > 0) a.noscript.pagesPreHead++;
    if (inv.other > 0) a.noscript.pagesOther++;
    a.noscript.elementsHead += inv.head;
    a.noscript.elementsPreHead += inv.preHead;
    a.noscript.elementsOther += inv.other;
    if (inv.head > 0) {
      const h = a.headNoscript;
      if (inv.headText > 0) h.pagesText++;
      if (inv.headComment > 0) h.pagesComment++;
      if (inv.headEmpty === inv.head) h.pagesEmptyOnly++;
      if (inv.h1Failures.length === 0) h.pagesPassH1++;
      if (inv.h2Failures.length === 0) h.pagesPassH2++;
      for (const t of inv.headTags) inc(h.tags, t);
      for (const f of inv.h1Failures) inc(h.h1Failures, f);
      for (const f of inv.h2Failures) inc(h.h2Failures, f);
    }
    if (inv.other + inv.preHead > 0) {
      if (inv.otherText > 0) a.otherNoscript.pagesText++;
      for (const t of inv.otherTags) inc(a.otherNoscript.tags, t);
    }
  }
  for (const m of MARKUPS) {
    const g = { valid: 0, lost: 0, lostReasons: {}, lostPages: [], disagree: { enabled: [], disabled: [] } };
    for (const r of ok) {
      const x = r.result.markup[m];
      if (x.valid) {
        g.valid++;
        for (const view of ['enabled', 'disabled']) if (x[view] === 'disagree') g.disagree[view].push(r.id);
      } else if (r.result.markup.shipped.valid) {
        g.lost++;
        inc(g.lostReasons, x.why ?? '?');
        g.lostPages.push({ id: r.id, url: r.url, why: x.why });
      }
    }
    g.lostReasons = sorted(g.lostReasons);
    a.markup[m] = g;
  }
  for (const rule of HEAD_VARIANTS) {
    const g = { found: 0, foundShippedValid: 0, lostShippedValid: 0, lostPages: [], disagreeShippedValid: { enabled: [], disabled: [] }, disagreeAll: { enabled: 0, disabled: 0 } };
    for (const r of ok) {
      const x = r.result.head[rule];
      const valid = r.result.markup.shipped.valid;
      if (x.at >= 0) {
        g.found++;
        if (valid) g.foundShippedValid++;
        for (const view of ['enabled', 'disabled']) {
          if (x[view] !== 'disagree') continue;
          g.disagreeAll[view]++;
          if (valid) g.disagreeShippedValid[view].push(r.id);
        }
      } else if (valid && r.result.head.shipped.at >= 0) {
        g.lostShippedValid++;
        g.lostPages.push({ id: r.id, url: r.url });
      }
    }
    a.head[rule] = g;
  }
  // Pages that a server sends with RQP: valid markup and a head end tag.
  for (const m of MARKUPS) {
    for (const rule of HEAD_VARIANTS) {
      a.combined[`${m}+${rule}`] = ok.filter((r) => r.result.markup[m].valid && r.result.head[rule].at >= 0).length;
    }
  }
  for (const name of ['small', 'main']) {
    const g = { documents: 0, insertRefused: 0 };
    for (const m of MARKUPS) g[m] = { valid: 0, validWithBlocks: 0, lost: 0, lostReasons: {}, disagree: { enabled: [], disabled: [] } };
    for (const r of ok) {
      const v = r.result.variants[name];
      if (v === undefined) continue;
      if (v.error !== undefined) {
        g.insertRefused++;
        continue;
      }
      g.documents++;
      for (const m of MARKUPS) {
        const x = v.markup[m];
        if (x.valid) {
          g[m].valid++;
          if (x.blocks > 0) g[m].validWithBlocks++;
          for (const view of ['enabled', 'disabled']) if (x[view] === 'disagree') g[m].disagree[view].push(r.id);
        } else if (v.markup.shipped.valid) {
          g[m].lost++;
          inc(g[m].lostReasons, x.why ?? '?');
        }
      }
    }
    for (const m of MARKUPS) g[m].lostReasons = sorted(g[m].lostReasons);
    a.variants[name] = g;
  }
  a.headNoscript.tags = sorted(a.headNoscript.tags);
  a.headNoscript.h1Failures = sorted(a.headNoscript.h1Failures);
  a.headNoscript.h2Failures = sorted(a.headNoscript.h2Failures);
  a.otherNoscript.tags = sorted(a.otherNoscript.tags);
  return a;
}

async function run(o) {
  const pages = readFileSync(path.join(o.data, 'pages.jsonl'), 'utf8')
    .split('\n')
    .filter((l) => l !== '')
    .map((l) => JSON.parse(l))
    .filter((p) => p.corpus === 'yes');
  process.stderr.write(`pages: ${pages.length}\n`);
  let done = 0;
  const rows = await pool(pages, o.jobs, async (p) => {
    const file = path.join(o.data, 'pages', p.id, 'input.html');
    const argv = [process.execPath, PAGE_SCRIPT, file];
    if (p.charset) argv.push('--charset', p.charset);
    const r = await runProcess(argv, HERE, o.timeoutS * 1000, 4 * 1024 * 1024);
    done++;
    if (done % 50 === 0) process.stderr.write(`  ${done}/${pages.length}\n`);
    const base = { id: p.id, url: redactUrl(p.url), charset: p.charset ?? null };
    if (r.kind !== 'exit' || r.code !== 0) return { ...base, error: r.kind === 'exit' ? `exit ${r.code}` : r.kind };
    try {
      return { ...base, result: JSON.parse(r.stdout) };
    } catch {
      return { ...base, error: 'bad output' };
    }
  });
  mkdirSync(o.evidence, { recursive: true });
  writeFileSync(path.join(o.evidence, 'corpus-pages.jsonl'), `${rows.map((r) => JSON.stringify(r)).join('\n')}\n`);
  return rows;
}

async function main(argv) {
  const o = parseArgs(argv);
  const rows = o.aggregate !== null
    ? readFileSync(o.aggregate, 'utf8').split('\n').filter((l) => l !== '').map((l) => JSON.parse(l))
    : await run(o);
  const agg = { date: new Date().toISOString().slice(0, 10), ...aggregate(rows) };
  mkdirSync(o.evidence, { recursive: true });
  writeFileSync(path.join(o.evidence, 'corpus.json'), `${JSON.stringify(agg, null, 1)}\n`);
  process.stderr.write('done\n');
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  main(process.argv.slice(2)).catch((err) => {
    process.stderr.write(`corpus: ${err instanceof Error ? err.message : String(err)}\n`);
    process.exit(2);
  });
}
