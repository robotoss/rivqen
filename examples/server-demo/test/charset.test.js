// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors
//
// The charset rule of the Content-Type field (negotiation section 4, H-62, items 1 to 4).
// The expected results are those of the reference server (DL-036: a second implementation,
// compared with the first). The reference is imported by tests only.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { CHARSET, charsetParameter, contentTypeCharset, fullContentType, isUtf8Label } from '../src/fields.js';
import { handle } from '../src/rqp.js';
import { handle as referenceHandle } from '../../../tools/rqp-protocol/src/server.mjs';
import { Charset, contentTypeCharset as referenceCharset } from '../../../tools/rqp-protocol/src/http.mjs';
import { get, header, page } from './helpers.js';

/** The response of A with the given Content-Type field lines. */
const withLines = (...values) => ({ status: 200, headers: [...values.map((v) => ['content-type', v]), ['cache-control', 'no-cache']] });
const run = (...values) => handle(get(), withLines(...values), page());

test('H-62 item 2: the name is not trimmed, so "charset " is not a charset and F gets the parameter', () => {
  for (const value of ['text/html; charset = utf-8', 'text/html; charset =utf-8']) {
    const r = run(value);
    assert.equal(r.mode, 'full', value);
    assert.equal(header(r, 'content-type'), 'text/html; charset=utf-8', value);
  }
});

test('H-66, DL-039: an unclosed quoted string has no charset: F has the exact Content-Type', () => {
  const r = run('text/html; x="abc');
  assert.equal(r.mode, 'full');
  assert.equal(header(r, 'content-type'), 'text/html; charset=utf-8');
});

test('H-62 item 2: an empty unquoted value does not count; the next valid charset parameter wins', () => {
  const none = run('text/html; charset=');
  assert.equal(none.mode, 'full');
  assert.equal(header(none, 'content-type'), 'text/html; charset=utf-8');
  const next = run('text/html; charset=; charset=utf-8');
  assert.equal(next.mode, 'full');
  assert.equal(header(next, 'content-type'), 'text/html; charset=utf-8');
});

test('H-62 item 2: a quoted value loses its quotes and its backslash escapes', () => {
  assert.equal(run('text/html; charset="utf\\-8"').mode, 'full');
  assert.equal(run('text/html; charset="utf-8"x').mode, 'full');
  assert.equal(run('text/html; charset="utf-8 ').mode, 'full'); // no closing quote: the value runs to the end
  assert.equal(run('text/html; charset=" utf-8 "').mode, 'full'); // item 3 trims white space
  assert.equal(run('text/html; charset="utf-8\\').mode, 'passthrough'); // the value is utf-8 with a backslash
});

test('H-62 item 2: the first valid charset parameter wins; a value with a bad code point is not valid', () => {
  assert.equal(run('text/html; charset=iso-8859-1; charset=utf-8').mode, 'passthrough');
  assert.equal(run('text/html; charset=utf-8; charset=iso-8859-1').mode, 'full');
  assert.equal(run('text/html; charset="a\u0001"; charset=utf-8').mode, 'full');
});

test('H-62 item 1: more than one field line or more than one media type fails closed', () => {
  assert.equal(run('text/html; charset=utf-8', 'text/html; charset=utf-8').mode, 'passthrough');
  assert.equal(run('text/html', 'text/html').mode, 'passthrough');
  assert.equal(run('text/html; charset=utf-8, text/plain').mode, 'passthrough');
  assert.equal(run('text/html, text/plain; charset=utf-8').mode, 'passthrough');
});

test('H-62 item 1: a comma inside a quoted string is not a list separator; empty list members are not media types', () => {
  assert.equal(run('text/html; x="a,b"; charset=utf-8').mode, 'full');
  assert.equal(run('text/html; x="a\\",b"; charset=utf-8').mode, 'full');
  assert.equal(run('text/html; charset=utf-8,').mode, 'full');
  assert.equal(contentTypeCharset([['content-type', '']]), CHARSET.OTHER);
  assert.equal(contentTypeCharset([['cache-control', 'no-cache']]), CHARSET.NONE);
});

test('H-66: a value with trailing white space and no charset gives F with the exact Content-Type', () => {
  const r = run('text/html \t ');
  assert.equal(r.mode, 'full');
  assert.equal(header(r, 'content-type'), 'text/html; charset=utf-8');
});

test('H-62 item 3: label comparison trims ASCII white space and ignores case; other values are not UTF-8', () => {
  for (const label of ['UTF-8', ' utf8', 'x-UNICODE20utf8', 'unicode11utf8', 'Unicode20UTF8', 'unicode-1-1-utf-8']) {
    assert.equal(contentTypeCharset([['content-type', `text/html; charset="${label}"`]]), CHARSET.UTF8, label);
  }
  for (const label of ['utf-9', 'utf_8', 'utf-16', '']) {
    assert.equal(contentTypeCharset([['content-type', `text/html; charset="${label}"`]]), CHARSET.OTHER, label);
  }
  // A tab and the code points 80 to FF are valid in a value; other control characters are not (the parameter is skipped).
  assert.equal(contentTypeCharset([['content-type', 'text/html; charset="utf-8\t"']]), CHARSET.UTF8);
  assert.equal(contentTypeCharset([['content-type', 'text/html; charset="é"']]), CHARSET.OTHER);
  assert.equal(contentTypeCharset([['content-type', 'text/html; charset="ÿ"']]), CHARSET.OTHER);
  assert.equal(contentTypeCharset([['content-type', 'text/html; charset="Ā"']]), CHARSET.NONE);
  assert.equal(contentTypeCharset([['content-type', 'text/html; charset="\u007f"']]), CHARSET.NONE);
  // The Encoding Standard trims 09, 0A, 0C, 0D and 20; the media type parser would drop 0A, 0C and 0D earlier.
  for (const space of ['\t', '\n', '\f', '\r', ' ']) assert.equal(isUtf8Label(`${space}utf-8${space}`), true, JSON.stringify(space));
  assert.equal(isUtf8Label('\u000butf-8'), false);
});

test('charsetParameter reads parameters up to ";" and skips names without a value', () => {
  assert.equal(charsetParameter('text/html'), null);
  assert.equal(charsetParameter('text/html; a; charset=x'), 'x');
  assert.equal(charsetParameter('text/html;CHARSET=x'), 'x');
  assert.equal(charsetParameter('text/html; charset'), null);
  assert.equal(charsetParameter('text/html; charset='), null);
  assert.equal(charsetParameter('text/html; charset=x \t'), 'x');
  assert.equal(charsetParameter('text/html; charset=a;b'), 'a');
  assert.equal(charsetParameter('text/html; charset="a;b"'), 'a;b');
});

test('fullContentType (H-66) gives one exact Content-Type line and keeps the other lines', () => {
  const ok = [['content-type', 'text/html; charset="UTF-8"'], ['x', 'y']];
  assert.deepEqual(fullContentType(ok), [['content-type', 'text/html; charset=utf-8'], ['x', 'y']]);
  assert.equal(fullContentType([['content-type', 'text/html; charset=latin1']]), null);
  const appended = fullContentType([['x', 'y'], ['content-type', 'text/html']]);
  assert.deepEqual(appended, [['x', 'y'], ['content-type', 'text/html; charset=utf-8']]);
});

// ---- differential property test against the reference server ----

function mulberry32(seed) {
  let a = seed >>> 0;
  return () => {
    a = (a + 0x6d2b79f5) >>> 0;
    let t = a;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

const FRAGMENTS = [
  ';', ';', '=', '=', '"', '"', '\\', ',', ' ', ' ', '\t', 'charset', 'Charset', 'CHARSET', 'charset ', 'utf-8', 'UTF-8', 'utf8', 'utf-16',
  'unicode-1-1-utf-8', 'x-unicode20utf8', 'iso-8859-1', 'x', 'a', 'text/html', 'text/plain', '; charset=utf-8', '; charset=', '; x="', '\x01',
];

function randomValue(rand) {
  let out = rand() < 0.9 ? 'text/html' : FRAGMENTS[Math.floor(rand() * FRAGMENTS.length)];
  const n = Math.floor(rand() * 9);
  for (let i = 0; i < n; i++) out += FRAGMENTS[Math.floor(rand() * FRAGMENTS.length)];
  return out;
}

test('property: the demo reads the Content-Type like the reference and makes the same response decision', () => {
  const rand = mulberry32(0x5eed62);
  const classes = new Map([[Charset.NONE, CHARSET.NONE], [Charset.UTF8, CHARSET.UTF8], [Charset.OTHER, CHARSET.OTHER]]);
  const seen = new Set();
  for (let i = 0; i < 20000; i++) {
    const lines = rand() < 0.1 ? [randomValue(rand), randomValue(rand)] : [randomValue(rand)];
    const app = withLines(...lines);
    const label = JSON.stringify(lines);
    const actualClass = contentTypeCharset(app.headers);
    assert.equal(actualClass, classes.get(referenceCharset(app.headers)), label);
    seen.add(actualClass);

    const reference = referenceHandle(get(), app, page());
    const demo = handle(get(), app, page());
    assert.equal(demo.mode === 'full', reference.kind === 'full', label);
    // H-66: the reference and the demo give F the exact value; the test does not compare that text
    // with the reference (the reference changes in a parallel lane). The demo value is asserted here.
    if (demo.mode === 'full') {
      assert.deepEqual(demo.headers.filter(([n]) => n === 'content-type'), [['content-type', 'text/html; charset=utf-8']], label);
    }
    assert.deepEqual(demo.body, reference.body, label);
  }
  assert.deepEqual([...seen].sort(), [CHARSET.NONE, CHARSET.OTHER, CHARSET.UTF8]);
});
