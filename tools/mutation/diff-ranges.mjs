#!/usr/bin/env node
// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors
//
// Turn the changed lines of a git diff into mutation-tool arguments, so that
// mutation testing runs only on new or changed code.
//
// Usage:
//   node tools/mutation/diff-ranges.mjs --base main --ext .ts,.tsx --format stryker
//   node tools/mutation/diff-ranges.mjs --base main --ext .swift --format files
//   node tools/mutation/diff-ranges.mjs --base main --ext .java,.kt --format pit-classes
//
// Formats:
//   stryker      src/a.ts:10-42,src/b.ts:7-7      (StrykerJS --mutate with line ranges)
//   files        Sources/A.swift,Sources/B.swift  (Muter --files-to-mutate)
//   pit-classes  dev.rivqen.server.Codec,...      (PIT targetClasses; whole classes)
//
// Test files are excluded. Deleted lines are ignored (nothing to mutate).
// Exit code 0 with empty output means: no changed source lines.
// Untracked files are not part of a git diff; the tool warns about them on stderr.

import { execFileSync } from 'node:child_process';

const TEST_PATH = /(^|\/)(test|tests|__tests__|androidTest|Tests)\/|\.(test|spec)\.[cm]?[jt]sx?$|Tests?\.(kt|java|swift)$/;

/** Parse `git diff -U0` output into { file: [[start, end], ...] } for added/changed lines. */
export function parseDiff(diffText) {
  const result = new Map();
  let file = null;
  for (const line of diffText.split('\n')) {
    if (line.startsWith('+++ ')) {
      const path = line.slice(4).trim();
      file = path === '/dev/null' ? null : path.replace(/^b\//, '');
      continue;
    }
    const m = /^@@ -\d+(?:,\d+)? \+(\d+)(?:,(\d+))? @@/.exec(line);
    if (m && file) {
      const start = Number(m[1]);
      const count = m[2] === undefined ? 1 : Number(m[2]);
      if (count === 0) continue; // pure deletion
      if (!result.has(file)) result.set(file, []);
      result.get(file).push([start, start + count - 1]);
    }
  }
  return result;
}

/** Keep source files with the given extensions; drop tests. */
export function filterFiles(ranges, exts) {
  const out = new Map();
  for (const [file, r] of ranges) {
    if (!exts.some((e) => file.endsWith(e))) continue;
    if (TEST_PATH.test(file)) continue;
    out.set(file, r);
  }
  return out;
}

/** Map a JVM source path to a fully qualified class name, or null. */
export function toClassName(file) {
  const m = /(?:^|\/)src\/main\/(?:java|kotlin)\/(.+)\.(?:java|kt)$/.exec(file);
  return m ? m[1].replace(/\//g, '.') : null;
}

export function format(ranges, kind) {
  const files = [...ranges.keys()];
  switch (kind) {
    case 'stryker':
      return [...ranges].flatMap(([f, rs]) => rs.map(([a, b]) => `${f}:${a}-${b}`)).join(',');
    case 'files':
      return files.join(',');
    case 'pit-classes':
      return [...new Set(files.map(toClassName).filter(Boolean))].join(',');
    default:
      throw new Error(`unknown format: ${kind}`);
  }
}

function arg(name, fallback) {
  const i = process.argv.indexOf(`--${name}`);
  return i > 0 && process.argv[i + 1] ? process.argv[i + 1] : fallback;
}

if (import.meta.url === `file://${process.argv[1]}`) {
  const base = arg('base', 'main');
  const exts = arg('ext', '').split(',').filter(Boolean);
  const kind = arg('format', 'files');
  if (exts.length === 0) {
    console.error('error: --ext is required, for example --ext .ts,.tsx');
    process.exit(2);
  }
  let diff;
  try {
    const mergeBase = execFileSync('git', ['merge-base', base, 'HEAD'], { encoding: 'utf8' }).trim();
    diff = execFileSync('git', ['diff', '-U0', '--no-color', '--diff-filter=AM', mergeBase], { encoding: 'utf8', maxBuffer: 64 * 1024 * 1024 });
  } catch (err) {
    console.error(`error: cannot read git diff against ${base}: ${err.message}`);
    process.exit(2);
  }
  // Untracked files are not in `git diff`. Warn so that new code is not skipped silently.
  const untracked = execFileSync('git', ['ls-files', '--others', '--exclude-standard'], { encoding: 'utf8' })
    .split('\n').filter((f) => f && exts.some((e) => f.endsWith(e)) && !TEST_PATH.test(f));
  if (untracked.length > 0) {
    console.error(`warning: ${untracked.length} untracked source file(s) are not included; run \`git add -N <file>\` or commit them: ${untracked.join(', ')}`);
  }
  process.stdout.write(format(filterFiles(parseDiff(diff), exts), kind) + '\n');
}
