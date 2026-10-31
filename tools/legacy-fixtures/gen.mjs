#!/usr/bin/env node
// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// Generator of the legacy golden fixtures (fixtures/legacy/README.md).
//
//   node tools/legacy-fixtures/gen.mjs           write expected.json of every fixture
//   node tools/legacy-fixtures/gen.mjs --check   change nothing; exit 1 when a check fails
//   ... --dir <path>                             another fixture directory in the same format
//                                                (fixtures/malformed/legacy): its own pages/ directory;
//                                                no catalog size, no rule coverage, no stored traces,
//                                                no lab page copies
//
// Checks (README "Generator checks"):
//   1. every expected.json equals the regenerated one; no unknown or missing file
//   2. every trace-backed profile response equals its stored trace (a fixture with
//      not_reproduced: the trace shows the documented difference); the request equals the
//      trace request; every stored trace is used
//   3. OVERRIDE_NOT_NEEDED: a profile fixture without not_reproduced equal to the canonical response
//   4. OVERRIDE_MISSING: a trace differs from the canonical response and has no profile fixture
//   5. every rule of a fixture exists in legacy wire section 10; every rule is covered once
//      the catalog is complete
// Exit codes: 0 all current (or written), 1 a stale, missing or extra file or a failed
// check, 2 a usage error.

import { existsSync, readFileSync, readdirSync, statSync, writeFileSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { isDeepStrictEqual } from 'node:util';
import { fileBytes } from '../rqp-protocol/src/fixtures.mjs';
import { CaseError, FIXTURES_DIR, PROFILE_SUFFIX, REPO_ROOT, UPSTREAM_PROFILES, expectedOf, loadCase, pageFile, resolvedRequest } from './src/fixture.mjs';
import { TraceError, compareNotReproduced, compareToTrace, parseTrace, sameRequest } from './src/trace.mjs';

/** Number of canonical fixtures of the complete catalog (fixtures/legacy/README.md). */
export const CATALOG_SIZE = 59;

const GENERATED = 'expected.json';
const TRACE_SERVERS = ['java', 'node', 'php'];

function sync(path, bytes, check, label) {
  const current = existsSync(path) ? readFileSync(path) : null;
  if (current !== null && current.equals(bytes)) return null;
  if (check) return `${label}: ${current === null ? 'missing' : 'stale'}`;
  writeFileSync(path, bytes);
  return null;
}

function readTrace(root, path, id, problems) {
  try {
    return parseTrace(readFileSync(join(root, path)));
  } catch (error) {
    if (error instanceof TraceError || error?.code === 'ENOENT') {
      problems.push(`${id}: trace ${path}: ${error.message}`);
      return null;
    }
    throw error;
  }
}

function responseOf(expected) {
  return { status: expected.status, fields: expected.fields, body: expected.body };
}

/** The repository-relative paths of all stored server traces. */
export function storedTraces(root) {
  const out = [];
  for (const server of TRACE_SERVERS) {
    const dir = join(root, 'evidence/traces/server', server);
    if (!existsSync(dir)) continue;
    for (const name of readdirSync(dir).sort()) if (name.endsWith('.http')) out.push(`evidence/traces/server/${server}/${name}`);
  }
  return out;
}

// `labPages` null: the pages need not be copies of the upstream lab pages.
function checkPages(pagesDir, labPages, problems) {
  if (!existsSync(pagesDir)) {
    problems.push('pages: directory missing');
    return;
  }
  for (const name of readdirSync(pagesDir).sort()) {
    if (!name.endsWith('.html')) {
      problems.push(`pages/${name}: unknown file`);
      continue;
    }
    const lab = labPages === null ? null : join(labPages, name);
    if (lab !== null && existsSync(lab) && !readFileSync(lab).equals(readFileSync(join(pagesDir, name)))) {
      problems.push(`pages/${name}: not a byte copy of tools/upstream-lab/pages/${name}`);
    }
  }
}

/**
 * Run the generator on a fixture directory. Returns { problems, notes, count }
 * (count: fixtures processed). `coverage` false: the rules of a case must exist, but no
 * rule needs a case (a directory that is not the catalog, such as fixtures/malformed/legacy).
 */
export function run({
  check,
  dir = FIXTURES_DIR,
  pagesDir = join(dir, 'pages'),
  root = REPO_ROOT,
  wirePage = join(root, 'docs/engineering/protocol/legacy-wire.md'),
  labPages = join(root, 'tools/upstream-lab/pages'),
  traces = storedTraces(root),
  catalogSize = CATALOG_SIZE,
  catalog = dir === FIXTURES_DIR,
  coverage = true,
  log = () => {},
}) {
  const problems = [];
  const notes = [];
  checkPages(pagesDir, labPages, problems);

  const names = [];
  for (const entry of readdirSync(dir).sort()) {
    if (entry === 'pages' || entry === 'README.md') continue;
    if (entry.startsWith('FX-LEG-') && statSync(join(dir, entry)).isDirectory()) names.push(entry);
    else problems.push(`${entry}: unknown entry`);
  }

  const cases = new Map();
  for (const name of names) {
    try {
      cases.set(name, loadCase(dir, name));
    } catch (error) {
      if (!(error instanceof CaseError)) throw error;
      problems.push(error.message);
    }
  }

  const expectedOfName = new Map();
  const used = new Set();
  const canonical = [...cases.values()].filter((c) => c.kind === 'canonical');
  for (const c of canonical) {
    try {
      expectedOfName.set(c.id, canonicalExpected(c));
    } catch (error) {
      if (!(error instanceof CaseError)) throw error;
      problems.push(error.message);
    }
  }

  function canonicalExpected(c) {
    const bytes = pageFile(c.id, pagesDir, c.page);
    const expected = expectedOf(c, 'legacy-canonical', bytes, pagesDir);
    expected.profiles_same = [];
    if (c.evidence !== 'trace') return expected;
    for (const profile of UPSTREAM_PROFILES) {
      const path = c.traces[profile];
      used.add(path);
      const trace = readTrace(root, path, c.id, problems);
      if (trace === null) continue;
      const diffs = compareToTrace(expected, trace, bytes);
      if (diffs.length === 0) {
        expected.profiles_same.push(profile);
        if (!sameRequest(resolvedRequest(c, profile, pagesDir), trace)) {
          problems.push(`${c.id}: the request under ${profile} differs from the request of ${path}`);
        }
      } else if (!cases.has(`${c.id}--${PROFILE_SUFFIX[profile]}`)) {
        problems.push(`${c.id}: OVERRIDE_MISSING for ${profile}: ${diffs[0]}`);
      }
    }
    return expected;
  }

  for (const c of cases.values()) {
    if (c.kind !== 'profile') continue;
    try {
      expectedOfName.set(c.id, profileExpected(c));
    } catch (error) {
      if (!(error instanceof CaseError)) throw error;
      problems.push(error.message);
    }
  }

  function profileExpected(p) {
    const base = cases.get(p.replaces);
    if (base === undefined || base.kind !== 'canonical') throw new CaseError(p.id, `replaces ${p.replaces}, which is not a canonical fixture`);
    if (base.evidence !== 'trace' || base.traces[p.profile] !== p.trace) {
      throw new CaseError(p.id, `trace must be the trace of ${p.replaces} for ${p.profile}`);
    }
    const bytes = pageFile(p.id, pagesDir, base.page);
    const expected = expectedOf({ ...base, id: p.id }, p.profile, bytes, pagesDir);
    used.add(p.trace);
    const trace = readTrace(root, p.trace, p.id, problems);
    if (trace !== null) {
      const diffs = p.not_reproduced === undefined ? compareToTrace(expected, trace, bytes) : compareNotReproduced(p.not_reproduced, expected, trace, bytes, base.app.status);
      for (const d of diffs) problems.push(`${p.id}: ${d} (${p.trace})`);
      // U-11: the profile resolves {{tag:<page>}} with the safe title rule, so the request
      // cannot equal the request that the upstream server got.
      if (p.not_reproduced !== 'U-11' && !sameRequest(expected.request, trace)) {
        problems.push(`${p.id}: the request differs from the request of ${p.trace}`);
      }
    }
    // A fixture with not_reproduced exists because its trace differs from the canonical
    // response (check 2 proves it); it can expect the canonical response (U-13: C-01 scope).
    const canon = expectedOfName.get(p.replaces);
    if (p.not_reproduced === undefined && canon !== undefined && isDeepStrictEqual(responseOf(expected), responseOf(canon))) {
      problems.push(`${p.id}: OVERRIDE_NOT_NEEDED: the expected response equals ${p.replaces}`);
    }
    expected.trace = p.trace;
    return expected;
  }

  let count = 0;
  for (const name of names) {
    const expected = expectedOfName.get(name);
    if (expected === undefined) continue;
    const p = sync(join(dir, name, GENERATED), fileBytes(expected), check, `${name}/${GENERATED}`);
    if (p !== null) problems.push(p);
    for (const file of readdirSync(join(dir, name))) {
      if (file !== 'case.json' && file !== GENERATED) problems.push(`${name}/${file}: unknown file`);
    }
    count++;
    log(name);
  }

  for (const path of traces) if (!used.has(path)) problems.push(`${path}: stored trace is not used by any fixture`);

  checkRules(canonical, wirePage, { catalogSize, catalog, coverage }, problems, notes);
  return { problems, notes, count };
}

// `catalog`: the repository catalog (fixtures/legacy). It must keep at least catalogSize
// canonical fixtures, so that removing fixtures cannot switch the rule coverage off.
function checkRules(canonical, wirePage, { catalogSize, catalog, coverage }, problems, notes) {
  const known = new Set([...readFileSync(wirePage, 'utf8').matchAll(/^\| (C-\d\d) /gm)].map((m) => m[1]));
  if (known.size === 0) problems.push(`${wirePage}: no rule C-nn found`);
  const covered = new Set();
  for (const c of canonical) {
    for (const rule of c.rules) {
      if (known.has(rule)) covered.add(rule);
      else problems.push(`${c.id}: rule ${rule} is not in legacy wire section 10`);
    }
  }
  if (!coverage) return;
  const missing = [...known].filter((r) => !covered.has(r));
  if (catalog && canonical.length < catalogSize) {
    problems.push(`the catalog has ${canonical.length} canonical fixtures, fewer than ${catalogSize} (CATALOG_SIZE)`);
  }
  if (catalog || canonical.length >= catalogSize) {
    for (const rule of missing) problems.push(`rule ${rule} is not covered by any fixture`);
  } else if (missing.length > 0) {
    notes.push(`rule coverage is checked at ${catalogSize} canonical fixtures (now ${canonical.length}); not covered yet: ${missing.join(', ')}`);
  }
}

const USAGE = 'usage: node gen.mjs [--check] [--dir <path>]\n';

/** Parse the arguments. Returns { check, dir } (dir null: the default) or null on a usage error. */
function parseArgs(argv) {
  let check = false;
  let dir = null;
  for (let i = 0; i < argv.length; i++) {
    if (argv[i] === '--check' && !check) {
      check = true;
    } else if (argv[i] === '--dir' && dir === null && i + 1 < argv.length && argv[i + 1] !== '') {
      dir = argv[++i];
    } else {
      return null;
    }
  }
  return { check, dir };
}

/** The command line. Returns the exit code. `io` is for tests: { stdout, stderr, ...options of run }. */
export function main(argv, io = {}) {
  const { stdout = process.stdout, stderr = process.stderr, ...options } = io;
  const args = parseArgs(argv);
  if (args === null) {
    stderr.write(USAGE);
    return 2;
  }
  let extra = {};
  if (args.dir !== null) {
    const dir = resolve(args.dir);
    if (!existsSync(dir) || !statSync(dir).isDirectory()) {
      stderr.write(`legacy-fixtures gen: ${args.dir}: not a directory\n`);
      return 2;
    }
    extra = { dir };
    // Another directory than the catalog: it has its own pages and no stored traces.
    if (dir !== FIXTURES_DIR) extra = { dir, catalog: false, coverage: false, labPages: null, traces: [] };
  }
  const { check } = args;
  const { problems, notes, count } = run({ check, ...extra, ...options });
  for (const n of notes) stderr.write(`legacy-fixtures gen: note: ${n}\n`);
  for (const p of problems) stderr.write(`legacy-fixtures gen: ${p}\n`);
  if (problems.length > 0) {
    if (check) {
      const dirOption = args.dir === null ? '' : ` --dir ${args.dir}`;
      stderr.write(`legacy-fixtures gen: run "node tools/legacy-fixtures/gen.mjs${dirOption}" and review the diff\n`);
    }
    return 1;
  }
  stdout.write(`legacy-fixtures gen: ${count} fixtures ${check ? 'are current' : 'written'}\n`);
  return 0;
}

if (process.argv[1] !== undefined && fileURLToPath(import.meta.url) === resolve(process.argv[1])) {
  process.exitCode = main(process.argv.slice(2));
}
