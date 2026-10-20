// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors
//
// Response selection of the demo (negotiation section 4), by behavior.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { buildDelta, buildManifest, changeSet, handle, membersOf, pageRevision, policyLineText } from '../src/rqp.js';
import { APP, TARGET, clientLines, exampleJson, get, header, katBytes, page, revisionsOf } from './helpers.js';

// The members of the second known-answer set (shop-policy.json): none of them is a default.
const POLICY = { policy: { ttl_seconds: 60, offline: true }, limits: { max_patch_bytes: 65536 } };
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
  assert.equal(header(withPolicy, 'etag'), '"r1.pONNMoySlMlYw50BpgdvbUlGESnMzZBCOmQ4SlebDxs"');
});

test('the page revision with a policy line follows the second known-answer set', () => {
  const kat = exampleJson('valid/manifest/shop-policy.json');
  const r = revisionsOf(katBytes('page.html'));
  assert.equal(pageRevision(r.template_revision, r.blocks, POLICY), kat.page_revision);
  assert.equal(pageRevision(r.template_revision, r.blocks, {}), r.page_revision);
  assert.equal(policyLineText({ policy: {}, limits: {} }), 'policy;limits');
});

test('default members are not written: defaults, empty objects and a hint not smaller than the server limit (H-103, DL-057, DL-058)', () => {
  const defaults = { policy: { mode: 'private', ttl_seconds: 0, offline: false }, limits: { max_patch_bytes: 262144 } };
  assert.deepEqual(membersOf(defaults), {});
  assert.deepEqual(membersOf({ policy: {}, limits: {} }), {});
  assert.deepEqual(membersOf(defaults, 262145), { limits: { max_patch_bytes: 262144 } });
  assert.deepEqual(membersOf({ policy: { mode: 'public', ttl_seconds: 0, offline: false } }), { policy: { mode: 'public' } });
  assert.deepEqual(membersOf({ policy: { ttl_seconds: 1 } }), { policy: { ttl_seconds: 1 } });
  assert.deepEqual(membersOf({ policy: { offline: true } }), { policy: { offline: true } });
  const p = katBytes('page.html');
  const r = run(get(), p, defaults);
  assert.deepEqual(r.body, katBytes('full.html'));
  assert.equal(header(r, 'etag'), `"${revisionsOf(p).page_revision}"`);
});

test('the manifest has exactly protocol, origin, path and page_revision, then policy and limits when they are not default (H-103)', () => {
  const a = revisionsOf(katBytes('page.html'));
  assert.deepEqual(Object.keys(buildManifest(a, TARGET, {})), ['protocol', 'origin', 'path', 'page_revision']);
  assert.deepEqual(Object.keys(buildManifest(a, TARGET, POLICY)), ['protocol', 'origin', 'path', 'page_revision', 'policy', 'limits']);
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
  assert.deepEqual(Object.keys(delta), ['next_revision', 'operations']); // H-104: no other member
  assert.deepEqual(delta.operations, [{ block_id: 'price', content: '130.00' }]);
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

test('the size rules are exact: a delta of exactly the limit is sent; a delta as large as F is not', () => {
  const held = page('120.00');
  const lines = clientLines(held);
  const current = page('130.00');
  const delta = run(get(lines), current);
  assert.equal(delta.mode, 'delta');
  const size = delta.body.length;
  assert.equal(run(get(lines), current, { maxPatchBytes: size }).mode, 'delta');
  assert.equal(run(get(lines), current, { maxPatchBytes: size - 1 }).reason, 'delta-too-large');
  // Each quote adds one byte to the delta (JSON escape) and none to F: find the first step where F is not larger.
  const quotes = (n) => Buffer.from(`<html><head></head><body><p data-rq-block="a">${'"'.repeat(n)}${'x'.repeat(400 - n)}</p></body></html>`);
  const heldQuotes = Buffer.from(`<html><head></head><body><p data-rq-block="a">${'y'.repeat(400)}</p></body></html>`);
  const base = clientLines(heldQuotes);
  let n = 1;
  while (run(get(base), quotes(n)).mode === 'delta') n++;
  const full = run(get(base), quotes(n));
  const analysis = revisionsOf(quotes(n));
  const envelope = buildDelta(quotes(n), analysis, analysis.page_revision, [0]);
  assert.equal(full.reason, 'delta-not-smaller');
  assert.equal(Buffer.byteLength(JSON.stringify(envelope)), full.body.length);
});

test('a changed html block with an on* attribute gives the full document (H-56)', () => {
  const held = page('120.00');
  const current = page('<b onclick="x()">130.00</b>');
  const r = run(get(clientLines(held)), current);
  assert.equal(r.reason, 'patch-content-rule');
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
  for (const status of [404, 201, 206, 304]) assert.equal(handle(get(), { status, headers: APP.headers }, p, {}).mode, 'passthrough', String(status));
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

test('a delta does not carry ETag, Last-Modified, Expires or the length of the application response', () => {
  const held = page('120.00');
  const app = {
    status: 200,
    headers: [...APP.headers, ['etag', '"app"'], ['last-modified', 'Mon, 12 Oct 2026 10:00:00 GMT'], ['expires', 'Tue, 13 Oct 2026 10:00:00 GMT'], ['content-length', '1'], ['x-app', 'kept']],
  };
  const r = handle(get(clientLines(held)), app, page('130.00'), {});
  assert.equal(r.mode, 'delta');
  for (const name of ['etag', 'last-modified', 'expires']) assert.equal(header(r, name), null, name);
  assert.equal(header(r, 'content-length'), String(r.body.length));
  assert.equal(header(r, 'x-app'), 'kept');
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
      return op === undefined ? b : { id: b.id, format: b.format, sha256: createHash('sha256').update(op.content).digest('base64url') };
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
  const delta = buildDelta(page('2'), cur, 'r1.b', [1, 2]);
  assert.equal(delta.next_revision, 'r1.b');
  assert.deepEqual(delta.operations.map((o) => o.block_id), ['price', 'cart']);
});

const withType = (value) => ({ status: 200, headers: [['content-type', value], ['cache-control', 'no-cache']] });
const runType = (value) => handle(get(), withType(value), page());

test('H-62: a charset other than UTF-8 makes the page invalid: it is sent unchanged, without Rq-* fields', () => {
  for (const value of ['text/html; charset=iso-8859-1', 'text/html;charset="windows-1252"', 'text/html; charset=utf-16', 'text/html; charset=""']) {
    const r = runType(value);
    assert.equal(r.mode, 'passthrough', value);
    assert.deepEqual(r.body, page(), value);
    assert.equal(header(r, 'content-type'), value);
    assert.equal(r.headers.filter(([n]) => n.startsWith('rq-')).length, 0, value);
  }
});

test('H-62: a UTF-8 charset label (any case, quoted or not) gives the full document with the exact content type (H-66)', () => {
  for (const value of ['text/html; charset=utf-8', 'text/html; charset="UTF-8"', 'text/html; Charset=utf8', 'text/html; charset=Unicode-1-1-UTF-8', 'text/html; foo="a;charset=x"; charset=x-unicode20utf8']) {
    const r = runType(value);
    assert.equal(r.mode, 'full', value);
    assert.equal(header(r, 'content-type'), 'text/html; charset=utf-8', value);
    assert.equal(r.headers.filter(([n]) => n === 'content-type').length, 1, value);
    assert.equal(header(r, 'rq-version'), '1');
  }
});

test('H-62: a content type without a charset gets "; charset=utf-8" in the full document; the body is the same', () => {
  const r = runType('text/html');
  assert.equal(r.mode, 'full');
  assert.equal(header(r, 'content-type'), 'text/html; charset=utf-8');
  assert.deepEqual(r.body, runType('text/html; charset=utf-8').body);
});

test('H-62: a charset parameter inside a quoted value of another parameter does not count as a charset', () => {
  const r = runType('text/html; foo="a;charset=iso-8859-1"');
  assert.equal(r.mode, 'full');
  assert.equal(header(r, 'content-type'), 'text/html; charset=utf-8'); // H-66: the app text is dropped
});

test('H-62: a delta keeps its own content type (the charset rule is for the full document)', () => {
  const held = page('120.00');
  const r = handle(get(clientLines(held)), withType('text/html'), page('130.00'));
  assert.equal(r.mode, 'delta');
  assert.equal(header(r, 'content-type'), 'application/vnd.rivqen.delta+json');
});

test('H-66: a Content-Type of A with a UTF-8 label in another spelling is replaced by the exact value', () => {
  for (const value of ['TEXT/HTML;CHARSET=UTF8', 'text/html;charset="utf-8"', 'text/html ; charset=utf-8 ']) {
    assert.equal(header(runType(value), 'content-type'), 'text/html; charset=utf-8', value);
  }
});

test('H-66: F keeps the other fields of A and the place of the Content-Type line', () => {
  const app = { status: 200, headers: [['x-a', '1'], ['content-type', 'text/html'], ['x-b', '2']] };
  const names = handle(get(), app, page()).headers.map(([n]) => n);
  assert.deepEqual(names.slice(0, 3), ['x-a', 'content-type', 'x-b']);
});

test('H-66 step 1: Unicode white space is not HTTP white space, so U+00A0 before the media type is out of scope', () => {
  const r = runType('\u00a0text/html');
  assert.equal(r.mode, 'passthrough');
  assert.equal(r.reason, 'out-of-scope');
  assert.equal(runType('\t text/html \t').mode, 'full');
});
