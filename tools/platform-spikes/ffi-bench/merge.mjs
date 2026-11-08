// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// Joins the case files of one target into the result file of CONTRACT.md section 10.
//   node merge.mjs --target <t> --date <d> --machine <m> --os <o> [--toolchain k=v]...
//                  [--toolchain-file <json>] --out <file> <case.json>...
// Fails (exit 1) when a case is missing a member, has sha256_ok false, or has docs != the others.

import { readFileSync, writeFileSync } from 'node:fs';

const CASE_KEYS = [
  'path', 'size', 'chunk', 'docs', 'median_ns', 'p95_ns', 'chunk_median_ns',
  'copied_bytes', 'rss_delta_kib', 'sha256_ok',
];

function fail(msg) {
  process.stderr.write(`merge: ${msg}\n`);
  process.exit(1);
}

const argv = process.argv.slice(2);
const opt = { toolchain: {} };
const files = [];
for (let i = 0; i < argv.length; i += 1) {
  const a = argv[i];
  if (!a.startsWith('--')) {
    files.push(a);
    continue;
  }
  const v = argv[i + 1];
  if (v === undefined) fail(`${a} needs a value`);
  i += 1;
  if (a === '--toolchain') {
    const eq = v.indexOf('=');
    if (eq < 1) fail(`bad --toolchain ${v}`);
    opt.toolchain[v.slice(0, eq)] = v.slice(eq + 1);
  } else if (a === '--toolchain-file') {
    Object.assign(opt.toolchain, JSON.parse(readFileSync(v, 'utf8')));
  } else {
    opt[a.slice(2)] = v;
  }
}
for (const k of ['target', 'date', 'machine', 'os', 'out']) {
  if (!opt[k]) fail(`--${k} is required`);
}
if (files.length === 0) fail('no case files');

const cases = files.map((f) => {
  const c = JSON.parse(readFileSync(f, 'utf8'));
  for (const k of CASE_KEYS) {
    if (!(k in c)) fail(`${f}: no ${k}`);
  }
  if (c.sha256_ok !== true) fail(`${f}: sha256_ok is not true`);
  // Contract member order.
  return Object.fromEntries(CASE_KEYS.map((k) => [k, c[k]]));
});

const toolchain = Object.fromEntries(
  Object.keys(opt.toolchain).sort().map((k) => [k, opt.toolchain[k]]),
);
const result = {
  target: opt.target,
  date: opt.date,
  machine: opt.machine,
  os: opt.os,
  toolchain,
  indicative: true,
  cases,
};
writeFileSync(opt.out, `${JSON.stringify(result, null, 2)}\n`);
process.stdout.write(`wrote ${opt.out} (${cases.length} cases)\n`);
