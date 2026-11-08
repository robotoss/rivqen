// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

import assert from 'node:assert/strict';
import { truncate, writeFile } from 'node:fs/promises';
import path from 'node:path';
import test from 'node:test';
import { copyFixture, expectProblem, loadCatalog, problemsOf, saveCatalog } from './helpers.mjs';

const REC = 'evidence/recordings/wp-04/CAP-F-01-ffi-host-jvm.mp4';

test('recording_caps: a recording of 1 MB is accepted', async (t) => {
  const dir = await copyFixture(t);
  await truncate(path.join(dir, REC), 1_000_000);
  assert.deepEqual(await problemsOf(dir), []);
});

test('recording_caps: a recording above 1 MB is reported', async (t) => {
  const dir = await copyFixture(t);
  await truncate(path.join(dir, REC), 1_000_001);
  expectProblem(await problemsOf(dir), /recording is 1000001 bytes, the limit is 1000000/);
});

test('recording_caps: a total above 25 MB is reported', async (t) => {
  const dir = await copyFixture(t);
  const catalog = await loadCatalog(dir);
  // 26 results of 1 MB each (not-tested results need no runs); each file is at the limit.
  catalog[0].verdict = 'pending';
  catalog[0].results = [];
  for (let i = 1; i <= 26; i += 1) {
    const rel = `evidence/recordings/wp-04/bulk-${i}.mp4`;
    const file = path.join(dir, rel);
    await writeFile(file, 'x');
    await truncate(file, 1_000_000);
    catalog[0].results.push({ os: `o${i}`, verdict: 'not-tested', condition: null, runs: [], recording: rel });
  }
  await saveCatalog(dir, catalog);
  expectProblem(await problemsOf(dir, catalog), /recordings total 26000000 bytes, the limit is 25000000/);
});
