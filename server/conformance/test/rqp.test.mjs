// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// The RQP suite on the real fixtures of fixtures/rqp/protocol, against a fake
// server that runs the reference server step of tools/rqp-protocol (the way
// the harness of CONTRACT.md section 2 does: the target is the public origin
// plus the request path).

import assert from 'node:assert/strict';
import { cpSync, mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { afterEach, describe, test } from 'node:test';
import { handle } from '../../../tools/rqp-protocol/src/server.mjs';
import { compareRqp } from '../src/compare.mjs';
import { RQP_DIR, runMain, startFake } from './helpers.mjs';

let fake = null;
afterEach(async () => {
  if (fake !== null) await fake.close();
  fake = null;
});

const CONTROL = new Set(['host', 'connection']);

/** The reference server step behind the fake; `change` edits its result. */
function reference(change = (r) => r) {
  return (request, kase) => {
    const req = { method: request.method, target: `https://shop.example.com${request.url}`, headers: request.fields.filter(([n]) => !CONTROL.has(n.toLowerCase())) };
    const r = change(handle(req, kase.app, Buffer.from(kase.page_base64, 'base64'), kase.server_config), req);
    return { status: r.status, headers: r.headers, body: r.body === null ? null : Buffer.from(r.body) };
  };
}

async function runRqp(change, args = []) {
  fake = await startFake({ respond: reference(change) });
  return runMain(['--target', fake.target, '--suite', 'rqp', ...args]);
}

const serverCases = readdirSync(RQP_DIR).filter((n) => n.startsWith('FX-RQ-P')).sort();

describe('rqp suite', () => {
  test('rqp_suite_uses_compare_server', async () => {
    // The reference server passes every server case; no client case runs.
    const all = await runRqp();
    assert.equal(all.code, 0, all.out);
    assert.equal(all.lines.length, serverCases.length + 1);
    assert.deepEqual(all.lines.slice(0, -1), serverCases.map((id) => `PASS ${id}`));
    assert.equal(all.lines.at(-1), `${serverCases.length} passed, 0 failed, 0 SHOULD failed, 0 skipped`);
    await fake.close();
    // A defect gives the failure text of compareServer.
    const broken = await runRqp((r) => ({ ...r, headers: r.headers.filter(([n]) => n !== 'rq-version') }), ['--only', 'FX-RQ-P01-FIRST-VISIT']);
    assert.deepEqual(broken.lines, ['FAIL FX-RQ-P01-FIRST-VISIT: header rq-version: absent, expected "1"', '0 passed, 1 failed, 0 SHOULD failed, 0 skipped']);
    assert.equal(broken.code, 1);
  });

  test('a SHOULD rule of compareServerShould is counted apart', async () => {
    const noVary = (r) => (r.kind === 'delta' ? { ...r, headers: r.headers.filter(([n, v]) => !(n === 'vary' && v === 'Rq-Nonce')) } : r);
    const r = await runRqp(noVary, ['--only', 'FX-RQ-P09-DELTA']);
    assert.ok(r.lines.includes('FAIL [SHOULD] FX-RQ-P09-DELTA: header vary: does not list rq-nonce'), r.out);
    assert.match(r.lines.at(-1), /^\d+ passed, 0 failed, [1-9]\d* SHOULD failed, 0 skipped$/);
    assert.equal(r.code, 0);
  });

  test('a MUST failure also lists the SHOULD problems of the case', async () => {
    const both = (r) => (r.kind === 'delta' ? { ...r, status: 299, headers: r.headers.filter(([n, v]) => !(n === 'vary' && v === 'Rq-Nonce')) } : r);
    const r = await runRqp(both, ['--only', 'FX-RQ-P09-DELTA-QUERY']);
    assert.equal(r.lines[0], 'FAIL FX-RQ-P09-DELTA-QUERY: status 299, expected 200; [SHOULD] header vary: does not list rq-nonce');
  });

  test('a case of level should fails as SHOULD', async () => {
    const id = serverCases.find((n) => JSON.parse(readFileSync(`${RQP_DIR}${n}/expected.json`, 'utf8')).level === 'should');
    const r = await runRqp((x) => ({ ...x, status: 299 }), ['--only', id]);
    assert.match(r.lines[0], new RegExp(`^FAIL \\[SHOULD\\] ${id}: status 299, expected `));
    assert.equal(r.code, 0);
  });

  test('sends the request path with the query and posts the RQP case members', async () => {
    const r = await runRqp(undefined, ['--only', 'FX-RQ-P09-DELTA-QUERY']);
    assert.equal(r.code, 0);
    const raw = fake.state.raw.map((_, i) => fake.rawRequest(i)).find((s) => !s.includes('/__rivqen/'));
    const expected = JSON.parse(readFileSync(`${RQP_DIR}FX-RQ-P09-DELTA-QUERY/expected.json`, 'utf8'));
    const lines = expected.request.headers.map(([n, v]) => `${n}: ${v}\r\n`).join('');
    assert.equal(raw, `GET /catalog?sort=price&page=2 HTTP/1.1\r\nHost: shop.example.com\r\n${lines}Connection: close\r\n\r\n`);
    await fake.close();
    await runRqp(undefined, ['--only', 'FX-RQ-P36-POLICY-ONLY']);
    const kase = JSON.parse(readFileSync(`${RQP_DIR}FX-RQ-P36-POLICY-ONLY/case.json`, 'utf8'));
    const page = readFileSync(`${RQP_DIR}pages/${kase.page}.html`);
    assert.deepEqual(fake.state.cases, [{ suite: 'rqp', case_id: 'FX-RQ-P36-POLICY-ONLY', page_base64: page.toString('base64'), app: kase.app, server_config: { policy: { ttl_seconds: 60 } } }]);
    await fake.close();
    await runRqp(undefined, ['--only', 'FX-RQ-P01-FIRST-VISIT']);
    assert.deepEqual(fake.state.cases[0].server_config, {});
    assert.equal(Object.hasOwn(fake.state.cases[0], 'profile'), false);
  });

  test('HEAD and POST cases are sent with their method', async () => {
    const r = await runRqp(undefined, ['--only', 'FX-RQ-P25-HEAD-FULL']);
    assert.equal(r.lines[0], 'PASS FX-RQ-P25-HEAD-FULL');
    assert.deepEqual(fake.state.requests.map((q) => q.method), ['HEAD']);
  });

  test('an RQP fixture defect fails that case only', async () => {
    const dir = `${mkdtempSync(join(tmpdir(), 'rivqen-rqp-'))}/`;
    try {
      for (const id of ['FX-RQ-P01-FIRST-VISIT', 'FX-RQ-P02-BROWSER', 'FX-RQ-P03-VERSION-2']) cpSync(`${RQP_DIR}${id}`, `${dir}${id}`, { recursive: true });
      cpSync(`${RQP_DIR}pages/page-a.html`, `${dir}pages/page-a.html`);
      cpSync(`${RQP_DIR}FX-RQ-C01-OTHER-BASE`, `${dir}FX-RQ-C01-OTHER-BASE`, { recursive: true });
      const e = JSON.parse(readFileSync(`${dir}FX-RQ-P02-BROWSER/expected.json`, 'utf8'));
      delete e.headers_required;
      writeFileSync(`${dir}FX-RQ-P02-BROWSER/expected.json`, JSON.stringify(e));
      writeFileSync(`${dir}FX-RQ-P03-VERSION-2/case.json`, '[');
      const r = await runRqp(undefined, ['--fixtures', dir]);
      assert.deepEqual(r.lines, [
        'PASS FX-RQ-P01-FIRST-VISIT',
        'FAIL FX-RQ-P02-BROWSER: fixture: FX-RQ-P02-BROWSER/expected.json: not a server case',
        'FAIL FX-RQ-P03-VERSION-2: fixture: FX-RQ-P03-VERSION-2/case.json: not valid JSON',
        '1 passed, 2 failed, 0 SHOULD failed, 0 skipped',
      ]);
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });

  test('a content coding fails an RQP case', () => {
    const expected = JSON.parse(readFileSync(`${RQP_DIR}FX-RQ-P30-APP-404/expected.json`, 'utf8'));
    const kase = JSON.parse(readFileSync(`${RQP_DIR}FX-RQ-P30-APP-404/case.json`, 'utf8'));
    const page = readFileSync(`${RQP_DIR}pages/${kase.page}.html`);
    const r = handle(expected.request, kase.app, page, {});
    const response = (extra) => ({ status: r.status, fields: [...r.headers, ...extra], body: Buffer.from(r.body) });
    assert.deepEqual(compareRqp(expected, page, response([])).must, []);
    assert.deepEqual(compareRqp(expected, page, response([['Content-Encoding', 'identity']])).must, []);
    assert.deepEqual(compareRqp(expected, page, response([['Content-Encoding', 'gzip']])).must, ['header content-encoding: ["gzip"], expected no content coding']);
    assert.deepEqual(compareRqp(expected, page, response([['content-encoding', 'identity, br']])).must, ['header content-encoding: ["identity, br"], expected no content coding']);
  });

  test('a comparison that throws is a failure, not a crash', () => {
    const got = compareRqp({ status: 200 }, Buffer.alloc(0), { status: 200, fields: [], body: null });
    assert.deepEqual(got, { must: ['the RQP comparison stopped (TypeError)'], should: [] });
  });
});
