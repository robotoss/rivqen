// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

import assert from 'node:assert/strict';
import { cp, mkdtemp, readFile, rm, writeFile } from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { runCheck } from '../src/check.mjs';

const here = path.dirname(fileURLToPath(import.meta.url));
export const FIXTURE = path.join(here, 'fixtures', 'valid');
export const RUN_DIR = 'evidence/spikes/wp-04/ffi/CAP-F-01/host-jvm';

/** Copies the valid fixture tree into a fresh temporary directory that the test may change. */
export async function copyFixture(t) {
  const dir = await mkdtemp(path.join(os.tmpdir(), 'spikes-check-'));
  t.after(() => rm(dir, { recursive: true, force: true }));
  await cp(FIXTURE, dir, { recursive: true });
  return dir;
}

export async function loadCatalog(dir) {
  return JSON.parse(await readFile(path.join(dir, 'capabilities.json'), 'utf8'));
}

export async function saveCatalog(dir, catalog) {
  await writeFile(path.join(dir, 'capabilities.json'), `${JSON.stringify(catalog, null, 2)}\n`);
}

/** Problems of the tree in `dir`, using its own capabilities.json. */
export async function problemsOf(dir, catalog) {
  const data = catalog ?? (await loadCatalog(dir));
  const { errors } = await runCheck({ root: dir, catalog: data });
  return errors;
}

export function runLines(text) {
  return text.split('\n').filter((line) => line !== '');
}

export async function rewriteRun(dir, name, mutate) {
  const file = path.join(dir, RUN_DIR, name);
  const lines = runLines(await readFile(file, 'utf8'));
  await writeFile(file, `${mutate(lines).join('\n')}\n`);
}

export function expectProblem(problems, pattern) {
  assert.ok(problems.some((p) => pattern.test(p)), `no problem matches ${pattern}: ${JSON.stringify(problems)}`);
}
