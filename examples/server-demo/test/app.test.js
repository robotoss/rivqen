// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors
//
// The HTTP adapter on a loopback socket: the scenario of docs/guide/examples/server.md
// (first visit, revisit, data change, template change) with real header lines.
import { test, before, after } from 'node:test';
import assert from 'node:assert/strict';
import { request } from 'node:http';

process.env.LOG_LEVEL = 'error';
const { createDemoServer } = await import('../src/app.js');
const { analyzePage } = await import('../src/markup.js');
const { revisionsOf } = await import('./helpers.js');

const ORIGIN = 'https://shop.example.com';
const ACTION = [['Rq-Demo-Action', '1']]; // the POST endpoints need this header (DL-033)
let server;
let base;

before(async () => {
  server = createDemoServer({ origin: ORIGIN });
  await new Promise((resolve) => server.listen(0, '127.0.0.1', resolve));
  base = `http://127.0.0.1:${server.address().port}`;
});
after(() => new Promise((resolve) => server.close(resolve)));

/** One request with exact header lines (fetch cannot send a field twice; an array of headers needs its own Host). */
function call(method, path, headers = []) {
  return new Promise((resolve, reject) => {
    const req = request(`${base}${path}`, { method, headers: [['Host', new URL(ORIGIN).host], ...headers].flat() }, (res) => {
      const chunks = [];
      res.on('data', (c) => chunks.push(c));
      res.on('end', () => resolve({ status: res.statusCode, headers: res.headers, raw: res.rawHeaders, body: Buffer.concat(chunks) }));
    });
    req.on('error', reject);
    req.end();
  });
}

const delta = (page_revision, template) => [
  ['Rq-Version', '1'],
  ['Rq-Capabilities', '(block-patch)'],
  ['Accept', 'text/html, application/vnd.rivqen.delta+json'],
  ['If-None-Match', `"${page_revision}"`],
  ['Rq-Have', `t=${template}`],
];

/** The original document of a full response: the manifest element removed (H-103: the client computes the rest). */
const originalOf = (body) => Buffer.from(body.toString('utf8').replace(/<script type="application\/rivqen-manifest\+json">[^<]*<\/script>/, ''), 'utf8');

const manifestOf = (body) => {
  const text = body.toString('utf8');
  const m = /<head><script type="application\/rivqen-manifest\+json">([^<]*)<\/script>/.exec(text);
  assert.ok(m, 'the manifest element is immediately after <head> (H-72)');
  return JSON.parse(m[1]);
};

test('first visit, revisit, data change and template change follow rqp/1', async () => {
  // 1. First visit: the full document with the manifest, a strong ETag and no Vary.
  const first = await call('GET', '/catalog', [['Rq-Version', '1']]);
  assert.equal(first.status, 200);
  const manifest = manifestOf(first.body);
  assert.equal(manifest.origin, ORIGIN); // the public origin of the configuration, not the connection
  assert.equal(manifest.path, '/catalog');
  assert.deepEqual(Object.keys(manifest), ['protocol', 'origin', 'path', 'page_revision']); // H-103: no default members
  const original = revisionsOf(originalOf(first.body));
  assert.equal(manifest.page_revision, original.page_revision);
  assert.equal(first.headers.etag, `"${manifest.page_revision}"`);
  assert.equal(first.headers['rq-version'], '1');
  assert.equal(first.headers.vary, undefined);
  assert.equal(first.headers['cache-control'], 'no-cache');
  assert.equal(first.headers['content-length'], String(first.body.length));
  const names = first.raw.filter((_, i) => i % 2 === 0).map((n) => n.toLowerCase());
  assert.ok(!names.includes('rq-revision') && !names.includes('rq-template-revision'));
  assert.ok(analyzePage(first.body).valid === false, 'the manifest element makes the full document differ from the page'); // M-13

  // 2. Revisit with nothing changed: 304 without a body.
  const again = await call('GET', '/catalog', delta(manifest.page_revision, original.template_revision));
  assert.equal(again.status, 304);
  assert.equal(again.body.length, 0);
  assert.equal(again.headers.etag, first.headers.etag);

  // 3. Data change: a delta with the changed blocks and no ETag. Without b, every block is in it (H-7).
  assert.equal((await call('POST', '/__demo/next-data', ACTION)).status, 200);
  const patch = await call('GET', '/catalog', delta(manifest.page_revision, original.template_revision));
  assert.equal(patch.status, 200);
  assert.equal(patch.headers['content-type'], 'application/vnd.rivqen.delta+json');
  assert.equal(patch.headers['cache-control'], 'no-store');
  assert.equal(patch.headers.etag, undefined);
  const envelope = JSON.parse(patch.body);
  assert.deepEqual(Object.keys(envelope), ['binding', 'operations']); // H-104, H-107
  assert.match(envelope.binding, /^d1\.[A-Za-z0-9_-]{43}$/)
  assert.deepEqual(envelope.operations.map((o) => o.block_id).sort(), ['cart', 'price', 'stock', 'title']);
  assert.ok(patch.body.length < first.body.length / 10);

  // 4. Template change: the full document with a new template revision.
  assert.equal((await call('POST', '/__demo/next-template', ACTION)).status, 200);
  const changed = await call('GET', '/catalog', delta(manifest.page_revision, original.template_revision));
  assert.equal(changed.status, 200);
  assert.match(changed.headers['content-type'], /^text\/html/);
  assert.notEqual(revisionsOf(originalOf(changed.body)).template_revision, original.template_revision);
});

test('a client without RQP gets the same bytes as an RQP client without a base', async () => {
  const plain = await call('GET', '/catalog');
  const rqp = await call('GET', '/catalog', [['Rq-Version', '1'], ['Rq-Capabilities', '(block-patch)']]);
  assert.deepEqual(plain.body, rqp.body);
  assert.equal(plain.headers.etag, rqp.headers.etag);
});

test('HEAD gives the headers of the full document without a body', async () => {
  const head = await call('HEAD', '/catalog');
  const get = await call('GET', '/catalog');
  assert.equal(head.status, 200);
  assert.equal(head.body.length, 0);
  assert.equal(head.headers.etag, get.headers.etag);
  assert.equal(head.headers['content-length'], String(get.body.length));
});

test('two lines of a field reach the server as two lines, and the other routes answer', async () => {
  const two = await call('GET', '/catalog', [['Rq-Capabilities', '(block-patch)'], ['Rq-Capabilities', '(realtime-ws)']]);
  assert.equal(two.status, 200);
  assert.equal((await call('GET', '/healthz')).status, 200);
  assert.equal((await call('GET', '/nope')).status, 404);
  assert.equal((await call('POST', '/catalog', ACTION)).status, 404);
});
