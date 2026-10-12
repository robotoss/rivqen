// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors
//
// The conformance harness: it passes the reference steps on every fixture and
// detects each kind of defect in a server or client response.

import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { test } from 'node:test';
import { compareClient, compareServer, runClient, runServer } from '../check.mjs';
import * as client from '../src/client.mjs';
import * as server from '../src/server.mjs';
import { FIXTURES_DIR, readJsonFile } from '../src/fixtures.mjs';
import { page } from './helpers.mjs';

const CHECK = fileURLToPath(new URL('../check.mjs', import.meta.url));
const load = (name) => {
  const c = readJsonFile(join(FIXTURES_DIR, name, 'case.json'));
  const expected = readJsonFile(join(FIXTURES_DIR, name, 'expected.json'));
  const reference = server.handle(expected.request, c.app, page(c.page), expected.server_config ?? {});
  return { c, expected, reference, page: page(c.page) };
};
const withHeaders = (r, f) => ({ ...r, headers: f(r.headers.map(([n, v]) => [n, v])) });

test('the reference server passes every server fixture', async () => {
  const results = await runServer(server);
  assert.ok(results.length >= 57);
  assert.deepEqual(results.filter((r) => r.failures.length > 0), []);
});

test('the reference client passes every client fixture', async () => {
  const results = await runClient(client);
  assert.ok(results.length >= 31);
  assert.deepEqual(results.filter((r) => r.failures.length > 0), []);
});

test('detects a wrong status', () => {
  const { expected, reference, page: p } = load('FX-RQ-P05-NOT-MODIFIED');
  const failures = compareServer(expected, p, { ...reference, status: 200 });
  assert.ok(failures.some((f) => f.startsWith('status 200')), failures.join('; '));
});

test('detects a missing required header and a wrong value', () => {
  const { expected, reference, page: p } = load('FX-RQ-P09-DELTA');
  const missing = compareServer(expected, p, withHeaders(reference, (h) => h.filter(([n]) => n !== 'cache-control')));
  assert.ok(missing.includes('header cache-control: absent, expected "no-store"'), missing.join('; '));
  const wrong = compareServer(expected, p, withHeaders(reference, (h) => h.map(([n, v]) => [n, n === 'rq-version' ? '2' : v])));
  assert.ok(wrong.some((f) => f.startsWith('header rq-version: "2"')));
});

test('detects a forbidden header: ETag on a delta, Rq-Revision, Rq-Version on a passthrough', () => {
  const delta = load('FX-RQ-P09-DELTA');
  assert.ok(compareServer(delta.expected, delta.page, withHeaders(delta.reference, (h) => [...h, ['ETag', '"x"']])).includes('header etag: forbidden'));
  const full = load('FX-RQ-P01-FIRST-VISIT');
  assert.ok(compareServer(full.expected, full.page, withHeaders(full.reference, (h) => [...h, ['rq-revision', 'r1.x']])).includes('header rq-revision: forbidden'));
  const app = load('FX-RQ-P29-INVALID-PAGE-FIRST-VISIT');
  assert.ok(compareServer(app.expected, app.page, withHeaders(app.reference, (h) => [...h, ['rq-version', '1']])).includes('header rq-version: forbidden'));
});

test('detects a wrong changed-block list in a delta', () => {
  const { expected, page: p } = load('FX-RQ-P09-DELTA');
  const all = load('FX-RQ-P10-DELTA-NO-B').reference; // all three blocks for the same pages
  const failures = compareServer(expected, p, all);
  assert.ok(failures.some((f) => f.startsWith('body: changed blocks [title,price,cart], expected [price]')), failures.join('; '));
});

test('detects a delta with a wrong revision, a wrong hash or a schema defect', () => {
  const { expected, reference, page: p } = load('FX-RQ-P09-DELTA');
  const env = JSON.parse(reference.body.toString('utf8'));
  const send = (e) => compareServer(expected, p, { ...reference, body: Buffer.from(JSON.stringify(e)) });
  assert.ok(send({ ...env, next_revision: env.base_revision }).some((f) => f.startsWith('body: next_revision')));
  const op = env.operations[0];
  assert.ok(send({ ...env, operations: [{ ...op, content: 'x' }] }).some((f) => f.includes('does not match its sha256')));
  assert.ok(send({ ...env, page_id: 'x' }).some((f) => f.includes('fails the schema')));
  assert.ok(compareServer(expected, p, { ...reference, body: Buffer.from(JSON.stringify(env).replace('{', '{"type":"rivqen.patch",')) }).some((f) => f.includes('duplicate')));
});

test('compares F by the bytes around the manifest, the position and the manifest value', () => {
  const { expected, reference, page: p } = load('FX-RQ-P01-FIRST-VISIT');
  const text = reference.body.toString('utf8');
  const ok = (body) => compareServer(expected, p, { ...reference, headers: reference.headers.filter(([n]) => n !== 'content-length'), body: Buffer.from(body) });
  // Other white space and member order in the manifest: still conforming.
  const m = JSON.parse(text.split('application/rivqen-manifest+json">')[1].split('</script>')[0]);
  const reordered = { blocks: m.blocks, ...m };
  const pretty = text.replace(JSON.stringify(m), JSON.stringify(reordered, null, 1));
  assert.deepEqual(ok(pretty), []);
  assert.ok(ok(text.replace('Static header', 'Static_header')).some((f) => f.includes('after the manifest element')));
  assert.ok(ok(text.replace('<title', '<TITLE')).some((f) => f.includes('before the manifest element')));
  assert.ok(ok(text.replace(m.page_revision, m.template_revision.replace('t1.', 'r1.'))).some((f) => f.includes('manifest value')));
  const moved = text.replace(/<script type="application\/rivqen-manifest\+json">.*?<\/script>/, '').replace('</body>', `<script type="application/rivqen-manifest+json">${JSON.stringify(m)}</script></body>`);
  assert.ok(ok(moved).some((f) => f.includes('immediately before the first </head>')));
});

test('detects Rq-* or Accept in Vary of F and a wrong Content-Length', () => {
  const { expected, reference, page: p } = load('FX-RQ-P01-FIRST-VISIT');
  const vary = compareServer(expected, p, withHeaders(reference, (h) => [...h, ['vary', 'Accept-Encoding, Rq-Have']]));
  assert.ok(vary.includes('header vary: lists rq-have (RQP-08)'));
  assert.deepEqual(compareServer(expected, p, withHeaders(reference, (h) => [...h, ['vary', 'Accept-Encoding']])), []);
  const length = compareServer(expected, p, withHeaders(reference, (h) => h.map(([n, v]) => [n, n === 'content-length' ? '1' : v])));
  assert.ok(length.some((f) => f.startsWith('header content-length 1')));
});

test('detects a body on a 304 or a HEAD and a changed passthrough body', () => {
  const nm = load('FX-RQ-P05-NOT-MODIFIED');
  assert.ok(compareServer(nm.expected, nm.page, { ...nm.reference, body: Buffer.from('x') }).includes('body: 1 bytes, expected none'));
  const app = load('FX-RQ-P30-POST');
  assert.ok(compareServer(app.expected, app.page, { ...app.reference, body: Buffer.from('x') }).includes('body: not the application response body'));
});

test('accepts headers as a plain object and a body as a string', () => {
  const { expected, reference, page: p } = load('FX-RQ-P09-DELTA');
  const headers = Object.fromEntries(reference.headers);
  assert.deepEqual(compareServer(expected, p, { status: 200, headers, body: reference.body.toString('utf8') }), []);
});

test('client comparison detects a wrong code, next action, snapshot and request field', () => {
  const expected = readJsonFile(join(FIXTURES_DIR, 'FX-RQ-C09-AMP-ACCEPTED', 'expected.json'));
  const good = expected.result;
  assert.deepEqual(compareClient(expected, good, expected.request.headers), []);
  assert.ok(compareClient(expected, { ...good, error: 'PATCH_INVALID' }).some((f) => f.startsWith('error')));
  assert.ok(compareClient(expected, { ...good, next: 'fallback-load' }).some((f) => f.startsWith('next')));
  assert.ok(compareClient(expected, { ...good, snapshot: null }).includes('snapshot differs'));
  const req = expected.request.headers.map(([n, v]) => [n, n === 'rq-have' ? 'x' : v]);
  assert.ok(compareClient(expected, good, req).some((f) => f.startsWith('request rq-have')));
  assert.ok(compareClient(expected, good, [...expected.request.headers, ['rq-base-revision', 'x']]).some((f) => f.includes('not an rqp/1 field')));
});

test('a server module that throws fails its fixtures, and the command exits 1', async () => {
  const results = await runServer({ handle: () => { throw new TypeError('x'); } }, { only: 'FX-RQ-P01' });
  assert.ok(results.length > 0 && results.every((r) => r.failures[0] === 'the server threw TypeError'));
  assert.throws(() => execFileSync(process.execPath, [CHECK, '--server', fileURLToPath(new URL('./fixtures/broken-server.mjs', import.meta.url)), '--only', 'FX-RQ-P01'], { stdio: 'pipe' }), (e) => e.status === 1);
  assert.throws(() => execFileSync(process.execPath, [CHECK, '--bad'], { stdio: 'pipe' }), (e) => e.status === 2);
  const out = execFileSync(process.execPath, [CHECK, '--only', 'FX-RQ-P09'], { encoding: 'utf8' });
  assert.match(out, /2 passed, 0 failed/);
});
