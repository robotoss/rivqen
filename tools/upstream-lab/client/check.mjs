#!/usr/bin/env node
// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// Offline check of the stored client evidence: node check.mjs [<evidence dir>] [--require-functions]
// Exit code 0: the evidence is valid. 1: findings. 2: bad use or unreadable input.
import { readFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { checkTree, functionsProblems } from './src/check.mjs';
import { catalogProblems } from './src/steps.mjs';

const here = path.dirname(fileURLToPath(import.meta.url));
const say = (text) => process.stdout.write(`${text}\n`);

async function main() {
  const args = process.argv.slice(2);
  const requireFunctions = args.includes('--require-functions');
  const rest = args.filter((a) => a !== '--require-functions');
  if (rest.length > 1 || rest.some((a) => a.startsWith('--'))) {
    process.stderr.write('usage: node check.mjs [<evidence dir>] [--require-functions]\n');
    return 2;
  }
  const root = path.resolve(rest[0] ?? path.join(here, '..', '..', '..', 'evidence', 'traces', 'client'));
  const catalog = JSON.parse(await readFile(path.join(here, 'scenarios.json'), 'utf8'));
  const cases = JSON.parse(await readFile(path.join(here, 'functions.json'), 'utf8'));
  const problems = catalogProblems(catalog);
  const { errors, unstable, runs } = await checkTree(root, { catalog });
  errors.push(...problems.map((p) => `scenarios.json: ${p}`));
  for (const platform of ['android', 'ios']) {
    let text = null;
    try {
      text = await readFile(path.join(root, platform, 'functions.jsonl'), 'utf8');
    } catch (cause) {
      if (cause.code !== 'ENOENT') throw cause;
      if (requireFunctions) errors.push(`${platform}/functions.jsonl is missing`);
    }
    if (text !== null) errors.push(...functionsProblems(text, platform, cases).map((p) => `${platform}/functions.jsonl: ${p}`));
  }
  for (const u of unstable) say(`unstable (recorded in run.json): ${u.scenario}/${u.mode}`);
  for (const e of errors) say(`ERROR ${e}`);
  say(`${runs} runs checked, ${errors.length} errors`);
  return errors.length === 0 ? 0 : 1;
}

main().then(
  (code) => { process.exitCode = code; },
  (e) => {
    process.stderr.write(`check failed: ${e.code ?? e.message}\n`);
    process.exitCode = 2;
  },
);
