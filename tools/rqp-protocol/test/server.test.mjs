// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors
//
// Reference server step: one test per branch of negotiation.md section 4, and
// the field rules of section 3.

import assert from 'node:assert/strict';
import { test } from 'node:test';
import { UnsupportedInput } from '../src/common.mjs';
import { capabilities, changeSet, deltaBase, handle, notModified, requestVersion, rqHave } from '../src/server.mjs';
import { acceptsMediaType, parseEntityTags } from '../src/http.mjs';
import { analyze as analyzeBytes, documentHeadEnd } from '../src/markup.mjs';
import { APP, TARGET, blockPage, deltaRequest, firstVisit, page, revisions } from './helpers.mjs';

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

test('step 2: a page without </head> in HTML context gets A unchanged (H-49)', () => {
  const noHead = Buffer.from('<!DOCTYPE html><title>x</title><!-- </head> --><p data-rq-block="a">1</p>', 'utf8');
  const r = handle(firstVisit(), APP, noHead);
  assert.equal(r.reason, 'no-head-end');
  assert.deepEqual(r.body, noHead);
});

test('step 2: the manifest goes before the first </head> end tag, not "</head>" text in script data or a comment', () => {
  const doc = Buffer.from('<!DOCTYPE html><html><head><script>var s = "</head>";</script><!-- </head> --></head><body></body></html>', 'utf8');
  const r = handle(firstVisit(), APP, doc);
  assert.equal(r.kind, 'full');
  assert.equal(r.body.indexOf('<script type="application/rivqen-manifest+json">'), doc.indexOf('</head><body>'));
});

// DL-031: the head end tag of the document (manifest-patch section 1.7).
const HEAD_BODY = '<body><p data-rq-block="a">1</p></body></html>';
const headEnd = (prefix) => documentHeadEnd(Buffer.from(prefix + HEAD_BODY, 'utf8'));

test('DL-031: the head end tag after white space, comments, DOCTYPE, html, a BOM and the head content', () => {
  const ok = [
    '<!DOCTYPE html><html><head></head>',
    '﻿ \t\n\r\f<!-- c --><!DOCTYPE html><html><head><!-- c --> <!doctype x></head>',
    '<head><base href="/"><basefont><bgsound><link rel="a"><meta charset="utf-8"><noframes>n</noframes><noscript>n</noscript>' +
      '<script>var s = "</head>";</script><style>s</style><template><p>t</p><head></head></template><title>t</title></head>',
    '<HTML><HEAD/></HEAD>',
  ];
  for (const p of ok) assert.equal(headEnd(p), Buffer.from(p).lastIndexOf('</'), p);
});

test('DL-031: no head end tag when something else comes before or in the head', () => {
  const none = [
    '<html><title>t</title></head>', // no <head> start tag
    '<html>x<head></head>', // text before the head
    '<p>x</p><head></head>', // another start tag before the head
    '<html><meta charset="utf-8"><head></head>', // head content before the head start tag (the head is implied)
    '<html><head>x</head>', // text in the head
    '<html><head></>', // "</>" gives no token: its bytes are not white space
    '<html><head><div></head>', // a start tag that is not head content
    '<html><head><svg><desc>d</desc></svg></head>', // svg closes the head
    '<html><head><head></head>', // a second head start tag
    '<html><head><title>t</title><body></head>', // the head is closed by body; </head> is stray
    '<html><head><template></head></template></head>', // </head> crosses the template
    '<html><head><template></head></template><div></head>', // the head left the stack: a later </head> never counts
    '<html><head><title>t</title>', // no </head> at all
  ];
  for (const p of none) assert.equal(headEnd(p), -1, p);
});

test('DL-034: an end tag other than head, body, html and br is ignored before and in the head', () => {
  const ok = [
    '<html><head><meta charset="utf-8"></meta><link rel="a"></link><base href="/"></base></head>', // stray void end tags
    '<html><head><script>s</script></script></head>', // a second </script> with no script open
    '<html><head></p></div></title></head>', // any other name, also of a head content element
    '</p><html></div><head></head>', // before the head (item 1)
    '<html><head> </meta> <!-- c --> </head>', // white space around it
  ];
  for (const p of ok) assert.equal(headEnd(p), Buffer.from(p).lastIndexOf('</'), p);
});

test('DL-034: </body>, </html> and </br> still fail, before and in the head, also without an <html> start tag', () => {
  const none = [
    '<html><head></br></head>',
    '<html><head></body></head>',
    '<html><head></html></head>',
    '<head></html></head>', // no <html> start tag: nothing to pop, still a failure
    '<head></body></head>',
    '<head></br></head>',
    '</br><head></head>', // item 1
    '</body><head></head>',
    '</html><head></head>',
    '<html></html><head></head>',
    '<html><head>x</meta></head>', // the gap before an ignored end tag is still checked
    'x</p><head></head>', // the same before the head (item 1)
  ];
  for (const p of none) assert.equal(headEnd(p), -1, p);
  // Upper case names are the same names (the tokenizer lowers them).
  assert.equal(headEnd('<html><head></BR></head>'), -1);
  const upper = '<html><head></META></head>';
  assert.equal(headEnd(upper), upper.lastIndexOf('</'));
});

test('DL-031: a later head element and a later </head> never count', () => {
  assert.equal(headEnd('<html><head><body></body><head></head>'), -1);
  const once = '<html><head></head><head></head>';
  assert.equal(headEnd(once), once.indexOf('</head>'));
});

test('step 2: an svg in the body does not change the head end tag', () => {
  const doc = Buffer.from('<!DOCTYPE html><html><head><title>x</title></head><body><svg><g></g></svg><p data-rq-block="a">1</p></body></html>', 'utf8');
  const r = handle(firstVisit(), APP, doc);
  assert.equal(r.kind, 'full');
  assert.equal(r.body.indexOf('<script type="application/rivqen-manifest+json">'), doc.indexOf('</head>'));
});

test('step 2: an svg start tag while the head is the current node closes the head: no RQP (DL-031)', () => {
  const inHead = Buffer.from('<!DOCTYPE html><html><head><svg><desc>d</desc></svg></head><body><p data-rq-block="a">1</p></body></html>', 'utf8');
  assert.equal(handle(firstVisit(), APP, inHead).reason, 'no-head-end');
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

test('step 3: F of a page without blocks has "blocks": []', () => {
  const r = handle(firstVisit(), APP, page('page-zero'));
  assert.match(r.body.toString('utf8'), /"blocks":\[\]/);
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
  assert.deepEqual(r.headers.map(([n]) => n), ['x-app', 'content-type', 'rq-version', 'cache-control', 'content-length']);
  assert.equal(header(r, 'cache-control'), 'no-store');
  assert.equal(header(r, 'content-length'), String(r.body.length));
  const env = JSON.parse(r.body.toString('utf8'));
  assert.equal(env.base_revision, revisions('page-big-a').page_revision);
  assert.equal(env.next_revision, revisions('page-big-b').page_revision);
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
  assert.equal(handle(deltaRequest('page-a'), APP, page('page-b-onclick')).reason, 'attribute-rule');
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
    ],
  };
  const r = handle(request, APP, blockPage(content));
  assert.equal(r.kind, 'delta');
  const op = JSON.parse(r.body.toString('utf8')).operations[0];
  assert.equal(op.content, content);
  assert.equal(op.sha256, analyzeBytes(blockPage(content)).blocks[0].sha256);
});
