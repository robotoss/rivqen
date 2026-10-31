// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

import assert from 'node:assert/strict';
import http from 'node:http';
import net from 'node:net';
import { after, describe, test } from 'node:test';
import { fileURLToPath } from 'node:url';
import {
  createHarness,
  controlGate,
  isControlHost,
  isJsonContentType,
  isLoopback,
  listen,
  loadHandle,
  parseCli,
  MAX_CASE_BYTES,
} from '../harness.mjs';

const servers = [];
after(() => {
  for (const s of servers) {
    s.close();
    s.closeAllConnections();
  }
});

async function start(handle, options = {}) {
  const server = createHarness({ handle, suite: 'legacy', conformance: true, ...options });
  const lines = [];
  await listen(server, 0, { write: (t) => lines.push(t) });
  servers.push(server);
  return { port: server.address().port, lines };
}

/** Send raw text, read until close. Returns { status, lines: [[name, value]], body: Buffer, head }. */
function raw(port, text) {
  return new Promise((resolve, reject) => {
    const sock = net.connect(port, '127.0.0.1', () => sock.write(text.replace('Host: %HOST%', `Host: 127.0.0.1:${port}`)));
    const chunks = [];
    sock.on('data', (c) => chunks.push(c));
    sock.on('error', () => {}); // the server may close early (413); the close event ends the read
    sock.on('close', () => {
      const all = Buffer.concat(chunks);
      const end = all.indexOf('\r\n\r\n');
      const head = all.subarray(0, end).toString('latin1').split('\r\n');
      resolve({
        status: Number(head[0].split(' ')[1]),
        lines: head.slice(1).map((l) => [l.slice(0, l.indexOf(':')), l.slice(l.indexOf(':') + 1).trim()]),
        body: all.subarray(end + 4),
      });
    });
  });
}

const req = (method, path, body, extra = '') => {
  const b = body === undefined ? '' : body;
  const len = body === undefined ? '' : `Content-Length: ${Buffer.byteLength(b)}\r\n`;
  return `${method} ${path} HTTP/1.1\r\nHost: %HOST%\r\n${extra}${len}Connection: close\r\n\r\n${b}`;
};

const CASE = {
  suite: 'legacy',
  case_id: 'FX-1',
  profile: 'legacy-canonical',
  page_base64: Buffer.from('<p>hi</p>').toString('base64'),
  app: { status: 200, headers: [['content-type', 'text/html']] },
  server_config: {},
};
const JSON_TYPE = 'Content-Type: application/json\r\n';
const postCase = (port, c, extra = JSON_TYPE) =>
  raw(port, req('POST', '/__rivqen/conformance/case', typeof c === 'string' ? c : JSON.stringify(c), extra));
const json = (r) => JSON.parse(r.body.toString('utf8'));
const echo = (request, app, page, config) => ({
  status: 200,
  headers: [['x-seen', JSON.stringify({ request, app, config, page: Buffer.from(page).toString() })]],
  body: Buffer.from('ok'),
});

describe('control endpoint gate', () => {
  test('control_404_without_flag', async () => {
    const { port } = await start(echo, { conformance: false });
    assert.equal((await raw(port, req('GET', '/__rivqen/conformance/info'))).status, 404);
    assert.equal((await postCase(port, CASE)).status, 404);
    assert.equal((await raw(port, req('GET', '/page'))).status, 503);
  });

  test('control_403_from_non_loopback', async () => {
    assert.equal(controlGate(true, '192.168.1.5'), 403);
    assert.equal(controlGate(true, '::2'), 403);
    assert.equal(controlGate(true, undefined), 403);
    assert.equal(controlGate(true, '127.0.0.1'), 0);
    assert.equal(controlGate(true, '::1'), 0);
    assert.equal(controlGate(false, '127.0.0.1'), 404);
    assert.equal(controlGate(false, '192.168.1.5'), 404);
    assert.equal(isLoopback('127.0.0.2'), false);
    const { port } = await start(echo);
    assert.equal((await raw(port, req('GET', '/__rivqen/conformance/info'))).status, 200);
  });
});

describe('control endpoint', () => {
  test('info_lists_suites_and_profiles', async () => {
    const legacy = await start(echo);
    const r = await raw(legacy.port, req('GET', '/__rivqen/conformance/info'));
    assert.deepEqual(json(r), {
      contract: 1,
      suites: ['legacy'],
      profiles: ['legacy-canonical', 'legacy-java', 'legacy-node', 'legacy-php'],
    });
    assert.ok(r.lines.some(([n, v]) => n === 'Content-Type' && v === 'application/json'));
    const rqp = await start(echo, { suite: 'rqp' });
    assert.deepEqual(json(await raw(rqp.port, req('GET', '/__rivqen/conformance/info'))), {
      contract: 1,
      suites: ['rqp'],
      profiles: [],
    });
  });

  test('case_replaces_previous', async () => {
    const { port } = await start(echo);
    assert.equal((await postCase(port, CASE)).status, 204);
    assert.equal((await postCase(port, { ...CASE, case_id: 'FX-2', page_base64: Buffer.from('two').toString('base64') })).status, 204);
    const r = await raw(port, req('GET', '/p'));
    const seen = JSON.parse(r.lines.find(([n]) => n === 'x-seen')[1]);
    assert.equal(seen.page, 'two');
  });

  test('bad_case_400', async () => {
    const { port } = await start(echo);
    const bad = [
      '{not json',
      '[]',
      { ...CASE, suite: 'x' },
      { ...CASE, case_id: 1 },
      { ...CASE, profile: undefined },
      { ...CASE, page_base64: '@@@@' },
      { ...CASE, page_base64: 'abc' },
      { ...CASE, app: { status: 99, headers: [] } },
      { ...CASE, app: { status: 200, headers: [['a']] } },
      { ...CASE, app: undefined },
      { ...CASE, server_config: [] },
    ];
    for (const c of bad) {
      const r = await postCase(port, c);
      assert.equal(r.status, 400, JSON.stringify(c));
      assert.deepEqual(json(r), { error: 'BAD_CASE' });
    }
    assert.equal((await postCase(port, { ...CASE, page_base64: '' })).status, 204);
  });

  test('unsupported_profile_422', async () => {
    const { port } = await start(echo);
    for (const c of [{ ...CASE, profile: 'legacy-ruby' }, { ...CASE, suite: 'rqp' }]) {
      const r = await postCase(port, c);
      assert.equal(r.status, 422);
      assert.deepEqual(json(r), { error: 'UNSUPPORTED' });
    }
  });

  test('oversized_case_413', async () => {
    const { port } = await start(echo);
    // The server answers before it has read the whole body, so a late write error of the client is expected.
    const status = await new Promise((resolve) => {
      const rq = http.request({ port, host: '127.0.0.1', method: 'POST', path: '/__rivqen/conformance/case', headers: { 'Content-Type': 'application/json' } }, (res) => {
        res.resume();
        resolve(res.statusCode);
      });
      rq.on('error', () => {});
      rq.end(' '.repeat(MAX_CASE_BYTES + 1));
    });
    assert.equal(status, 413);
    assert.equal((await postCase(port, ' '.repeat(MAX_CASE_BYTES))).status, 400, 'a body of exactly the limit is read');
    assert.equal((await postCase(port, CASE)).status, 204);
  });
});

describe('case requests', () => {
  test('request_without_case_is_503', async () => {
    const { port } = await start(echo);
    assert.equal((await raw(port, req('GET', '/page'))).status, 503);
  });

  test('passes_raw_request_lines_to_handle', async () => {
    const { port } = await start(echo, { conformance: true });
    await postCase(port, { ...CASE, profile: 'legacy-java', server_config: { a: 1 } });
    const r = await raw(port, `GET /p?q=1 HTTP/1.1\r\nHost: shop.example.com\r\nX-Dup: 1\r\nx-dup: 2\r\nAccept: A\r\nConnection: close\r\n\r\n`);
    const seen = JSON.parse(r.lines.find(([n]) => n === 'x-seen')[1]);
    assert.deepEqual(seen.request.headers, [
      ['Host', 'shop.example.com'],
      ['X-Dup', '1'],
      ['x-dup', '2'],
      ['Accept', 'A'],
      ['Connection', 'close'],
    ]);
    assert.equal(seen.request.method, 'GET');
    assert.deepEqual(seen.app, CASE.app);
    assert.deepEqual(seen.config, { a: 1, profile: 'legacy-java' });
    assert.equal(seen.page, '<p>hi</p>');
  });

  test('rqp_config_has_no_profile', async () => {
    const { port } = await start(echo, { suite: 'rqp' });
    await postCase(port, { ...CASE, suite: 'rqp', profile: undefined, server_config: { policy: {} } });
    const r = await raw(port, req('GET', '/p'));
    assert.deepEqual(JSON.parse(r.lines.find(([n]) => n === 'x-seen')[1]).config, { policy: {} });
  });

  test('target_uses_origin_option', async () => {
    const a = await start(echo);
    await postCase(a.port, CASE);
    const seenA = JSON.parse((await raw(a.port, req('GET', '/x/y?z=1'))).lines.find(([n]) => n === 'x-seen')[1]);
    assert.equal(seenA.request.target, 'https://shop.example.com/x/y?z=1');
    const b = await start(echo, { origin: 'https://other.test/' });
    await postCase(b.port, CASE);
    const seenB = JSON.parse((await raw(b.port, req('GET', '/x'))).lines.find(([n]) => n === 'x-seen')[1]);
    assert.equal(seenB.request.target, 'https://other.test/x');
  });

  test('writes_repeated_response_lines', async () => {
    const handle = () => ({
      status: 203,
      headers: [['Set-Cookie', 'a=1'], ['X-Mid', 'm'], ['set-cookie', 'b=2'], ['Cache-Control', 'no-cache'], ['Cache-Control', 'private']],
      body: Buffer.from('hello'),
    });
    const { port } = await start(handle);
    await postCase(port, CASE);
    const r = await raw(port, req('GET', '/p'));
    assert.equal(r.status, 203);
    const mine = r.lines.filter(([n]) => !['Connection', 'Content-Length'].includes(n));
    assert.deepEqual(mine, [['Set-Cookie', 'a=1'], ['X-Mid', 'm'], ['set-cookie', 'b=2'], ['Cache-Control', 'no-cache'], ['Cache-Control', 'private']]);
    assert.equal(r.lines.find(([n]) => n === 'Content-Length')[1], '5');
    assert.equal(r.body.toString(), 'hello');
    assert.ok(!r.lines.some(([n]) => n.toLowerCase() === 'date'));
  });

  test('keeps_handle_content_length_and_sends_no_body_for_head_and_304', async () => {
    const handle = (request) =>
      request.target.endsWith('/n')
        ? { status: 304, headers: [['etag', '"x"']], body: null }
        : { status: 200, headers: [['content-length', '5'], ['x-a', '1']], body: request.method === 'HEAD' ? null : Buffer.from('hello') };
    const { port } = await start(handle);
    await postCase(port, CASE);
    const get = await raw(port, req('GET', '/p'));
    assert.equal(get.lines.filter(([n]) => n.toLowerCase() === 'content-length').length, 1);
    const head = await raw(port, req('HEAD', '/p'));
    assert.deepEqual(head.lines.find(([n]) => n === 'content-length'), ['content-length', '5']);
    assert.equal(head.body.length, 0);
    const nm = await raw(port, req('GET', '/n'));
    assert.equal(nm.status, 304);
    assert.equal(nm.body.length, 0);
    assert.ok(!nm.lines.some(([n]) => n.toLowerCase() === 'content-length'));
  });

  test('handle_exception_gives_500_without_page_bytes', async () => {
    const { port } = await start(() => {
      throw new Error('secret <p>hi</p>');
    });
    await postCase(port, CASE);
    const r = await raw(port, req('GET', '/p'));
    assert.equal(r.status, 500);
    assert.ok(!r.body.toString().includes('hi'));
  });

  test('works_with_the_rqp_reference_module', async () => {
    const handle = await loadHandle(fileURLToPath(new URL('../../../../tools/rqp-protocol/src/server.mjs', import.meta.url)));
    const page = '<!DOCTYPE html><html><head><title>t</title></head><body><p>x</p></body></html>';
    const { port } = await start(handle, { suite: 'rqp' });
    await postCase(port, {
      suite: 'rqp',
      case_id: 'FX-RQ',
      page_base64: Buffer.from(page).toString('base64'),
      app: { status: 200, headers: [['content-type', 'text/html; charset=utf-8']] },
      server_config: {},
    });
    const r = await raw(port, req('GET', '/', undefined, 'Accept: text/html\r\n'));
    assert.equal(r.status, 200);
    assert.ok(r.lines.some(([n]) => n.toLowerCase() === 'etag'));
    assert.ok(r.body.length > page.length);
  });
});

describe('command line', () => {
  test('parseCli_reads_options_and_env', () => {
    assert.deepEqual(parseCli(['--module', 'm.mjs', '--suite', 'rqp', '--port', '0'], {}), {
      module: 'm.mjs',
      suite: 'rqp',
      port: 0,
      origin: 'https://shop.example.com',
      conformance: false,
    });
    assert.equal(parseCli(['--module', 'm', '--suite', 'legacy', '--conformance'], {}).conformance, true);
    assert.equal(parseCli(['--module', 'm', '--suite', 'legacy'], { RIVQEN_CONFORMANCE: '1' }).conformance, true);
    assert.equal(parseCli(['--module', 'm', '--suite', 'legacy'], { RIVQEN_CONFORMANCE: '0' }).conformance, false);
    for (const bad of [[], ['--module', 'm'], ['--module', 'm', '--suite', 'x'], ['--module', 'm', '--suite', 'rqp', '--port', '99999'], ['--module', 'm', '--suite', 'rqp', '--nope']]) {
      assert.throws(() => parseCli(bad, {}), { code: 'USAGE' });
    }
  });

  test('loadHandle_rejects_missing_module', async () => {
    await assert.rejects(loadHandle('/nonexistent/x.mjs'), { code: 'BAD_MODULE' });
    await assert.rejects(loadHandle(fileURLToPath(import.meta.url)), { code: 'BAD_MODULE' });
  });
});

describe('control endpoint: browser and rebinding defence (DL-078, L1)', () => {
  const hostReq = (method, path, host, body, extra = '') =>
    `${method} ${path} HTTP/1.1\r\nHost: ${host}\r\n${extra}${body === undefined ? '' : `Content-Length: ${Buffer.byteLength(body)}\r\n`}Connection: close\r\n\r\n${body ?? ''}`;

  test('control_403_on_foreign_host', async () => {
    const { port } = await start(echo);
    for (const host of ['evil.example', `evil.example:${port}`, '127.0.0.1', `127.0.0.1.evil.example:${port}`]) {
      assert.equal((await raw(port, hostReq('GET', '/__rivqen/conformance/info', host))).status, 403, host);
      assert.equal((await raw(port, hostReq('POST', '/__rivqen/conformance/case', host, JSON.stringify(CASE), JSON_TYPE))).status, 403, host);
    }
    assert.equal((await raw(port, 'GET /__rivqen/conformance/info HTTP/1.0\r\n\r\n')).status, 403, 'no Host');
    assert.equal((await raw(port, 'GET /page HTTP/1.1\r\nHost: evil.example\r\nConnection: close\r\n\r\n')).status, 503, 'a case request is not a control request');
  });

  test('control_403_on_rebinding_host_with_right_port', async () => {
    const { port } = await start(echo);
    assert.equal((await raw(port, hostReq('GET', '/__rivqen/conformance/info', `rebind.example:${port}`))).status, 403);
    assert.equal((await raw(port, hostReq('GET', '/__rivqen/conformance/info', `127.0.0.1:${port + 1}`))).status, 403);
    for (const host of [`127.0.0.1:${port}`, `localhost:${port}`, `LocalHost:${port}`, `[::1]:${port}`]) {
      assert.equal((await raw(port, hostReq('GET', '/__rivqen/conformance/info', host))).status, 200, host);
    }
    assert.equal(isControlHost(undefined, 1), false);
    assert.equal(isControlHost('[::1]:7', 7), true);
  });

  test('control_415_without_json_content_type', async () => {
    const { port } = await start(echo);
    for (const extra of ['', 'Content-Type: text/plain\r\n', 'Content-Type: application/jsonx\r\n', 'Content-Type: application/json-seq\r\n', 'Content-Type: x/application/json\r\n']) {
      const r = await postCase(port, CASE, extra);
      assert.equal(r.status, 415, extra);
      assert.deepEqual(json(r), { error: 'UNSUPPORTED_MEDIA_TYPE' });
    }
    assert.equal((await raw(port, req('GET', '/page'))).status, 503, 'no case was stored');
  });

  test('control_accepts_json_with_charset', async () => {
    const { port } = await start(echo);
    for (const extra of ['Content-Type: application/json; charset=utf-8\r\n', 'Content-Type: APPLICATION/Json\r\n', 'Content-Type: application/json ;charset=x\r\n']) {
      assert.equal((await postCase(port, CASE, extra)).status, 204, extra);
    }
    assert.equal(isJsonContentType(undefined), false);
    assert.equal(isJsonContentType('application/json'), true);
  });
});

describe('harness never crashes (DL-078, L2)', () => {
  test('harness_survives_sync_throw_in_serve', async () => {
    const cases = [
      () => {
        throw new Error('sync');
      },
      async () => {
        throw new Error('rejected');
      },
      () => null,
      () => undefined,
      () => ({ status: 99, headers: [] }),
      () => ({ status: 200, headers: [['bad name', 'x']], body: null }),
      () => ({ status: 200, headers: [['x', 'a\r\nb: c']], body: null }),
    ];
    let current = 0;
    const { port } = await start((...args) => cases[current](...args));
    await postCase(port, CASE);
    for (current = 0; current < cases.length; current++) {
      const r = await raw(port, req('GET', '/p'));
      assert.equal(r.status, 500, `case ${current}`);
      assert.deepEqual(json(r), { error: 'HANDLE_FAILED' });
    }
    current = 0;
    cases[0] = () => ({ status: 200, headers: [], body: Buffer.from('ok') });
    assert.equal((await raw(port, req('GET', '/p'))).status, 200, 'still serving');
  });

  test('bad_header_value_in_case_400', async () => {
    const { port } = await start(echo);
    for (const value of ['a\r\nb: c', 'a\nb', 'a\rb', 'a\u0000b', 'a\u007fb', 'aĀb']) {
      const r = await postCase(port, { ...CASE, app: { status: 200, headers: [['x', value]] } });
      assert.equal(r.status, 400, JSON.stringify(value));
      assert.deepEqual(json(r), { error: 'BAD_CASE' });
    }
    assert.equal((await postCase(port, { ...CASE, app: { status: 200, headers: [['x', 'tab\there é']] } })).status, 204);
  });

  test('bad_header_name_in_case_400', async () => {
    const { port } = await start(echo);
    for (const name of ['', 'a b', 'a:b', 'a\r\nb', 'a\u0000', 'naïve', 'a/b', '(a)']) {
      const r = await postCase(port, { ...CASE, app: { status: 200, headers: [[name, 'v']] } });
      assert.equal(r.status, 400, JSON.stringify(name));
    }
    assert.equal((await postCase(port, { ...CASE, app: { status: 200, headers: [["X-Tok!#$%&'*+-.^_`|~9", 'v']] } })).status, 204);
  });

  test('handle_result_bad_headers_500', async () => {
    const bad = [{ status: 200 }, { status: 200, headers: 'x' }, { status: 200, headers: [['a']] }, { status: 200, headers: [['a', 1]] }, { status: 200, headers: [['a', 'b', 'c']] }];
    let current = 0;
    const { port } = await start(() => bad[current]);
    await postCase(port, CASE);
    for (current = 0; current < bad.length; current++) assert.equal((await raw(port, req('GET', '/p'))).status, 500, JSON.stringify(bad[current]));
  });
});

describe('control endpoint edges', () => {
  test('ipv4_mapped_loopback_is_loopback', () => {
    // A dual-stack socket reports an IPv4 client as ::ffff:127.0.0.1.
    assert.equal(isLoopback('::ffff:127.0.0.1'), true);
    assert.equal(isLoopback('::ffff:127.0.0.2'), false);
    assert.equal(isLoopback('::ffff:10.0.0.1'), false);
  });

  test('app_status_bounds_100_to_599', async () => {
    const { port } = await start(echo);
    for (const status of [99, 600, 1000]) {
      const r = await postCase(port, { ...CASE, app: { status, headers: [] } });
      assert.equal(r.status, 400, String(status));
    }
    for (const status of [100, 599]) assert.equal((await postCase(port, { ...CASE, app: { status, headers: [] } })).status, 204, String(status));
  });

  test('control_paths_405_on_the_wrong_method', async () => {
    const { port } = await start(echo);
    const info = await raw(port, req('POST', '/__rivqen/conformance/info', '{}', JSON_TYPE));
    assert.equal(info.status, 405);
    assert.deepEqual(json(info), { error: 'METHOD_NOT_ALLOWED' });
    const kase = await raw(port, req('GET', '/__rivqen/conformance/case'));
    assert.equal(kase.status, 405);
    assert.deepEqual(json(kase), { error: 'METHOD_NOT_ALLOWED' });
    const unknown = await raw(port, req('GET', '/__rivqen/conformance/other'));
    assert.equal(unknown.status, 404);
    assert.deepEqual(json(unknown), { error: 'NOT_FOUND' });
  });

  test('refusals_name_their_reason', async () => {
    const off = createHarness({ handle: echo, suite: 'legacy', conformance: false });
    await listen(off, 0, { write: () => {} });
    servers.push(off);
    const r = await raw(off.address().port, req('GET', '/__rivqen/conformance/info'));
    assert.equal(r.status, 404);
    assert.deepEqual(json(r), { error: 'NOT_FOUND' });
  });
});
