// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// The body fault set_body_member (CONTRACT section 3, S2, H-126): proxy behavior and the pure edit.
import assert from 'node:assert/strict';
import test from 'node:test';
import zlib from 'node:zlib';
import { MAX_BODY_MEMBER_BYTES, decodeBounded, decodeJsonString, replaceMember, setBodyMember, topLevelMembers } from '../src/bodymember.mjs';
import { eventProblems } from '../src/check.mjs';
import { pairs } from '../src/proxy.mjs';
import { send, startProxy, startUpstream, until } from './helpers.mjs';

const events = (log, ev) => log.events.filter((e) => e.ev === ev);
const field = (raw, name) => pairs(raw).filter(([n]) => n.toLowerCase() === name).map(([, v]) => v);
const WRONG = '0000000000000000000000000000000000000000';
// The shape of a data update of the upstream Node.js server (stored trace CS-26), with synthetic values.
const UPDATE = '{"data":{"{price}":"<p>130.00</p>"},"diff":"","html-sha1":"4c33dabea7ea65c6255b9e6a6539976674886a2c","template-tag":"3c77"}';

/** A lab answer: chunked, with the given content coding. */
const answer = (text, coding) => (req, res) => {
  res.sendDate = false;
  const raw = Buffer.from(text, 'utf8');
  const body = coding === 'gzip' ? zlib.gzipSync(raw) : raw;
  res.writeHead(200, ['Content-Type', 'text/html; charset=utf-8', ...(coding ? ['Content-Encoding', coding] : []), 'Etag', 'e1']);
  res.write(body.subarray(0, 5));
  res.end(body.subarray(5));
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

test('set_body_member_replaces_member_of_gzip_json_body', async () => {
  await withProxy(answer(UPDATE, 'gzip'), async ({ proxy, log, files }) => {
    proxy.arm({ do: 'set_body_member', name: 'html-sha1', value: WRONG });
    const res = await send({ port: proxy.port });
    const expected = UPDATE.replace('4c33dabea7ea65c6255b9e6a6539976674886a2c', WRONG);
    assert.equal(res.body.toString('utf8'), expected); // identity, other members in their order and bytes
    assert.deepEqual(field(res.rawHeaders, 'content-encoding'), []);
    assert.deepEqual(field(res.rawHeaders, 'transfer-encoding'), []);
    assert.deepEqual(field(res.rawHeaders, 'content-length'), [String(Buffer.byteLength(expected))]);
    assert.deepEqual(field(res.rawHeaders, 'etag'), ['e1']);
    const [ev] = events(log, 'proxy.set_body_member');
    assert.deepEqual({ exchange: ev.exchange, name: ev.name, value: ev.value, applied: ev.applied, reason: ev.reason }, { exchange: 1, name: 'html-sha1', value: WRONG, applied: true, reason: undefined });
    assert.deepEqual(eventProblems(ev), []);
    const end = events(log, 'proxy.response_end')[0];
    assert.deepEqual([end.body_len, end.complete], [Buffer.byteLength(expected), true]);
    // the event comes before the delivered head
    assert.ok(ev.seq < events(log, 'proxy.response_head')[0].seq);
    await until(() => files.size === 1);
    const [upstream, delivered] = files.get(1).toString('utf8').split('--- delivered ---');
    assert.match(upstream, /Content-Encoding: gzip/);
    assert.ok(upstream.includes('4c33dabea7ea65c6255b9e6a6539976674886a2c'));
    assert.doesNotMatch(delivered, /Content-Encoding/);
    assert.ok(delivered.includes(`[body delivered: ${Buffer.byteLength(expected)} of `));
    assert.ok(delivered.endsWith(expected));
    // the fault belongs to one exchange
    const again = await send({ port: proxy.port });
    assert.deepEqual(field(again.rawHeaders, 'content-encoding'), ['gzip']);
    assert.equal(events(log, 'proxy.set_body_member').length, 1);
  });
});

test('set_body_member_adds_an_absent_member_at_the_end', async () => {
  await withProxy(answer('{"a":1}', ''), async ({ proxy }) => {
    proxy.arm({ do: 'set_body_member', name: 'html-sha1', value: 'x' });
    assert.equal((await send({ port: proxy.port })).body.toString(), '{"a":1,"html-sha1":"x"}');
  });
  assert.equal(replaceMember('{}', 'k', 'v').text, '{"k":"v"}');
  assert.equal(replaceMember(' { } ', 'k', 'v').text, ' { "k":"v"} ');
});

test('set_body_member_leaves_non_json_body', async () => {
  const html = '<html><p>not json</p></html>';
  await withProxy(answer(html, 'gzip'), async ({ proxy, log, files }) => {
    proxy.arm({ do: 'set_body_member', name: 'html-sha1', value: WRONG });
    const res = await send({ port: proxy.port });
    assert.deepEqual(field(res.rawHeaders, 'content-encoding'), ['gzip']);
    assert.equal(zlib.gunzipSync(res.body).toString(), html);
    const [ev] = events(log, 'proxy.set_body_member');
    assert.deepEqual([ev.applied, ev.reason], [false, 'body is not JSON']);
    assert.deepEqual(eventProblems(ev), []);
    await until(() => files.size === 1);
    assert.match(files.get(1).toString('latin1'), /--- delivered ---\nHTTP 200\n[^]*Content-Encoding: gzip/);
  });
  for (const text of ['[1,2]', 'null', '"s"', '12']) assert.equal(replaceMember(text, 'k', 'v').reason, 'body is not a JSON object', text);
});

test('set_body_member_skips_body_over_1_mib', () => {
  assert.equal(MAX_BODY_MEMBER_BYTES, 1024 * 1024);
  const big = Buffer.from(`{"a":"${'x'.repeat(MAX_BODY_MEMBER_BYTES)}"}`);
  const r = setBodyMember({ fields: [], body: big, name: 'a', value: 'v' });
  assert.deepEqual([r.applied, r.reason, r.body === big], [false, `body over ${MAX_BODY_MEMBER_BYTES} bytes`, true]);
  // exactly at the limit is edited
  const at = Buffer.from(`{"a":"${'x'.repeat(MAX_BODY_MEMBER_BYTES - 8)}"}`);
  assert.equal(at.length, MAX_BODY_MEMBER_BYTES);
  assert.equal(setBodyMember({ fields: [], body: at, name: 'a', value: 'v' }).applied, true);
  // a small gzip body that decodes to more than the limit is not decoded past it
  const bomb = zlib.gzipSync(Buffer.alloc(MAX_BODY_MEMBER_BYTES + 1, 0x20));
  assert.ok(bomb.length < 4096);
  const b = setBodyMember({ fields: [['Content-Encoding', 'gzip']], body: bomb, name: 'a', value: 'v' });
  assert.deepEqual([b.applied, b.reason, b.body === bomb], [false, `decoded body over ${MAX_BODY_MEMBER_BYTES} bytes`, true]);
});

test('set_body_member_logs_applied_false_with_reason', async () => {
  const notModified = (req, res) => {
    res.sendDate = false;
    res.writeHead(304, ['Etag', 'e1']);
    res.end();
  };
  await withProxy(notModified, async ({ proxy, log }) => {
    proxy.arm({ do: 'set_body_member', name: 'html-sha1', value: WRONG });
    const res = await send({ port: proxy.port });
    assert.equal(res.status, 304);
    const [ev] = events(log, 'proxy.set_body_member');
    assert.deepEqual([ev.exchange, ev.applied, ev.reason], [1, false, 'empty body']);
    assert.deepEqual(eventProblems(ev), []);
  });
  const fields = [['Content-Encoding', 'gzip']];
  const cases = [
    [{ fields, body: Buffer.from('not gzip') }, 'content-encoding gzip: Z_DATA_ERROR'],
    [{ fields: [['Content-Encoding', 'zstd']], body: Buffer.from('{}') }, 'content-encoding zstd not supported'],
    [{ fields: [], body: Buffer.from([0x7b, 0xff, 0x7d]) }, 'body is not UTF-8'],
    [{ fields: [], body: Buffer.from('﻿{"a":1}') }, 'body is not JSON'], // a BOM is kept, so the text is not JSON
  ];
  for (const [input, reason] of cases) {
    const r = setBodyMember({ ...input, name: 'a', value: 'v' });
    assert.deepEqual([r.applied, r.reason, r.body, r.fields], [false, reason, input.body, input.fields], reason);
  }
});

test('set_body_member_event_has_a_reason_exactly_when_not_applied', () => {
  const base = { ev: 'proxy.set_body_member', seq: 1, src: 'proxy', t: 0, exchange: 1, name: 'n', value: 'v' };
  assert.deepEqual(eventProblems({ ...base, applied: true }), []);
  assert.deepEqual(eventProblems({ ...base, applied: false, reason: 'r' }), []);
  assert.deepEqual(eventProblems({ ...base, applied: true, reason: 'r' }), ['proxy.set_body_member: reason only and always with applied false']);
  assert.deepEqual(eventProblems({ ...base, applied: false }), ['proxy.set_body_member: reason only and always with applied false']);
  assert.ok(eventProblems({ ...base, applied: 'yes' }).includes('proxy.set_body_member: bad applied'));
  assert.ok(eventProblems({ ...base, applied: false, reason: 1 }).includes('proxy.set_body_member: bad reason'));
  assert.ok(eventProblems({ ...base, applied: true, value: 2 }).includes('proxy.set_body_member: bad value'));
});

test('set_body_member_decodes_each_supported_coding', () => {
  const text = '{"a":1}';
  for (const [coding, encode] of [['gzip', zlib.gzipSync], ['x-gzip', zlib.gzipSync], ['deflate', zlib.deflateSync], ['br', zlib.brotliCompressSync], ['identity', (b) => b], ['', (b) => b]]) {
    const fields = coding === '' ? [] : [['content-encoding', ` ${coding.toUpperCase()} `]];
    const r = setBodyMember({ fields: [...fields, ['Transfer-Encoding', 'chunked'], ['content-length', '99'], ['X-Keep', 'k']], body: encode(Buffer.from(text)), name: 'a', value: 'v' });
    assert.equal(r.applied, true, coding);
    assert.equal(r.body.toString(), '{"a":"v"}', coding);
    assert.deepEqual(r.fields, [['X-Keep', 'k'], ['Content-Length', '9']], coding);
  }
  // the last Content-Encoding field counts
  assert.equal(decodeBounded(Buffer.from('x'), 'identity').bytes.toString(), 'x');
  const two = setBodyMember({ fields: [['Content-Encoding', 'br'], ['Content-Encoding', 'gzip']], body: zlib.gzipSync(Buffer.from(text)), name: 'a', value: 'v' });
  assert.equal(two.applied, true);
});

test('set_body_member_keeps_the_raw_text_of_the_other_members', () => {
  const text = ' {\n  "2": [1, {"}": "]"}],\n  "k" : "old\\"]}" ,\n  "1": {"a": [true, null]},\n  "n": -1.5e3\n}\n';
  const r = replaceMember(text, 'k', 'new "value"');
  assert.equal(r.text, ' {\n  "2": [1, {"}": "]"}],\n  "k" : "new \\"value\\"" ,\n  "1": {"a": [true, null]},\n  "n": -1.5e3\n}\n');
  // integer-like keys keep their place in the text (a JS object would move them first)
  assert.deepEqual(topLevelMembers(r.text).members.map((m) => m.key), ['2', 'k', '1', 'n']);
  // an escaped key is compared after decoding; a repeated key gets the value at every place
  assert.equal(replaceMember('{"\\u006b":1,"x":2,"k":3}', 'k', 'v').text, '{"\\u006b":"v","x":2,"k":"v"}');
  // a nested member with the name is not changed
  assert.equal(replaceMember('{"o":{"k":1}}', 'k', 'v').text, '{"o":{"k":1},"k":"v"}');
});

/** A small seeded generator (mulberry32) for the property test. */
function rng(seed) {
  let a = seed >>> 0;
  return () => {
    a = (a + 0x6d2b79f5) >>> 0;
    let t = a;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}
const ALPHABET = ['a', 'k', '1', '"', '\\', '}', ']', '{', '[', ',', ':', ' ', 'é', '\u2028', '\n', '0'];

function genString(r) {
  let s = '';
  const n = Math.floor(r() * 6);
  for (let i = 0; i < n; i += 1) s += ALPHABET[Math.floor(r() * ALPHABET.length)];
  return s;
}
function genValue(r, depth) {
  const kind = Math.floor(r() * (depth > 2 ? 4 : 6));
  if (kind === 0) return genString(r);
  if (kind === 1) return Math.floor(r() * 2000) - 1000;
  if (kind === 2) return [true, false, null][Math.floor(r() * 3)];
  if (kind === 3) return r() * 10;
  if (kind === 4) return Array.from({ length: Math.floor(r() * 3) }, () => genValue(r, depth + 1));
  return genObject(r, depth + 1);
}
function genObject(r, depth) {
  const o = {};
  const n = Math.floor(r() * 5);
  for (let i = 0; i < n; i += 1) o[genString(r)] = genValue(r, depth);
  return o;
}
/** JSON text with random whitespace between tokens of the top level. */
function spaced(r, obj) {
  const ws = () => [' ', '', '\n', '\t ', '\r\n'][Math.floor(r() * 5)];
  const parts = Object.entries(obj).map(([k, v]) => `${ws()}${JSON.stringify(k)}${ws()}:${ws()}${JSON.stringify(v, null, r() < 0.5 ? 0 : 2)}${ws()}`);
  return `${ws()}{${parts.join(',')}${parts.length === 0 ? ws() : ''}}${ws()}`;
}

// The oracle does not use the keys of JSON.parse: Node.js 26.8.2 returns a wrong one-character escaped key
// after a key "\\" (see decodeJsonString). Values of strings and the raw text are compared instead.
test('set_body_member_property_only_the_named_member_changes', () => {
  const r = rng(20261107);
  for (let i = 0; i < 3000; i += 1) {
    const obj = genObject(r, 0);
    const text = spaced(r, obj);
    const keys = Object.keys(obj); // the order of the text (spaced writes Object.entries)
    const name = keys.length > 0 && r() < 0.7 ? keys[Math.floor(r() * keys.length)] : genString(r);
    const value = genString(r);
    const out = replaceMember(text, name, value);
    assert.equal(typeof out.text, 'string', text);
    const parsed = JSON.parse(out.text); // still JSON, still an object
    assert.ok(parsed !== null && typeof parsed === 'object' && !Array.isArray(parsed), text);
    const before = topLevelMembers(text).members;
    const after = topLevelMembers(out.text).members;
    assert.deepEqual(before.map((m) => m.key), keys, JSON.stringify(text));
    assert.deepEqual(after.map((m) => m.key), keys.includes(name) ? keys : [...keys, name], JSON.stringify(text));
    for (const m of after) if (m.key === name) assert.equal(JSON.parse(out.text.slice(m.valueStart, m.valueEnd)), value, JSON.stringify(text));
    // the raw text of the other members is unchanged, and so is their order
    const raw = (t, list) => list.filter((m) => m.key !== name).map((m) => t.slice(m.valueStart, m.valueEnd));
    assert.deepEqual(raw(out.text, after), raw(text, before), JSON.stringify(text));
    // outside the edited value, the text is unchanged
    const hit = before.filter((m) => m.key === name);
    if (hit.length === 1) {
      const [m] = hit;
      assert.equal(out.text, text.slice(0, m.valueStart) + JSON.stringify(value) + text.slice(m.valueEnd), JSON.stringify(text));
    }
  }
});

test('set_body_member_decodes_escaped_keys_itself', () => {
  for (const token of ['"\\\\"', '"\\n"', '"\\t"', '"k"', '"\\u00e9"', '"\\ud83d\\ude00"', '"a\\/b"', '"\\""', '"\\b\\f\\r"', '""']) {
    const expected = JSON.parse(`[${token}]`)[0]; // a string value is decoded correctly
    assert.equal(decodeJsonString(token), expected, token);
  }
  // after a key "\\", a key "\n" is still a line feed
  const text = '{"\\\\":1,"\\n":2}';
  assert.deepEqual(topLevelMembers(text).members.map((m) => m.key), ['\\', '\n']);
  assert.equal(replaceMember(text, '\n', 'v').text, '{"\\\\":1,"\\n":"v"}');
});
