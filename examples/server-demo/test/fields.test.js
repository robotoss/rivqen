// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors
//
// Request fields (negotiation section 3) and the Structured Field parser they use.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import {
  acceptsDelta,
  capabilities,
  deltaBase,
  noneMatchFails,
  parseIfNoneMatch,
  requestLines,
  requestVersion,
  rqHave,
} from '../src/fields.js';
import { parseDictionary, parseItem, parseList } from '../src/sf.js';

const lines = (name, ...values) => values.map((v) => [name, v]);
const R1 = `r1.${'A'.repeat(43)}`;

test('Rq-Version is supported only as the Integer 1', () => {
  assert.equal(requestVersion(lines('rq-version', '1')), 1);
  assert.equal(requestVersion(lines('rq-version', '1;x=y')), 1);
  for (const bad of ['2', '1.0', '"1"', 'one', '?1', '', '1, 1']) assert.notEqual(requestVersion(lines('rq-version', bad)), 1, bad);
  assert.equal(requestVersion([]), null);
});

test('Rq-Capabilities is an Inner List of Tokens; every other shape is the empty set', () => {
  assert.deepEqual([...capabilities(lines('rq-capabilities', '(block-patch realtime-ws);v=2'))], ['block-patch', 'realtime-ws']);
  assert.deepEqual([...capabilities(lines('rq-capabilities', '()'))], []);
  for (const bad of ['block-patch', '(block-patch "x")', '(block-patch), (realtime-ws)', '(block-patch', 'a b']) {
    assert.equal(capabilities(lines('rq-capabilities', bad)).size, 0, bad);
  }
  assert.equal(capabilities(lines('rq-capabilities', '(block-patch)', '(realtime-ws)')).size, 0); // two lines
  const long = `(${'a'.repeat(1022)})`;
  assert.equal(capabilities(lines('rq-capabilities', long)).size, 1);
  assert.equal(capabilities(lines('rq-capabilities', `(${'a'.repeat(1023)})`)).size, 0);
});

test('Rq-Have gives t and b; a wrong type of b leaves t; a wrong t or a parse failure is absent', () => {
  const t = `t1.${'B'.repeat(43)}`;
  const ok = rqHave(lines('rq-have', `t=${t}, b=:AAECAwQFBgc=:`));
  assert.equal(ok.t, t);
  assert.deepEqual([...ok.b], [0, 1, 2, 3, 4, 5, 6, 7]);
  assert.equal(rqHave(lines('rq-have', `t=${t}, b=?1`)).b, null);
  assert.equal(rqHave(lines('rq-have', `t=${t}, b="x"`)).b, null);
  assert.equal(rqHave(lines('rq-have', `t=${t}`)).b, null);
  assert.equal(rqHave(lines('rq-have', `t="${t}"`)), null);
  assert.equal(rqHave(lines('rq-have', 'b=:AA==:')), null);
  assert.equal(rqHave(lines('rq-have', `t=${t}, b=:`)), null);
  assert.equal(rqHave(lines('rq-have', `t=${t}, b=:AAECAwQFBgc=:, t=other`)).t, 'other'); // the last key wins
  assert.equal(rqHave(lines('rq-have', `t=${t}`, 'b=:AA==:')).b.length, 1); // two lines are joined with ", "
});

test('Rq-Have longer than 4096 bytes is absent without a parse', () => {
  const t = `t1.${'B'.repeat(43)}`;
  const pad = (n) => `t=${t}, x=${'a'.repeat(n - t.length - 6)}`;
  assert.notEqual(rqHave(lines('rq-have', pad(4096))), null);
  assert.equal(rqHave(lines('rq-have', pad(4097))), null);
});

test('If-None-Match: a base needs exactly one r1 tag, weak or strong; 304 uses weak comparison', () => {
  assert.equal(deltaBase(lines('if-none-match', `"${R1}"`)), R1);
  assert.equal(deltaBase(lines('if-none-match', `W/"${R1}"`)), R1);
  for (const bad of ['*', `"${R1}", "${R1}"`, '"x"', `"${R1}`, `"t1.${'A'.repeat(43)}"`, `"${R1}x"`]) {
    assert.equal(deltaBase(lines('if-none-match', bad)), null, bad);
  }
  assert.equal(deltaBase([]), null);
  assert.equal(noneMatchFails(lines('if-none-match', '*'), `"${R1}"`), true);
  assert.equal(noneMatchFails(lines('if-none-match', `"x", W/"${R1}"`), `"${R1}"`), true);
  assert.equal(noneMatchFails(lines('if-none-match', `"${R1}"`), `W/"${R1}"`), true);
  assert.equal(noneMatchFails(lines('if-none-match', '"other"'), `"${R1}"`), false);
  assert.equal(noneMatchFails(lines('if-none-match', 'not a tag'), `"${R1}"`), false);
  assert.deepEqual(parseIfNoneMatch('"a",,"b"').tags.map((t) => t.opaque), ['a', 'b']);
});

test('If-None-Match: empty list elements are allowed, also first and last (RFC 9110 section 5.6.1.2)', () => {
  const tags = (v) => parseIfNoneMatch(v)?.tags.map((t) => t.opaque);
  assert.deepEqual(tags(',"a"'), ['a']);
  assert.deepEqual(tags(' , ,"a" ,, \t,"b",'), ['a', 'b']);
  assert.equal(parseIfNoneMatch(','), null);
  assert.equal(parseIfNoneMatch(''), null);
  assert.equal(deltaBase(lines('if-none-match', `, "${R1}",`)), R1);
  assert.equal(noneMatchFails(lines('if-none-match', `,,"${R1}"`), `"${R1}"`), true);
});

test('Accept lists the delta type only as itself and with a weight above 0', () => {
  const accept = (v) => acceptsDelta(lines('accept', v));
  assert.equal(accept('text/html, application/vnd.rivqen.delta+json'), true);
  assert.equal(accept('APPLICATION/VND.RIVQEN.DELTA+JSON;q=0.5'), true);
  assert.equal(accept('application/vnd.rivqen.delta+json;q=0'), false);
  assert.equal(accept('application/vnd.rivqen.delta+json;q=0.000'), false);
  assert.equal(accept('application/vnd.rivqen.delta+json;q=2'), false);
  assert.equal(accept('application/vnd.rivqen.delta+json;q=0;q=1'), true); // the last q wins
  assert.equal(accept('application/vnd.rivqen.delta+json;q=1;q=0'), false);
  assert.equal(accept('application/vnd.rivqen.delta+json;q=1;q=2'), false);
  for (const no of ['*/*', 'application/*', 'text/html', 'application/vnd.rivqen.delta+json2', '']) assert.equal(accept(no), false, no);
  assert.equal(acceptsDelta([]), false);
});

test('request lines keep every field line, in order, with lower-case names', () => {
  assert.deepEqual(requestLines(['Rq-Have', 'a', 'X', 'b', 'rq-have', 'c']), [['rq-have', 'a'], ['x', 'b'], ['rq-have', 'c']]);
});

test('Structured Field items: types, parameters and failures', () => {
  const type = (text) => parseItem(text)?.type;
  assert.deepEqual(['1', '-12', '1.5', '"a\\"b"', 'tok:en/x', ':AA==:', '?0', '@1659578233', '%"f%c3%bc"'].map(type), [
    'integer', 'integer', 'decimal', 'string', 'token', 'bytes', 'boolean', 'date', 'displaystring',
  ]);
  assert.equal(parseItem('"a\\"b"').value, 'a"b');
  assert.equal(parseItem('%"f%c3%bc"').value, 'fü');
  assert.equal(parseItem(' 5;a=1 ').params.get('a').value, 1);
  for (const bad of ['', '1.', '1234567890123456', '1.2345', '"abc', '"a\\x"', ':a:b:', ':A:', '?2', '@1.5', '%"%C3"', '%"%ff"', '1 2', '(a)']) {
    assert.equal(parseItem(bad), null, bad);
  }
  assert.equal(parseItem('123456789012345').value, 123456789012345);
});

test('Structured Field lists and dictionaries: members, inner lists, white space and trailing comma', () => {
  assert.equal(parseList('a, (b c);p=1,\td').length, 3);
  assert.equal(parseList('').length, 0);
  assert.equal(parseList('a,'), null);
  assert.equal(parseList('(a,b)'), null);
  assert.equal(parseList('(a'), null);
  const d = parseDictionary('a, b=2, c=(1 2), d;x=y');
  assert.equal(d.get('a').value, true);
  assert.equal(d.get('c').items.length, 2);
  assert.equal(d.get('d').params.get('x').value, 'y');
  assert.equal(parseDictionary('A=1'), null);
  assert.equal(parseDictionary('a=1,'), null);
  assert.equal(parseDictionary('a=1 b=2'), null);
  assert.equal(parseDictionary('').size, 0);
});

test('property: random field values never make the parsers throw', () => {
  let seed = 11;
  const next = () => (seed = (seed * 1103515245 + 12345) % 2147483648);
  const alphabet = '(),;=:?"\\ \t%@-.*/abtq019AZ_+~é';
  for (let i = 0; i < 2000; i++) {
    let text = '';
    for (let n = next() % 24; n > 0; n--) text += alphabet[next() % alphabet.length];
    for (const parse of [parseItem, parseList, parseDictionary]) {
      const out = parse(text);
      assert.ok(out === null || typeof out === 'object', text);
    }
  }
});
