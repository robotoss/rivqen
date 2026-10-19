// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors
//
// The S2 review fixes (DL-030 to DL-033) at the level of handle(): Rq-* fields of the
// application response, Last-Modified of F, the head rule (H-72), the attribute rule schemes.
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

test('a page without a head start tag of the document and without an implied head is sent unchanged (H-72, H-79a)', () => {
  for (const html of [
    '<html><body><p data-rq-block="a">1</p></body></head></html>', // no <head> start tag, body content first
    '<html><p>x</p><head></head><body><p data-rq-block="a">1</p></body></html>', // content before <head>
    '<html><noscript></noscript><head></head><body><p data-rq-block="a">1</p></body></html>', // noscript is not head content here
    '<html><template></template><head></head><body><p data-rq-block="a">1</p></body></html>', // nor is template
  ]) {
    const p = Buffer.from(html);
    const r = run(get(), p);
    assert.equal(r.mode, 'passthrough', html);
    assert.deepEqual(r.body, p);
  }
});

test('head content before any head start tag implies the head: the manifest goes immediately before it (H-79a)', () => {
  for (const [html, stop] of [
    ['<html><title>t</title><head></head><body><p data-rq-block="a">1</p></body></html>', '<title>'],
    ['<!DOCTYPE html>\n<meta charset="utf-8">\n<title>t</title><p data-rq-block="a">1</p>', '<meta'],
    ['<html>\n  <link rel="icon" href="/i.png"><body><p data-rq-block="a">1</p></body></html>', '<link'],
    ['<script src="/a.js"></script><p data-rq-block="a">1</p>', '<script src'],
  ]) {
    const r = run(get(), Buffer.from(html));
    assert.equal(r.mode, 'full', html);
    const text = r.body.toString('utf8');
    const at = html.indexOf(stop);
    assert.equal(text.indexOf('<script type="application/rivqen-manifest+json">'), at, html);
    assert.equal(text.slice(0, at), html.slice(0, at), html);
    assert.equal(text.slice(text.indexOf('</script>') + '</script>'.length), html.slice(at), html);
  }
});

test('the manifest goes immediately after the head start tag, also when the head has no end tag (H-72)', () => {
  const MANIFEST = '<html><head><script type="application/rivqen-manifest+json">';
  for (const html of [
    '<html><head><title>a</head>b</title></head><body><p data-rq-block="a">1</p></body></html>', // "</head>" in the title text
    '<html><head><title>t</title><body><p data-rq-block="a">1</p></head></body></html>', // closed by body; stray </head>
    '<html><head><svg></svg></head><body><p data-rq-block="a">1</p></body></html>', // svg closes the head
    '<html><head><meta charset=utf-8></meta><title>x</title></head><body><p data-rq-block="a">1</p></body></html>', // stray </meta>
    '<html><head></br></head><body><p data-rq-block="a">1</p></body></html>', // </br> closes the head
  ]) {
    const r = run(get(), Buffer.from(html));
    assert.equal(r.mode, 'full', html);
    const text = r.body.toString('utf8');
    assert.ok(text.startsWith(MANIFEST), html);
    // The bytes after the manifest element are the page bytes after the head start tag.
    assert.equal(text.slice(text.indexOf('</script>') + '</script>'.length), html.slice('<html><head>'.length), html);
  }
});

test('a changed block with a data:, vbscript: or javascript: URL gives the full document (H-59)', () => {
  const held = page('120.00');
  for (const href of ['data:text/html,x', ' VBScript:x', '\n\tjava\tscript:x','javascript:x']) {
    const r = run(get(clientLines(held)), page(`<a href="${href}">p</a>`));
    assert.equal(r.reason, 'patch-content-rule', href);
  }
  assert.equal(run(get(clientLines(held)), page('<a href="/data:x">p</a>')).mode, 'delta');
});

test('a changed block with script, style or iframe, or an svg a element, gives the full document (H-78)', () => {
  const held = page('120.00');
  for (const content of ['<script>x()</script>', '<style>p{}</style>', '<iframe src="/v"></iframe>', '<svg><a href="#x"><text>t</text></a></svg>']) {
    const r = run(get(clientLines(held)), page(content));
    assert.deepEqual([r.mode, r.reason], ['full', 'patch-content-rule'], content);
  }
  assert.equal(run(get(clientLines(held)), page('<svg><title>t</title><path d="M0 0"/></svg>')).mode, 'delta');
});

test('a changed block with a raster data: image or an ordinary query string gets a delta (H-77, H-83)', () => {
  const held = page('120.00');
  for (const content of ['<img src="data:image/png;base64,iVBO" alt="">', '<a href="?a=1&b=2">p</a>', '<b title="&#169; &quot;">p</b>']) {
    assert.equal(run(get(clientLines(held)), page(content)).mode, 'delta', content);
  }
  for (const content of ['<img src="data:image/svg+xml,x" alt="">', '<img srcset="/a.png 1x, javascript:x 2x" alt="">', '<a href="java&#x73\ncript:x">p</a>']) {
    assert.equal(run(get(clientLines(held)), page(content)).reason, 'patch-content-rule', content);
  }
});
