// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

import assert from 'node:assert/strict';
import { rm } from 'node:fs/promises';
import path from 'node:path';
import test from 'node:test';
import { copyFixture, expectProblem, loadCatalog, problemsOf, saveCatalog, RUN_DIR } from './helpers.mjs';

test('links_exist: the valid fixture tree has no problems', async (t) => {
  const dir = await copyFixture(t);
  assert.deepEqual(await problemsOf(dir), []);
});

test('links_exist: a missing run file is reported', async (t) => {
  const dir = await copyFixture(t);
  await rm(path.join(dir, RUN_DIR, 'run-2.jsonl'));
  expectProblem(await problemsOf(dir), /missing file .*run-2\.jsonl/);
});

test('links_exist: a missing metrics file is reported', async (t) => {
  const dir = await copyFixture(t);
  await rm(path.join(dir, RUN_DIR, 'run-1.metrics.json'));
  expectProblem(await problemsOf(dir), /missing file .*run-1\.metrics\.json/);
});

test('links_exist: a missing recording is reported', async (t) => {
  const dir = await copyFixture(t);
  await rm(path.join(dir, 'evidence/recordings/wp-04/CAP-F-01-ffi-host-jvm.mp4'));
  expectProblem(await problemsOf(dir), /missing file .*CAP-F-01-ffi-host-jvm\.mp4/);
});

test('links_exist: a path that leaves the root is reported and not read', async (t) => {
  const dir = await copyFixture(t);
  const catalog = await loadCatalog(dir);
  catalog[0].results[0].recording = '../outside.mp4';
  await saveCatalog(dir, catalog);
  expectProblem(await problemsOf(dir, catalog), /path leaves the root: \.\.\/outside\.mp4/);
});

test('links_exist: an absolute path is reported as leaving the root', async (t) => {
  const dir = await copyFixture(t);
  const catalog = await loadCatalog(dir);
  catalog[0].results[0].metrics = path.join(dir, RUN_DIR, 'run-1.metrics.json');
  expectProblem(await problemsOf(dir, catalog), /path leaves the root/);
});
