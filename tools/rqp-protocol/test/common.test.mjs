// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors
//
// Shared values of src/common.mjs and src/pages.mjs: the policy line of the
// page revision (manifest-patch.md section 1.3, H-55), the members policy and
// limits, and the error codes of the reference.

import assert from 'node:assert/strict';
import { test } from 'node:test';
import { NONCE_BYTES, UnsupportedInput, canonicalUrl, deltaBinding, deltaBindingPreimage, isNonceText, membersOf, nonceText, pageRevision, policyLimitsText, sha256b64u } from '../src/common.mjs';
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

const R_A = 'r1.RdZd06nCaPfKHlfMbqXINPA1sGSJ58jvWVRWF94FDz8';
const R_B = 'r1.psbsc6whgs8ooESisNWp6E3PrQfkg4D69WmXAo12pbI';

const N = 'AAECAwQFBgcICQoLDA0ODw';
const N2 = 'EBESExQVFhcYGRobHB0eHw';

test('H-107, H-108: the binding preimage is "rqp-d1" LF U LF B LF N LF R\' LF and the binding is d1. and its SHA-256', () => {
  const p = deltaBindingPreimage('https://shop.example.com/catalog', R_A, N, R_B);
  assert.equal(p, `rqp-d1\nhttps://shop.example.com/catalog\n${R_A}\n${N}\n${R_B}\n`);
  assert.equal(deltaBinding('https://shop.example.com/catalog', R_A, N, R_B), `d1.${sha256b64u(p)}`);
  // The KAT value (spec/rqp/examples/kat/kat.json, member bindings).
  assert.equal(deltaBinding('https://shop.example.com/catalog', R_A, N, R_B), 'd1.orrx-_-Iflnnjs7DvvuOOdxW3FzbZpU_PTlR0jdWw54');
  // Each input changes the value; B and R' are not interchangeable.
  const values = new Set([
    deltaBinding('https://shop.example.com/catalog', R_A, N, R_B),
    deltaBinding('https://shop.example.com/catalog?x', R_A, N, R_B),
    deltaBinding('https://shop.example.com/catalog', R_B, N, R_A),
    deltaBinding('https://shop.example.com/catalog', R_A, N, R_A),
    deltaBinding('https://shop.example.com/catalog', R_A, N2, R_B),
  ]);
  assert.equal(values.size, 5);
});

test('H-108: the nonce text N is canonical base64url without padding of exactly 16 bytes', () => {
  assert.equal(NONCE_BYTES, 16);
  const bytes = Buffer.from([...Array(16).keys()]);
  assert.equal(nonceText(bytes), N);
  assert.equal(isNonceText(N), true);
  assert.equal(isNonceText(nonceText(Buffer.alloc(16, 0xff))), true);
  // 22 characters; the last one carries 2 bits, so only A, Q, g and w are canonical.
  for (const last of ['A', 'Q', 'g', 'w']) assert.equal(isNonceText(`${'A'.repeat(21)}${last}`), true, last);
  for (const bad of [`${'A'.repeat(21)}B`, `${N}==`, N.slice(0, -1), `${N}A`, 'AAECAwQFBgcICQoLDA0OD+', 'AAECAwQFBgcICQoLDA0OD/', null, undefined, 7, bytes, [N], { toString: () => N }]) {
    assert.equal(isNonceText(bad), false, String(bad));
  }
  for (const bad of [Buffer.alloc(15), Buffer.alloc(17), new Uint8Array(16), 'AAECAwQFBgcICQoLDA0ODw', null]) {
    assert.throws(() => nonceText(bad), TypeError);
  }
});

test('H-107, H-108: an input that would make the preimage ambiguous stops the reference with a TypeError', () => {
  const bad = [
    ['https://a.example/\nr1.x', R_A, N, R_B],
    [null, R_A, N, R_B],
    ['https://a.example/', 't1.RdZd06nCaPfKHlfMbqXINPA1sGSJ58jvWVRWF94FDz8', N, R_B],
    ['https://a.example/', R_A, N, `${R_B}\n`],
    ['https://a.example/', R_A, N, R_B.slice(0, -1)],
    ['https://a.example/', undefined, N, R_B],
    ['https://a.example/', R_A, N, 7],
    ['https://a.example/', R_A, `${N}\n`, R_B],
    ['https://a.example/', R_A, `${N}==`, R_B],
    ['https://a.example/', R_A, null, R_B],
    ['https://a.example/', R_A, R_B],
  ];
  for (const args of bad) assert.throws(() => deltaBindingPreimage(...args), TypeError, JSON.stringify(args));
});

test('H-107, H-108: each defective input of the preimage is named, and a revision has no prefix', () => {
  const failure = (args) => {
    try {
      deltaBindingPreimage(...args);
    } catch (e) {
      return e;
    }
    return null;
  };
  assert.match(failure([undefined, R_A, N, R_B]).message, /URL/);
  assert.match(failure(['https://a.example/', 7, N, R_B]).message, /base/);
  assert.match(failure(['https://a.example/', R_A, N, 7]).message, /next revision/);
  assert.match(failure(['https://a.example/', `x${R_A}`, N, R_B]).message, /base/);
  assert.match(failure(['https://a.example/', R_A, N, `x${R_B}`]).message, /next revision/);
  assert.match(failure(['https://a.example/', R_A, 'x', R_B]).message, /nonce/);
});

test('H-107: U is the WHATWG serialization without the fragment; it has no line feed', () => {
  assert.equal(canonicalUrl('https://SHOP.example.com:443/catalog?b=1&a=2#top'), 'https://shop.example.com/catalog?b=1&a=2');
  assert.equal(canonicalUrl('https://shop.example.com/a\nb?c\td'), 'https://shop.example.com/ab?cd');
  assert.equal(canonicalUrl('https://shop.example.com/%7e?q=%7E'), 'https://shop.example.com/%7e?q=%7E'); // no percent normalization
  assert.equal(canonicalUrl('https://shop.example.com'), 'https://shop.example.com/');
  assert.throws(() => canonicalUrl('not a url'), TypeError);
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
