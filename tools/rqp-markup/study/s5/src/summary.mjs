#!/usr/bin/env node
// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// Totals of the S5 sweeps (results/sweep-*.json) per option combination: documents,
// valid, gained, findings (base-valid and gained documents together), informative kinds
// (known classes, depth inside templates), patch rule rejections and patch apply
// findings, monotonic violations.
//
// Usage: node src/summary.mjs --out results/summary.json results/sweep-*.json

import { readFileSync, writeFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const GENERATED = new Set(['classic', 'review', 'review2', 'review3', 'tpl', 'nos', 'mix']);

const add = (to, from) => {
  for (const [k, n] of Object.entries(from ?? {})) to[k] = (to[k] ?? 0) + n;
};

/** Totals of sweep outputs. */
export function summarize(sweeps) {
  const out = { documents: 0, generated: 0, monotonic: 0, sources: {}, combos: {} };
  for (const s of sweeps) {
    out.documents += s.count;
    if (GENERATED.has(s.source)) out.generated += s.count;
    out.monotonic += s.monotonic;
    out.sources[s.source] = s.count;
    for (const [c, r] of Object.entries(s.combos)) {
      const t = (out.combos[c] ??= { valid: 0, gained: 0, findingDocuments: 0, findings: {}, informative: {}, patchRejected: {}, patchFindings: {} });
      t.valid += r.valid;
      t.gained += r.gained;
      t.findingDocuments += r.docsWithFinding + r.gainedDocsWithFinding;
      add(t.findings, r.findings);
      add(t.findings, r.gainedFindings);
      add(t.informative, r.informative);
      add(t.patchRejected, r.patch.rejected);
      add(t.patchFindings, r.patch.findings);
    }
  }
  return out;
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    const argv = process.argv.slice(2);
    const i = argv.indexOf('--out');
    if (i < 0 || argv[i + 1] === undefined) throw new Error('--out <file> is required');
    const out = argv[i + 1];
    const files = argv.filter((_, k) => k !== i && k !== i + 1);
    const sweeps = files.map((f) => JSON.parse(readFileSync(f, 'utf8')));
    writeFileSync(out, `${JSON.stringify({ date: sweeps[0]?.date ?? null, ...summarize(sweeps) }, null, 1)}\n`);
  } catch (err) {
    process.stderr.write(`summary: ${err instanceof Error ? err.message : String(err)}\n`);
    process.exit(2);
  }
}
