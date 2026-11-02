// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors
//
// The inputs that the steps of negotiation section 4 leave open (DL-089, DL-092, DL-093), one test per
// behavior. Each choice gives the normal page, or the application response without its Rq-* fields, and
// never an exception.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { MAX_ORIGIN_CHARS, MAX_PATH_CHARS, hasManifestUrl, handle } from '../src/rqp.js';
import { acceptsDelta, deltaBase, hasContentCoding, noneMatchFails, parseIfNoneMatch, rqHave, rqNonce } from '../src/fields.js';
import { parseItem } from '../src/sf.js';
import { APP, NONCE, clientLines, get, header, page, revisionsOf } from './helpers.js';

const held = page('120.00');
const current = page('130.00');
const R1 = revisionsOf(held).page_revision;
const APP_WITH_RQ = { status: 200, headers: [...APP.headers, ['rq-version', '9'], ['x-app', '1']] };
const run = (request, app = APP, p = current) => handle(request, structuredClone(app), p, {});
const DELTA = 'application/vnd.rivqen.delta+json';

test('open inputs 1 and 13: If-Match, If-Unmodified-Since and If-Range give A without Rq-* fields, also with an empty value', () => {
  for (const name of ['if-match', 'if-unmodified-since', 'if-range']) {
    for (const value of ['"x"', '*', '']) {
      for (const lines of [[], clientLines(held)]) {
        const r = run(get([...lines, [name, value]]), APP_WITH_RQ);
        assert.deepEqual([r.mode, r.reason, r.status], ['passthrough', 'out-of-scope', 200], `${name}: ${value}`);
        assert.deepEqual(r.headers, [...APP.headers, ['x-app', '1']]);
        assert.deepEqual(r.body, current);
      }
    }
  }
});

test('open input 2: If-Modified-Since without If-None-Match is ignored', () => {
  const r = run(get([['rq-version', '1'], ['if-modified-since', 'Sun, 18 Oct 2026 10:00:00 GMT']]));
  assert.deepEqual([r.mode, r.status, r.reason], ['full', 200, 'not-eligible']);
});

test('open inputs 5 and 17: a content coding of A other than identity gives A without Rq-* fields', () => {
  for (const coding of ['gzip', 'br', 'identity, gzip', 'GZIP', 'x-identity']) {
    const r = run(get(clientLines(held)), { status: 200, headers: [...APP_WITH_RQ.headers, ['content-encoding', coding]] });
    assert.deepEqual([r.mode, r.reason], ['passthrough', 'out-of-scope'], coding);
    assert.deepEqual(r.headers, [...APP.headers, ['x-app', '1'], ['content-encoding', coding]]);
  }
  const twoLines = { status: 200, headers: [...APP.headers, ['content-encoding', 'identity'], ['content-encoding', 'gzip']] };
  assert.equal(run(get(), twoLines).mode, 'passthrough');
});

test('open input 17: identity in any case, empty list members and an empty value are no content coding', () => {
  for (const coding of ['identity', 'Identity', 'IDENTITY', ' identity ', 'identity, ', ', identity', '', ',']) {
    const r = run(get(), { status: 200, headers: [...APP.headers, ['content-encoding', coding]] });
    assert.equal(r.mode, 'full', JSON.stringify(coding));
    assert.equal(hasContentCoding([['content-encoding', coding]]), false, JSON.stringify(coding));
  }
  assert.equal(hasContentCoding([]), false);
});

test('open inputs 6 and 14: a path over 4096 characters or an origin over 512 characters gives A; the limits themselves give F', () => {
  const at = (target) => run({ ...get(), target }, APP_WITH_RQ);
  assert.equal(at(`https://shop.example.com/${'a'.repeat(4095)}`).mode, 'full');
  const over = at(`https://shop.example.com/${'a'.repeat(4096)}`);
  assert.equal(over.mode, 'passthrough');
  assert.deepEqual(over.headers, [...APP.headers, ['x-app', '1']]);
  assert.equal(at(`https://${'h'.repeat(512 - 'https://'.length)}/x`).mode, 'full');
  assert.equal(at(`https://${'h'.repeat(513 - 'https://'.length)}/x`).mode, 'passthrough');
  // The path is the serialized path: a non-ASCII character is 6 characters (two bytes, %XX each).
  assert.equal(at(`https://shop.example.com/aaa${'é'.repeat(682)}`).mode, 'full'); // 4096 characters
  assert.equal(at(`https://shop.example.com/aaaa${'é'.repeat(682)}`).mode, 'passthrough'); // 4097 characters
  assert.deepEqual([MAX_PATH_CHARS, MAX_ORIGIN_CHARS], [4096, 512]);
});

test('open input 14: a target that is not an http or https URL gives A and never throws', () => {
  for (const target of ['not a url', '', undefined, null, 'ftp://shop.example.com/', 'file:///x', 'https://', 'https://[::1', 'http://a b/']) {
    const r = run({ ...get(), target });
    assert.equal(r.mode, 'passthrough', String(target));
    assert.equal(r.error, undefined, String(target));
    assert.equal(hasManifestUrl(target), false, String(target));
  }
  assert.equal(hasManifestUrl('http://localhost:8080/x?q=1#f'), true);
});

test('open input 10: an empty list element gives the tag for the 304 test and no delta base', () => {
  for (const v of [`"${R1}", `, `, "${R1}"`, `"${R1}",`, `,"${R1}"`]) {
    assert.equal(deltaBase([['if-none-match', v]]), null, v);
    assert.equal(parseIfNoneMatch(v).emptyElement, true, v);
    assert.equal(noneMatchFails([['if-none-match', v]], `"${R1}"`), true, v);
  }
  assert.equal(parseIfNoneMatch(`"${R1}", "x"`).emptyElement, false);
  assert.equal(parseIfNoneMatch(`  "${R1}"  `).emptyElement, false);
  assert.equal(deltaBase([['if-none-match', `"${R1}"`]]), R1);
  const r = run(get(clientLines(held, { 'if-none-match': `"${R1}",` })));
  assert.deepEqual([r.mode, r.reason], ['full', 'not-eligible']);
  assert.equal(run(get(clientLines(held, { 'if-none-match': `"${revisionsOf(current).page_revision}",` }))).status, 304);
});

test('open inputs 3 and 10: an If-None-Match of another syntax gives no 304 and no base', () => {
  const now = `"${revisionsOf(current).page_revision}"`;
  for (const v of ['', ',', ', ,', `*, ${now}`, `${now}, *`, `w/${now}`, `W/ ${now}`, now.slice(1, -1), now.slice(0, -1), `${now} "x"`]) {
    assert.equal(noneMatchFails([['if-none-match', v]], now), false, JSON.stringify(v));
    assert.equal(deltaBase([['if-none-match', v]]), null, JSON.stringify(v));
    assert.equal(run(get(clientLines(held, { 'if-none-match': v }))).mode, 'full', JSON.stringify(v));
  }
  assert.equal(noneMatchFails([['if-none-match', '*'], ['if-none-match', '*']], '"r1.X"'), false);
});

test('open input 9: one accepting range is enough; two q parameters, a quoted q and a syntax error accept nothing', () => {
  const mode = (accept) => run(get(clientLines(held, { accept }))).mode;
  assert.equal(mode(`${DELTA};q=0, ${DELTA}`), 'delta');
  assert.equal(mode(`text/html, ${DELTA};q=0, ${DELTA};q=0.1`), 'delta');
  assert.equal(mode(`${DELTA};Q=1`), 'delta');
  for (const accept of [`${DELTA};q=1;q=1`, `${DELTA};q="1"`, `${DELTA};q=0.5;Q=1`, `${DELTA};q`, `${DELTA};q = 1`, `${DELTA};a="b`, `${DELTA};q=1.5`, `${DELTA};q=`]) {
    assert.equal(mode(accept), 'full', accept);
  }
});

test('open input 7: a Byte Sequence with an incomplete padding fails the parse; an absent or complete padding does not', () => {
  for (const f of [':AA=:', ':AAA==:', ':AQ=:', ':AAAA=:', ':=:', ':==:', ':AQID=:']) assert.equal(parseItem(f), null, f);
  for (const f of [':AA==:', ':AAA=:', ':AQI:', ':AQ:', ':AAAA:', '::']) assert.notEqual(parseItem(f), null, f);
  assert.deepEqual(parseItem(':AB==:').value, Buffer.from([0])); // non-zero pad bits are accepted
});

test('open inputs 7 and 12: Rq-Have and Rq-Nonce follow the RFC 9651 parse', () => {
  const t = `t1.${'A'.repeat(43)}`;
  const have = (v) => rqHave([['rq-have', v]]);
  assert.equal(have(`t=${t}, b=:AA=:`), null); // an incomplete padding fails the Dictionary parse: Rq-Have is absent
  assert.deepEqual(have(`t=${t}, b`), { t, b: null }); // a bare key b is an absent b
  assert.equal(have(`t, b=:AA==:`), null); // a bare key t makes Rq-Have absent
  assert.equal(rqNonce([['rq-nonce', ':AAECAwQFBgcICQoLDA0ODw=:']]), null);
  assert.deepEqual(rqNonce([['rq-nonce', `:${NONCE.toString('base64')}:`]]), NONCE);
  assert.deepEqual(rqNonce([['rq-nonce', `:${NONCE.toString('base64').replace(/=+$/, '')}:`]]), NONCE);
});

test('open input 15: the Vary members of A stay on F and on 304, also Accept; a line with an Rq-* member is written again', () => {
  const app = { status: 200, headers: [...APP.headers, ['vary', 'Accept, Rq-Nonce, , Cookie'], ['vary', 'Accept-Encoding,  Accept']] };
  const expected = [['vary', 'Accept, Cookie'], ['vary', 'Accept-Encoding,  Accept']];
  const f = run(get(), app, current);
  const notModified = run(get([['if-none-match', `"${revisionsOf(current).page_revision}"`]]), app, current);
  assert.equal(notModified.status, 304);
  for (const r of [f, notModified]) assert.deepEqual(r.headers.filter(([n]) => n === 'vary'), expected);
  assert.equal(header(f, 'vary'), 'Accept, Cookie, Accept-Encoding,  Accept');
});

test('open inputs 4 and 9: the parameter grammar of a media range (RFC 9110 sections 5.6.6 and 12.4.2)', () => {
  const accept = (v) => acceptsDelta([['accept', v]]);
  for (const ok of [`${DELTA}; q=0.5`, `${DELTA};\tq=0.5`, `${DELTA};  level=1 \t; q=1`, `${DELTA};q=0.123`, `${DELTA};q=1.000`, `${DELTA};q=1.00`, `${DELTA};q=1`, `${DELTA};a="x\\"y"`, `${DELTA};a=b!#$%&'*+-.^_\`|~`]) {
    assert.equal(accept(ok), true, ok);
  }
  const no = [
    `${DELTA};level=`, // an empty value
    `${DELTA};level=1 x`, // text after a value
    `${DELTA};level=1\tx`,
    `${DELTA};a b=1`, // a name that is not a token
    `${DELTA};a@=1`,
    `${DELTA};level=a@b`, // a value that is not a token
    `${DELTA};=1`,
    `${DELTA};level;q=1`, // a name without "="
    `${DELTA};a="x\\`, // an escape at the end
    `${DELTA};q=0.1234`,
    `${DELTA};q=1.1`,
    `${DELTA};q=00.5`,
  ];
  for (const v of no) assert.equal(accept(v), false, v);
});
