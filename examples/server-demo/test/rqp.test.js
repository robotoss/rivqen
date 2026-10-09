// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { analyze, buildPatch, findBlocks, injectManifest, manifest, readRequest, RqpError } from '../src/rqp.js';

const page = (price, extra = '') => `<!DOCTYPE html><html><head><title data-rq-block="title">Shop</title></head>` +
  `<body>${extra}<p data-rq-block="price">${price}</p><script type="application/json" data-rq-block="cart">{"n":1}</script></body></html>`;

test('finds blocks in document order with formats', () => {
  const blocks = findBlocks(page('120.00'));
  assert.deepEqual(blocks.map((b) => [b.id, b.format]), [['title', 'html'], ['price', 'html'], ['cart', 'json']]);
});

test('template excludes block content; revisions are stable', () => {
  const a = analyze(page('120.00'));
  const b = analyze(page('130.00'));
  assert.equal(a.templateRevision, b.templateRevision);
  assert.notEqual(a.pageRevision, b.pageRevision);
  assert.ok(!a.template.includes('120.00'));
});

test('template change changes the template revision', () => {
  assert.notEqual(analyze(page('1')).templateRevision, analyze(page('1', '<nav>new</nav>')).templateRevision);
});

test('patch contains only changed blocks', () => {
  const base = analyze(page('120.00'));
  const cur = analyze(page('130.00'));
  const p = buildPatch({ pageId: 'x', base, current: cur, sequence: 1 });
  assert.deepEqual(p.operations.map((o) => o.block_id), ['price']);
  assert.equal(p.base_revision, base.pageRevision);
  assert.equal(p.next_revision, cur.pageRevision);
});

test('no patch across template changes', () => {
  assert.equal(buildPatch({ pageId: 'x', base: analyze(page('1')), current: analyze(page('1', '<nav>n</nav>')), sequence: 1 }), null);
});

for (const [name, html, code] of [
  ['duplicate', '<html><head></head><body><p data-rq-block="a">1</p><p data-rq-block="a">2</p></body></html>', 'RQP_MARKUP_DUPLICATE'],
  ['nested', '<html><head></head><body><div data-rq-block="a"><p data-rq-block="b">1</p></div></body></html>', 'RQP_MARKUP_NESTED'],
  ['bad id', '<html><head></head><body><p data-rq-block="Price">1</p></body></html>', 'RQP_MARKUP_INVALID_ID'],
  ['forbidden', '<html><head></head><body><textarea data-rq-block="t">1</textarea></body></html>', 'RQP_MARKUP_FORBIDDEN_ELEMENT'],
  ['script', '<html><head></head><body><script data-rq-block="s">x()</script></body></html>', 'RQP_MARKUP_FORBIDDEN_ELEMENT'],
  ['void', '<html><head></head><body><img data-rq-block="i"></body></html>', 'RQP_MARKUP_FORBIDDEN_ELEMENT'],
  ['bad json', '<html><head></head><body><script type="application/json" data-rq-block="j">{x</script></body></html>', 'RQP_MARKUP_BAD_JSON'],
]) {
  test(`rejects ${name} markup with ${code}`, () => {
    assert.throws(() => analyze(html), (e) => e instanceof RqpError && e.code === code);
  });
}

test('manifest has no content and is injected inert', () => {
  const a = analyze(page('<b>1</b>'));
  const m = manifest({ origin: 'https://shop.example.com', path: '/c', analysis: a, policy: {} });
  assert.ok(!JSON.stringify(m).includes('<b>'));
  const out = injectManifest(page('1'), m);
  assert.match(out, /<script type="application\/rivqen-manifest\+json" id="rq-manifest">/);
});

test('reads RQP request headers', () => {
  const r = readRequest({ 'rq-version': '1', 'rq-capabilities': '(block-patch manifest)', 'rq-base-revision': '"rev-1"', 'if-none-match': 'W/"rev-0", "rev-2"' });
  assert.equal(r.isRqp, true);
  assert.ok(r.caps.has('block-patch'));
  assert.equal(r.base, 'rev-1');
  assert.ok(r.ifNoneMatch.has('rev-0') && r.ifNoneMatch.has('rev-2'));
});
