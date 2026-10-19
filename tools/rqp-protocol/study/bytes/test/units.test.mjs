// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// Unit tests of the measurement helpers: the labelled JSON serializer, the
// sizes, the CBOR encoder, the HTTP/2 frame counter and the legacy breakdown.

import assert from 'node:assert/strict';
import { test } from 'node:test';
import { brotliCompressSync, brotliDecompressSync, gunzipSync, gzipSync } from 'node:zlib';
import { encode } from '../src/cbor.mjs';
import { FrameCounter, http1Bytes } from '../src/hpack.mjs';
import { segments, tally, tallySum, partsOf } from '../src/json-segments.mjs';
import { legacyDataRaw } from '../src/legacy.mjs';
import { prng } from '../src/pages.mjs';
import { sizes } from '../src/sizes.mjs';

const CHARS = ['a', 'Z', '0', ' ', '"', '\\', '/', '<', '\n', '\t', '\u0001', '\u001f', 'é', '—', '😀', ' '];

function randomValue(rand, depth) {
  const r = rand();
  if (depth > 3 || r < 0.35) {
    let s = '';
    const n = Math.floor(rand() * 12);
    for (let i = 0; i < n; i++) s += CHARS[Math.floor(rand() * CHARS.length)];
    return s;
  }
  if (r < 0.45) return Math.floor(rand() * 1e6);
  if (r < 0.5) return rand() < 0.5;
  if (r < 0.55) return null;
  if (r < 0.75) return Array.from({ length: Math.floor(rand() * 4) }, () => randomValue(rand, depth + 1));
  const o = {};
  for (let i = Math.floor(rand() * 4); i > 0; i--) o[`k${Math.floor(rand() * 5)}${CHARS[Math.floor(rand() * 6)]}`] = randomValue(rand, depth + 1);
  return o;
}

test('the labelled serializer writes the bytes of JSON.stringify, and its tally adds up', () => {
  const rand = prng(20261019);
  for (let i = 0; i < 2000; i++) {
    const v = { root: randomValue(rand, 0) };
    const segs = segments(v);
    assert.equal(segs.map((s) => s.text).join(''), JSON.stringify(v));
    const t = tally(segs, new Set(['root']));
    assert.equal(t.total, Buffer.byteLength(JSON.stringify(v)));
    assert.equal(tallySum(t), t.total);
  }
});

test('a content string splits into quotes, decoded bytes and escapes', () => {
  assert.deepEqual(partsOf(JSON.stringify('{"a":"x"}'), true), { quotes: 2, raw: 9, escape: 4 });
  assert.deepEqual(partsOf(JSON.stringify('a\nb'), true), { quotes: 2, raw: 3, escape: 1 });
  assert.deepEqual(partsOf(JSON.stringify('—'), true), { quotes: 2, raw: 3, escape: 0 });
  assert.deepEqual(partsOf(`"r1.${'A'.repeat(43)}"`, false), { quotes: 2, prefix: 3, digest: 43 });
  assert.deepEqual(partsOf(`"${'A'.repeat(43)}"`, false), { quotes: 2, digest: 43 });
  assert.deepEqual(partsOf('"html"', false), { text: 6 });
});

test('sizes gives 0 for an empty body and compressed forms that decode to the body', () => {
  assert.deepEqual(sizes(''), { raw: 0, gzip: 0, br: 0 });
  const body = Buffer.from('<p>Rivqen byte study</p>'.repeat(20));
  const s = sizes(body);
  assert.equal(s.raw, body.length);
  assert.ok(s.gzip < s.raw && s.br < s.raw);
  // The numbers are the lengths of real streams with the default settings.
  const gz = gzipSync(body);
  const br = brotliCompressSync(body);
  assert.equal(s.gzip, gz.length);
  assert.equal(s.br, br.length);
  assert.ok(gunzipSync(gz).equals(body));
  assert.ok(brotliDecompressSync(br).equals(body));
  assert.deepEqual(sizes(body.toString('utf8')), s);
});

test('the CBOR encoder gives the RFC 8949 Appendix A encodings', () => {
  const hex = (v) => encode(v).toString('hex');
  assert.equal(hex(0), '00');
  assert.equal(hex(23), '17');
  assert.equal(hex(24), '1818');
  assert.equal(hex(1000), '1903e8');
  assert.equal(hex(1000000), '1a000f4240');
  assert.equal(hex(1000000000000), '1b000000e8d4a51000');
  assert.equal(hex(''), '60');
  assert.equal(hex('IETF'), '6449455446');
  assert.equal(hex('ü'), '62c3bc');
  assert.equal(hex(Buffer.from([1, 2, 3, 4])), '4401020304');
  assert.equal(hex([]), '80');
  assert.equal(hex([1, [2, 3], [4, 5]]), '8301820203820405');
  assert.equal(hex({ a: 1, b: [2, 3] }), 'a26161016162820203');
  assert.equal(hex(false), 'f4');
  assert.equal(hex(true), 'f5');
  assert.equal(hex(null), 'f6');
  assert.throws(() => encode(-1), { code: 'STUDY_CBOR' });
});

function frame(type, flags, stream, payload) {
  const h = Buffer.alloc(9);
  h.writeUIntBE(payload.length, 0, 3);
  h[3] = type;
  h[4] = flags;
  h.writeUInt32BE(stream, 5);
  return Buffer.concat([h, payload]);
}

test('the frame counter sums header block fragments without padding and priority bytes', () => {
  const c = new FrameCounter(true);
  const preface = Buffer.from('PRI * HTTP/2.0\r\n\r\nSM\r\n\r\n');
  const settings = frame(0x4, 0, 0, Buffer.alloc(6));
  // Stream 1: HEADERS with PADDED (pad length 3) and PRIORITY, fragment 10 bytes, no END_HEADERS; CONTINUATION 7 bytes with END_HEADERS.
  const headers1 = frame(0x1, 0x8 | 0x20, 1, Buffer.concat([Buffer.from([3]), Buffer.alloc(5), Buffer.alloc(10, 0x41), Buffer.alloc(3)]));
  const cont1 = frame(0x9, 0x4, 1, Buffer.alloc(7, 0x42));
  // Stream 3: HEADERS with END_HEADERS, 12 bytes; a later HEADERS (trailers) of stream 3 does not count.
  const headers3 = frame(0x1, 0x4, 3, Buffer.alloc(12));
  const trailers3 = frame(0x1, 0x4 | 0x1, 3, Buffer.alloc(5));
  const all = Buffer.concat([preface, settings, headers1, cont1, headers3, trailers3]);
  // Feed in small pieces: frames can cross chunk borders.
  for (let i = 0; i < all.length; i += 5) c.feed(all.subarray(i, i + 5));
  assert.equal(c.blocks.get(1), 17);
  assert.equal(c.blocks.get(3), 12);
  assert.equal(c.blocks.size, 2);
});

test('HTTP/1.1 field bytes count name, colon, space, value and CRLF', () => {
  assert.equal(http1Bytes([['rq-version', '1']]), 15);
  assert.equal(http1Bytes([['etag', '"x"'], ['a', '']]), 11 + 5);
});

test('the legacy data breakdown splits markers, tags, content and the hash members', () => {
  const sha = 'a'.repeat(40);
  const body = Buffer.from(
    JSON.stringify({
      data: { '{title}': '<title>Shop</title>', '{price}': '<!--sonicdiff-price--><p class="price">120.00</p><!--sonicdiff-price-end-->' },
      diff: '',
      'html-sha1': sha,
      'template-tag': sha,
    }),
  );
  const r = legacyDataRaw(body);
  assert.equal(r.exact, true);
  assert.equal(r.entries, 2);
  assert.equal(r.rows.placeholder_names, 18); // "{title}" "{price}"
  assert.equal(r.rows.marker_comments, 48); // <!--sonicdiff-price--> 22 + <!--sonicdiff-price-end--> 26
  assert.equal(r.rows.content_bytes, 10); // Shop, 120.00
  assert.equal(r.rows.element_tags, 15 + 21); // <title></title> 7 + 8; <p class="price"></p> 17 + 4
  assert.equal(r.rows.escapes_and_quotes, 4 + 2); // two pairs of quotes; two \" escapes
  assert.equal(r.rows.html_sha1, 42);
  assert.equal(r.rows.template_tag, 42);
  assert.equal(r.rows.diff, 2);
  const sum = Object.values(r.rows).reduce((a, b) => a + b, 0);
  assert.equal(sum, body.length);
  assert.equal(r.total, body.length);
});
