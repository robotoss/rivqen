// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors
//
// H-62 (3), UTF-8 only: the charset parameter of Content-Type (WHATWG MIME
// Sniffing "parse a MIME type" and Encoding Standard "get an encoding"), the
// server rules of negotiation section 4 (steps 2 and 3) and the client rule of
// manifest-patch section 1.7 (client item 1). A seeded property test checks
// the invariant: every F of the server has exactly one line
// "Content-Type: text/html; charset=utf-8" (H-66), and only a charset that is
// not UTF-8 passes the page through.

import assert from 'node:assert/strict';
import { test } from 'node:test';
import { Charset, UTF8_LABELS, charsetParameter, contentTypeCharset, isUtf8Label } from '../src/http.mjs';
import { decodedAsUtf8, handleResponse } from '../src/client.mjs';
import { FULL_CONTENT_TYPE, fullContentType, handle } from '../src/server.mjs';
import { APP, clientContext, deltaRequest, firstVisit, page, revisions } from './helpers.mjs';

const typed = (value) => [['content-type', value]];
const appWith = (value) => ({ status: 200, headers: [['content-type', value], ['cache-control', 'no-cache']] });
const header = (r, name) => r.headers.filter(([n]) => n === name).map(([, v]) => v).join(', ') || null;

test('the six UTF-8 labels of the Encoding Standard, ASCII case-insensitive, without outer ASCII white space', () => {
  assert.deepEqual(UTF8_LABELS, ['unicode-1-1-utf-8', 'unicode11utf8', 'unicode20utf8', 'utf-8', 'utf8', 'x-unicode20utf8']);
  for (const label of ['utf-8', 'UTF-8', 'Utf8', 'unicode-1-1-UTF-8', 'X-Unicode20UTF8', ' utf-8\t', '\f\n\rutf8 ']) {
    assert.equal(isUtf8Label(label), true, JSON.stringify(label));
  }
  // No other mapping: not latin1 or UTF-16, no inner space, no Unicode case folding (Kelvin sign, long s), no NBSP trimming.
  for (const label of ['', 'utf-16', 'utf-16le', 'iso-8859-1', 'us-ascii', 'utf 8', 'utf_8', 'utf-8;', '"utf-8"', 'utf-8 ', 'utſ-8', 'x-user-defined']) {
    assert.equal(isUtf8Label(label), false, JSON.stringify(label));
  }
});

test('charset parameter: quoted values lose their quotes and escapes; unquoted values lose trailing white space', () => {
  const cases = [
    ['text/html', null],
    ['text/html;charset=utf-8', 'utf-8'],
    ['text/html; CHARSET=UTF-8', 'UTF-8'],
    ['text/html; charset="UTF-8"', 'UTF-8'],
    ['text/html; charset="u\\tf-8"', 'utf-8'],
    ['text/html; charset="utf-8" ; x=1', 'utf-8'],
    ['text/html; charset="utf-8"junk', 'utf-8'],
    ['text/html; charset=utf-8 \t', 'utf-8'],
    ['text/html; charset="utf-8', 'utf-8'],
    ['text/html; charset="a\\', 'a\\'],
    ['text/html; charset=""', ''],
    ['text/html; x=1; charset=iso-8859-1', 'iso-8859-1'],
    ['text/html; charset=iso-8859-1; charset=utf-8', 'iso-8859-1'], // the first parameter of a name wins
    ['text/html; charset=; charset=utf-8', 'utf-8'], // an empty unquoted value is skipped
    ['text/html; charset; charset=utf-8', 'utf-8'], // a name without "=" is skipped
    ['text/html; charset =utf-8', null], // the name "charset " is another name
    ['text/html; charset', null],
    ['text/html; charset=', null],
    ['text/html; charset="é"', 'é'],
    ['text/html; charset="Ā"; charset=utf-8', 'utf-8'], // not a quoted-string token code point: skipped
    ['text/html;;; charset=utf-8', 'utf-8'],
  ];
  for (const [value, want] of cases) assert.equal(charsetParameter(value), want, value);
});

test('Content-Type classes: none, utf-8, other; more than one line or media type is other', () => {
  assert.equal(contentTypeCharset([]), Charset.NONE);
  assert.equal(contentTypeCharset(typed('text/html')), Charset.NONE);
  assert.equal(contentTypeCharset(typed('text/html; charset=utf-8')), Charset.UTF8);
  assert.equal(contentTypeCharset(typed('text/html; charset=iso-2022-jp')), Charset.OTHER);
  assert.equal(contentTypeCharset(typed('text/html; charset=""')), Charset.OTHER);
  assert.equal(contentTypeCharset(typed('text/html; x="a,b"; charset=utf-8')), Charset.UTF8);
  // The Fetch Standard uses the last media type of a list; the reference fails closed.
  assert.equal(contentTypeCharset(typed('text/html; charset="utf-8", text/html; charset=iso-2022-jp')), Charset.OTHER);
  assert.equal(contentTypeCharset(typed('text/html, text/html; charset=utf-8')), Charset.OTHER);
  assert.equal(contentTypeCharset([['content-type', 'text/html; charset=utf-8'], ['content-type', 'text/html; charset=utf-8']]), Charset.OTHER);
  assert.equal(contentTypeCharset([['content-type', 'text/html; charset=utf-8'], ['x-other', 'charset=latin1']]), Charset.UTF8);
});

// Server, negotiation section 4.
test('step 2: a charset that is not a UTF-8 label passes A through, also for a request with the revision of F', () => {
  for (const value of ['text/html; charset=iso-8859-1', 'text/html; charset=windows-1252', 'text/html; charset=""', 'text/html; charset=utf-16']) {
    const r = handle(firstVisit(), appWith(value), page('page-a'));
    assert.deepEqual([r.kind, r.reason, header(r, 'content-type'), header(r, 'rq-version')], ['app', 'charset', value, null], value);
    const nm = handle(deltaRequest('page-a'), appWith(value), page('page-a'));
    assert.deepEqual([nm.status, nm.kind], [200, 'app'], value);
  }
});

test('H-66, step 3: F has exactly one line "Content-Type: text/html; charset=utf-8", whatever the text of A; the ETag and the 304 do not change', () => {
  const utf8 = handle(firstVisit(), APP, page('page-a'));
  for (const value of ['text/html', 'Text/HTML  ', 'text/html; x=1', 'text/html; charset="UTF-8"', 'text/html;charset=utf8', 'text/html; charset=unicode-1-1-utf-8',
    'text/html; charset = utf-8', 'text/html; charset =latin1', 'text/html; x="abc', 'text/html; charset="utf-8" ; x=1']) {
    const r = handle(firstVisit(), appWith(value), page('page-a'));
    assert.equal(r.kind, 'full', value);
    assert.deepEqual(r.headers.filter(([n]) => n === 'content-type'), [['content-type', FULL_CONTENT_TYPE]], value);
    assert.equal(header(r, 'etag'), header(utf8, 'etag'), value);
    assert.deepEqual(r.body, utf8.body, value);
    // HEAD of F has the same field.
    assert.equal(header(handle(firstVisit('HEAD'), appWith(value), page('page-a')), 'content-type'), FULL_CONTENT_TYPE, value);
  }
  assert.equal(FULL_CONTENT_TYPE, 'text/html; charset=utf-8');
  // The line keeps its position among the fields of A.
  const r = handle(firstVisit(), { status: 200, headers: [['x-a', '1'], ['content-type', 'text/html'], ['x-b', '2']] }, page('page-a'));
  assert.deepEqual(r.headers.slice(0, 3), [['x-a', '1'], ['content-type', FULL_CONTENT_TYPE], ['x-b', '2']]);
  // A 304 has no Content-Type, and its ETag is the ETag of F.
  const nm = handle(deltaRequest('page-a'), appWith('text/html'), page('page-a'));
  assert.deepEqual([nm.status, header(nm, 'content-type'), header(nm, 'etag')], [304, null, `"${revisions('page-a').page_revision}"`]);
  // A delta has the delta media type.
  const d = handle(deltaRequest('page-big-a'), appWith('text/html; charset="UTF-8"'), page('page-big-b'));
  assert.deepEqual([d.kind, header(d, 'content-type')], ['delta', 'application/vnd.rivqen.delta+json']);
});

test('H-66: the charset rule only decides the passthrough; an unclosed quoted string is no charset, so F', () => {
  assert.deepEqual(fullContentType(typed('text/html; x="abc')), typed(FULL_CONTENT_TYPE));
  assert.deepEqual(fullContentType([['content-type', 'text/html'], ['vary', 'cookie']]), [['content-type', FULL_CONTENT_TYPE], ['vary', 'cookie']]);
  for (const value of ['text/html; charset=latin1', 'text/html; charset=""', 'text/html; charset=utf-8, text/html']) {
    assert.equal(fullContentType(typed(value)), null, value);
  }
  assert.equal(fullContentType([['content-type', 'text/html; charset=utf-8'], ['content-type', 'text/html; charset=utf-8']]), null);
});

test('property: for any Content-Type of A, F has exactly the Content-Type of H-66, and nothing throws', () => {
  let s = 0x0c4a5e7;
  const rnd = () => {
    s = (Math.imul(s, 1103515245) + 12345) >>> 0;
    return s / 4294967296;
  };
  const parts = ['text/html', ';', ' ', '\t', 'charset', '=', '"', '\\', ',', 'utf-8', 'UTF8', 'latin1', 'x', 'é', 'Ā', ''];
  let full = 0;
  for (let i = 0; i < 2000; i++) {
    let value = 'text/html';
    const n = Math.floor(rnd() * 8);
    for (let k = 0; k < n; k++) value += parts[Math.floor(rnd() * parts.length)];
    const r = handle(firstVisit(), appWith(value), page('page-a'));
    if (r.kind === 'full') {
      full++;
      assert.deepEqual(r.headers.filter(([n]) => n === 'content-type'), [['content-type', FULL_CONTENT_TYPE]], value);
      assert.notEqual(contentTypeCharset(typed(value)), Charset.OTHER, value);
    } else {
      assert.equal(r.kind, 'app', value);
      // Only the charset rule or the scope (a media type that is not text/html) passes the page through.
      if (r.reason === 'charset') assert.equal(contentTypeCharset(typed(value)), Charset.OTHER, value);
      else assert.equal(r.reason, 'out-of-scope', value);
    }
    const c = handleResponse(clientContext(), {
      status: 200,
      headers: [['content-type', value], ['rq-version', '1']],
      body: handle(firstVisit(), APP, page('page-a')).body,
    });
    assert.ok(['stored', 'rejected', 'normal'].includes(c.outcome), value);
  }
  assert.ok(full > 200, `only ${full} full documents`);
});

// Client, manifest-patch section 1.7, client item 1.
const fullA = () => handle(firstVisit(), APP, page('page-a')).body;
const first = clientContext();
const respond = (contentType, body = fullA()) => handleResponse(first, { status: 200, headers: [['content-type', contentType], ['rq-version', '1']], body });

test('client: F is used only with a UTF-8 charset label or a UTF-8 byte order mark', () => {
  assert.equal(respond('text/html; charset=utf-8').outcome, 'stored');
  assert.equal(respond('text/html; charset="UTF8"').outcome, 'stored');
  for (const value of ['text/html; charset=iso-2022-jp', 'text/html', 'text/html; charset=utf-16', 'text/html; charset=utf-8, text/html']) {
    assert.deepEqual([respond(value).outcome, respond(value).error, respond(value).next, respond(value).snapshot], ['rejected', 'MANIFEST_INVALID', 'show', null], value);
  }
  // F of page-bom (the BOM is part of the template, so the manifest is the manifest of that page).
  const bom = handle(firstVisit(), APP, page('page-bom')).body;
  assert.deepEqual([...bom.subarray(0, 3)], [0xef, 0xbb, 0xbf]);
  assert.equal(respond('text/html', bom).outcome, 'stored');
  assert.equal(respond('text/html; charset=iso-2022-jp', bom).outcome, 'stored'); // BOM sniffing comes first
  assert.equal(decodedAsUtf8(typed('text/html'), Buffer.from([0xef, 0xbb])), false);
  assert.equal(decodedAsUtf8(typed('text/html'), Buffer.from([0xef, 0xbb, 0x41])), false); // the BOM is all three bytes
  assert.equal(decodedAsUtf8(typed('text/html'), Buffer.from([0xfe, 0xff, 0x00])), false);
  assert.equal(decodedAsUtf8(typed('text/html'), Buffer.from([0xef, 0xbb, 0xbf])), true);
});

test('client: without Rq-Version 1 a non-UTF-8 document is a normal page with no error', () => {
  const r = handleResponse(first, { status: 200, headers: typed('text/html; charset=iso-2022-jp'), body: fullA() });
  assert.deepEqual([r.outcome, r.error], ['normal', null]);
});
