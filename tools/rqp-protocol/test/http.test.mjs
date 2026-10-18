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
  assert.deepEqual(parseEntityTags(', ,"a",\t"b"\t, W/"c"'), {
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
