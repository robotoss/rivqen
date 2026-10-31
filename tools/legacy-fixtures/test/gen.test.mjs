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
  assert.equal(traces.length, 93);
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
  assert.match(out.stderr, /usage: node gen.mjs \[--check\]/);
  const two = spawnSync(process.execPath, [GEN, '--check', '--check'], { encoding: 'utf8' });
  assert.equal(two.status, 2);
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
