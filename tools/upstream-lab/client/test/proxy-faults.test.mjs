// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

import assert from 'node:assert/strict';
import test from 'node:test';
import { pairs, setField } from '../src/proxy.mjs';
import { send, startProxy, startUpstream, until } from './helpers.mjs';

const BODY = Buffer.from('0123456789abcdefghij');
const names = (rawHeaders) => pairs(rawHeaders).map(([n]) => n.toLowerCase());
const events = (log, ev) => log.events.filter((e) => e.ev === ev);
const lines = (raw, name) => pairs(raw).filter(([n]) => n.toLowerCase() === name);

const plain = (req, res) => {
  res.sendDate = false;
  res.writeHead(200, ['Content-Type', 'text/html', 'ETag', 'e1', 'Content-Length', String(BODY.length)]);
  res.end(BODY);
};
const repeated = (req, res) => {
  res.sendDate = false;
  res.writeHead(200, ['Content-Type', 'text/html', 'Cache-Offline', 'true', 'X-Other', 'o', 'cache-offline', 'store', 'Content-Length', String(BODY.length)]);
  res.end(BODY);
};

async function withProxy(handler, fn) {
  const up = await startUpstream(handler);
  const px = await startProxy(up.port);
  try {
    await fn({ up, ...px });
  } finally {
    await px.proxy.stop();
    await up.close();
  }
}

test('proxy_set_field_replaces_all_lines', async () => {
  await withProxy(repeated, async ({ proxy, log, files }) => {
    proxy.arm({ do: 'set_field', name: 'Cache-Offline', value: 'false' });
    const res = await send({ port: proxy.port });
    assert.deepEqual(lines(res.rawHeaders, 'cache-offline'), [['Cache-Offline', 'false']]);
    assert.deepEqual(names(res.rawHeaders).filter((n) => n !== 'connection' && n !== 'keep-alive'), ['content-type', 'cache-offline', 'x-other', 'content-length']);
    assert.deepEqual(res.body, BODY);
    assert.deepEqual(events(log, 'proxy.set_field').map(({ exchange, name, value }) => ({ exchange, name, value })), [{ exchange: 1, name: 'Cache-Offline', value: 'false' }]);
    await until(() => files.size === 1);
    const [upstream, delivered] = files.get(1).toString().split('--- delivered ---');
    assert.match(upstream, /Cache-Offline: true\n[^]*cache-offline: store/);
    assert.match(delivered, /Cache-Offline: false/);
    assert.doesNotMatch(delivered, /store/);
    const head = events(log, 'proxy.response_head')[0];
    assert.deepEqual(head.fields.filter(([n]) => n.toLowerCase() === 'cache-offline'), [['Cache-Offline', 'false']]);
    // the fault belongs to one exchange
    assert.equal(lines((await send({ port: proxy.port })).rawHeaders, 'cache-offline').length, 2);
  });
});

test('proxy_set_field_adds_missing_field', async () => {
  await withProxy(plain, async ({ proxy }) => {
    proxy.arm({ do: 'set_field', name: 'html-sha1', value: 'wrong' });
    const res = await send({ port: proxy.port });
    assert.deepEqual(lines(res.rawHeaders, 'html-sha1'), [['html-sha1', 'wrong']]);
    assert.deepEqual(res.body, BODY);
  });
});

test('proxy_set_field_keeps_the_place_of_the_first_line', () => {
  assert.deepEqual(setField([['a', '1'], ['B', '2'], ['b', '3'], ['c', '4']], 'b', 'x'), [['a', '1'], ['b', 'x'], ['c', '4']]);
  assert.deepEqual(setField([['a', '1']], 'b', 'x'), [['a', '1'], ['b', 'x']]);
});

test('proxy_drops_connection_listed_fields', async () => {
  const handler = (req, res) => {
    res.sendDate = false;
    res.writeHead(200, ['Connection', 'close, X-Hop', 'X-Hop', 'h', 'X-Keep', 'k', 'Content-Length', String(BODY.length)]);
    res.end(BODY);
  };
  await withProxy(handler, async ({ proxy, up }) => {
    const res = await send({ port: proxy.port, headers: ['Connection', 'x-req-hop , close', 'X-Req-Hop', '1', 'X-Req-Keep', '2'] });
    assert.deepEqual(names(res.rawHeaders).filter((n) => n.startsWith('x-')), ['x-keep']);
    const sent = names(up.seen[0].rawHeaders);
    assert.ok(!sent.includes('x-req-hop'));
    assert.ok(sent.includes('x-req-keep'));
  });
});
