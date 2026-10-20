// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors
//
// HTTP field helpers of src/http.mjs: header line forms, combined values,
// media types, entity-tag lists (RFC 9110 section 8.8.3), comma lists with
// quoted strings, the charset parameter and the q value of Accept.

import assert from 'node:assert/strict';
import { test } from 'node:test';
import {
  Charset,
  acceptsMediaType,
  charsetParameter,
  combined,
  contentTypeCharset,
  isUtf8Label,
  mediaType,
  normalizeHeaders,
  parseEntityTags,
  splitList,
  startsWithUtf8Bom,
  trimChars,
  trimOws,
  trimOwsEnd,
  varyMembers,
} from '../src/http.mjs';
import { cacheDirectives, decodedAsUtf8 } from '../src/client.mjs';

const DELTA = 'application/vnd.rivqen.delta+json';

test('header lines: a list, a Map, a Headers object and a plain object give lower-case [name, value] lines', () => {
  assert.deepEqual(normalizeHeaders(null), []);
  assert.deepEqual(normalizeHeaders(undefined), []);
  assert.deepEqual(normalizeHeaders([['ETag', 1]]), [['etag', '1']]);
  assert.deepEqual(normalizeHeaders(new Map([['Rq-Version', '1']])), [['rq-version', '1']]);
  assert.deepEqual(normalizeHeaders(new Headers([['X-A', 'b']])), [['x-a', 'b']]);
  assert.deepEqual(normalizeHeaders({ 'Set-Cookie': ['a=1', 'b=2'], Vary: 'accept' }), [
    ['set-cookie', 'a=1'],
    ['set-cookie', 'b=2'],
    ['vary', 'accept'],
  ]);
});

test('combined value and media type: outer white space goes, inner white space stays', () => {
  assert.equal(combined([['x', ' \t a b \t '], ['x', '  c']], 'X'), 'a b, c');
  assert.equal(mediaType('  Text/HTML \t; charset=utf-8'), 'text/html');
  assert.equal(mediaType('text /html'), 'text /html');
  assert.equal(mediaType(' ;charset=utf-8'), null);
  assert.equal(mediaType(''), null);
});

test('entity-tag lists: empty elements, OWS around commas, the weak flag, and the etagc range', () => {
  assert.deepEqual(parseEntityTags(', ,"a" ,\t"b"\t, W/"c"'), {
    star: false,
    tags: [
      { weak: false, opaque: '"a"' },
      { weak: false, opaque: '"b"' },
      { weak: true, opaque: '"c"' },
    ],
  });
  assert.deepEqual(parseEntityTags('*'), { star: true, tags: [] });
  assert.deepEqual(parseEntityTags('  "x"  ').tags, [{ weak: false, opaque: '"x"' }]);
  // No opening DQUOTE, a space or a control character in the tag.
  for (const f of ['x"', 'W/x"', '"a b"', '"a\u0001"']) assert.equal(parseEntityTags(f), null, JSON.stringify(f));
});

test('comma lists: a quoted string keeps its commas and escapes; empty elements go', () => {
  assert.deepEqual(splitList('"a\\",b", c'), ['"a\\",b"', 'c']);
  assert.deepEqual(splitList('"a\\'), ['"a\\']);
  assert.deepEqual(splitList('a,, \t ,  b  '), ['a', 'b']);
  // One media type with a trailing comma is still one media type.
  assert.equal(contentTypeCharset([['content-type', 'text/html; charset=utf-8,']]), Charset.UTF8);
});

test('charset parameter: HTTP white space after ";", parameters only after the first ";"', () => {
  for (const ws of ['\t', '\n', '\r', ' ']) assert.equal(charsetParameter(`text/html;${ws}charset=utf-8`), 'utf-8', JSON.stringify(ws));
  assert.equal(charsetParameter('charset=utf-8'), null);
  assert.equal(charsetParameter('text/html; charset= utf-8'), ' utf-8'); // white space after "=" is part of the value
  assert.equal(charsetParameter('text/html; charset=utf-8 \t'), 'utf-8'); // trailing white space goes
  assert.equal(charsetParameter('xcharset=utf-8'), null);
  assert.equal(charsetParameter(';charset=utf-8'), 'utf-8');
  // After a quoted value, the rest up to the next ";" is ignored.
  assert.equal(charsetParameter('text/html; a="b"xcharset=utf-8'), null);
  assert.equal(charsetParameter('text/html;a="b";charset=utf-8'), 'utf-8');
  // Only trailing white space leaves an unquoted value.
  assert.equal(charsetParameter('text/html; charset=utf -8'), 'utf -8');
  assert.equal(isUtf8Label(' \tutf-8 \t'), true);
});

test('UTF-8 byte order mark: exactly EF BB BF at offset 0', () => {
  assert.equal(startsWithUtf8Bom(Buffer.from([0xef, 0xbb, 0xbf])), true);
  for (const b of [[0xef, 0xbb], [0xef, 0xbb, 0x00], [0xef, 0x00, 0xbf], [0x00, 0xbb, 0xbf]]) {
    assert.equal(startsWithUtf8Bom(Buffer.from(b)), false, JSON.stringify(b));
  }
  assert.equal(decodedAsUtf8([['content-type', 'text/html']], Buffer.from([0xe0, 0xbb, 0xbf, 0x3c])), false);
});

test('Accept: the q value grammar of RFC 9110 section 12.4.2; parameters other than q are ignored', () => {
  for (const ok of [`${DELTA};q=1`, `${DELTA};q=1.00`, `${DELTA} ;q=0.5`, `${DELTA};q=0.5 ;level=1`, `${DELTA};level=1`, `${DELTA};charset=utf-8`, `${DELTA} \t;q=0.5`, `${DELTA};qq`]) {
    assert.equal(acceptsMediaType(ok, DELTA), true, ok);
  }
  for (const no of [`${DELTA};q=20`, `${DELTA};q=0.5x`, `${DELTA}; q=0`, 'application/ vnd.rivqen.delta+json']) {
    assert.equal(acceptsMediaType(no, DELTA), false, no);
  }
});

test('Cache-Control directive names: a name ends at "=" and loses trailing white space only', () => {
  assert.deepEqual([...cacheDirectives('Private \t="x", max-age=0, pub lic')], ['private', 'max-age', 'pub lic']);
});

test('trimChars, trimOws, trimOwsEnd: only the given characters, only at the given ends', () => {
  assert.equal(trimOws(' \t a \t b \t '), 'a \t b');
  assert.equal(trimOws('\n a \r'), '\n a \r'); // OWS is SP and HTAB only
  assert.equal(trimOws(''), '');
  assert.equal(trimOws(' \t '), '');
  assert.equal(trimOwsEnd(' a \t'), ' a');
  const digits = (c) => c >= 0x30 && c <= 0x39;
  assert.equal(trimChars('12a3b45', digits), 'a3b');
  assert.equal(trimChars('12a3b45', digits, { start: false }), '12a3b');
  assert.equal(trimChars('12a3b45', digits, { end: false }), 'a3b45');
  assert.equal(trimChars('123', digits), '');
});

test('property (DL-063): trimOws gives the result of the regular expression it replaced', () => {
  let s = 0x7e1;
  const rnd = () => {
    s = (Math.imul(s, 1103515245) + 12345) >>> 0;
    return s / 4294967296;
  };
  const alphabet = ' \t\n\ra,';
  for (let i = 0; i < 5000; i++) {
    let v = '';
    const n = Math.floor(rnd() * 10);
    for (let k = 0; k < n; k++) v += alphabet[Math.floor(rnd() * alphabet.length)];
    assert.equal(trimOws(v), v.replace(/^[ \t]+|[ \t]+$/g, ''), JSON.stringify(v));
    assert.equal(trimOwsEnd(v), v.replace(/[ \t]+$/, ''), JSON.stringify(v));
  }
});

test('M1 (DL-063): each field helper is linear: 64 KB of white space inside a value takes under 50 ms', () => {
  // A trim with /^[ \t]+|[ \t]+$/g tries the second alternative at each position
  // of a run of white space inside the value: 64 KB took 1.4 s to 3 s per call.
  const ws = ' \t'.repeat(32768);
  const fastestMs = (fn) => {
    let best = Infinity;
    for (let i = 0; i < 3; i++) {
      const t = process.hrtime.bigint();
      fn();
      best = Math.min(best, Number(process.hrtime.bigint() - t) / 1e6);
    }
    return best;
  };
  const calls = {
    combined: () => combined([['x', `a${ws}b`]], 'x'),
    mediaType: () => mediaType(`text/html${ws}x`),
    parseEntityTags: () => parseEntityTags(`"a"${ws}x`),
    splitList: () => splitList(`a${ws}b, c`),
    'acceptsMediaType (essence)': () => acceptsMediaType(`${DELTA}${ws}x`, DELTA),
    'acceptsMediaType (q)': () => acceptsMediaType(`${DELTA};q${ws}=1`, DELTA),
    charsetParameter: () => charsetParameter(`text/html;charset=utf-8${ws}x`),
    isUtf8Label: () => isUtf8Label(`utf-8${ws}x`),
    contentTypeCharset: () => contentTypeCharset([['content-type', `text/html; charset="utf-8${ws}x"`]]),
    cacheDirectives: () => cacheDirectives(`private${ws}x=1`),
    varyMembers: () => varyMembers([['vary', `a${ws}b`]]),
  };
  const slow = Object.entries(calls)
    .map(([name, fn]) => [name, fastestMs(fn)])
    .filter(([, ms]) => ms >= 50);
  assert.deepEqual(slow, []);
});

test('varyMembers (DL-063): lower-case field names of every Vary line, without white space and empty elements', () => {
  assert.deepEqual(varyMembers([['vary', ' Accept-Encoding ,, RQ-Nonce'], ['content-type', 'x'], ['vary', '*']]), ['accept-encoding', 'rq-nonce', '*']);
  assert.deepEqual(varyMembers([['vary', '']]), []);
  assert.deepEqual(varyMembers([]), []);
});
