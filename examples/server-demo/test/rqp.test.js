// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors
//
// Response selection of the demo (negotiation section 4), by behavior.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { buildDelta, changeSet, handle, pageRevision, policyLineText } from '../src/rqp.js';
import { APP, TARGET, clientLines, exampleJson, get, header, katBytes, page, revisionsOf } from './helpers.js';

const POLICY = { policy: { mode: 'private', ttl_seconds: 60, offline: false }, limits: { max_patch_bytes: 262144 } };
const run = (request, p = page('130.00'), config = {}) => handle(request, structuredClone(APP), p, config);

test('every request gets the same full document, with Rq-Version and a strong ETag of the page revision', () => {
  const p = page();
  const plain = run(get(), p);
  const rqp = run(get([['rq-version', '1']]), p);
  const unsupported = run(get([['rq-version', '2']]), p);
  for (const r of [plain, rqp, unsupported]) {
    assert.equal(r.status, 200);
    assert.deepEqual(r.body, plain.body);
    assert.equal(header(r, 'etag'), `"${revisionsOf(p).page_revision}"`);
    assert.equal(header(r, 'rq-version'), '1');
    assert.equal(header(r, 'vary'), null);
  }
});

test('the full document is the page with the manifest before the first </head> (known answers)', () => {
  assert.deepEqual(run(get(), katBytes('page.html')).body, katBytes('full.html'));
  const withPolicy = run(get(), katBytes('page.html'), POLICY);
  assert.deepEqual(withPolicy.body, katBytes('full-policy.html'));
  assert.equal(header(withPolicy, 'etag'), '"r1.ke0dIjtkqBtefkyyLQU5ICLWPia5PfammTko2xmMLOQ"');
});

test('the page revision with a policy line follows the second known-answer set', () => {
  const kat = exampleJson('valid/manifest/shop-policy.json');
  const r = revisionsOf(katBytes('page.html'));
  assert.equal(pageRevision(r.template_revision, r.blocks, POLICY), kat.page_revision);
  assert.equal(pageRevision(r.template_revision, r.blocks, {}), r.page_revision);
  assert.equal(policyLineText({ policy: {}, limits: {} }), 'policy;limits');
});

test('the same If-None-Match as the page revision gives 304 without body and with the cache fields', () => {
  const p = page();
  const r = run(get(clientLines(p)), p);
  assert.equal(r.status, 304);
  assert.equal(r.body, null);
  assert.equal(header(r, 'etag'), `"${revisionsOf(p).page_revision}"`);
  assert.equal(header(r, 'cache-control'), 'no-cache');
  assert.equal(header(r, 'content-type'), null);
});

test('a weak If-None-Match tag gives 304 (weak comparison)', () => {
  const p = page();
  const r = run(get([['if-none-match', `W/"${revisionsOf(p).page_revision}"`]]), p);
  assert.equal(r.status, 304);
});

test('a changed block gives a delta with only that block, no-store and no ETag', () => {
  const held = page('120.00');
  const r = run(get(clientLines(held)), page('130.00'));
  assert.equal(r.status, 200);
  assert.equal(header(r, 'content-type'), 'application/vnd.rivqen.delta+json');
  assert.equal(header(r, 'cache-control'), 'no-store');
  assert.equal(header(r, 'etag'), null);
  const delta = JSON.parse(r.body);
  assert.deepEqual(delta.operations.map((o) => [o.op, o.block_id, o.content]), [['replace_block', 'price', '130.00']]);
  assert.equal(delta.base_revision, revisionsOf(held).page_revision);
  assert.equal(delta.next_revision, revisionsOf(page('130.00')).page_revision);
});

test('the delta of the known-answer page equals the example patch (one block, and all blocks without b)', () => {
  const held = katBytes('page.html');
  const current = Buffer.from(held.toString('utf8').replace('120.00', '130.00'));
  assert.deepEqual(JSON.parse(run(get(clientLines(held)), current).body), exampleJson('valid/patch/shop-price.json'));
  const noB = clientLines(held, { 'rq-have': `t=${revisionsOf(held).template_revision}` });
  assert.equal(JSON.parse(run(get(noB), current).body).operations.length, 3); // H-7: every block
  const all = Buffer.from(current.toString('utf8').replace('>Shop<', '><').replace('{"items":3}', '{"items":4}'));
  assert.deepEqual(JSON.parse(run(get(clientLines(held)), all).body), exampleJson('valid/patch/shop-all.json'));
});

test('a delta needs each of: block-patch, Accept for the delta type, one r1 tag, a matching template', () => {
  const held = page('120.00');
  const current = page('130.00');
  const cases = {
    'no block-patch': { 'rq-capabilities': '(realtime-ws)' },
    'Accept without the delta type': { accept: 'text/html' },
    'Accept wildcard': { accept: '*/*' },
    'no If-None-Match': { 'if-none-match': null },
    'two tags': { 'if-none-match': `"${revisionsOf(held).page_revision}", "r1.${'A'.repeat(43)}"` },
    'no Rq-Have': { 'rq-have': null },
    'other template': { 'rq-have': `t=t1.${'A'.repeat(43)}` },
    'version 2': { 'rq-version': '2' },
  };
  for (const [name, change] of Object.entries(cases)) {
    const r = run(get(clientLines(held, change)), current);
    assert.equal(header(r, 'content-type'), 'text/html; charset=utf-8', name);
    assert.equal(r.status, 200, name);
  }
});

test('a request with Range or a HEAD request gets no delta', () => {
  const held = page('120.00');
  const range = run(get([...clientLines(held), ['range', 'bytes=0-9']]), page('130.00'));
  assert.equal(header(range, 'content-type'), 'text/html; charset=utf-8');
  const head = run({ ...get(clientLines(held)), method: 'HEAD' }, page('130.00'));
  assert.equal(head.status, 200);
  assert.equal(head.body, null);
  assert.equal(header(head, 'content-type'), 'text/html; charset=utf-8');
});

test('a b with the wrong length is absent: the delta has every block', () => {
  const held = page('120.00');
  const r = run(get(clientLines(held, { 'rq-have': `t=${revisionsOf(held).template_revision}, b=:AAAAAAAAAAA=:` })), page('130.00'));
  assert.equal(JSON.parse(r.body).operations.length, 3);
});

test('equal digest prefixes give an empty change set and the full document, not a delta', () => {
  const held = page('120.00');
  const lines = clientLines(held, { 'if-none-match': `"r1.${'A'.repeat(43)}"` });
  const r = run(get(lines), held);
  assert.equal(r.status, 200);
  assert.equal(header(r, 'content-type'), 'text/html; charset=utf-8');
  assert.equal(r.reason, 'empty-change-set');
});

test('a changed template gives the full document', () => {
  const held = page('120.00');
  const r = run(get(clientLines(held)), page('120.00', '<nav>new</nav>'));
  assert.equal(r.reason, 'not-eligible');
  assert.equal(header(r, 'content-type'), 'text/html; charset=utf-8');
});

test('a delta larger than the limit, or not smaller than the full document, gives the full document', () => {
  const held = page('120.00');
  const lines = clientLines(held);
  const big = page('1'.repeat(3000));
  assert.equal(run(get(lines), big, { maxPatchBytes: 1000 }).reason, 'delta-too-large');
  assert.equal(run(get(lines), big, { limits: { max_patch_bytes: 1024 } }).reason, 'delta-too-large');
  // JSON doubles every quote of the content, so the delta is larger than F.
  const quotes = (c) => Buffer.from(`<html><head></head><body><p data-rq-block="a">${c.repeat(1500)}</p></body></html>`);
  assert.equal(run(get(clientLines(quotes("'"))), quotes('"')).reason, 'delta-not-smaller');
  assert.equal(run(get(clientLines(quotes("'"))), quotes('x')).reason, 'delta');
});

test('a changed html block with an on* attribute gives the full document (H-56)', () => {
  const held = page('120.00');
  const current = page('<b onclick="x()">130.00</b>');
  const r = run(get(clientLines(held)), current);
  assert.equal(r.reason, 'attribute-rule');
  assert.equal(header(r, 'etag'), `"${revisionsOf(current).page_revision}"`);
});

test('a page that is not valid for RQP, or has no </head>, is sent unchanged without Rq fields', () => {
  for (const html of [
    '<html><head></head><body><p data-rq-block="a">1</p><p data-rq-block="a">2</p></body></html>',
    '<html><body><p data-rq-block="a">1</p></body></html>',
  ]) {
    const p = Buffer.from(html);
    const r = run(get([['rq-version', '1']]), p);
    assert.deepEqual(r.body, p);
    assert.equal(r.status, 200);
    assert.equal(header(r, 'rq-version'), null);
    assert.equal(header(r, 'etag'), null);
  }
});

test('a response out of scope is sent unchanged: POST, status 404, JSON type', () => {
  const p = page();
  assert.equal(run({ ...get(), method: 'POST' }, p).mode, 'passthrough');
  assert.equal(handle(get(), { status: 404, headers: APP.headers }, p, {}).mode, 'passthrough');
  assert.equal(handle(get(), { status: 200, headers: [['content-type', 'application/json']] }, p, {}).mode, 'passthrough');
});

test('an ETag of the application is replaced; with a content coding the ETag is weak (H-52)', () => {
  const p = page();
  const app = { status: 200, headers: [...APP.headers, ['etag', '"app-1"'], ['content-length', '7']] };
  const r = handle(get(), app, p, {});
  assert.equal(header(r, 'etag'), `"${revisionsOf(p).page_revision}"`);
  assert.equal(header(r, 'content-length'), String(r.body.length));
  const weak = handle(get(), app, p, { contentCoding: true });
  assert.equal(header(weak, 'etag'), `W/"${revisionsOf(p).page_revision}"`);
});

test('an internal error sends the application response unchanged and reports it', () => {
  const p = page();
  const r = handle({ method: 'GET', target: TARGET }, structuredClone(APP), p, {}); // no headers: a defect of the caller
  assert.equal(r.mode, 'passthrough');
  assert.equal(r.reason, 'SERVER_PASSTHROUGH');
  assert.deepEqual(r.body, p);
  assert.ok(r.error instanceof Error);
});

test('the server keeps no state: the same request gives the same answer after other requests', () => {
  const held = page('120.00');
  const current = page('130.00');
  const first = run(get(clientLines(held)), current);
  run(get(clientLines(current)), page('140.00'));
  run(get(), page('150.00'));
  const again = run(get(clientLines(held)), current);
  assert.deepEqual(again.body, first.body);
  assert.deepEqual(again.headers, first.headers);
});

test('property: the delta of random price changes leads to the revision of the current page', () => {
  let seed = 7;
  const next = () => (seed = (seed * 1103515245 + 12345) % 2147483648);
  for (let i = 0; i < 60; i++) {
    const held = page(`${next() % 1000}.00`);
    const current = page(`${next() % 1000}.00`);
    const r = run(get(clientLines(held)), current);
    const cur = revisionsOf(current);
    if (r.status === 304) {
      assert.equal(cur.page_revision, revisionsOf(held).page_revision);
      continue;
    }
    const delta = JSON.parse(r.body);
    const merged = revisionsOf(held).blocks.map((b) => {
      const op = delta.operations.find((o) => o.block_id === b.id);
      return op === undefined ? b : { id: op.block_id, format: op.format, sha256: op.sha256 };
    });
    assert.equal(pageRevision(cur.template_revision, merged, {}), delta.next_revision);
    assert.equal(delta.next_revision, cur.page_revision);
  }
});

test('changeSet and buildDelta: one operation per changed block, in the given order', () => {
  const held = revisionsOf(page('1'));
  const cur = revisionsOf(page('2', ''));
  const prefixes = Buffer.concat(held.blocks.map((b) => Buffer.from(b.sha256, 'base64url').subarray(0, 8)));
  assert.deepEqual(changeSet(cur.blocks, prefixes), [1]);
  assert.deepEqual(changeSet(cur.blocks, null), [0, 1, 2]);
  const delta = buildDelta(page('2'), cur, 'r1.a', 'r1.b', [1, 2]);
  assert.deepEqual(delta.operations.map((o) => o.block_id), ['price', 'cart']);
});
