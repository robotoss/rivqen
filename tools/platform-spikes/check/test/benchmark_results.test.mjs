// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

import assert from 'node:assert/strict';
import test from 'node:test';
import { catalogProblems } from '../src/catalog.mjs';
import { evidenceProblems } from '../src/evidence.mjs';
import { copyFixture, loadCatalog } from './helpers.mjs';

async function withResult(t, change) {
  const dir = await copyFixture(t);
  const catalog = await loadCatalog(dir);
  Object.assign(catalog[0].results[0], change);
  return { catalog, ...(await evidenceProblems(catalog, dir)) };
}

test('benchmark_result: a supported benchmark result needs its metrics file and no runs', async (t) => {
  const { catalog, problems } = await withResult(t, { kind: 'benchmark', runs: [] });
  assert.deepEqual(problems, []);
  assert.deepEqual(catalogProblems(catalog), []);
});

test('benchmark_result: a benchmark result without metrics or with runs is reported', async (t) => {
  const dir = await copyFixture(t);
  const catalog = await loadCatalog(dir);
  Object.assign(catalog[0].results[0], { kind: 'benchmark', metrics: null });
  assert.ok(catalogProblems(catalog).some((p) => p.endsWith('a benchmark result needs a metrics file')));
  Object.assign(catalog[0].results[0], { metrics: 'x.json' });
  assert.ok(catalogProblems(catalog).some((p) => p.endsWith('a benchmark result has no runs')));
  Object.assign(catalog[0].results[0], { kind: 'other', runs: [] });
  assert.ok(catalogProblems(catalog).some((p) => p.endsWith('kind must be "benchmark" when it is set')));
});

test('benchmark_result: without kind a supported result still needs 3 runs', async (t) => {
  const { problems } = await withResult(t, { runs: [] });
  assert.deepEqual(problems, ['CAP-F-01/host-jvm: 0 runs, at least 3 are needed']);
});
