// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors
//
// The field rules of DL-226 in the canonical client model: (a) a `sonic-etag-key` or `Connection`
// value never takes `Content-Type` or a protected field out of the page fields, (b) a field line
// with a bad name or value is no page field. The head test also applies to a 304 (CL-13 item 2).

import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { test } from 'node:test';
import {
  NOT_PAGE_FIELDS, PROTECTED_FIELD_PREFIX, PROTECTED_FIELDS, hasControlByte, isForbiddenConnectionToken, isForbiddenEtagKey,
  isLegacyResponse, isProtectedField, isSafeHead, isToken, isValidFieldLine, mergePageFields, pageFields,
} from '../src/client-canonical.mjs';
import { run } from '../src/client-model.mjs';
import { ENV, ROOT } from './client-helpers.mjs';

const RUST = readFileSync(join(ROOT, 'crates/rivqen-proto-legacy/src/fields.rs'), 'utf8');

/** The string literals of `pub const NAME: ... = [ ... ];` in fields.rs, with the named constants resolved. */
function rustList(name) {
  const m = new RegExp(`pub const ${name}: [^=]*= \\[([\\s\\S]*?)\\];`).exec(RUST);
  assert.ok(m, `${name} not found in fields.rs`);
  const consts = { CONNECTION: 'connection', SONIC_ETAG_KEY: 'sonic-etag-key', TEMPLATE_TAG: 'template-tag', TEMPLATE_CHANGE: 'template-change', CACHE_OFFLINE: 'cache-offline' };
  return m[1].split(',').map((x) => x.trim()).filter((x) => x !== '').map((x) => (x.startsWith('"') ? x.slice(1, -1) : consts[x]));
}

const HTML = ['content-type', 'text/html; charset=utf-8'];
const LEGACY = [HTML, ['etag', 'e1'], ['template-tag', 't1'], ['template-change', 'true'], ['cache-offline', 'true']];

test('the protected and not-page lists equal the lists of the Rust codec', () => {
  const lower = (list) => list.map((n) => n.toLowerCase()).sort();
  assert.deepEqual(lower(NOT_PAGE_FIELDS), lower(rustList('NOT_PAGE_FIELDS')));
  assert.deepEqual(lower(PROTECTED_FIELDS), lower(rustList('PROTECTED_FIELDS')));
  const prefix = /pub const PROTECTED_FIELD_PREFIX: &str = "([^"]*)";/.exec(RUST);
  assert.equal(PROTECTED_FIELD_PREFIX, prefix?.[1].toLowerCase());
});

test('a protected field is a listed name or a cross-origin name, in any letter case', () => {
  for (const name of ['Content-Security-Policy', 'X-FRAME-OPTIONS', 'Cross-Origin-Opener-Policy', 'CROSS-ORIGIN-x']) assert.equal(isProtectedField(name), true, name);
  for (const name of ['Cross-Origin', 'x-cross-origin-a', 'content-type', '']) assert.equal(isProtectedField(name), false, name);
});

test('token and control byte tests follow RFC 9110', () => {
  assert.equal(isToken("a-B_1!#$%&'*+.^`|~"), true);
  for (const bad of ['', 'a b', 'a:b', 'a,b', 'aé', 'a(']) assert.equal(isToken(bad), false, JSON.stringify(bad));
  assert.equal(hasControlByte('a\tb c\u0080'), false);
  for (const bad of ['\u0000', '\u0008', '\n', '\r', '\u001f', '\u007f']) assert.equal(hasControlByte(`a${bad}b`), true, JSON.stringify(bad));
  assert.equal(isValidFieldLine(['x-a', 'v\tw']), true);
  assert.equal(isValidFieldLine(['x a', 'v']), false);
  assert.equal(isValidFieldLine(['x-a', 'v\r\nx']), false);
});

test('a sonic-etag-key that names a forbidden field is forbidden; etag and other names are allowed', () => {
  for (const name of ['Content-Type', 'content-security-policy', 'cross-origin-resource-policy', 'Set-Cookie', 'vary', 'sonic-etag-key', 'template-tag', 'cache-offline', 'isReload', 'ISRELOAD', 'a b', 'a,b']) {
    assert.equal(isForbiddenEtagKey(name), true, name);
  }
  for (const name of ['etag', 'x-etag', 'Etag']) assert.equal(isForbiddenEtagKey(name), false, name);
});

test('a Connection token is forbidden only when it names Content-Type or a protected field', () => {
  for (const t of ['content-type', 'Content-Security-Policy', 'cross-origin-opener-policy']) assert.equal(isForbiddenConnectionToken(t), true, t);
  for (const t of ['close', 'x-hop', 'etag', 'set-cookie']) assert.equal(isForbiddenConnectionToken(t), false, t);
});

test('the head test rejects a forbidden sonic-etag-key on any line, a forbidden Connection token and an invalid line', () => {
  assert.equal(isSafeHead([...LEGACY]), true);
  assert.equal(isSafeHead([['sonic-etag-key', 'etag']]), true);
  assert.equal(isSafeHead([['sonic-etag-key', '']]), true);
  assert.equal(isSafeHead([['sonic-etag-key', 'x-tag'], ['Sonic-Etag-Key', 'Content-Security-Policy']]), false);
  assert.equal(isSafeHead([['connection', 'close, X-Frame-Options']]), false);
  assert.equal(isSafeHead([['Connection', 'close'], ['Connection', ' content-type ']]), false);
  assert.equal(isSafeHead([['connection', 'close, x-hop']]), true);
  assert.equal(isSafeHead([['x a', 'v']]), false);
  assert.equal(isSafeHead([['x-a', 'v\u0001']]), false);
});

test('a legacy response with a failing head is not a legacy response', () => {
  assert.equal(isLegacyResponse(LEGACY), true);
  assert.equal(isLegacyResponse([...LEGACY, ['connection', 'content-security-policy']]), false);
  assert.equal(isLegacyResponse([...LEGACY.filter(([n]) => n !== 'etag'), ['sonic-etag-key', 'content-security-policy'], ['content-security-policy', 'e1']]), false);
  assert.equal(isLegacyResponse([...LEGACY, ['x-a', 'v\u007f']]), false);
});

test('page fields keep Content-Type and protected fields whatever sonic-etag-key and Connection say', () => {
  const h = [HTML, ['Content-Security-Policy', "default-src 'self'"], ['Cross-Origin-Opener-Policy', 'same-origin'], ['sonic-etag-key', 'Content-Security-Policy'], ['connection', 'content-type, Cross-Origin-Opener-Policy, x-hop'], ['x-hop', '1'], ['x-keep', '2']];
  assert.deepEqual(pageFields(h).map(([n]) => n), ['content-security-policy', 'content-type', 'cross-origin-opener-policy', 'x-keep']);
});

test('page fields still drop the etag field and the Connection names', () => {
  const h = [HTML, ['sonic-etag-key', 'x-tag'], ['x-tag', 'e'], ['connection', 'X-Hop'], ['x-hop', '1'], ['etag', 'other'], ['x-keep', '2']];
  assert.deepEqual(pageFields(h).map(([n]) => n), ['content-type', 'etag', 'x-keep']);
});

test('page fields drop a line with a bad name or a control byte in the value, and keep HTAB', () => {
  const h = [HTML, ['bad name', 'v'], ['x-ctl', 'a\u0001b'], ['x-tab', 'a\tb'], ['x-keep', 'ok']];
  assert.deepEqual(pageFields(h), [['content-type', 'text/html; charset=utf-8'], ['x-keep', 'ok'], ['x-tab', 'a\tb']]);
});

test('the update of a 304 replaces the stored lines of its names and drops stored lines that fail rule (b)', () => {
  const stored = [['content-type', 'text/html'], ['x-old', '1'], ['x-ctl', 'a\u0000'], ['bad name', 'v'], ['x-new', 'old']];
  const merged = mergePageFields(stored, [['x-new', 'fresh'], ['cache-offline', 'true']]);
  assert.deepEqual(merged, [['content-type', 'text/html'], ['x-old', '1'], ['x-new', 'fresh']]);
});

/** A visible entry, then a 304 whose head has `field`; a second session for the key. */
function notModified(mode, field) {
  const session = [{ do: 'create_session' }, { do: 'bind_client' }, { do: 'client_ready' }, { do: 'request_resource' }, { do: 'wait', for: 'proxy.response_end' }, { do: 'settle', ms: 2500 }, { do: 'request_resource' }];
  const c = {
    mode, server_profile: 'legacy-canonical', responses: { r1: { page: 'a' } }, cache_before: [{ page: 'a' }],
    schedule: [{ case: 'r1' }, ...(field ? [{ do: 'set_field', name: field[0], value: field[1] }] : []), ...session, { do: 'get_diff' }, { do: 'snapshot_cache' }, { do: 'destroy' }],
  };
  const r = run('legacy-canonical', mode, c, ENV);
  assert.equal(r.ok, true, r.ok ? '' : r.error.message);
  const all = (key) => r.client.phases.flatMap((p) => p[key]);
  return { codes: all('results').map(({ src_code: s, final_code: f }) => [s, f]), snapshot: all('snapshots').at(-1).entries, status: all('requests')[0].response.status, actions: all('actions').map((a) => a.do) };
}

for (const mode of ['quick', 'standard']) {
  test(`a 304 with a good head is not modified: the entry stays, codes 304 / 304 (${mode})`, () => {
    const r = notModified(mode, ['x-extra', 'v']);
    assert.equal(r.status, 304);
    assert.deepEqual(r.codes, [[304, 304]]);
    assert.equal(r.snapshot.length, 1);
  });

  for (const field of [['connection', 'content-security-policy'], ['sonic-etag-key', 'Cross-Origin-Opener-Policy'], ['x-bad', 'a\u0001']]) {
    test(`a 304 whose head has ${field[0]} ${JSON.stringify(field[1])} is a normal load: the entry is removed, codes -1 / -1 (${mode})`, () => {
      const r = notModified(mode, field);
      assert.equal(r.status, 304);
      assert.deepEqual(r.codes, [[-1, -1]]);
      assert.deepEqual(r.snapshot, []);
      assert.ok(r.actions.includes('load_url'));
    });
  }
}
