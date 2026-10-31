// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors
//
// The generator and its checks, on small fixture directories in a temporary directory.

import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { cpSync, existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { after, test } from 'node:test';
import { CATALOG_SIZE, main, run, storedTraces } from '../gen.mjs';
import { REPO_ROOT, UPSTREAM } from '../src/fixture.mjs';
import { respond } from '../src/model.mjs';
import { APP, page, traceText } from './helpers.mjs';

const GEN = fileURLToPath(new URL('../gen.mjs', import.meta.url));
const WIRE = join(REPO_ROOT, 'docs/engineering/protocol/legacy-wire.md');
const tmp = mkdtempSync(join(tmpdir(), 'legacy-fixtures-gen-'));
after(() => rmSync(tmp, { recursive: true, force: true }));

const PROFILE_OF = { java: 'legacy-java', node: 'legacy-node', php: 'legacy-php' };
const LEGACY_REQUEST = [['Accept-Encoding', 'gzip'], ['accept-diff', 'true']];
const PLAIN_REQUEST = [['Accept-Encoding', 'gzip']];
let counter = 0;

/** A temporary root with pages a and b and, per option, fixtures and traces. */
function setup() {
  const root = join(tmp, `case-${counter++}`);
  const dir = join(root, 'fx');
  mkdirSync(join(dir, 'pages'), { recursive: true });
  for (const id of ['a', 'b']) writeFileSync(join(dir, 'pages', `${id}.html`), page(id));
  mkdirSync(join(root, 'traces'));
  return { root, dir };
}

function write(dir, name, value) {
  mkdirSync(join(dir, name), { recursive: true });
  writeFileSync(join(dir, name, 'case.json'), JSON.stringify(value));
}

function canonicalCase(id, request, traces, extra = {}) {
  return {
    id,
    description: 'test',
    profile: 'legacy-canonical',
    rules: ['C-02'],
    upstream: { ...UPSTREAM },
    sources: ['docs:engineering/protocol/legacy-wire#canonical-server'],
    evidence: traces === null ? 'code-reading' : 'trace',
    ...(traces === null ? {} : { traces }),
    page: 'a',
    request: { headers: request },
    ...extra,
  };
}

/** Write the traces of one scenario from the model; `change` can alter the response of a server. */
function writeTraces(root, stem, request, change = {}) {
  const traces = {};
  for (const [server, profile] of Object.entries(PROFILE_OF)) {
    const resolved = request.map(([n, v]) => [n, v]);
    let response = respond(profile, { method: 'GET', headers: resolved }, APP, page('a'));
    if (change[server] !== undefined) response = change[server](response);
    const path = `traces/${server}-${stem}.http`;
    writeFileSync(join(root, path), traceText('/p/a', resolved, response));
    traces[profile] = path;
  }
  return traces;
}

function options(root, dir, extra = {}) {
  return { check: true, dir, root, wirePage: WIRE, labPages: join(root, 'no-lab'), traces: [], ...extra };
}

test('check_detects_stale_expected', () => {
  const { root, dir } = setup();
  write(dir, 'FX-LEG-S', canonicalCase('FX-LEG-S', PLAIN_REQUEST, null));
  const written = run(options(root, dir, { check: false }));
  assert.deepEqual(written.problems, []);
  assert.equal(written.count, 1);
  const file = join(dir, 'FX-LEG-S', 'expected.json');
  assert.deepEqual(run(options(root, dir)).problems, []);

  writeFileSync(file, readFileSync(file, 'utf8').replace('"status": 200', '"status": 201'));
  assert.deepEqual(run(options(root, dir)).problems, ['FX-LEG-S/expected.json: stale']);
  assert.deepEqual(run(options(root, dir, { check: false })).problems, []);
  assert.deepEqual(run(options(root, dir)).problems, []);

  rmSync(file);
  assert.deepEqual(run(options(root, dir)).problems, ['FX-LEG-S/expected.json: missing']);
  assert.equal(existsSync(file), false, '--check writes nothing');

  run(options(root, dir, { check: false }));
  writeFileSync(join(dir, 'FX-LEG-S', 'notes.txt'), 'x');
  writeFileSync(join(dir, 'stray.txt'), 'x');
  mkdirSync(join(dir, 'other'));
  assert.deepEqual(run(options(root, dir)).problems.sort(), ['FX-LEG-S/notes.txt: unknown file', 'other: unknown entry', 'stray.txt: unknown entry']);
});

test('trace_backed_fixture_passes_when_the_traces_equal_the_model', () => {
  const { root, dir } = setup();
  const traces = writeTraces(root, 'first', LEGACY_REQUEST);
  write(dir, 'FX-LEG-T', canonicalCase('FX-LEG-T', LEGACY_REQUEST, traces));
  for (const [s, p] of [['JAVA', 'legacy-java'], ['NODE', 'legacy-node'], ['PHP', 'legacy-php']]) {
    write(dir, `FX-LEG-T--${s}`, { id: `FX-LEG-T--${s}`, description: 'x', profile: p, replaces: 'FX-LEG-T', divergences: ['D-09'], sources: ['upstream:x:1'], trace: traces[p] });
  }
  const result = run(options(root, dir, { check: false }));
  assert.deepEqual(result.problems, []);
  assert.equal(result.count, 4);
  const expected = JSON.parse(readFileSync(join(dir, 'FX-LEG-T--JAVA', 'expected.json'), 'utf8'));
  assert.equal(expected.profile, 'legacy-java');
  assert.equal(expected.trace, traces['legacy-java']);
  assert.deepEqual(expected.fields['cache-control'], ['no-cache']);
  assert.equal(Object.hasOwn(expected, 'profiles_same'), false);
  const canonical = JSON.parse(readFileSync(join(dir, 'FX-LEG-T', 'expected.json'), 'utf8'));
  assert.deepEqual(canonical.profiles_same, []);
});

test('override_missing_is_error', () => {
  const { root, dir } = setup();
  const traces = writeTraces(root, 'first', LEGACY_REQUEST);
  write(dir, 'FX-LEG-T', canonicalCase('FX-LEG-T', LEGACY_REQUEST, traces));
  const { problems } = run(options(root, dir, { check: false }));
  assert.equal(problems.length, 3);
  assert.match(problems[0], /^FX-LEG-T: OVERRIDE_MISSING for legacy-java: cache-control: expected \["no-cache, private"\], trace \["no-cache"\]/);
  assert.match(problems[1], /OVERRIDE_MISSING for legacy-node/);
  assert.match(problems[2], /OVERRIDE_MISSING for legacy-php/);
});

test('override_not_needed_is_error', () => {
  const { root, dir } = setup();
  const traces = writeTraces(root, 'plain', PLAIN_REQUEST);
  write(dir, 'FX-LEG-P', canonicalCase('FX-LEG-P', PLAIN_REQUEST, traces));
  // java adds legacy fields to a plain request (D-02): it needs an override; node and php do not
  write(dir, 'FX-LEG-P--JAVA', { id: 'FX-LEG-P--JAVA', description: 'x', profile: 'legacy-java', replaces: 'FX-LEG-P', divergences: ['D-02'], sources: ['upstream:x:1'], trace: traces['legacy-java'] });
  write(dir, 'FX-LEG-P--NODE', { id: 'FX-LEG-P--NODE', description: 'x', profile: 'legacy-node', replaces: 'FX-LEG-P', divergences: [], sources: ['upstream:x:1'], trace: traces['legacy-node'] });
  const { problems } = run(options(root, dir, { check: false }));
  assert.deepEqual(problems, ['FX-LEG-P--NODE: OVERRIDE_NOT_NEEDED: the expected response equals FX-LEG-P']);
  const canonical = JSON.parse(readFileSync(join(dir, 'FX-LEG-P', 'expected.json'), 'utf8'));
  assert.deepEqual(canonical.profiles_same, ['legacy-node', 'legacy-php']);
});

test('profile_fixture_must_equal_its_trace', () => {
  const { root, dir } = setup();
  const traces = writeTraces(root, 'first', LEGACY_REQUEST, {
    java: (r) => ({ ...r, headers: r.headers.filter(([n]) => n !== 'template-change').concat([['template-change', 'false']]) }),
  });
  write(dir, 'FX-LEG-T', canonicalCase('FX-LEG-T', LEGACY_REQUEST, traces));
  write(dir, 'FX-LEG-T--JAVA', { id: 'FX-LEG-T--JAVA', description: 'x', profile: 'legacy-java', replaces: 'FX-LEG-T', divergences: [], sources: ['upstream:x:1'], trace: traces['legacy-java'] });
  const { problems } = run(options(root, dir, { check: false }));
  assert.ok(problems.includes(`FX-LEG-T--JAVA: template-change: expected ["true"], trace ["false"] (${traces['legacy-java']})`), problems.join('\n'));
});

test('request_lines_must_equal_the_trace_request', () => {
  const { root, dir } = setup();
  const traces = writeTraces(root, 'first', LEGACY_REQUEST);
  write(dir, 'FX-LEG-T', canonicalCase('FX-LEG-T', LEGACY_REQUEST.map(([n, v]) => [n.toLowerCase(), v]), traces));
  write(dir, 'FX-LEG-T--JAVA', { id: 'FX-LEG-T--JAVA', description: 'x', profile: 'legacy-java', replaces: 'FX-LEG-T', divergences: [], sources: ['upstream:x:1'], trace: traces['legacy-java'] });
  const { problems } = run(options(root, dir, { check: false }));
  assert.ok(problems.includes('FX-LEG-T--JAVA: the request differs from the request of traces/java-first.http'), problems.join('\n'));
});

test('request_under_profiles_same_must_equal_the_trace_request', () => {
  const { root, dir } = setup();
  const traces = writeTraces(root, 'plain', PLAIN_REQUEST);
  write(dir, 'FX-LEG-P', canonicalCase('FX-LEG-P', [['accept-encoding', 'gzip']], traces));
  const { problems } = run(options(root, dir, { check: false }));
  assert.ok(problems.includes('FX-LEG-P: the request under legacy-node differs from the request of traces/node-plain.http'), problems.join('\n'));
});

test('not_reproduced_fixture_needs_a_different_template_tag_in_the_trace', () => {
  const { root, dir } = setup();
  const different = (r) => ({ ...r, headers: r.headers.map(([n, v]) => (n === 'template-tag' ? [n, '0'.repeat(40)] : [n, v])) });
  const traces = writeTraces(root, 'first', LEGACY_REQUEST, { java: different });
  write(dir, 'FX-LEG-T', canonicalCase('FX-LEG-T', LEGACY_REQUEST, traces));
  const profileCase = { id: 'FX-LEG-T--JAVA', description: 'x', profile: 'legacy-java', replaces: 'FX-LEG-T', divergences: ['D-13'], sources: ['upstream:x:1'], trace: traces['legacy-java'], not_reproduced: 'U-11' };
  write(dir, 'FX-LEG-T--JAVA', profileCase);
  for (const s of ['NODE', 'PHP']) {
    const p = PROFILE_OF[s.toLowerCase()];
    write(dir, `FX-LEG-T--${s}`, { ...profileCase, id: `FX-LEG-T--${s}`, profile: p, trace: traces[p], not_reproduced: undefined });
  }
  assert.deepEqual(run(options(root, dir, { check: false })).problems, []);
  // the trace shows no difference: the exemption is not allowed
  const same = writeTraces(root, 'same', LEGACY_REQUEST);
  write(dir, 'FX-LEG-T', canonicalCase('FX-LEG-T', LEGACY_REQUEST, same));
  write(dir, 'FX-LEG-T--JAVA', { ...profileCase, trace: same['legacy-java'] });
  const { problems } = run(options(root, dir, { check: false }));
  assert.ok(problems.some((p) => p.includes('does not show the documented difference')), problems.join('\n'));
});

/**
 * Write the traces of a case request (Host first line, path /catalog) for each server.
 * `response(profile)` gives the response that the "upstream" server sends.
 */
function writeCaseTraces(root, stem, method, request, response) {
  const traces = {};
  for (const [server, profile] of Object.entries(PROFILE_OF)) {
    const path = `traces/${server}-${stem}.http`;
    writeFileSync(join(root, path), traceText('/catalog', request, response(profile), 'identity', { method, host: 'shop.example.com' }));
    traces[profile] = path;
  }
  return traces;
}

test('u13_profile_fixtures_expect_the_pass_through_and_their_traces_show_legacy_processing', () => {
  const { root, dir } = setup();
  const request = [...LEGACY_REQUEST, ['If-None-Match', respond('legacy-canonical', { method: 'GET', headers: LEGACY_REQUEST }, APP, page('a')).headers.find(([n]) => n === 'etag')[1]]];
  const app = { status: 302, headers: [...APP.headers, ['location', 'https://shop.example.com/login']] };
  // the upstream servers answer the 302 like a 200: a 304 (trace S24)
  const upstream = (profile) => respond(profile, { method: 'GET', headers: request }, APP, page('a'));
  const traces = writeCaseTraces(root, 'redirect', 'GET', request, upstream);
  write(dir, 'FX-LEG-R', canonicalCase('FX-LEG-R', request, traces, { app }));
  // without profile fixtures: OVERRIDE_MISSING for every profile
  assert.deepEqual(run(options(root, dir, { check: false })).problems.map((p) => p.slice(0, 37)), [
    'FX-LEG-R: OVERRIDE_MISSING for legacy',
    'FX-LEG-R: OVERRIDE_MISSING for legacy',
    'FX-LEG-R: OVERRIDE_MISSING for legacy',
  ]);
  const profileCase = (s) => ({ id: `FX-LEG-R--${s}`, description: 'x', profile: PROFILE_OF[s.toLowerCase()], replaces: 'FX-LEG-R', divergences: [], sources: ['upstream:x:1'], trace: traces[PROFILE_OF[s.toLowerCase()]], not_reproduced: 'U-13' });
  for (const s of ['JAVA', 'NODE', 'PHP']) write(dir, `FX-LEG-R--${s}`, profileCase(s));
  // the profile fixtures equal the canonical pass-through: no OVERRIDE_NOT_NEEDED
  assert.deepEqual(run(options(root, dir, { check: false })).problems, []);
  const expected = JSON.parse(readFileSync(join(dir, 'FX-LEG-R--NODE', 'expected.json'), 'utf8'));
  assert.equal(expected.status, 302);
  assert.deepEqual(expected.fields.location, ['https://shop.example.com/login']);
  assert.deepEqual(expected.fields['cache-offline'], []);
  // the U-13 marker still needs a request equal to the trace request
  write(dir, 'FX-LEG-R', canonicalCase('FX-LEG-R', request.map(([n, v]) => [n.toLowerCase(), v]), traces, { app }));
  assert.ok(run(options(root, dir, { check: false })).problems.includes('FX-LEG-R--JAVA: the request differs from the request of traces/java-redirect.http'));
  // a trace that is the pass-through shows no difference: the marker is not allowed
  const passed = writeCaseTraces(root, 'passed', 'GET', request, (profile) => respond(profile, { method: 'GET', headers: request }, app, page('a')));
  write(dir, 'FX-LEG-R', canonicalCase('FX-LEG-R', request, passed, { app }));
  write(dir, 'FX-LEG-R--JAVA', { ...profileCase('JAVA'), trace: passed['legacy-java'] });
  for (const s of ['NODE', 'PHP']) rmSync(join(dir, `FX-LEG-R--${s}`), { recursive: true });
  const { problems } = run(options(root, dir, { check: false }));
  assert.deepEqual(problems, ['FX-LEG-R--JAVA: U-13: the trace does not show the documented difference (no 304, data body or legacy field) (traces/java-passed.http)']);
});

test('a_case_request_trace_must_have_the_method_and_the_target_of_the_fixture', () => {
  const { root, dir } = setup();
  const head = writeCaseTraces(root, 'head', 'HEAD', LEGACY_REQUEST, (profile) => respond(profile, { method: 'HEAD', headers: LEGACY_REQUEST }, APP, page('a')));
  write(dir, 'FX-LEG-H', canonicalCase('FX-LEG-H', LEGACY_REQUEST, head, { request: { method: 'HEAD', headers: LEGACY_REQUEST } }));
  assert.deepEqual(run(options(root, dir, { check: false })).problems, []);
  assert.deepEqual(JSON.parse(readFileSync(join(dir, 'FX-LEG-H', 'expected.json'), 'utf8')).profiles_same, ['legacy-java', 'legacy-node', 'legacy-php']);
  // a PUT trace for a POST fixture: the same pass-through response, but the method differs
  const put = writeCaseTraces(root, 'put', 'PUT', LEGACY_REQUEST, (profile) => respond(profile, { method: 'PUT', headers: LEGACY_REQUEST }, APP, page('a')));
  write(dir, 'FX-LEG-H', canonicalCase('FX-LEG-H', LEGACY_REQUEST, put, { request: { method: 'POST', headers: LEGACY_REQUEST } }));
  assert.deepEqual(run(options(root, dir, { check: false })).problems, ['java', 'node', 'php'].map((s) => `FX-LEG-H: the request under legacy-${s} differs from the request of traces/${s}-put.http`));
  // another target path: the request differs
  write(dir, 'FX-LEG-H', canonicalCase('FX-LEG-H', LEGACY_REQUEST, head, { request: { method: 'HEAD', target: 'https://shop.example.com/other', headers: LEGACY_REQUEST } }));
  const { problems } = run(options(root, dir, { check: false }));
  assert.deepEqual(problems, ['java', 'node', 'php'].map((s) => `FX-LEG-H: the request under legacy-${s} differs from the request of traces/${s}-head.http`));
});

test('stored_trace_that_no_fixture_uses_is_an_error', () => {
  const { root, dir } = setup();
  write(dir, 'FX-LEG-S', canonicalCase('FX-LEG-S', PLAIN_REQUEST, null));
  const { problems } = run(options(root, dir, { check: false, traces: ['evidence/traces/server/java/S99.http'] }));
  assert.deepEqual(problems, ['evidence/traces/server/java/S99.http: stored trace is not used by any fixture']);
});

test('a_missing_or_malformed_trace_is_an_error', () => {
  const { root, dir } = setup();
  const traces = writeTraces(root, 'first', LEGACY_REQUEST);
  writeFileSync(join(root, traces['legacy-node']), 'not a trace');
  rmSync(join(root, traces['legacy-php']));
  write(dir, 'FX-LEG-T', canonicalCase('FX-LEG-T', LEGACY_REQUEST, traces));
  const { problems } = run(options(root, dir, { check: false }));
  assert.ok(problems.some((p) => p.startsWith('FX-LEG-T: trace traces/node-first.http: missing "--- response ---"')), problems.join('\n'));
  assert.ok(problems.some((p) => p.startsWith('FX-LEG-T: trace traces/php-first.http: ENOENT')), problems.join('\n'));
});

test('rules_must_exist_and_are_covered_at_the_full_catalog', () => {
  const { root, dir } = setup();
  write(dir, 'FX-LEG-S', canonicalCase('FX-LEG-S', PLAIN_REQUEST, null, { rules: ['C-02', 'C-99'] }));
  assert.deepEqual(run(options(root, dir, { check: false })).problems, ['FX-LEG-S: rule C-99 is not in legacy wire section 10']);
  write(dir, 'FX-LEG-S', canonicalCase('FX-LEG-S', PLAIN_REQUEST, null));
  const partial = run(options(root, dir, { check: false }));
  assert.deepEqual(partial.problems, []);
  assert.equal(partial.notes.length, 1);
  assert.match(partial.notes[0], /checked at 59 canonical fixtures \(now 1\); not covered yet: C-01, C-03, C-04/);
  const full = run(options(root, dir, { check: false, catalogSize: 1 }));
  assert.equal(full.problems.length, 15); // C-01 to C-16 without C-02
  assert.equal(full.problems[0], 'rule C-01 is not covered by any fixture');
  assert.equal(full.notes.length, 0);
  assert.equal(CATALOG_SIZE, 59);
});

test('the_repository_catalog_cannot_shrink_below_catalog_size', () => {
  // A smaller catalog would switch the rule coverage off; for the repository catalog it is an error.
  const { root, dir } = setup();
  write(dir, 'FX-LEG-S', canonicalCase('FX-LEG-S', PLAIN_REQUEST, null));
  const r = run(options(root, dir, { check: false, catalog: true }));
  assert.equal(r.problems[0], 'the catalog has 1 canonical fixtures, fewer than 59 (CATALOG_SIZE)');
  assert.equal(r.problems[1], 'rule C-01 is not covered by any fixture');
  assert.equal(r.notes.length, 0);
});

test('a_wire_page_without_rules_is_an_error', () => {
  const { root, dir } = setup();
  const empty = join(root, 'empty.md');
  writeFileSync(empty, '# nothing');
  write(dir, 'FX-LEG-S', canonicalCase('FX-LEG-S', PLAIN_REQUEST, null, { rules: ['C-02'] }));
  const { problems } = run(options(root, dir, { check: false, wirePage: empty }));
  assert.equal(problems.length, 2);
  assert.match(problems[0], /no rule C-nn found/);
});

test('defective_cases_are_reported_and_do_not_stop_the_run', () => {
  const { root, dir } = setup();
  write(dir, 'FX-LEG-GOOD', canonicalCase('FX-LEG-GOOD', PLAIN_REQUEST, null));
  write(dir, 'FX-LEG-BAD', { id: 'FX-LEG-OTHER' });
  write(dir, 'FX-LEG-PAGE', canonicalCase('FX-LEG-PAGE', PLAIN_REQUEST, null, { page: 'nope' }));
  write(dir, 'FX-LEG-X--JAVA', { id: 'FX-LEG-X--JAVA', description: 'x', profile: 'legacy-java', replaces: 'FX-LEG-X', divergences: [], sources: ['upstream:x:1'], trace: 't' });
  const { problems, count } = run(options(root, dir, { check: false }));
  assert.equal(count, 1);
  assert.deepEqual(problems, [
    'FX-LEG-BAD: id must be FX-LEG-BAD',
    'FX-LEG-PAGE: page nope is missing',
    'FX-LEG-X--JAVA: replaces FX-LEG-X, which is not a canonical fixture',
  ]);
});

test('profile_fixture_must_use_the_trace_of_the_replaced_fixture', () => {
  const { root, dir } = setup();
  const traces = writeTraces(root, 'first', LEGACY_REQUEST);
  write(dir, 'FX-LEG-T', canonicalCase('FX-LEG-T', LEGACY_REQUEST, traces));
  write(dir, 'FX-LEG-T--JAVA', { id: 'FX-LEG-T--JAVA', description: 'x', profile: 'legacy-java', replaces: 'FX-LEG-T', divergences: [], sources: ['upstream:x:1'], trace: 'traces/other.http' });
  write(dir, 'FX-LEG-S', canonicalCase('FX-LEG-S', PLAIN_REQUEST, null));
  write(dir, 'FX-LEG-S--PHP', { id: 'FX-LEG-S--PHP', description: 'x', profile: 'legacy-php', replaces: 'FX-LEG-S', divergences: [], sources: ['upstream:x:1'], trace: 'traces/x.http' });
  const { problems } = run(options(root, dir, { check: false }));
  assert.ok(problems.includes('FX-LEG-T--JAVA: trace must be the trace of FX-LEG-T for legacy-java'));
  assert.ok(problems.includes('FX-LEG-S--PHP: trace must be the trace of FX-LEG-S for legacy-php'));
});

test('pages_must_be_byte_copies_of_the_lab_pages', () => {
  const { root, dir } = setup();
  const lab = join(root, 'lab');
  mkdirSync(lab);
  writeFileSync(join(lab, 'a.html'), 'different');
  writeFileSync(join(lab, 'b.html'), page('b'));
  writeFileSync(join(dir, 'pages', 'extra.txt'), 'x');
  write(dir, 'FX-LEG-S', canonicalCase('FX-LEG-S', PLAIN_REQUEST, null));
  const { problems } = run(options(root, dir, { check: false, labPages: lab }));
  assert.deepEqual(problems, ['pages/a.html: not a byte copy of tools/upstream-lab/pages/a.html', 'pages/extra.txt: unknown file']);
  rmSync(join(dir, 'pages'), { recursive: true });
  assert.deepEqual(run(options(root, dir, { check: false })).problems, ['pages: directory missing', 'FX-LEG-S: page a is missing']);
});

test('the_committed_fixtures_are_current_and_every_stored_trace_is_used', () => {
  const out = spawnSync(process.execPath, [GEN, '--check'], { encoding: 'utf8' });
  assert.equal(out.status, 0, out.stderr);
  assert.match(out.stdout, /^legacy-fixtures gen: \d+ fixtures are current\n$/);
  const traces = storedTraces(REPO_ROOT);
  assert.equal(traces.length, 117);
  const result = run({ check: true });
  assert.deepEqual(result.problems, []);
  assert.ok(result.count >= 31);
});

test('every_trace_backed_canonical_fixture_names_the_same_trace_scenario_for_the_three_servers', () => {
  const dir = join(REPO_ROOT, 'fixtures/legacy');
  const stem = (p) => p.split('/').pop();
  for (const name of readdirSync(dir)) {
    if (!name.startsWith('FX-LEG-') || name.includes('--')) continue;
    const c = JSON.parse(readFileSync(join(dir, name, 'case.json'), 'utf8'));
    if (c.evidence !== 'trace') continue;
    const stems = new Set(Object.values(c.traces).map(stem));
    assert.equal(stems.size, 1, name);
    for (const [profile, path] of Object.entries(c.traces)) assert.ok(path.includes(`/${profile.slice(7)}/`), `${name} ${profile}`);
  }
});

test('gen_usage_error_exits_2', () => {
  const out = spawnSync(process.execPath, [GEN, '--bogus'], { encoding: 'utf8' });
  assert.equal(out.status, 2);
  assert.match(out.stderr, /usage: node gen.mjs \[--check\] \[--dir <path>\]/);
  const two = spawnSync(process.execPath, [GEN, '--check', '--check'], { encoding: 'utf8' });
  assert.equal(two.status, 2);
});

test('gen_dir_usage_errors_exit_2', () => {
  const { dir } = setup();
  for (const argv of [['--dir'], ['--dir', ''], ['--dir', dir, '--dir', dir], ['--dir', dir, '--bogus'], ['--check', '--dir']]) {
    const out = spawnSync(process.execPath, [GEN, ...argv], { encoding: 'utf8' });
    assert.equal(out.status, 2, argv.join(' '));
    assert.match(out.stderr, /^usage: node gen.mjs \[--check\] \[--dir <path>\]\n$/);
  }
  const missing = spawnSync(process.execPath, [GEN, '--dir', join(dir, 'nope')], { encoding: 'utf8' });
  assert.equal(missing.status, 2);
  assert.match(missing.stderr, /nope: not a directory\n$/);
});

test('gen_check_exits_1_on_a_stale_file_and_0_when_current', () => {
  const { root } = setup();
  const copy = join(root, 'copy');
  cpSync(join(REPO_ROOT, 'fixtures/legacy'), copy, { recursive: true });
  const capture = () => {
    const out = { stdout: '', stderr: '' };
    return { out, io: { dir: copy, stdout: { write: (t) => (out.stdout += t) }, stderr: { write: (t) => (out.stderr += t) } } };
  };
  const current = capture();
  assert.equal(main(['--check'], current.io), 0);
  assert.match(current.out.stdout, /fixtures are current/);

  const file = join(copy, 'FX-LEG-304', 'expected.json');
  writeFileSync(file, readFileSync(file, 'utf8').replace('"status": 304', '"status": 200'));
  const stale = capture();
  assert.equal(main(['--check'], stale.io), 1);
  assert.match(stale.out.stderr, /FX-LEG-304\/expected.json: stale/);
  assert.match(stale.out.stderr, /run "node tools\/legacy-fixtures\/gen.mjs" and review the diff/);
  assert.equal(stale.out.stdout, '');

  const write = capture();
  assert.equal(main([], write.io), 0);
  assert.match(write.out.stdout, /fixtures written/);
  assert.equal(main(['--check'], capture().io), 0);
});

test('gen_accepts_a_second_fixture_directory', () => {
  // A directory in the format of fixtures/legacy with its own pages: not the catalog, so it needs no
  // stored trace, no copy of a lab page and no catalog size.
  const { dir } = setup();
  writeFileSync(join(dir, 'pages', 'a.html'), 'not the lab page a');
  write(dir, 'FX-LEG-MAL-S', canonicalCase('FX-LEG-MAL-S', PLAIN_REQUEST, null));
  const capture = () => spawnSync(process.execPath, [GEN, '--dir', dir], { encoding: 'utf8' });
  const written = capture();
  assert.equal(written.status, 0, written.stderr);
  assert.equal(written.stdout, 'legacy-fixtures gen: 1 fixtures written\n');
  assert.equal(written.stderr, '');
  const expected = JSON.parse(readFileSync(join(dir, 'FX-LEG-MAL-S', 'expected.json'), 'utf8'));
  assert.equal(expected.id, 'FX-LEG-MAL-S');
  assert.equal(expected.request.headers.length, 1);

  const current = spawnSync(process.execPath, [GEN, '--check', '--dir', dir], { encoding: 'utf8' });
  assert.equal(current.status, 0, current.stderr);
  assert.equal(current.stdout, 'legacy-fixtures gen: 1 fixtures are current\n');

  const file = join(dir, 'FX-LEG-MAL-S', 'expected.json');
  writeFileSync(file, readFileSync(file, 'utf8').replace('"status": 200', '"status": 201'));
  const stale = spawnSync(process.execPath, [GEN, '--check', '--dir', dir], { encoding: 'utf8' });
  assert.equal(stale.status, 1);
  assert.match(stale.stderr, /FX-LEG-MAL-S\/expected.json: stale/);
  assert.ok(stale.stderr.includes(`run "node tools/legacy-fixtures/gen.mjs --dir ${dir}" and review the diff`), stale.stderr);
  assert.equal(readFileSync(file, 'utf8').includes('"status": 201'), true, '--check writes nothing');
});

test('malformed_dir_has_no_rule_coverage_check', () => {
  const { root, dir } = setup();
  write(dir, 'FX-LEG-MAL-S', canonicalCase('FX-LEG-MAL-S', PLAIN_REQUEST, null));
  const wire = join(root, 'wire.md');
  writeFileSync(wire, ['C-01', 'C-02', 'C-17'].map((r) => `| ${r} Name | text |`).join('\n'));
  const capture = () => {
    const out = { stdout: '', stderr: '' };
    return { out, io: { wirePage: wire, stdout: { write: (t) => (out.stdout += t) }, stderr: { write: (t) => (out.stderr += t) } } };
  };
  // another directory: C-01 and C-17 stay uncovered and nothing is said about it
  assert.equal(main(['--dir', dir], capture().io), 0);
  const second = capture();
  assert.equal(main(['--check', '--dir', dir], second.io), 0);
  assert.equal(second.out.stderr, '');
  // without --dir, the old behavior: a directory below the catalog size gets a note about the rules
  const plain = capture();
  assert.equal(main(['--check'], { ...plain.io, dir, traces: [] }), 0);
  assert.match(plain.out.stderr, /note: rule coverage is checked at 59 canonical fixtures \(now 1\); not covered yet: C-01, C-17/);
  // a rule that does not exist is still an error in this directory
  write(dir, 'FX-LEG-MAL-S', canonicalCase('FX-LEG-MAL-S', PLAIN_REQUEST, null, { rules: ['C-02', 'C-99'] }));
  const unknown = capture();
  assert.equal(main(['--check', '--dir', dir], unknown.io), 1);
  assert.match(unknown.out.stderr, /FX-LEG-MAL-S: rule C-99 is not in legacy wire section 10/);
  assert.doesNotMatch(unknown.out.stderr, /not covered/);
  // the catalog directory itself, named with --dir, keeps the coverage check
  const catalog = capture();
  assert.equal(main(['--check', '--dir', join(REPO_ROOT, 'fixtures/legacy')], catalog.io), 1);
  assert.match(catalog.out.stderr, /rule C-17 is not covered by any fixture/);
});
