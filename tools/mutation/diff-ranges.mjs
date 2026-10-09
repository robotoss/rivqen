#!/usr/bin/env node
// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors
//
// Turn the changed lines of a git diff into mutation-tool arguments, so that
// mutation testing runs only on new or changed code.
//
// Run it from the directory where the mutation tool runs (the repository root
// or a package directory). Paths are printed relative to the current
// directory, and only files below it are included.
//
// Usage:
//   node <repo>/tools/mutation/diff-ranges.mjs --base main --ext .ts,.tsx,.js,.mjs --format stryker
//   node <repo>/tools/mutation/diff-ranges.mjs --base main --ext .swift --format files
//   node <repo>/tools/mutation/diff-ranges.mjs --base main --ext .java,.kt --format pit-classes
//
// Formats:
//   stryker      src/a.ts:10-42,src/b.ts:7-7      (StrykerJS --mutate with line ranges)
//   files        Sources/A.swift,Sources/B.swift  (Muter --files-to-mutate)
//   pit-classes  dev.rivqen.Codec,dev.rivqen.Codec$*,...  (PIT targetClasses: the class,
//                                                  its inner classes and, for Kotlin,
//                                                  the file facade CodecKt)
//
// The diff is: merge-base(base, HEAD) → working tree. Committed, staged and
// unstaged changes to tracked files are included; renamed and copied files too.
// Test files, test source sets and tool configuration (*.config.*) are excluded. Deleted lines are ignored (nothing to mutate).
// Untracked files are not part of a git diff; the tool warns about them on stderr
// (`git add -N <file>` makes them visible).
// Exit code 0 with empty output means: no changed source lines.

import { execFileSync } from 'node:child_process';
import { realpathSync } from 'node:fs';
import { pathToFileURL } from 'node:url';

const TEST_PATH = /(^|\/)(test|tests|__tests__|androidTest|Tests)\/|(^|\/)src\/[A-Za-z]*[Tt]est[A-Za-z]*\/|\.(test|spec)\.[cm]?[jt]sx?$|Tests?\.(kt|java|swift)$/;
// Tool and build configuration is not product code (vite.config.ts, eslint.config.js, …).
const CONFIG_PATH = /(^|\/)[^/]*\.config\.[cm]?[jt]sx?$/;

/** Undo git's C-style quoting of a path ("a\303\251.ts" → "aé.ts"). */
export function unquotePath(raw) {
  if (!(raw.startsWith('"') && raw.endsWith('"'))) return raw;
  const bytes = [];
  const s = raw.slice(1, -1);
  for (let i = 0; i < s.length; i++) {
    if (s[i] !== '\\') {
      bytes.push(...Buffer.from(s[i], 'utf8'));
      continue;
    }
    const next = s[++i];
    if (/[0-7]/.test(next)) {
      bytes.push(parseInt(s.slice(i, i + 3), 8));
      i += 2;
    } else {
      bytes.push({ n: 10, t: 9, r: 13, '"': 34, '\\': 92, a: 7, b: 8, f: 12, v: 11 }[next] ?? next.charCodeAt(0));
    }
  }
  return Buffer.from(bytes).toString('utf8');
}

/** Parse `git diff -U0` output into Map(file → [[start, end], ...]) for added or changed lines. */
export function parseDiff(diffText) {
  const result = new Map();
  let file = null;
  let inHeader = false; // between "diff --git" and the first "@@" of that file
  for (const line of diffText.split('\n')) {
    if (line.startsWith('diff --git ')) {
      inHeader = true;
      file = null;
      continue;
    }
    if (inHeader && line.startsWith('+++ ')) {
      const path = unquotePath(line.slice(4).trim());
      file = path === '/dev/null' ? null : path.replace(/^b\//, '');
      continue;
    }
    const m = /^@@ -\d+(?:,\d+)? \+(\d+)(?:,(\d+))? @@/.exec(line);
    if (m) {
      inHeader = false;
      if (!file) continue;
      const start = Number(m[1]);
      const count = m[2] === undefined ? 1 : Number(m[2]);
      if (count === 0) continue; // pure deletion
      if (!result.has(file)) result.set(file, []);
      result.get(file).push([start, start + count - 1]);
    }
  }
  return result;
}

/** Keep source files with the given extensions; drop tests and tool configuration. */
export function filterFiles(ranges, exts) {
  const out = new Map();
  for (const [file, r] of ranges) {
    if (!exts.some((e) => file.endsWith(e))) continue;
    if (TEST_PATH.test(file) || CONFIG_PATH.test(file)) continue;
    out.set(file, r);
  }
  return out;
}

/**
 * Map a JVM source path to PIT class patterns, or null for test source sets.
 * "pkg/Codec.kt" → "pkg.Codec,pkg.Codec$*,pkg.CodecKt": the class, its inner
 * classes and lambdas, and the Kotlin file facade — not sibling classes.
 */
export function toClassGlob(file) {
  const m = /(?:^|\/)src\/([A-Za-z]+)\/(?:java|kotlin)\/(.+)\.(java|kt)$/.exec(file);
  if (!m || /[Tt]est/.test(m[1])) return null;
  const cls = m[2].replace(/\//g, '.');
  return m[3] === 'kt' ? `${cls},${cls}$*,${cls}Kt` : `${cls},${cls}$*`;
}

export function format(ranges, kind) {
  const files = [...ranges.keys()];
  const bad = files.filter((f) => f.includes(','));
  if (bad.length > 0) {
    // The tools split their lists on commas; such a path cannot be passed safely.
    throw new Error(`path contains a comma, cannot build a list: ${bad.join(' | ')}`);
  }
  switch (kind) {
    case 'stryker':
      return [...ranges].flatMap(([f, rs]) => rs.map(([a, b]) => `${f}:${a}-${b}`)).join(',');
    case 'files':
      return files.join(',');
    case 'pit-classes':
      return [...new Set(files.map(toClassGlob).filter(Boolean))].join(',');
    default:
      throw new Error(`unknown format: ${kind}`);
  }
}

function arg(name, fallback) {
  const i = process.argv.indexOf(`--${name}`);
  return i > 0 && process.argv[i + 1] ? process.argv[i + 1] : fallback;
}

function isMain() {
  if (!process.argv[1]) return false;
  try {
    return import.meta.url === pathToFileURL(realpathSync(process.argv[1])).href;
  } catch {
    return false;
  }
}

function git(args) {
  return execFileSync('git', ['-c', 'core.quotePath=false', ...args], { encoding: 'utf8', maxBuffer: 64 * 1024 * 1024 });
}

function main() {
  const base = arg('base', 'main');
  const exts = arg('ext', '').split(',').filter(Boolean);
  const kind = arg('format', 'files');
  if (exts.length === 0) {
    console.error('error: --ext is required, for example --ext .ts,.tsx');
    process.exit(2);
  }
  let diff;
  let untrackedRaw;
  try {
    const mergeBase = git(['merge-base', base, 'HEAD']).trim();
    // Fixed prefixes override diff.noprefix / diff.mnemonicPrefix; --relative limits
    // the diff to the current directory and prints paths relative to it.
    diff = git(['diff', '-U0', '--no-color', '--no-ext-diff', '--relative', '--src-prefix=a/', '--dst-prefix=b/',
      '--find-renames', '--find-copies', '--diff-filter=ACMR', mergeBase]);
    untrackedRaw = git(['ls-files', '--others', '--exclude-standard']);
  } catch (err) {
    console.error(`error: cannot read git diff against ${base}: ${err.message}`);
    process.exit(2);
  }
  const untracked = untrackedRaw.split('\n').filter((f) => f && exts.some((e) => f.endsWith(e)) && !TEST_PATH.test(f));
  if (untracked.length > 0) {
    console.error(`warning: ${untracked.length} untracked source file(s) are not included; run \`git add -N <file>\` or commit them: ${untracked.join(', ')}`);
  }
  try {
    process.stdout.write(format(filterFiles(parseDiff(diff), exts), kind) + '\n');
  } catch (err) {
    console.error(`error: ${err.message}`);
    process.exit(2);
  }
}

if (isMain()) main();
