// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors
//
// Reference client step: each error code of manifest-patch.md section 5, the
// check order of diff-engine.md section 3, the manifest checks of sections 1.7
// and 1.8, and the attribute rule of section 4.2.

import assert from 'node:assert/strict';
import { test } from 'node:test';
import { LIMITS, pageRevision, sha256b64u } from '../src/common.mjs';
import { checkFull, handleResponse, requestHeaders, snapshotOf, verifyDelta } from '../src/client.mjs';
import { attributeRejected, attributesOf, passesAttributeRule } from '../src/markup.mjs';
import { fullDocument, handle, manifestOf } from '../src/server.mjs';
import { APP, TARGET, blockPage, deltaRequest, firstVisit, page, revisions } from './helpers.mjs';

const snapA = () => snapshotOf(page('page-a'));
const ctx = (fields = {}) => ({ url: TARGET, build: 'release', allow_http: false, navigation: 'delta', snapshot: snapA(), ...fields });
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
  const on = handleResponse({ url: http, build: 'debug', allow_http: true, navigation: 'first-visit', snapshot: null }, { status: 200, headers: HTML, body });
  assert.equal(on.outcome, 'stored');
  for (const [build, flag] of [['release', true], ['debug', false]]) {
    const off = handleResponse({ url: http, build, allow_http: flag, navigation: 'first-visit', snapshot: null }, { status: 200, headers: HTML, body });
    assert.equal(off.outcome, 'normal');
  }
});

test('304 keeps the snapshot', () => {
  assert.deepEqual(handleResponse(ctx(), { status: 304, headers: [], body: null }).next, 'keep');
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
    "<a href='\x01javascript:x'>y</a>", '<a title="x" title="javascript:y">z</a>', '<a href=&AMP;>y</a>'];
  const accepted = ['<a href="/p?a=1&amp;b=2">y</a>', '<b title="Tom &amp; Jerry">y</b>', '<a data-x="javascript">y</a>', '<a href="/javascript:x">y</a>', 'text & more', '<!-- <a onclick=x> -->'];
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
