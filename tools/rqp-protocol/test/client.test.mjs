// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors
//
// Reference client step: each error code of manifest-patch.md section 5, the
// check order of diff-engine.md section 3, the manifest checks of sections 1.7
// and 1.8, and the attribute rule of section 4.2.

import assert from 'node:assert/strict';
import { test } from 'node:test';
import { LIMITS, pageRevision, sha256b64u } from '../src/common.mjs';
import {
  cacheDirectives,
  checkFull,
  clientStep,
  deltaPartition,
  handleResponse,
  hasUserinfo,
  inAllowlist,
  partitionFor,
  redirected,
  requestHeaders,
  rqpAllowed,
  setsCookie,
  snapshotOf,
  validAllowlistEntry,
  verifyDelta,
  withoutCredentials,
} from '../src/client.mjs';
import { attributeRejected, attributesOf, passesAttributeRule } from '../src/markup.mjs';
import { fullDocument, handle, manifestOf } from '../src/server.mjs';
import { APP, TARGET, blockPage, clientContext, deltaRequest, firstVisit, page, revisions } from './helpers.mjs';

const snapA = () => snapshotOf(page('page-a'), {}, TARGET);
const ctx = (fields = {}) => clientContext({ navigation: 'delta', snapshot: snapA(), ...fields });
const DELTA = [['content-type', 'application/vnd.rivqen.delta+json']];
const HTML = [['content-type', 'text/html; charset=utf-8'], ['rq-version', '1']];

/** A consistent envelope from the snapshot of page-a: ops = [[blockId, content]]. */
function envelope(ops, change = {}) {
  const a = revisions('page-a');
  const hashes = new Map(ops.map(([id, c]) => [id, sha256b64u(Buffer.from(c, 'utf8'))]));
  const next = a.blocks.map((b) => ({ ...b, sha256: hashes.get(b.id) ?? b.sha256 }));
  return {
    type: 'rivqen.patch',
    protocol: 'rqp/1',
    base_revision: a.page_revision,
    next_revision: pageRevision(a.template_revision, next),
    template_revision: a.template_revision,
    operations: ops.map(([id, content]) => ({
      op: 'replace_block',
      block_id: id,
      format: a.blocks.find((b) => b.id === id).format,
      content,
      sha256: hashes.get(id),
    })),
    ...change,
  };
}

const verify = (env, snap = snapA()) => verifyDelta(snap, Buffer.from(typeof env === 'string' ? env : JSON.stringify(env), 'utf8'), true);
const code = (env, snap) => verify(env, snap).code ?? null;

// Delta: diff-engine section 3 order.
test('a consistent delta is applied: new revision and block contents', () => {
  const r = handleResponse(ctx(), { status: 200, headers: DELTA, body: Buffer.from(JSON.stringify(envelope([['price', '130.00']]))) });
  assert.equal(r.outcome, 'patched');
  assert.equal(r.snapshot.page_revision, revisions('page-b').page_revision);
  assert.equal(r.snapshot.blocks.find((b) => b.id === 'price').content, '130.00');
  assert.equal(r.snapshot.document_sha256, sha256b64u(page('page-b')));
});

test('PATCH_INVALID: an unsolicited delta (no snapshot, first visit, fallback load)', () => {
  const body = Buffer.from(JSON.stringify(envelope([['price', '130.00']])));
  const noSnap = handleResponse(ctx({ snapshot: null, navigation: 'first-visit' }), { status: 200, headers: DELTA, body });
  assert.deepEqual([noSnap.error, noSnap.next], ['PATCH_INVALID', 'fallback-load']);
  const first = handleResponse(ctx({ navigation: 'first-visit' }), { status: 200, headers: DELTA, body });
  assert.equal(first.error, 'PATCH_INVALID');
  const fallback = handleResponse(ctx({ navigation: 'fallback' }), { status: 200, headers: DELTA, body });
  assert.deepEqual([fallback.error, fallback.next], ['PATCH_INVALID', 'webview-load']);
});

test('PATCH_TOO_LARGE: an envelope over the client limit, before any JSON check', () => {
  const big = Buffer.alloc(LIMITS.maxPatchBytes + 1, 0x20);
  assert.equal(verifyDelta(snapA(), big, true).code, 'PATCH_TOO_LARGE');
  const small = { ...snapA(), members: { limits: { max_patch_bytes: 1024 } } };
  assert.equal(verifyDelta(small, Buffer.alloc(1025, 0x20), true).code, 'PATCH_TOO_LARGE');
  assert.equal(verifyDelta(small, Buffer.alloc(1024, 0x20), true).code, 'PATCH_INVALID');
});

test('PATCH_TOO_LARGE: content over maxBlockBytes, with a client limit that lets the envelope pass', () => {
  const wide = 4 * LIMITS.maxBlockBytes;
  // ASCII over 1 MiB: the schema maxLength (code points) fails first.
  const over = Buffer.from(JSON.stringify(envelope([['price', 'x'.repeat(LIMITS.maxBlockBytes + 1)]])));
  assert.equal(verifyDelta(snapA(), over, true, wide).code, 'PATCH_INVALID');
  // Multi-byte characters pass maxLength: 524289 two-byte characters are 1048578 UTF-8 bytes.
  const utf8 = Buffer.from(JSON.stringify(envelope([['price', 'é'.repeat(LIMITS.maxBlockBytes / 2 + 1)]])));
  assert.equal(verifyDelta(snapA(), utf8, true, wide).code, 'PATCH_TOO_LARGE');
  const at = Buffer.from(JSON.stringify(envelope([['price', 'x'.repeat(LIMITS.maxBlockBytes)]])));
  assert.equal(verifyDelta(snapA(), at, true, wide).code, undefined);
});

test('PATCH_INVALID: JSON rules, schema and operation rules', () => {
  const ok = envelope([['price', '130.00']]);
  const text = JSON.stringify(ok);
  assert.equal(code(text.replace('{', '{"type":"rivqen.patch",')), 'PATCH_INVALID'); // duplicate member
  assert.equal(code(text.slice(0, -1)), 'PATCH_INVALID'); // syntax
  assert.equal(code({ ...ok, page_id: 'x' }), 'PATCH_INVALID'); // schema: unknown member
  assert.equal(code({ ...ok, operations: [] }), 'PATCH_INVALID'); // schema: minItems
  assert.equal(code({ ...ok, operations: [{ ...ok.operations[0], op: 'remove_block' }] }), 'PATCH_INVALID');
  assert.equal(code({ ...ok, operations: [{ ...ok.operations[0], block_id: 'nope' }] }), 'PATCH_INVALID'); // rule 1
  assert.equal(code({ ...ok, operations: [ok.operations[0], ok.operations[0]] }), 'PATCH_INVALID'); // rule 2
  assert.equal(code({ ...ok, operations: [{ ...ok.operations[0], format: 'json' }] }), 'PATCH_INVALID'); // rule 4
  const two = envelope([['title', 'S'], ['price', '1']]);
  assert.equal(code({ ...two, operations: [...two.operations].reverse() }), 'PATCH_INVALID'); // rule 3
  assert.equal(code(two), null);
});

test('TEMPLATE_MISMATCH comes before PATCH_BASE_MISMATCH', () => {
  const ok = envelope([['price', '130.00']]);
  const t2 = revisions('page-t2').template_revision;
  assert.equal(code({ ...ok, template_revision: t2, base_revision: revisions('page-b').page_revision }), 'TEMPLATE_MISMATCH');
  assert.equal(code({ ...ok, base_revision: revisions('page-b').page_revision }), 'PATCH_BASE_MISMATCH');
});

test('INTEGRITY_ERROR: an operation hash that is not the hash of its content (step 4)', () => {
  const ok = envelope([['price', '130.00']]);
  const wrong = { ...ok, operations: [{ ...ok.operations[0], sha256: sha256b64u('131.00') }] };
  assert.equal(code(wrong), 'INTEGRITY_ERROR');
});

test('INTEGRITY_ERROR: next_revision does not match the blocks (step 7), also after an omitted block', () => {
  const omitted = envelope([['title', 'Shop']], { next_revision: revisions('page-b').page_revision });
  assert.equal(code(omitted), 'INTEGRITY_ERROR');
});

test('INTEGRITY_ERROR: next_revision with another policy line than the snapshot (H-55)', () => {
  const snap = snapshotOf(page('page-a'), { policy: { ttl_seconds: 60 } });
  const a = revisions('page-a');
  const content = '130.00';
  const hash = sha256b64u(content);
  const blocks = a.blocks.map((b) => (b.id === 'price' ? { ...b, sha256: hash } : b));
  const env = (ttl) => ({
    ...envelope([['price', content]]),
    base_revision: snap.page_revision,
    next_revision: pageRevision(a.template_revision, blocks, { policy: { ttl_seconds: ttl } }),
  });
  assert.equal(code(env(61), snap), 'INTEGRITY_ERROR');
  assert.equal(code(env(60), snap), null);
});

test('PATCH_CONTENT_REJECTED: markup analysis of the patched document (step 8)', () => {
  for (const [id, content] of [['price', '<script>alert(1)</script>'], ['price', '1</p><p>2'], ['title', '</title>'], ['cart', ''], ['cart', '{']]) {
    assert.equal(code(envelope([[id, content]])), 'PATCH_CONTENT_REJECTED', `${id}: ${content}`);
  }
});

test('PATCH_CONTENT_REJECTED: the attribute rule (step 9) after a valid analysis', () => {
  assert.equal(code(envelope([['price', '<a onclick="x">y</a>']])), 'PATCH_CONTENT_REJECTED');
  assert.equal(code(envelope([['price', '<a href="/p?a=1&amp;b=2">y</a>']])), null);
});

test('PATCH_CONTENT_REJECTED sets the metric and asks for the fallback load', () => {
  const body = Buffer.from(JSON.stringify(envelope([['price', '<a onclick="x">y</a>']])));
  const r = handleResponse(ctx(), { status: 200, headers: DELTA, body });
  assert.deepEqual([r.outcome, r.error, r.next, r.metric, r.snapshot], ['rejected', 'PATCH_CONTENT_REJECTED', 'fallback-load', true, null]);
});

test('PATCH_TOO_LARGE: a patched document over maxDocumentBytes', () => {
  // Snapshot of a document just under the limit; the patch adds enough bytes to pass it.
  const filler = 'a'.repeat(LIMITS.maxDocumentBytes - 200);
  const doc = Buffer.from(`<!DOCTYPE html><html><head><title>t</title></head><body><p>${filler}</p><div data-rq-block="x">1</div></body></html>`);
  const snap = snapshotOf(doc);
  const content = 'b'.repeat(300);
  const a = snap.analysis;
  const blocks = [{ ...a.blocks[0], sha256: sha256b64u(content) }];
  const env = {
    type: 'rivqen.patch',
    protocol: 'rqp/1',
    base_revision: snap.page_revision,
    next_revision: pageRevision(a.template_revision, blocks),
    template_revision: a.template_revision,
    operations: [{ op: 'replace_block', block_id: 'x', format: 'html', content, sha256: blocks[0].sha256 }],
  };
  assert.equal(verifyDelta(snap, Buffer.from(JSON.stringify(env)), true).code, 'PATCH_TOO_LARGE');
});

// Full document: manifest-patch sections 1.7 and 1.8.
const fullOf = (id, members = {}, url = TARGET) => handle({ ...firstVisit(), target: url }, APP, page(id), members).body;

test('a full document with Rq-Version 1 is stored with the manifest revisions', () => {
  const r = handleResponse(ctx({ snapshot: null, navigation: 'first-visit' }), { status: 200, headers: HTML, body: fullOf('page-a') });
  assert.equal(r.outcome, 'stored');
  assert.equal(r.snapshot.page_revision, revisions('page-a').page_revision);
  assert.equal(r.snapshot.document_sha256, sha256b64u(page('page-a')));
});

test('without Rq-Version 1 the manifest is not used and there is no error', () => {
  for (const headers of [[['content-type', 'text/html']], [['content-type', 'text/html'], ['rq-version', '2']], [['content-type', 'text/html'], ['rq-version', '"1"']]]) {
    const r = handleResponse(ctx({ snapshot: null }), { status: 200, headers, body: fullOf('page-a') });
    assert.deepEqual([r.outcome, r.error], ['normal', null]);
  }
});

test('MANIFEST_INVALID: each failed check of sections 1.7 and 1.8', () => {
  const a = revisions('page-a');
  const p = page('page-a');
  const at = p.indexOf('</head>');
  const build = (manifest, type = 'application/rivqen-manifest+json', where = at) =>
    Buffer.concat([p.subarray(0, where), Buffer.from(`<script type="${type}">${typeof manifest === 'string' ? manifest : JSON.stringify(manifest)}</script>`), p.subarray(where)]);
  const m = manifestOf(a, TARGET);
  const reasons = (body) => checkFull({ url: TARGET }, body).error ?? null;
  assert.equal(reasons(build(m)), null);
  assert.equal(reasons(p), 'count'); // no manifest element
  assert.equal(reasons(build(m, 'application/rıvqen-manifest+json')), 'count');
  assert.equal(reasons(build(m, 'application/rivqen-manifest+json ')), 'count'); // no trimming
  assert.equal(reasons(build(m, undefined, p.indexOf('</body>'))), 'position');
  const element = build(m).subarray(at, build(m).length - (p.length - at));
  assert.equal(reasons(Buffer.concat([p.subarray(0, at), element, element, p.subarray(at)])), 'count'); // two elements
  assert.equal(reasons(build(JSON.stringify(m).replace('{', '{"protocol":"rqp/1",'))), 'json-duplicate');
  assert.equal(reasons(build({ ...m, extra: 1 })), 'schema');
  assert.equal(reasons(build({ ...m, blocks: [m.blocks[0], m.blocks[0]] })), 'ids');
  assert.equal(reasons(build({ ...m, origin: 'https://other.example.com' })), 'url');
  assert.equal(reasons(build({ ...m, path: '/other' })), 'url');
  const bBlocks = revisions('page-b').blocks.map(({ id, format, sha256 }) => ({ id, format, sha256 }));
  assert.equal(reasons(build({ ...m, blocks: bBlocks, page_revision: revisions('page-b').page_revision })), 'analysis');
  assert.equal(reasons(build({ ...m, policy: { ttl_seconds: 60 } })), 'revision'); // H-55: policy without a new r1
  assert.equal(reasons(build(`${JSON.stringify(m)}${' '.repeat(LIMITS.maxManifestBytes)}`)), 'size');
});

test('a manifest text of exactly maxManifestBytes passes the size check; one byte more fails it', () => {
  const p = page('page-a');
  const at = p.indexOf('</head>');
  const m = JSON.stringify(manifestOf(revisions('page-a'), TARGET));
  const build = (size) => Buffer.concat([
    p.subarray(0, at),
    Buffer.from(`<script type="application/rivqen-manifest+json">${m}${' '.repeat(size - m.length)}</script>`),
    p.subarray(at),
  ]);
  assert.equal(checkFull({ url: TARGET }, build(LIMITS.maxManifestBytes)).error, undefined);
  assert.equal(checkFull({ url: TARGET }, build(LIMITS.maxManifestBytes + 1)).error, 'size');
});

test('MANIFEST_INVALID: an upper-case manifest type is accepted (ASCII case-insensitive, H-28)', () => {
  const body = Buffer.from(fullOf('page-a').toString('utf8').replace('application/rivqen-manifest+json', 'APPLICATION/Rivqen-Manifest+JSON'));
  assert.equal(checkFull({ url: TARGET }, body).error, undefined);
});

test('a manifest with policy and limits and the matching page revision is accepted (H-55)', () => {
  const members = { policy: { mode: 'public', ttl_seconds: 0 }, limits: { max_patch_bytes: 2048 } };
  const r = handleResponse(ctx({ snapshot: null }), { status: 200, headers: HTML, body: fullOf('page-a', members) });
  assert.equal(r.outcome, 'stored');
  assert.deepEqual(r.snapshot.limits, members.limits);
});

test('the manifest is bound to the response URL: an http URL needs a debug build and the app flag', () => {
  const http = 'http://shop.example.com/catalog';
  const body = fullOf('page-a', {}, http);
  const on = handleResponse(clientContext({ url: http, build: 'debug', allow_http: true }), { status: 200, headers: HTML, body });
  assert.equal(on.outcome, 'stored');
  for (const [build, flag] of [['release', true], ['debug', false]]) {
    const off = handleResponse(clientContext({ url: http, build, allow_http: flag }), { status: 200, headers: HTML, body });
    assert.equal(off.outcome, 'normal');
  }
});

test('304 keeps the snapshot', () => {
  assert.deepEqual(handleResponse(ctx(), { status: 304, headers: [], body: null }).next, 'keep');
});

// H-58: response binding.
const r1 = (id) => `"${revisions(id).page_revision}"`;
const on304 = (fields, etag) => handleResponse(ctx(fields), { status: 304, headers: etag === undefined ? [] : [['etag', etag]], body: null });

test('H-58: a 304 is kept only for a request that sent If-None-Match "R" and an ETag that matches R weakly', () => {
  for (const etag of [undefined, r1('page-a'), `W/${r1('page-a')}`, ` ${r1('page-a')} `]) {
    assert.deepEqual([on304({}, etag).outcome, on304({}, etag).next], ['not-modified', 'keep'], String(etag));
  }
  for (const etag of [r1('page-b'), `W/${r1('page-b')}`, `${r1('page-a')}, ${r1('page-a')}`, '*', 'r1.x', `w/${r1('page-a')}`]) {
    assert.deepEqual([on304({}, etag).outcome, on304({}, etag).next, on304({}, etag).error], ['normal', 'webview-load', null], etag);
  }
});

test('H-58: a 304 for a request without the RQP If-None-Match gives a normal load', () => {
  for (const fields of [{ navigation: 'fallback' }, { navigation: 'first-visit' }, { snapshot: null }]) {
    assert.deepEqual([on304(fields).outcome, on304(fields).next], ['normal', 'webview-load'], JSON.stringify(Object.keys(fields)));
  }
});

test('H-58: a 304 from another final URL gives a normal load; a fragment is not part of the key', () => {
  assert.equal(on304({ final_url: `${TARGET}?page=2` }).next, 'webview-load');
  assert.equal(on304({ final_url: 'https://shop.example.com:8443/catalog' }).next, 'webview-load');
  assert.equal(on304({ final_url: `${TARGET}#top` }).next, 'keep');
});

test('H-58: a delta from a final URL with another cache key is PATCH_INVALID', () => {
  const body = Buffer.from(JSON.stringify(envelope([['price', '130.00']])));
  for (const final of [`${TARGET}?page=2`, 'http://shop.example.com/catalog', 'https://other.example.com/catalog', 'https://shop.example.com/catalog/']) {
    const r = handleResponse(ctx({ final_url: final }), { status: 200, headers: DELTA, body });
    assert.deepEqual([r.error, r.next], ['PATCH_INVALID', 'fallback-load'], final);
  }
  assert.equal(handleResponse(ctx({ final_url: `${TARGET}#x` }), { status: 200, headers: DELTA, body }).outcome, 'patched');
});

test('H-58: a full document is checked against the final URL and stored under its cache key', () => {
  const final = 'https://shop.example.com/catalog?page=2';
  const ok = handleResponse(ctx({ snapshot: null, navigation: 'first-visit', final_url: final }), { status: 200, headers: HTML, body: fullOf('page-a', {}, final) });
  assert.deepEqual([ok.outcome, ok.snapshot.url], ['stored', final]);
  const moved = handleResponse(ctx({ snapshot: null, navigation: 'first-visit', final_url: 'https://shop.example.com/other' }), { status: 200, headers: HTML, body: fullOf('page-a') });
  assert.deepEqual([moved.outcome, moved.error], ['rejected', 'MANIFEST_INVALID']);
});

test('H-58: a release build does not use a manifest from an http final URL; a debug build with the flag does', () => {
  const http = 'http://shop.example.com/catalog';
  const body = fullOf('page-a', {}, http);
  const run = (build, flag) => handleResponse(ctx({ snapshot: null, navigation: 'first-visit', final_url: http, build, allow_http: flag }), { status: 200, headers: HTML, body });
  assert.deepEqual([run('release', true).outcome, run('release', true).error, run('release', true).snapshot], ['normal', null, null]);
  assert.equal(run('debug', false).outcome, 'normal');
  assert.deepEqual([run('debug', true).outcome, run('debug', true).snapshot.url], ['stored', http]);
});

// H-61: the public partition.
test('H-61: the public partition needs mode public, a public origin, no private, and public or no credentials', () => {
  const store = (fields, cacheControl, members = { policy: { mode: 'public' } }) => {
    const headers = cacheControl === null ? HTML : [...HTML, ['cache-control', cacheControl]];
    const r = handleResponse(ctx({ snapshot: null, navigation: 'first-visit', public_origin: true, credentials: true, ...fields }), { status: 200, headers, body: fullOf('page-a', members) });
    return r.snapshot.partition;
  };
  assert.equal(store({}, 'public, max-age=60'), 'public');
  assert.equal(store({}, 'Public'), 'public');
  assert.equal(store({ credentials: false }, null), 'public');
  assert.equal(store({ credentials: false }, 'no-cache'), 'public');
  assert.equal(store({}, null), 'request');
  assert.equal(store({}, 'no-cache, max-age=0'), 'request');
  assert.equal(store({}, 'public, private'), 'request');
  assert.equal(store({ credentials: false }, 'private="set-cookie"'), 'request');
  assert.equal(store({ public_origin: false }, 'public'), 'request');
  assert.equal(store({ public_origin: undefined, credentials: false }, 'public'), 'request');
  assert.equal(store({ credentials: undefined }, null), 'request'); // unknown credentials: fail closed
  assert.equal(store({}, 'public', { policy: { mode: 'private' } }), 'request');
  assert.equal(store({}, 'public', {}), 'request');
  assert.deepEqual([...cacheDirectives('a, B=1 , c="x,private"')], ['a', 'b', 'c']);
});

test('H-62 (2), H-63: unknown credentials count as present; Set-Cookie in the response counts as credentials', () => {
  const store = (credentials, extra = [], cacheControl = 'no-cache') => {
    const headers = [...HTML, ['cache-control', cacheControl], ...extra];
    const r = handleResponse(ctx({ snapshot: null, navigation: 'first-visit', public_origin: true, credentials }), { status: 200, headers, body: fullOf('page-a', { policy: { mode: 'public' } }) });
    return r.snapshot.partition;
  };
  assert.equal(store(false), 'public');
  assert.equal(store(null), 'request');
  assert.equal(store(undefined), 'request');
  assert.equal(store(true), 'request');
  assert.equal(store(false, [['set-cookie', 'a=1']]), 'request');
  assert.equal(store(false, [['Set-Cookie', 'a=1']]), 'request'); // field names are case-insensitive
  // The explicit public directive does not depend on credentials (H-61).
  assert.equal(store(null, [], 'public'), 'public');
  // H-63: a Set-Cookie field gives the request partition also with an explicit public directive.
  assert.equal(store(false, [['set-cookie', 'a=1']], 'public'), 'request');
  assert.equal(store(null, [['Set-Cookie', 'a=1']], 'public'), 'request');
});

test('H-62 (1): F is used only when the final request carried RQP fields (origin allowlist, RQP-05)', () => {
  const other = 'https://pay.example.net/catalog';
  const run = (fields) => handleResponse(ctx({ snapshot: null, navigation: 'first-visit', final_url: other, ...fields }), { status: 200, headers: HTML, body: fullOf('page-a', {}, other) });
  assert.deepEqual([run({}).outcome, run({}).error, run({}).snapshot], ['normal', null, null]);
  // Control: the same redirect to an origin in the allowlist stores the snapshot under the final URL.
  const listed = run({ allowlist: ['https://shop.example.com', 'https://pay.example.net'] });
  assert.deepEqual([listed.outcome, listed.snapshot.url], ['stored', other]);
});

test('H-62 (1): the allowlist decides for each URL; a 304 and a delta from another origin fail the cache key', () => {
  const other = 'https://pay.example.net/catalog';
  // The request URL is in the allowlist, so the request carries RQP fields, whatever the final URL is.
  assert.deepEqual(requestHeaders(ctx({ final_url: other })), deltaRequest('page-a').headers);
  // A request URL outside the allowlist: no RQP fields, a normal response.
  assert.deepEqual(requestHeaders(ctx({ allowlist: [] })), [['accept', 'text/html']]);
  const r = handleResponse(ctx({ snapshot: null, navigation: 'first-visit', allowlist: ['https://pay.example.net'] }), { status: 200, headers: HTML, body: fullOf('page-a') });
  assert.deepEqual([r.outcome, r.snapshot], ['normal', null]);
  // A 304 and a delta from an origin outside the allowlist never have the cache key of the snapshot.
  assert.deepEqual([on304({ final_url: other }).outcome, on304({ final_url: other }).next], ['normal', 'webview-load']);
  const delta = handleResponse(ctx({ final_url: other }), { status: 200, headers: DELTA, body: Buffer.from(JSON.stringify(envelope([['price', '130.00']]))) });
  assert.deepEqual([delta.error, delta.next], ['PATCH_INVALID', 'fallback-load']);
});

// DL-038: the allowlist compares serialized origins by exact match.
test('DL-038: the serialized origin of the URL matches an allowlist entry exactly; no wildcard, no suffix match', () => {
  const list = (allowlist) => (url) => inAllowlist({ allowlist }, url);
  const shop = list(['https://shop.example.com']);
  for (const url of ['https://shop.example.com/x', 'https://SHOP.Example.COM/x', 'https://shop.example.com:443/x', 'https://user:pw@shop.example.com/x?q#f']) {
    assert.equal(shop(url), true, url);
  }
  for (const url of ['https://shop.example.com.attacker.net/x', 'https://attacker.net/shop.example.com', 'http://shop.example.com/x', 'https://shop.example.com:8443/x',
    'https://www.shop.example.com/x', 'https://example.com/x', 'https://shop.example.co/x', 'https://shop.example.com./x']) {
    assert.equal(shop(url), false, url);
  }
  // IDNA: the entry is the ASCII serialization (xn--) of the host.
  assert.equal(list(['https://xn--bcher-kva.example'])('https://BÜCHER.example/x'), true);
  assert.equal(list(['https://bücher.example'])('https://bücher.example/x'), false);
  // An entry that is not a serialized origin never matches: no wildcard, no trailing "/", no upper case, no default port.
  for (const entry of ['*', 'https://*.example.com', '*.example.com', 'https://shop.example.com/', 'https://SHOP.example.com', 'https://shop.example.com:443', 'shop.example.com']) {
    assert.equal(list([entry])('https://shop.example.com/x'), false, entry);
  }
  // Without a list, no origin is in the allowlist (fail closed).
  for (const allowlist of [undefined, null, 'https://shop.example.com', {}]) assert.equal(inAllowlist({ allowlist }, 'https://shop.example.com/x'), false);
});

test('DL-038: a final URL that differs only in host case or a default port is the same origin; a suffix host is not', () => {
  const shop = ['https://shop.example.com'];
  const run = (final) => handleResponse(ctx({ snapshot: null, navigation: 'first-visit', final_url: final, allowlist: shop }), { status: 200, headers: HTML, body: fullOf('page-a', {}, final) });
  for (const final of ['https://SHOP.Example.COM/catalog', 'https://shop.example.com:443/catalog']) {
    assert.deepEqual([run(final).outcome, run(final).snapshot.url], ['stored', TARGET], final);
  }
  const suffix = run('https://shop.example.com.attacker.net/catalog');
  assert.deepEqual([suffix.outcome, suffix.error, suffix.snapshot], ['normal', null, null]);
  // The request URL is checked the same way.
  assert.deepEqual(requestHeaders(ctx({ url: 'https://Shop.Example.com:443/catalog', allowlist: shop })), deltaRequest('page-a').headers);
  assert.deepEqual(requestHeaders(ctx({ url: 'https://shop.example.com.attacker.net/catalog', allowlist: shop })), [['accept', 'text/html']]);
});

// H-64, H-65, DL-038: the public partition.
const PUBLIC = { policy: { mode: 'public' } };
const publicCtx = (fields = {}) => ctx({ snapshot: null, navigation: 'first-visit', public_origin: true, credentials: false, ...fields });
const storeIn = (fields, headers) => handleResponse(publicCtx(fields), { status: 200, headers: [...HTML, ...headers], body: fullOf('page-a', PUBLIC) }).snapshot.partition;

test('H-64: the public partition only for a response whose header fields the native SDK saw in full', () => {
  assert.equal(storeIn({}, [['cache-control', 'public']]), 'public');
  assert.equal(storeIn({}, [['cache-control', 'no-cache']]), 'public');
  for (const visible of [false, undefined, null, 'true', 1]) {
    // As if the response had a Set-Cookie field: also an explicit public directive does not help.
    assert.equal(storeIn({ response_fields_visible: visible }, [['cache-control', 'public']]), 'request', String(visible));
    assert.equal(storeIn({ response_fields_visible: visible }, [['cache-control', 'no-cache']]), 'request', String(visible));
  }
  assert.equal(partitionFor({ public_origin: true, credentials: false, redirected: false }, [], PUBLIC), 'request');
  assert.equal(partitionFor({ public_origin: true, credentials: false, response_fields_visible: true, redirected: false }, [], PUBLIC), 'public');
});

test('DL-038: Set-Cookie2 counts as Set-Cookie; field names match ASCII case-insensitively', () => {
  assert.equal(storeIn({}, [['cache-control', 'public'], ['set-cookie2', 'a=1; Version=1']]), 'request');
  assert.equal(storeIn({}, [['cache-control', 'public'], ['Set-Cookie2', 'a=1']]), 'request');
  assert.equal(storeIn({}, [['cache-control', 'public'], ['SET-COOKIE', 'a=1']]), 'request');
  assert.equal(storeIn({}, [['cache-control', 'public'], ['set-cookie3', 'a=1'], ['x-set-cookie', 'a=1']]), 'public');
  assert.equal(setsCookie([['set-cookie', '']]), true); // an empty value is still the field
  assert.equal(setsCookie([['cookie', 'a=1']]), false);
});

test('withoutCredentials is only the credentials flag; partitionFor is the one place of the Set-Cookie rule', () => {
  assert.equal(withoutCredentials({ credentials: false }), true);
  for (const credentials of [true, null, undefined, 0, 'false']) assert.equal(withoutCredentials({ credentials }), false, String(credentials));
  assert.equal(partitionFor({ public_origin: true, credentials: false, response_fields_visible: true, redirected: false }, [['set-cookie', 'a=1']], PUBLIC), 'request');
});

const publicSnapshot = () => snapshotOf(page('page-a'), PUBLIC, TARGET, 'public');
/** A consistent delta from the public snapshot of page-a (its page revision has the policy line). */
function publicDelta(extraHeaders = []) {
  const a = revisions('page-a');
  const content = '130.00';
  const hash = sha256b64u(Buffer.from(content, 'utf8'));
  const next = a.blocks.map((b) => (b.id === 'price' ? { ...b, sha256: hash } : b));
  const env = {
    type: 'rivqen.patch',
    protocol: 'rqp/1',
    base_revision: pageRevision(a.template_revision, a.blocks, PUBLIC),
    next_revision: pageRevision(a.template_revision, next, PUBLIC),
    template_revision: a.template_revision,
    operations: [{ op: 'replace_block', block_id: 'price', format: 'html', content, sha256: hash }],
  };
  return { status: 200, headers: [...DELTA, ['cache-control', 'no-store'], ...extraHeaders], body: Buffer.from(JSON.stringify(env)) };
}
const onPublic = (fields, extraHeaders) => {
  const snap = publicSnapshot();
  const r = handleResponse(ctx({ public_origin: true, credentials: false, snapshot: snap, ...fields }), publicDelta(extraHeaders));
  // The public snapshot itself does not change (H-65).
  assert.deepEqual([snap.partition, snap.page_revision, snap.document], [publicSnapshot().partition, publicSnapshot().page_revision, page('page-a')]);
  return r;
};

test('H-65: a delta on a public snapshot keeps the result public only under the RQP-25 conditions and known absent credentials', () => {
  const ok = onPublic({});
  assert.deepEqual([ok.outcome, ok.snapshot.partition, ok.snapshot.url], ['patched', 'public', TARGET]);
  assert.equal(ok.snapshot.blocks.find((b) => b.id === 'price').content, '130.00');
  const cases = [
    [{ credentials: null }, []],
    [{ credentials: undefined }, []],
    [{ credentials: true }, []],
    // A delta has no-store; an explicit public directive does not replace known absent credentials.
    [{ credentials: true }, [['cache-control', 'public']]],
    [{ response_fields_visible: false }, []],
    [{}, [['set-cookie', 'a=1']]],
    [{}, [['set-cookie2', 'a=1']]],
    [{}, [['cache-control', 'private']]],
    [{ public_origin: false }, []],
  ];
  for (const [fields, headers] of cases) {
    const r = onPublic(fields, headers);
    assert.deepEqual([r.outcome, r.snapshot.partition], ['patched', 'request'], JSON.stringify([fields, headers]));
  }
});

test('H-65: a snapshot in the partition of the request gives a result in the partition of the request', () => {
  const snap = snapshotOf(page('page-a'), PUBLIC, TARGET);
  const r = handleResponse(ctx({ public_origin: true, credentials: false, snapshot: snap }), publicDelta([['cache-control', 'public']]));
  assert.deepEqual([r.outcome, r.snapshot.partition], ['patched', 'request']);
  assert.equal(deltaPartition({ public_origin: true, credentials: false, response_fields_visible: true, redirected: false }, [], { partition: 'request', members: PUBLIC }), 'request');
  assert.equal(deltaPartition({ public_origin: true, credentials: false, response_fields_visible: true, redirected: false }, [], { partition: 'public', members: PUBLIC }), 'public');
});

test('H-65: a 304 on a public snapshot keeps the snapshot as it is, also with Set-Cookie or private on the 304', () => {
  for (const headers of [[], [['set-cookie', 'a=1']], [['cache-control', 'private']]]) {
    const snap = publicSnapshot();
    const r = handleResponse(ctx({ public_origin: true, credentials: null, snapshot: snap }), { status: 304, headers, body: null });
    assert.deepEqual([r.outcome, r.next, r.snapshot, snap.partition], ['not-modified', 'keep', null, 'public'], JSON.stringify(headers));
  }
});

test('the harness interface takes the partition of the context snapshot', () => {
  const s = publicSnapshot();
  const plain = { document: page('page-a'), template_revision: s.analysis.template_revision, page_revision: s.page_revision, blocks: [], ...PUBLIC };
  const context = { ...ctx({ public_origin: true, credentials: false, snapshot: null }), snapshot: { ...plain, partition: 'public' } };
  assert.equal(clientStep(context, publicDelta()).snapshot.partition, 'public');
  assert.equal(clientStep({ ...context, snapshot: plain }, publicDelta()).snapshot.partition, 'request');
  assert.throws(() => clientStep({ ...context, snapshot: { ...plain, partition: 'shared' } }, publicDelta()), /unknown partition shared/);
});

// H-67: a result that comes through a redirect never goes to the public partition.
test('H-67: redirected is true for any redirect, also to the same URL; only false with the same final URL means no redirect', () => {
  assert.equal(redirected({ url: TARGET, redirected: false }), false);
  assert.equal(redirected({ url: TARGET, final_url: TARGET, redirected: false }), false);
  assert.equal(redirected({ url: TARGET, final_url: TARGET, redirected: true }), true);
  // Another final URL is a redirect, also with the flag false and for the same cache key.
  for (const final of ['https://shop.example.com/other', 'https://SHOP.example.com/catalog', `${TARGET}#x`]) {
    assert.equal(redirected({ url: TARGET, final_url: final, redirected: false }), true, final);
  }
  // Fails closed: a flag that is not false counts as a redirect.
  for (const flag of [undefined, null, 0, 'false']) assert.equal(redirected({ url: TARGET, redirected: flag }), true, String(flag));
});

test('H-67: F that comes through a redirect goes to the partition of the request, also with Cache-Control public', () => {
  assert.equal(storeIn({}, [['cache-control', 'public']]), 'public'); // control: no redirect
  const final = 'https://shop.example.com/catalog?page=1';
  const moved = handleResponse(publicCtx({ final_url: final }), { status: 200, headers: [...HTML, ['cache-control', 'public']], body: fullOf('page-a', PUBLIC, final) });
  assert.deepEqual([moved.outcome, moved.snapshot.url, moved.snapshot.partition], ['stored', final, 'request']);
  assert.equal(storeIn({ redirected: true }, [['cache-control', 'public']]), 'request');
  assert.equal(storeIn({ redirected: true, final_url: TARGET }, []), 'request');
  assert.equal(partitionFor({ public_origin: true, credentials: false, response_fields_visible: true, url: TARGET, redirected: true }, [['cache-control', 'public']], PUBLIC), 'request');
});

test('H-67: a delta on a public snapshot that comes through a redirect gives a result in the partition of the request', () => {
  for (const fields of [{ redirected: true }, { final_url: 'https://SHOP.example.com:443/catalog' }]) {
    const r = onPublic(fields, []);
    assert.deepEqual([r.outcome, r.snapshot.url, r.snapshot.partition], ['patched', TARGET, 'request'], JSON.stringify(fields));
  }
  assert.equal(deltaPartition({ public_origin: true, credentials: false, response_fields_visible: true, url: TARGET, redirected: true }, [], { partition: 'public', members: PUBLIC }), 'request');
});

test('H-67: a 304 on a public snapshot after a redirect to the same URL keeps the snapshot as it is', () => {
  const snap = publicSnapshot();
  const r = handleResponse(ctx({ public_origin: true, credentials: false, redirected: true, snapshot: snap }), { status: 304, headers: [], body: null });
  assert.deepEqual([r.outcome, r.next, r.snapshot, snap.partition], ['not-modified', 'keep', null, 'public']);
});

// H-68: no RQP for a URL with a user name or a password.
test('H-68: hasUserinfo is true for a user name, a password or both; an empty user information does not count', () => {
  for (const url of ['https://user:secret@shop.example.com/catalog', 'https://user@shop.example.com/catalog', 'https://:secret@shop.example.com/catalog', 'https://shop.example.com@attacker.example/']) {
    assert.equal(hasUserinfo(url), true, url);
  }
  for (const url of [TARGET, 'https://@shop.example.com/catalog', 'https://:@shop.example.com/catalog', 'https://shop.example.com/a@b', 'https://shop.example.com\\@attacker.example/']) {
    assert.equal(hasUserinfo(url), false, url);
  }
});

test('H-68: no RQP field to a URL with user information, also when its origin is in the allowlist', () => {
  const allowlist = ['https://shop.example.com'];
  for (const url of ['https://user:secret@shop.example.com/catalog', 'https://user@shop.example.com/catalog', 'https://:secret@shop.example.com/catalog']) {
    assert.equal(inAllowlist({ allowlist }, url), true, url); // the origin has no user information
    assert.equal(rqpAllowed(clientContext({ url, allowlist })), false, url);
    assert.deepEqual(requestHeaders(clientContext({ url, allowlist, navigation: 'first-visit' })), [['accept', 'text/html']], url);
    const r = handleResponse(clientContext({ url, allowlist }), { status: 200, headers: HTML, body: fullOf('page-a') });
    assert.deepEqual([r.outcome, r.error, r.snapshot], ['normal', null, null], url);
  }
  assert.equal(rqpAllowed(clientContext({ url: 'https://@shop.example.com/catalog', allowlist })), true);
});

test('H-68: a final URL with user information: F gives a normal page, a 304 and a delta fail the cache key', () => {
  const final = 'https://user:secret@shop.example.com/catalog';
  const full = handleResponse(ctx({ snapshot: null, navigation: 'first-visit', final_url: final }), { status: 200, headers: HTML, body: fullOf('page-a', {}, final) });
  assert.deepEqual([full.outcome, full.error, full.snapshot], ['normal', null, null]);
  assert.deepEqual([on304({ final_url: final }).outcome, on304({ final_url: final }).next], ['normal', 'webview-load']);
  const delta = handleResponse(ctx({ final_url: final }), { status: 200, headers: DELTA, body: Buffer.from(JSON.stringify(envelope([['price', '130.00']]))) });
  assert.deepEqual([delta.outcome, delta.error, delta.next], ['rejected', 'PATCH_INVALID', 'fallback-load']);
});

// H-69: an allowlist entry is its own serialized origin.
test('H-69: validAllowlistEntry accepts only a string that is its own serialized origin', () => {
  for (const entry of ['https://shop.example.com', 'http://shop.example.com', 'https://shop.example.com:8443', 'https://xn--fa-hia.example', 'https://[::1]', 'https://192.0.2.1']) {
    assert.equal(validAllowlistEntry(entry), true, entry);
  }
  const invalid = ['HTTPS://shop.example.com', 'https://SHOP.example.com', 'https://shop.example.com:443', 'http://shop.example.com:80', 'https://shop.example.com/',
    'https://shop.example.com/catalog', 'https://faß.example', 'https://bücher.example', '*', '*.example.com', 'shop.example.com', ' https://shop.example.com',
    'https://shop.example.com ', 'https://user:pw@shop.example.com', 'https://shop.example.com?q', 'https://shop.example.com#f', 'null', 'file:///x', '', 'https://[0:0::1]'];
  for (const entry of invalid) assert.equal(validAllowlistEntry(entry), false, entry);
  // Never throws: also not for a value that cannot become a string.
  for (const entry of [null, undefined, 1, ['https://shop.example.com'], { href: 'https://shop.example.com' }, Symbol('x')]) assert.equal(validAllowlistEntry(entry), false, typeof entry);
});

// DL-040 L-3: the origin is the one of the WHATWG parse of the URL.
test('DL-040 L-3: a backslash in an https URL is a path separator, and an IDN host has its nontransitional xn-- form', () => {
  const shop = { allowlist: ['https://shop.example.com'] };
  assert.equal(inAllowlist(shop, 'https://attacker.example\\@shop.example.com/catalog'), false);
  assert.equal(inAllowlist(shop, 'https://shop.example.com\\@attacker.example/catalog'), true);
  assert.equal(inAllowlist(shop, 'https://shop.example.com@attacker.example/catalog'), false);
  assert.equal(inAllowlist({ allowlist: ['https://xn--fa-hia.example'] }, 'https://faß.example/'), true);
  assert.equal(inAllowlist({ allowlist: ['https://fass.example'] }, 'https://faß.example/'), false);
});

test('a patched snapshot keeps the cache key of the snapshot', () => {
  const r = handleResponse(ctx(), { status: 200, headers: DELTA, body: Buffer.from(JSON.stringify(envelope([['price', '130.00']]))) });
  assert.deepEqual([r.snapshot.url, r.snapshot.partition], [TARGET, 'request']);
});

// Request fields (negotiation section 6.2).
test('request fields: R(snapshot) for a delta navigation, no delta fields otherwise', () => {
  assert.deepEqual(requestHeaders(ctx()), deltaRequest('page-a').headers);
  const first = requestHeaders(ctx({ snapshot: null, navigation: 'first-visit' }));
  assert.deepEqual(first, firstVisit().headers);
  assert.deepEqual(requestHeaders(ctx({ navigation: 'fallback' })), firstVisit().headers);
  assert.deepEqual(requestHeaders(ctx({ url: 'http://shop.example.com/catalog' })), [['accept', 'text/html']]);
});

// Attribute rule (manifest-patch section 4.2 table).
test('attribute rule: the examples of manifest-patch section 4.2', () => {
  const rejected = ['<a onclick="x()">y</a>', '<a ONLOAD=x>y</a>', '<a href="javascript:x">y</a>', '<a href=" JavaScript:x">y</a>',
    '<a href="java&#x09;script:x">y</a>', '<b title="a &quot; b">y</b>', '<a href="?a=1&b=2">y</a>', '<a href="java\tscript:x">y</a>',
    "<a href='\x01javascript:x'>y</a>", '<a title="x" title="javascript:y">z</a>', '<a href=&AMP;>y</a>',
    // H-59: vbscript: and data: with the same normalization.
    '<a href="vbscript:x">y</a>', '<a href="data:text/html,x">y</a>', '<img src="DATA:image/png,x">', '<a href=" \tDa\nTa:x">y</a>',
    '<a href="\x00VbScRiPt:x">y</a>', "<a title='data:x'>y</a>", '<a href=data:x>y</a>'];
  const accepted = ['<a href="/p?a=1&amp;b=2">y</a>', '<b title="Tom &amp; Jerry">y</b>', '<a data-x="javascript">y</a>', '<a href="/javascript:x">y</a>', 'text & more', '<!-- <a onclick=x> -->',
    '<a href="/data:x">y</a>', '<a href="database:x">y</a>', '<a href="data">y</a>', '<a href="vbscript">y</a>', '<a href="x-data:y">y</a>'];
  for (const c of rejected) assert.equal(passesAttributeRule(Buffer.from(c)), false, c);
  for (const c of accepted) assert.equal(passesAttributeRule(Buffer.from(c)), true, c);
});

test('attribute tokenizer: quoted, unquoted, valueless, duplicate and "=" names', () => {
  const tag = Buffer.from('<a b="1" c=\'2\' d=3 e f = "4"/ =g h="5"x=6>');
  const got = attributesOf(tag, 0, tag.length).map(({ name, value }) => [name, value.toString()]);
  assert.deepEqual(got, [['b', '1'], ['c', '2'], ['d', '3'], ['e', ''], ['f', '4'], ['=g', ''], ['h', '5'], ['x', '6']]);
  assert.equal(attributeRejected('onx', Buffer.alloc(0)), true);
  assert.equal(attributeRejected('o', Buffer.from('&amp')), true);
});

test('the server never inserts a manifest text that contains "<" (manifest-patch section 1.7, server item 3)', () => {
  assert.equal(fullDocument(page('page-a'), 0, { a: '<' }), null);
  assert.equal(snapshotOf(blockPage('é')).analysis.blocks[0].id, 'x');
});
