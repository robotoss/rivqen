// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors
//
// The S2 review fixes (DL-030 to DL-033) at the level of handle(): Rq-* fields of the
// application response, Last-Modified of F, the head rule, the attribute rule schemes.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { handle } from '../src/rqp.js';
import { APP, TARGET, clientLines, get, header, page, revisionsOf } from './helpers.js';

const run = (request, p = page('130.00'), config = {}) => handle(request, structuredClone(APP), p, config);
const RQ_APP = [['rq-version', '2'], ['rq-revision', 'x'], ['rq-x-app', '1'], ['x-app', 'kept']];
const rqNames = (r) => r.headers.map(([n]) => n).filter((n) => n.startsWith('rq-'));

test('no outcome carries an Rq-* field of the application response (DL-030)', () => {
  const held = page('120.00');
  const p = page('130.00');
  const withRq = (extra = []) => ({ status: 200, headers: [...APP.headers, ...RQ_APP, ...extra] });
  const outcomes = {
    full: handle(get([['rq-version', '1']]), withRq(), p, {}),
    delta: handle(get(clientLines(held)), withRq(), p, {}),
    'not modified': handle(get(clientLines(p)), withRq([['etag', '"a"'], ['vary', 'accept']]), p, {}),
    'out of scope': handle({ ...get(), method: 'POST' }, withRq(), p, {}),
    'invalid page': handle(get(), withRq(), Buffer.from('<html><body></body></html>'), {}),
    'internal error': handle({ method: 'GET', target: TARGET }, withRq(), p, {}),
  };
  assert.deepEqual(
    Object.values(outcomes).map((r) => r.mode),
    ['full', 'delta', '304', 'passthrough', 'passthrough', 'passthrough'],
  );
  assert.equal(outcomes['internal error'].reason, 'SERVER_PASSTHROUGH');
  for (const [name, r] of Object.entries(outcomes)) {
    const own = r.mode === 'full' || r.mode === 'delta';
    assert.deepEqual(rqNames(r), own ? ['rq-version'] : [], name);
    if (own) assert.equal(header(r, 'rq-version'), '1', name);
    if (r.mode !== '304') assert.equal(header(r, 'x-app'), 'kept', name);
  }
});

test('Last-Modified of the application is not sent with the full document (DL-032)', () => {
  const app = { status: 200, headers: [...APP.headers, ['last-modified', 'Mon, 12 Oct 2026 10:00:00 GMT'], ['etag', '"app"']] };
  const r = handle(get(), app, page(), {});
  assert.equal(r.mode, 'full');
  assert.equal(header(r, 'last-modified'), null);
  assert.equal(header(r, 'etag'), `"${revisionsOf(page()).page_revision}"`);
});

test('a page whose only head end tag is stray (body, content before head, svg in head) is sent unchanged (DL-031)', () => {
  for (const html of [
    '<html><head><title>t</title><body><p data-rq-block="a">1</p></head></body></html>',
    '<html><body><p data-rq-block="a">1</p></body></head></html>',
    '<html><p>x</p><head></head><body><p data-rq-block="a">1</p></body></html>',
    '<html><head><svg></svg></head><body><p data-rq-block="a">1</p></body></html>',
  ]) {
    const p = Buffer.from(html);
    const r = run(get(), p);
    assert.equal(r.mode, 'passthrough', html);
    assert.deepEqual(r.body, p);
  }
});

test('the manifest goes before the head end tag of the document, not before one in the title text', () => {
  const html = '<html><head><title>a</head>b</title></head><body><p data-rq-block="a">1</p></body></html>';
  const r = run(get(), Buffer.from(html));
  assert.equal(r.mode, 'full');
  const text = r.body.toString('utf8');
  assert.ok(text.startsWith('<html><head><title>a</head>b</title><script type="application/rivqen-manifest+json">'));
  assert.ok(text.includes('</script></head><body>'));
});

test('a stray end tag in the head does not move the insertion point; </br> in the head means passthrough (DL-034)', () => {
  const ok = '<html><head><meta charset=utf-8></meta><title>x</title></head><body><p data-rq-block="a">1</p></body></html>';
  const r = run(get(), Buffer.from(ok));
  assert.equal(r.mode, 'full');
  assert.ok(r.body.toString('utf8').includes('</script></head><body>'));
  const br = Buffer.from('<html><head></br></head><body><p data-rq-block="a">1</p></body></html>');
  assert.equal(run(get(), br).mode, 'passthrough');
});

test('a changed block with a data:, vbscript: or javascript: URL gives the full document (H-59)', () => {
  const held = page('120.00');
  for (const href of ['data:text/html,x', ' VBScript:x', '\n\tjava\tscript:x','javascript:x']) {
    const r = run(get(clientLines(held)), page(`<a href="${href}">p</a>`));
    assert.equal(r.reason, 'attribute-rule', href);
  }
  assert.equal(run(get(clientLines(held)), page('<a href="/data:x">p</a>')).mode, 'delta');
});
