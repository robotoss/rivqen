// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// Byte study CLI (WP-17 S6 T-01). See README.md.
//
//   node run.mjs [--out FILE]                      compute and write the results
//   node run.mjs --lab URL --pages DIR --lab-out FILE
//                                                  run the legacy visits through lab-node
//   node run.mjs --check FILE                      compare the raw numbers of FILE with a new run
//   node run.mjs --tables FILE                     print the Markdown tables of the report from FILE

import { readFileSync, writeFileSync } from 'node:fs';
import { labRun } from './src/legacy.mjs';
import { compute, labScenarios } from './src/study.mjs';
import { tables } from './src/tables.mjs';

const arg = (name) => {
  const i = process.argv.indexOf(name);
  return i < 0 ? null : process.argv[i + 1] ?? null;
};
const here = (p) => new URL(p, import.meta.url);
const LAB_FILE = here('results/legacy-lab.json');

/** The raw columns only: they do not depend on the zlib version. */
export function rawOnly(value) {
  if (Array.isArray(value)) return value.map(rawOnly);
  if (value === null || typeof value !== 'object') return value;
  const out = {};
  for (const [k, v] of Object.entries(value)) {
    if (k === 'gzip' || k === 'br' || k === 'environment' || k.endsWith('_hpack')) continue;
    out[k] = rawOnly(v);
  }
  return out;
}

async function main() {
  const tablesFile = arg('--tables');
  if (tablesFile !== null) {
    const t = tables(JSON.parse(readFileSync(tablesFile, 'utf8')));
    for (const [name, text] of Object.entries(t)) console.log(`### ${name}\n\n${text}\n`);
    return;
  }
  const labUrl = arg('--lab');
  if (labUrl !== null) {
    const pagesDir = arg('--pages');
    const labOut = arg('--lab-out');
    if (pagesDir === null || labOut === null) throw new Error('--lab needs --pages and --lab-out');
    const observations = await labRun({ url: labUrl, pagesDir, scenarios: labScenarios() });
    writeFileSync(labOut, `${JSON.stringify({ upstream_commit: '59936beff656d4b5718ff6444d6c5e001a2c5231', sonic_differ: '1.0.7', observations }, null, 2)}\n`);
    console.log(`lab: ${observations.length} scenarios -> ${labOut}`);
    return;
  }
  let lab = null;
  try {
    lab = JSON.parse(readFileSync(LAB_FILE, 'utf8')).observations;
  } catch (error) {
    if (error.code !== 'ENOENT') throw error;
  }
  const results = await compute({ lab });
  const check = arg('--check');
  if (check !== null) {
    const committed = JSON.parse(readFileSync(check, 'utf8'));
    const a = JSON.stringify(rawOnly(committed));
    const b = JSON.stringify(rawOnly(results));
    if (a !== b) {
      console.error(`${check}: the raw numbers differ from a new run`);
      process.exitCode = 1;
      return;
    }
    console.log(`${check}: the raw numbers are current`);
    return;
  }
  const text = `${JSON.stringify(results, null, 2)}\n`;
  const out = arg('--out');
  if (out === null) process.stdout.write(text);
  else writeFileSync(out, text);
}

main().catch((error) => {
  console.error(`byte study failed: ${error.code ?? 'ERROR'}: ${error.message}`);
  process.exitCode = 1;
});
