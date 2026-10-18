// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors
//
// The demo against the protocol fixtures: the conformance harness of
// tools/rqp-protocol runs the server fixtures (FX-RQ-P*) on handle() of src/rqp.js.
// Some fixtures have level SHOULD (P-35, H-56); the demo implements them, so no fixture
// may fail, and no line may say FAIL or SHOULD FAIL.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';

const CHECK = fileURLToPath(new URL('../../../tools/rqp-protocol/check.mjs', import.meta.url));
const SERVER = fileURLToPath(new URL('../src/rqp.js', import.meta.url));

test('the demo passes all 77 server fixtures, also the SHOULD fixtures', () => {
  const run = spawnSync(process.execPath, [CHECK, '--server', SERVER], { encoding: 'utf8' });
  const out = `${run.stdout}${run.stderr}`;
  assert.equal(run.status, 0, out);
  assert.doesNotMatch(out, /^FAIL /m);
  assert.doesNotMatch(out, /SHOULD\] /);
  assert.match(out, /rqp-protocol check: 77 passed, 0 failed, 0 SHOULD failed/);
});
