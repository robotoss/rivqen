// SPDX-License-Identifier: Apache-2.0
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { parseDiff, filterFiles, format, toClassName } from '../diff-ranges.mjs';

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
  const r = parseDiff(DIFF);
  assert.deepEqual(r.get('src/a.ts'), [[11, 13], [43, 43]]);
});

test('drops test files and other extensions', () => {
  const r = filterFiles(parseDiff(DIFF), ['.ts']);
  assert.deepEqual([...r.keys()], ['src/a.ts']);
});

test('formats Stryker line ranges', () => {
  assert.equal(format(filterFiles(parseDiff(DIFF), ['.ts']), 'stryker'), 'src/a.ts:11-13,src/a.ts:43-43');
});

test('maps JVM paths to class names for PIT', () => {
  assert.equal(toClassName('sdk/x/src/main/kotlin/dev/rivqen/Engine.kt'), 'dev.rivqen.Engine');
  assert.equal(toClassName('src/test/java/dev/rivqen/CodecTest.java'), null);
  assert.equal(format(filterFiles(parseDiff(DIFF), ['.java']), 'pit-classes'), 'dev.rivqen.Codec');
});

test('rejects an unknown format', () => {
  assert.throws(() => format(new Map(), 'xml'), /unknown format/);
});
