// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors
//
// The abstract cache of the canonical client (CL-38, src/client-canonical.mjs CanonicalStore)
// and the canonical field functions, called directly: the key of a record (CL-12, CL-26), the
// integrity check (CL-40), the Vary recomputation of a 304 (CL-17), the account change (CL-06)
// and the cache_before expansion (README "cache_before entry").

import assert from 'node:assert/strict';
import { test } from 'node:test';
import { CanonicalStore, canonicalDiff, canonicalMessage, charsetOf, etagFieldName, varyNames } from '../src/client-canonical.mjs';
import { ClientCaseError } from '../src/client-case.mjs';
import { run } from '../src/client-model.mjs';
import { ENV, pageOf } from './client-helpers.mjs';

const URL_A = 'https://shop.example.com/catalog';
const URL_B = 'https://shop.example.com/other';

/** A store of a canonical sim with only what `seed` reads. */
function store({ account = 'p' } = {}) {
  const sim = {
    url: URL_A,
    b: { server_profile: 'legacy-canonical' },
    config: { account, authenticated_cache: false },
    pageBytes: (id) => {
      const bytes = pageOf(id);
      if (bytes === null) throw new Error(`no page ${id}`);
      return Buffer.from(bytes);
    },
  };
  return new CanonicalStore(sim);
}

/** Seed records (cache_before entries) and return the store. */
function seeded(...entries) {
  const s = store();
  for (const e of entries) s.seed({ page: 'a', ...e });
  return s;
}
const keyOf = (r) => `${r.partition} ${r.url} ${JSON.stringify(r.vary)}`;

test('cl12_a_record_is_found_by_its_partition_url_and_vary_values', () => {
  const s = seeded({}, { partition: 'q' }, { url: URL_B }, { url: 'https://shop.example.com/v', vary: [['x-a', '1']] });
  assert.equal(keyOf(s.find('p', URL_A, [])), `p ${URL_A} []`);
  assert.equal(keyOf(s.find('q', URL_A, [])), `q ${URL_A} []`);
  assert.equal(keyOf(s.find('p', URL_B, [])), `p ${URL_B} []`);
  assert.equal(s.find('r', URL_A, []), null);
  assert.equal(s.find('q', URL_B, []), null);
  assert.equal(keyOf(s.find('p', 'https://shop.example.com/v', [['X-A', '1']])), 'p https://shop.example.com/v [["x-a","1"]]');
  assert.equal(s.find('p', 'https://shop.example.com/v', [['x-a', '2']]), null);
  assert.equal(s.find('p', 'https://shop.example.com/v', []), null);
});

test('cl12_a_cached_record_is_visible_when_its_vary_values_equal_the_values_that_the_client_sends', () => {
  // The client sends accept-encoding gzip and, when it is not empty, the cookie (CL-09, CL-11).
  const cookie = 'session=rq-test-cookie-1';
  const visible = (vary, configCookie) => {
    const c = { mode: 'quick', server_profile: 'legacy-canonical', config: { cookie: configCookie }, responses: { a: { page: 'a' } }, cache_before: [{ page: 'a', vary }], schedule: [{ case: 'a' }, { do: 'create_session' }] };
    const r = run('legacy-canonical', 'quick', c, ENV);
    assert.equal(r.ok, true, r.error?.message);
    return r.client.phases[0].requests[0].fields.some(([n]) => n === 'if-none-match');
  };
  const cases = [
    [[['accept-encoding', 'gzip']], null, true],
    [[['accept-encoding', null]], null, false],
    [[['cookie', null]], null, true],
    [[['cookie', null]], '', true],
    [[['cookie', null]], cookie, false],
    [[['cookie', cookie]], cookie, true],
    [[['cookie', cookie]], null, false],
    [[['cookie', '']], '', false],
  ];
  for (const [vary, configCookie, want] of cases) assert.equal(visible(vary, configCookie), want, `${JSON.stringify(vary)} ${configCookie}`);
});

test('cl40_a_record_that_fails_integrity_is_removed_when_it_is_found', () => {
  for (const change of [{ etag: '0'.repeat(40) }, { template_tag: '0'.repeat(40) }]) {
    const s = seeded(change);
    assert.equal(s.find('p', URL_A, []), null, JSON.stringify(change));
    assert.deepEqual(s.snapshot(), [], JSON.stringify(change));
  }
  const noTemplate = seeded({});
  noTemplate.records[0].template = null;
  assert.equal(noTemplate.find('p', URL_A, []), null);
  assert.deepEqual(noTemplate.snapshot(), []);
});

test('cl26_a_commit_replaces_only_the_record_of_its_key', () => {
  const s = seeded({}, { partition: 'q' }, { url: URL_B }, { vary: [['x-a', '1']] });
  const replacement = { ...s.records[0], fields: [['x-new', '1']] };
  s.commit(replacement);
  assert.deepEqual(s.records.map(keyOf), [`q ${URL_A} []`, `p ${URL_B} []`, `p ${URL_A} [["x-a","1"]]`, `p ${URL_A} []`]);
  assert.equal(s.records.at(-1), replacement);
});

test('cl17_rekey_gives_the_record_its_new_vary_values_and_replaces_only_that_key', () => {
  const s = seeded({}, { vary: [['x-a', '1']] }, { partition: 'q', vary: [['x-a', '1']] }, { url: URL_B, vary: [['x-a', '1']] });
  const record = s.records[0];
  s.rekey(record, [['x-a', '1']]);
  assert.deepEqual(s.records.map(keyOf), [`p ${URL_A} [["x-a","1"]]`, `q ${URL_A} [["x-a","1"]]`, `p ${URL_B} [["x-a","1"]]`]);
  assert.equal(s.records[0], record);
});

test('cl06_remove_partition_removes_the_records_of_that_partition_only', () => {
  const s = seeded({}, { partition: 'q' }, { url: URL_B });
  s.removePartition('p');
  assert.deepEqual(s.records.map(keyOf), [`q ${URL_A} []`]);
});

test('a_cache_before_entry_takes_its_overrides_with_lowercase_sorted_names', () => {
  const s = seeded({ url: URL_B, vary: [['X-B', '2'], ['x-a', null]], fields: [['X-F', '1'], ['a-f', '2']] });
  const [r] = s.snapshot();
  assert.equal(r.url, URL_B);
  assert.deepEqual(r.vary, [['x-a', null], ['x-b', '2']]);
  assert.deepEqual(r.fields, [['a-f', '2'], ['x-f', '1']]);
  assert.equal(seeded({ partition: null }).snapshot()[0].partition, null);
});

test('a_cache_before_page_that_commits_no_record_is_a_case_error', () => {
  for (const page of ['badutf8', 'dup']) {
    assert.throws(() => store().seed({ page }), (e) => e instanceof ClientCaseError && e.code === 'CASE_CACHE_BEFORE' && e.message.includes(`page ${page}`), page);
  }
});

// --- field functions ---

test('cl14_the_etag_field_name_is_a_non_empty_sonic_etag_key_else_etag', () => {
  assert.equal(etagFieldName([]), 'etag');
  assert.equal(etagFieldName([['sonic-etag-key', '']]), 'etag');
  assert.equal(etagFieldName([['Sonic-Etag-Key', 'X-My-Etag']]), 'x-my-etag');
});

test('cl16_a_missing_content_type_has_the_charset_utf8', () => {
  assert.equal(charsetOf(undefined), 'utf-8');
  assert.equal(charsetOf('text/html; charset=GBK'), 'gbk');
});

test('cl12_vary_names_skip_empty_items_and_are_sorted', () => {
  assert.deepEqual(varyNames([['vary', 'X-B, ,a,,Accept-Diff'], ['vary', 'template-tag']]), { star: false, names: ['a', 'x-b'] });
  assert.equal(varyNames([['vary', 'a, *']]).star, true);
});

test('cl42_the_diff_is_null_when_a_stored_or_server_value_is_not_a_string', () => {
  assert.deepEqual(canonicalDiff(null, [['k', 'v']]), [['k', 'v']]);
  assert.equal(canonicalDiff([['k', 1]], [['k', 'v']]), null);
  assert.equal(canonicalDiff([], [['k', 1]]), null);
});

test('cl24_the_message_has_a_result_only_for_a_final_200_with_a_non_empty_diff', () => {
  const headers = [['etag', 'e']];
  const message = (final, diff) => JSON.parse(canonicalMessage({ src: 200, final, diff, headers, reload: false }));
  assert.equal(message(200, [['k', 'v']]).result, '{"k":"v"}');
  for (const [final, diff] of [[200, []], [200, null], [304, [['k', 'v']]], [1000, [['k', 'v']]]]) {
    assert.equal(Object.hasOwn(message(final, diff), 'result'), false, `${final} ${JSON.stringify(diff)}`);
  }
  // An empty sonic-etag-key gives the default key name.
  assert.deepEqual(JSON.parse(canonicalMessage({ src: 200, final: 304, diff: null, headers: [['sonic-etag-key', ''], ['etag', 'e']], reload: false })).extra, { eTag: 'e', isReload: 'false' });
});
