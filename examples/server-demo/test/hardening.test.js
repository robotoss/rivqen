// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors
//
// Hardening of the HTTP adapter (DL-033): the log has a path template, and the POST
// endpoints reject cross-site requests.
import { test, before, after } from 'node:test';
import assert from 'node:assert/strict';
import { request } from 'node:http';

process.env.LOG_LEVEL = 'info';
const { createDemoServer, pathTemplate } = await import('../src/app.js');

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
    const req = request(`${base}${path}`, { method, headers: [['Host', new URL(ORIGIN).host], ...headers].flat() }, (res) => {
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
