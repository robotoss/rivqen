#!/usr/bin/env node
// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// Generator of the rqp/1 protocol fixtures (fixtures/rqp/protocol/README.md).
//
//   node tools/rqp-protocol/gen.mjs           write generated pages and expected files
//   node tools/rqp-protocol/gen.mjs --check   change nothing; exit 1 when a file is stale
//
// Generated files: pages/<id>.html of the page recipes (src/pages.mjs),
// FX-RQ-*/expected.json and, for client cases with a body, FX-RQ-*/response.body.
// Exit codes: 0 all files are current (or written), 1 a stale, missing or extra
// file or a defective case, 2 a usage error.

import { existsSync, readFileSync, readdirSync, writeFileSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { analyze } from './src/markup.mjs';
import { EXPECTED, GENERATED, KAT_PAGE, PAGES_DIR, PAGE_IDS, pageBytes } from './src/pages.mjs';
import { CaseError, FIXTURES_DIR, RESPONSE_BODY, expectFixture, fileBytes, fixtureNames } from './src/fixtures.mjs';

const GENERATED_NAMES = ['expected.json', RESPONSE_BODY];

/** Check the markup result of each page against EXPECTED. Returns a list of problems. */
export function checkPages(read = pageBytes) {
  const problems = [];
  for (const id of PAGE_IDS) {
    let bytes;
    try {
      bytes = read(id);
    } catch {
      problems.push(`page ${id}: missing`);
      continue;
    }
    const r = analyze(bytes);
    const want = EXPECTED[id];
    if (want.error !== undefined) {
      if (r.error !== want.error) problems.push(`page ${id}: expected ${want.error}, got ${r.error ?? 'valid'}`);
    } else if (!r.valid || r.blocks.map((b) => b.id).join(',') !== want.blocks.join(',')) {
      problems.push(`page ${id}: expected blocks [${want.blocks}], got ${r.error ?? r.blocks.map((b) => b.id)}`);
    }
  }
  if (!readFileSync(KAT_PAGE).equals(read('page-a'))) problems.push('page page-a: not equal to spec/rqp/examples/kat/page.html');
  for (const name of readdirSync(PAGES_DIR)) {
    if (!name.endsWith('.html') || !PAGE_IDS.includes(name.slice(0, -5))) problems.push(`pages/${name}: not a known page`);
  }
  return problems;
}

/** Compare or write one file. Returns a problem text or null. */
function sync(path, bytes, check) {
  const current = existsSync(path) ? readFileSync(path) : null;
  if (current !== null && current.equals(bytes)) return null;
  if (check) return `${path.slice(FIXTURES_DIR.length)}: ${current === null ? 'missing' : 'stale'}`;
  writeFileSync(path, bytes);
  return null;
}

/** Run the generator. Returns { problems, written } (written: fixtures processed). */
export function run({ check, dir = FIXTURES_DIR, log = () => {} }) {
  const problems = [];
  for (const [id, recipe] of Object.entries(GENERATED)) {
    const p = sync(join(PAGES_DIR, `${id}.html`), Buffer.from(recipe(), 'utf8'), check);
    if (p !== null) problems.push(p);
  }
  // Hand pages are read from disk; generated pages from the recipe (equal after the sync above).
  problems.push(...checkPages());
  let count = 0;
  for (const name of fixtureNames(dir)) {
    let files;
    try {
      ({ files } = expectFixture(dir, name));
    } catch (error) {
      if (!(error instanceof CaseError)) throw error;
      problems.push(error.message);
      continue;
    }
    for (const [file, value] of Object.entries(files)) {
      const p = sync(join(dir, name, file), fileBytes(value), check);
      if (p !== null) problems.push(p);
    }
    for (const file of GENERATED_NAMES) {
      if (!Object.hasOwn(files, file) && existsSync(join(dir, name, file))) problems.push(`${name}/${file}: not generated for this case`);
    }
    for (const file of readdirSync(join(dir, name))) {
      if (file !== 'case.json' && !GENERATED_NAMES.includes(file)) problems.push(`${name}/${file}: unknown file`);
    }
    count++;
    log(name);
  }
  return { problems, count };
}

function main(argv) {
  if (argv.length > 1 || (argv.length === 1 && argv[0] !== '--check')) {
    process.stderr.write('usage: node gen.mjs [--check]\n');
    return 2;
  }
  const check = argv[0] === '--check';
  const { problems, count } = run({ check });
  for (const p of problems) process.stderr.write(`rqp-protocol gen: ${p}\n`);
  if (problems.length > 0) {
    if (check) process.stderr.write('rqp-protocol gen: run "node tools/rqp-protocol/gen.mjs" and review the diff\n');
    return 1;
  }
  process.stdout.write(`rqp-protocol gen: ${count} fixtures ${check ? 'are current' : 'written'}\n`);
  return 0;
}

if (process.argv[1] !== undefined && fileURLToPath(import.meta.url) === resolve(process.argv[1])) {
  process.exitCode = main(process.argv.slice(2));
}
