// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

import assert from 'node:assert/strict';
import { mkdtempSync, readFileSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { test } from 'node:test';
import { run } from '../gen.mjs';
import { esc, renderAndroid, renderIos, widthProblems } from '../src/render.mjs';

const res = (os, verdict, extra = {}) => ({
  os,
  webview: null,
  verdict,
  condition: verdict === 'conditional' ? 'Only with <tag> blocks' : null,
  runs: ['evidence/r/run-1.jsonl'],
  recording: null,
  ...extra,
});
const imp = (os) => res(os, 'impossible', { runs: [], source: 'https://example.com/doc' });
const cap = (id, platform, results, verdict) => ({
  id, platform, title: `Title of ${id}`, prototype: 'P', questions: ['Q-1'], sprint: 'S1', results, verdict,
});
const fixture = () => [
  cap('CAP-A-01', 'android', [res('37', 'supported'), res('29', 'conditional')], 'conditional'),
  cap('CAP-I-01', 'ios', [res('27.0', 'supported'), res('26.5', 'supported'), res('18.5', 'supported')], 'supported'),
  cap('CAP-I-08', 'ios', [imp('27.0'), imp('26.5'), imp('18.5')], 'impossible'),
  cap('CAP-F-01', 'ffi', [res('host-jvm', 'supported', { kind: 'benchmark', runs: [], metrics: 'evidence/m.json', note: 'fast' })], 'supported'),
];
const silent = { log() {}, error() {} };
const capture = () => {
  const lines = [];
  return { lines, log: (m) => lines.push(m), error: (m) => lines.push(m) };
};
const section = (page, from, to) => page.split(from)[1].split(to)[0];

function setup(catalog = fixture()) {
  const dir = mkdtempSync(join(tmpdir(), 'matrix-'));
  const cat = join(dir, 'cat.json');
  writeFileSync(cat, JSON.stringify(catalog));
  return { cat, out: join(dir, 'out') };
}

test('ios page has the 8 items; items without a capability are marked not measured', () => {
  const page = renderIos(fixture());
  assert.equal((page.match(/^### Item \d:/gm) ?? []).length, 8);
  assert.match(section(page, '### Item 6', '###'), /not measured/);
  assert.match(section(page, '### Item 4', '###'), /CAP-I-08/);
  assert.doesNotMatch(section(page, '### Item 4', '###'), /not measured/);
  assert.match(page, /\| # \| ID \| 27\.0 \| 26\.5 \| 18\.5 \| Verdict \|/);
});

test('a capability of two items appears in both tables', () => {
  const c = fixture();
  c.push(cap('CAP-I-06', 'ios', [res('27.0', 'supported'), res('26.5', 'supported'), res('18.5', 'supported')], 'supported'));
  const page = renderIos(c);
  assert.match(section(page, '### Item 3', '###'), /CAP-I-06/);
  assert.match(section(page, '### Item 6', '###'), /CAP-I-06/);
  // Item 1 (TLS, redirects) has no iOS capability: not measured in WP-04.
  assert.match(section(page, '### Item 1', '###'), /not measured/);
});

test('an iOS capability without an item stops the generator', () => {
  const c = fixture();
  c.push(cap('CAP-I-99', 'ios', [res('27.0', 'supported')], 'pending'));
  assert.throws(() => renderIos(c), /CAP-I-99/);
});

test('shared FFI rows are on both pages, identical except the note number', () => {
  const a = renderAndroid(fixture());
  const i = renderIos(fixture());
  const sa = section(a, '## Shared rows', '## Notes');
  assert.match(sa, /CAP-F-01/);
  const strip = (s) => s.replace(/^\| \d+ \|/gm, '| n |');
  assert.equal(strip(sa), strip(section(i, '## Shared rows', '## Notes')));
});

test('android table has one column per API level and a verdict column', () => {
  const page = renderAndroid(fixture());
  assert.match(page, /\| # \| ID \| 37 \| 29 \| Verdict \|/);
  assert.match(page, /\| 1 \| CAP-A-01 \| <Badge type="tip" text="supported" \/> \| <Badge type="warning" text="conditional" \/> \| <Badge type="warning" text="conditional" \/> \|/);
});

test('notes are numbered, escape raw tags and link files to GitHub', () => {
  const page = renderAndroid(fixture());
  assert.match(page, /^1\. \*\*CAP-A-01\*\* Title of CAP-A-01\./m);
  assert.match(page, /Only with &lt;tag&gt; blocks/);
  assert.match(page, /\[run 1\]\(https:\/\/github\.com\/robotoss\/rivqen\/blob\/main\/evidence\/r\/run-1\.jsonl\)/);
  assert.match(page, /\[metrics\]\(https:\/\/github\.com\/robotoss\/rivqen\/blob\/main\/evidence\/m\.json\)/);
  assert.match(page, /Do not edit it by hand/);
  assert.match(renderIos(fixture()), /\[source\]\(https:\/\/example\.com\/doc\)/);
});

test('double curly braces are refused; code spans keep angle brackets', () => {
  assert.throws(() => esc('a {{ b }}'), /curly/);
  assert.equal(esc('x <y> `<z>`'), 'x &lt;y&gt; `<z>`');
});

test('375 px rule: generated tables fit; a long cell or row is reported', () => {
  for (const p of [renderAndroid(fixture()), renderIos(fixture())]) assert.deepEqual(widthProblems(p), []);
  assert.equal(widthProblems('| a | this cell is far too long |').length, 1);
  assert.equal(widthProblems('| aaaaaaaaaa | bbbbbbbbbb | cccccccccc |', 14, 20).length, 1);
  assert.deepEqual(widthProblems('|---|---|'), []);
});

test('--check passes after generation and fails with a summary when the catalog changed', () => {
  const { cat, out } = setup();
  assert.equal(run(['--catalog', cat, '--out-dir', out], silent), 0);
  assert.equal(run(['--check', '--catalog', cat, '--out-dir', out], silent), 0);
  const changed = fixture();
  changed[0].results[1] = imp('29');
  changed[0].verdict = 'impossible';
  writeFileSync(cat, JSON.stringify(changed));
  const log = capture();
  assert.equal(run(['--check', '--catalog', cat, '--out-dir', out], log), 1);
  assert.match(log.lines.join('\n'), /android-matrix\.md: stale, first difference at line \d+/);
});

test('--check fails for a missing page', () => {
  const { cat, out } = setup();
  const log = capture();
  assert.equal(run(['--check', '--catalog', cat, '--out-dir', out], log), 1);
  assert.match(log.lines.join('\n'), /missing/);
});

test('--check fails when a page was edited by hand', () => {
  const { cat, out } = setup();
  run(['--catalog', cat, '--out-dir', out], silent);
  const f = join(out, 'ios-parity-matrix.md');
  writeFileSync(f, `${readFileSync(f, 'utf8')}extra\n`);
  assert.equal(run(['--check', '--catalog', cat, '--out-dir', out], silent), 1);
});

test('a defective catalog gives exit 1 and a bad argument exit 2', () => {
  const bad = fixture();
  bad[0].verdict = 'bogus';
  const { cat, out } = setup(bad);
  assert.equal(run(['--catalog', cat, '--out-dir', out], silent), 1);
  assert.equal(run(['--nope'], silent), 2);
});
