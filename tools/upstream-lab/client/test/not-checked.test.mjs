// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

import assert from 'node:assert/strict';
import { mkdir, mkdtemp, readFile, rm, writeFile } from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';
import { canonicalLine } from '../src/canon.mjs';
import { checkTree, runJsonProblems } from '../src/check.mjs';
import { buildRunJson } from '../src/runjson.mjs';

const SHA = 'a'.repeat(64);
const HTTP = 'GET /catalog HTTP/1.1\nHost: 10.0.2.2:18090\n\n--- response ---\nHTTP/1.1 200 OK\n\nbody\n';

function runText() {
  const raw = [
    ['conductor', 'run.start', { scenario: 'CS-01', mode: 'quick', repeat: 1, origin: 'lab-node', upstream_sha: '59936bef' }],
    ['conductor', 'step.start', { step: { do: 'create_session', seq: 1 } }],
    ['harness', 'session.created', { thread: 'main', session_id: 'abc' }],
    ['harness', 'step.done', { thread: 'main', step: { do: 'create_session', seq: 1 } }],
    ['proxy', 'proxy.request', { exchange: 1, method: 'GET', target: '/catalog', fields: [['Host', '10.0.2.2:18090']] }],
    ['proxy', 'proxy.response_head', { exchange: 1, status: 200, fields: [['ETag', 'e1']] }],
    ['proxy', 'proxy.response_end', { exchange: 1, body_len: 4, body_sha256: SHA, complete: true }],
    ['conductor', 'run.end', { ok: true, error: null }],
  ];
  let hseq = 0;
  return raw.map(([src, ev, members], i) => `${canonicalLine({ ...members, ...(src === 'harness' ? { hseq: ++hseq } : {}), ev, seq: i + 1, src, t: i })}\n`).join('');
}

async function tree(count) {
  const root = await mkdtemp(path.join(os.tmpdir(), 'lab-nc-'));
  const dir = path.join(root, 'android', 'CS-01', 'quick');
  await mkdir(dir, { recursive: true });
  for (let k = 1; k <= count; k += 1) {
    await writeFile(path.join(dir, `run-${k}.jsonl`), runText());
    await writeFile(path.join(dir, `run-${k}-x1.http`), HTTP);
  }
  const j = await buildRunJson({ root, meta: { date: '2026-11-05', tools: { node: 'v22' }, upstreamSha: '59936bef', emulatorImage: 'api28' } });
  await writeFile(path.join(root, 'run.json'), JSON.stringify(j));
  return { root, file: path.join(root, 'run.json'), j, done: () => rm(root, { recursive: true, force: true }) };
}
const KEY = { scenario: 'CS-01', mode: 'quick' };
const summary = (s) => ({ stable: [], unstable: [], not_checked: [], ...s });

test('check_one_run_is_not_checked', async () => {
  const t = await tree(1);
  try {
    assert.deepEqual(t.j.summary, summary({ not_checked: [KEY] }));
    const res = await checkTree(t.root);
    assert.deepEqual(res.errors, []);
    assert.deepEqual(res.notChecked, [KEY]);
  } finally {
    await t.done();
  }
});

test('check_one_run_listed_as_stable_or_unlisted_is_an_error', async () => {
  const t = await tree(1);
  try {
    await writeFile(t.file, JSON.stringify({ ...t.j, summary: summary({ stable: [KEY] }) }));
    assert.deepEqual((await checkTree(t.root)).errors, [
      'CS-01/quick: run.json lists one run as stable or unstable; it is not checked',
      'CS-01/quick: one run; not listed in run.json summary.not_checked',
    ]);
    await writeFile(t.file, JSON.stringify({ ...t.j, summary: summary({ unstable: [{ ...KEY, reason: 'r' }] }) }));
    assert.ok((await checkTree(t.root)).errors.includes('CS-01/quick: run.json lists one run as stable or unstable; it is not checked'));
  } finally {
    await t.done();
  }
});

test('check_two_runs_listed_as_not_checked_is_an_error', async () => {
  const t = await tree(2);
  try {
    assert.deepEqual(t.j.summary, summary({ stable: [KEY] }));
    await writeFile(t.file, JSON.stringify({ ...t.j, summary: summary({ stable: [KEY], not_checked: [KEY] }) }));
    assert.deepEqual((await checkTree(t.root)).errors, ['CS-01/quick: run.json says not checked, the mode has 2 runs']);
  } finally {
    await t.done();
  }
});

test('run_json_not_checked_must_be_a_list_of_scenario_and_mode', async () => {
  const t = await tree(1);
  try {
    const bad = { ...t.j, summary: summary({ not_checked: [KEY, { scenario: 'CS-01' }] }) };
    assert.deepEqual(runJsonProblems(bad), ['run.json: summary.not_checked must be a list of {scenario, mode}']);
    assert.deepEqual(runJsonProblems({ ...t.j, summary: { stable: [], unstable: [], not_checked: 'x' } }), ['run.json: summary.not_checked must be a list of {scenario, mode}']);
    // run.json written before S2 has no not_checked: still valid input.
    assert.deepEqual(runJsonProblems({ ...t.j, summary: { stable: [], unstable: [] } }), []);
    assert.equal(JSON.parse(await readFile(t.file, 'utf8')).summary.not_checked.length, 1);
  } finally {
    await t.done();
  }
});
