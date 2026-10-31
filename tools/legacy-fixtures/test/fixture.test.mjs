// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

import assert from 'node:assert/strict';
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { after, test } from 'node:test';
import { CaseError, DEFAULT_APP, UPSTREAM, expectedOf, loadCase, profileOfName, resolve, resolvedRequest, tagOf } from '../src/fixture.mjs';
import { sha1hex } from '../src/model.mjs';
import { bodyKindOf, compareToTrace, parseTrace, TraceError } from '../src/trace.mjs';
import { first, hasDirective, normalizeFields, parseContentType, single, trimOws, valuesOf, without } from '../src/fields.mjs';
import { APP, CANONICAL, etagOf, legacy, page, pagesDir, tagOf as modelTag, traceText } from './helpers.mjs';

const tmp = mkdtempSync(join(tmpdir(), 'legacy-fixtures-case-'));
after(() => rmSync(tmp, { recursive: true, force: true }));
const dirs = pagesDir();

test('placeholders_resolved_per_profile', () => {
  const pageBytes = page('emptyname');
  assert.equal(resolve('X', '{{etag:a}}', CANONICAL, dirs), etagOf('a'));
  assert.equal(resolve('X', '{{ETAG:a}}', CANONICAL, dirs), etagOf('a').toUpperCase());
  assert.equal(resolve('X', '"{{etag:a}}"', CANONICAL, dirs), `"${etagOf('a')}"`);
  assert.equal(resolve('X', 'W/"{{etag:a}}"', 'legacy-java', dirs), `W/"${etagOf('a')}"`);
  // the tag depends on the profile of the fixture: node gives auto names to empty blocks
  const canonical = resolve('X', '{{tag:emptyname}}', CANONICAL, dirs);
  const node = resolve('X', '{{tag:emptyname}}', 'legacy-node', dirs);
  const java = resolve('X', '{{tag:emptyname}}', 'legacy-java', dirs);
  assert.equal(canonical, tagOf(CANONICAL, pageBytes));
  assert.equal(node, '3abaaf0fbd449206c1c3d953f0219f39cde7d8a2');
  assert.equal(java, 'cb9e61d48580a1ecf19e2ec8a026447fcf884bbd');
  assert.notEqual(canonical, node);
  assert.notEqual(canonical, java);
  assert.equal(resolve('X', '{{TAG:emptyname}}', 'legacy-node', dirs), node.toUpperCase());
  assert.equal(resolve('X', 'a{{tag:a}}b', CANONICAL, dirs), `a${modelTag(CANONICAL, 'a')}b`);
  // no tag: the empty string
  assert.equal(resolve('X', '{{tag:dup}}', CANONICAL, dirs), '');
  assert.equal(resolve('X', '{{tag:dup}}', 'legacy-java', dirs).length, 40);
});

test('placeholder_errors_are_case_errors', () => {
  assert.throws(() => resolve('X', '{{etag:nope}}', CANONICAL, dirs), (e) => e instanceof CaseError && e.message === 'X: page nope is missing');
  assert.throws(() => resolve('X', '{{foo:a}}', CANONICAL, dirs), /unknown placeholder/);
  assert.throws(() => resolve('X', '{{etag:A}}', CANONICAL, dirs), /unknown placeholder/);
  assert.equal(resolve('X', 'plain', CANONICAL, dirs), 'plain');
});

test('tag_of_an_invalid_utf8_page_is_empty', () => {
  assert.equal(tagOf(CANONICAL, Buffer.from([0xff])), '');
  assert.equal(tagOf(CANONICAL, Buffer.from('<p>x</p>')), sha1hex('<p>x</p>'));
});

function caseDir(name, value) {
  const dir = join(tmp, 'cases');
  mkdirSync(join(dir, name), { recursive: true });
  writeFileSync(join(dir, name, 'case.json'), typeof value === 'string' ? value : JSON.stringify(value));
  return dir;
}

const valid = () => ({
  id: 'FX-LEG-OK',
  description: 'd',
  profile: 'legacy-canonical',
  rules: ['C-02'],
  upstream: { ...UPSTREAM },
  sources: ['docs:x#y'],
  evidence: 'code-reading',
  page: 'a',
  request: { headers: [['accept-diff', 'true']] },
});

test('canonical_case_gets_its_defaults', () => {
  const c = loadCase(caseDir('FX-LEG-OK', valid()), 'FX-LEG-OK');
  assert.equal(c.kind, 'canonical');
  assert.equal(c.level, 'must');
  assert.deepEqual(c.app, DEFAULT_APP);
  assert.deepEqual(c.request, { method: 'GET', target: 'https://shop.example.com/catalog', headers: [['accept-diff', 'true']] });
});

test('case_defects_are_case_errors', () => {
  const bad = (change, message, name = 'FX-LEG-OK') => {
    const value = { ...valid(), id: name };
    change(value);
    assert.throws(() => loadCase(caseDir(name, value), name), (e) => e instanceof CaseError && message.test(e.message), `${message}`);
  };
  bad((c) => { c.extra = 1; }, /unknown member case.extra/);
  bad((c) => { delete c.page; }, /missing member case.page/);
  bad((c) => { c.id = 'FX-LEG-OTHER'; }, /id must be FX-LEG-OK/);
  bad((c) => { c.description = ''; }, /description/);
  bad((c) => { c.sources = []; }, /sources/);
  bad((c) => { c.sources = ['somewhere']; }, /sources/);
  bad((c) => { c.profile = 'legacy-java'; }, /profile must be legacy-canonical/);
  bad((c) => { c.rules = ['C-1']; }, /rules/);
  bad((c) => { c.upstream = { repo: 'x', sha: 'y' }; }, /pinned/);
  bad((c) => { c.upstream.extra = 1; }, /unknown member upstream.extra/);
  bad((c) => { c.evidence = 'guess'; }, /evidence/);
  bad((c) => { c.evidence = 'trace'; }, /traces must be an object/);
  bad((c) => { c.traces = {}; }, /only for evidence trace/);
  bad((c) => { c.evidence = 'trace'; c.traces = { 'legacy-java': 'a', 'legacy-node': 'b', 'legacy-php': 1 }; }, /traces.legacy-php/);
  bad((c) => { c.page = 'A/b'; }, /page must be a page ID/);
  bad((c) => { c.app = { status: '200', headers: [] }; }, /app.status/);
  bad((c) => { c.app = { status: 200, headers: [['a']] }; }, /pairs of strings/);
  bad((c) => { c.app = { status: 200 }; }, /missing member app.headers/);
  bad((c) => { c.request = { headers: 'x' }; }, /request.headers must be an array/);
  bad((c) => { c.request = { headers: [], method: 5 }; }, /strings/);
  bad((c) => { c.request = { headers: [], target: 5 }; }, /strings/);
  bad((c) => { c.level = 'maybe'; }, /level/);
  bad((c) => { c.level = 'should'; c.request = []; }, /request must be an object/);
  bad(() => {}, /not a fixture ID/, 'FX-LEG-lower');
  assert.throws(() => loadCase(caseDir('FX-LEG-BROKEN', '{'), 'FX-LEG-BROKEN'), /case.json cannot be read/);
  assert.throws(() => loadCase(caseDir('FX-LEG-ARRAY', '[]'), 'FX-LEG-ARRAY'), /must be an object/);
  assert.equal(loadCase(caseDir('FX-LEG-OK', { ...valid(), level: 'should' }), 'FX-LEG-OK').level, 'should');
});

test('profile_case_is_checked', () => {
  const profile = () => ({ id: 'FX-LEG-OK--JAVA', description: 'd', profile: 'legacy-java', replaces: 'FX-LEG-OK', divergences: ['D-09'], sources: ['upstream:x:1'], trace: 't' });
  const name = 'FX-LEG-OK--JAVA';
  assert.equal(loadCase(caseDir(name, profile()), name).kind, 'profile');
  const bad = (change, message) => {
    const value = profile();
    change(value);
    assert.throws(() => loadCase(caseDir(name, value), name), (e) => e instanceof CaseError && message.test(e.message), `${message}`);
  };
  bad((c) => { c.profile = 'legacy-php'; }, /profile must be legacy-java/);
  bad((c) => { c.replaces = 'FX-LEG-OTHER'; }, /replaces/);
  bad((c) => { c.divergences = ['X-1']; }, /divergences/);
  bad((c) => { c.divergences = 'D-01'; }, /divergences/);
  bad((c) => { c.trace = 1; }, /trace must be a string/);
  bad((c) => { c.not_reproduced = 'x'; }, /not_reproduced/);
  bad((c) => { delete c.trace; }, /missing member case.trace/);
  bad((c) => { c.rules = ['C-01']; }, /unknown member case.rules/);
  assert.equal(loadCase(caseDir(name, { ...profile(), not_reproduced: 'U-11' }), name).not_reproduced, 'U-11');
  assert.equal(profileOfName('FX-LEG-A--PHP'), 'legacy-php');
  assert.equal(profileOfName('FX-LEG-A'), null);
  assert.equal(profileOfName('FX-LEG-A--XYZ'), null);
});

test('expected_of_a_data_case_has_split_and_body_members', () => {
  const c = { ...valid(), kind: 'canonical', level: 'must', app: DEFAULT_APP, request: { method: 'GET', target: 't', headers: [['accept-diff', 'true'], ['template-tag', '{{tag:a}}']] }, page: 'b', id: 'FX-LEG-OK' };
  const e = expectedOf(c, CANONICAL, page('b'), dirs);
  assert.deepEqual(Object.keys(e), ['id', 'profile', 'level', 'request', 'status', 'fields', 'body', 'split']);
  assert.equal(e.request.headers[1][1], modelTag(CANONICAL, 'a'));
  assert.deepEqual(Object.keys(e.fields), [
    'etag', 'template-tag', 'template-change', 'cache-offline', 'cache-control', 'content-type', 'sonic-etag-key',
    'vary', 'cdn-cache-control', 'surrogate-control',
  ]);
  assert.deepEqual(e.fields.vary, ['template-tag']);
  assert.deepEqual(e.fields['cdn-cache-control'], []);
  assert.equal(e.body.kind, 'data');
  assert.equal(e.body.html_sha1, etagOf('b'));
  assert.deepEqual(e.split, { applied: true, template_sha1: modelTag(CANONICAL, 'a'), keys: ['{title}', '{price}', '{stock}'], rebuild_sha1: etagOf('b') });
  // extra fields of the response follow the fixed ones in order; content-length is dropped
  const app = { status: 200, headers: [...APP.headers, ['X-App', '1'], ['x-app', '2'], ['Content-Length', '9']] };
  const f = expectedOf({ ...c, app }, CANONICAL, page('b'), dirs);
  assert.deepEqual(f.fields['x-app'], ['1', '2']);
  assert.equal(Object.hasOwn(f.fields, 'content-length'), false);
  assert.equal(Object.keys(f.fields).at(-1), 'x-app');
});

test('expected_of_a_304_and_of_a_pass_through_case', () => {
  const base = { ...valid(), kind: 'canonical', level: 'should', app: DEFAULT_APP, id: 'FX-LEG-OK' };
  const nm = expectedOf({ ...base, request: { method: 'GET', target: 't', headers: [['accept-diff', 'true'], ['if-none-match', '{{etag:a}}']] } }, CANONICAL, page('a'), dirs);
  assert.equal(nm.level, 'should');
  assert.equal(nm.status, 304);
  assert.deepEqual(nm.body, { kind: 'none' });
  const head = expectedOf({ ...base, request: { method: 'HEAD', target: 't', headers: [] } }, CANONICAL, page('a'), dirs);
  assert.deepEqual(head.body, { kind: 'none' });
  assert.deepEqual(head.split, { applied: false, template_sha1: null, keys: [], rebuild_sha1: null });
  const post = expectedOf({ ...base, app: { status: 404, headers: [] }, request: { method: 'GET', target: 't', headers: [] } }, CANONICAL, page('a'), dirs);
  assert.deepEqual(post.split, { applied: false, template_sha1: null, keys: [], rebuild_sha1: null });
  assert.deepEqual(post.body, { kind: 'page' });
  const r = resolvedRequest({ ...base, request: { method: 'POST', target: 'u', headers: [['x', '{{etag:a}}']] } }, CANONICAL, dirs);
  assert.deepEqual(r, { method: 'POST', target: 'u', headers: [['x', etagOf('a')]] });
});

// --- trace reader and comparison ---

test('trace_is_parsed_with_the_exact_field_lines', () => {
  const response = { status: 200, headers: [['Etag', 'abc'], ['x', 'a: b'], ['Cache-Offline', 'true'], ['Cache-Offline', 'store']], body: Buffer.from('{"a":1}\n[body decoded from content-encoding=x]\n') };
  const t = parseTrace(traceText('/p/a', [['Accept-Encoding', 'gzip'], ['accept-diff', 'true']], response, 'gzip'));
  assert.equal(t.request.line, 'GET /p/a HTTP/1.1');
  assert.deepEqual(t.request.headers, [['Accept-Encoding', 'gzip'], ['accept-diff', 'true']]);
  assert.equal(t.status, 200);
  assert.deepEqual(t.headers, response.headers);
  assert.equal(t.encoding, 'gzip');
  assert.deepEqual(t.body, response.body);
  const empty = parseTrace(traceText('/p/a', [['a', 'b']], { status: 304, headers: [['Cache-Offline', 'store']], body: null }));
  assert.equal(empty.status, 304);
  assert.equal(empty.body.length, 0);
  assert.equal(empty.encoding, 'identity');
});

test('trace_errors_are_trace_errors', () => {
  const ok = traceText('/p', [['a', 'b']], { status: 200, headers: [['c', 'd']], body: Buffer.from('x') });
  const text = ok.toString();
  const bad = [
    ['missing "--- response ---"', 'GET /p HTTP/1.1\na: b\n'],
    ['missing the body marker line', text.replace('\n[body decoded', '\n[x')],
    ['the body marker line has no end', text.slice(0, text.indexOf(']') + 1)],
    ['bad status line', text.replace('HTTP 200', 'HTTP 2000')],
    ['not a field line', text.replace('c: d', 'cd')],
    ['request: not a field line', text.replace('a: b', ': b')],
  ];
  for (const [message, content] of bad) {
    assert.throws(() => parseTrace(Buffer.from(content)), (e) => e instanceof TraceError && e.code === 'LEGACY_TRACE_FORMAT' && e.message.includes(message), message);
  }
});

test('body_kind_of_a_trace_body', () => {
  assert.equal(bodyKindOf(Buffer.alloc(0)), 'none');
  assert.equal(bodyKindOf(Buffer.from('<html>')), 'page');
  assert.equal(bodyKindOf(Buffer.from('{"data":{},"html-sha1":"x"}')), 'data');
  assert.equal(bodyKindOf(Buffer.from('{"data":{}}')), 'page');
  assert.equal(bodyKindOf(Buffer.from('[1]')), 'page');
  assert.equal(bodyKindOf(Buffer.from('null')), 'page');
  assert.equal(bodyKindOf(Buffer.from('{"data":')), 'page');
});

test('trace_comparison_reports_status_fields_and_body', () => {
  const request = legacy(['template-tag', modelTag(CANONICAL, 'a')]);
  const c = { ...valid(), kind: 'canonical', level: 'must', app: DEFAULT_APP, request: { method: 'GET', target: 't', headers: request.headers }, page: 'b' };
  const expected = expectedOf(c, CANONICAL, page('b'), dirs);
  const trace = (response) => parseTrace(traceText('/p', [['a', 'b']], response));
  const good = { status: 200, headers: [['Cache-Control', 'no-cache, private'], ['Cache-Offline', 'true'], ['Etag', etagOf('b')], ['template-tag', modelTag(CANONICAL, 'a')], ['template-change', 'false']], body: Buffer.from(JSON.stringify({ 'html-sha1': etagOf('b'), diff: '', 'template-tag': modelTag(CANONICAL, 'a'), data: expected.body.data })) };
  assert.deepEqual(compareToTrace(expected, trace(good), page('b')), []);
  assert.deepEqual(compareToTrace(expected, trace({ ...good, status: 201 }), page('b')), ['status: expected 200, trace 201']);
  const noOffline = compareToTrace(expected, trace({ ...good, headers: good.headers.filter(([n]) => n !== 'Cache-Offline') }), page('b'));
  assert.deepEqual(noOffline, ['cache-offline: expected ["true"], trace []']);
  const twice = compareToTrace(expected, trace({ ...good, headers: [...good.headers, ['Cache-Offline', 'true']] }), page('b'));
  assert.equal(twice.length, 1);
  assert.deepEqual(compareToTrace(expected, trace({ ...good, body: Buffer.from('<html>') }), page('b')), ['body: expected data, trace page']);
  const changed = JSON.parse(good.body.toString());
  changed.data['{title}'] = 'x';
  assert.deepEqual(compareToTrace(expected, trace({ ...good, body: Buffer.from(JSON.stringify(changed)) }), page('b')), ['body: data differs']);
  changed.data = expected.body.data;
  changed.diff = 'x';
  assert.deepEqual(compareToTrace(expected, trace({ ...good, body: Buffer.from(JSON.stringify(changed)) }), page('b')), ['body: data differs']);
  // a page body is compared byte for byte
  const full = expectedOf({ ...c, request: { method: 'GET', target: 't', headers: [['accept-diff', 'true']] } }, CANONICAL, page('b'), dirs);
  const fullTrace = (body) => trace({ status: 200, headers: [['Cache-Control', 'no-cache, private'], ['Cache-Offline', 'true'], ['Etag', etagOf('b')], ['template-tag', modelTag(CANONICAL, 'b')], ['template-change', 'true']], body });
  assert.deepEqual(compareToTrace(full, fullTrace(page('b')), page('b')), []);
  assert.deepEqual(compareToTrace(full, fullTrace(page('a')), page('b')), ['body: page bytes differ']);
  // the none kind: an empty body
  const nm = expectedOf({ ...c, request: { method: 'GET', target: 't', headers: [['accept-diff', 'true'], ['if-none-match', etagOf('b')]] } }, CANONICAL, page('b'), dirs);
  assert.equal(nm.body.kind, 'none');
  assert.deepEqual(compareToTrace(nm, trace({ status: 304, headers: [['Cache-Control', 'no-cache, private'], ['Cache-Offline', 'store']], body: null }), page('b')), []);
  assert.deepEqual(compareToTrace(nm, trace({ status: 304, headers: [['Cache-Control', 'no-cache, private'], ['Cache-Offline', 'store']], body: Buffer.from('x') }), page('b')), ['body: expected none, trace page']);
});

test('reduced_trace_comparison_skips_the_named_field_and_the_body_content', () => {
  const c = { ...valid(), kind: 'canonical', level: 'must', app: DEFAULT_APP, request: { method: 'GET', target: 't', headers: [['accept-diff', 'true']] }, page: 'b' };
  const expected = expectedOf(c, CANONICAL, page('b'), dirs);
  const trace = (tag, body) => parseTrace(traceText('/p', [['a', 'b']], { status: 200, headers: [['Cache-Control', 'no-cache, private'], ['Cache-Offline', 'true'], ['Etag', etagOf('b')], ['template-tag', tag], ['template-change', 'true']], body }));
  const skip = new Set(['template-tag']);
  assert.deepEqual(compareToTrace(expected, trace('f'.repeat(40), page('a')), page('b'), skip), []);
  assert.deepEqual(compareToTrace(expected, trace(modelTag(CANONICAL, 'b'), page('b')), page('b'), skip), ['template-tag: the trace does not show the documented difference']);
  assert.deepEqual(compareToTrace(expected, trace('f'.repeat(40), Buffer.alloc(0)), page('b'), skip), ['body: expected page, trace none']);
});

// --- field helpers ---

test('field_helpers', () => {
  assert.equal(trimOws(' \t x y \t'), 'x y');
  assert.equal(trimOws(' \t \t'), '');
  assert.equal(trimOws('x\r'), 'x\r');
  assert.equal(trimOws(' x'), ' x');
  assert.deepEqual(normalizeFields([['a', 'b'], ['a'], null, ['a', 1], 'x', [1, 'b']]), [['a', 'b']]);
  assert.deepEqual(normalizeFields('x'), []);
  const fields = [['Accept-Diff', ' true '], ['accept-diff', 'x'], ['Other', 'o']];
  assert.deepEqual(valuesOf(fields, 'ACCEPT-DIFF'), ['true', 'x']);
  assert.equal(first(fields, 'accept-diff'), 'true');
  assert.equal(first(fields, 'none'), null);
  assert.equal(single(fields, 'accept-diff'), null);
  assert.equal(single(fields, 'other'), 'o');
  assert.deepEqual(without(fields, new Set(['accept-diff'])), [['Other', 'o']]);
  assert.deepEqual(parseContentType('Text/HTML ; Charset = "UTF-8"; x'), { type: 'text/html', params: new Map([['charset', 'UTF-8']]) });
  assert.deepEqual([...parseContentType('a; charset=x; charset=y').params], [['charset', 'x']]);
  assert.equal(parseContentType('text/html').params.size, 0);
  assert.equal(hasDirective([['Cache-Control', 'a, NO-STORE=1']], 'no-store'), true);
  assert.equal(hasDirective([['cache-control', 'a'], ['cache-control', ' no-store']], 'no-store'), true);
  assert.equal(hasDirective([['cache-control', 'xno-store, no-store-x']], 'no-store'), false);
  assert.equal(hasDirective([], 'no-store'), false);
});
