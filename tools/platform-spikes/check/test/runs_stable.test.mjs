// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

import assert from 'node:assert/strict';
import test from 'node:test';
import { copyFixture, expectProblem, loadCatalog, problemsOf, rewriteRun, saveCatalog } from './helpers.mjs';

test('runs_stable: runs that differ only in timing, read splits, metrics and Date are stable', async (t) => {
  // The valid fixture has these differences between its three runs.
  const dir = await copyFixture(t);
  assert.deepEqual(await problemsOf(dir), []);
});

test('runs_stable: a result with fewer than 3 runs is reported', async (t) => {
  const dir = await copyFixture(t);
  const catalog = await loadCatalog(dir);
  catalog[0].results[0].runs = catalog[0].results[0].runs.slice(0, 2);
  await saveCatalog(dir, catalog);
  expectProblem(await problemsOf(dir, catalog), /2 runs, at least 3 are needed/);
});

test('runs_stable: a run whose normalized log differs is reported', async (t) => {
  const dir = await copyFixture(t);
  await rewriteRun(dir, 'run-2.jsonl', (lines) => lines.map((l) => l.replace('"status":200', '"status":404')));
  expectProblem(await problemsOf(dir), /run-2\.jsonl differs from .*run-1\.jsonl after normalization/);
});
