#!/usr/bin/env node
// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// Corpus run of the H-40 study (WP-17 S3 T-01, report section 3.2). Same method as
// S1 T-15 (tools/rqp-markup/corpus/run.mjs): pages valid without blocks, and the
// simulated blocks "small" and "main" on the pages that all four candidates accept.
//
// Usage: node src/corpus.mjs --data <corpus-data> --scratch <dir> --evidence <dir> [--jobs 6] [--timeout-s 600]
//
// 1. src/corpus-page.mjs on each page (child process, time limit): every combination
//    of relaxations on the page and on its variants, parse5 oracle, attribute walk.
//    It writes the variant documents to <scratch>/variants/.
// 2. The four shipped candidates (candidates.json) in batch mode on the pages and on
//    the variants: the baseline of this corpus fetch, and their agreement.
// 3. <evidence>/corpus.jsonl (one line per page) and <evidence>/corpus-summary.json.
//
// Page bytes go only to <scratch>. The evidence files have URLs, numbers, rule ids and
// tag and attribute names only. Delete <scratch> after the run.

import { mkdirSync, readFileSync, writeFileSync, existsSync, readdirSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { parseCandidates, runCandidate, runProcess } from '../../../diff.mjs';
import { redactUrl } from '../../../corpus/run.mjs';
import { COMBOS } from './markup-relax.mjs';

const HERE = path.dirname(fileURLToPath(import.meta.url));
const REPO_ROOT = path.resolve(HERE, '..', '..', '..', '..', '..');
const WORKER = path.join(HERE, 'corpus-page.mjs');
const log = (s) => process.stderr.write(`${s}\n`);

export function parseArgs(argv) {
  const o = { data: null, scratch: null, evidence: null, jobs: 6, timeoutS: 600 };
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i];
    const v = argv[++i];
    if (v === undefined) throw new Error(`option ${a} needs a value`);
    if (a === '--data') o.data = path.resolve(v);
    else if (a === '--scratch') o.scratch = path.resolve(v);
    else if (a === '--evidence') o.evidence = path.resolve(v);
    else if (a === '--jobs') o.jobs = Number(v);
    else if (a === '--timeout-s') o.timeoutS = Number(v);
    else throw new Error(`unknown option ${a}`);
  }
  if (o.data === null || o.scratch === null || o.evidence === null) throw new Error('--data, --scratch and --evidence are required');
  if (!Number.isInteger(o.jobs) || o.jobs < 1 || o.jobs > 32) throw new Error('--jobs must be 1..32');
  if (!Number.isInteger(o.timeoutS) || o.timeoutS < 1) throw new Error('--timeout-s must be a positive integer');
  return o;
}

async function pool(items, jobs, fn) {
  let next = 0;
  const worker = async () => {
    while (next < items.length) {
      const i = next++;
      await fn(items[i], i);
    }
  };
  await Promise.all(Array.from({ length: Math.min(jobs, items.length) }, worker));
}

/** The four candidates on a batch directory: name -> (id -> { valid, blocks key }). */
async function candidates(dir, timeoutMs) {
  const list = parseCandidates(readFileSync(path.join(REPO_ROOT, 'tools', 'rqp-markup', 'candidates.json'), 'utf8'));
  const out = {};
  for (const c of list) {
    const t0 = Date.now();
    const r = await runCandidate({ ...c, build: null }, { repoRoot: REPO_ROOT, fixturesDir: dir, timeoutMs });
    log(`  ${c.name} on ${path.basename(dir)}: ${r.state} in ${((Date.now() - t0) / 1000).toFixed(1)} s`);
    if (r.state !== 'ok') throw new Error(`candidate ${c.name}: ${r.state} ${r.reason}`);
    const m = new Map();
    for (const [id, res] of r.results) {
      const blocks = Array.isArray(res.blocks) ? res.blocks.map((b) => `${b.id}/${b.start}/${b.end}`).join(',') : '';
      m.set(id, { valid: res.valid === true, key: res.valid === true ? blocks : 'invalid' });
    }
    out[c.name] = m;
  }
  return out;
}

/** Validity of the four candidates for one document: all valid, and whether they agree. */
function four(cands, id) {
  const rows = Object.values(cands).map((m) => m.get(id));
  if (rows.some((r) => r === undefined)) return { allValid: false, agree: false, missing: true };
  return { allValid: rows.every((r) => r.valid), agree: rows.every((r) => r.key === rows[0].key), missing: false };
}

async function main(argv) {
  const o = parseArgs(argv);
  const all = readFileSync(path.join(o.data, 'pages.jsonl'), 'utf8')
    .split('\n')
    .filter((l) => l !== '')
    .map((l) => JSON.parse(l));
  const pages = all.filter((p) => p.corpus === 'yes');
  const variantsDir = path.join(o.scratch, 'variants');
  if (existsSync(variantsDir) && readdirSync(variantsDir).length > 0) throw new Error(`${variantsDir} is not empty`);
  mkdirSync(variantsDir, { recursive: true });
  log(`pages: ${pages.length}`);

  const rows = new Map();
  let done = 0;
  const t0 = Date.now();
  await pool(pages, o.jobs, async (p) => {
    const argvW = [process.execPath, WORKER, path.join(o.data, 'pages', p.id, 'input.html'), '--variants-dir', variantsDir, '--id', p.id];
    if (p.charset) argvW.push('--charset', p.charset);
    const r = await runProcess(argvW, HERE, o.timeoutS * 1000, 64 * 1024 * 1024);
    let row;
    if (r.kind !== 'exit' || r.code !== 0) row = { error: r.kind === 'exit' ? `exit ${r.code}` : r.kind };
    else {
      try {
        row = JSON.parse(r.stdout);
      } catch {
        row = { error: 'bad output' };
      }
    }
    rows.set(p.id, row);
    done++;
    if (done % 50 === 0) log(`  ${done}/${pages.length} pages, ${((Date.now() - t0) / 1000).toFixed(0)} s`);
  });

  log('four candidates');
  const timeoutMs = o.timeoutS * 1000 * 10;
  const onPages = await candidates(path.join(o.data, 'pages'), timeoutMs);
  const onVariants = {};
  for (const v of ['small', 'main']) {
    const dir = path.join(variantsDir, v);
    onVariants[v] = existsSync(dir) ? await candidates(dir, timeoutMs) : null;
  }

  mkdirSync(o.evidence, { recursive: true });
  const lines = [];
  for (const p of pages) {
    const row = rows.get(p.id);
    const f = four(onPages, p.id);
    const variants = {};
    for (const v of ['small', 'main']) {
      const x = row.variants?.[v];
      if (!x) continue;
      const fv = onVariants[v] && x.combos ? four(onVariants[v], p.id) : null;
      variants[v] = { ...x, four: fv };
    }
    lines.push({
      id: p.id,
      url: redactUrl(p.url),
      category: p.category,
      kind: p.kind,
      lang: p.lang,
      platform: p.platform,
      bytes: p.bytes,
      four: f,
      ...row,
      variants,
    });
  }
  writeFileSync(path.join(o.evidence, 'corpus.jsonl'), `${lines.map((l) => JSON.stringify(l)).join('\n')}\n`);
  writeFileSync(path.join(o.evidence, 'corpus-summary.json'), `${JSON.stringify(summarize(lines), null, 1)}\n`);
  log('done');
}

const inc = (o, k, n = 1) => {
  o[k] = (o[k] ?? 0) + n;
};
const top = (o, n) => Object.fromEntries(Object.entries(o).sort((a, b) => b[1] - a[1]).slice(0, n));

/**
 * Aggregate the per-page lines. Exported for the tests.
 * @param {object[]} lines
 */
export function summarize(lines) {
  const s = {
    date: new Date().toISOString().slice(0, 10),
    combos: COMBOS,
    pages: lines.length,
    workerErrors: lines.filter((l) => l.error).map((l) => ({ id: l.id, error: l.error })),
    fourAllValid: 0,
    fourDisagree: 0,
    studyBaseEqualsShipped: 0,
    studyBaseEqualsFour: 0,
    page: {},
    variants: {},
  };
  const ok = lines.filter((l) => !l.error);
  for (const l of ok) {
    if (l.four.allValid) s.fourAllValid++;
    if (!l.four.agree) s.fourDisagree++;
    if (l.baseEqualsShipped) s.studyBaseEqualsShipped++;
    if (l.page.base.valid === l.four.allValid) s.studyBaseEqualsFour++;
  }
  for (const c of COMBOS) {
    const row = { valid: 0, gained: 0, gainedUrls: [], why: {}, findings: {}, gainedFindings: {} };
    for (const l of ok) {
      const x = l.page[c];
      if (x.valid) {
        row.valid++;
        if (!l.page.base.valid) {
          row.gained++;
          row.gainedUrls.push(l.url);
          for (const k of x.findings ?? []) inc(row.gainedFindings, k);
        } else for (const k of x.findings ?? []) inc(row.findings, k);
      } else inc(row.why, x.why);
    }
    row.why = top(row.why, 12);
    s.page[c] = row;
  }
  for (const v of ['small', 'main']) {
    const out = { basePages: 0, noPick: 0, variantsA: 0, fourValidA: 0, fourDisagreeA: 0, baseEqualsShippedA: 0, combos: {} };
    for (const l of ok) {
      if (!l.four.allValid) continue;
      out.basePages++;
      const x = l.variants[v];
      if (!x || !x.combos) {
        if (x && x.picks === 0) out.noPick++;
        continue;
      }
      out.variantsA++;
      if (x.four?.allValid) out.fourValidA++;
      if (x.four && !x.four.agree) out.fourDisagreeA++;
      if (x.baseEqualsShipped) out.baseEqualsShippedA++;
    }
    for (const c of COMBOS) {
      const row = { validA: 0, gainedA: 0, pagesB: 0, variantsB: 0, validB: 0, why: {}, findings: {}, gainedFindings: {}, foreignVariants: 0, noscriptSvgVariants: 0 };
      for (const l of ok) {
        const x = l.variants[v];
        const pageValid = l.page[c].valid;
        if (pageValid) row.pagesB++;
        if (!x || !x.combos) continue;
        const r = x.combos[c];
        if (pageValid) {
          row.variantsB++;
          if (r.valid) row.validB++;
        }
        if (!l.four.allValid) continue;
        if (r.valid) {
          row.validA++;
          if (r.foreignTags > 0) row.foreignVariants++;
          if (r.noscriptSvg) row.noscriptSvgVariants++;
          if (!x.combos.base.valid) {
            row.gainedA++;
            for (const k of r.findings ?? []) inc(row.gainedFindings, k);
          } else for (const k of r.findings ?? []) inc(row.findings, k);
        } else inc(row.why, r.why);
      }
      row.why = top(row.why, 15);
      out.combos[c] = row;
    }
    out.attrs = attrSummary(ok.filter((l) => l.four.allValid).map((l) => l.variants[v]?.attrs).filter(Boolean));
    s.variants[v] = out;
  }
  return s;
}

function attrSummary(list) {
  const a = {
    variants: list.length,
    withForeign: 0,
    htmlTags: 0,
    foreignTags: 0,
    current: { html: {}, foreign: {} },
    proposed: {},
    rejectedCurrent: 0,
    rejectedCurrentForeignOnly: 0,
    rejectedProposed: 0,
    foreignElements: {},
    foreignElementVariants: {},
    foreignAttributes: {},
  };
  for (const x of list) {
    a.htmlTags += x.htmlTags;
    a.foreignTags += x.foreignTags;
    if (x.foreignTags > 0) a.withForeign++;
    for (const ns of ['html', 'foreign']) for (const [k, n] of Object.entries(x.current[ns])) inc(a.current[ns], k, n);
    for (const [k, n] of Object.entries(x.proposed)) inc(a.proposed, k, n);
    if (x.rejectedCurrent) a.rejectedCurrent++;
    const htmlRejects = Object.values(x.current.html).some((n) => n > 0);
    if (x.rejectedCurrent && !htmlRejects) a.rejectedCurrentForeignOnly++;
    if (x.rejectedProposed) a.rejectedProposed++;
    for (const [k, n] of Object.entries(x.foreignElements)) {
      inc(a.foreignElements, k, n);
      inc(a.foreignElementVariants, k);
    }
    for (const [k, n] of Object.entries(x.foreignAttributes)) inc(a.foreignAttributes, k, n);
  }
  a.foreignElements = top(a.foreignElements, 40);
  a.foreignElementVariants = top(a.foreignElementVariants, 40);
  a.foreignAttributes = top(a.foreignAttributes, 60);
  return a;
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  main(process.argv.slice(2)).catch((err) => {
    process.stderr.write(`corpus: ${err instanceof Error ? err.message : String(err)}\n`);
    process.exit(2);
  });
}
