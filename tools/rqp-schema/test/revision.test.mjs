// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors
//
// The policy line of the page_revision preimage (manifest-patch.md section 1.3, step 4; H-55).
// Exhaustive checks over the value space of policy and limits: the line is absent only when
// both members are absent, and different manifest values always give different preimages.

import assert from 'node:assert/strict';
import { test } from 'node:test';
import { contentHash, pageRevision, pageRevisionPreimage, policyLimitsText, validators } from './helpers.mjs';

const T = 't1.86FtOeBe__KDLc87s9j68x63RC6DK9AYpEkvnbgQ_LM';
const BLOCKS = [
  { id: 'p', format: 'html', sha256: contentHash('a') },
  { id: 'pl', format: 'json', sha256: contentHash('{}') },
];
const { manifest: validateManifest } = validators();

/** Each member absent or one of two values (the range ends where there is a range). */
const MODE = [undefined, 'private', 'public'];
const TTL = [undefined, 0, 31536000];
const OFFLINE = [undefined, false, true];
const MAX = [undefined, 1024, 5242880];

/** Every combination: policy absent or an object with any subset of members; the same for limits. */
function* combinations() {
  const policies = [undefined];
  for (const mode of MODE) for (const ttl of TTL) for (const offline of OFFLINE) {
    const p = {};
    if (mode !== undefined) p.mode = mode;
    if (ttl !== undefined) p.ttl_seconds = ttl;
    if (offline !== undefined) p.offline = offline;
    policies.push(p);
  }
  const limits = [undefined];
  for (const max of MAX) limits.push(max === undefined ? {} : { max_patch_bytes: max });
  for (const policy of policies) for (const limit of limits) {
    const m = {};
    if (policy !== undefined) m.policy = policy;
    if (limit !== undefined) m.limits = limit;
    yield m;
  }
}

// H-103: no template revision and no block list in the manifest; the client computes them.
const manifestOf = (m) => ({
  protocol: 'rqp/1',
  origin: 'https://example.com',
  path: '/',
  page_revision: pageRevision(T, BLOCKS, m),
  ...m,
});

test('every combination of policy and limits is schema-valid and has its own preimage (injective)', () => {
  const seen = new Map();
  let n = 0;
  for (const m of combinations()) {
    n += 1;
    assert.equal(validateManifest(manifestOf(m)), true, JSON.stringify(m));
    const p = pageRevisionPreimage(T, BLOCKS, m);
    assert.equal(seen.has(p), false, `${JSON.stringify(m)} and ${seen.get(p)}`);
    seen.set(p, JSON.stringify(m));
  }
  // policy: absent or 3 x 3 x 3 member choices; limits: absent, {} or one of 2 values.
  assert.equal(n, 28 * 4);
});

test('the policy line is absent only when both policy and limits are absent', () => {
  for (const m of combinations()) {
    const lines = pageRevisionPreimage(T, BLOCKS, m).split('\n');
    const hasLine = lines[2].startsWith('PL\t');
    assert.equal(hasLine, Object.hasOwn(m, 'policy') || Object.hasOwn(m, 'limits'), JSON.stringify(m));
  }
});

test('without policy and limits the preimage is the earlier form, so earlier revisions stay the same', () => {
  const earlier = `rqp-r1\n${T}\n` + BLOCKS.map((b) => `${b.id}\t${b.format}\t${b.sha256}\n`).join('');
  assert.equal(pageRevisionPreimage(T, BLOCKS, {}), earlier);
  assert.equal(pageRevisionPreimage(T, BLOCKS, { protocol: 'rqp/1', origin: 'https://example.com' }), earlier);
});

test('the policy line is the third line, before the block lines, and has no tab or line feed in its text', () => {
  const m = { policy: { mode: 'public', ttl_seconds: 5, offline: true }, limits: { max_patch_bytes: 2048 } };
  const lines = pageRevisionPreimage(T, BLOCKS, m).split('\n');
  assert.deepEqual(lines.slice(0, 3), [
    'rqp-r1',
    T,
    'PL\tpolicy;mode=public;ttl_seconds=5;offline=true;limits;max_patch_bytes=2048',
  ]);
  assert.equal(lines[3], `p\thtml\t${BLOCKS[0].sha256}`);
  for (const x of combinations()) assert.doesNotMatch(policyLimitsText(x), /[\t\n]/);
});

test('the marker PL can never be a block id (M-15), so the policy line never looks like a block line', () => {
  const blockId = /^[a-z0-9][a-z0-9_-]{0,63}$/;
  assert.equal(blockId.test('PL'), false);
});

test('an empty object, an object with default values and no object are three different revisions', () => {
  const absent = pageRevision(T, BLOCKS, {});
  const empty = pageRevision(T, BLOCKS, { policy: {} });
  const defaults = pageRevision(T, BLOCKS, { policy: { mode: 'private', ttl_seconds: 0, offline: false } });
  assert.equal(new Set([absent, empty, defaults]).size, 3);
  assert.equal(policyLimitsText({ policy: {} }), 'policy');
  assert.equal(policyLimitsText({ limits: {} }), 'limits');
  assert.equal(policyLimitsText({ policy: {}, limits: {} }), 'policy;limits');
});

test('member order and white space of the JSON text do not change the policy line', () => {
  const a = JSON.parse('{"limits":{"max_patch_bytes":4096},"policy":{"offline":true,"ttl_seconds":60,"mode":"public"}}');
  const b = JSON.parse('{ "policy" : { "mode" : "public" , "ttl_seconds" : 60 , "offline" : true } , "limits" : { "max_patch_bytes" : 4096 } }');
  assert.equal(policyLimitsText(a), 'policy;mode=public;ttl_seconds=60;offline=true;limits;max_patch_bytes=4096');
  assert.equal(pageRevision(T, BLOCKS, a), pageRevision(T, BLOCKS, b));
});

test('integers are written as decimal digits without leading zeros', () => {
  assert.equal(policyLimitsText({ policy: { ttl_seconds: 0 } }), 'policy;ttl_seconds=0');
  assert.equal(policyLimitsText({ policy: { ttl_seconds: 31536000 } }), 'policy;ttl_seconds=31536000');
  assert.equal(policyLimitsText({ limits: { max_patch_bytes: 1024 } }), 'limits;max_patch_bytes=1024');
});
