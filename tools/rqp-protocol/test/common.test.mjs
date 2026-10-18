// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors
//
// Shared values of src/common.mjs and src/pages.mjs: the policy line of the
// page revision (manifest-patch.md section 1.3, H-55), the members policy and
// limits, and the error codes of the reference.

import assert from 'node:assert/strict';
import { test } from 'node:test';
import { UnsupportedInput, membersOf, pageRevision, policyLimitsText } from '../src/common.mjs';
import { UnknownPage, pageBytes } from '../src/pages.mjs';
import { handle } from '../src/server.mjs';
import { APP, firstVisit, page } from './helpers.mjs';

test('policy line: only the members that the manifest has, in the order of the table', () => {
  assert.equal(policyLimitsText({}), '');
  assert.equal(policyLimitsText({ limits: {} }), 'limits');
  assert.equal(policyLimitsText({ policy: {} }), 'policy');
  assert.equal(
    policyLimitsText({ limits: { max_patch_bytes: 0 }, policy: { offline: false, ttl_seconds: 0, mode: 'private' } }),
    'policy;mode=private;ttl_seconds=0;offline=false;limits;max_patch_bytes=0',
  );
});

test('policy line: a value outside the schema stops the reference with a TypeError', () => {
  const bad = [
    { policy: { mode: 'shared' } },
    { policy: { offline: 'yes' } },
    { policy: { ttl_seconds: -1 } },
    { policy: { ttl_seconds: 1.5 } },
    { limits: { max_patch_bytes: 2 ** 53 } },
  ];
  for (const members of bad) assert.throws(() => pageRevision('t1.x', [], members), TypeError, JSON.stringify(members));
});

test('membersOf: only policy and limits, also for null and undefined', () => {
  assert.deepEqual(membersOf(null), {});
  assert.deepEqual(membersOf(undefined), {});
  assert.deepEqual(membersOf({ policy: { mode: 'public' }, limits: {}, origin: 'x' }), { policy: { mode: 'public' }, limits: {} });
});

test('error codes: an unmodelled input and an unknown page', () => {
  const request = { ...firstVisit(), headers: [...firstVisit().headers, ['if-match', '"x"']] };
  assert.throws(
    () => handle(request, APP, page('page-a')),
    (e) => e instanceof UnsupportedInput && e.name === 'UnsupportedInput' && e.code === 'RQP_REFERENCE_UNSUPPORTED',
  );
  assert.throws(() => pageBytes('page-none'), (e) => e instanceof UnknownPage && e.name === 'UnknownPage' && e.code === 'RQP_FIXTURE_PAGE');
});
