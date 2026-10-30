// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

import assert from 'node:assert/strict';
import { test } from 'node:test';
import { PROFILES, ModelError, analyze, respond, sha1hex } from '../src/model.mjs';
import { handle } from '../src/server.mjs';
import { APP, CANONICAL, bodyText, etagOf, fieldValues, legacy, page, run, tagOf } from './helpers.mjs';

const JAVA = 'legacy-java';
const NODE = 'legacy-node';
const PHP = 'legacy-php';
const bytes = (text) => Buffer.from(text, 'utf8');
const html = (s) => bytes(`<html><head><title>T</title></head><body>${s}</body></html>`);
const LEGACY_NAMES = ['etag', 'template-tag', 'template-change', 'cache-offline', 'sonic-etag-key'];

function assertPassThrough(response, app, body) {
  assert.equal(response.status, app.status);
  assert.deepEqual(response.headers, app.headers);
  assert.deepEqual(response.body === null ? null : Buffer.from(response.body), body === null ? null : Buffer.from(body));
}

test('canonical_title_first_match_only', () => {
  const r = respond(CANONICAL, legacy(), APP, bytes('<TITLE lang=x>One</TITLE><svg><title>Two</title></svg>'));
  assert.equal(r.status, 200);
  assert.equal(fieldValues(r, 'template-change')[0], 'true');
  const { page: info } = analyze(CANONICAL, legacy(), APP, bytes('<TITLE lang=x>One</TITLE><svg><title>Two</title></svg>'));
  assert.equal(info.data.get('{title}'), '<TITLE lang=x>One</TITLE>');
  assert.equal(info.template, '{title}<svg><title>Two</title></svg>');
});

test('canonical_empty_name_stays_in_template', () => {
  const body = html('<!--sonicdiff--><p>anon</p><!--sonicdiff-end--><!--sonicdiff-a--><p>a</p><!--sonicdiff-a-end-->');
  const { page: info } = analyze(CANONICAL, legacy(), APP, body);
  assert.deepEqual([...info.data.keys()], ['{title}', '{a}']);
  assert.match(info.template, /<!--sonicdiff--><p>anon<\/p><!--sonicdiff-end-->\{a\}/);
});

test('canonical_duplicate_name_passes_through', () => {
  const body = page('dup');
  const r = respond(CANONICAL, legacy(['template-tag', 'x']), APP, body);
  assertPassThrough(r, APP, body);
  for (const name of LEGACY_NAMES) assert.deepEqual(fieldValues(r, name), []);
  // a block named title collides with the title key in the same way
  const clash = html('<!--sonicdiff-title--><p>x</p><!--sonicdiff-title-end-->');
  assertPassThrough(respond(CANONICAL, legacy(), APP, clash), APP, clash);
});

test('canonical_name_129_chars_rejected', () => {
  const block = (n) => `<!--sonicdiff-${n}--><p>x</p><!--sonicdiff-${n}-end-->`;
  const long = 'p'.repeat(129);
  const ok = 'q'.repeat(128);
  const { page: info } = analyze(CANONICAL, legacy(), APP, html(block(long) + block(ok)));
  assert.deepEqual([...info.data.keys()], ['{title}', `{${ok}}`]);
  assert.ok(info.template.includes(block(long)));
});

test('canonical_304_strips_weak_and_quotes', () => {
  const etag = etagOf('a');
  for (const value of [etag, etag.toUpperCase(), `"${etag}"`, `W/"${etag}"`, `W/${etag}`, `  ${etag}\t`]) {
    const r = run(CANONICAL, legacy(['If-None-Match', value]));
    assert.equal(r.status, 304, value);
    assert.equal(r.body, null);
    assert.deepEqual(fieldValues(r, 'cache-offline'), ['store']);
    assert.deepEqual(fieldValues(r, 'cache-control'), ['no-cache, private']);
    for (const name of ['etag', 'template-tag', 'template-change']) assert.deepEqual(fieldValues(r, name), [], name);
  }
  for (const value of [`""${etag}""`, `"${etag}`, `${etag}x`, `w/"${etag}"`, '']) {
    assert.equal(run(CANONICAL, legacy(['If-None-Match', value])).status, 200, value);
  }
});

test('canonical_two_accept_diff_lines_pass_through', () => {
  const two = { method: 'GET', headers: [['accept-diff', 'true'], ['Accept-Diff', 'true']] };
  assertPassThrough(run(CANONICAL, two), APP, page('a'));
  // two If-None-Match lines: no 304; two template-tag lines: full response
  const etag = etagOf('a');
  const tag = tagOf(CANONICAL, 'a');
  const inm2 = run(CANONICAL, legacy(['If-None-Match', etag], ['If-None-Match', etag]));
  assert.equal(inm2.status, 200);
  const tt2 = run(CANONICAL, legacy(['template-tag', tag], ['template-tag', tag]));
  assert.deepEqual(fieldValues(tt2, 'template-change'), ['true']);
  assert.deepEqual(bodyText(tt2), page('a').toString('utf8'));
  // the value must be exactly true
  for (const value of ['false', 'True', 'yes', '']) assertPassThrough(run(CANONICAL, { method: 'GET', headers: [['accept-diff', value]] }), APP, page('a'));
});

test('canonical_cache_offline_app_value_rules', () => {
  const withOffline = (...values) => ({ status: 200, headers: [...APP.headers, ...values.map((v) => ['cache-offline', v])] });
  const offline = (app, request = legacy()) => fieldValues(run(CANONICAL, request, 'a', app), 'cache-offline');
  for (const v of ['true', 'false', 'store', 'http']) assert.deepEqual(offline(withOffline(v)), [v]);
  assert.deepEqual(offline(withOffline('TRUE')), ['true']);
  assert.deepEqual(offline(withOffline('other')), ['true']);
  assert.deepEqual(offline(withOffline('false', 'http')), ['true']);
  assert.deepEqual(offline({ status: 200, headers: APP.headers }), ['true']);
  // on 304 the value is store whatever the application sets
  assert.deepEqual(offline(withOffline('false'), legacy(['If-None-Match', etagOf('a')])), ['store']);
});

test('canonical_no_store_kept', () => {
  const app = { status: 200, headers: [...APP.headers, ['Cache-Control', 'Private, No-Store'], ['x-app', '1']] };
  const r = run(CANONICAL, legacy(), 'a', app);
  assert.deepEqual(fieldValues(r, 'cache-control'), ['Private, No-Store']);
  assert.deepEqual(fieldValues(r, 'x-app'), ['1']);
  const replaced = run(CANONICAL, legacy(), 'a', { status: 200, headers: [...APP.headers, ['cache-control', 'max-age=60']] });
  assert.deepEqual(fieldValues(replaced, 'cache-control'), ['no-cache, private']);
  // a directive that only contains the text is not no-store
  const near = run(CANONICAL, legacy(), 'a', { status: 200, headers: [...APP.headers, ['cache-control', 'x-no-store=1']] });
  assert.deepEqual(fieldValues(near, 'cache-control'), ['no-cache, private']);
  // no-store also holds on 304
  const notModified = run(CANONICAL, legacy(['If-None-Match', etagOf('a')]), 'a', app);
  assert.equal(notModified.status, 304);
  assert.deepEqual(fieldValues(notModified, 'cache-control'), ['Private, No-Store']);
});

test('canonical_non_utf8_passes_through', () => {
  const bad = Buffer.concat([bytes('<html><body>'), Buffer.from([0xff]), bytes('</body></html>')]);
  assertPassThrough(respond(CANONICAL, legacy(), APP, bad), APP, bad);
  const latin = { status: 200, headers: [['content-type', 'text/html; charset=iso-8859-1']] };
  assertPassThrough(run(CANONICAL, legacy(), 'a', latin), latin, page('a'));
  // charset in capitals and quotes is utf-8
  const quoted = { status: 200, headers: [['Content-Type', 'Text/HTML; Charset="UTF-8"']] };
  assert.equal(fieldValues(run(CANONICAL, legacy(), 'a', quoted), 'template-change')[0], 'true');
  // no charset parameter is fine
  assert.equal(fieldValues(run(CANONICAL, legacy(), 'a', { status: 200, headers: [['content-type', 'text/html']] }), 'etag').length, 1);
});

test('canonical_scope_is_get_200_html', () => {
  const cases = [
    [{ method: 'HEAD', headers: legacy().headers }, APP],
    [{ method: 'POST', headers: legacy().headers }, APP],
    [{ method: 'get', headers: legacy().headers }, APP],
    [legacy(), { status: 404, headers: APP.headers }],
    [legacy(), { status: 302, headers: [...APP.headers, ['location', 'https://shop.example.com/login']] }],
    [legacy(), { status: 200, headers: [['content-type', 'application/json']] }],
    [legacy(), { status: 200, headers: [] }],
    [legacy(), { status: 200, headers: [['content-type', 'text/html'], ['content-type', 'text/html']] }],
    [legacy(), { status: 200, headers: [['content-type', 'text/htmlx']] }],
  ];
  for (const [request, app] of cases) {
    for (const profile of PROFILES) {
      const body = page('a');
      const r = respond(profile, request, app, body);
      assertPassThrough(r, app, request.method === 'HEAD' ? null : body);
    }
  }
});

test('canonical_data_response_body_and_fields', () => {
  const tag = tagOf(CANONICAL, 'a');
  const r = run(CANONICAL, legacy(['template-tag', tag]), 'b');
  assert.equal(r.status, 200);
  const json = JSON.parse(bodyText(r));
  assert.deepEqual(Object.keys(json), ['data', 'template-tag', 'html-sha1', 'diff']);
  assert.equal(json['template-tag'], tag);
  assert.equal(json['html-sha1'], etagOf('b'));
  assert.equal(json.diff, '');
  assert.deepEqual(Object.keys(json.data), ['{title}', '{price}', '{stock}']);
  assert.deepEqual(fieldValues(r, 'template-change'), ['false']);
  assert.deepEqual(fieldValues(r, 'etag'), [etagOf('b')]);
  assert.deepEqual(fieldValues(r, 'template-tag'), [tag]);
  assert.deepEqual(fieldValues(r, 'content-type'), ['text/html; charset=utf-8']);
  // the tag is compared exactly
  assert.deepEqual(fieldValues(run(CANONICAL, legacy(['template-tag', tag.toUpperCase()]), 'b'), 'template-change'), ['true']);
  assert.deepEqual(fieldValues(run(CANONICAL, legacy(['template-tag', ` ${tag}\t`]), 'b'), 'template-change'), ['false']);
  // no HTML escaping, UTF-8
  const text = bodyText(run(CANONICAL, legacy(['template-tag', tagOf(CANONICAL, 'unicode')]), 'unicode'));
  assert.ok(text.includes('<title>Магазин</title>') && text.includes('₽') && text.includes('</p>'));
});

test('canonical_app_fields_replaced_and_kept', () => {
  const app = {
    status: 200,
    headers: [...APP.headers, ['ETag', '"app-1"'], ['template-tag', 'x'], ['Sonic-Etag-Key', 'k'], ['x-app', '1'], ['Content-Length', '1'], ['template-change', 'x']],
  };
  const r = run(CANONICAL, legacy(), 'a', app);
  assert.deepEqual(fieldValues(r, 'etag'), [etagOf('a')]);
  assert.deepEqual(fieldValues(r, 'template-tag'), [tagOf(CANONICAL, 'a')]);
  assert.deepEqual(fieldValues(r, 'sonic-etag-key'), []);
  assert.deepEqual(fieldValues(r, 'content-length'), []);
  assert.deepEqual(fieldValues(r, 'x-app'), ['1']);
  assert.deepEqual(fieldValues(r, 'template-change'), ['true']);
});

test('java_data_response_without_accept_diff', () => {
  const tag = tagOf(JAVA, 'a');
  for (const request of [
    { method: 'GET', headers: [['accept-diff', 'false'], ['template-tag', tag]] },
    { method: 'GET', headers: [['template-tag', tag]] },
  ]) {
    const r = run(JAVA, request, 'b');
    assert.deepEqual(fieldValues(r, 'template-change'), ['false']);
    assert.equal(JSON.parse(bodyText(r))['html-sha1'], etagOf('b'));
    assert.deepEqual(fieldValues(r, 'cache-control'), []);
    assert.deepEqual(fieldValues(r, 'cache-offline'), []);
  }
  // a request without validators gets the legacy fields on a full response (D-02)
  const plain = run(JAVA, { method: 'GET', headers: [] });
  assert.deepEqual(fieldValues(plain, 'template-change'), ['true']);
  assert.deepEqual(fieldValues(plain, 'etag'), [etagOf('a')]);
  // no 304 without accept-diff true
  assert.equal(run(JAVA, { method: 'GET', headers: [['If-None-Match', etagOf('a')]] }).status, 200);
  // legacy request: cache headers, one cache-control line
  const full = run(JAVA, legacy());
  assert.deepEqual(fieldValues(full, 'cache-control'), ['no-cache']);
  assert.deepEqual(fieldValues(full, 'cache-offline'), ['true']);
});

test('java_two_cache_offline_on_304', () => {
  const r = run(JAVA, legacy(['If-None-Match', etagOf('a').toUpperCase()]));
  assert.equal(r.status, 304);
  assert.deepEqual(fieldValues(r, 'cache-offline'), ['true', 'store']);
  assert.deepEqual(fieldValues(r, 'cache-control'), ['no-cache']);
  assert.equal(r.body, null);
  // quotes are not removed
  assert.equal(run(JAVA, legacy(['If-None-Match', `"${etagOf('a')}"`])).status, 200);
});

test('java_escapes_html_characters_in_data', () => {
  const tag = tagOf(JAVA, 'unicode');
  const text = bodyText(run(JAVA, legacy(['template-tag', tag]), 'unicode'));
  assert.ok(text.includes('\\u003c') && text.includes('\\u0026amp;') && text.includes('Магазин'));
  assert.ok(!/[<>]/.test(text));
  assert.deepEqual(JSON.parse(text), JSON.parse(bodyText(run(CANONICAL, legacy(['template-tag', tagOf(CANONICAL, 'unicode')]), 'unicode'))));
});

test('java_empty_and_repeated_names_last_wins', () => {
  const { page: info } = analyze(JAVA, legacy(), APP, page('emptyname'));
  assert.deepEqual([...info.data.keys()], ['{title}', '{}']);
  assert.ok(info.data.get('{}').includes('anon two'));
  const dup = analyze(JAVA, legacy(), APP, page('dup'));
  assert.ok(dup.page.data.get('{x}').includes('second'));
  assert.equal(dup.page.template.split('{x}').length, 3);
});

test('node_identity_encoding_skips_legacy', () => {
  const identity = { method: 'GET', headers: [['Accept-Encoding', 'identity'], ['accept-diff', 'true']] };
  assertPassThrough(run(NODE, identity), APP, page('a'));
  assertPassThrough(run(NODE, { method: 'GET', headers: [['accept-diff', 'true']] }), APP, page('a'));
  for (const coding of ['gzip', 'deflate', 'GZIP;q=0.5', 'br, deflate']) {
    const r = run(NODE, { method: 'GET', headers: [['Accept-Encoding', coding], ['accept-diff', 'true']] });
    assert.deepEqual(fieldValues(r, 'template-change'), ['true'], coding);
  }
  // any non-empty accept-diff value counts
  const falseValue = run(NODE, { method: 'GET', headers: [['Accept-Encoding', 'gzip'], ['accept-diff', 'false']] });
  assert.deepEqual(fieldValues(falseValue, 'template-change'), ['true']);
  assertPassThrough(run(NODE, { method: 'GET', headers: [['Accept-Encoding', 'gzip'], ['accept-diff', '']] }), APP, page('a'));
  assertPassThrough(run(NODE, { method: 'GET', headers: [['Accept-Encoding', 'gzip']] }), APP, page('a'));
});

test('node_auto_names', () => {
  const { page: info } = analyze(NODE, legacy(['Accept-Encoding', 'gzip']), APP, page('emptyname'));
  assert.deepEqual([...info.data.keys()], ['{title}', '{auto0}', '{auto1}']);
  assert.equal(info.template.includes('{auto0}{auto1}'), true);
});

test('node_page_without_blocks_is_always_full', () => {
  const request = { method: 'GET', headers: [['Accept-Encoding', 'gzip'], ['accept-diff', 'true'], ['template-tag', tagOf(NODE, 'nomark')]] };
  const r = run(NODE, request, 'nomark');
  assert.deepEqual(fieldValues(r, 'template-change'), ['true']);
  assert.equal(bodyText(r), page('nomark').toString('utf8'));
  // with blocks the same request shape gets a data response
  const withBlocks = run(NODE, { ...request, headers: [...request.headers.slice(0, 2), ['template-tag', tagOf(NODE, 'a')]] });
  assert.deepEqual(fieldValues(withBlocks, 'template-change'), ['false']);
});

test('node_304_is_case_sensitive_and_has_no_cache_control', () => {
  const base = [['Accept-Encoding', 'gzip'], ['accept-diff', 'true']];
  const r = run(NODE, { method: 'GET', headers: [...base, ['If-None-Match', etagOf('a')]] });
  assert.equal(r.status, 304);
  assert.deepEqual(fieldValues(r, 'cache-offline'), ['store']);
  assert.deepEqual(fieldValues(r, 'cache-control'), []);
  assert.equal(run(NODE, { method: 'GET', headers: [...base, ['If-None-Match', etagOf('a').toUpperCase()]] }).status, 200);
  const full = run(NODE, { method: 'GET', headers: base });
  assert.deepEqual(fieldValues(full, 'cache-control'), []);
  assert.deepEqual(fieldValues(full, 'cache-offline'), ['true']);
});

test('php_capitalized_name_not_detected', () => {
  assertPassThrough(run(PHP, { method: 'GET', headers: [['Accept-Diff', 'true']] }), APP, page('a'));
  assertPassThrough(run(PHP, { method: 'GET', headers: [['accept-diff', 'false']] }), APP, page('a'));
  assertPassThrough(run(PHP, { method: 'GET', headers: [['accept-diff', 'yes']] }), APP, page('a'));
  const r = run(PHP, legacy());
  assert.deepEqual(fieldValues(r, 'template-change'), ['true']);
  assert.deepEqual(fieldValues(r, 'cache-control'), ['no-cache']);
  assert.deepEqual(fieldValues(r, 'cache-offline'), ['true']);
});

test('php_304_has_one_cache_offline_and_is_case_sensitive', () => {
  const r = run(PHP, legacy(['If-None-Match', etagOf('a')]));
  assert.equal(r.status, 304);
  assert.deepEqual(fieldValues(r, 'cache-offline'), ['store']);
  assert.deepEqual(fieldValues(r, 'cache-control'), ['no-cache']);
  assert.equal(run(PHP, legacy(['If-None-Match', etagOf('a').toUpperCase()])).status, 200);
});

test('php_escapes_slash_and_non_ascii_in_data', () => {
  const text = bodyText(run(PHP, legacy(['template-tag', tagOf(PHP, 'unicode')]), 'unicode'));
  assert.ok(text.includes('<\\/p>') && text.includes('\\u0426\\u0435\\u043d\\u0430'));
  assert.ok(!/[^\x00-\x7f]/.test(text));
  assert.deepEqual(JSON.parse(text), JSON.parse(bodyText(run(CANONICAL, legacy(['template-tag', tagOf(CANONICAL, 'unicode')]), 'unicode'))));
});

test('profiles_keep_no_store', () => {
  const app = { status: 200, headers: [...APP.headers, ['cache-control', 'no-store']] };
  assert.deepEqual(fieldValues(run(JAVA, legacy(), 'a', app), 'cache-control'), ['no-store']);
  assert.deepEqual(fieldValues(run(PHP, legacy(), 'a', app), 'cache-control'), ['no-store']);
});

test('no_profile_shares_state_between_requests', () => {
  // U-01: the result of a request does not depend on an earlier request
  for (const profile of PROFILES) {
    const request = legacy(['Accept-Encoding', 'gzip']);
    const before = run(profile, request, 'a');
    run(profile, request, 'dup');
    run(profile, request, 'emptyname');
    assert.deepEqual(run(profile, request, 'a'), before);
  }
});

test('model_never_throws_on_malformed_input', () => {
  const junk = [undefined, null, 0, 'text', [], {}, { headers: 5 }, { headers: [null, 1, ['a'], ['a', 1], ['a', 'b', 'c']] }, { method: 7, headers: [['accept-diff', 'true']] }];
  const apps = [...junk, { status: '200', headers: [] }, { status: 200.5 }];
  for (const profile of PROFILES) {
    for (const request of junk) {
      for (const app of apps) {
        for (const bytesIn of [undefined, null, 'x', page('a'), new Uint8Array(0)]) {
          const r = respond(profile, request, app, bytesIn);
          assert.equal(Number.isInteger(r.status), true);
          assert.ok(Array.isArray(r.headers));
        }
      }
    }
  }
  // an application with no usable members is a 200 without fields
  assert.deepEqual(respond(CANONICAL, undefined, undefined, undefined), { status: 200, headers: [], body: Buffer.alloc(0) });
});

test('model_accepts_a_view_of_a_larger_buffer', () => {
  const big = Buffer.concat([bytes('xx'), page('a'), bytes('yy')]);
  const r = respond(CANONICAL, legacy(), APP, big.subarray(2, 2 + page('a').length));
  assert.deepEqual(fieldValues(r, 'etag'), [etagOf('a')]);
});

test('unknown_profile_is_a_configuration_error', () => {
  assert.throws(() => respond('legacy-ruby', legacy(), APP, page('a')), (e) => e instanceof ModelError && e.code === 'LEGACY_UNKNOWN_PROFILE');
});

test('handle_uses_the_profile_of_the_config', () => {
  const request = { method: 'GET', target: 'https://shop.example.com/catalog', headers: legacy().headers };
  assert.deepEqual(fieldValues(handle(request, APP, page('a')), 'cache-control'), ['no-cache, private']);
  assert.deepEqual(fieldValues(handle(request, APP, page('a'), {}), 'cache-control'), ['no-cache, private']);
  assert.deepEqual(fieldValues(handle(request, APP, page('a'), null), 'cache-control'), ['no-cache, private']);
  assert.deepEqual(fieldValues(handle(request, APP, page('a'), { profile: PHP }), 'cache-control'), ['no-cache']);
  assert.deepEqual(fieldValues(handle(request, APP, page('a'), { profile: NODE }), 'cache-control'), []);
});

test('etag_is_the_sha1_of_the_body_bytes', () => {
  assert.equal(sha1hex(page('a')), etagOf('a'));
  assert.equal(sha1hex('abc'), 'a9993e364706816aba3e25717850c26c9cd0d89d');
});
