// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors
//
// The conformance harness: it passes the reference steps on every fixture and
// detects each kind of defect in a server or client response.

import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { cpSync, mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { test } from 'node:test';
import { compareClient, compareServer, runClient, runServer } from '../check.mjs';
import * as client from '../src/client.mjs';
import * as server from '../src/server.mjs';
import { FIXTURES_DIR, fixtureNames, readJsonFile } from '../src/fixtures.mjs';
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

test('DL-036 (I2): Rq-Version on a 304 is optional, and when present exactly one line with the value 1', () => {
  const { expected, reference, page: p } = load('FX-RQ-P05-NOT-MODIFIED');
  assert.deepEqual(expected.headers_optional, { 'rq-version': '1' });
  const add = (...values) => compareServer(expected, p, withHeaders(reference, (h) => [...h, ...values.map((v) => ['rq-version', v])]));
  assert.deepEqual(add(), []);
  assert.deepEqual(add('1'), []);
  assert.deepEqual(add(' 1 '), []); // field values have no outer white space (RFC 9110 section 5.5)
  assert.ok(add('2').includes('header rq-version: "2", expected "1" or absent'));
  assert.ok(add('1;v=2').some((f) => f.startsWith('header rq-version: "1;v=2"')));
  assert.ok(add('1, 1').some((f) => f.startsWith('header rq-version: "1, 1"')));
  assert.ok(add('1', '1').includes('header rq-version: 2 field lines, expected at most one'));
  // F has no headers_optional: there Rq-Version is required.
  assert.equal(load('FX-RQ-P01-FIRST-VISIT').expected.headers_optional, undefined);
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
  assert.ok(ok(text.replace('<!DOCTYPE', '<!doctype')).some((f) => f.includes('before the manifest element')));
  assert.ok(ok(text.replace('<title', '<TITLE')).some((f) => f.includes('after the manifest element')));
  assert.ok(ok(text.replace(m.page_revision, m.template_revision.replace('t1.', 'r1.'))).some((f) => f.includes('manifest value')));
  const moved = text.replace(/<script type="application\/rivqen-manifest\+json">.*?<\/script>/, '').replace('</body>', `<script type="application/rivqen-manifest+json">${JSON.stringify(m)}</script></body>`);
  assert.ok(ok(moved).some((f) => f.includes('at the insertion point')));
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

test('the client fixtures catch a client that ignores H-64, H-65 or the exact origin match of DL-038', async () => {
  const failing = async (change) => {
    const mod = { clientStep: (context, response) => client.clientStep(change(context), response), clientRequest: client.clientRequest };
    return (await runClient(mod)).filter((r) => r.failures.length > 0).map((r) => r.name);
  };
  // H-64: a client that trusts the fields of every response.
  assert.deepEqual(await failing((c) => ({ ...c, response_fields_visible: true })), ['FX-RQ-C24-PUBLIC-FIELDS-HIDDEN', 'FX-RQ-C27-PUBLIC-SNAPSHOT-DELTA-FIELDS-HIDDEN']);
  // H-81: a client that uses the public partition without the app setting.
  assert.deepEqual(await failing((c) => ({ ...c, public_enabled: true })), ['FX-RQ-C24-PUBLIC-NOT-ENABLED']);
  // H-65: a client that forgets the partition of its snapshot.
  assert.deepEqual(await failing((c) => ({ ...c, snapshot: c.snapshot === null ? null : { ...c.snapshot, partition: 'request' } })), ['FX-RQ-C27-PUBLIC-SNAPSHOT-DELTA-STAYS-PUBLIC']);
  // DL-038: a client that matches the allowlist by host suffix, or compares the URL text without serialization.
  const suffix = (c) => ({ ...c, allowlist: c.allowlist.some((o) => new URL(c.final_url).origin.startsWith(`${o}.`)) ? [...c.allowlist, new URL(c.final_url).origin] : c.allowlist });
  assert.deepEqual(await failing(suffix), ['FX-RQ-C25-FINAL-HOST-SUFFIX']);
  const raw = (c) => ({ ...c, allowlist: c.allowlist.includes(c.final_url.split('/').slice(0, 3).join('/')) ? c.allowlist : [] });
  assert.deepEqual(await failing(raw), [
    'FX-RQ-C25-FINAL-BACKSLASH-ALLOWED-HOST',
    'FX-RQ-C25-FINAL-DEFAULT-PORT',
    'FX-RQ-C25-FINAL-HOST-CASE',
    'FX-RQ-C25-IDN-HOST',
    'FX-RQ-C25-USERINFO-FINAL-304',
    'FX-RQ-C25-USERINFO-FINAL-DELTA',
    'FX-RQ-C27-PUBLIC-SNAPSHOT-DELTA-REDIRECT-HOST-CASE',
  ]);
});

test('the client fixtures catch a client that ignores H-67, H-68 or the WHATWG parse of DL-040 L-3', async () => {
  const failing = async (change) => {
    const mod = { clientStep: (context, response) => client.clientStep(change(context), response), clientRequest: (context) => client.clientRequest(change(context)) };
    return (await runClient(mod)).filter((r) => r.failures.length > 0).map((r) => r.name);
  };
  // H-67: a client that forgets the redirect when it chooses the partition (it looks at the final request only).
  assert.deepEqual(await failing((c) => (c.public_origin ? { ...c, url: c.final_url, redirected: false } : c)), [
    'FX-RQ-C24-PUBLIC-REDIRECT',
    'FX-RQ-C24-PUBLIC-REDIRECT-SAME-URL',
    'FX-RQ-C27-PUBLIC-SNAPSHOT-DELTA-REDIRECT',
    'FX-RQ-C27-PUBLIC-SNAPSHOT-DELTA-REDIRECT-HOST-CASE',
  ]);
  // H-68: a client that removes the user information before its checks.
  const strip = (u) => {
    const x = new URL(u);
    x.username = '';
    x.password = '';
    return x.href;
  };
  assert.deepEqual(await failing((c) => ({ ...c, url: strip(c.url), final_url: strip(c.final_url), redirected: c.redirected })), [
    'FX-RQ-C25-USERINFO-FINAL',
    'FX-RQ-C25-USERINFO-FINAL-304',
    'FX-RQ-C25-USERINFO-FINAL-DELTA',
    'FX-RQ-C25-USERINFO-REQUEST',
  ]);
  // DL-040 L-3: a client that parses the URL again with a parser that reads the text before a backslash and "@" as
  // user information and takes the host after it.
  const rfc = (u) => u.replace(/^https:\/\/[^/?#]*\\@/, 'https://');
  assert.deepEqual(await failing((c) => ({ ...c, final_url: rfc(c.final_url) })), ['FX-RQ-C25-FINAL-BACKSLASH-ALLOWED-HOST', 'FX-RQ-C25-FINAL-BACKSLASH-OTHER-HOST']);
});

test('a missing response.body fails that client fixture only', async () => {
  const dir = mkdtempSync(join(tmpdir(), 'rqp-protocol-check-'));
  try {
    for (const name of ['FX-RQ-C01-BASE-MISMATCH', 'FX-RQ-C02-OMITTED-BLOCK']) cpSync(join(FIXTURES_DIR, name), join(dir, name), { recursive: true });
    rmSync(join(dir, 'FX-RQ-C01-BASE-MISMATCH', 'response.body'));
    const results = await runClient(client, { dir });
    assert.deepEqual(results, [
      { name: 'FX-RQ-C01-BASE-MISMATCH', failures: ['response.body is missing'] },
      { name: 'FX-RQ-C02-OMITTED-BLOCK', failures: [] },
    ]);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});

test('a server module that throws fails its fixtures, and the command exits 1', async () => {
  const results = await runServer({ handle: () => { throw new TypeError('x'); } }, { only: 'FX-RQ-P01' });
  assert.ok(results.length > 0 && results.every((r) => r.failures[0] === 'the server threw TypeError'));
  assert.throws(() => execFileSync(process.execPath, [CHECK, '--server', fileURLToPath(new URL('./fixtures/broken-server.mjs', import.meta.url)), '--only', 'FX-RQ-P01'], { stdio: 'pipe' }), (e) => e.status === 1);
  assert.throws(() => execFileSync(process.execPath, [CHECK, '--bad'], { stdio: 'pipe' }), (e) => e.status === 2);
  const out = execFileSync(process.execPath, [CHECK, '--only', 'FX-RQ-P09'], { encoding: 'utf8' });
  assert.match(out, /2 passed, 0 failed, 0 SHOULD failed/);
});

test('a failed SHOULD fixture is labelled and counted apart from a failed MUST fixture', () => {
  const app = fileURLToPath(new URL('./fixtures/app-only-server.mjs', import.meta.url));
  const run = (only) => {
    try {
      return { status: 0, out: execFileSync(process.execPath, [CHECK, '--server', app, '--only', only], { encoding: 'utf8', stdio: 'pipe' }) };
    } catch (e) {
      return { status: e.status, out: e.stdout.toString() };
    }
  };
  const should = run('FX-RQ-P35-ATTRIBUTE-RULE-DATA');
  assert.equal(should.status, 1);
  assert.match(should.out, /^FAIL \[SHOULD\] FX-RQ-P35-ATTRIBUTE-RULE-DATA: /m);
  assert.match(should.out, /0 passed, 0 failed, 1 SHOULD failed/);
  const must = run('FX-RQ-P09-DELTA-WEAK');
  assert.equal(must.status, 1);
  assert.match(must.out, /^FAIL FX-RQ-P09-DELTA-WEAK-BASE: /m);
  assert.match(must.out, /0 passed, 1 failed, 0 SHOULD failed/);
});

test('only the cases of P-35 have level "should" in their expected files', () => {
  const levels = fixtureNames()
    .filter((n) => n.startsWith('FX-RQ-P'))
    .filter((n) => readJsonFile(join(FIXTURES_DIR, n, 'expected.json')).level === 'should');
  assert.deepEqual(levels, [
    'FX-RQ-P35-ATTRIBUTE-RULE',
    'FX-RQ-P35-ATTRIBUTE-RULE-DATA',
    'FX-RQ-P35-ATTRIBUTE-RULE-SVG',
    'FX-RQ-P35-PATCH-CONTENT-IFRAME',
    'FX-RQ-P35-PATCH-CONTENT-SCRIPT',
    'FX-RQ-P35-PATCH-CONTENT-STYLE',
  ]);
});
