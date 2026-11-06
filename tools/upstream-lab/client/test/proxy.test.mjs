// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

import assert from 'node:assert/strict';
import net from 'node:net';
import os from 'node:os';
import test from 'node:test';
import { LabError } from '../src/errors.mjs';
import { forwardFields, pairs, readBounded } from '../src/proxy.mjs';
import { send, sleep, startProxy, startUpstream, until } from './helpers.mjs';

const BODY = Buffer.from('0123456789abcdefghij');
const answer = (req, res) => {
  res.sendDate = false;
  res.writeHead(200, ['Content-Type', 'text/html', 'ETag', 'e1', 'Content-Length', String(BODY.length)]);
  res.end(BODY);
};
const names = (rawHeaders) => pairs(rawHeaders).map(([n]) => n.toLowerCase());
const events = (log, ev) => log.events.filter((e) => e.ev === ev);

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

test('proxy_forwards_bytes_unchanged', async () => {
  const all = Buffer.from(Array.from({ length: 1024 }, (_, i) => i % 256));
  const handler = (req, res) => {
    res.sendDate = false;
    res.writeHead(201, 'Made', ['X-Mixed-Case', 'v1', 'Set-Cookie', 'a=1', 'Set-Cookie', 'b=2', 'Content-Length', String(all.length)]);
    res.end(all);
  };
  await withProxy(handler, async ({ proxy, up }) => {
    const res = await send({
      port: proxy.port, method: 'POST', path: '/p/a?x=1', headers: ['X-Req-Case', 'A', 'x-dup', '1', 'x-dup', '2', 'Content-Length', '3'], body: 'abc',
    });
    assert.equal(res.status, 201);
    assert.deepEqual(res.body, all);
    assert.deepEqual(pairs(res.rawHeaders).filter(([n]) => n !== 'Connection' && n !== 'Keep-Alive'), [
      ['X-Mixed-Case', 'v1'], ['Set-Cookie', 'a=1'], ['Set-Cookie', 'b=2'], ['Content-Length', '1024'],
    ]);
    const seen = up.seen[0];
    assert.equal(seen.method, 'POST');
    assert.equal(seen.url, '/p/a?x=1');
    assert.equal(seen.body.toString(), 'abc');
    const got = pairs(seen.rawHeaders).filter(([n]) => !['Host', 'Connection'].includes(n));
    assert.deepEqual(got, [['X-Req-Case', 'A'], ['x-dup', '1'], ['x-dup', '2'], ['Content-Length', '3']]);
  });
});

test('proxy_records_request_head_and_end_events', async () => {
  await withProxy(answer, async ({ proxy, log }) => {
    await send({ port: proxy.port, path: '/catalog', headers: ['X-A', 'b'] });
    await until(() => events(log, 'proxy.response_end').length === 1);
    const [req] = events(log, 'proxy.request');
    assert.equal(req.exchange, 1);
    assert.equal(req.target, '/catalog');
    assert.deepEqual(req.fields.find(([n]) => n === 'X-A'), ['X-A', 'b']);
    const [head] = events(log, 'proxy.response_head');
    assert.equal(head.status, 200);
    assert.deepEqual(head.fields, [['Content-Type', 'text/html'], ['ETag', 'e1'], ['Content-Length', '20']]);
    const [end] = events(log, 'proxy.response_end');
    assert.equal(end.body_len, 20);
    assert.equal(end.complete, true);
    assert.match(end.body_sha256, /^[0-9a-f]{64}$/);
  });
});

test('proxy_hold_request_delays_the_upstream_call', async () => {
  await withProxy(answer, async ({ proxy, up, log }) => {
    proxy.arm({ do: 'hold', point: 'request' });
    const pending = send({ port: proxy.port });
    await until(() => events(log, 'proxy.hold').length === 1);
    await sleep(50);
    assert.equal(up.seen.length, 0);
    proxy.release();
    const res = await pending;
    assert.equal(res.status, 200);
    assert.equal(up.seen.length, 1);
    assert.equal(events(log, 'proxy.hold')[0].point, 'request');
    assert.equal(events(log, 'proxy.release')[0].exchange, 1);
  });
});

test('proxy_hold_head_delays_the_response_head', async () => {
  await withProxy(answer, async ({ proxy, up, log }) => {
    proxy.arm({ do: 'hold', point: 'head' });
    let headSeen = false;
    const pending = send({ port: proxy.port, onData: () => { headSeen = true; } });
    await until(() => events(log, 'proxy.hold').length === 1);
    assert.equal(up.seen.length, 1); // the upstream answered; the client has nothing yet
    await sleep(50);
    assert.equal(headSeen, false);
    assert.equal(events(log, 'proxy.response_head').length, 0);
    proxy.release();
    assert.deepEqual((await pending).body, BODY);
    assert.equal(events(log, 'proxy.response_head').length, 1);
  });
});

test('proxy_hold_body_stops_after_n_bytes', async () => {
  await withProxy(answer, async ({ proxy, log }) => {
    proxy.arm({ do: 'hold', point: 'body', bytes: 5 });
    let got = Buffer.alloc(0);
    const pending = send({ port: proxy.port, onData: (b) => { got = b; } });
    await until(() => events(log, 'proxy.hold').length === 1);
    await until(() => got.length === 5);
    await sleep(50);
    assert.equal(got.toString(), '01234');
    assert.equal(events(log, 'proxy.hold')[0].bytes, 5);
    assert.equal(events(log, 'proxy.response_end').length, 0);
    proxy.release();
    const res = await pending;
    assert.deepEqual(res.body, BODY);
    assert.equal(res.complete, true);
  });
});

test('proxy_release_without_hold_fails', async () => {
  await withProxy(answer, async ({ proxy }) => {
    assert.throws(() => proxy.release(), (e) => e instanceof LabError && e.code === 'NO_HOLD');
  });
});

test('proxy_release_before_the_exchange_lets_it_pass', async () => {
  await withProxy(answer, async ({ proxy }) => {
    proxy.arm({ do: 'hold', point: 'head' });
    proxy.release();
    assert.equal((await send({ port: proxy.port })).status, 200);
  });
});

test('proxy_second_hold_while_one_is_armed_fails', async () => {
  await withProxy(answer, async ({ proxy }) => {
    proxy.arm({ do: 'hold', point: 'head' });
    assert.throws(() => proxy.arm({ do: 'hold', point: 'body', bytes: 1 }), (e) => e.code === 'HOLD_PENDING');
  });
});

test('proxy_cut_closes_after_n_body_bytes', async () => {
  await withProxy(answer, async ({ proxy, log, files }) => {
    proxy.arm({ do: 'cut', bytes: 7 });
    const res = await send({ port: proxy.port });
    assert.equal(res.body.toString(), '0123456');
    assert.equal(res.complete, false);
    await until(() => events(log, 'proxy.response_end').length === 1);
    assert.equal(events(log, 'proxy.cut')[0].bytes, 7);
    const end = events(log, 'proxy.response_end')[0];
    assert.equal(end.body_len, 7);
    assert.equal(end.complete, false);
    await until(() => files.size === 1);
    const text = files.get(1).toString();
    assert.match(text, /--- delivered ---\nHTTP 200\n/);
    assert.match(text, /\[body delivered: 7 of 20 wire bytes, content-encoding=identity\]\n0123456$/);
    assert.ok(text.includes('0123456789abcdefghij')); // the upstream response is stored whole
  });
});

test('proxy_drop_field_removes_the_field_and_keeps_both_forms', async () => {
  await withProxy(answer, async ({ proxy, log, files }) => {
    proxy.arm({ do: 'drop_field', name: 'etag' });
    const res = await send({ port: proxy.port });
    assert.deepEqual(names(res.rawHeaders).filter((n) => n === 'etag'), []);
    assert.deepEqual(res.body, BODY);
    await until(() => files.size === 1);
    assert.deepEqual(events(log, 'proxy.drop_field')[0], { ...events(log, 'proxy.drop_field')[0], exchange: 1, name: 'etag' });
    const text = files.get(1).toString();
    const [upstream, delivered] = text.split('--- delivered ---');
    assert.match(upstream, /ETag: e1/);
    assert.doesNotMatch(delivered, /ETag/);
  });
});

test('proxy_http_file_has_no_delivered_section_without_a_byte_fault', async () => {
  // CONTRACT section 5: only a drop_field or cut exchange adds `--- delivered ---`; a plain or held
  // exchange stores the request and the upstream response only (hop-by-hop fields are not a fault).
  await withProxy(answer, async ({ proxy, log, files }) => {
    await send({ port: proxy.port });
    proxy.arm({ do: 'hold', point: 'body', bytes: 3 });
    const held = send({ port: proxy.port });
    await until(() => events(log, 'proxy.hold').length === 1);
    proxy.release();
    assert.deepEqual((await held).body, BODY);
    await until(() => files.size === 2);
    for (const n of [1, 2]) {
      const text = files.get(n).toString();
      assert.ok(text.endsWith('\n0123456789abcdefghij'), `exchange ${n} ends with the upstream body`);
      assert.doesNotMatch(text, /--- delivered ---/);
    }
  });
});

test('proxy_faults_apply_to_one_exchange_only', async () => {
  await withProxy(answer, async ({ proxy }) => {
    proxy.arm({ do: 'drop_field', name: 'etag' });
    await send({ port: proxy.port });
    assert.ok(names((await send({ port: proxy.port })).rawHeaders).includes('etag'));
  });
});

test('proxy_server_down_refuses_and_server_up_accepts_again', async () => {
  await withProxy(answer, async ({ proxy }) => {
    const port = proxy.port;
    await proxy.stop();
    await assert.rejects(send({ port }), (e) => e.code === 'ECONNREFUSED');
    await proxy.start();
    assert.equal(proxy.port, port);
    assert.equal((await send({ port })).status, 200);
  });
});

test('proxy_armed_hold_does_not_survive_server_down', async () => {
  await withProxy(answer, async ({ proxy, log }) => {
    const port = proxy.port;
    proxy.arm({ do: 'hold', point: 'head' });
    await proxy.stop();
    await proxy.start();
    assert.equal((await send({ port })).status, 200);
    assert.equal(events(log, 'proxy.hold').length, 0);
    proxy.arm({ do: 'hold', point: 'head' }); // a new hold can be armed again
    proxy.release();
  });
});

test('proxy_stop_aborts_a_held_exchange', async () => {
  await withProxy(answer, async ({ proxy, log, records }) => {
    proxy.arm({ do: 'hold', point: 'head' });
    const pending = send({ port: proxy.port }).catch((e) => e);
    await until(() => events(log, 'proxy.hold').length === 1);
    await proxy.stop();
    await pending;
    assert.equal(records.get(1).error.startsWith('ABORTED'), true);
  });
});

test('proxy_answers_502_when_the_upstream_is_not_reachable', async () => {
  const up = await startUpstream(answer);
  const { port } = up;
  await up.close();
  const px = await startProxy(port);
  try {
    const res = await send({ port: px.proxy.port });
    assert.equal(res.status, 502);
    await until(() => px.files.size === 1);
    assert.equal(events(px.log, 'proxy.response_end')[0].complete, false);
    assert.match(px.files.get(1).toString(), /--- proxy error ---\nUPSTREAM: /);
  } finally {
    await px.proxy.stop();
  }
});

test('proxy_bounds_the_response_buffer', async () => {
  const handler = (req, res) => res.end(Buffer.alloc(200));
  const up = await startUpstream(handler);
  const px = await startProxy(up.port, { maxBytes: 100 });
  try {
    assert.equal((await send({ port: px.proxy.port })).status, 502);
    await until(() => px.records.size === 1);
    assert.match(px.records.get(1).error, /^TOO_LARGE/);
  } finally {
    await px.proxy.stop();
    await up.close();
  }
});

test('proxy_bounds_the_request_buffer', async () => {
  await withProxy(answer, async ({ proxy, up }) => {
    const small = await startProxy(up.port, { maxBytes: 4 });
    try {
      const res = await send({ port: small.proxy.port, method: 'POST', headers: ['Content-Length', '10'], body: '0123456789' }).catch((e) => e);
      assert.ok(res instanceof Error || res.status === 502);
      assert.equal(up.seen.length, 0);
      void proxy;
    } finally {
      await small.proxy.stop();
    }
  });
});

test('proxy_listens_on_loopback_only', async (t) => {
  const external = Object.values(os.networkInterfaces()).flat().find((a) => a && a.family === 'IPv4' && !a.internal);
  if (!external) return t.skip('no non-loopback IPv4 address on this host');
  await withProxy(answer, async ({ proxy }) => {
    await assert.rejects(
      new Promise((resolve, reject) => {
        const s = net.connect(proxy.port, external.address, () => { s.destroy(); resolve(); });
        s.on('error', reject);
      }),
      (e) => e.code === 'ECONNREFUSED',
    );
  });
});

test('forward_fields_drops_hop_by_hop_and_named_fields', () => {
  const fields = [['Connection', 'close'], ['Transfer-Encoding', 'chunked'], ['ETag', 'x'], ['X-A', '1']];
  assert.deepEqual(forwardFields(fields, ['etag']), [['X-A', '1']]);
});

test('read_bounded_fails_above_the_limit', async () => {
  const { Readable } = await import('node:stream');
  await assert.rejects(readBounded(Readable.from([Buffer.alloc(5), Buffer.alloc(5)]), 8), (e) => e.code === 'TOO_LARGE');
  assert.equal((await readBounded(Readable.from([Buffer.alloc(8)]), 8)).length, 8);
});
