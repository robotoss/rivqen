// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors
//
// The stored exchanges of the canonical client fixtures (fixtures/legacy/README.md "Client
// exchanges.json", DL-216): the file of one model run, and the rule that the projection of every
// canonical fixture follows from its stored bytes.

import assert from 'node:assert/strict';
import { readFileSync, readdirSync } from 'node:fs';
import { join } from 'node:path';
import { test } from 'node:test';
import { CLIENT_FLOOR } from '../gen.mjs';
import { sha256hex } from '../src/client-engine.mjs';
import { EXCHANGES, FORMAT, bodyBytes, exchangeFile, explainProjection } from '../src/client-exchanges.mjs';
import { run } from '../src/client-model.mjs';
import { sha1hex } from '../src/model.mjs';
import { PAGES, ROOT, pageOf } from './client-helpers.mjs';

const ENV = { page: pageOf, fixture: () => null };
const FIRST = [{ do: 'create_session' }, { do: 'bind_client' }, { do: 'client_ready' }, { do: 'request_resource' }, { do: 'settle', ms: 100 }, { do: 'destroy' }];

function cc({ mode = 'quick', responses = { a: { page: 'a' }, b: { page: 'b' } }, schedule, cache_before = [] }) {
  return { mode, server_profile: 'legacy-canonical', responses, cache_before, schedule };
}

/** One model run: the file and the projection. */
function fileOf(c) {
  const r = run('legacy-canonical', c.mode, c, ENV);
  assert.equal(r.ok, true, r.ok ? '' : `${r.error.code}: ${r.error.message}`);
  return { file: exchangeFile('FX-LEG-CL-T-Q', r.wire), client: r.client };
}

const firstThenSecond = (steps) => [{ case: 'a' }, ...FIRST, { case: 'b' }, ...steps];

test('exchanges_file_has_the_members_in_the_readme_order', () => {
  const { file } = fileOf(cc({ cache_before: [{ page: 'a' }], schedule: firstThenSecond(FIRST) }));
  assert.deepEqual(Object.keys(file), ['id', 'format', 'cache_before', 'exchanges']);
  assert.equal(file.format, FORMAT);
  assert.deepEqual(Object.keys(file.cache_before[0]), ['partition', 'url', 'vary', 'etag', 'template_tag', 'page', 'template', 'data', 'fields']);
  assert.deepEqual(Object.keys(file.exchanges[0]), ['exchange', 'step', 'session', 'request', 'response', 'stop']);
  assert.deepEqual(Object.keys(file.exchanges[0].request), ['method', 'target', 'fields']);
  assert.deepEqual(Object.keys(file.exchanges[0].response), ['status', 'fields', 'body']);
});

test('exchanges_full_page_is_a_page_reference_and_a_data_body_is_base64', () => {
  const { file } = fileOf(cc({ schedule: firstThenSecond(FIRST) }));
  const [full, data] = file.exchanges;
  const a = pageOf('a');
  assert.deepEqual(full.response.body, { page: 'a', len: a.length, sha256: sha256hex(a) });
  assert.deepEqual(full.request.fields, [['accept-diff', 'true'], ['accept-encoding', 'gzip']]);
  assert.equal(data.exchange, 2);
  assert.equal(data.step, FIRST.length + 1);
  assert.equal(data.session, 2);
  assert.deepEqual(data.request.fields.map(([n]) => n), ['accept-diff', 'if-none-match', 'template-tag', 'accept-encoding']);
  const bytes = Buffer.from(data.response.body.base64, 'base64');
  assert.equal(data.response.body.len, bytes.length);
  assert.equal(data.response.body.page, undefined);
  const json = JSON.parse(bytes.toString('utf8'));
  assert.equal(json['html-sha1'], sha1hex(pageOf('b')));
  assert.deepEqual(file.exchanges.map((e) => e.stop), [null, null]);
});

test('exchanges_304_has_no_body', () => {
  const { file } = fileOf(cc({ responses: { a: { page: 'a' } }, schedule: [{ case: 'a' }, ...FIRST, ...FIRST] }));
  assert.equal(file.exchanges[1].response.status, 304);
  assert.equal(file.exchanges[1].response.body, null);
});

test('exchanges_refused_connection_has_no_number_and_no_response', () => {
  const { file, client } = fileOf(cc({ schedule: [{ case: 'a' }, { do: 'server_down' }, ...FIRST, { do: 'server_up' }, ...FIRST] }));
  assert.deepEqual(file.exchanges.map((e) => e.exchange), [null, 1]);
  assert.deepEqual(file.exchanges[0].response, null);
  assert.deepEqual(file.exchanges[0].stop, { kind: 'refused' });
  assert.equal(file.exchanges[0].step, 2);
  assert.deepEqual(explainProjection(client, file, pageOf).problems, []);
});

test('exchanges_cut_stores_the_cut_point_at_most_the_body_length', () => {
  const cut = (bytes) => fileOf(cc({ mode: 'standard', schedule: [{ case: 'a' }, { do: 'cut', bytes }, ...FIRST] }));
  const short = cut(40);
  assert.deepEqual(short.file.exchanges[0].stop, { kind: 'cut', at: 40 });
  assert.deepEqual(explainProjection(short.client, short.file, pageOf).problems, []);
  assert.deepEqual(cut(1_000_000).file.exchanges[0].stop, { kind: 'cut', at: pageOf('a').length });
});

test('exchanges_store_the_delivered_form_after_the_field_and_body_faults', () => {
  const { file } = fileOf(cc({
    schedule: firstThenSecond([
      { do: 'drop_field', name: 'vary' }, { do: 'set_field', name: 'template-tag', value: 'x' },
      { do: 'set_body_member', name: 'html-sha1', value: '0' }, ...FIRST,
    ]),
  }));
  const r = file.exchanges[1].response;
  assert.deepEqual(r.fields.filter(([n]) => n === 'vary'), []);
  assert.deepEqual(r.fields.filter(([n]) => n === 'template-tag'), [['template-tag', 'x']]);
  assert.equal(JSON.parse(Buffer.from(r.body.base64, 'base64').toString('utf8'))['html-sha1'], '0');
});

test('exchanges_set_body_member_on_a_page_gives_base64_only_when_the_bytes_change', () => {
  const { file } = fileOf(cc({ schedule: [{ case: 'a' }, { do: 'set_body_member', name: 'x', value: 'y' }, ...FIRST] }));
  // An HTML body is not a JSON object: the proxy delivers it unchanged, so it stays a page reference.
  assert.equal(file.exchanges[0].response.body.page, 'a');
});

test('exchanges_cache_before_record_is_the_seeded_record_after_the_overrides', () => {
  const { file } = fileOf(cc({
    cache_before: [{ page: 'a', partition: null, etag: 'abc', fields: [['X-App', '1']], vary: [['Cookie', null]] }],
    schedule: [{ do: 'snapshot_cache' }],
  }));
  const [r] = file.cache_before;
  assert.equal(r.partition, null);
  assert.equal(r.etag, 'abc');
  assert.equal(r.template_tag.length, 40);
  assert.deepEqual(r.fields, [['x-app', '1']]);
  assert.deepEqual(r.vary, [['cookie', null]]);
  assert.deepEqual(r.page, { page: 'a', len: pageOf('a').length, sha256: sha256hex(pageOf('a')) });
  assert.deepEqual(r.data.map(([k]) => k), ['{title}', '{price}', '{stock}']);
  assert.ok(r.template.includes('{price}'));
  assert.deepEqual(file.exchanges, []);
});

test('exchanges_record_copy_does_not_follow_later_changes_of_the_entry', () => {
  // A 304 with a page field merges it into the stored entry (CL-17); the file keeps the seeded record.
  const app = { status: 200, headers: [['content-type', 'text/html; charset=utf-8'], ['x-new', '1']] };
  const { file } = fileOf(cc({ responses: { a: { page: 'a', app } }, cache_before: [{ page: 'a' }], schedule: [{ case: 'a' }, ...FIRST] }));
  assert.equal(file.exchanges[0].response.status, 304);
  assert.deepEqual(file.cache_before[0].fields, [['content-type', 'text/html; charset=utf-8']]);
});

test('body_bytes_checks_len_and_sha256_and_takes_exactly_one_form', () => {
  const a = pageOf('a');
  const form = { page: 'a', len: a.length, sha256: sha256hex(a) };
  assert.deepEqual(bodyBytes(form, pageOf), a);
  assert.equal(bodyBytes({ ...form, len: a.length + 1 }, pageOf), null);
  assert.equal(bodyBytes({ ...form, sha256: sha256hex('x') }, pageOf), null);
  assert.equal(bodyBytes({ ...form, base64: '' }, pageOf), null);
  assert.equal(bodyBytes({ page: 'none', len: 0, sha256: sha256hex('') }, pageOf), null);
  assert.deepEqual(bodyBytes({ base64: 'eHk=', len: 2, sha256: sha256hex('xy') }, pageOf), Buffer.from('xy'));
  assert.equal(bodyBytes(null, pageOf), null);
});

test('explain_projection_reports_a_body_that_follows_from_no_stored_bytes', () => {
  const { file, client } = fileOf(cc({ schedule: firstThenSecond(FIRST) }));
  assert.deepEqual(explainProjection(client, file, pageOf).problems, []);
  const other = structuredClone(client);
  const action = other.phases.flatMap((p) => p.actions).find((x) => x.do === 'load_data');
  action.body_sha256 = sha256hex('other');
  assert.match(explainProjection(other, file, pageOf).problems.join('\n'), /load_data: body_len \d+ and body_sha256 follow from no stored bytes/);
});

test('explain_projection_reports_a_changed_status_field_or_missing_exchange', () => {
  const { file, client } = fileOf(cc({ schedule: firstThenSecond(FIRST) }));
  const changed = structuredClone(file);
  changed.exchanges[1].response.status = 304;
  changed.exchanges[1].response.fields = changed.exchanges[1].response.fields.filter(([n]) => n !== 'etag');
  changed.exchanges[0].request.fields = [['accept-diff', 'false']];
  const p = explainProjection(client, changed, pageOf).problems;
  assert.ok(p.includes('exchange 2: the status differs'));
  assert.ok(p.includes('exchange 2: the response fields differ'));
  assert.ok(p.includes('exchange 1: the request fields differ'));
  const fewer = { ...file, exchanges: file.exchanges.slice(0, 1) };
  // Without the data response, the rebuilt page of the second session follows from nothing too.
  assert.deepEqual(explainProjection(client, fewer, pageOf).problems, [
    'exchange 2: not stored',
    '1 stored exchanges, 2 requests in the projection',
    'phase 2 load_data: body_len 296 and body_sha256 follow from no stored bytes',
  ]);
  const stops = structuredClone(file);
  stops.exchanges[0].stop = { kind: 'cut', at: 1 };
  assert.ok(explainProjection(client, stops, pageOf).problems.includes('exchange 1: complete, but the stored exchange stops'));
});

// README "Client exchanges.json" rule 1, over every canonical client fixture of the repository.
test('every_canonical_client_fixture_projection_follows_from_its_exchanges_json', () => {
  const dir = join(ROOT, 'fixtures/legacy');
  const pages = (id) => {
    try {
      return readFileSync(join(PAGES, `${id}.html`));
    } catch {
      return null;
    }
  };
  const names = readdirSync(dir).filter((n) => n.startsWith('FX-LEG-CL-') && !n.includes('--'));
  assert.equal(names.length, CLIENT_FLOOR);
  const totals = { exchange: 0, cache: 0, rebuilt: 0, prefix: 0 };
  let exchanges = 0;
  for (const name of names) {
    const file = JSON.parse(readFileSync(join(dir, name, EXCHANGES), 'utf8'));
    const expected = JSON.parse(readFileSync(join(dir, name, 'expected.json'), 'utf8'));
    assert.equal(file.id, name);
    const { problems, counts } = explainProjection(expected.client, file, pages);
    assert.deepEqual(problems, [], name);
    for (const k of Object.keys(totals)) totals[k] += counts[k];
    exchanges += file.exchanges.length;
  }
  // Every kind of origin occurs in the catalog, so each branch of the rule is used.
  for (const [k, v] of Object.entries(totals)) assert.ok(v > 0, `${k}: ${v}`);
  assert.ok(exchanges > CLIENT_FLOOR);
});
