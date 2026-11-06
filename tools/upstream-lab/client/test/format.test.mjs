// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

import assert from 'node:assert/strict';
import test from 'node:test';
import zlib from 'node:zlib';
import { canonicalLine, sortKeys } from '../src/canon.mjs';
import { EventLog } from '../src/eventlog.mjs';
import { contentEncoding, decodeBody, formatExchange } from '../src/httpfile.mjs';

function clock(...values) {
  let i = 0;
  return () => values[Math.min(i++, values.length - 1)];
}

test('event_log_format', () => {
  const log = new EventLog({ clock: clock(1000, 1007, 1412) });
  log.emit('conductor', 'run.start', { scenario: 'CS-01', mode: 'quick', repeat: 1, origin: 'lab-node', upstream_sha: '59936bef' });
  log.emit('harness', 'client.load_data', { thread: 'main', headers: [['content-type', 'text/html; charset=utf-8']], body_len: 3, zeta: { b: 1, a: 2 }, seq: 77, t: 5, src: 'x', ev: 'y' });
  const text = log.toJsonl();
  assert.equal(
    text,
    '{"ev":"run.start","mode":"quick","origin":"lab-node","repeat":1,"scenario":"CS-01","seq":1,"src":"conductor","t":7,"upstream_sha":"59936bef"}\n' +
      '{"body_len":3,"ev":"client.load_data","headers":[["content-type","text/html; charset=utf-8"]],"seq":2,"src":"harness","t":412,"thread":"main","zeta":{"a":2,"b":1}}\n',
  );
  assert.ok(text.endsWith('\n') && !text.includes('\r'));
  assert.equal(new EventLog().toJsonl(), '');
});

test('event_log_numbers_events_from_one_and_the_conductor_owns_ev_seq_src_t', () => {
  const log = new EventLog({ clock: clock(0, 0, 0) });
  const first = log.emit('proxy', 'proxy.request', { ev: 'other', seq: 9, src: 'harness', t: 9 });
  assert.deepEqual([first.ev, first.seq, first.src, first.t], ['proxy.request', 1, 'proxy', 0]);
  assert.equal(log.emit('proxy', 'a').seq, 2);
});

test('event_log_time_never_goes_below_zero', () => {
  const log = new EventLog({ clock: clock(100, 90) });
  assert.equal(log.emit('conductor', 'a').t, 0);
});

test('event_log_is_bounded', () => {
  const log = new EventLog({ maxEvents: 2 });
  log.emit('conductor', 'a');
  log.emit('conductor', 'a');
  assert.throws(() => log.emit('conductor', 'a'), (e) => e.code === 'EVENT_LIMIT');
});

test('event_log_wait_for_resolves_on_a_past_or_a_future_event_after_the_given_seq', async () => {
  const log = new EventLog();
  log.emit('conductor', 'x');
  assert.equal((await log.waitFor('x', 0, 10)).seq, 1);
  await assert.rejects(log.waitFor('x', 1, 10), (e) => e.code === 'WAIT_TIMEOUT');
  const later = log.waitFor('y', 0, 1000);
  log.emit('conductor', 'z');
  log.emit('conductor', 'y');
  assert.equal((await later).seq, 3);
});

test('canonical_line_sorts_keys_at_every_level_and_keeps_array_order', () => {
  assert.equal(canonicalLine({ b: [{ z: 1, a: 2 }, 3], a: null }), '{"a":null,"b":[{"a":2,"z":1},3]}');
  assert.deepEqual(sortKeys('x'), 'x');
});

test('event_log_format_http_file_matches_the_server_trace_format', () => {
  const gz = zlib.gzipSync('<p>hi</p>');
  const rec = {
    method: 'GET', target: '/catalog', requestFields: [['Accept-Encoding', 'gzip'], ['accept-diff', 'true']], requestBody: Buffer.alloc(0),
    response: { status: 304, fields: [['Content-Encoding', 'gzip'], ['Cache-Offline', 'store']], body: gz },
    delivered: null, error: null,
  };
  assert.equal(
    formatExchange(rec).toString(),
    'GET /catalog HTTP/1.1\nAccept-Encoding: gzip\naccept-diff: true\n\n--- response ---\nHTTP 304\nContent-Encoding: gzip\nCache-Offline: store\n\n[body decoded from content-encoding=gzip]\n<p>hi</p>',
  );
});

test('http_file_adds_a_delivered_section_with_the_wire_bytes', () => {
  const rec = {
    method: 'GET', target: '/', requestFields: [], requestBody: Buffer.alloc(0),
    response: { status: 200, fields: [['ETag', 'e1']], body: Buffer.from('abcdef') },
    delivered: { status: 200, fields: [], body: Buffer.from('abc') }, error: null,
  };
  assert.equal(
    formatExchange(rec).toString(),
    'GET / HTTP/1.1\n\n--- response ---\nHTTP 200\nETag: e1\n\n[body decoded from content-encoding=identity]\nabcdef\n--- delivered ---\nHTTP 200\n\n[body delivered: 3 of 6 wire bytes, content-encoding=identity]\nabc',
  );
});

test('http_file_keeps_a_request_body_and_an_error', () => {
  const text = formatExchange({
    method: 'POST', target: '/x', requestFields: [['Content-Length', '2']], requestBody: Buffer.from('hi'), response: null, delivered: null, error: 'UPSTREAM: down',
  }).toString();
  assert.equal(text, 'POST /x HTTP/1.1\nContent-Length: 2\n\n[request body: 2 bytes]\nhi\n--- response ---\nHTTP none\n\n\n--- proxy error ---\nUPSTREAM: down\n');
});

test('http_file_decodes_deflate_and_brotli_and_survives_bad_data', () => {
  assert.equal(decodeBody(zlib.deflateSync('d'), 'deflate').body.toString(), 'd');
  assert.equal(decodeBody(zlib.brotliCompressSync('b'), 'br').body.toString(), 'b');
  const bad = decodeBody(Buffer.from('not gzip'), 'gzip');
  assert.equal(bad.body.toString(), 'not gzip');
  assert.match(bad.note, /^\[body not decoded: content-encoding=gzip, /);
  assert.equal(decodeBody(Buffer.from('x'), 'zstd').note, '[body not decoded: content-encoding=zstd]');
});

test('content_encoding_reads_the_last_field_case_insensitively', () => {
  assert.equal(contentEncoding([['Content-Encoding', ' GZIP '], ['x', 'y']]), 'gzip');
  assert.equal(contentEncoding([['content-encoding', 'gzip'], ['CONTENT-ENCODING', 'identity']]), 'identity');
  assert.equal(contentEncoding([]), '');
});
