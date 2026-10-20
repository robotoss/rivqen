// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors
//
// Hardening of the HTTP adapter (DL-033): the log has a path template, and the POST
// endpoints reject cross-site requests.
import { test, before, after } from 'node:test';
import assert from 'node:assert/strict';
import { request } from 'node:http';

process.env.LOG_LEVEL = 'info';
const { createDemoServer, pathTemplate, isOwnHost } = await import('../src/app.js');

const ORIGIN = 'https://shop.example.com';
const ACTION = [['Rq-Demo-Action', '1']];
let server;
let base;

before(async () => {
  server = createDemoServer({ origin: ORIGIN });
  await new Promise((resolve) => server.listen(0, '127.0.0.1', resolve));
  base = `http://127.0.0.1:${server.address().port}`;
});
after(() => new Promise((resolve) => server.close(resolve)));

/** One request with exact header lines. */
function call(method, path, headers = []) {
  return new Promise((resolve, reject) => {
    const hasHost = headers.some(([n]) => n.toLowerCase() === 'host');
    const lines = hasHost ? headers : [['Host', new URL(ORIGIN).host], ...headers];
    const req = request(`${base}${path}`, { method, headers: lines.flat() }, (res) => {
      const chunks = [];
      res.on('data', (c) => chunks.push(c));
      res.on('end', () => resolve({ status: res.statusCode, body: Buffer.concat(chunks) }));
    });
    req.on('error', reject);
    req.end();
  });
}

const state = async () => JSON.parse((await call('GET', '/healthz')).body).state;

/** Run `fn` and return the JSON log lines that the demo wrote meanwhile. */
async function captured(fn) {
  const lines = [];
  const write = process.stdout.write;
  process.stdout.write = (chunk, ...rest) => {
    const text = String(chunk);
    if (!text.startsWith('{"ts"')) return write.call(process.stdout, chunk, ...rest);
    lines.push(JSON.parse(text));
    return true;
  };
  try {
    await fn();
  } finally {
    process.stdout.write = write;
  }
  return lines;
}

test('POST without the custom header is rejected with 403 and changes nothing', async () => {
  const before = await state();
  for (const path of ['/__demo/next-data', '/__demo/next-template']) {
    assert.equal((await call('POST', path)).status, 403, path);
    assert.equal((await call('POST', path, [['Rq-Demo-Action', '0']])).status, 403, `${path} with another value`);
    assert.equal((await call('POST', path, [['Origin', ORIGIN]])).status, 403, `${path} with Origin only`);
  }
  assert.deepEqual(await state(), before);
});

test('POST with a foreign or null Origin is rejected with 403 and changes nothing', async () => {
  const before = await state();
  for (const origin of ['https://evil.example', 'null', `${ORIGIN}.evil.example`, 'http://shop.example.com']) {
    assert.equal((await call('POST', '/__demo/next-data', [...ACTION, ['Origin', origin]])).status, 403, origin);
  }
  assert.deepEqual(await state(), before);
});

test('POST with the custom header is accepted without Origin and with the own origin', async () => {
  const before = await state();
  assert.equal((await call('POST', '/__demo/next-data', ACTION)).status, 200);
  assert.equal((await call('POST', '/__demo/next-data', [...ACTION, ['Origin', ORIGIN]])).status, 200);
  assert.equal((await call('POST', '/__demo/next-template', [...ACTION, ['Origin', ORIGIN]])).status, 200);
  assert.notDeepEqual(await state(), before);
});

test('the request log has a path template, not the raw path or query', async () => {
  const lines = await captured(async () => {
    await call('GET', '/catalog?user=alice@example.com&token=s3cret');
    await call('GET', '/users/alice@example.com/orders/12345');
  });
  assert.deepEqual(lines.map((l) => l.path), ['/catalog', '<unmatched>']);
  assert.ok(!JSON.stringify(lines).includes('alice'));
  assert.ok(!JSON.stringify(lines).includes('s3cret'));
});

test('pathTemplate keeps only the routes of the demo', () => {
  assert.equal(pathTemplate('/healthz'), '/healthz');
  assert.equal(pathTemplate('/__demo/next-data'), '/__demo/next-data');
  assert.equal(pathTemplate('/other'), '<unmatched>');
});

test('a request with another Host is refused with 421, no body, on every route, and changes nothing (DL-036 I4)', async () => {
  const before = await state();
  for (const host of ['evil.example', 'shop.example.com:8443', 'shop.example.com.evil.example', '']) {
    for (const [method, path, extra] of [
      ['GET', '/catalog', []],
      ['GET', '/healthz', []],
      ['GET', '/other', []],
      ['POST', '/__demo/next-data', ACTION],
      ['POST', '/__demo/next-template', ACTION],
    ]) {
      const r = await call(method, path, [['Host', host], ...extra]);
      assert.equal(r.status, 421, `${method} ${path} Host "${host}"`);
      assert.equal(r.body.length, 0);
    }
  }
  assert.deepEqual(await state(), before);
});

test('the default port of the origin scheme is the same authority as no port (RFC 9110 sections 4.2.1, 4.2.2)', async () => {
  assert.equal((await call('GET', '/healthz', [['Host', 'shop.example.com:443']])).status, 200);
  assert.equal((await call('GET', '/healthz', [['Host', 'SHOP.example.com:443']])).status, 200);
  for (const host of ['shop.example.com:80', 'shop.example.com:', 'shop.example.com:0443', 'shop.example.com:443:443']) {
    assert.equal((await call('GET', '/healthz', [['Host', host]])).status, 421, host);
  }
});

test('isOwnHost: the default port depends on the scheme; a non-default port must match exactly', () => {
  const own = (host, value, port) => isOwnHost({ headers: { host: value } }, host, port);
  assert.equal(own('a.test', 'a.test:80', 80), true);
  assert.equal(own('a.test', 'a.test:443', 80), false);
  assert.equal(own('a.test', 'A.TEST:443', 443), true);
  assert.equal(own('a.test:8080', 'a.test:8080', undefined), true);
  assert.equal(own('a.test:8080', 'a.test:8080:80', undefined), false);
  assert.equal(own('a.test:8080', 'a.test', undefined), false);
  assert.equal(own('a.test', 'a.test', 80), true);
  assert.equal(isOwnHost({ headers: {} }, 'a.test', 80), false);
});

test('an origin with a non-default port accepts only that port', async () => {
  const own = createDemoServer({ origin: 'http://shop.example.com:8080' });
  await new Promise((resolve) => own.listen(0, '127.0.0.1', resolve));
  try {
    const status = (host) =>
      new Promise((resolve, reject) => {
        request(`http://127.0.0.1:${own.address().port}/healthz`, { headers: { Host: host } }, (res) => {
          res.resume();
          res.on('end', () => resolve(res.statusCode));
        }).on('error', reject).end();
      });
    assert.equal(await status('shop.example.com:8080'), 200);
    assert.equal(await status('shop.example.com'), 421);
    assert.equal(await status('shop.example.com:80'), 421);
    assert.equal(await status('shop.example.com:8080:80'), 421);
  } finally {
    await new Promise((resolve) => own.close(resolve));
  }
});

test('an http origin accepts :80', async () => {
  const own = createDemoServer({ origin: 'http://shop.example.com' });
  await new Promise((resolve) => own.listen(0, '127.0.0.1', resolve));
  try {
    const status = (host) =>
      new Promise((resolve, reject) => {
        request(`http://127.0.0.1:${own.address().port}/healthz`, { headers: { Host: host } }, (res) => {
          res.resume();
          res.on('end', () => resolve(res.statusCode));
        }).on('error', reject).end();
      });
    assert.equal(await status('shop.example.com:80'), 200);
    assert.equal(await status('shop.example.com:443'), 421);
  } finally {
    await new Promise((resolve) => own.close(resolve));
  }
});

test('the Host name is compared without case', async () => {
  assert.equal((await call('GET', '/healthz', [['Host', 'SHOP.Example.COM']])).status, 200);
});

test('the Origin check uses the normalized origin: a trailing slash or an upper-case host in the setting is accepted', async () => {
  for (const setting of ['https://shop.example.com/', 'HTTPS://SHOP.EXAMPLE.COM', 'https://Shop.Example.com:443']) {
    const own = createDemoServer({ origin: setting });
    await new Promise((resolve) => own.listen(0, '127.0.0.1', resolve));
    try {
      const url = `http://127.0.0.1:${own.address().port}/__demo/next-data`;
      const send = (origin) =>
        new Promise((resolve, reject) => {
          const req = request(url, { method: 'POST', headers: { Host: 'shop.example.com', 'Rq-Demo-Action': '1', Origin: origin } }, (res) => {
            res.resume();
            res.on('end', () => resolve(res.statusCode));
          });
          req.on('error', reject);
          req.end();
        });
      assert.equal(await send('https://shop.example.com'), 200, setting);
      assert.equal(await send('https://evil.example'), 403, setting);
    } finally {
      await new Promise((resolve) => own.close(resolve));
    }
  }
});

// ---- DL-063 ----
const { handle, DELTA_TYPE } = await import('../src/rqp.js');
const { normalizeHeaders, rqNonce, rqHave: parseRqHave, requestVersion, withoutRqVary } = await import('../src/fields.js');
const { APP, NONCE, TARGET, clientLines, get, header, page } = await import('./helpers.js');

const held = page('120.00');
const next = page('130.00');
const nonceLine = (n = NONCE) => `:${n.toString('base64')}:`;

test('a planted Rq-Nonce value appears in no log line (DL-063, L3)', async () => {
  const planted = Buffer.from('PLANTED-NONCE-16'); // 16 bytes
  const forms = [planted.toString('base64'), planted.toString('base64url'), planted.toString('hex'), 'PLANTED-NONCE-16'];
  const lines = await captured(async () => {
    await call('GET', '/catalog', [['Rq-Nonce', nonceLine(planted)]]);
    await call('GET', '/catalog', [
      ['Rq-Version', '1'],
      ['Rq-Capabilities', '(block-patch)'],
      ['Accept', DELTA_TYPE],
      ['If-None-Match', '"r1.x"'],
      ['Rq-Have', 't=x'],
      ['Rq-Nonce', nonceLine(planted)],
    ]);
    await call('GET', '/catalog', [['Rq-Nonce', `:${'A'.repeat(200)}${planted.toString('base64')}:`]]);
    await call('GET', '/nonce-route', [['Rq-Nonce', nonceLine(planted)]]);
  });
  assert.ok(lines.length >= 4);
  const text = JSON.stringify(lines);
  for (const form of forms) assert.ok(!text.includes(form), 'a nonce form is in the log');
});

test('an Rq-Nonce field line longer than 128 bytes is no nonce; a line of 128 bytes is parsed as before (DL-063)', () => {
  assert.equal(rqNonce([['rq-nonce', nonceLine()]])?.equals(NONCE), true);
  assert.equal(rqNonce([['rq-nonce', `:${'A'.repeat(127)}:`]]), null);
  const padded = `${nonceLine()};a="${'x'.repeat(128 - 26 - 5)}"`; // a parameter fills the line to 128 bytes
  assert.equal(Buffer.byteLength(padded), 128);
  assert.equal(rqNonce([['rq-nonce', padded]])?.equals(NONCE), true);
  assert.equal(rqNonce([['rq-nonce', `${padded.slice(0, -1)}x"`]]), null); // 129 bytes, still a valid Item
});

test('a too long Rq-Nonce gives the full document, not a delta', () => {
  const longer = `${nonceLine()};a="${'x'.repeat(130)}"`;
  assert.equal(handle(get(clientLines(held, { 'rq-nonce': longer })), APP, next).mode, 'full');
  assert.equal(handle(get(clientLines(held)), APP, next).mode, 'delta');
});

test('a delta carries Vary: Rq-Nonce and nosniff, and keeps the other Vary tokens of the application (DL-063)', () => {
  const app = { ...APP, headers: [...APP.headers, ['Vary', 'Accept-Encoding, RQ-Have'], ['X-Content-Type-Options', 'nosniff']] };
  const r = handle(get(clientLines(held)), app, next);
  assert.equal(r.mode, 'delta');
  const vary = r.headers.filter(([n]) => n === 'vary').map(([, v]) => v);
  assert.ok(vary.includes('Rq-Nonce'));
  assert.ok(vary.includes('Accept-Encoding'));
  assert.ok(!vary.join(',').toLowerCase().includes('rq-have'));
  assert.deepEqual(r.headers.filter(([n]) => n === 'x-content-type-options'), [['x-content-type-options', 'nosniff']]);
  const plain = handle(get(clientLines(held)), APP, next);
  assert.equal(header(plain, 'vary'), 'Rq-Nonce');
  assert.equal(header(plain, 'x-content-type-options'), 'nosniff');
});

test('F, 304 and a passthrough response lose the Rq-* tokens of the application Vary and keep the others (DL-063, L2)', () => {
  const app = (vary) => ({ ...APP, headers: [...APP.headers, ['Vary', vary]] });
  const full = handle(get([]), app('Accept-Encoding, rq-nonce, RQ-Have ,Cookie'), held);
  assert.equal(full.mode, 'full');
  assert.equal(header(full, 'vary'), 'Accept-Encoding, Cookie');
  const notModified = handle(get([['if-none-match', header(full, 'etag')]]), app('Rq-Nonce, Accept-Encoding'), held);
  assert.equal(notModified.mode, '304');
  assert.equal(header(notModified, 'vary'), 'Accept-Encoding');
  const out = handle({ method: 'POST', target: TARGET, headers: [] }, app('Origin,Rq-Version'), held);
  assert.equal(out.mode, 'passthrough');
  assert.equal(header(out, 'vary'), 'Origin');
  assert.equal(header(handle(get([]), app('Rq-Nonce, rq-have'), held), 'vary'), null, 'the field is dropped when no token is left');
  assert.equal(header(handle(get([]), app('*'), held), 'vary'), '*');
});

test('withoutRqVary: only whole tokens that start with "rq-" go; other lines stay as they are', () => {
  assert.deepEqual(withoutRqVary([['vary', 'Accept,  Rq-X']]), [['vary', 'Accept']]);
  assert.deepEqual(withoutRqVary([['vary', 'Accept,,Rq-X, ']]), [['vary', 'Accept']]);
  assert.deepEqual(withoutRqVary([['vary', 'Accept ,  X-Rq-Y,Prq-Z']]), [['vary', 'Accept ,  X-Rq-Y,Prq-Z']]);
  assert.deepEqual(
    withoutRqVary([['vary', 'rq-a'], ['vary', 'B'], ['x', 'rq-a']]),
    [['vary', 'B'], ['x', 'rq-a']],
  );
});

test('normalizeHeaders trims leading and trailing space and TAB of the values like OWS (DL-036 I4)', () => {
  assert.deepEqual(normalizeHeaders([['Rq-Version', '\t 1 \t'], ['X', 'a b\tc']]), [['rq-version', '1'], ['x', 'a b\tc']]);
  assert.equal(requestVersion(normalizeHeaders([['Rq-Version', '\t1\t']])), 1);
});

test('64 KB adversarial values of Rq-Version, Rq-Have and Rq-Nonce parse in under 50 ms (DL-063, M1)', () => {
  const n = 65536;
  const values = [
    `:${'='.repeat(n)}A:`,
    `:${'='.repeat(n)}`,
    `:${'A'.repeat(n)}${'='.repeat(n)}:`,
    `:${'A='.repeat(n / 2)}:`,
    '1'.repeat(n),
    `-${'0'.repeat(n)}.`,
    `"${'\\'.repeat(n)}`,
    `%"${'%'.repeat(n)}`,
    `${'a'.repeat(n)};${'b=:'.repeat(n / 3)}`,
    `t=x, b=:${'='.repeat(n)}A:`,
    `(${'a '.repeat(n / 2)}`,
    'A'.repeat(n),
    `${' '.repeat(n)}x`,
  ];
  const readers = [
    ['rq-version', requestVersion],
    ['rq-have', parseRqHave],
    ['rq-nonce', rqNonce],
  ];
  for (const value of values) {
    for (const [name, read] of readers) {
      const start = process.hrtime.bigint();
      read([[name, value]]);
      const ms = Number(process.hrtime.bigint() - start) / 1e6;
      assert.ok(ms < 50, `${name} with a value of ${value.length} chars took ${ms} ms`);
    }
  }
});
