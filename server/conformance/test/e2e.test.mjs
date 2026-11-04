// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors
//
// The e2e driver starts a server program with --command (DL-090).
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { dirname, resolve } from 'node:path';

const here = dirname(fileURLToPath(import.meta.url));
const run = resolve(here, '../e2e/run.mjs');
const harness = resolve(here, '../harness/harness.mjs');
const reference = resolve(here, '../../../tools/rqp-protocol/src/server.mjs');

function drive(args) {
  return spawnSync(process.execPath, [run, ...args], { encoding: 'utf8' });
}

test('--command starts a program and runs the runner against it', () => {
  const r = drive([
    '--command', process.execPath,
    '--arg', harness, '--arg', '--module', '--arg', reference, '--arg', '--suite', '--arg', 'rqp',
    '--arg', '--port', '--arg', '0', '--arg', '--conformance',
    '--suite', 'rqp', '--', '--only', 'FX-RQ-P01',
  ]);
  assert.equal(r.status, 0, r.stderr);
  assert.match(r.stdout, /PASS FX-RQ-P01/);
});

test('--module and --command together is a usage error', () => {
  const r = drive(['--module', reference, '--command', process.execPath, '--suite', 'rqp']);
  assert.equal(r.status, 2);
  assert.match(r.stderr, /need \(--module/);
});

test('--arg without --command is a usage error', () => {
  const r = drive(['--module', reference, '--arg', 'x', '--suite', 'rqp']);
  assert.equal(r.status, 2);
  assert.match(r.stderr, /--arg needs --command/);
});

test('a program that cannot start is a harness error', () => {
  const r = drive(['--command', resolve(here, 'no-such-program'), '--suite', 'rqp']);
  assert.equal(r.status, 2);
  assert.match(r.stderr, /cannot start/);
});

test('--start-timeout-ms is checked and used', () => {
  const bad = drive(['--module', reference, '--suite', 'rqp', '--start-timeout-ms', '0']);
  assert.equal(bad.status, 2);
  assert.match(bad.stderr, /--start-timeout-ms must be/);
  // A program that never prints the line: the driver gives up after the limit.
  const slow = drive([
    '--command', process.execPath, '--arg', '-e', '--arg', 'setTimeout(() => {}, 5000)',
    '--suite', 'rqp', '--start-timeout-ms', '200',
  ]);
  assert.equal(slow.status, 2);
  assert.match(slow.stderr, /in 200 ms/);
});
