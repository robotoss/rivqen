// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors
//
// Structured Field Values (RFC 9651 section 4.2) and a seeded property test:
// no request field or response body makes a reference step throw.

import assert from 'node:assert/strict';
import { test } from 'node:test';
import { parseDictionary, parseItem, parseList } from '../src/sf.mjs';
import { acceptsMediaType, parseEntityTags } from '../src/http.mjs';
import { parseStrict } from '../src/json-strict.mjs';
import { handle } from '../src/server.mjs';
import { handleResponse, snapshotOf } from '../src/client.mjs';
import { APP, clientContext, deltaRequest, page } from './helpers.mjs';

const item = (v) => parseItem(v)?.item ?? null;

test('Item: every bare item type', () => {
  assert.deepEqual(item('42'), { type: 'integer', value: 42 });
  assert.deepEqual(item('-0'), { type: 'integer', value: -0 });
  assert.deepEqual(item('1.5'), { type: 'decimal', value: 1.5 });
  assert.deepEqual(item('"a\\"b"'), { type: 'string', value: 'a"b' });
  assert.deepEqual(item('t1.x_-:/'), { type: 'token', value: 't1.x_-:/' });
  assert.deepEqual(item('*x'), { type: 'token', value: '*x' });
  assert.deepEqual(item(':AQID:'), { type: 'bytes', value: Buffer.from([1, 2, 3]) });
  assert.deepEqual(item('?0'), { type: 'boolean', value: false });
  assert.deepEqual(item('@1659578233'), { type: 'date', value: 1659578233 });
  assert.deepEqual(item('%"f%c3%bc"'), { type: 'displaystring', value: 'fü' });
});

test('Item: parse failures of RFC 9651 section 4.2', () => {
  const failures = ['', '1.', '1.1234', '1234567890123456', '12345678901234.5', '"a', '"\\x"', '"\u0001"', ':AQ=D:', ':AQID', ':A:', '?2', '@1.5',
    '%"F%C3%BC"', '%"%ff"', 'a b', '1;', '1;A=1', '(', 'é', '1,', '-', '-a'];
  for (const f of failures) assert.equal(parseItem(f), null, JSON.stringify(f));
});

test('Byte Sequence: missing padding is accepted', () => {
  assert.deepEqual(item(':AQI:'), { type: 'bytes', value: Buffer.from([1, 2]) });
  assert.deepEqual(item(':AQI=:'), { type: 'bytes', value: Buffer.from([1, 2]) });
});

test('List and Dictionary: members, parameters, white space and trailing commas', () => {
  const list = parseList('(a b);x=1, c , ("s" 2)');
  assert.equal(list.length, 3);
  assert.equal(list[0].inner, true);
  assert.equal(list[0].params.get('x').value, 1);
  assert.equal(list[1].item.value, 'c');
  assert.equal(parseList('a,'), null);
  assert.equal(parseList('(a b'), null);
  assert.equal(parseList('(a,b)'), null);
  assert.deepEqual(parseList(''), []);
  const dict = parseDictionary('a=1, b, c=(x);p, a=2');
  assert.equal(dict.get('a').item.value, 2);
  assert.equal(dict.get('b').item.value, true);
  assert.equal(dict.get('c').inner, true);
  assert.equal(parseDictionary('A=1'), null);
  assert.equal(parseDictionary('a=1,,b=2'), null);
});

test('Item: the digit limits of Integer and Decimal (RFC 9651 section 4.2.4)', () => {
  assert.deepEqual(item('999999999999999'), { type: 'integer', value: 999999999999999 });
  assert.deepEqual(item('-999999999999999'), { type: 'integer', value: -999999999999999 });
  // 12 integer digits and 3 fraction digits: the longest Decimal (16 characters).
  assert.deepEqual(item('123456789012.123'), { type: 'decimal', value: 123456789012.123 });
  assert.deepEqual(item('1.123'), { type: 'decimal', value: 1.123 });
  for (const f of ['1234567890123.1', '1.2.3', '-;a', '1.1234']) assert.equal(parseItem(f), null, f);
  assert.equal(parseList('-, 1'), null);
});

test('Item: the characters of String and Display String (RFC 9651 sections 4.2.5, 4.2.10)', () => {
  assert.deepEqual(item('"a\\\\b"'), { type: 'string', value: 'a\\b' });
  assert.deepEqual(item('?1'), { type: 'boolean', value: true });
  assert.deepEqual(item(' 1 '), { type: 'integer', value: 1 });
  // A control character or DEL is not allowed in either string type.
  for (const f of ['"a\u001fb"', '"a\u007f"', '%"a\u001f"', '%"a\u007f"', '%abc"', '%"%c3"%bc"', '!"a"']) {
    assert.equal(parseItem(f), null, JSON.stringify(f));
  }
});

test('Item: Byte Sequence characters and padding (RFC 9651 section 4.2.7)', () => {
  for (const f of [':ab$d:', ':YQ===:', ':ab-c:']) assert.equal(parseItem(f), null, f);
});

test('parameters and separators: SP after ";", a bare key is true, OWS around ","', () => {
  const p = parseItem('a; b=1;c');
  assert.equal(p.params.get('b').value, 1);
  assert.deepEqual(p.params.get('c'), { type: 'boolean', value: true });
  assert.deepEqual(parseDictionary('a').get('a'), { inner: false, item: { type: 'boolean', value: true }, params: new Map() });
  assert.equal(parseDictionary('*a=1').get('*a').item.value, 1);
  assert.deepEqual([...parseDictionary('a=1 ,\tb=2').keys()], ['a', 'b']);
  assert.deepEqual(parseList('a,\tb').map((m) => m.item.value), ['a', 'b']);
  assert.equal(parseList('a, '), null); // a trailing comma
  assert.equal(parseList('a bc'), null); // no comma between members
  assert.equal(parseDictionary('a=1,'), null);
  assert.equal(parseDictionary('a=1 bc=2'), null); // no comma between members
  assert.equal(parseList('(a"b")'), null); // no SP between inner list items
  // A non-string value is ignored, as a parse failure.
  assert.equal(parseItem(undefined), null);
});

/** Deterministic PRNG (mulberry32), fixed seed. */
function prng(seed) {
  let s = seed >>> 0;
  return () => {
    s = (s + 0x6d2b79f5) >>> 0;
    let t = s;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

const ALPHABET = ' \t,;=:()"\\?@%*-.0123456789abtW/rAZ_+é{}[]<>&\u0001';

function randomText(rnd, max) {
  let s = '';
  const n = Math.floor(rnd() * max);
  for (let i = 0; i < n; i++) s += ALPHABET[Math.floor(rnd() * ALPHABET.length)];
  return s;
}

test('property: random field values never make the parsers or the server step throw', () => {
  const rnd = prng(0x5eed);
  const fields = ['rq-version', 'rq-capabilities', 'rq-have', 'if-none-match', 'accept', 'range'];
  for (let i = 0; i < 1500; i++) {
    const v = randomText(rnd, 60);
    parseItem(v);
    parseList(v);
    parseDictionary(v);
    parseEntityTags(v);
    acceptsMediaType(v, 'application/vnd.rivqen.delta+json');
    const field = fields[i % fields.length];
    const r = handle(deltaRequest('page-big-a', { [field]: v }), APP, page('page-big-b'));
    assert.ok(['full', 'delta', 'none'].includes(r.kind));
  }
});

test('property: random bodies never make the client step throw', () => {
  const rnd = prng(0xc11e);
  const snapshot = snapshotOf(page('page-a'));
  const ctx = clientContext({ navigation: 'delta', snapshot });
  const valid = JSON.stringify({ type: 'rivqen.patch', protocol: 'rqp/1', operations: [] });
  for (let i = 0; i < 1500; i++) {
    // Random text, and a valid prefix with a random tail.
    const body = Buffer.from(i % 2 === 0 ? randomText(rnd, 80) : valid.slice(0, Math.floor(rnd() * valid.length)) + randomText(rnd, 20), 'utf8');
    parseStrict(body);
    const delta = handleResponse(ctx, { status: 200, headers: [['content-type', 'application/vnd.rivqen.delta+json']], body });
    assert.equal(delta.outcome, 'rejected');
    // A UTF-8 charset (H-62), so that the body reaches the manifest checks.
    const full = handleResponse(ctx, { status: 200, headers: [['content-type', 'text/html; charset=utf-8'], ['rq-version', '1']], body });
    assert.ok(['rejected', 'stored'].includes(full.outcome));
  }
});
