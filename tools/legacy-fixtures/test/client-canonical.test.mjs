// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors
//
// The canonical client model: one test for each rule CL-nn that it implements
// (docs/engineering/protocol/legacy-client.md section 9), on small synthetic cases.

import assert from 'node:assert/strict';
import { test } from 'node:test';
import { sha256hex } from '../src/client-engine.mjs';
import { run } from '../src/client-model.mjs';
import { sha1hex } from '../src/model.mjs';
import { pageOf } from './client-helpers.mjs';

const URL_ = 'https://shop.example.com/catalog';
const EXTRA = { empty: Buffer.alloc(0), gbk: Buffer.from('<html><head><title>T</title></head><body><!--sonicdiff-a-->A<!--sonicdiff-a-end--></body></html>') };
const ENV = { page: (id) => EXTRA[id] ?? pageOf(id), fixture: () => null };

const FIRST = [{ do: 'create_session' }, { do: 'bind_client' }, { do: 'client_ready' }, { do: 'request_resource' }, { do: 'settle', ms: 100 }, { do: 'destroy' }];

function cc({ mode = 'quick', responses = { a: { page: 'a' }, b: { page: 'b' }, c: { page: 'c' } }, schedule, config, cache_before = [], url } = {}) {
  return { mode, server_profile: 'legacy-canonical', responses, cache_before, schedule, ...(config ? { config } : {}), ...(url ? { url } : {}) };
}

function go(c, profile = 'legacy-canonical') {
  const r = run(profile, c.mode, c, ENV);
  assert.equal(r.ok, true, r.ok ? '' : `${r.error.code}: ${r.error.message}`);
  return r.client;
}

const all = (client, key) => client.phases.flatMap((p) => p[key]);
const actions = (client, session) => all(client, 'actions').filter((a) => session === undefined || a.session === session);
const results = (client) => all(client, 'results');
const lastSnapshot = (client) => all(client, 'snapshots').at(-1).entries;
const requests = (client) => all(client, 'requests');

/** A first load of page a, then the given steps (the second session sees the committed entry). */
const afterFirstLoad = (next, steps) => [{ case: 'a' }, ...FIRST, { case: next }, ...steps];

const SECOND = (mode) => (mode === 'quick'
  ? [{ do: 'create_session' }, { do: 'bind_client' }, { do: 'client_ready' }, { do: 'request_resource' }, { do: 'get_diff' }, { do: 'settle', ms: 100 }, { do: 'snapshot_cache' }, { do: 'destroy' }]
  : [{ do: 'create_session' }, { do: 'bind_client' }, { do: 'client_ready' }, { do: 'request_resource' }, { do: 'get_diff' }, { do: 'settle', ms: 100 }, { do: 'snapshot_cache' }, { do: 'destroy' }]);

/** The cached page is delivered before the response head: hold the head of the second request. */
const CACHED_FIRST = [
  { do: 'hold', point: 'head' }, { do: 'create_session' }, { do: 'bind_client' }, { do: 'client_ready' }, { do: 'request_resource' },
  { do: 'get_diff' }, { do: 'release' }, { do: 'settle', ms: 100 }, { do: 'get_diff' }, { do: 'snapshot_cache' }, { do: 'destroy' },
];

test('cl03_accept_diff_off_sends_false_and_gets_a_normal_load', () => {
  const c = go(cc({ config: { accept_diff: false }, schedule: [{ case: 'a' }, ...FIRST, { do: 'snapshot_cache' }] }));
  assert.deepEqual(requests(c)[0].fields, [['accept-diff', 'false']]);
  assert.deepEqual(results(c), [{ session: 1, src_code: -1, final_code: -1 }]);
  assert.deepEqual(lastSnapshot(c), []);
});

test('cl05_null_account_is_its_own_partition', () => {
  const c = go(cc({ config: { account: null }, schedule: [{ case: 'a' }, ...FIRST, { do: 'snapshot_cache' }] }));
  assert.equal(c.sessions[0].partition, null);
  assert.equal(lastSnapshot(c)[0].partition, null);
});

test('cl06_account_change_stops_sessions_and_removes_old_entries', () => {
  const c = go(cc({
    schedule: [{ case: 'a' }, ...FIRST, { do: 'hold', point: 'head' }, { do: 'create_session' }, { do: 'bind_client' }, { do: 'client_ready' }, { do: 'request_resource' },
      { do: 'get_diff' }, { do: 'set_account', value: 'rq-test-account-2' }, { do: 'release' }, { do: 'settle', ms: 10 }, { do: 'snapshot_cache' }, { do: 'destroy' }],
  }));
  assert.deepEqual(lastSnapshot(c), []);
  assert.deepEqual(actions(c, 2).filter((a) => a.do === 'message'), []);
  assert.deepEqual(results(c).at(-1), { session: 2, src_code: -1, final_code: -1 });
});

test('cl07_second_session_for_an_open_key_does_not_open', () => {
  const c = go(cc({ schedule: [{ case: 'a' }, { do: 'create_session' }, { do: 'create_session' }, { do: 'set_account', value: 'rq-test-account-2' }, { do: 'create_session' }] }));
  assert.deepEqual(c.sessions.map((s) => s.opened), [true, false, true]);
  assert.equal(requests(c).length, 2);
});

test('cl08_cl19_http_disables_the_key_for_its_period', () => {
  const app = { status: 200, headers: [['content-type', 'text/html; charset=utf-8'], ['cache-offline', 'http']] };
  const c = go(cc({
    config: { unavailable_time_ms: 1000 },
    responses: { h: { page: 'a', app }, a: { page: 'a' } },
    schedule: [{ case: 'h' }, ...FIRST, { do: 'create_session' }, { do: 'settle', ms: 1000 }, { case: 'a' }, { do: 'create_session' }],
  }));
  assert.deepEqual(c.sessions.map((s) => s.opened), [true, false, true]);
  assert.equal(requests(c).length, 2);
  assert.deepEqual(results(c)[0], { session: 1, src_code: -1, final_code: -1 });
});

test('cl09_request_has_validators_only_with_a_visible_entry_and_a_cookie', () => {
  // authenticated_cache: the first load with a cookie commits (CL-25), so the second request has an entry.
  const c = go(cc({ config: { cookie: 'session=rq-test-cookie-1', authenticated_cache: true }, schedule: afterFirstLoad('a', [{ do: 'create_session' }]) }));
  const [first, second] = requests(c);
  assert.deepEqual(first.fields, [['accept-diff', 'true'], ['cookie', 'session=rq-test-cookie-1']]);
  assert.deepEqual(second.fields.map(([n]) => n), ['accept-diff', 'if-none-match', 'template-tag', 'cookie']);
  assert.equal(second.fields[1][1], sha1hex(pageOf('a')));
});

test('cl10_no_network_sends_nothing_and_keeps_the_cached_page', () => {
  const c = go(cc({ schedule: afterFirstLoad('b', [{ do: 'set_network', valid: false }, ...SECOND('quick')]) }));
  assert.equal(requests(c).length, 1);
  assert.equal(all(c, 'cookies').length, 1);
  assert.equal(actions(c, 2)[0].do, 'load_data');
  assert.deepEqual(results(c).at(-1), { session: 2, src_code: -1, final_code: -1 });
  assert.equal(lastSnapshot(c).length, 1);
});

test('cl10_no_network_without_entry_is_a_normal_load', () => {
  const c = go(cc({ config: { network_valid: false }, schedule: [{ case: 'a' }, ...FIRST] }));
  assert.deepEqual(actions(c).map((a) => a.do), ['load_url']);
  assert.deepEqual(all(c, 'intercepts'), [{ session: 1, null: true }]);
});

test('cl11_cookie_set_for_each_response_and_never_stored', () => {
  const app = { status: 200, headers: [['content-type', 'text/html; charset=utf-8'], ['set-cookie', 'session=rq-test-cookie-1; Path=/']] };
  const c = go(cc({ config: { authenticated_cache: true }, responses: { a: { page: 'a', app } }, schedule: [{ case: 'a' }, ...FIRST, ...SECOND('quick')] }));
  const sets = all(c, 'cookies').filter((x) => x.call === 'set');
  assert.equal(sets.length, 2);
  assert.deepEqual(sets[0].values, ['session=rq-test-cookie-1; Path=/']);
  assert.ok(lastSnapshot(c)[0].fields.every(([n]) => n !== 'set-cookie'));
  assert.ok(actions(c).filter((a) => a.do === 'load_data').every((a) => a.fields.every(([n]) => n !== 'set-cookie')));
});

test('cl11_dl121_cookie_get_also_when_the_connection_is_then_refused', () => {
  const c = go(cc({ config: { cookie: 'session=rq-test-cookie-1' }, schedule: [{ case: 'a' }, { do: 'server_down' }, ...FIRST] }));
  assert.equal(requests(c).length, 0);
  assert.deepEqual(all(c, 'cookies'), [{ call: 'get', url: URL_, value: 'session=rq-test-cookie-1' }]);
});

test('cl11_cl35_dl121_old_generation_response_sets_its_cookie_but_not_the_document_codes_or_cache', () => {
  const app = (value) => ({ status: 200, headers: [['content-type', 'text/html; charset=utf-8'], ['set-cookie', value]] });
  const responses = { a: { page: 'a', app: app('gen=0') }, c: { page: 'c', app: app('gen=1') } };
  // Exchange 1 (page a) is held at the head; the refresh sends exchange 2 (page c), which ends at once; then exchange 1 arrives.
  const schedule = [
    { case: 'a' }, { do: 'hold', point: 'head' }, { do: 'create_session' },
    { case: 'c' }, { do: 'refresh' }, { do: 'release' },
    { do: 'bind_client' }, { do: 'client_ready' }, { do: 'get_diff' }, { do: 'snapshot_cache' }, { do: 'destroy' },
  ];
  const c = go(cc({ config: { authenticated_cache: true }, responses, schedule }));
  assert.deepEqual(all(c, 'cookies').filter((x) => x.call === 'set').map((x) => x.values).sort(), [['gen=0'], ['gen=1']]);
  const c256 = sha256hex(pageOf('c'));
  assert.equal(actions(c).find((a) => a.do === 'load_data').body_sha256, c256);
  assert.deepEqual(lastSnapshot(c).map((r) => r.page_sha256), [c256]);
  assert.equal(results(c).length, 1);
  assert.equal(results(c)[0].src_code, 1000);
});

test('cl11_no_cookie_call_after_destroy', () => {
  const c = go(cc({ schedule: [{ case: 'a' }, { do: 'hold', point: 'request' }, { do: 'create_session' }, { do: 'destroy' }, { do: 'release' }] }));
  assert.deepEqual(all(c, 'cookies').map((x) => x.call), ['get']);
});

test('cl12_vary_values_are_part_of_the_key', () => {
  const app = { status: 200, headers: [['content-type', 'text/html; charset=utf-8'], ['vary', 'Cookie, Template-Tag']] };
  const c = go(cc({
    config: { cookie: 'session=rq-test-cookie-1', authenticated_cache: true },
    responses: { a: { page: 'a', app } },
    schedule: [{ case: 'a' }, ...FIRST, { do: 'set_cookie', value: 'session=rq-test-cookie-2' }, { do: 'create_session' }, { do: 'snapshot_cache' }],
  }));
  assert.deepEqual(lastSnapshot(c)[0].vary, [['cookie', 'session=rq-test-cookie-1']]);
  assert.deepEqual(requests(c)[1].fields.map(([n]) => n), ['accept-diff', 'cookie']);
});

test('cl12_vary_star_gives_no_commit', () => {
  const app = { status: 200, headers: [['content-type', 'text/html; charset=utf-8'], ['vary', '*']] };
  const c = go(cc({ responses: { a: { page: 'a', app } }, schedule: [{ case: 'a' }, ...FIRST, { do: 'snapshot_cache' }] }));
  assert.deepEqual(lastSnapshot(c), []);
});

test('cl14_absent_cache_offline_is_a_normal_load_and_removes_the_entry', () => {
  const c = go(cc({ schedule: afterFirstLoad('b', [{ do: 'drop_field', name: 'cache-offline' }, ...SECOND('quick')]) }));
  assert.deepEqual(actions(c, 2).map((a) => a.do), ['load_url']);
  assert.deepEqual(lastSnapshot(c), []);
  assert.deepEqual(results(c).at(-1), { session: 2, src_code: -1, final_code: -1 });
});

test('cl14_two_different_values_on_two_lines_is_not_a_legacy_response', () => {
  // legacy-java sends its own cache-offline true and the application line (D-14), as in CS-08.
  const app = { status: 200, headers: [['content-type', 'text/html; charset=utf-8'], ['cache-offline', 'http']] };
  const c = go({ ...cc({ responses: { a: { page: 'a' }, h: { page: 'b', app } }, schedule: afterFirstLoad('h', SECOND('quick')) }), server_profile: 'legacy-java' });
  assert.deepEqual(results(c).at(-1), { session: 2, src_code: -1, final_code: -1 });
  assert.deepEqual(lastSnapshot(c), []);
});

test('cl15_weak_etag_equal_to_the_request_is_not_modified', () => {
  const c = go(cc({ schedule: afterFirstLoad('a', [{ do: 'set_field', name: 'etag', value: `W/"${sha1hex(pageOf('a'))}"` }, ...SECOND('quick')]) }));
  assert.deepEqual(results(c).at(-1), { session: 2, src_code: 304, final_code: 304 });
});

test('cl15_template_change_zero_is_a_data_update_and_upper_true_commits', () => {
  const c = go(cc({ schedule: afterFirstLoad('b', [{ do: 'set_field', name: 'template-change', value: '0' }, { do: 'set_field', name: 'cache-offline', value: 'TRUE' }, ...SECOND('quick')]) }));
  assert.deepEqual(results(c).at(-1), { session: 2, src_code: 200, final_code: 304 });
  assert.equal(lastSnapshot(c)[0].etag, sha1hex(pageOf('b')));
});

test('cl15_other_cache_offline_value_counts_as_false', () => {
  const c = go(cc({ schedule: afterFirstLoad('b', [{ do: 'set_field', name: 'cache-offline', value: 'maybe' }, ...SECOND('quick')]) }));
  assert.equal(actions(c, 2)[0].body_sha256 !== undefined, true);
  assert.deepEqual(lastSnapshot(c), []);
});

test('cl16_a_charset_other_than_utf8_is_a_normal_load', () => {
  const app = { status: 200, headers: [['content-type', 'text/html; charset=gbk']] };
  const c = go(cc({ responses: { g: { page: 'gbk', app } }, schedule: [{ case: 'g' }, ...FIRST, { do: 'snapshot_cache' }] }));
  assert.deepEqual(lastSnapshot(c), []);
  assert.deepEqual(results(c), [{ session: 1, src_code: -1, final_code: -1 }]);
});

test('cl17_not_modified_updates_the_stored_page_fields', () => {
  const app = { status: 200, headers: [['content-type', 'text/html; charset=utf-8'], ['content-security-policy', "default-src 'self'"]] };
  const c = go(cc({ responses: { a: { page: 'a' }, s: { page: 'a', app } }, schedule: afterFirstLoad('s', [{ do: 'set_field', name: 'x-new', value: '1' }, ...SECOND('quick')]) }));
  assert.equal(results(c).at(-1).final_code, 304);
  assert.deepEqual(lastSnapshot(c)[0].fields.map(([n]) => n), ['content-security-policy', 'content-type', 'x-new']);
});

test('cl18_error_with_a_visible_entry_keeps_page_and_entry', () => {
  const c = go(cc({ responses: { a: { page: 'a' }, e: { page: 'b', app: { status: 500, headers: [] } } }, schedule: afterFirstLoad('e', SECOND('standard')), mode: 'standard' }));
  assert.equal(all(c, 'intercepts').at(-1).body_sha256 !== undefined, true);
  assert.equal(lastSnapshot(c).length, 1);
  assert.deepEqual(results(c).at(-1), { session: 2, src_code: -1, final_code: -1 });
});

test('cl18_refused_connection_without_entry_is_a_normal_load', () => {
  const c = go(cc({ schedule: [{ case: 'a' }, { do: 'server_down' }, ...FIRST] }));
  assert.equal(requests(c).length, 0);
  assert.deepEqual(all(c, 'intercepts'), [{ session: 1, null: true }]);
});

test('cl19_http_after_the_cached_page_calls_load_url_and_removes_the_entry', () => {
  const c = go(cc({ schedule: afterFirstLoad('b', [{ do: 'set_field', name: 'cache-offline', value: 'http' }, ...CACHED_FIRST]) }));
  assert.deepEqual(actions(c, 2).map((a) => a.do), ['load_data', 'load_url']);
  assert.deepEqual(lastSnapshot(c), []);
});

test('cl20_quick_gives_a_complete_document_as_data_else_load_url', () => {
  const c = go(cc({ schedule: [{ case: 'a' }, { do: 'hold', point: 'head' }, { do: 'create_session' }, { do: 'bind_client' }, { do: 'client_ready' }, { do: 'request_resource' }, { do: 'release' }] }));
  assert.deepEqual(actions(c).map((a) => a.do), ['load_url']);
  assert.equal(all(c, 'intercepts')[0].complete, true);
  const d = go(cc({ schedule: [{ case: 'a' }, ...FIRST] }));
  assert.deepEqual(actions(d).map((a) => a.do), ['load_data']);
  assert.deepEqual(all(d, 'intercepts'), [{ session: 1, null: true }]);
});

test('cl20_standard_answers_one_intercept_with_the_document', () => {
  const c = go(cc({ mode: 'standard', schedule: [{ case: 'a' }, ...FIRST.slice(0, 4), { do: 'request_resource' }] }));
  assert.deepEqual(actions(c).map((a) => a.do), ['load_url']);
  const [one, two] = all(c, 'intercepts').sort((a) => (a.null ? 1 : -1));
  assert.equal(one.status, 200);
  assert.deepEqual(two, { session: 1, null: true });
});

test('cl21_page_fields_keep_csp_and_drop_transport_and_legacy_fields', () => {
  const app = { status: 200, headers: [['content-type', 'text/html; charset=utf-8'], ['content-security-policy', "default-src 'self'"], ['cache-control', 'max-age=60'], ['set-cookie', 'a=b'], ['connection', 'x-hop'], ['x-hop', '1']] };
  const c = go(cc({ responses: { a: { page: 'a', app } }, schedule: [{ case: 'a' }, ...FIRST] }));
  assert.deepEqual(actions(c)[0].fields, [['content-security-policy', "default-src 'self'"], ['content-type', 'text/html; charset=utf-8']]);
});

test('cl22_first_load_codes_depend_on_the_first_delivery', () => {
  const body = go(cc({ mode: 'standard', schedule: [{ case: 'a' }, ...FIRST] }));
  assert.deepEqual(results(body), [{ session: 1, src_code: 1000, final_code: 304 }]);
  const stream = go(cc({ mode: 'standard', schedule: [{ case: 'a' }, { do: 'hold', point: 'head' }, ...FIRST.slice(0, 4), { do: 'release' }, { do: 'destroy' }] }));
  assert.deepEqual(results(stream), [{ session: 1, src_code: 1000, final_code: 1000 }]);
});

test('cl23_message_after_get_diff_and_again_at_the_next_call', () => {
  const c = go(cc({ schedule: [{ case: 'a' }, { do: 'create_session' }, { do: 'get_diff' }, { do: 'get_diff' }] }));
  const msgs = actions(c).filter((a) => a.do === 'message');
  assert.equal(msgs.length, 2);
  assert.equal(msgs[0].json, msgs[1].json);
});

test('cl23_no_message_for_final_minus_one', () => {
  const c = go(cc({ schedule: [{ case: 'a' }, { do: 'server_down' }, { do: 'create_session' }, { do: 'get_diff' }] }));
  assert.deepEqual(actions(c), []);
});

test('cl24_message_text_uses_the_sonic_etag_key_name', () => {
  const c = go(cc({ schedule: [{ case: 'a' }, { do: 'set_field', name: 'sonic-etag-key', value: 'x-etag' }, { do: 'set_field', name: 'x-etag', value: sha1hex(pageOf('a')) }, { do: 'create_session' }, { do: 'get_diff' }] }));
  const json = JSON.parse(actions(c)[0].json);
  assert.deepEqual(Object.keys(json), ['code', 'srcCode', 'extra']);
  assert.deepEqual(Object.keys(json.extra), ['x-etag', 'template-tag', 'cache-offline', 'isReload']);
});

test('cl25_cache_offline_false_gives_no_commit', () => {
  const c = go(cc({ schedule: [{ case: 'a' }, { do: 'set_field', name: 'cache-offline', value: 'false' }, ...FIRST, { do: 'snapshot_cache' }] }));
  assert.deepEqual(lastSnapshot(c), []);
  assert.equal(results(c)[0].src_code, 1000);
});

test('cl25_etag_that_is_not_the_page_sha1_gives_no_commit', () => {
  const c = go(cc({ schedule: [{ case: 'a' }, { do: 'set_field', name: 'etag', value: 'abc' }, ...FIRST, { do: 'snapshot_cache' }] }));
  assert.deepEqual(lastSnapshot(c), []);
});

test('cl26_cl38_commit_survives_destroy_and_has_the_abstract_record', () => {
  const c = go(cc({ schedule: [{ case: 'a' }, { do: 'create_session' }, { do: 'destroy' }, { do: 'snapshot_cache' }] }));
  const [r] = lastSnapshot(c);
  assert.deepEqual(Object.keys(r), ['partition', 'url', 'vary', 'etag', 'template_tag', 'page_sha256', 'template_sha256', 'data_sha256', 'fields']);
  assert.equal(r.url, URL_);
  assert.equal(r.etag, sha1hex(pageOf('a')));
});

test('cl27_diff_has_only_the_changed_blocks_in_server_order', () => {
  const c = go(cc({ schedule: afterFirstLoad('b', CACHED_FIRST) }));
  const msg = JSON.parse(actions(c, 2).find((a) => a.do === 'message').json);
  assert.deepEqual(Object.keys(JSON.parse(msg.result)), ['{price}']);
  assert.deepEqual(results(c).at(-1), { session: 2, src_code: 200, final_code: 200 });
});

test('cl27_cache_offline_false_delivers_then_removes_the_entry', () => {
  const c = go(cc({ schedule: afterFirstLoad('b', [{ do: 'set_field', name: 'cache-offline', value: 'false' }, ...SECOND('quick')]) }));
  assert.equal(actions(c, 2)[0].body_sha256, sha256Of('b'));
  assert.deepEqual(lastSnapshot(c), []);
});

function sha256Of(id) {
  return createHashHex(pageOf(id));
}
import { createHash } from 'node:crypto';
function createHashHex(b) {
  return createHash('sha256').update(b).digest('hex');
}

test('cl28_dl130_old_diff_that_the_page_never_got_gives_a_normal_load_and_final_minus_one', () => {
  const steps = [{ do: 'hold', point: 'head' }, { do: 'create_session' }, { do: 'bind_client' }, { do: 'client_ready' }, { do: 'release' }, { do: 'settle', ms: 30_001 },
    { do: 'get_diff' }, { do: 'request_resource' }, { do: 'get_diff' }, { do: 'snapshot_cache' }, { do: 'destroy' }];
  const c = go(cc({ schedule: afterFirstLoad('b', steps) }));
  assert.deepEqual(actions(c, 2).map((a) => a.do), ['load_data', 'load_url']); // the cached page, then the reload
  assert.deepEqual(all(c, 'intercepts').filter((i) => i.session === 2), [{ session: 2, null: true }]);
  assert.deepEqual(results(c).at(-1), { session: 2, src_code: 200, final_code: -1 });
  assert.equal(lastSnapshot(c)[0].etag, sha1hex(pageOf('b'))); // the entry stays: the cache has the new data
});

test('cl28_old_diff_that_the_page_already_got_is_not_sent_again_and_gives_no_reload', () => {
  const steps = [{ do: 'hold', point: 'head' }, { do: 'create_session' }, { do: 'bind_client' }, { do: 'client_ready' }, { do: 'get_diff' }, { do: 'release' },
    { do: 'settle', ms: 30_001 }, { do: 'get_diff' }, { do: 'destroy' }];
  const c = go(cc({ schedule: afterFirstLoad('b', steps) }));
  assert.deepEqual(actions(c, 2).map((a) => a.do), ['load_data', 'message']);
  assert.deepEqual(results(c).at(-1), { session: 2, src_code: 200, final_code: 200 });
});

test('cl29_rebuild_check_failure_is_a_normal_load', () => {
  const c = go(cc({ schedule: afterFirstLoad('b', [{ do: 'set_field', name: 'etag', value: sha1hex('other') }, ...CACHED_FIRST]) }));
  assert.deepEqual(actions(c, 2).map((a) => a.do), ['load_data', 'load_url']);
  assert.deepEqual(lastSnapshot(c), []);
  assert.deepEqual(results(c).at(-1), { session: 2, src_code: -1, final_code: -1 });
});

test('cl30_store_with_the_cached_page_delivered_commits_and_gives_304', () => {
  const c = go(cc({ schedule: afterFirstLoad('b', [{ do: 'set_field', name: 'cache-offline', value: 'store' }, ...CACHED_FIRST]) }));
  assert.deepEqual(results(c).at(-1), { session: 2, src_code: 200, final_code: 304 });
  assert.ok(!JSON.parse(actions(c, 2).find((a) => a.do === 'message').json).result);
  assert.equal(lastSnapshot(c)[0].etag, sha1hex(pageOf('b')));
});

test('cl31_template_change_reload_quick_data_standard_load_url', () => {
  const q = go(cc({ responses: { a: { page: 'a' }, c: { page: 'c' } }, schedule: afterFirstLoad('c', CACHED_FIRST) }));
  assert.deepEqual(actions(q, 2).filter((a) => a.do !== 'message').map((a) => a.do), ['load_data', 'load_data']);
  assert.deepEqual(results(q).at(-1), { session: 2, src_code: 2000, final_code: 2000 });
  const s = go(cc({ mode: 'standard', responses: { a: { page: 'a' }, c: { page: 'c' } }, schedule: afterFirstLoad('c', [...CACHED_FIRST.slice(0, 7), { do: 'request_resource' }, ...CACHED_FIRST.slice(7)]) }));
  assert.deepEqual(actions(s, 2).filter((a) => a.do !== 'message').map((a) => a.do), ['load_url', 'load_url']);
  assert.equal(all(s, 'intercepts').filter((i) => i.session === 2).at(-1).body_sha256, sha256Of('c'));
});

test('cl23_cl31_reload_drops_the_page_callback', () => {
  const q = go(cc({ responses: { a: { page: 'a' }, c: { page: 'c' } }, schedule: afterFirstLoad('c', CACHED_FIRST) }));
  const msgs = q.phases.map((p) => p.actions.filter((a) => a.do === 'message' && a.session === 2).length);
  assert.deepEqual(msgs.slice(-2), [0, 1]); // no message at the result; one at the next get_diff
});

test('cl32_empty_body_is_a_normal_load', () => {
  const c = go(cc({ responses: { e: { page: 'empty' } }, schedule: [{ case: 'e' }, ...FIRST, { do: 'snapshot_cache' }] }));
  assert.deepEqual(actions(c).map((a) => a.do), ['load_url']);
  assert.deepEqual(results(c), [{ session: 1, src_code: -1, final_code: -1 }]);
});

test('cl33_cut_stream_ends_incomplete_then_load_url', () => {
  const c = go(cc({ schedule: [{ case: 'a' }, { do: 'cut', bytes: 50 }, { do: 'hold', point: 'head' }, ...FIRST.slice(0, 4), { do: 'release' }, { do: 'snapshot_cache' }, { do: 'destroy' }] }));
  assert.deepEqual(actions(c).map((a) => a.do), ['load_url', 'load_url']);
  assert.equal(all(c, 'intercepts')[0].complete, false);
  assert.equal(all(c, 'intercepts')[0].body_len, 50);
  assert.deepEqual(lastSnapshot(c), []);
  assert.deepEqual(results(c), [{ session: 1, src_code: -1, final_code: -1 }]);
});

test('cl34_destroy_answers_a_waiting_intercept_with_null', () => {
  const c = go(cc({ mode: 'standard', schedule: [{ case: 'a' }, { do: 'hold', point: 'head' }, ...FIRST.slice(0, 4), { do: 'destroy' }, { do: 'release' }, { do: 'snapshot_cache' }] }));
  assert.deepEqual(all(c, 'intercepts'), [{ session: 1, null: true }]);
  assert.deepEqual(lastSnapshot(c), []);
});

test('cl35_refresh_sends_again_with_a_new_cookie_get_and_is_reload', () => {
  const c = go(cc({ schedule: [{ case: 'a' }, ...FIRST.slice(0, 4), { do: 'refresh' }, { do: 'get_diff' }] }));
  assert.equal(requests(c).length, 2);
  assert.equal(all(c, 'cookies').length, 2);
  const msg = JSON.parse(actions(c).find((a) => a.do === 'message').json);
  assert.deepEqual([msg.code, msg.srcCode, msg.extra.isReload], [304, 304, 'true']);
});

test('cl39_no_store_gives_no_commit', () => {
  const app = { status: 200, headers: [['content-type', 'text/html; charset=utf-8'], ['cache-control', 'no-store']] };
  const c = go(cc({ responses: { a: { page: 'a', app } }, schedule: [{ case: 'a' }, ...FIRST, { do: 'snapshot_cache' }] }));
  assert.deepEqual(lastSnapshot(c), []);
});

test('cl40_an_entry_that_fails_integrity_is_removed_and_not_used', () => {
  const c = go(cc({ cache_before: [{ page: 'a', etag: 'abc' }], schedule: [{ do: 'snapshot_cache' }, { case: 'a' }, { do: 'create_session' }, { do: 'snapshot_cache' }] }));
  assert.equal(c.phases[0].snapshots[0].entries[0].etag, 'abc');
  assert.deepEqual(requests(c)[0].fields, [['accept-diff', 'true']]);
});

test('cache_before_expands_into_the_record_of_a_first_load', () => {
  const c = go(cc({ cache_before: [{ page: 'a', partition: 'rq-test-account-2' }], schedule: [{ do: 'snapshot_cache' }] }));
  const [r] = c.phases[0].snapshots[0].entries;
  assert.equal(r.partition, 'rq-test-account-2');
  assert.equal(r.etag, sha1hex(pageOf('a')));
});

// --- S2 security review: DL-123 to DL-131 ---

const HTML = ['content-type', 'text/html; charset=utf-8'];
const appWith = (status, ...lines) => ({ status, headers: [HTML, ...lines] });
const snapAfterFirst = (config, responses = undefined) => lastSnapshot(go(cc({ config, ...(responses ? { responses } : {}), schedule: [{ case: 'a' }, ...FIRST, { do: 'snapshot_cache' }] })));

test('cl03_cl25_dl123_a_request_cookie_commits_only_with_authenticated_cache_and_a_partition', () => {
  const cookie = 'session=rq-test-cookie-1';
  assert.deepEqual(snapAfterFirst({ cookie }), []);
  assert.equal(snapAfterFirst({ cookie, authenticated_cache: true }).length, 1);
  assert.deepEqual(snapAfterFirst({ cookie, authenticated_cache: true, account: null }), []);
  assert.equal(snapAfterFirst({ cookie: '' }).length, 1); // an empty cookie is not a credential
  assert.equal(snapAfterFirst({ cookie: ' \t' }).length, 1); // nor a value of white space only (the field value is empty)
  assert.equal(snapAfterFirst({}).length, 1); // anonymous: no change
});

test('cl25_dl123_set_cookie_or_set_cookie2_in_the_response_makes_the_exchange_credentialed', () => {
  for (const name of ['Set-Cookie', 'set-cookie2']) {
    const responses = { a: { page: 'a', app: appWith(200, [name, 'k=v']) } };
    const c = go(cc({ responses, schedule: [{ case: 'a' }, ...FIRST, { do: 'snapshot_cache' }] }));
    assert.deepEqual(lastSnapshot(c), [], name);
    assert.deepEqual(actions(c).map((a) => a.do), ['load_data'], name); // the page is still shown
    assert.deepEqual(results(c), [{ session: 1, src_code: 1000, final_code: 304 }], name);
    assert.equal(snapAfterFirst({ authenticated_cache: true }, responses).length, 1, name);
  }
});

test('cl27_dl123_a_credentialed_data_update_is_delivered_then_removes_the_entry', () => {
  const steps = [{ do: 'set_cookie', value: 'session=rq-test-cookie-1' }, ...CACHED_FIRST];
  const c = go(cc({ schedule: afterFirstLoad('b', steps) }));
  assert.deepEqual(results(c).at(-1), { session: 2, src_code: 200, final_code: 200 });
  assert.ok(actions(c, 2).some((a) => a.do === 'message'));
  assert.deepEqual(lastSnapshot(c), []);
  const allowed = go(cc({ config: { authenticated_cache: true }, schedule: afterFirstLoad('b', steps) }));
  assert.equal(lastSnapshot(allowed)[0].etag, sha1hex(pageOf('b')));
});

test('cl31_dl123_dl132_a_credentialed_template_change_is_shown_does_not_commit_and_removes_the_old_entry', () => {
  const steps = [{ do: 'set_cookie', value: 'session=rq-test-cookie-1' }, ...CACHED_FIRST];
  const c = go(cc({ schedule: afterFirstLoad('c', steps) }));
  assert.deepEqual(results(c).at(-1), { session: 2, src_code: 2000, final_code: 2000 });
  assert.deepEqual(lastSnapshot(c), []); // CL-31 (DL-132): no commit, and the old entry with the old template is removed
  const allowed = go(cc({ config: { authenticated_cache: true }, schedule: afterFirstLoad('c', steps) }));
  assert.equal(lastSnapshot(allowed)[0].etag, sha1hex(pageOf('c')));
});

test('cl18_dl124_5xx_408_and_429_keep_the_cached_page_and_the_entry', () => {
  for (const status of [500, 599, 408, 429]) {
    const c = go(cc({ mode: 'standard', responses: { a: { page: 'a' }, e: { page: 'b', app: { status, headers: [] } } }, schedule: afterFirstLoad('e', SECOND('standard')) }));
    assert.equal(all(c, 'intercepts').filter((i) => i.session === 2)[0].body_sha256, sha256Of('a'), String(status));
    assert.equal(lastSnapshot(c).length, 1, String(status));
    assert.deepEqual(results(c).at(-1), { session: 2, src_code: -1, final_code: -1 });
  }
});

test('cl18_dl124_401_403_404_410_after_the_cached_page_was_delivered_load_the_url_and_remove_the_entry', () => {
  for (const status of [401, 403, 404, 410, 499, 407, 409, 428, 430, 204]) {
    const c = go(cc({ responses: { a: { page: 'a' }, e: { page: 'b', app: appWith(status) } }, schedule: afterFirstLoad('e', CACHED_FIRST) }));
    assert.deepEqual(actions(c, 2).map((a) => a.do), ['load_data', 'load_url'], String(status));
    assert.deepEqual(lastSnapshot(c), [], String(status));
    assert.deepEqual(results(c).at(-1), { session: 2, src_code: -1, final_code: -1 });
  }
});

test('cl18_dl124_403_before_the_delivery_is_a_normal_load_without_the_cached_page', () => {
  const c = go(cc({ mode: 'standard', responses: { a: { page: 'a' }, e: { page: 'b', app: appWith(403) } }, schedule: afterFirstLoad('e', SECOND('standard')) }));
  assert.deepEqual(all(c, 'intercepts').filter((i) => i.session === 2), [{ session: 2, null: true }]);
  assert.deepEqual(lastSnapshot(c), []);
});

test('cl14_dl125_a_legacy_200_with_text_plain_is_a_normal_load_and_removes_the_entry', () => {
  const plain = { do: 'set_field', name: 'content-type', value: 'text/plain; charset=utf-8' };
  const c = go(cc({ schedule: afterFirstLoad('b', [plain, ...CACHED_FIRST]) }));
  assert.deepEqual(actions(c, 2).map((a) => a.do), ['load_data', 'load_url']);
  assert.deepEqual(lastSnapshot(c), []);
  const first = go(cc({ schedule: [{ case: 'a' }, plain, ...FIRST, { do: 'snapshot_cache' }] }));
  assert.deepEqual(results(first), [{ session: 1, src_code: -1, final_code: -1 }]);
  assert.deepEqual(lastSnapshot(first), []);
  const upper = go(cc({ schedule: [{ case: 'a' }, { do: 'set_field', name: 'Content-Type', value: 'Text/HTML' }, ...FIRST, { do: 'snapshot_cache' }] }));
  assert.equal(lastSnapshot(upper).length, 1);
});

test('cl17_dl126_no_store_on_a_304_removes_the_entry_and_the_page_stays', () => {
  const c = go(cc({ schedule: afterFirstLoad('a', [{ do: 'set_field', name: 'cache-control', value: 'private, No-Store' }, ...CACHED_FIRST]) }));
  assert.deepEqual(actions(c, 2).map((a) => a.do), ['load_data', 'message', 'message']);
  assert.deepEqual(results(c).at(-1), { session: 2, src_code: 304, final_code: 304 });
  assert.deepEqual(lastSnapshot(c), []);
});

test('cl17_dl126_vary_star_or_a_cache_offline_other_than_true_or_store_on_a_304_removes_the_entry', () => {
  for (const [name, value] of [['vary', '*'], ['cache-offline', 'false'], ['cache-offline', 'maybe']]) {
    const c = go(cc({ schedule: afterFirstLoad('a', [{ do: 'set_field', name, value }, ...CACHED_FIRST]) }));
    assert.deepEqual(results(c).at(-1), { session: 2, src_code: 304, final_code: 304 }, value);
    assert.deepEqual(lastSnapshot(c), [], value);
  }
  for (const value of ['true', 'STORE']) {
    const c = go(cc({ schedule: afterFirstLoad('a', [{ do: 'set_field', name: 'cache-offline', value }, ...CACHED_FIRST]) }));
    assert.equal(lastSnapshot(c).length, 1, value);
  }
});

test('cl17_dl126_http_on_a_304_is_cl19', () => {
  const c = go(cc({ schedule: afterFirstLoad('a', [{ do: 'set_field', name: 'cache-offline', value: 'http' }, ...CACHED_FIRST, { do: 'create_session' }]) }));
  assert.deepEqual(actions(c, 2).map((a) => a.do), ['load_data', 'load_url']);
  assert.deepEqual(results(c).at(-1), { session: 2, src_code: -1, final_code: -1 });
  assert.deepEqual(lastSnapshot(c), []);
  assert.equal(c.sessions.at(-1).opened, false); // CL-08: the key is disabled
});

test('cl17_dl126_a_vary_field_on_a_304_recomputes_the_vary_values_and_replaces_the_entry_of_that_key', () => {
  const cookie = 'session=rq-test-cookie-1';
  const steps = [{ case: 'a' }, { do: 'set_field', name: 'vary', value: 'Cookie' }, ...SECOND('quick')];
  const c = go(cc({ config: { cookie }, cache_before: [{ page: 'a' }, { page: 'a', vary: [['cookie', cookie]] }], schedule: steps }));
  assert.deepEqual(results(c), [{ session: 1, src_code: 304, final_code: 304 }]);
  assert.deepEqual(lastSnapshot(c).map((r) => r.vary), [[['cookie', cookie]]]);
  // Without a Vary field the stored Vary values stay.
  const keep = go(cc({ cache_before: [{ page: 'a', vary: [['x-a', null]] }], schedule: [{ case: 'a' }, { do: 'drop_field', name: 'vary' }, ...SECOND('quick')] }));
  assert.deepEqual(lastSnapshot(keep).map((r) => r.vary), [[['x-a', null]]]);
});

/** The page fields of a first load in both modes: load_data (Quick), the intercept (Standard) and the record. */
function pageFieldNames(faults) {
  const out = [];
  for (const mode of ['quick', 'standard']) {
    const c = go(cc({ mode, schedule: [{ case: 'a' }, ...faults, ...FIRST, { do: 'snapshot_cache' }] }));
    const delivered = mode === 'quick' ? actions(c).find((a) => a.do === 'load_data').fields : all(c, 'intercepts').find((i) => i.fields).fields;
    out.push(delivered.map(([n]) => n), lastSnapshot(c)[0].fields.map(([n]) => n));
  }
  return out;
}

test('cl21_dl129_action_fields_are_not_page_fields_in_any_case_also_as_connection_tokens', () => {
  const set = (name, value) => ({ do: 'set_field', name, value });
  const lists = pageFieldNames([
    set('Clear-Site-Data', '"cache"'), set('REFRESH', '0; url=/login'), set('WWW-Authenticate', 'Basic'), set('authentication-info', 'x'),
    set('connection', 'X-Hop, X-Other'), set('X-Hop', '1'), set('x-other', '2'), set('content-security-policy', "default-src 'self'"),
  ]);
  assert.equal(lists.length, 4);
  for (const names of lists) assert.deepEqual(names, ['content-security-policy', 'content-type']);
});

test('cl21_the_fields_that_the_trace_normalization_removes_are_not_page_fields_either', () => {
  // README "Trace projection": these names are removed on both sides before check 7, so only this test checks CL-21 for them.
  const set = (name, value) => ({ do: 'set_field', name, value });
  const lists = pageFieldNames([
    set('Vary', 'Accept-Language'), set('CDN-Cache-Control', 'max-age=60'), set('surrogate-control', 'max-age=60'),
    set('Content-Encoding', 'identity'), set('content-length', '296'), set('Transfer-Encoding', 'chunked'),
  ]);
  assert.equal(lists.length, 4);
  for (const names of lists) assert.deepEqual(names, ['content-type']);
});

test('cl44_dl131_a_redirect_without_an_entry_is_a_normal_load_and_its_cookie_goes_to_the_store', () => {
  for (const status of [300, 302, 307, 399]) {
    const app = appWith(status, ['location', 'https://login.example.com/'], ['set-cookie', 'r=1']);
    const c = go(cc({ mode: 'standard', responses: { r: { page: 'a', app } }, schedule: [{ case: 'r' }, ...FIRST.slice(0, 4), { do: 'snapshot_cache' }, { do: 'destroy' }] }));
    assert.deepEqual(actions(c).map((a) => a.do), ['load_url'], String(status));
    assert.deepEqual(all(c, 'intercepts'), [{ session: 1, null: true }]);
    assert.deepEqual(all(c, 'cookies').filter((x) => x.call === 'set'), [{ call: 'set', url: URL_, values: ['r=1'] }]);
    assert.deepEqual(lastSnapshot(c), []);
    assert.deepEqual(results(c), [{ session: 1, src_code: -1, final_code: -1 }]);
  }
});

test('cl44_dl131_a_redirect_after_the_cached_page_was_delivered_loads_the_url_and_removes_the_entry', () => {
  const c = go(cc({ responses: { a: { page: 'a' }, r: { page: 'b', app: appWith(302, ['location', '/login']) } }, schedule: afterFirstLoad('r', CACHED_FIRST) }));
  assert.deepEqual(actions(c, 2).map((a) => a.do), ['load_data', 'load_url']);
  assert.deepEqual(lastSnapshot(c), []);
  assert.deepEqual(results(c).at(-1), { session: 2, src_code: -1, final_code: -1 });
});
