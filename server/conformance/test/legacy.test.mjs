// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// The legacy suite against a fake server on the mini fixtures of
// test/fixtures/legacy (fixtures/legacy/README.md format).

import assert from 'node:assert/strict';
import { cpSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { afterEach, describe, test } from 'node:test';
import { gzipSync } from 'node:zlib';
import { compareLegacy } from '../src/compare.mjs';
import { LEGACY_DIR, legacyResponder, readExpected, runMain, startFake } from './helpers.mjs';

let fake = null;
const temps = [];
afterEach(async () => {
  if (fake !== null) await fake.close();
  fake = null;
  for (const t of temps.splice(0)) rmSync(t, { recursive: true, force: true });
});

async function runLegacy(responderOptions = {}, args = [], fakeOptions = {}) {
  fake = await startFake({ respond: legacyResponder(responderOptions), ...fakeOptions });
  return runMain(['--target', fake.target, '--suite', 'legacy', '--fixtures', responderOptions.dir ?? LEGACY_DIR, ...args]);
}

/** A copy of the mini fixtures that a test may change. */
function tempFixtures() {
  const dir = `${mkdtempSync(join(tmpdir(), 'rivqen-conf-'))}/`;
  temps.push(dir);
  cpSync(LEGACY_DIR, dir, { recursive: true });
  return dir;
}

const edit = (file, f) => writeFileSync(file, JSON.stringify(f(JSON.parse(readFileSync(file, 'utf8')))));
const withField = (response, name, values) => ({ ...response, headers: [...response.headers.filter(([n]) => n.toLowerCase() !== name), ...values.map((v) => [name, v])] });
const onlyFor = (id, change) => (response, kase) => (kase.case_id === id ? change(response, kase) : response);
const reqLine = (raw) => raw.split('\r\n')[0];

describe('legacy suite', () => {
  test('a conforming server passes; a canonical fixture without expected.json is a skip', async () => {
    const r = await runLegacy();
    assert.deepEqual(r.lines, ['PASS FX-LEG-T-304', 'PASS FX-LEG-T-DATA', 'PASS FX-LEG-T-FIRST', 'SKIP FX-LEG-T-PENDING: no expected.json', '3 passed, 0 failed, 0 SHOULD failed, 1 skipped']);
    assert.equal(r.code, 0);
  });

  test('sends_host_first_and_no_extra_lines', async () => {
    const r = await runLegacy({}, ['--only', 'FX-LEG-T-']);
    assert.equal(r.code, 0);
    const raws = fake.state.raw.map((_, i) => fake.rawRequest(i));
    const caseRequests = raws.filter((s) => s.startsWith('GET /catalog'));
    const ea = readExpected(LEGACY_DIR, 'FX-LEG-T-304').fields.etag[0];
    const tag = readExpected(LEGACY_DIR, 'FX-LEG-T-DATA').fields['template-tag'][0];
    assert.deepEqual(caseRequests, [
      `GET /catalog HTTP/1.1\r\nHost: shop.example.com\r\naccept-encoding: gzip\r\naccept-diff: true\r\nif-none-match: ${ea}\r\ntemplate-tag: ${tag}\r\nConnection: close\r\n\r\n`,
      `GET /catalog HTTP/1.1\r\nHost: shop.example.com\r\naccept-encoding: gzip\r\nAccept-Diff: true\r\nif-none-match: ${ea}\r\nif-none-match: ${ea}\r\ntemplate-tag: ${tag}\r\nConnection: close\r\n\r\n`,
      'GET /catalog?sort=price&page=2 HTTP/1.1\r\nHost: shop.example.com\r\naccept-encoding: gzip\r\naccept-diff: true\r\nConnection: close\r\n\r\n',
    ]);
    // Control requests: GET /info first, then POST /case before each case request.
    assert.deepEqual(raws.map(reqLine), [
      'GET /__rivqen/conformance/info HTTP/1.1',
      ...['', '', ''].flatMap((_, i) => ['POST /__rivqen/conformance/case HTTP/1.1', reqLine(caseRequests[i])]),
    ]);
    assert.match(raws[0], new RegExp(`^GET /__rivqen/conformance/info HTTP/1\\.1\\r\\nHost: 127\\.0\\.0\\.1:${fake.port}\\r\\nConnection: close\\r\\n\\r\\n$`));
  });

  test('posts the case: profile, page bytes, app (default), empty server_config', async () => {
    const r = await runLegacy({}, ['--profile', 'legacy-java', '--only', 'FX-LEG-T-DATA']);
    assert.equal(r.code, 0);
    const page = readFileSync(`${LEGACY_DIR}pages/b.html`);
    assert.deepEqual(fake.state.cases, [
      {
        suite: 'legacy',
        case_id: 'FX-LEG-T-DATA--JAVA',
        profile: 'legacy-java',
        page_base64: page.toString('base64'),
        app: { status: 200, headers: [['content-type', 'text/html; charset=utf-8']] },
        server_config: {},
      },
    ]);
    const post = fake.state.raw.map((_, i) => fake.rawRequest(i)).find((s) => s.startsWith('POST '));
    const json = JSON.stringify(fake.state.cases[0]);
    assert.equal(post, `POST /__rivqen/conformance/case HTTP/1.1\r\nHost: 127.0.0.1:${fake.port}\r\nContent-Type: application/json\r\nContent-Length: ${Buffer.byteLength(json)}\r\nConnection: close\r\n\r\n${json}`);
  });

  test('legacy_fields_exact_list', async () => {
    const one = await runLegacy({ change: onlyFor('FX-LEG-T-DATA', (resp) => withField(resp, 'cache-control', ['no-cache'])) }, ['--only', 'FX-LEG-T-DATA']);
    assert.deepEqual(one.lines, ['FAIL FX-LEG-T-DATA: field cache-control: expected ["no-cache, private"], got ["no-cache"]', '0 passed, 1 failed, 0 SHOULD failed, 0 skipped']);
    assert.equal(one.code, 1);
    await fake.close();
    // Two lines with the same combined value are a different list.
    const two = await runLegacy({ change: onlyFor('FX-LEG-T-DATA', (resp) => withField(resp, 'cache-control', ['no-cache', 'private'])) }, ['--only', 'FX-LEG-T-DATA']);
    assert.equal(two.lines[0], 'FAIL FX-LEG-T-DATA: field cache-control: expected ["no-cache, private"], got ["no-cache","private"]');
  });

  test('field names compare case-insensitively', async () => {
    const change = (resp) => ({ ...resp, headers: resp.headers.map(([n, v]) => [n.toUpperCase(), v]) });
    const r = await runLegacy({ change }, ['--only', 'FX-LEG-T-DATA']);
    assert.deepEqual(r.lines, ['PASS FX-LEG-T-DATA', '1 passed, 0 failed, 0 SHOULD failed, 0 skipped']);
  });

  test('legacy_absent_field_detected', async () => {
    const change = onlyFor('FX-LEG-T-DATA', (resp) => withField(withField(resp, 'sonic-etag-key', ['k1']), 'etag', []));
    const r = await runLegacy({ change }, ['--only', 'FX-LEG-T-DATA']);
    const eb = readExpected(LEGACY_DIR, 'FX-LEG-T-DATA').fields.etag[0];
    assert.equal(r.lines[0], `FAIL FX-LEG-T-DATA: field etag: expected ["${eb}"], got []; field sonic-etag-key: expected [], got ["k1"]`);
    assert.equal(r.code, 1);
  });

  test('a status difference fails', async () => {
    const r = await runLegacy({ change: onlyFor('FX-LEG-T-FIRST', (resp) => ({ ...resp, status: 203 })) }, ['--only', 'FX-LEG-T-FIRST']);
    assert.equal(r.lines[0], 'FAIL FX-LEG-T-FIRST: status 203, expected 200');
  });

  test('legacy_gzip_decoded_before_body_check', async () => {
    const r = await runLegacy({ gzip: true });
    assert.deepEqual(r.lines.slice(0, 3), ['PASS FX-LEG-T-304', 'PASS FX-LEG-T-DATA', 'PASS FX-LEG-T-FIRST']);
    assert.equal(r.code, 0);
    // The coding name is case-insensitive; a body that is not gzip fails.
    await fake.close();
    const upper = await runLegacy({ gzip: true, change: (resp) => ({ ...resp, headers: resp.headers.map(([n, v]) => [n, n === 'content-encoding' ? 'GZIP' : v]) }) }, ['--only', 'FX-LEG-T-FIRST']);
    assert.equal(upper.lines[0], 'PASS FX-LEG-T-FIRST');
    await fake.close();
    const broken = await runLegacy({ gzip: true, change: (resp) => ({ ...resp, body: Buffer.from('not gzip') }) }, ['--only', 'FX-LEG-T-FIRST']);
    assert.equal(broken.lines[0], 'FAIL FX-LEG-T-FIRST: body: gzip decoding failed (Z_DATA_ERROR)');
  });

  test('legacy_br_encoding_fails', async () => {
    const r = await runLegacy({ change: onlyFor('FX-LEG-T-FIRST', (resp) => withField(resp, 'content-encoding', ['br'])) }, ['--only', 'FX-LEG-T-FIRST']);
    assert.equal(r.lines[0], 'FAIL FX-LEG-T-FIRST: header content-encoding: ["br"], expected absent or ["gzip"]');
    assert.equal(r.code, 1);
    for (const values of [['identity'], ['gzip', 'gzip'], ['gzip, br']]) {
      const got = compareLegacy({ status: 200, fields: {}, body: { kind: 'page' } }, Buffer.from('x'), { status: 200, fields: values.map((v) => ['Content-Encoding', v]), body: Buffer.from('x') });
      assert.deepEqual(got.must, [`header content-encoding: ${JSON.stringify(values)}, expected absent or ["gzip"]`]);
    }
  });

  test('legacy_data_body_compared_as_json', async () => {
    const expected = readExpected(LEGACY_DIR, 'FX-LEG-T-DATA');
    const { data, template_tag, html_sha1, diff } = expected.body;
    // Other member order, other data key order, escaped "<" (D-08): the same JSON value.
    const same = `{"diff":"","html-sha1":"${html_sha1}","data":{"{price}":"12.00","{title}":"\\u003ctitle>Shop\\u003c/title>"},"template-tag":"${template_tag}"}`;
    const page = Buffer.alloc(0);
    const cmp = (text) => compareLegacy(expected, page, { status: 200, fields: Object.entries(expected.fields).flatMap(([n, vs]) => vs.map((v) => [n, v])), body: Buffer.from(text, 'utf8') }).must;
    assert.deepEqual(cmp(same), []);
    assert.deepEqual(cmp(same.replace('12.00', '13.00')), ['body: data "{price}": expected "12.00", got "13.00"']);
    assert.deepEqual(cmp(JSON.stringify({ data: { ...data, '{extra}': 'x' }, 'template-tag': template_tag, 'html-sha1': html_sha1, diff })), ['body: data "{extra}" is not expected']);
    assert.deepEqual(cmp(JSON.stringify({ data: { '{title}': data['{title}'] }, 'template-tag': template_tag, 'html-sha1': html_sha1, diff })), ['body: data "{price}" is absent']);
    assert.deepEqual(cmp(JSON.stringify({ data, 'template-tag': template_tag, 'html-sha1': 'x', diff, more: 1 })), ['body: unknown member "more"', `body: member html-sha1: expected "${html_sha1}", got "x"`]);
    assert.deepEqual(cmp(JSON.stringify({ data, 'html-sha1': html_sha1, diff })), ['body: member template-tag is absent']);
    assert.deepEqual(cmp(JSON.stringify({ data: [], 'template-tag': template_tag, 'html-sha1': html_sha1, diff })), [`body: member data: expected ${JSON.stringify(data)}, got []`]);
    assert.deepEqual(cmp(`{"diff":"","diff":""}`), ['body: not a valid JSON text (duplicate)']);
    assert.deepEqual(cmp('{"data":'), ['body: not a valid JSON text (syntax)']);
    assert.deepEqual(cmp('[1]'), ['body: not a JSON object']);
    assert.deepEqual(cmp('null'), ['body: not a JSON object']);
    // Through the runner as well.
    const r = await runLegacy({ change: onlyFor('FX-LEG-T-DATA', (resp) => ({ ...resp, body: Buffer.from(same) })) }, ['--only', 'FX-LEG-T-DATA']);
    assert.equal(r.lines[0], 'PASS FX-LEG-T-DATA');
  });

  test('a long value in a problem is shortened', () => {
    const expected = { status: 200, fields: {}, body: { kind: 'data', data: { k: 'v' }, template_tag: 't', html_sha1: 'h', diff: '' } };
    const got = compareLegacy(expected, null, { status: 200, fields: [], body: Buffer.from(JSON.stringify({ data: { k: 'x'.repeat(500) }, 'template-tag': 't', 'html-sha1': 'h', diff: '' })) }).must;
    assert.equal(got.length, 1);
    assert.equal(got[0], `body: data "k": expected "v", got "${'x'.repeat(119)}…`);
  });

  test('legacy_page_body_byte_exact', async () => {
    const flip = (resp) => {
      const body = Buffer.from(resp.body);
      body[20] ^= 0x20;
      return { ...resp, body };
    };
    const r = await runLegacy({ change: onlyFor('FX-LEG-T-FIRST', flip) }, ['--only', 'FX-LEG-T-FIRST']);
    const size = readFileSync(`${LEGACY_DIR}pages/a.html`).length;
    assert.equal(r.lines[0], `FAIL FX-LEG-T-FIRST: body: not the application body (${size} bytes, expected ${size}; first difference at byte 20)`);
    await fake.close();
    const longer = await runLegacy({ change: onlyFor('FX-LEG-T-FIRST', (resp) => ({ ...resp, body: Buffer.concat([resp.body, Buffer.from('\n')]) })) }, ['--only', 'FX-LEG-T-FIRST']);
    assert.equal(longer.lines[0], `FAIL FX-LEG-T-FIRST: body: not the application body (${size + 1} bytes, expected ${size}; first difference at byte ${size})`);
  });

  test('body kinds none and a missing body', () => {
    const none = { status: 200, fields: {}, body: { kind: 'none' } };
    assert.deepEqual(compareLegacy(none, null, { status: 200, fields: [], body: Buffer.from('abc') }).must, ['body: 3 bytes, expected none']);
    assert.deepEqual(compareLegacy(none, null, { status: 200, fields: [], body: Buffer.alloc(0) }).must, []);
    assert.deepEqual(compareLegacy(none, null, { status: 200, fields: [], body: null }).must, []);
    for (const kind of ['page', 'data']) {
      assert.deepEqual(compareLegacy({ status: 200, fields: {}, body: { kind } }, Buffer.from('p'), { status: 200, fields: [], body: null }).must, [`body: none, expected ${kind}`]);
    }
  });

  test('a gzip body larger than the limit fails without a crash', () => {
    const big = gzipSync(Buffer.alloc(4096, 0x61));
    const got = compareLegacy({ status: 200, fields: {}, body: { kind: 'page' } }, Buffer.alloc(4096, 0x61), { status: 200, fields: [['content-encoding', 'gzip']], body: big }, { bodyLimit: 1024 }).must;
    assert.deepEqual(got, ['body: the decoded body is larger than 1024 bytes']);
  });

  test('content_length_mismatch_fails', async () => {
    const lie = (delta) => (resp) => ({ raw: Buffer.concat([Buffer.from(`HTTP/1.1 200 OK\r\netag: ${resp.headers.find(([n]) => n === 'etag')[1]}\r\nContent-Length: ${resp.body.length + delta}\r\n\r\n`), resp.body]) });
    const short = await runLegacy({ change: onlyFor('FX-LEG-T-FIRST', lie(5)) }, ['--only', 'FX-LEG-T-FIRST']);
    const size = readFileSync(`${LEGACY_DIR}pages/a.html`).length;
    assert.equal(short.lines[0], `FAIL FX-LEG-T-FIRST: the connection closed before the end of the body (${size} bytes received)`);
    assert.equal(short.code, 1);
    await fake.close();
    const long = await runLegacy({ change: onlyFor('FX-LEG-T-FIRST', lie(-3)) }, ['--only', 'FX-LEG-T-FIRST']);
    assert.match(long.lines[0], /^FAIL FX-LEG-T-FIRST: 3 bytes after the end of the response; /);
    assert.equal(long.code, 1);
  });

  test('a malformed response is a failed case and the run goes on', async () => {
    const r = await runLegacy({ change: onlyFor('FX-LEG-T-304', () => ({ raw: Buffer.from('HTTP/1.1 304\r\nbad line\r\n\r\n') })) });
    assert.equal(r.lines[0], 'FAIL FX-LEG-T-304: malformed response: field line without a name "bad line"');
    assert.equal(r.lines[1], 'PASS FX-LEG-T-DATA');
    assert.equal(r.code, 1);
  });

  test('timeout_is_a_fail_not_a_hang', async () => {
    const t0 = Date.now();
    const r = await runLegacy({ change: onlyFor('FX-LEG-T-304', () => ({ hang: true })) }, ['--timeout-ms', '200']);
    assert.deepEqual(r.lines.slice(0, 3), ['FAIL FX-LEG-T-304: no complete response within 200 ms', 'PASS FX-LEG-T-DATA', 'PASS FX-LEG-T-FIRST']);
    assert.equal(r.code, 1);
    assert.ok(Date.now() - t0 < 5000);
  });
});

describe('profile selection', () => {
  const idsOf = (r) => r.lines.slice(0, -1).map((l) => l.split(' ')[1].replace(/:$/, ''));

  test('profile_runs_only_confirmed_fixtures', async () => {
    const runs = {};
    for (const profile of ['legacy-canonical', 'legacy-java', 'legacy-node', 'legacy-php']) {
      runs[profile] = idsOf(await runLegacy({}, ['--profile', profile]));
      await fake.close();
      fake = null;
    }
    assert.deepEqual(runs, {
      'legacy-canonical': ['FX-LEG-T-304', 'FX-LEG-T-DATA', 'FX-LEG-T-FIRST', 'FX-LEG-T-PENDING'],
      // T-304 lists legacy-java; T-DATA is replaced by its --JAVA fixture; T-FIRST does not list legacy-java.
      'legacy-java': ['FX-LEG-T-304', 'FX-LEG-T-DATA--JAVA', 'FX-LEG-T-PENDING'],
      'legacy-node': ['FX-LEG-T-FIRST', 'FX-LEG-T-PENDING'],
      // T-FIRST lists legacy-php, but FX-LEG-T-FIRST--PHP replaces it.
      'legacy-php': ['FX-LEG-T-304', 'FX-LEG-T-FIRST--PHP', 'FX-LEG-T-PENDING'],
    });
  });

  test('a profile fixture takes page and app of the replaced fixture', async () => {
    const dir = tempFixtures();
    edit(`${dir}FX-LEG-T-FIRST/case.json`, (c) => ({ ...c, app: { status: 200, headers: [['content-type', 'text/html'], ['x-app', '1']] } }));
    const r = await runLegacy({ dir }, ['--profile', 'legacy-php', '--only', 'FX-LEG-T-FIRST']);
    assert.equal(r.lines[0], 'PASS FX-LEG-T-FIRST--PHP');
    assert.deepEqual(fake.state.cases[0].app, { status: 200, headers: [['content-type', 'text/html'], ['x-app', '1']] });
    assert.equal(fake.state.cases[0].page_base64, readFileSync(`${dir}pages/a.html`).toString('base64'));
  });

  test('a name that is not a fixture name of the README is not selected', async () => {
    const dir = tempFixtures();
    mkdirSync(`${dir}FX-LEG-T-ODD--GO`);
    writeFileSync(`${dir}FX-LEG-T-FILE`, '');
    const r = await runLegacy({ dir });
    assert.deepEqual(idsOf(r), ['FX-LEG-T-304', 'FX-LEG-T-DATA', 'FX-LEG-T-FIRST', 'FX-LEG-T-PENDING']);
  });

  test('--only selects by id prefix', async () => {
    const r = await runLegacy({}, ['--only', 'FX-LEG-T-F']);
    assert.deepEqual(idsOf(r), ['FX-LEG-T-FIRST']);
  });
});

describe('fixture defects and levels', () => {
  test('a fixture defect fails that case only', async () => {
    const dir = tempFixtures();
    writeFileSync(`${dir}FX-LEG-T-304/expected.json`, '{ not json');
    edit(`${dir}FX-LEG-T-DATA--JAVA/case.json`, (c) => ({ ...c, replaces: 'FX-LEG-OTHER' }));
    edit(`${dir}FX-LEG-T-FIRST/case.json`, (c) => ({ ...c, page: '../a' }));
    edit(`${dir}FX-LEG-T-DATA/expected.json`, (e) => ({ ...e, body: { kind: 'html' } }));
    const canonical = await runLegacy({ dir });
    assert.deepEqual(canonical.lines.slice(0, 3), [
      'FAIL FX-LEG-T-304: fixture: FX-LEG-T-304/expected.json: not valid JSON',
      'FAIL FX-LEG-T-DATA: fixture: FX-LEG-T-DATA/expected.json: body.kind must be page, data or none',
      'FAIL FX-LEG-T-FIRST: fixture: FX-LEG-T-FIRST/case.json: page "../a" is not a page id',
    ]);
    assert.equal(canonical.code, 1);
    await fake.close();
    const java = await runLegacy({ dir }, ['--profile', 'legacy-java', '--only', 'FX-LEG-T-DATA']);
    assert.equal(java.lines[0], 'FAIL FX-LEG-T-DATA--JAVA: fixture: FX-LEG-T-DATA--JAVA/case.json: replaces must be FX-LEG-T-DATA');
  });

  test('more fixture defects', async () => {
    const dir = tempFixtures();
    edit(`${dir}FX-LEG-T-304/expected.json`, (e) => ({ ...e, level: 'may' }));
    edit(`${dir}FX-LEG-T-DATA/expected.json`, (e) => ({ ...e, fields: { etag: 'x' } }));
    edit(`${dir}FX-LEG-T-FIRST/case.json`, (c) => ({ ...c, page: 'missing' }));
    const r = await runLegacy({ dir });
    assert.deepEqual(r.lines.slice(0, 3), [
      'FAIL FX-LEG-T-304: fixture: FX-LEG-T-304/expected.json: level "may"',
      'FAIL FX-LEG-T-DATA: fixture: FX-LEG-T-DATA/expected.json: fields must map names to lists of strings',
      'FAIL FX-LEG-T-FIRST: fixture: FX-LEG-T-FIRST: cannot read pages/missing.html (ENOENT)',
    ]);
    await fake.close();
    const dir2 = tempFixtures();
    edit(`${dir2}FX-LEG-T-304/expected.json`, (e) => ({ ...e, status: '304' }));
    edit(`${dir2}FX-LEG-T-DATA/expected.json`, (e) => ({ ...e, request: { method: 'GET', headers: [] } }));
    edit(`${dir2}FX-LEG-T-FIRST/case.json`, (c) => ({ ...c, app: { status: 200, headers: [['a']] } }));
    const r2 = await runLegacy({ dir: dir2 });
    assert.deepEqual(r2.lines.slice(0, 3), [
      'FAIL FX-LEG-T-304: fixture: FX-LEG-T-304/expected.json: status must be an integer',
      'FAIL FX-LEG-T-DATA: fixture: FX-LEG-T-DATA/expected.json: request must have method, target and headers',
      'FAIL FX-LEG-T-FIRST: fixture: FX-LEG-T-FIRST/case.json: app must have status and headers',
    ]);
  });

  test('a case over the 8 MiB limit of POST /case fails without a request', async () => {
    const dir = tempFixtures();
    // base64 of 6 MiB + 3 bytes and the other members: just over 8 MiB.
    writeFileSync(`${dir}pages/a.html`, Buffer.alloc(6 * 1024 * 1024 + 3, 0x61));
    const r = await runLegacy({ dir }, ['--only', 'FX-LEG-T-FIRST']);
    assert.match(r.lines[0], /^FAIL FX-LEG-T-FIRST: fixture: the case is 838\d{4} bytes, more than the 8388608 bytes of POST \/__rivqen\/conformance\/case$/);
    assert.equal(fake.state.cases.length, 0);
    // Just under the limit is sent.
    writeFileSync(`${dir}pages/a.html`, Buffer.alloc(6 * 1024 * 1024 - 300, 0x61));
    await fake.close();
    const ok = await runLegacy({ dir, change: (resp) => resp }, ['--only', 'FX-LEG-T-FIRST']);
    assert.equal(ok.lines[0], 'PASS FX-LEG-T-FIRST');
  });

  test('a request target that is not absolute fails the case', async () => {
    const dir = tempFixtures();
    edit(`${dir}FX-LEG-T-FIRST/expected.json`, (e) => ({ ...e, request: { ...e.request, target: '/catalog' } }));
    const r = await runLegacy({ dir }, ['--only', 'FX-LEG-T-FIRST']);
    assert.equal(r.lines[0], 'FAIL FX-LEG-T-FIRST: fixture: request target "/catalog" is not an absolute URL with a host');
  });

  test('a SHOULD case that fails is counted apart and does not fail the run', async () => {
    const dir = tempFixtures();
    edit(`${dir}FX-LEG-T-FIRST/expected.json`, (e) => ({ ...e, level: 'should' }));
    const r = await runLegacy({ dir, change: onlyFor('FX-LEG-T-FIRST', (resp) => ({ ...resp, status: 201 })) });
    assert.deepEqual(r.lines, ['PASS FX-LEG-T-304', 'PASS FX-LEG-T-DATA', 'FAIL [SHOULD] FX-LEG-T-FIRST: status 201, expected 200', 'SKIP FX-LEG-T-PENDING: no expected.json', '2 passed, 0 failed, 1 SHOULD failed, 1 skipped']);
    assert.equal(r.code, 0);
  });

  test('a case that the server does not store fails', async () => {
    const r = await runLegacy({}, ['--only', 'FX-LEG-T-DATA'], { caseStatus: () => 422 });
    assert.equal(r.lines[0], 'FAIL FX-LEG-T-DATA: POST /__rivqen/conformance/case: status 422 (the server does not support this case)');
    assert.equal(fake.state.requests.length, 0);
    await fake.close();
    const bad = await runLegacy({}, ['--only', 'FX-LEG-T-DATA'], { caseStatus: () => 400 });
    assert.equal(bad.lines[0], 'FAIL FX-LEG-T-DATA: POST /__rivqen/conformance/case: status 400 (the server did not store the case)');
  });
});
