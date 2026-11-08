// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

import assert from 'node:assert/strict';
import { truncate, writeFile } from 'node:fs/promises';
import path from 'node:path';
import test from 'node:test';
import { evidenceProblems } from '../src/evidence.mjs';
import { copyFixture, expectProblem, loadCatalog, problemsOf, RUN_DIR, rewriteRun, saveCatalog } from './helpers.mjs';

const RUNS = [1, 2, 3].map((k) => `${RUN_DIR}/run-${k}.jsonl`);

/** Evidence problems of the fixture tree with the result of the first capability changed. */
async function withResult(t, change) {
  const dir = await copyFixture(t);
  const catalog = await loadCatalog(dir);
  Object.assign(catalog[0].results[0], change);
  return { dir, catalog, ...(await evidenceProblems(catalog, dir)) };
}

test('needs_runs: a supported result with no runs needs 3', async (t) => {
  const { problems } = await withResult(t, { runs: [] });
  assert.deepEqual(problems, ['CAP-F-01/host-jvm: 0 runs, at least 3 are needed']);
});

test('needs_runs: a supported result with a missing runs member needs 3 and does not crash', async (t) => {
  const { problems } = await withResult(t, { runs: undefined });
  assert.deepEqual(problems, ['CAP-F-01/host-jvm: 0 runs, at least 3 are needed']);
});

test('needs_runs: a conditional result with 2 runs needs 3', async (t) => {
  const { problems } = await withResult(t, { verdict: 'conditional', runs: RUNS.slice(0, 2) });
  assert.deepEqual(problems, ['CAP-F-01/host-jvm: 2 runs, at least 3 are needed']);
});

test('needs_runs: the same run listed three times counts once', async (t) => {
  const { problems } = await withResult(t, { runs: [RUNS[0], RUNS[0], RUNS[0]] });
  assert.deepEqual(problems, ['CAP-F-01/host-jvm: 1 runs, at least 3 are needed']);
});

test('needs_runs: a not-tested result needs no runs', async (t) => {
  const { problems } = await withResult(t, { verdict: 'not-tested', runs: [] });
  assert.deepEqual(problems, []);
});

test('needs_runs: a not-tested result with some runs needs 3', async (t) => {
  const { problems } = await withResult(t, { verdict: 'not-tested', runs: RUNS.slice(0, 1) });
  assert.deepEqual(problems, ['CAP-F-01/host-jvm: 1 runs, at least 3 are needed']);
});

test('needs_runs: an impossible result with a source and no runs needs none', async (t) => {
  const { problems } = await withResult(t, { verdict: 'impossible', runs: [], source: 'https://x.test/p' });
  assert.deepEqual(problems, []);
});

test('needs_runs: an impossible result with one run needs 3', async (t) => {
  const { problems } = await withResult(t, { verdict: 'impossible', runs: RUNS.slice(0, 1) });
  assert.deepEqual(problems, ['CAP-F-01/host-jvm: 1 runs, at least 3 are needed']);
});

test('needs_runs: entries of runs that are not strings are ignored by the evidence check', async (t) => {
  const { problems } = await withResult(t, { runs: [...RUNS, 5, null] });
  assert.deepEqual(problems, []);
});

test('evidence: a result without an os is labelled by its position', async (t) => {
  const { problems } = await withResult(t, { os: undefined, runs: [] });
  assert.deepEqual(problems, ['CAP-F-01/result 1: 0 runs, at least 3 are needed']);
});

test('evidence: rows and results that are not objects are skipped and not counted', async (t) => {
  const dir = await copyFixture(t);
  const catalog = [null, 'x', [], { id: 'A', results: 'none' }, { id: 'B', results: [null, 5, []] }];
  assert.deepEqual(await evidenceProblems(catalog, dir), { problems: [], counts: { results: 0, runs: 0, recordings: 0 } });
});

test('evidence: a catalog that is not an array has no evidence', async (t) => {
  const dir = await copyFixture(t);
  assert.deepEqual(await evidenceProblems({ id: 'x' }, dir), { problems: [], counts: { results: 0, runs: 0, recordings: 0 } });
});

test('evidence: a run of another platform is reported', async (t) => {
  const dir = await copyFixture(t);
  await rewriteRun(dir, 'run-1.jsonl', (lines) => lines.map((l) => l.replace('"platform":"ffi"', '"platform":"url"')));
  expectProblem(await problemsOf(dir), /run-1\.jsonl: run\.start platform "url" is not "ffi"/);
});

test('evidence: an empty run log gives its own problem and does not crash the check', async (t) => {
  const dir = await copyFixture(t);
  await writeFile(path.join(dir, RUNS[1]), '');
  assert.deepEqual(await problemsOf(dir), [`CAP-F-01/host-jvm: ${RUNS[1]}: empty log`]);
});

test('evidence: a run with shape problems is not compared with the other runs', async (t) => {
  const dir = await copyFixture(t);
  // Unsorted keys on the app.stream.read line, and another byte count: only the shape problem is reported.
  await rewriteRun(dir, 'run-2.jsonl', (lines) =>
    lines.map((l, i) => (i === 2 ? '{"ev":"app.stream.read","bytes":999,"seq":1,"src":"app","t":15}' : l)),
  );
  assert.deepEqual(await problemsOf(dir), [`CAP-F-01/host-jvm: ${RUNS[1]}: line 3: keys are not sorted`]);
});

test('evidence: a run with a bad run.start id is not checked against its capability', async (t) => {
  const dir = await copyFixture(t);
  await rewriteRun(dir, 'run-3.jsonl', (lines) => lines.map((l) => l.replace('"run":"CAP-F-01-ffi-host-jvm-3"', '"run":""').replace('"capability":"CAP-F-01"', '"capability":"OTHER"')));
  assert.deepEqual(await problemsOf(dir), [`CAP-F-01/host-jvm: ${RUNS[2]}: run.start has no run id`]);
});

test('recording_total: recordings that add up to exactly 25 MB are accepted', async (t) => {
  const dir = await copyFixture(t);
  const catalog = await loadCatalog(dir);
  catalog[0].verdict = 'pending';
  catalog[0].results = [];
  for (let i = 1; i <= 25; i += 1) {
    const rel = `evidence/recordings/wp-04/bulk-${i}.mp4`;
    await writeFile(path.join(dir, rel), 'x');
    await truncate(path.join(dir, rel), 1_000_000);
    catalog[0].results.push({ os: `o${i}`, verdict: 'not-tested', condition: null, runs: [], recording: rel });
  }
  await saveCatalog(dir, catalog);
  assert.deepEqual(await problemsOf(dir, catalog), []);
  assert.equal((await evidenceProblems(catalog, dir)).counts.recordings, 25);
});

test('recording_total: a recording listed twice counts once', async (t) => {
  const dir = await copyFixture(t);
  const catalog = await loadCatalog(dir);
  catalog[0].verdict = 'pending';
  catalog[0].results = [];
  const rel = 'evidence/recordings/wp-04/CAP-F-01-ffi-host-jvm.mp4';
  await truncate(path.join(dir, rel), 1_000_000);
  for (let i = 1; i <= 26; i += 1) catalog[0].results.push({ os: `o${i}`, verdict: 'not-tested', condition: null, runs: [], recording: rel });
  assert.deepEqual((await evidenceProblems(catalog, dir)).problems, []);
});
