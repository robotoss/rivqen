// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

import assert from 'node:assert/strict';
import { after, before, describe, test } from 'node:test';
import { gzipSync } from 'node:zlib';
import { HttpCode, HttpError, LIMITS, ResponseParser, exchange, serializeRequest } from '../src/raw-http.mjs';
import { startFake } from './helpers.mjs';

const raw = (s) => Buffer.from(s, 'latin1');

/** A fake whose case requests get the answers in `answers` by path. */
let fake;
const answers = new Map();
before(async () => {
  fake = await startFake({ respond: (request) => answers.get(request.url) ?? { status: 404, headers: [], body: null } });
});
after(() => fake.close());

const get = (path, extra = {}) => exchange({ host: '127.0.0.1', port: fake.port, method: 'GET', target: path, fields: [['Host', 'shop.example.com'], ['Connection', 'close']], timeoutMs: 2000, ...extra });

/** Parse a whole response with the parser alone (no socket). */
function parse(text, options) {
  const p = new ResponseParser(options);
  const bytes = typeof text === 'string' ? raw(text) : text;
  return p.push(bytes) ? p.result() : p.end();
}

const rejectsWith = (code, message) => (error) => {
  assert.ok(error instanceof HttpError, `not an HttpError: ${error}`);
  assert.equal(error.code, code);
  if (message !== undefined) assert.match(error.message, message);
  return true;
};

describe('request bytes', () => {
  test('sends_repeated_field_lines_separately', async () => {
    answers.set('/rep', { raw: raw('HTTP/1.1 204 No Content\r\n\r\n') });
    const before = fake.state.raw.length;
    await exchange({
      host: '127.0.0.1',
      port: fake.port,
      method: 'GET',
      target: '/rep',
      fields: [
        ['Host', 'shop.example.com'],
        ['if-none-match', 'a'],
        ['If-None-Match', 'b'],
        ['if-none-match', 'a'],
        ['Connection', 'close'],
      ],
    });
    assert.equal(fake.rawRequest(before), 'GET /rep HTTP/1.1\r\nHost: shop.example.com\r\nif-none-match: a\r\nIf-None-Match: b\r\nif-none-match: a\r\nConnection: close\r\n\r\n');
  });

  test('serializes the body after the empty line and an empty value as "name: "', () => {
    const bytes = serializeRequest({ method: 'POST', target: '/x?y=1', fields: [['a', ''], ['b', 'c d']], body: raw('{}') });
    assert.equal(bytes.toString('latin1'), 'POST /x?y=1 HTTP/1.1\r\na: \r\nb: c d\r\n\r\n{}');
  });

  test('refuses a request that cannot be sent exactly', () => {
    const bad = [
      { method: 'G T', target: '/', fields: [] },
      { method: 'GET', target: '/a b', fields: [] },
      { method: 'GET', target: '', fields: [] },
      { method: 'GET', target: '/', fields: [['a:b', 'x']] },
      { method: 'GET', target: '/', fields: [['a', 'x\r\nInjected: 1']] },
      { method: 'GET', target: '/', fields: [['a', 'café']] },
      { method: 'GET', target: '/', fields: [['a']] },
      { method: 'GET', target: '/', fields: [['a', 1]] },
    ];
    for (const request of bad) assert.throws(() => serializeRequest(request), rejectsWith(HttpCode.REQUEST), JSON.stringify(request));
  });

  test('a request that cannot be sent is rejected before a connection', async () => {
    const count = fake.state.raw.length;
    await assert.rejects(exchange({ host: '127.0.0.1', port: fake.port, method: 'GET', target: '/', fields: [['a', 'x\ny']] }), rejectsWith(HttpCode.REQUEST));
    assert.equal(fake.state.raw.length, count);
  });
});

describe('response field lines', () => {
  test('keeps_response_field_case_and_order', async () => {
    answers.set('/case', { raw: raw('HTTP/1.1 200 Fine\r\nX-B: 1\r\nx-a:2\r\nCONTENT-LENGTH: 2\r\nEtag:   "v"  \t\r\n\r\nok') });
    const r = await get('/case');
    assert.equal(r.status, 200);
    assert.equal(r.reason, 'Fine');
    assert.deepEqual(r.fields, [
      ['X-B', '1'],
      ['x-a', '2'],
      ['CONTENT-LENGTH', '2'],
      ['Etag', '"v"'],
    ]);
    assert.equal(r.body.toString('latin1'), 'ok');
  });

  test('does_not_join_repeated_response_fields', async () => {
    answers.set('/join', { raw: raw('HTTP/1.1 200 OK\r\ncache-control: no-cache\r\nContent-Length: 0\r\nCache-Control: private\r\nset-cookie: a=1\r\nset-cookie: b=2\r\n\r\n') });
    const r = await get('/join');
    assert.deepEqual(r.fields, [
      ['cache-control', 'no-cache'],
      ['Content-Length', '0'],
      ['Cache-Control', 'private'],
      ['set-cookie', 'a=1'],
      ['set-cookie', 'b=2'],
    ]);
  });

  test('keeps obs-text bytes of a value one to one', () => {
    const r = parse(Buffer.concat([raw('HTTP/1.1 200 OK\r\nx: '), Buffer.from([0xc3, 0xa9]), raw('\r\nContent-Length: 0\r\n\r\n')]));
    assert.deepEqual(Buffer.from(r.fields[0][1], 'latin1'), Buffer.from([0xc3, 0xa9]));
  });

  test('accepts HTTP/1.0 and a status line without a reason', () => {
    assert.deepEqual([parse('HTTP/1.0 200\r\nContent-Length: 0\r\n\r\n').version, parse('HTTP/1.0 200\r\nContent-Length: 0\r\n\r\n').reason], ['HTTP/1.0', '']);
    assert.equal(parse('HTTP/1.1 404 \r\nContent-Length: 0\r\n\r\n').status, 404);
  });
});

describe('response body framing', () => {
  test('does not decode a content coding', async () => {
    const gz = gzipSync(raw('hello'));
    answers.set('/gz', { raw: Buffer.concat([raw(`HTTP/1.1 200 OK\r\nContent-Encoding: gzip\r\nContent-Length: ${gz.length}\r\n\r\n`), gz]) });
    const r = await get('/gz');
    assert.deepEqual(r.body, gz);
  });

  test('decodes the chunked transfer coding (framing, not content)', async () => {
    answers.set('/chunked', { raw: raw('HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n5;ext=1\r\nhello\r\nA\r\n0123456789\r\n0\r\nx-trailer: 1\r\n\r\n') });
    const r = await get('/chunked');
    assert.equal(r.body.toString('latin1'), 'hello0123456789');
    assert.equal(r.extraBytes, 0);
    assert.deepEqual(r.fields, [['Transfer-Encoding', 'chunked']]);
  });

  test('reads a chunked body split at every byte', () => {
    const text = raw('HTTP/1.1 200 OK\r\ntransfer-encoding: Chunked\r\n\r\n3\r\nabc\r\n2\r\nde\r\n0\r\n\r\n');
    const p = new ResponseParser();
    let done = false;
    for (let i = 0; i < text.length; i++) done = p.push(text.subarray(i, i + 1));
    assert.equal(done, true);
    assert.equal(p.result().body.toString('latin1'), 'abcde');
    assert.equal(p.result().extraBytes, 0);
  });

  test('reads a body until the close when there is no length', async () => {
    answers.set('/close', { raw: raw('HTTP/1.1 200 OK\r\nx: 1\r\n\r\nall of it') });
    assert.equal((await get('/close')).body.toString('latin1'), 'all of it');
  });

  test('has no body for HEAD, 204 and 304', () => {
    assert.equal(parse('HTTP/1.1 200 OK\r\nContent-Length: 10\r\n\r\n', { method: 'HEAD' }).body, null);
    assert.equal(parse('HTTP/1.1 304 Not Modified\r\nContent-Length: 10\r\n\r\n').body, null);
    assert.equal(parse('HTTP/1.1 204 No Content\r\n\r\n').body, null);
    assert.deepEqual(parse('HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n').body, Buffer.alloc(0));
  });

  test('counts the bytes after the end of the response', () => {
    assert.equal(parse('HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nokEXTRA').extraBytes, 5);
    assert.equal(parse('HTTP/1.1 304 Not Modified\r\n\r\nbody').extraBytes, 4);
    const p = new ResponseParser();
    assert.equal(p.push(raw('HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nok')), true);
    assert.equal(p.push(raw('xyz')), true);
    assert.equal(p.result().extraBytes, 3);
  });

  test('a body shorter than Content-Length fails', async () => {
    answers.set('/short', { raw: raw('HTTP/1.1 200 OK\r\nContent-Length: 10\r\n\r\nabc') });
    await assert.rejects(get('/short'), rejectsWith(HttpCode.CLOSED, /before the end of the body \(3 bytes/));
  });

  test('a close without a response, or inside the field section, fails', async () => {
    answers.set('/nothing', { raw: Buffer.alloc(0) });
    await assert.rejects(get('/nothing'), rejectsWith(HttpCode.CLOSED, /without a response/));
    answers.set('/half', { raw: raw('HTTP/1.1 200 OK\r\nx: 1\r\n') });
    await assert.rejects(get('/half'), rejectsWith(HttpCode.CLOSED, /inside the field section/));
    answers.set('/half-chunk', { raw: raw('HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n5\r\nab') });
    await assert.rejects(get('/half-chunk'), rejectsWith(HttpCode.CLOSED, /before the end of the body/));
  });
});

describe('malformed responses', () => {
  const cases = [
    ['HTTP/1.1 20 OK\r\n\r\n', /status line/],
    ['HTTP/2 200 OK\r\n\r\n', /status line/],
    ['http/1.1 200 OK\r\n\r\n', /status line/],
    ['HTTP/1.1 200 O\x01K\r\n\r\n', /reason phrase/],
    ['HTTP/1.1 200 O\x7fK\r\n\r\n', /reason phrase/],
    ['HTTP/1.1 200 OK\r\nx: 1\r\n folded\r\n\r\n', /obsolete line folding/],
    ['HTTP/1.1 200 OK\r\nno colon\r\n\r\n', /without a name/],
    ['HTTP/1.1 200 OK\r\n: v\r\n\r\n', /without a name/],
    ['HTTP/1.1 200 OK\r\nx y: 1\r\n\r\n', /field name/],
    ['HTTP/1.1 200 OK\r\nx : 1\r\n\r\n', /field name/],
    ['HTTP/1.1 200 OK\r\nx: a\rb\r\n\r\n', /control character/],
    ['HTTP/1.1 200 OK\r\nx: a\nb: c\r\n\r\n', /control character/],
    ['HTTP/1.1 200 OK\r\nx: a\x00\r\n\r\n', /control character/],
    ['HTTP/1.1 200 OK\r\nx: a\x7f\r\n\r\n', /control character/],
    ['HTTP/1.1 200 OK\nContent-Length: 0\n\n', null],
    ['HTTP/1.1 100 Continue\r\n\r\n', /interim 100/],
    ['HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nContent-Length: 3\r\n\r\n', /both Transfer-Encoding and Content-Length/],
    ['HTTP/1.1 200 OK\r\nTransfer-Encoding: gzip, chunked\r\n\r\n', /transfer coding/],
    ['HTTP/1.1 200 OK\r\nTransfer-Encoding: identity\r\n\r\n', /transfer coding/],
    ['HTTP/1.1 200 OK\r\nContent-Length: 3, 3\r\n\r\nabc', /content-length/],
    ['HTTP/1.1 200 OK\r\nContent-Length: 3\r\nContent-Length: 4\r\n\r\nabc', /content-length/],
    ['HTTP/1.1 200 OK\r\nContent-Length: -1\r\n\r\n', /content-length/],
    ['HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\nzz\r\n', /chunk-size line/],
    ['HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n2\r\nabXY0\r\n\r\n', /no CRLF after the chunk data/],
    ['HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n0\r\nbad trailer\r\n\r\n', /without a name/],
  ];
  for (const [text, message] of cases) {
    test(`fails ${JSON.stringify(text.slice(0, 60))}`, () => {
      const code = message === null ? HttpCode.CLOSED : HttpCode.MALFORMED;
      assert.throws(() => parse(text), rejectsWith(code, message ?? undefined));
    });
  }

  test('two equal Content-Length lines frame the body', () => {
    assert.equal(parse('HTTP/1.1 200 OK\r\nContent-Length: 3\r\ncontent-length: 3\r\n\r\nabc').body.toString(), 'abc');
  });

  test('random bytes never throw anything but HttpError (property, fixed seed)', () => {
    let seed = 0x2f6b1d;
    const next = () => {
      seed = (seed * 1103515245 + 12345) & 0x7fffffff;
      return seed;
    };
    const alphabet = raw('HTTP/1.0 200\r\n:;\t chunked content-length transfer-encoding 0123456789abcdefABCDEF\x00\x7f\xff');
    const limits = { fields: 256, body: 512 };
    for (let n = 0; n < 2000; n++) {
      const prefix = n % 2 === 0 ? 'HTTP/1.1 200 OK\r\n' : '';
      const len = next() % 300;
      const bytes = Buffer.alloc(len);
      for (let i = 0; i < len; i++) bytes[i] = alphabet[next() % alphabet.length];
      const p = new ResponseParser({ limits });
      try {
        let done = false;
        const all = Buffer.concat([raw(prefix), bytes]);
        for (let i = 0; i < all.length && !done; i += 1 + (next() % 7)) done = p.push(all.subarray(i, i + 1 + (next() % 7)));
        const r = done ? p.result() : p.end();
        assert.ok(r.body === null || r.body.length <= limits.body);
      } catch (error) {
        assert.ok(error instanceof HttpError, `input ${JSON.stringify(bytes.toString('latin1'))}: ${error}`);
      }
    }
  });
});

describe('limits', () => {
  test('a field section of exactly 64 KiB passes and one byte more fails', () => {
    const fill = (total) => {
      const start = 'HTTP/1.1 200 OK\r\nContent-Length: 0\r\nx: ';
      return `${start}${'a'.repeat(total - start.length - 4)}\r\n\r\n`;
    };
    assert.equal(parse(fill(LIMITS.fields)).status, 200);
    assert.throws(() => parse(fill(LIMITS.fields + 1)), rejectsWith(HttpCode.TOO_LARGE, /field section is larger than 65536 bytes/));
  });

  test('an endless field section stops at the limit', () => {
    const p = new ResponseParser();
    p.push(raw('HTTP/1.1 200 OK\r\n'));
    assert.throws(() => {
      for (let i = 0; i < 100; i++) p.push(raw(`x: ${'a'.repeat(1000)}\r\n`));
    }, rejectsWith(HttpCode.TOO_LARGE, /field section/));
  });

  test('a Content-Length over 64 MiB fails before the body is read', () => {
    assert.equal(LIMITS.body, 64 * 1024 * 1024);
    assert.throws(() => parse(`HTTP/1.1 200 OK\r\nContent-Length: ${LIMITS.body + 1}\r\n\r\n`), rejectsWith(HttpCode.TOO_LARGE, /67108865 is larger than the body limit of 67108864/));
    assert.throws(() => parse('HTTP/1.1 200 OK\r\nContent-Length: 99999999999999999999\r\n\r\n'), rejectsWith(HttpCode.TOO_LARGE));
    const p = new ResponseParser();
    assert.equal(p.push(raw(`HTTP/1.1 200 OK\r\nContent-Length: ${LIMITS.body}\r\n\r\n`)), false);
  });

  test('a body without a length stops at the limit', () => {
    const limits = { fields: 1024, body: 10 };
    assert.equal(parse('HTTP/1.1 200 OK\r\n\r\n0123456789', { limits }).body.length, 10);
    assert.throws(() => parse('HTTP/1.1 200 OK\r\n\r\n0123456789X', { limits }), rejectsWith(HttpCode.TOO_LARGE, /body is larger than the limit of 10/));
  });

  test('a chunked body stops at the limit, also by the announced chunk size', () => {
    const limits = { fields: 1024, body: 10 };
    assert.equal(parse('HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n5\r\n01234\r\n5\r\n56789\r\n0\r\n\r\n', { limits }).body.length, 10);
    assert.throws(() => parse('HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n5\r\n01234\r\n6\r\n', { limits }), rejectsWith(HttpCode.TOO_LARGE));
    assert.throws(() => parse('HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\nFFFFFFFFFFFFFFFFFFFF\r\n', { limits }), rejectsWith(HttpCode.TOO_LARGE));
  });

  test('a chunk-size line and a trailer section are bounded', () => {
    const limits = { fields: 64, body: 1024 };
    assert.throws(() => parse(`HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n1;${'e'.repeat(100)}`, { limits }), rejectsWith(HttpCode.TOO_LARGE, /chunk-size line/));
    assert.throws(() => parse(`HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n1;${'e'.repeat(100)}\r\n`, { limits }), rejectsWith(HttpCode.TOO_LARGE, /chunk-size line/));
    const trailers = Array.from({ length: 10 }, (_, i) => `t${i}: ${'v'.repeat(5)}\r\n`).join('');
    assert.throws(() => parse(`HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n0\r\n${trailers}\r\n`, { limits }), rejectsWith(HttpCode.TOO_LARGE, /trailer section/));
  });
});

describe('connection', () => {
  test('an exchange stops at the deadline', async () => {
    answers.set('/hang', { hang: true });
    const t0 = Date.now();
    await assert.rejects(get('/hang', { timeoutMs: 150 }), rejectsWith(HttpCode.TIMEOUT, /no complete response within 150 ms/));
    assert.ok(Date.now() - t0 < 2000);
  });

  test('a reset after the connection is open is a NETWORK error', async () => {
    const { createServer } = await import('node:net');
    const server = createServer((s) => s.once('data', () => s.resetAndDestroy()));
    await new Promise((r) => server.listen(0, '127.0.0.1', r));
    try {
      await assert.rejects(exchange({ host: '127.0.0.1', port: server.address().port, method: 'GET', target: '/', fields: [] }), rejectsWith(HttpCode.NETWORK, /the connection failed \(ECONNRESET\)/));
    } finally {
      server.close();
    }
  });

  test('a refused connection is a CONNECT error', async () => {
    const other = await startFake({ respond: () => ({ hang: true }) });
    const port = other.port;
    await other.close();
    await assert.rejects(exchange({ host: '127.0.0.1', port, method: 'GET', target: '/', fields: [] }), rejectsWith(HttpCode.CONNECT, /cannot connect to 127\.0\.0\.1/));
  });
});
