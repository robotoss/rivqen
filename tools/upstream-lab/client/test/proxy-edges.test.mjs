// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

import assert from 'node:assert/strict';
import net from 'node:net';
import { Readable } from 'node:stream';
import test from 'node:test';
import { Proxy, pairs, readBounded } from '../src/proxy.mjs';
import { send, startProxy, startUpstream, until } from './helpers.mjs';

const BODY = Buffer.from('0123456789abcdefghij');
const answer = (req, res) => {
  res.sendDate = false;
  res.writeHead(200, ['Content-Type', 'text/html', 'ETag', 'e1', 'Content-Length', String(BODY.length)]);
  res.end(BODY);
};
const events = (log, ev) => log.events.filter((e) => e.ev === ev);

async function withProxy(handler, fn, options = {}) {
  const up = await startUpstream(handler);
  const px = await startProxy(up.port, options);
  try {
    await fn({ up, ...px });
  } finally {
    await px.proxy.stop();
    await up.close();
  }
}

test('proxy_does_not_forward_hop_by_hop_fields_to_the_upstream', async () => {
  await withProxy(answer, async ({ proxy, up }) => {
    const request = 'GET / HTTP/1.1\r\nHost: x\r\nKeep-Alive: 5\r\nTE: trailers\r\nProxy-Connection: x\r\nTrailer: X-T\r\nUpgrade: foo\r\nX-Keep: yes\r\nConnection: close\r\n\r\n';
    await new Promise((resolve) => {
      const s = net.connect(proxy.port, '127.0.0.1', () => s.write(request));
      s.on('data', () => {});
      s.on('close', resolve);
    });
    const got = pairs(up.seen[0].rawHeaders).map(([n]) => n.toLowerCase());
    for (const name of ['keep-alive', 'te', 'proxy-connection', 'trailer', 'upgrade']) assert.equal(got.includes(name), false, name);
    assert.ok(got.includes('x-keep'));
  });
});

test('proxy_adds_a_host_field_for_the_upstream_when_the_request_has_none', async () => {
  await withProxy(answer, async ({ proxy, up }) => {
    const reply = await new Promise((resolve) => {
      const s = net.connect(proxy.port, '127.0.0.1', () => s.write('GET /p HTTP/1.0\r\n\r\n'));
      const chunks = [];
      s.on('data', (c) => chunks.push(c));
      s.on('close', () => resolve(Buffer.concat(chunks).toString()));
    });
    assert.match(reply, /^HTTP\/1\.1 200 /);
    assert.deepEqual(pairs(up.seen[0].rawHeaders).filter(([n]) => n.toLowerCase() === 'host'), [['Host', `127.0.0.1:${up.port}`]]);
  });
});

test('proxy_keeps_the_host_field_of_the_request', async () => {
  await withProxy(answer, async ({ proxy, up }) => {
    await send({ port: proxy.port, headers: ['X-A', '1'] });
    const hosts = pairs(up.seen[0].rawHeaders).filter(([n]) => n.toLowerCase() === 'host');
    assert.deepEqual(hosts, [['Host', `127.0.0.1:${proxy.port}`]]); // the emulator's Host is passed on, not replaced
  });
});

test('proxy_listening_follows_start_and_stop', async () => {
  await withProxy(answer, async ({ proxy }) => {
    assert.equal(proxy.listening, true);
    await proxy.stop();
    assert.equal(proxy.listening, false);
    await proxy.stop(); // stopping twice is harmless
    await proxy.start();
    await proxy.start(); // starting twice is harmless
    assert.equal(proxy.listening, true);
  });
});

test('proxy_start_fails_with_listen_when_the_port_is_taken', async () => {
  await withProxy(answer, async ({ proxy, up }) => {
    const second = new Proxy({ upstreamPort: up.port, listenPort: proxy.port, emit: () => {} });
    await assert.rejects(second.start(), (e) => e.code === 'LISTEN' && e.message === `cannot listen on 127.0.0.1:${proxy.port}`);
  });
});

test('proxy_fault_errors_say_what_is_wrong', async () => {
  await withProxy(answer, async ({ proxy }) => {
    assert.throws(() => proxy.release(), (e) => e.code === 'NO_HOLD' && e.message === 'no hold to release');
    assert.throws(() => proxy.arm({ do: 'settle' }), (e) => e.code === 'BAD_FAULT' && e.message === 'not a fault step: settle');
    proxy.arm({ do: 'hold', point: 'head' });
    assert.throws(() => proxy.arm({ do: 'hold', point: 'head' }), (e) => e.code === 'HOLD_PENDING' && e.message === 'a hold is already armed or active');
  });
});

test('proxy_hold_event_names_the_byte_count_only_for_a_body_hold', async () => {
  await withProxy(answer, async ({ proxy, log }) => {
    proxy.arm({ do: 'hold', point: 'head' });
    const pending = send({ port: proxy.port });
    await until(() => events(log, 'proxy.hold').length === 1);
    assert.equal('bytes' in events(log, 'proxy.hold')[0], false);
    proxy.release();
    await pending;
  });
});

test('proxy_stops_at_the_earlier_of_a_cut_and_a_hold', async () => {
  await withProxy(answer, async ({ proxy, log }) => {
    proxy.arm({ do: 'hold', point: 'body', bytes: 10 });
    proxy.arm({ do: 'cut', bytes: 5 });
    const res = await send({ port: proxy.port });
    assert.equal(res.body.toString(), '01234');
    assert.equal(events(log, 'proxy.cut').length, 1);
    assert.equal(events(log, 'proxy.hold').length, 0);
  });
});

test('proxy_502_is_logged_as_head_and_incomplete_end', async () => {
  const up = await startUpstream(answer);
  const { port } = up;
  await up.close();
  const px = await startProxy(port);
  try {
    await send({ port: px.proxy.port });
    await until(() => events(px.log, 'proxy.response_end').length === 1);
    assert.deepEqual(events(px.log, 'proxy.response_head')[0].fields, []);
    assert.equal(events(px.log, 'proxy.response_head')[0].status, 502);
    const end = events(px.log, 'proxy.response_end')[0];
    assert.deepEqual([end.body_len, end.complete], [0, false]);
  } finally {
    await px.proxy.stop();
  }
});

test('proxy_upstream_that_does_not_answer_ends_with_a_timeout_error', async () => {
  const up = await startUpstream(() => {}); // never answers
  const px = await startProxy(up.port, { upstreamTimeoutMs: 50 });
  try {
    assert.equal((await send({ port: px.proxy.port })).status, 502);
    await until(() => px.records.size === 1);
    assert.equal(px.records.get(1).error, 'UPSTREAM_TIMEOUT: upstream did not answer within 50 ms');
  } finally {
    await px.proxy.stop();
    await up.close();
  }
});

test('read_bounded_reports_stream_errors_and_the_limit', async () => {
  const stream = new Readable({
    read() {
      this.destroy(new Error('boom'));
    },
  });
  await assert.rejects(readBounded(stream, 10), (e) => e.code === 'STREAM' && e.message === 'stream error' && e.cause.message === 'boom');
  await assert.rejects(readBounded(Readable.from([Buffer.alloc(9)]), 8), (e) => e.message === 'more than 8 bytes');
});
