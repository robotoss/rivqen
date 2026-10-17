// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors
//
// The generator: the committed files are current, --check finds stale,
// missing and extra files, and defective cases stop the generator.

import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { cpSync, mkdtempSync, rmSync, writeFileSync, readFileSync, mkdirSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { after, test } from 'node:test';
import { checkPages, run } from '../gen.mjs';
import { CaseError, FIXTURES_DIR, expectFixture, fixtureNames, formatJson, substitute } from '../src/fixtures.mjs';
import { GENERATED, pageBytes } from '../src/pages.mjs';

const GEN = fileURLToPath(new URL('../gen.mjs', import.meta.url));
const tmp = mkdtempSync(join(tmpdir(), 'rqp-protocol-gen-'));
after(() => rmSync(tmp, { recursive: true, force: true }));

function copyFixture(name) {
  const dir = join(tmp, `${name}-${Math.random().toString(36).slice(2)}`);
  mkdirSync(dir);
  cpSync(join(FIXTURES_DIR, name), join(dir, name), { recursive: true });
  return dir;
}

test('gen.mjs --check: every committed page and expected file is current', () => {
  const out = execFileSync(process.execPath, [GEN, '--check'], { encoding: 'utf8' });
  assert.match(out, /130 fixtures are current/);
});

test('every catalog case P-01 to P-39 and C-01 to C-24 has at least one fixture', () => {
  const names = fixtureNames();
  for (let i = 1; i <= 39; i++) assert.ok(names.some((n) => n.startsWith(`FX-RQ-P${String(i).padStart(2, '0')}-`)), `P-${i}`);
  for (let i = 1; i <= 24; i++) assert.ok(names.some((n) => n.startsWith(`FX-RQ-C${String(i).padStart(2, '0')}-`)), `C-${i}`);
});

test('--check reports a stale expected file and a missing one, and the generator rewrites them', () => {
  const dir = copyFixture('FX-RQ-P09-DELTA');
  const file = join(dir, 'FX-RQ-P09-DELTA', 'expected.json');
  const good = readFileSync(file);
  writeFileSync(file, good.toString('utf8').replace('"price"', '"title"'));
  assert.deepEqual(run({ check: true, dir }).problems, ['FX-RQ-P09-DELTA/expected.json: stale']);
  rmSync(file);
  assert.deepEqual(run({ check: true, dir }).problems, ['FX-RQ-P09-DELTA/expected.json: missing']);
  assert.deepEqual(run({ check: false, dir }).problems, []);
  assert.deepEqual(readFileSync(file), good);
});

test('--check reports an unknown file and a response.body that the case does not have', () => {
  const dir = copyFixture('FX-RQ-P09-DELTA');
  writeFileSync(join(dir, 'FX-RQ-P09-DELTA', 'response.body'), 'x');
  writeFileSync(join(dir, 'FX-RQ-P09-DELTA', 'notes.txt'), 'x');
  const problems = run({ check: true, dir }).problems;
  assert.ok(problems.includes('FX-RQ-P09-DELTA/response.body: not generated for this case'));
  assert.ok(problems.includes('FX-RQ-P09-DELTA/notes.txt: unknown file'));
});

test('a stale response.body of a client case is found', () => {
  const dir = copyFixture('FX-RQ-C01-BASE-MISMATCH');
  writeFileSync(join(dir, 'FX-RQ-C01-BASE-MISMATCH', 'response.body'), '{}');
  assert.deepEqual(run({ check: true, dir }).problems, ['FX-RQ-C01-BASE-MISMATCH/response.body: stale']);
});

test('defective cases stop with a case error: id, unknown member, placeholder, page', () => {
  const dir = copyFixture('FX-RQ-P09-DELTA');
  const file = join(dir, 'FX-RQ-P09-DELTA', 'case.json');
  const c = JSON.parse(readFileSync(file, 'utf8'));
  const expectError = (mutated, pattern) => {
    writeFileSync(file, JSON.stringify(mutated));
    assert.throws(() => expectFixture(dir, 'FX-RQ-P09-DELTA'), (e) => e instanceof CaseError && pattern.test(e.message));
  };
  expectError({ ...c, id: 'FX-RQ-OTHER' }, /not the directory name/);
  expectError({ ...c, extra: 1 }, /unknown member case.extra/);
  expectError({ ...c, request: { ...c.request, headers: [['if-none-match', '{{r2:page-a}}']] } }, /unknown placeholder kind r2/);
  expectError({ ...c, request: { ...c.request, headers: [['if-none-match', '{{r1:page-a}']] } }, /not closed/);
  expectError({ ...c, request: { ...c.request, headers: [['If-None-Match', 'x']] } }, /not lower case/);
  expectError({ ...c, page: 'page-zzz' }, /unknown page page-zzz/);
  writeFileSync(file, JSON.stringify({ ...c, request: { ...c.request, headers: [['if-none-match', '{{r1:page-c}}']] } }));
  assert.ok(run({ check: true, dir }).problems.some((p) => /page page-c is not valid for RQP/.test(p)));
});

test('level "should" is copied to expected.json; another level is a defective case', () => {
  const name = 'FX-RQ-P35-ATTRIBUTE-RULE';
  const dir = copyFixture(name);
  const file = join(dir, name, 'case.json');
  const c = JSON.parse(readFileSync(file, 'utf8'));
  assert.equal(expectFixture(dir, name).files['expected.json'].level, 'should');
  writeFileSync(file, JSON.stringify({ ...c, level: 'must' }));
  assert.throws(() => expectFixture(dir, name), (e) => e instanceof CaseError && /level must be "should"/.test(e.message));
  const { level: _level, ...withoutLevel } = c;
  writeFileSync(file, JSON.stringify(withoutLevel));
  assert.equal(Object.hasOwn(expectFixture(dir, name).files['expected.json'], 'level'), false);
});

test('placeholders: r1 with quotes, t1 as a token, have as a Byte Sequence; ":" for zero blocks', () => {
  assert.match(substitute('T', '{{r1:page-a}}'), /^"r1\.[A-Za-z0-9_-]{43}"$/);
  assert.match(substitute('T', '{{t1:page-a}}'), /^t1\.[A-Za-z0-9_-]{43}$/);
  assert.equal(substitute('T', '{{have:page-zero}}'), '::');
  assert.equal(substitute('T', 'no placeholder'), 'no placeholder');
});

test('page recipes: the sizes of fixtures.md section 4.1 and the expected markup results', () => {
  assert.deepEqual(checkPages(), []);
  const size = (id) => pageBytes(id).length;
  assert.ok(size('page-big-a') > 19 * 1024 && size('page-big-a') < 22 * 1024);
  const huge = pageBytes('page-huge-a').toString('latin1');
  assert.equal(huge.split('data-rq-block="huge">')[1].split('</div>')[0].length, 300 * 1024);
  const a = pageBytes('page-huge-a');
  const b = pageBytes('page-huge-b');
  const diff = [...a.keys()].filter((i) => a[i] !== b[i]);
  assert.equal(diff.length, 1);
  assert.equal(pageBytes('page-esc-a').toString('latin1').split('"').length - 1, 2048 + 2); // and the 2 quotes of the block attribute
  assert.deepEqual(Object.keys(GENERATED).sort(), ['page-big-a', 'page-big-b', 'page-esc-a', 'page-esc-b', 'page-huge-a', 'page-huge-b']);
});

test('checkPages finds a page with the wrong markup result', () => {
  const problems = checkPages((id) => (id === 'page-c' ? pageBytes('page-a') : pageBytes(id)));
  assert.deepEqual(problems, ['page page-c: expected RQP_MARKUP_DUPLICATE, got valid']);
});

test('formatJson keeps header lines on one line and is valid JSON', () => {
  const value = { a: [['x', 'y']], b: [], c: {}, d: [{ e: 1 }] };
  const text = formatJson(value);
  assert.deepEqual(JSON.parse(text), value);
  assert.match(text, /\["x","y"\]/);
});

test('the usage error exits 2', () => {
  assert.throws(() => execFileSync(process.execPath, [GEN, '--write'], { stdio: 'pipe' }), (e) => e.status === 2);
});
