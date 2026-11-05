// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

import assert from 'node:assert/strict';
import test from 'node:test';
import { eventProblems, foreignHosts, runJsonProblems } from '../src/check.mjs';

const SHA = 'a'.repeat(64);
const base = { seq: 1, t: 0, src: 'harness', thread: 'main', hseq: 1 };
const load = (extra) => ({ ...base, ev: 'client.load_data', base_url: 'u', mime: 'm', encoding: 'e', history_url: 'u', headers: [['a', 'b']], body_len: 0, body_sha256: SHA, ...extra });

test('check_sha256_must_be_exactly_64_lowercase_hex_digits', () => {
  assert.deepEqual(eventProblems(load({})), []);
  for (const sha of ['a'.repeat(63), 'a'.repeat(65), `x${SHA}`, `${SHA}x`, 'A'.repeat(64), `${SHA}\n`]) {
    assert.deepEqual(eventProblems(load({ body_sha256: sha })), ['client.load_data: bad body_sha256'], JSON.stringify(sha));
  }
});

test('check_field_lists_are_pairs_of_two_strings', () => {
  for (const headers of [[['a', 1]], [[1, 'b']], [['a', 'b', 'c']], [['a']], ['ab'], 'x', [['a', 'b'], ['c']]]) {
    assert.deepEqual(eventProblems(load({ headers })), ['client.load_data: bad headers'], JSON.stringify(headers));
  }
  assert.deepEqual(eventProblems(load({ headers: [] })), []);
  const extras = (v) => eventProblems({ ...base, ev: 'client.load_url', url: 'u', extras: v });
  assert.deepEqual(extras(['a', 'b']), []);
  assert.deepEqual(extras(['a', 1]), ['client.load_url: bad extras']);
  assert.deepEqual(extras('a'), ['client.load_url: bad extras']);
});

test('check_cache_files_have_sizes_from_zero_and_strictly_increasing_names', () => {
  const snap = (files) => eventProblems({ ...base, ev: 'cache.snapshot', files, db: null });
  assert.deepEqual(snap([{ name: 'a', size: 0, sha256: SHA }]), []);
  assert.deepEqual(snap([{ name: 'a', size: -1, sha256: SHA }]), ['cache.snapshot: bad files']);
  assert.deepEqual(snap([{ name: 'a', size: 1.5, sha256: SHA }]), ['cache.snapshot: bad files']);
  assert.deepEqual(snap([{ name: 'a', size: 1, sha256: SHA }, { name: 'a', size: 1, sha256: SHA }]), ['cache.snapshot: bad files']);
  assert.deepEqual(snap([null]), ['cache.snapshot: bad files']);
  assert.deepEqual(snap([{ size: 1, sha256: SHA }]), ['cache.snapshot: bad files']);
  assert.deepEqual(snap([{ name: 'a', sha256: SHA }]), ['cache.snapshot: bad files']);
  assert.deepEqual(snap([{ name: 'a', size: 1 }]), ['cache.snapshot: bad files']);
  assert.deepEqual(snap('a'), ['cache.snapshot: bad files']);
});

test('check_session_index_starts_at_one', () => {
  const at = (session) => eventProblems({ ...base, ev: 'session.result', src_code: 1, final_code: 1, session });
  assert.deepEqual(at(1), []);
  assert.deepEqual(at(2), []);
  assert.equal(at(0).length, 1);
  assert.equal(at(1.5).length, 1);
  assert.equal(at('1').length, 1);
});

test('check_optional_members_are_typed', () => {
  assert.deepEqual(eventProblems({ ...base, src: 'conductor', thread: undefined, hseq: undefined, ev: 'step.done', step: {}, error: 5 }).filter((p) => p.includes('error')), ['step.done: bad error']);
  assert.deepEqual(eventProblems({ ...base, src: 'proxy', thread: undefined, hseq: undefined, ev: 'proxy.hold', exchange: 1, point: 'body', bytes: 'x' }), ['proxy.hold: bad bytes']);
});

test('check_source_must_be_one_of_the_three', () => {
  const problems = eventProblems({ ...base, src: 'device', ev: 'client.clear_history' });
  assert.ok(problems.includes('src is not harness, proxy or conductor'));
});

test('check_hosts_must_be_whole_names', () => {
  assert.deepEqual(foreignHosts('http://example.com/ http://a.b.example.net/ https://10.0.2.2:18090/ http://localhost:1/ http://127.0.0.1/'), []);
  assert.deepEqual(foreignHosts('http://evil-example.com/'), ['evil-example.com']);
  assert.deepEqual(foreignHosts('http://example.com.evil.test/'), ['example.com.evil.test']);
  assert.deepEqual(foreignHosts('http://xlocalhost/'), ['xlocalhost']);
  assert.deepEqual(foreignHosts('http://10.0.2.22/'), ['10.0.2.22']);
  assert.deepEqual(foreignHosts('http://example.io/'), ['example.io']);
  assert.deepEqual(foreignHosts('HTTPS://EXAMPLE.COM/'), []);
});

const good = { date: '2026-11-05', tools: {}, emulator_image: 'x', upstream_sha: '59936bef', scenarios: [{ id: 'CS-01', modes: {} }], summary: { stable: [], unstable: [] } };

test('check_run_json_values_must_match_whole_patterns', () => {
  assert.deepEqual(runJsonProblems(good), []);
  for (const date of ['x2026-11-05', '2026-11-05x', '2026-1-05', '2026-11-05\n']) assert.ok(runJsonProblems({ ...good, date }).length > 0, date);
  for (const sha of ['zz59936bef', '59936bef!', '5993', 'A'.repeat(8), 'a'.repeat(41)]) assert.ok(runJsonProblems({ ...good, upstream_sha: sha }).length > 0, sha);
  assert.deepEqual(runJsonProblems({ ...good, upstream_sha: 'a'.repeat(40) }), []);
});

test('check_run_json_entries_have_their_members', () => {
  const bad = [
    { scenarios: [{ id: 'CS-01', modes: {} }, { modes: {} }] },
    { scenarios: [{ id: 'CS-01', modes: {} }, { id: 'CS-02' }] },
    { scenarios: [null] },
    { summary: { stable: [{ scenario: 'a' }], unstable: [] } },
    { summary: { stable: [{ mode: 'quick' }], unstable: [] } },
    { summary: { stable: [null], unstable: [] } },
    { summary: { stable: [], unstable: [{ scenario: 'a', mode: 'quick' }] } },
    { summary: { stable: [], unstable: [{ scenario: 'a', reason: 'r' }] } },
    { summary: { stable: [], unstable: [{ mode: 'quick', reason: 'r' }] } },
    { summary: { stable: [], unstable: [null] } },
    { summary: { stable: [] } },
    { summary: { unstable: [] } },
    { summary: { stable: {}, unstable: [] } },
  ];
  for (const patch of bad) assert.ok(runJsonProblems({ ...good, ...patch }).length > 0, JSON.stringify(patch));
  assert.deepEqual(runJsonProblems({ ...good, summary: { stable: [{ scenario: 'a', mode: 'quick' }], unstable: [{ scenario: 'a', mode: 'standard', reason: 'r' }] } }), []);
  assert.ok(runJsonProblems({ ...good, functions: { ios: {}, web: {} } }).length > 0);
  assert.ok(runJsonProblems({ ...good, tools: { node: 1 } }).length > 0);
  assert.ok(runJsonProblems({ ...good, tools: null }).length > 0);
});
