// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors
//
// The demo against the protocol fixtures: the conformance harness of
// tools/rqp-protocol runs the server fixtures (FX-RQ-P*) on handle() of src/rqp.js.
// P-35 (H-56) has level SHOULD; the demo implements it, so no fixture may fail.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';

const CHECK = fileURLToPath(new URL('../../../tools/rqp-protocol/check.mjs', import.meta.url));
const SERVER = fileURLToPath(new URL('../src/rqp.js', import.meta.url));

test('the demo passes all 57 server fixtures, also the SHOULD fixture P-35', () => {
  const out = execFileSync(process.execPath, [CHECK, '--server', SERVER], { encoding: 'utf8' });
  assert.match(out, /rqp-protocol check: 57 passed, 0 failed, 0 SHOULD failed/);
});
