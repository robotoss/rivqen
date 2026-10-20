// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors
//
// Reference server step: one test per branch of negotiation.md section 4, and
// the field rules of section 3.

import assert from 'node:assert/strict';
import { test } from 'node:test';
import { UnsupportedInput, deltaBinding, nonceText } from '../src/common.mjs';
import { capabilities, changeSet, deltaBase, handle, notModified, requestVersion, rqHave, rqNonce, withoutRqVary, writtenMembers } from '../src/server.mjs';
import { acceptsMediaType, parseEntityTags } from '../src/http.mjs';
import { analyze as analyzeBytes, documentHeadStart } from '../src/markup.mjs';
import { APP, NONCE, NONCE_SF, TARGET, blockPage, deltaRequest, firstVisit, page, revisions } from './helpers.mjs';
import { havePrefixes } from '../src/client.mjs';

const header = (r, name) => r.headers.filter(([n]) => n === name).map(([, v]) => v).join(', ') || null;
const lines = (h) => h.map(([n, v]) => [n, v]);

// Step 1.
test('step 1: a POST, a status other than 200 and a non-HTML type get A unchanged', () => {
  for (const [request, app] of [
    [deltaRequest('page-a', {}, 'POST'), APP],
    [deltaRequest('page-a'), { ...APP, status: 404 }],
    [deltaRequest('page-a'), { status: 200, headers: [['content-type', 'application/json']] }],
    [deltaRequest('page-a'), { status: 200, headers: [] }],
  ]) {
    const r = handle(request, app, page('page-a'));
    assert.equal(r.reason, 'out-of-scope');
    assert.equal(r.status, app.status);
    assert.deepEqual(r.body, page('page-a'));
    assert.deepEqual(r.headers, lines(app.headers));
  }
});

test('step 1: a 2xx status other than 200 gets A unchanged', () => {
  for (const status of [201, 203, 206]) {
    assert.equal(handle(deltaRequest('page-big-a'), { ...APP, status }, page('page-big-b')).reason, 'out-of-scope', String(status));
  }
});

test('step 1: the media type match ignores parameters and ASCII case', () => {
  const app = { status: 200, headers: [['content-type', 'Text/HTML ; charset=utf-8']] };
  assert.equal(handle(firstVisit(), app, page('page-a')).kind, 'full');
});

test('step 1: HEAD is in scope', () => {
  assert.equal(handle(firstVisit('HEAD'), APP, page('page-a')).reason, 'not-eligible');
});

// Step 2.
test('step 2: an invalid page gets A unchanged, with the ETag of the application', () => {
  const app = { status: 200, headers: [...APP.headers, ['etag', '"app-1"']] };
  const r = handle(deltaRequest('page-a'), app, page('page-c'));
  assert.equal(r.reason, 'invalid-page');
  assert.equal(header(r, 'etag'), '"app-1"');
  assert.equal(header(r, 'rq-version'), null);
});

test('step 2: a page without a head start tag and without an implied head gets A unchanged (H-72, H-79a)', () => {
  const noHead = Buffer.from('<!DOCTYPE html><div>x</div><!-- <head> --></head><p data-rq-block="a">1</p>', 'utf8');
  assert.equal(analyzeBytes(noHead).valid, true);
  const r = handle(firstVisit(), APP, noHead);
  assert.equal(r.reason, 'no-head-start');
  assert.deepEqual(r.body, noHead);
});

test('step 3: a page whose first head content comes before any head start tag gets F with the manifest before it (H-79a)', () => {
  const doc = Buffer.from('<!DOCTYPE html>\n<meta charset="utf-8"><title>x</title><p data-rq-block="a">1</p>', 'utf8');
  const r = handle(firstVisit(), APP, doc);
  assert.equal(r.kind, 'full');
  const at = doc.indexOf('<meta');
  assert.equal(r.body.indexOf('<script type="application/rivqen-manifest+json">'), at);
  assert.ok(r.body.subarray(0, at).equals(doc.subarray(0, at)));
  assert.ok(r.body.subarray(r.body.indexOf('</script>') + '</script>'.length).equals(doc.subarray(at)));
});

test('step 2: the manifest goes immediately after the head start tag, not after "<head>" text in a comment', () => {
  const doc = Buffer.from('<!DOCTYPE html><!-- <head> --><html><head><script>var s = "</head>";</script></head><body></body></html>', 'utf8');
  const r = handle(firstVisit(), APP, doc);
  assert.equal(r.kind, 'full');
  const at = doc.indexOf('<html><head>') + '<html><head>'.length;
  assert.equal(r.body.indexOf('<script type="application/rivqen-manifest+json">'), at);
  assert.ok(r.body.subarray(0, at).equals(doc.subarray(0, at)));
});

// H-72: the head start tag of the document (manifest-patch section 1.7, "Document head").
// Body content first: a title or meta here would imply a head (H-79a) for the prefixes without a head start tag.
const HEAD_BODY = '<div>t</div></head><body><p data-rq-block="a">1</p></body></html>';
const headStart = (prefix) => documentHeadStart(Buffer.from(prefix + HEAD_BODY, 'utf8'));
const afterHead = (prefix) => Buffer.byteLength(prefix, 'utf8');

test('H-72: the head start tag after white space, comments, DOCTYPE tokens, html start tags and a BOM', () => {
  const ok = [
    '<head>',
    '<!DOCTYPE html><html><head>',
    '﻿ \t\n\r\f<!-- c --><!DOCTYPE html><!doctype x>\n<html lang="en"><html><head>',
    '<HTML><HEAD/>', // upper case and the self-closing flag: still the head start tag
    '<html><head class="a>b">', // a ">" in a quoted attribute value is not the end of the tag
    '<html><head\n>',
    '<?pi x?><!x><html><head>', // bogus comments
  ];
  for (const p of ok) assert.equal(headStart(p), afterHead(p), JSON.stringify(p));
});

test('H-72: only the first head start tag counts, whatever comes after it', () => {
  // The head needs no end tag: body content, a stray </head> later, a second head start tag.
  for (const after of ['<body>', '<div>x</div>', 'text', '<svg></svg>', '<head>', '</br>', '<noscript><img src=x></noscript>']) {
    const doc = `<html><head>${after}</head><body></body></html>`;
    assert.equal(documentHeadStart(Buffer.from(doc)), '<html><head>'.length, after);
  }
});

test('H-72: no head start tag when something else comes before it', () => {
  const none = [
    '', // no head start tag at all
    '<html>', // the end of input before a head start tag
    '<html>x<head>', // text before the head
    '<html>&amp;<head>', // a character reference is text
    '<p>x</p><head>', // another start tag before the head
    '<body><head>',
    '<html></><head>', // "</>" gives no token: its bytes are not white space
    '<svg><head>',
    '<template><head>', // H-79a: template is not in the list
    '<noscript></noscript><head>', // H-79a: noscript is not in the list
    '<html>x<meta>', // text before head content
  ];
  for (const p of none) assert.equal(headStart(p), -1, JSON.stringify(p));
  assert.equal(documentHeadStart(Buffer.alloc(0)), -1);
});

test('H-79a: head content before any head start tag implies the head: the insertion point is its "<"', () => {
  for (const name of ['base', 'basefont', 'bgsound', 'link', 'meta', 'noframes', 'script', 'style', 'title', 'META', 'Title']) {
    for (const prefix of ['', '<!DOCTYPE html>\n<html lang="en">\n', '﻿<!-- c --></p> ']) {
      const doc = `${prefix}<${name} x="<head>">t</${name}><head>`;
      assert.equal(documentHeadStart(Buffer.from(doc, 'utf8')), Buffer.byteLength(prefix, 'utf8'), JSON.stringify(doc));
    }
  }
  // The first stop wins: a later head start tag does not move the point.
  assert.equal(documentHeadStart(Buffer.from('<html><meta charset="utf-8"><head lang="en">')), '<html>'.length);
  // Self-closing flag and attributes do not change it.
  assert.equal(documentHeadStart(Buffer.from('<link/><head>')), 0);
  // Another name, also one that starts like a list name, is not a stop.
  for (const doc of ['<metadata><head>', '<tit><head>', '<scripts>']) assert.equal(documentHeadStart(Buffer.from(doc)), -1, doc);
  // An end tag of a list name is an ignored end tag (DL-034), not a stop.
  assert.equal(documentHeadStart(Buffer.from('<html></title><head>')), '<html></title><head>'.length);
});

test('H-72 (DL-034 item 1): an end tag other than head, body, html and br is ignored before the head', () => {
  for (const p of ['</p><html></div><head>', '<html></meta> <!-- c --> </title><head>', '</P><head>']) {
    assert.equal(headStart(p), afterHead(p), p);
  }
  for (const p of ['</br><head>', '</BR><head>', '</body><head>', '</html><head>', '</head><head>', '<html></html><head>', 'x</p><head>']) {
    assert.equal(headStart(p), -1, p);
  }
});

test('step 2: a page whose head ends without </head> gets F with the manifest after the head start tag (H-72)', () => {
  for (const html of [
    '<!DOCTYPE html><html><head><title>t</title><body><p data-rq-block="a">1</p></head></body></html>', // closed by body; stray </head>
    '<!DOCTYPE html><html><head><svg><desc>d</desc></svg><p data-rq-block="a">1</p></body></html>', // svg closes the head
  ]) {
    const doc = Buffer.from(html, 'utf8');
    const r = handle(firstVisit(), APP, doc);
    assert.equal(r.kind, 'full', html);
    assert.equal(r.body.indexOf('<script type="application/rivqen-manifest+json">'), doc.indexOf('<head>') + 6, html);
  }
});

test('step 2: text before the head start tag means no RQP (H-72)', () => {
  const doc = Buffer.from('<!DOCTYPE html>x<html><head><title>t</title></head><body><p data-rq-block="a">1</p></body></html>', 'utf8');
  assert.equal(analyzeBytes(doc).valid, true);
  assert.equal(handle(firstVisit(), APP, doc).reason, 'no-head-start');
});

// Step 3.
test('step 3: F keeps the application fields, replaces its ETag and sets Rq-Version and Content-Length', () => {
  const app = { status: 200, headers: [...APP.headers, ['etag', '"app-1"'], ['content-length', '1']] };
  const r = handle(firstVisit(), app, page('page-a'));
  assert.equal(header(r, 'etag'), `"${revisions('page-a').page_revision}"`);
  assert.equal(header(r, 'rq-version'), '1');
  assert.equal(header(r, 'cache-control'), 'no-cache');
  assert.equal(header(r, 'content-length'), String(r.body.length));
  assert.equal(r.headers.filter(([n]) => n === 'etag').length, 1);
});

test('step 3: F does not depend on RQP request fields (H-51)', () => {
  const a = handle(firstVisit(), APP, page('page-a'));
  const b = handle({ method: 'GET', target: TARGET, headers: [] }, APP, page('page-a'));
  assert.deepEqual(b.body, a.body);
  assert.deepEqual(b.headers, a.headers);
});

test('step 3: the manifest has no block list and no template revision (H-103), also for a page without blocks', () => {
  for (const id of ['page-a', 'page-zero']) {
    const r = handle(firstVisit(), APP, page(id));
    const text = r.body.toString('utf8');
    const m = JSON.parse(text.slice(text.indexOf('{'), text.indexOf('</script>')));
    assert.deepEqual(m, { protocol: 'rqp/1', origin: 'https://shop.example.com', path: '/catalog', page_revision: revisions(id).page_revision }, id);
  }
});

test('writtenMembers: no default member and no empty object (MUST), no hint that is not smaller than the server limit (SHOULD); H-103', () => {
  assert.deepEqual(writtenMembers({}), {});
  assert.deepEqual(writtenMembers({ policy: { mode: 'private', ttl_seconds: 0, offline: false }, limits: {} }), {});
  assert.deepEqual(writtenMembers({ policy: { mode: 'public', ttl_seconds: 0, offline: true } }), { policy: { mode: 'public', offline: true } });
  assert.deepEqual(writtenMembers({ policy: { mode: 'private', ttl_seconds: 1 } }), { policy: { ttl_seconds: 1 } });
  assert.deepEqual(writtenMembers({ limits: { max_patch_bytes: 262143 } }), { limits: { max_patch_bytes: 262143 } });
  assert.deepEqual(writtenMembers({ limits: { max_patch_bytes: 262144 } }), {});
  assert.deepEqual(writtenMembers({ limits: { max_patch_bytes: 524288 } }, 1048576), { limits: { max_patch_bytes: 524288 } });
  assert.deepEqual(writtenMembers({ limits: { max_patch_bytes: 1048576 } }, 1048576), {});
  // A value that is not the default keeps its member, whatever its type (the schema check is not the job of this function).
  assert.deepEqual(writtenMembers({ policy: { offline: 'false', ttl_seconds: '0' } }), { policy: { offline: 'false', ttl_seconds: '0' } });
  // The ETag of F: default members do not change the page revision of F.
  const plain = handle(firstVisit(), APP, page('page-a'));
  const defaults = handle(firstVisit(), APP, page('page-a'), { policy: { mode: 'private' }, limits: { max_patch_bytes: 262144 } });
  assert.deepEqual([header(defaults, 'etag'), defaults.body], [header(plain, 'etag'), plain.body]);
});

test('step 3: a manifest that would contain "<" makes the server pass A through', () => {
  const r = handle({ ...firstVisit(), target: 'https://shop.example.com/a<b' }, APP, page('page-a'));
  // The WHATWG URL parser percent-encodes "<" in a path, so the manifest has no "<".
  assert.equal(r.kind, 'full');
  assert.doesNotMatch(r.body.toString('utf8').split('</script>')[0].split('>').slice(-1)[0], /</);
});

// Step 4 and step 9.
test('step 4: a matching strong, weak or listed tag and "*" give 304 with ETag and Cache-Control only', () => {
  const r1 = `"${revisions('page-a').page_revision}"`;
  for (const inm of [r1, `W/${r1}`, `"r1.x", ${r1}`, '*']) {
    const r = handle(deltaRequest('page-a', { 'if-none-match': inm }), { status: 200, headers: [...APP.headers, ['x-app', '1']] }, page('page-a'));
    assert.equal(r.status, 304, inm);
    assert.equal(r.body, null);
    assert.deepEqual(r.headers.map(([n]) => n).sort(), ['cache-control', 'etag']);
  }
});

test('step 4: the 304 keeps the fields of RFC 9110 section 15.4.5 from F, in the order of F', () => {
  const app = {
    status: 200,
    headers: [
      ['date', 'Sun, 18 Oct 2026 10:00:00 GMT'],
      ...APP.headers,
      ['expires', 'Sun, 18 Oct 2026 11:00:00 GMT'],
      ['vary', 'cookie'],
      ['content-location', '/catalog'],
      ['x-app', '1'],
    ],
  };
  const r = handle(deltaRequest('page-a'), app, page('page-a'));
  assert.deepEqual([r.status, r.kind, r.reason], [304, 'none', 'not-modified']);
  assert.deepEqual(r.headers.map(([n]) => n), ['date', 'cache-control', 'expires', 'vary', 'content-location', 'etag']);
});

test('step 4: If-Modified-Since together with If-None-Match is modelled (If-None-Match wins)', () => {
  const r = handle(deltaRequest('page-a', { 'if-modified-since': 'Sun, 18 Oct 2026 10:00:00 GMT' }), APP, page('page-a'));
  assert.equal(r.status, 304);
});

test('step 4: HEAD with a matching tag gives 304', () => {
  assert.equal(handle(deltaRequest('page-a', {}, 'HEAD'), APP, page('page-a')).status, 304);
});

test('step 4: the ETag of the application is not a validator (RQP-16)', () => {
  const app = { status: 200, headers: [...APP.headers, ['etag', '"app-1"']] };
  assert.equal(handle(deltaRequest('page-a', { 'if-none-match': '"app-1"' }), app, page('page-a')).status, 200);
});

// Step 5.
test('step 5: each missing condition makes the request not delta-eligible', () => {
  const changes = [
    { 'rq-version': '2' },
    { 'rq-version': null },
    { 'rq-capabilities': '(realtime-ws)' },
    { 'rq-capabilities': null },
    { accept: 'text/html' },
    { 'if-none-match': null },
    { 'rq-have': null },
    { 'rq-have': `t=${revisions('page-t2').template_revision}` },
    { range: 'bytes=0-1' },
  ];
  for (const change of changes) {
    const r = handle(deltaRequest('page-big-a', change), APP, page('page-big-b'));
    assert.equal(r.reason, 'not-eligible', JSON.stringify(change));
    assert.equal(r.kind, 'full');
  }
  assert.equal(handle(deltaRequest('page-big-a', {}, 'HEAD'), APP, page('page-big-b')).kind, 'none');
});

// Step 6.
test('step 6: with b, only the blocks whose prefix differs; without b, all blocks', () => {
  const blocks = revisions('page-big-b').blocks;
  const prefixes = (id) => Buffer.concat(revisions(id).blocks.map((b) => Buffer.from(b.sha256, 'base64url').subarray(0, 8)));
  assert.deepEqual(changeSet(blocks, prefixes('page-big-a')), [1]);
  assert.deepEqual(changeSet(blocks, prefixes('page-big-b')), []);
  assert.deepEqual(changeSet(blocks, null), [0, 1, 2]);
});

test('step 6: b of the wrong length is absent and gives all blocks', () => {
  const t = revisions('page-big-a').template_revision;
  const r = handle(deltaRequest('page-big-a', { 'rq-have': `t=${t}, b=:${Buffer.alloc(16).toString('base64')}:` }), APP, page('page-big-b'));
  assert.deepEqual(JSON.parse(r.body.toString('utf8')).operations.map((o) => o.block_id), ['title', 'price', 'cart']);
});

test('step 6: an empty change set gives F', () => {
  const t = revisions('page-big-a').template_revision;
  const prefixes = Buffer.concat(revisions('page-big-b').blocks.map((b) => Buffer.from(b.sha256, 'base64url').subarray(0, 8)));
  const r = handle(deltaRequest('page-big-a', { 'rq-have': `t=${t}, b=:${prefixes.toString('base64')}:` }), APP, page('page-big-b'));
  assert.equal(r.reason, 'empty-change-set');
});

// Step 7.
test('step 7: the delta has the delta fields, no ETag, Last-Modified or Expires, and the other application fields', () => {
  const app = { status: 200, headers: [...APP.headers, ['etag', '"a"'], ['last-modified', 'x'], ['expires', 'y'], ['x-app', '1']] };
  const r = handle(deltaRequest('page-big-a'), app, page('page-big-b'));
  assert.equal(r.kind, 'delta');
  assert.deepEqual(r.headers.map(([n]) => n), ['x-app', 'content-type', 'x-content-type-options', 'rq-version', 'cache-control', 'vary', 'content-length']);
  assert.equal(header(r, 'cache-control'), 'no-store');
  // DL-063 (L1): nosniff (MUST) and Vary: Rq-Nonce (SHOULD).
  assert.equal(header(r, 'x-content-type-options'), 'nosniff');
  assert.equal(header(r, 'vary'), 'Rq-Nonce');
  assert.equal(header(r, 'content-length'), String(r.body.length));
  const env = JSON.parse(r.body.toString('utf8'));
  // H-104, H-107: exactly the binding and the operations; each operation exactly block_id and content.
  assert.deepEqual(Object.keys(env), ['binding', 'operations']);
  assert.equal(env.binding, deltaBinding(TARGET, revisions('page-big-a').page_revision, NONCE, revisions('page-big-b').page_revision));
  const changed = revisions('page-big-b').blocks[1];
  assert.deepEqual(env.operations, [{ block_id: changed.id, content: page('page-big-b').toString('utf8', changed.start, changed.end) }]);
});

test('step 7, H-107: the binding has U of the request target, B of If-None-Match (also weak) and R\' of the current page', () => {
  const binding = (request) => JSON.parse(handle(request, APP, page('page-big-b')).body.toString('utf8')).binding;
  const a = revisions('page-big-a').page_revision;
  const b = revisions('page-big-b').page_revision;
  // A weak base gives the same B: the r1. value between the quotes.
  assert.equal(binding(deltaRequest('page-big-a', { 'if-none-match': `W/"${a}"` })), deltaBinding(TARGET, a, NONCE, b));
  // U is the WHATWG serialization of the target with its query and without the fragment.
  const target = 'https://SHOP.example.com:443/catalog?page=2#top';
  assert.equal(binding({ ...deltaRequest('page-big-a'), target }), deltaBinding('https://shop.example.com/catalog?page=2', a, NONCE, b));
  assert.notEqual(binding({ ...deltaRequest('page-big-a'), target }), binding(deltaRequest('page-big-a')));
  // Another base (a client with another snapshot that sends the same prefixes) gives another binding.
  const other = revisions('page-a').page_revision;
  assert.equal(binding(deltaRequest('page-big-a', { 'if-none-match': `"${other}"` })), deltaBinding(TARGET, other, NONCE, b));
  // H-108, H-109: N is the nonce of this request (Rq-Nonce), as the base64url text of the decoded Byte Sequence.
  const bytes = Buffer.from([...Array(16).keys()].map((i) => 255 - i));
  const sf = bytes.toString('base64');
  assert.ok(sf.includes('/') || sf.includes('+'));
  const withNonce = binding(deltaRequest('page-big-a', { 'rq-nonce': `:${sf}:` }));
  assert.equal(withNonce, deltaBinding(TARGET, a, nonceText(bytes), b));
  assert.notEqual(withNonce, binding(deltaRequest('page-big-a')));
  // Without padding, with non-zero pad bits (RFC 9651 section 4.2.7) and with a parameter, the bytes and so N are the same.
  assert.equal(binding(deltaRequest('page-big-a', { 'rq-nonce': `:${sf.replace(/=+$/, '')}:` })), withNonce);
  const padBits = sf.replace(/A==$/, 'B==');
  assert.notEqual(padBits, sf);
  assert.equal(binding(deltaRequest('page-big-a', { 'rq-nonce': `:${padBits}:` })), withNonce);
  assert.equal(binding(deltaRequest('page-big-a', { 'rq-nonce': `:${sf}:;v=2` })), withNonce);
  // H-109: a member n of Rq-Have is ignored, also when it names another nonce: N comes only from Rq-Nonce.
  const have = `t=${revisions('page-big-a').template_revision}, b=:${havePrefixes(revisions('page-big-a').blocks)}:`;
  assert.equal(binding(deltaRequest('page-big-a', { 'rq-have': `${have}, n=:${sf}:` })), binding(deltaRequest('page-big-a')));
  assert.equal(handle(deltaRequest('page-big-a', { 'rq-have': `${have}, n=:${sf}:`, 'rq-nonce': null }), APP, page('page-big-b')).reason, 'no-nonce');
});

test('step 7: a delta larger than maxPatchBytes gives F; at the limit it is sent', () => {
  const r = handle(deltaRequest('page-big-a'), APP, page('page-big-b'));
  const size = r.body.length;
  assert.equal(handle(deltaRequest('page-big-a'), APP, page('page-big-b'), { maxPatchBytes: size }).kind, 'delta');
  assert.equal(handle(deltaRequest('page-big-a'), APP, page('page-big-b'), { maxPatchBytes: size - 1 }).reason, 'delta-too-large');
});

test('step 7: the limits hint max_patch_bytes of the server lowers the limit', () => {
  const r = handle(deltaRequest('page-huge-a'), APP, page('page-huge-b'), { maxPatchBytes: 5242880 });
  assert.equal(r.kind, 'delta');
  const hinted = { maxPatchBytes: 5242880, limits: { max_patch_bytes: 307200 } };
  assert.equal(handle(deltaRequest('page-huge-a'), APP, page('page-huge-b'), hinted).reason, 'delta-too-large');
});

test('step 7: a delta that is not smaller than F gives F', () => {
  assert.equal(handle(deltaRequest('page-esc-a'), APP, page('page-esc-b')).reason, 'delta-not-smaller');
});

test('step 7: a delta of exactly the size of F gives F; one byte smaller is sent', () => {
  // Quotes cost 2 bytes in D and 1 byte in F. Pad the template so that the sizes meet.
  const doc = (pad, content) => Buffer.from(`<!DOCTYPE html><html><head><title>T</title></head><body>${' '.repeat(pad)}<p data-rq-block="q">${content}</p></body></html>`, 'utf8');
  const quotes = '"'.repeat(600);
  const send = (pad) => {
    const before = analyzeBytes(doc(pad, 'old'));
    const request = {
      method: 'GET',
      target: TARGET,
      headers: [
        ['rq-version', '1'],
        ['rq-capabilities', '(block-patch)'],
        ['accept', 'application/vnd.rivqen.delta+json'],
        ['if-none-match', `"${before.page_revision}"`],
        ['rq-have', `t=${before.template_revision}`],
        ['rq-nonce', NONCE_SF],
      ],
    };
    return handle(request, APP, doc(pad, quotes));
  };
  assert.equal(send(0).reason, 'delta-not-smaller');
  // The size of D does not depend on the template; the size of F grows by one byte per pad byte.
  const deltaSize = send(10000).body.length;
  const equalPad = deltaSize - handle(firstVisit(), APP, doc(0, quotes)).body.length;
  assert.ok(equalPad > 0 && equalPad < 10000);
  assert.equal(send(equalPad).reason, 'delta-not-smaller');
  assert.equal(send(equalPad + 1).kind, 'delta');
});

test('step 7: a changed html block that fails the attribute rule gives F (H-56)', () => {
  assert.equal(handle(deltaRequest('page-a'), APP, page('page-b-onclick')).reason, 'patch-content-rule');
});

test('step 7: a changed html block with an element of PATCH_FORBIDDEN or a foreign element outside FOREIGN_ALLOWED gives F (H-78)', () => {
  const withPrice = (content) => Buffer.from(page('page-a').toString('utf8').replace('>120.00<', `>${content}<`), 'utf8');
  for (const content of ['<script>x()</script>', '<style>p{}</style>', '<iframe src="/v"></iframe>', '<link rel="x">', '<svg><a href="#x"><text>t</text></a></svg>']) {
    const doc = withPrice(content);
    assert.equal(analyzeBytes(doc).valid, true, content);
    assert.deepEqual([handle(deltaRequest('page-a'), APP, doc).kind, handle(deltaRequest('page-a'), APP, doc).reason], ['full', 'patch-content-rule'], content);
  }
  // Control: the same block with allowed content gets a delta.
  assert.equal(handle(deltaRequest('page-a'), APP, withPrice('<b>130.00</b>')).kind, 'delta');
});

test('step 7: the patch content rule reads only html blocks; a json block with markup text gets a delta', () => {
  const next = Buffer.from(page('page-a').toString('utf8').replace('{"items":3}', '{"t":"<b onclick=x>"}'), 'utf8');
  assert.equal(analyzeBytes(next).valid, true);
  const r = handle(deltaRequest('page-a'), APP, next);
  assert.deepEqual([r.kind, r.reason], ['delta', 'delta']);
  assert.deepEqual(JSON.parse(r.body.toString('utf8')).operations.map((op) => op.block_id), ['cart']);
});

test('step 7: the delta has exactly one Content-Length, the length of D', () => {
  const app = { status: 200, headers: [...APP.headers, ['content-length', '99999']] };
  const r = handle(deltaRequest('page-big-a'), app, page('page-big-b'));
  assert.equal(r.kind, 'delta');
  assert.deepEqual(r.headers.filter(([n]) => n === 'content-length'), [['content-length', String(r.body.length)]]);
});

test('step 3: a server configuration whose manifest text would contain "<" passes A through', () => {
  const r = handle(firstVisit(), APP, page('page-a'), { policy: { mode: 'private', note: '</script>' } });
  assert.deepEqual([r.kind, r.reason, r.status], ['app', 'server-passthrough', 200]);
  assert.deepEqual(r.body, page('page-a'));
});

test('the reference server does not model content codings and other preconditions', () => {
  assert.throws(() => handle(firstVisit(), { status: 200, headers: [...APP.headers, ['content-encoding', 'br']] }, page('page-a')), UnsupportedInput);
  for (const name of ['if-match', 'if-unmodified-since', 'if-range']) {
    assert.throws(() => handle({ ...firstVisit(), headers: [[name, '*']] }, APP, page('page-a')), UnsupportedInput, name);
  }
  assert.throws(() => handle({ ...firstVisit(), headers: [['if-modified-since', 'x']] }, APP, page('page-a')), UnsupportedInput);
});

// Section 3: field rules.
test('Rq-Version: only the Integer 1, parameters ignored', () => {
  const v = (value) => requestVersion([['rq-version', value]]);
  assert.equal(v('1'), 1);
  assert.equal(v('1;x=2'), 1);
  assert.equal(v(' 1 '), 1);
  assert.equal(v('"1"'), null);
  assert.equal(v('1.0'), null);
  assert.equal(v('abc'), null);
  assert.equal(v('1, 1'), null);
  assert.equal(requestVersion([]), null);
});

test('Rq-Capabilities: one Inner List of Tokens; any other shape is empty (H-53)', () => {
  const c = (...values) => [...capabilities(values.map((v) => ['rq-capabilities', v]))].sort();
  assert.deepEqual(c('(block-patch)'), ['block-patch']);
  assert.deepEqual(c('(block-patch realtime-ws);v=2'), ['block-patch', 'realtime-ws']);
  assert.deepEqual(c('()'), []);
  assert.deepEqual(c('block-patch'), []);
  assert.deepEqual(c('(block-patch "x")'), []);
  assert.deepEqual(c('(block-patch), (realtime-ws)'), []);
  assert.deepEqual(c('(block-patch)', '(realtime-ws)'), []);
  assert.deepEqual(c(`(block-patch x-${'a'.repeat(1008)})`).length, 2);
  assert.deepEqual(c(`(block-patch x-${'a'.repeat(1009)})`), []);
});

test('Rq-Have: t must be a Token; b a Byte Sequence; the last key wins; 4096 bytes at most', () => {
  const h = (v) => rqHave([['rq-have', v]]);
  assert.deepEqual(h('t=t1.x, b=:AAAAAAAAAAA=:'), { t: 't1.x', b: Buffer.alloc(8) });
  assert.equal(h('t="t1.x"'), null);
  assert.equal(h('b=:AAAA:'), null);
  assert.equal(h('t='), null);
  assert.equal(h('t=t1.x, b=?1').b, null);
  assert.equal(h('t=t1.x, b="x"').b, null);
  assert.equal(h('t=t1.y, t=t1.x').t, 't1.x');
  // 't=t1.x, y="' and '"' are 12 bytes.
  assert.equal(h(`t=t1.x, y="${'a'.repeat(4096 - 12)}"`).t, 't1.x');
  assert.equal(h(`t=t1.x, y="${'a'.repeat(4097 - 12)}"`), null);
  assert.deepEqual(rqHave([['rq-have', 't=t1.x'], ['rq-have', 'b=::']]), { t: 't1.x', b: Buffer.alloc(0) });
  // H-109: n is an unknown member (step 7): ignored, also repeated, invalid or as the only other member.
  const n16 = `:${Buffer.alloc(16).toString('base64')}:`;
  for (const v of [`t=t1.x, b=:AAAAAAAAAAA=:, n=${n16}`, `t=t1.x, b=:AAAAAAAAAAA=:, n=${n16}, n=${n16}`, 't=t1.x, b=:AAAAAAAAAAA=:, n="x"', 'n=?1, t=t1.x, b=:AAAAAAAAAAA=:']) {
    assert.deepEqual(h(v), { t: 't1.x', b: Buffer.alloc(8) }, v);
  }
});

test('Rq-Nonce (H-109): one field line, an Item whose bare item is a Byte Sequence of exactly 16 bytes, else no nonce; parameters are ignored', () => {
  const sixteen = Buffer.from([...Array(16).keys()]);
  const sf = `:${sixteen.toString('base64')}:`;
  const n = (...lines) => rqNonce(lines.map((v) => ['rq-nonce', v]));
  assert.deepEqual(n(sf), sixteen);
  // RFC 9651 section 4.2.7: no padding and non-zero pad bits are accepted; only the bytes count.
  assert.deepEqual(n(`:${sixteen.toString('base64').replace(/=+$/, '')}:`), sixteen);
  assert.deepEqual(n(':AAECAwQFBgcICQoLDA0ODx==:'), sixteen);
  // Parameters are ignored; leading and trailing white space of the line is not part of the value.
  assert.deepEqual(n(`${sf};x=1;y="z"`), sixteen);
  assert.deepEqual(n(` \t${sf}\t `), sixteen);
  // No field, two field lines (also equal and valid, or one empty), a parse failure.
  assert.equal(rqNonce([]), null);
  assert.equal(rqNonce([['rq-have', `t=t1.x, n=${sf}`]]), null);
  assert.equal(n(sf, sf), null);
  assert.equal(n(sf, ''), null);
  assert.equal(n('', sf), null);
  assert.equal(n(''), null);
  assert.equal(n(`${sf}, ${sf}`), null);
  assert.equal(n(`${sf} x`), null);
  assert.equal(n(':AAECAwQFBgcICQoLDA0ODw'), null);
  assert.equal(n(`${sf.slice(0, -1)}=:`), null);
  assert.equal(n(':AAECAwQFBgcICQoLDA0OD!==:'), null);
  assert.equal(n(`${sf}\u00e9`), null);
  // Another type: String, Token, Integer, Decimal, Boolean, Date, Display String, an Inner List, a Dictionary.
  for (const v of [`"${sixteen.toString('base64url')}"`, 'A'.repeat(22), 'A'.repeat(16), '1', '1.5', '?1', '?0', '@1', '%"x"', `(${sf})`, `n=${sf}`]) {
    assert.equal(n(v), null, v);
  }
  // Exactly 16 bytes: 0, 15 and 17 bytes are no nonce.
  for (const length of [0, 1, 15, 17, 32]) assert.equal(n(`:${Buffer.alloc(length).toString('base64')}:`), null, String(length));
  // Field names are lower case here (normalizeHeaders); another field is not Rq-Nonce.
  assert.equal(rqNonce([['rq-nonce-x', sf]]), null);
});

test('Rq-Nonce (DL-063): a field value longer than 128 bytes is no nonce and is not parsed; outer white space does not count', () => {
  const sixteen = Buffer.from([...Array(16).keys()]);
  const withFiller = (length) => `${NONCE_SF};x="${'a'.repeat(length - NONCE_SF.length - 5)}"`;
  assert.equal(withFiller(128).length, 128);
  assert.deepEqual(rqNonce([['rq-nonce', withFiller(128)]]), sixteen);
  assert.equal(rqNonce([['rq-nonce', withFiller(129)]]), null);
  // The limit applies to the value without leading and trailing white space, as for Rq-Have (combined value).
  assert.deepEqual(rqNonce([['rq-nonce', ` \t${withFiller(128)} \t`]]), sixteen);
  // White space inside the value counts: SP after ";" is valid (RFC 9651 section 4.2.3.2).
  assert.deepEqual(rqNonce([['rq-nonce', `${NONCE_SF};${' '.repeat(100)}a`]]), sixteen);
  assert.equal(rqNonce([['rq-nonce', `${NONCE_SF};${' '.repeat(101)}a`]]), null);
  // The cap is in bytes: a line of 128 characters is 128 bytes only in ASCII (a non-ASCII byte fails the parse anyway).
  assert.equal(rqNonce([['rq-nonce', `${NONCE_SF};x="${'é'.repeat(40)}"`]]), null);
  // F for a request whose only defect is the length.
  const r = handle(deltaRequest('page-big-a', { 'rq-nonce': withFiller(129) }), APP, page('page-big-b'));
  assert.deepEqual([r.kind, r.reason], ['full', 'no-nonce']);
  assert.equal(handle(deltaRequest('page-big-a', { 'rq-nonce': withFiller(128) }), APP, page('page-big-b')).kind, 'delta');
});

test('withoutRqVary (DL-063, L2): Rq-* members of Vary go (ASCII case-insensitive); a line without one stays byte for byte', () => {
  const lines = [
    ['content-type', 'text/html'],
    ['vary', 'Accept-Encoding,Cookie'], // no Rq-* member: unchanged, also without a space after the comma
    ['vary', ' Accept-Encoding , Rq-Nonce,,RQ-Have, rq-x'],
    ['vary', 'Rq-Nonce'], // only Rq-*: the line goes
    ['vary', 'Rqx, X-Rq-Nonce, rq'], // not Rq-*: "Rq-" must be the first three characters
    ['x-vary', 'Rq-Nonce'], // another field
  ];
  assert.deepEqual(withoutRqVary(lines), [
    ['content-type', 'text/html'],
    ['vary', 'Accept-Encoding,Cookie'],
    ['vary', 'Accept-Encoding'],
    ['vary', 'Rqx, X-Rq-Nonce, rq'],
    ['x-vary', 'Rq-Nonce'],
  ]);
  assert.deepEqual(withoutRqVary([['vary', ' , ']]), [['vary', ' , ']]); // no member at all: unchanged
  assert.deepEqual(withoutRqVary([]), []);
  // The kept members are joined with a comma and a space.
  assert.deepEqual(withoutRqVary([['vary', 'Accept-Encoding,Cookie,Rq-Nonce']]), [['vary', 'Accept-Encoding, Cookie']]);
});

test('DL-063 (L1, L2): Vary of A loses its Rq-* members in every outcome; D lists Rq-Nonce and has X-Content-Type-Options: nosniff', () => {
  const app = (status = 200) => ({ status, headers: [...APP.headers, ['vary', 'Accept-Encoding, RQ-Nonce'], ['vary', 'rq-have'], ['x-content-type-options', 'other']] });
  const cases = [
    ['out-of-scope', handle(deltaRequest('page-a'), app(404), page('page-a')), 'Accept-Encoding'],
    ['invalid-page', handle(firstVisit(), app(), page('page-c')), 'Accept-Encoding'],
    ['full', handle(firstVisit(), app(), page('page-a')), 'Accept-Encoding'],
    ['head', handle(firstVisit('HEAD'), app(), page('page-a')), 'Accept-Encoding'],
    ['not-modified', handle(deltaRequest('page-a'), app(), page('page-a')), 'Accept-Encoding'],
    ['delta', handle(deltaRequest('page-big-a'), app(), page('page-big-b')), 'Accept-Encoding, Rq-Nonce'],
  ];
  const reasons = { full: 'not-eligible', head: 'not-eligible' };
  for (const [what, r, vary] of cases) {
    assert.equal(r.reason, reasons[what] ?? what, what);
    assert.equal(header(r, 'vary'), vary, what);
    // D replaces the X-Content-Type-Options of A with exactly one line nosniff; the other outcomes keep the field of A.
    const xcto = r.headers.filter(([n]) => n === 'x-content-type-options').map(([, v]) => v);
    assert.deepEqual(xcto, what === 'delta' ? ['nosniff'] : what === 'not-modified' ? [] : ['other'], what);
  }
  // Without a Vary of A, D has exactly one Vary line, Rq-Nonce; F has none.
  assert.deepEqual(handle(deltaRequest('page-big-a'), APP, page('page-big-b')).headers.filter(([n]) => n === 'vary'), [['vary', 'Rq-Nonce']]);
  assert.equal(header(handle(firstVisit(), APP, page('page-a')), 'vary'), null);
});

test('H-108, H-109: a delta only for a request with exactly one valid Rq-Nonce; the 304 does not need it', () => {
  const sf16 = (n) => `:${Buffer.alloc(n).toString('base64')}:`;
  for (const v of [null, sf16(15), sf16(17), [NONCE_SF, NONCE_SF], `${NONCE_SF}, ${NONCE_SF}`, '?1', '"AAECAwQFBgcICQoLDA0ODw"']) {
    const r = handle(deltaRequest('page-big-a', { 'rq-nonce': v }), APP, page('page-big-b'));
    assert.deepEqual([r.kind, r.reason], ['full', 'no-nonce'], String(v));
  }
  // The nonce of H-108 in Rq-Have alone is no nonce.
  const have = deltaRequest('page-big-a').headers.find(([name]) => name === 'rq-have')[1];
  const inHave = handle(deltaRequest('page-big-a', { 'rq-have': `${have}, n=${NONCE_SF}`, 'rq-nonce': null }), APP, page('page-big-b'));
  assert.deepEqual([inHave.kind, inHave.reason], ['full', 'no-nonce']);
  assert.equal(handle(deltaRequest('page-big-a', { 'rq-nonce': null }), APP, page('page-big-a')).status, 304);
  assert.equal(handle(deltaRequest('page-big-a'), APP, page('page-big-b')).kind, 'delta');
  // The nonce is checked after eligibility: a request that is not eligible keeps its reason.
  assert.equal(handle(deltaRequest('page-big-a', { 'rq-nonce': null, 'rq-version': null }), APP, page('page-big-b')).reason, 'not-eligible');
});

test('If-None-Match gives a base only for exactly one r1. tag, strong or weak', () => {
  const r1 = `r1.${'A'.repeat(43)}`;
  const b = (v) => deltaBase([['if-none-match', v]]);
  assert.equal(b(`"${r1}"`), r1);
  assert.equal(b(`W/"${r1}"`), r1);
  assert.equal(b(`"${r1}", "${r1}"`), null);
  assert.equal(b('*'), null);
  assert.equal(b('"abc"'), null);
  assert.equal(b(`"t1.${'A'.repeat(43)}"`), null);
  assert.equal(b(`"${r1}A"`), null);
  assert.equal(deltaBase([]), null);
});

test('If-None-Match condition: weak comparison against the ETag of F', () => {
  const m = (v) => notModified([['if-none-match', v]], '"r1.X"');
  assert.equal(m('"r1.X"'), true);
  assert.equal(m('W/"r1.X"'), true);
  assert.equal(m('"a", "r1.X"'), true);
  assert.equal(m('*'), true);
  assert.equal(m('"r1.Y"'), false);
  assert.equal(m('r1.X'), false);
  assert.equal(notModified([], '"r1.X"'), false);
  assert.equal(notModified([['if-none-match', '"r1.X"']], 'W/"r1.X"'), true);
});

test('entity-tag lists: commas inside a tag, empty elements, syntax errors', () => {
  assert.deepEqual(parseEntityTags('"a,b"').tags, [{ weak: false, opaque: '"a,b"' }]);
  assert.equal(parseEntityTags('"a", , W/"b"').tags.length, 2);
  assert.equal(parseEntityTags('"a" "b"'), null);
  assert.equal(parseEntityTags('"a'), null);
  assert.equal(parseEntityTags(''), null);
});

test('Accept: the delta media type with q > 0, no wildcard', () => {
  const t = 'application/vnd.rivqen.delta+json';
  assert.equal(acceptsMediaType(`text/html, ${t}`, t), true);
  assert.equal(acceptsMediaType('Application/Vnd.Rivqen.Delta+JSON', t), true);
  assert.equal(acceptsMediaType(`${t};q=0.5`, t), true);
  assert.equal(acceptsMediaType(`${t};Q=0.001`, t), true);
  assert.equal(acceptsMediaType(`${t};q=0`, t), false);
  assert.equal(acceptsMediaType(`${t};q=0.000`, t), false);
  assert.equal(acceptsMediaType(`${t};q=abc`, t), false);
  assert.equal(acceptsMediaType('*/*', t), false);
  assert.equal(acceptsMediaType('application/*', t), false);
  assert.equal(acceptsMediaType(null, t), false);
  assert.equal(acceptsMediaType(`text/html;a="x,${t}"`, t), false);
});

test('DL-030: no outcome carries an Rq-* field of the application response', () => {
  const rq = [['rq-version', '2'], ['Rq-Revision', 'r1.app'], ['rq-x', '1']];
  const app = { status: 200, headers: [...APP.headers, ...rq] };
  const cases = [
    ['out-of-scope', handle(deltaRequest('page-a', {}, 'POST'), app, page('page-a'))],
    ['invalid-page', handle(firstVisit(), app, page('page-c'))],
    ['full', handle(firstVisit(), app, page('page-a'))],
    ['delta', handle(deltaRequest('page-big-a'), app, page('page-big-b'))],
    ['not-modified', handle(deltaRequest('page-a'), app, page('page-a'))],
  ];
  for (const [what, r] of cases) {
    assert.equal(header(r, 'rq-revision'), null, what);
    assert.equal(header(r, 'rq-x'), null, what);
    const versions = r.headers.filter(([n]) => n === 'rq-version').map(([, v]) => v);
    assert.deepEqual(versions, what === 'full' || what === 'delta' ? ['1'] : [], what);
  }
  assert.deepEqual(cases[0][1].body, page('page-a')); // the body of A stays
});

test('DL-032: F and its 304 have no Last-Modified of the application; a passthrough keeps it', () => {
  const app = { status: 200, headers: [...APP.headers, ['last-modified', 'Mon, 12 Oct 2026 08:00:00 GMT']] };
  assert.equal(header(handle(firstVisit(), app, page('page-a')), 'last-modified'), null);
  assert.equal(header(handle(firstVisit('HEAD'), app, page('page-a')), 'last-modified'), null);
  assert.equal(header(handle(deltaRequest('page-a'), app, page('page-a')), 'last-modified'), null);
  assert.equal(header(handle(firstVisit(), app, page('page-c')), 'last-modified'), 'Mon, 12 Oct 2026 08:00:00 GMT');
});

test('no response of a valid page has Rq-Revision or Rq-Template-Revision (H-54)', () => {
  for (const r of [handle(firstVisit(), APP, page('page-a')), handle(deltaRequest('page-big-a'), APP, page('page-big-b')), handle(deltaRequest('page-a'), APP, page('page-a'))]) {
    assert.ok(!r.headers.some(([n]) => n === 'rq-revision' || n === 'rq-template-revision'));
  }
});

test('the delta carries the exact content of a block with quotes, escapes, controls and non-ASCII text', () => {
  const content = 'a "quoted" \\ line\nnext\ttab é \u{1F600} <i>x</i>';
  const before = analyzeBytes(blockPage('old'));
  const request = {
    method: 'GET',
    target: TARGET,
    headers: [
      ['rq-version', '1'],
      ['rq-capabilities', '(block-patch)'],
      ['accept', 'application/vnd.rivqen.delta+json'],
      ['if-none-match', `"${before.page_revision}"`],
      ['rq-have', `t=${before.template_revision}`],
      ['rq-nonce', NONCE_SF],
    ],
  };
  const r = handle(request, APP, blockPage(content));
  assert.equal(r.kind, 'delta');
  const op = JSON.parse(r.body.toString('utf8')).operations[0];
  assert.deepEqual(op, { block_id: 'x', content });
});

test('writtenMembers: only max_patch_bytes is judged against the server limit; another limit keeps its value', () => {
  assert.deepEqual(writtenMembers({ limits: { other_limit: 1048576, max_patch_bytes: 262144 } }, 262144), { limits: { other_limit: 1048576 } });
});
