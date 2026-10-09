// SPDX-License-Identifier: Apache-2.0
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { mkdtempSync, mkdirSync, writeFileSync, symlinkSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { parseDiff, filterFiles, format, toClassGlob, unquotePath } from '../diff-ranges.mjs';

const TOOL = join(dirname(fileURLToPath(import.meta.url)), '..', 'diff-ranges.mjs');

const DIFF = `diff --git a/src/a.ts b/src/a.ts
--- a/src/a.ts
+++ b/src/a.ts
@@ -10,0 +11,3 @@ fn
+x
+y
+z
@@ -40,2 +43 @@ fn
-old
-old
+new
@@ -60,2 +62,0 @@ fn
-gone
-gone
diff --git a/src/a.test.ts b/src/a.test.ts
--- a/src/a.test.ts
+++ b/src/a.test.ts
@@ -1,0 +2,1 @@
+t
diff --git a/server/java/src/main/java/dev/rivqen/Codec.java b/server/java/src/main/java/dev/rivqen/Codec.java
--- /dev/null
+++ b/server/java/src/main/java/dev/rivqen/Codec.java
@@ -0,0 +1,5 @@
+a
`;

test('collects added and changed line ranges, ignores pure deletions', () => {
  assert.deepEqual(parseDiff(DIFF).get('src/a.ts'), [[11, 13], [43, 43]]);
});

test('drops test files and other extensions', () => {
  assert.deepEqual([...filterFiles(parseDiff(DIFF), ['.ts']).keys()], ['src/a.ts']);
});

test('formats Stryker line ranges', () => {
  assert.equal(format(filterFiles(parseDiff(DIFF), ['.ts']), 'stryker'), 'src/a.ts:11-13,src/a.ts:43-43');
});

test('an added line that starts with "++ " is content, not a file header', () => {
  const diff = `diff --git a/c.ts b/c.ts
--- a/c.ts
+++ b/c.ts
@@ -1,0 +2 @@
+++ counter;
@@ -9,0 +11,2 @@
+a
+b
`;
  assert.deepEqual(parseDiff(diff).get('c.ts'), [[2, 2], [11, 12]]);
  assert.equal(parseDiff(diff).size, 1);
});

test('unquotes C-style quoted paths', () => {
  assert.equal(unquotePath('"b/src/\\303\\251.ts"'), 'b/src/é.ts');
  assert.equal(unquotePath('b/plain.ts'), 'b/plain.ts');
  const diff = `diff --git "a/src/\\303\\251.ts" "b/src/\\303\\251.ts"\n--- "a/src/\\303\\251.ts"\n+++ "b/src/\\303\\251.ts"\n@@ -1,0 +1 @@\n+x\n`;
  assert.deepEqual([...parseDiff(diff).keys()], ['src/é.ts']);
});

test('maps JVM paths to PIT class globs, including KMP source sets', () => {
  assert.equal(toClassGlob('sdk/x/src/main/kotlin/dev/rivqen/Engine.kt'), 'dev.rivqen.Engine*');
  assert.equal(toClassGlob('sdk/x/src/jvmMain/kotlin/dev/rivqen/Io.kt'), 'dev.rivqen.Io*');
  assert.equal(toClassGlob('src/test/java/dev/rivqen/CodecTest.java'), null);
  assert.equal(format(filterFiles(parseDiff(DIFF), ['.java']), 'pit-classes'), 'dev.rivqen.Codec*');
});

test('rejects an unknown format', () => {
  assert.throws(() => format(new Map(), 'xml'), /unknown format/);
});

test('CLI: path with a space, symlinked tool, renamed file, package-relative output', (t) => {
  const root = mkdtempSync(join(tmpdir(), 'rq mut '));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  const repo = join(root, 'repo dir');
  const sh = (args, cwd = repo) => execFileSync('git', args, { cwd, encoding: 'utf8' });
  mkdirSync(join(repo, 'pkg', 'src'), { recursive: true });
  sh(['init', '-q', '-b', 'main']);
  sh(['config', 'user.email', 't@example.invalid']);
  sh(['config', 'user.name', 'T']);
  sh(['config', 'diff.mnemonicPrefix', 'true']);
  const body = Array.from({ length: 20 }, (_, i) => `export const v${i} = ${i};`).join('\n') + '\n';
  writeFileSync(join(repo, 'pkg', 'src', 'old.ts'), body);
  sh(['add', '.']);
  sh(['commit', '-q', '-m', 'base']);
  sh(['checkout', '-q', '-b', 'work']);
  sh(['mv', 'pkg/src/old.ts', 'pkg/src/new.ts']);
  writeFileSync(join(repo, 'pkg', 'src', 'new.ts'), body.replace('v4 = 4', 'v4 = 40'));
  sh(['commit', '-q', '-am', 'rename and edit']);

  const link = join(root, 'tool-link.mjs');
  symlinkSync(TOOL, link);
  const run = (cwd) => execFileSync('node', [link, '--base', 'main', '--ext', '.ts', '--format', 'stryker'], { cwd, encoding: 'utf8' }).trim();
  assert.equal(run(repo), 'pkg/src/new.ts:5-5');
  assert.equal(run(join(repo, 'pkg')), 'src/new.ts:5-5');

  assert.throws(() => execFileSync('node', [link, '--base', 'main'], { cwd: repo, stdio: 'pipe' }), /--ext is required/);
});
