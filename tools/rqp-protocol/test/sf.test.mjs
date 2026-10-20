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
import { handle, requestVersion, rqNonce } from '../src/server.mjs';
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
  for (const f of [':ab$d:', ':YQ===:', ':ab-c:', ':ab{d:', ':ab~d:', ':ab\u00e9d:']) assert.equal(parseItem(f), null, f);
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

// ---------------------------------------------------------------------------
// M1 (S6 final security review, DL-063): the parse is linear. Before DL-063 the
// Byte Sequence parse removed the padding with /=+$/ after a check that allowed
// "=" anywhere, so ":" + "=" x n + "A:" took quadratic time (64 KB: 1.2 s;
// 256 KB: 41 s), through Rq-Nonce and Rq-Version (no RQP byte limit). The OWS
// trim of combined() (src/http.mjs) had the same defect for white space inside
// a value (64 KB: 1.4 s). Linear work for 64 KB is a few milliseconds; the
// bound of 50 ms leaves room for a slow CI machine and stays far below the
// quadratic times. Each time is the shortest of three runs, so one pause of
// the garbage collector does not fail the test.

const KB64 = 65536;
const LIMIT_MS = 50;

/** The shortest of three runs in milliseconds (the first run also warms up the code). */
function fastestMs(fn) {
  let best = Infinity;
  for (let i = 0; i < 3; i++) {
    const t = process.hrtime.bigint();
    fn();
    best = Math.min(best, Number(process.hrtime.bigint() - t) / 1e6);
  }
  return best;
}

const fill = (unit, n = KB64) => unit.repeat(Math.ceil(n / unit.length)).slice(0, n);

/** Adversarial values of about 64 KB for each bare item type, for parameters and for white space. */
const ADVERSARIAL = {
  'integer, too many digits': fill('9'),
  'decimal, too many digits': `1.${fill('1')}`,
  'string, escapes and no end quote': `"${fill('\\"')}`,
  'string, valid and long': `"${fill('a')}"`,
  'token, valid and long': `a${fill('b')}`,
  'token, then a quote': `${fill('a')}"`,
  'bytes, "=" run then "A" (review M1)': `:${fill('=')}A:`,
  'bytes, "A=" alternating': `:${fill('A=')}A:`,
  'bytes, valid and long': `:${fill('AAAA')}:`,
  'bytes, no end colon': `:${fill('A')}`,
  'bytes, colons only': fill(':'),
  'boolean with many parameters': `?1${fill(';a')}`,
  'boolean with a long parameter key': `?1;${fill('a')}`,
  'date, too many digits': `@${fill('1')}`,
  'display string, valid and long': `%"${fill('%c3%bc')}"`,
  'display string, percent run': `%"${fill('%')}`,
  'parameter with a bytes value': `1;a=:${fill('=')}A:`,
  'white space inside': `1${fill(' \t')}x`,
  'white space only': fill(' '),
};

/** The four RQP request fields of the parse and their shapes (RFC 9651 section 4.2). */
const FIELDS = {
  'rq-version': { parse: parseItem, shapes: (v) => [v] },
  'rq-nonce': { parse: parseItem, shapes: (v) => [v] },
  'rq-capabilities': { parse: parseList, shapes: (v) => [v, `(${v})`, `(a ${v})`] },
  'rq-have': { parse: parseDictionary, shapes: (v) => [`t=${v}`, `t=t1.x, b=${v}`, `b=(${v})`] },
};

test('M1 (DL-063): 64 KB adversarial values of every bare item type parse in under 50 ms in Rq-Version, Rq-Capabilities, Rq-Have and Rq-Nonce', () => {
  const slow = [];
  for (const [field, { parse, shapes }] of Object.entries(FIELDS)) {
    for (const [name, value] of Object.entries(ADVERSARIAL)) {
      for (const shaped of shapes(value)) {
        // The parser alone (as a server without the RQP byte limits would run it) ...
        const parseMs = fastestMs(() => parse(shaped));
        // ... and the full server step with the field in a delta request (combined(), limits, all steps).
        const request = deltaRequest('page-big-a', { [field]: shaped });
        const handleMs = fastestMs(() => handle(request, APP, page('page-big-b')));
        if (parseMs >= LIMIT_MS || handleMs >= LIMIT_MS) slow.push(`${field} ${name}: parse ${parseMs.toFixed(1)} ms, handle ${handleMs.toFixed(1)} ms`);
      }
    }
  }
  assert.deepEqual(slow, []);
});

test('M1 (DL-063) regression: ":" + "=" x 65536 + "A:" is no Byte Sequence and parses in linear time', () => {
  const value = `:${'='.repeat(KB64)}A:`;
  assert.ok(fastestMs(() => assert.equal(parseItem(value), null)) < LIMIT_MS);
  assert.ok(fastestMs(() => assert.equal(parseDictionary(`t=t1.x, b=${value}`), null)) < LIMIT_MS);
  assert.ok(fastestMs(() => assert.equal(parseList(`(${value})`), null)) < LIMIT_MS);
  // Rq-Version has no RQP byte limit, so it reaches the parser.
  assert.ok(fastestMs(() => assert.equal(requestVersion([['rq-version', value]]), null)) < LIMIT_MS);
  // Rq-Nonce: the 128-byte limit stops the line before the parse.
  assert.ok(fastestMs(() => assert.equal(rqNonce([['rq-nonce', value]]), null)) < LIMIT_MS);
  // Linear growth: four times the input stays under four times the bound (quadratic: sixteen times).
  assert.ok(fastestMs(() => parseItem(`:${'='.repeat(4 * KB64)}A:`)) < 4 * LIMIT_MS);
});

/** The Byte Sequence rule of the parse before DL-063 (quadratic, but correct): the oracle of the property test. */
function byteSequenceBeforeDl063(b64) {
  for (const c of b64) if (!/[A-Za-z0-9+/=]/.test(c)) return null;
  const data = b64.replace(/=+$/, '');
  if (data.includes('=') || b64.length - data.length > 2 || data.length % 4 === 1) return null;
  return Buffer.from(data, 'base64');
}

test('property (DL-063): the linear Byte Sequence parse accepts exactly the values of the parse before DL-063, with the same bytes', () => {
  const rnd = prng(0xb64);
  const alphabet = 'AQgw+/=$-_a9';
  let accepted = 0;
  for (let i = 0; i < 20000; i++) {
    let s = '';
    const n = Math.floor(rnd() * 14);
    for (let k = 0; k < n; k++) s += alphabet[Math.floor(rnd() * alphabet.length)];
    const want = byteSequenceBeforeDl063(s);
    const got = parseItem(`:${s}:`);
    if (want === null) {
      assert.equal(got, null, JSON.stringify(s));
    } else {
      accepted++;
      assert.deepEqual(got, { item: { type: 'bytes', value: want }, params: new Map() }, JSON.stringify(s));
    }
  }
  assert.ok(accepted > 500, `only ${accepted} accepted values`);
});

test('Byte Sequence (DL-063): at most two "=" and only at the end; "=" alone is the empty sequence', () => {
  assert.deepEqual(item('::'), { type: 'bytes', value: Buffer.alloc(0) });
  assert.deepEqual(item(':=:'), { type: 'bytes', value: Buffer.alloc(0) });
  assert.deepEqual(item(':==:'), { type: 'bytes', value: Buffer.alloc(0) });
  assert.deepEqual(item(':AQID=:'), { type: 'bytes', value: Buffer.from([1, 2, 3]) });
  assert.deepEqual(item(':AQ==:'), { type: 'bytes', value: Buffer.from([1]) });
  assert.deepEqual(item(':+/+/:'), { type: 'bytes', value: Buffer.from([0xfb, 0xff, 0xbf]) });
  for (const f of [':===:', ':AQ===:', ':A=Q=:', ':=A:', ':A:', ':AAAAA:', ':AAAAA==:', ':A==:', ':AQ=$:', ':AQ-_:', ':AQ.=:']) {
    assert.equal(parseItem(f), null, f);
  }
  // The parse ends at the first ":" after the start; the rest is parameters or a failure.
  assert.deepEqual(parseItem(':AQ==:;a=:AQ==:').params.get('a'), { type: 'bytes', value: Buffer.from([1]) });
  assert.equal(parseItem(':AQ==::'), null);
});
