// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// Command line of gen.mjs: options, exit codes, messages, default paths.

import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { existsSync, mkdtempSync, readFileSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { test } from 'node:test';
import { diffSummary, run } from '../gen.mjs';

const GEN = fileURLToPath(new URL('../gen.mjs', import.meta.url));
const USAGE = 'usage: gen.mjs [--check] [--catalog <file>] [--out-dir <dir>]';

const res = (os, verdict = 'supported') => ({ os, webview: null, verdict, condition: null, runs: ['evidence/r/run-1.jsonl'], recording: null });
const cap = (id, platform, results, verdict) => ({
  id, platform, title: `Title of ${id}`, prototype: 'P', questions: ['Q-1'], sprint: 'S1', results, verdict,
});
const fixture = () => [
  cap('CAP-A-01', 'android', [res('37'), res('29')], 'supported'),
  cap('CAP-I-01', 'ios', [res('27.0'), res('26.5'), res('18.5')], 'supported'),
];
const capture = () => {
  const out = { log: [], error: [] };
  return { out, log: (m) => out.log.push(m), error: (m) => out.error.push(m) };
};
const setup = (catalog = fixture()) => {
  const dir = mkdtempSync(join(tmpdir(), 'matrix-gen-'));
  const cat = join(dir, 'cat.json');
  writeFileSync(cat, JSON.stringify(catalog));
  return { dir, cat, out: join(dir, 'out') };
};

test('an unknown argument gives exit 2, the usage text and the argument', () => {
  const l = capture();
  assert.equal(run(['--nope'], l), 2);
  assert.deepEqual(l.out.error, [`${USAGE}\nunknown argument --nope`]);
  assert.deepEqual(l.out.log, []);
});

test('--catalog without a value is a usage error', () => {
  const l = capture();
  assert.equal(run(['--catalog'], l), 2);
  assert.deepEqual(l.out.error, [`${USAGE}\nunknown argument --catalog`]);
});

test('--out-dir without a value is a usage error', () => {
  const { cat } = setup();
  const l = capture();
  assert.equal(run(['--catalog', cat, '--out-dir'], l), 2);
  assert.deepEqual(l.out.error, [`${USAGE}\nunknown argument --out-dir`]);
});

test('a usage error stops before anything is written', () => {
  const { cat, out } = setup();
  assert.equal(run(['--catalog', cat, '--out-dir', out, '--bogus'], capture()), 2);
  assert.equal(existsSync(out), false);
});

test('--out-dir chooses where both pages are written, and the log names them', () => {
  const { cat, out } = setup();
  const l = capture();
  assert.equal(run(['--catalog', cat, '--out-dir', out], l), 0);
  assert.deepEqual(l.out.log, ['wrote ios-parity-matrix.md', 'wrote android-matrix.md']);
  assert.deepEqual(l.out.error, []);
  assert.match(readFileSync(join(out, 'ios-parity-matrix.md'), 'utf8'), /^# iOS parity matrix\n/);
  assert.match(readFileSync(join(out, 'android-matrix.md'), 'utf8'), /^# Android capability matrix\n/);
});

test('--catalog chooses the catalog file', () => {
  const { cat, out } = setup([cap('CAP-A-77', 'android', [res('37'), res('29')], 'supported')]);
  assert.equal(run(['--out-dir', out, '--catalog', cat], capture()), 0);
  assert.match(readFileSync(join(out, 'android-matrix.md'), 'utf8'), /CAP-A-77/);
});

test('options can come in any order and --check can be last', () => {
  const { cat, out } = setup();
  assert.equal(run(['--out-dir', out, '--catalog', cat], capture()), 0);
  const l = capture();
  assert.equal(run(['--out-dir', out, '--catalog', cat, '--check'], l), 0);
  assert.deepEqual(l.out.log, ['matrix pages are current']);
});

test('write mode does not print the "current" message; --check does not write', () => {
  const { cat, out } = setup();
  const w = capture();
  run(['--catalog', cat, '--out-dir', out], w);
  assert.ok(!w.out.log.includes('matrix pages are current'));
  const other = join(out, 'sub');
  const c = capture();
  assert.equal(run(['--check', '--catalog', cat, '--out-dir', other], c), 1);
  assert.equal(existsSync(other), false);
});

test('--check on a stale page prints no "current" message', () => {
  const { cat, out } = setup();
  run(['--catalog', cat, '--out-dir', out], capture());
  writeFileSync(join(out, 'android-matrix.md'), 'old\n');
  const l = capture();
  assert.equal(run(['--check', '--catalog', cat, '--out-dir', out], l), 1);
  assert.ok(!l.out.log.includes('matrix pages are current'));
});

test('a missing page is reported for every page, not only the first', () => {
  const { cat, out } = setup();
  const l = capture();
  assert.equal(run(['--check', '--catalog', cat, '--out-dir', out], l), 1);
  assert.deepEqual(l.out.error, ['ios-parity-matrix.md: missing; run gen.mjs', 'android-matrix.md: missing; run gen.mjs']);
});

test('a stale page is reported with the first differing line and both lines', () => {
  const { cat, out } = setup();
  run(['--catalog', cat, '--out-dir', out], capture());
  const file = join(out, 'android-matrix.md');
  const lines = readFileSync(file, 'utf8').split('\n');
  lines[0] = '# Hand edited';
  writeFileSync(file, lines.join('\n'));
  const l = capture();
  assert.equal(run(['--check', '--catalog', cat, '--out-dir', out], l), 1);
  assert.deepEqual(l.out.error, [
    `android-matrix.md: stale, first difference at line 1: expected "# Android capability matrix", found "# Hand edited" (${lines.length} expected lines, ${lines.length} found); run gen.mjs`,
  ]);
});

test('both stale pages are reported', () => {
  const { cat, out } = setup();
  run(['--catalog', cat, '--out-dir', out], capture());
  writeFileSync(join(out, 'android-matrix.md'), 'a\n');
  writeFileSync(join(out, 'ios-parity-matrix.md'), 'b\n');
  const l = capture();
  assert.equal(run(['--check', '--catalog', cat, '--out-dir', out], l), 1);
  assert.equal(l.out.error.length, 2);
  assert.match(l.out.error[0], /^ios-parity-matrix\.md: stale, /);
  assert.match(l.out.error[1], /^android-matrix\.md: stale, /);
});

test('an unreadable catalog gives exit 1 and a message with the path', () => {
  const { dir, out } = setup();
  const missing = join(dir, 'nope.json');
  const l = capture();
  assert.equal(run(['--catalog', missing, '--out-dir', out], l), 1);
  assert.equal(l.out.error.length, 1);
  assert.ok(l.out.error[0].startsWith(`cannot read the catalog ${missing}: `), l.out.error[0]);
  assert.match(l.out.error[0], /ENOENT/);
  assert.equal(existsSync(out), false);
});

test('a catalog that is not JSON gives exit 1 with the same message', () => {
  const { dir, out } = setup();
  const bad = join(dir, 'bad.json');
  writeFileSync(bad, '{not json');
  const l = capture();
  assert.equal(run(['--catalog', bad, '--out-dir', out], l), 1);
  assert.ok(l.out.error[0].startsWith(`cannot read the catalog ${bad}: `), l.out.error[0]);
});

test('catalog problems are listed one per line with the catalog prefix, and nothing is written', () => {
  const bad = fixture();
  bad[0].verdict = 'bogus';
  bad[1].verdict = 'bogus';
  const { cat, out } = setup(bad);
  const l = capture();
  assert.equal(run(['--catalog', cat, '--out-dir', out], l), 1);
  assert.ok(l.out.error.length >= 2, String(l.out.error.length));
  for (const m of l.out.error) assert.match(m, /^catalog: \S/);
  assert.equal(existsSync(out), false);
});

test('an iOS capability without a spike matrix item is reported with the page file', () => {
  const { cat, out } = setup([cap('CAP-I-99', 'ios', [res('27.0')], 'pending')]);
  const l = capture();
  assert.equal(run(['--catalog', cat, '--out-dir', out], l), 1);
  assert.deepEqual(l.out.error, ['ios-parity-matrix.md: CAP-I-99 has no spike matrix item: add it to IOS_ITEM_MAP']);
  assert.equal(existsSync(join(out, 'ios-parity-matrix.md')), false);
});

test('a too wide cell is reported with the file and line, exit 1, and the page is not written', () => {
  const { cat, out } = setup([cap('CAP-A-01', 'android', [res('37'), res('29'), res('API-level-37-wide')], 'supported')]);
  const l = capture();
  assert.equal(run(['--catalog', cat, '--out-dir', out], l), 1);
  assert.ok(l.out.error.length >= 1);
  for (const m of l.out.error) assert.match(m, /^android-matrix\.md: line \d+: cell "[^"]+" is longer than 14$/);
  assert.equal(existsSync(join(out, 'android-matrix.md')), false);
});

test('a too wide row is reported with the file', () => {
  const oses = ['aaaaaaaaaaaa', 'bbbbbbbbbbbb', 'cccccccccccc', 'dddddddddddd'];
  const { cat, out } = setup([cap('CAP-A-01', 'android', ['37', '29', ...oses].map((o) => res(o)), 'supported')]);
  const l = capture();
  assert.equal(run(['--catalog', cat, '--out-dir', out], l), 1);
  assert.ok(l.out.error.some((m) => /^android-matrix\.md: line \d+: row is longer than 60$/.test(m)), l.out.error.join('\n'));
});

test('diffSummary: names the first differing line and both lines', () => {
  assert.equal(
    diffSummary('a\nb\nc', 'a\nX\nc'),
    'first difference at line 2: expected "b", found "X" (3 expected lines, 3 found)',
  );
});

test('diffSummary: a shorter found page shows <end> for the found line', () => {
  assert.equal(diffSummary('a\nb', 'a'), 'first difference at line 2: expected "b", found "<end>" (2 expected lines, 1 found)');
});

test('diffSummary: a longer found page shows <end> for the expected line', () => {
  assert.equal(diffSummary('a', 'a\nb'), 'first difference at line 2: expected "<end>", found "b" (1 expected lines, 2 found)');
});

test('diffSummary: lines are cut at 80 characters', () => {
  const s = diffSummary(`x${'a'.repeat(100)}`, `y${'b'.repeat(100)}`);
  assert.ok(s.includes(`expected "x${'a'.repeat(79)}", found "y${'b'.repeat(79)}"`), s);
});

test('diffSummary: equal texts give "no difference"', () => {
  assert.equal(diffSummary('a\nb', 'a\nb'), 'no difference');
});

const exec = (args, cwd) => spawnSync(process.execPath, [GEN, ...args], { cwd, encoding: 'utf8' });

test('script: default catalog and out dir come from the repository root, whatever the working directory', () => {
  const r = exec(['--check'], mkdtempSync(join(tmpdir(), 'matrix-cwd-')));
  assert.equal(r.status, 0, r.stderr);
  assert.equal(r.stdout, 'matrix pages are current\n');
  assert.equal(r.stderr, '');
});

test('script: an unknown argument exits with code 2 and prints the usage to stderr', () => {
  const r = exec(['--nope'], tmpdir());
  assert.equal(r.status, 2);
  assert.equal(r.stderr, `${USAGE}\nunknown argument --nope\n`);
  assert.equal(r.stdout, '');
});

test('script: writes the pages when run without --check', () => {
  const { cat, out } = setup();
  const r = exec(['--catalog', cat, '--out-dir', out], tmpdir());
  assert.equal(r.status, 0, r.stderr);
  assert.equal(r.stdout, 'wrote ios-parity-matrix.md\nwrote android-matrix.md\n');
  assert.ok(existsSync(join(out, 'android-matrix.md')));
});

test('importing the module does not run the generator', () => {
  const r = spawnSync(process.execPath, ['--input-type=module', '-e', `import ${JSON.stringify(GEN)}; console.log('imported');`], { encoding: 'utf8', cwd: tmpdir() });
  assert.equal(r.status, 0, r.stderr);
  assert.equal(r.stdout, 'imported\n');
});
