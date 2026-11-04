// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

import assert from 'node:assert/strict';
import { execFile } from 'node:child_process';
import { mkdtemp, rm } from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';
import { fileURLToPath } from 'node:url';
import { promisify } from 'node:util';

const run = promisify(execFile);
const here = path.dirname(fileURLToPath(import.meta.url));
const script = (name) => path.join(here, '..', name);
const node = (args) => run(process.execPath, args).then((r) => ({ code: 0, ...r }), (e) => ({ code: e.code, stdout: e.stdout, stderr: e.stderr }));

test('check_cli_exits_1_on_findings_and_2_on_bad_use', async () => {
  const empty = await mkdtemp(path.join(os.tmpdir(), 'lab-cli-'));
  try {
    const found = await node([script('check.mjs'), empty]);
    assert.equal(found.code, 1);
    assert.match(found.stdout, /ERROR run\.json is missing/);
    assert.match(found.stdout, /0 runs checked, 1 errors/);
    assert.equal((await node([script('check.mjs'), '--nope'])).code, 2);
    assert.equal((await node([script('check.mjs'), empty, '--require-functions'])).stdout.includes('android/functions.jsonl is missing'), true);
  } finally {
    await rm(empty, { recursive: true, force: true });
  }
});

test('run_cli_dry_run_prints_the_adb_commands_and_runs_nothing', async () => {
  const res = await node([script('run.mjs'), '--scenario', 'CS-05', '--dry-run', '--package', 'a.b']);
  assert.equal(res.code, 0);
  assert.match(res.stdout, /^6 runs into /);
  assert.match(res.stdout, /adb shell pm clear a\.b/);
  assert.match(res.stdout, /adb shell am start -W -n a\.b\/\.MainActivity --es run CS-05-quick-r1 --es control http:\/\/10\.0\.2\.2:18091/);
});

test('run_cli_exits_2_with_the_usage_on_bad_arguments', async () => {
  const res = await node([script('run.mjs'), '--mode', 'quick']);
  assert.equal(res.code, 2);
  assert.match(res.stderr, /USAGE: give exactly one of --scenario and --all/);
  assert.match(res.stderr, /usage: node run\.mjs/);
  assert.equal((await node([script('run.mjs'), '--scenario', 'CS-99', '--dry-run'])).code, 2);
});
