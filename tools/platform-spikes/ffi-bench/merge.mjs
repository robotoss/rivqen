// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// Joins the case files of one target into the result file of CONTRACT.md section 10.
//   node merge.mjs --target <t> --date <d> --machine <m> --os <o> [--toolchain k=v]...
//                  [--toolchain-file <json>] --out <file> <case.json>...
// Fails (exit 1) when a case is missing a member, has sha256_ok false, has docs != the
// others, or has a copy split (rust_copied_bytes, glue_copied_bytes) that does not add up to
// copied_bytes. Tests: node --test merge.test.mjs

import { readFileSync, writeFileSync } from 'node:fs';
import { pathToFileURL } from 'node:url';

const CASE_KEYS = [
  'path', 'size', 'chunk', 'docs', 'median_ns', 'p95_ns', 'chunk_median_ns',
  'copied_bytes', 'rss_delta_kib', 'sha256_ok',
];
// Optional members of section 10, written right after copied_bytes.
const SPLIT_KEYS = ['rust_copied_bytes', 'glue_copied_bytes'];

export class MergeError extends Error {
  constructor(message) {
    super(message);
    this.name = 'MergeError';
    this.code = 'E_MERGE';
  }
}

const isCount = (v) => Number.isSafeInteger(v) && v >= 0;

/** One case in contract member order; `name` names the source in errors. */
export function normalizeCase(c, name) {
  if (c === null || typeof c !== 'object' || Array.isArray(c)) {
    throw new MergeError(`${name}: not an object`);
  }
  for (const k of CASE_KEYS) {
    if (!(k in c)) throw new MergeError(`${name}: no ${k}`);
  }
  if (c.sha256_ok !== true) throw new MergeError(`${name}: sha256_ok is not true`);
  const split = SPLIT_KEYS.filter((k) => k in c);
  if (split.length === 1) throw new MergeError(`${name}: ${split[0]} without its pair`);
  const out = {};
  for (const k of CASE_KEYS) {
    out[k] = c[k];
    if (k === 'copied_bytes' && split.length === 2) {
      for (const s of SPLIT_KEYS) {
        if (!isCount(c[s])) throw new MergeError(`${name}: ${s} is not a count`);
        out[s] = c[s];
      }
      if (c.rust_copied_bytes + c.glue_copied_bytes !== c.copied_bytes) {
        throw new MergeError(`${name}: rust_copied_bytes + glue_copied_bytes != copied_bytes`);
      }
    }
  }
  return out;
}

/** The result file object of one target. */
export function buildResult(opt, namedCases) {
  for (const k of ['target', 'date', 'machine', 'os']) {
    if (!opt[k]) throw new MergeError(`--${k} is required`);
  }
  if (namedCases.length === 0) throw new MergeError('no case files');
  const cases = namedCases.map(([name, c]) => normalizeCase(c, name));
  const docs = cases[0].docs;
  cases.forEach((c, i) => {
    if (c.docs !== docs) throw new MergeError(`${namedCases[i][0]}: docs ${c.docs}, others ${docs}`);
  });
  const tc = opt.toolchain ?? {};
  const toolchain = Object.fromEntries(Object.keys(tc).sort().map((k) => [k, tc[k]]));
  return {
    target: opt.target,
    date: opt.date,
    machine: opt.machine,
    os: opt.os,
    toolchain,
    indicative: true,
    cases,
  };
}

function main(argv) {
  const opt = { toolchain: {} };
  const files = [];
  for (let i = 0; i < argv.length; i += 1) {
    const a = argv[i];
    if (!a.startsWith('--')) {
      files.push(a);
      continue;
    }
    const v = argv[i + 1];
    if (v === undefined) throw new MergeError(`${a} needs a value`);
    i += 1;
    if (a === '--toolchain') {
      const eq = v.indexOf('=');
      if (eq < 1) throw new MergeError(`bad --toolchain ${v}`);
      opt.toolchain[v.slice(0, eq)] = v.slice(eq + 1);
    } else if (a === '--toolchain-file') {
      Object.assign(opt.toolchain, JSON.parse(readFileSync(v, 'utf8')));
    } else {
      opt[a.slice(2)] = v;
    }
  }
  if (!opt.out) throw new MergeError('--out is required');
  const result = buildResult(opt, files.map((f) => [f, JSON.parse(readFileSync(f, 'utf8'))]));
  writeFileSync(opt.out, `${JSON.stringify(result, null, 2)}\n`);
  process.stdout.write(`wrote ${opt.out} (${result.cases.length} cases)\n`);
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  try {
    main(process.argv.slice(2));
  } catch (e) {
    process.stderr.write(`merge: ${e.message}\n`);
    process.exit(1);
  }
}
