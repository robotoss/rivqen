#!/usr/bin/env node
// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// Generator of the two WP-04 matrix pages (see README.md).
//
//   node tools/platform-spikes/matrix/gen.mjs           write the pages
//   node tools/platform-spikes/matrix/gen.mjs --check   change nothing; exit 1 when a page is stale
//
// Options: --catalog <file>, --out-dir <dir> (tests use them).
// Exit codes: 0 pages current (or written), 1 stale/missing page or a defective catalog, 2 usage error.

import { existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { catalogProblems } from '../check/src/catalog.mjs';
import { PAGES, widthProblems } from './src/render.mjs';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '../../..');

function parseArgs(argv) {
  const opts = {
    check: false,
    catalog: join(root, 'tools/platform-spikes/capabilities.json'),
    outDir: join(root, 'docs/engineering/platforms'),
  };
  for (let i = 0; i < argv.length; i++) {
    if (argv[i] === '--check') opts.check = true;
    else if (argv[i] === '--catalog' && argv[i + 1]) opts.catalog = resolve(argv[++i]);
    else if (argv[i] === '--out-dir' && argv[i + 1]) opts.outDir = resolve(argv[++i]);
    else throw new Error(`unknown argument ${argv[i]}`);
  }
  return opts;
}

/** First differing line of two texts, as a short summary. */
export function diffSummary(expected, found) {
  const a = expected.split('\n');
  const b = found.split('\n');
  const n = Math.max(a.length, b.length);
  for (let i = 0; i < n; i++) {
    if (a[i] !== b[i]) {
      const show = (s) => JSON.stringify((s ?? '<end>').slice(0, 80));
      return `first difference at line ${i + 1}: expected ${show(a[i])}, found ${show(b[i])} (${a.length} expected lines, ${b.length} found)`;
    }
  }
  return 'no difference';
}

export function run(argv, log = console) {
  let opts;
  try {
    opts = parseArgs(argv);
  } catch (e) {
    log.error(`usage: gen.mjs [--check] [--catalog <file>] [--out-dir <dir>]\n${e.message}`);
    return 2;
  }
  let catalog;
  try {
    catalog = JSON.parse(readFileSync(opts.catalog, 'utf8'));
  } catch (e) {
    log.error(`cannot read the catalog ${opts.catalog}: ${e.message}`);
    return 1;
  }
  const problems = catalogProblems(catalog);
  if (problems.length > 0) {
    problems.forEach((p) => log.error(`catalog: ${p}`));
    return 1;
  }
  let stale = false;
  for (const { file, render } of PAGES) {
    let text;
    try {
      text = render(catalog);
    } catch (e) {
      log.error(`${file}: ${e.message}`);
      return 1;
    }
    const wide = widthProblems(text);
    if (wide.length > 0) {
      wide.forEach((w) => log.error(`${file}: ${w}`));
      return 1;
    }
    const path = join(opts.outDir, file);
    if (opts.check) {
      if (!existsSync(path)) {
        log.error(`${file}: missing; run gen.mjs`);
        stale = true;
      } else {
        const found = readFileSync(path, 'utf8');
        if (found !== text) {
          log.error(`${file}: stale, ${diffSummary(text, found)}; run gen.mjs`);
          stale = true;
        }
      }
    } else {
      mkdirSync(opts.outDir, { recursive: true });
      writeFileSync(path, text);
      log.log(`wrote ${file}`);
    }
  }
  if (opts.check && !stale) log.log('matrix pages are current');
  return stale ? 1 : 0;
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  process.exitCode = run(process.argv.slice(2));
}
