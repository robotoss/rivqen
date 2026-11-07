// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// The client part of the generator (fixtures/legacy/README.md "Generator checks of the client
// part", checks 6 to 12): it writes the expected.json of every client fixture and the function
// fixtures, and checks them against the stored client evidence. gen.mjs calls clientPart().
//
// Every problem is a text that starts with the fixture ID or the file. A defect of a case or
// of a stored file is a problem, never an exception; an I/O error other than a missing file
// propagates (the generator must not hide it).

import { existsSync, readFileSync, readdirSync, statSync } from 'node:fs';
import { join } from 'node:path';
import { isDeepStrictEqual } from 'node:util';
import { fileBytes } from '../../rqp-protocol/src/fixtures.mjs';
import { ClientCaseError, validateClientCase } from './client-case.mjs';
import { run as runModel } from './client-model.mjs';
import { compareClient, parseRun, projectRuns, TraceProjectionError, traceSteps } from './client-trace.mjs';
import { FUNCTION_PROFILES, functionFixture, parseCases, parseEvidence } from './functions.mjs';

export const GENERATED = 'expected.json';
const MAX_RUNS = 64;

/** True when a directory holds a client case.json (`kind: "client"`). */
export function isClientDir(dir, name) {
  try {
    const c = JSON.parse(readFileSync(join(dir, name, 'case.json'), 'utf8'));
    return c !== null && typeof c === 'object' && c.kind === 'client';
  } catch {
    return false;
  }
}

/** The CL rules of legacy client section 9: Map of ID to true when the last column says "Fixture". */
export function clientRules(page) {
  const rules = new Map();
  for (const m of readFileSync(page, 'utf8').matchAll(/^\| (CL-\d\d) \|.*\| ([^|]*?) \|\s*$/gm)) rules.set(m[1], m[2].trim() === 'Fixture');
  return rules;
}

/** The stored scenario modes of the Android evidence: [{ scenario, mode, dir }]. */
export function storedModes(evidenceDir) {
  const base = join(evidenceDir, 'android');
  if (!existsSync(base)) return [];
  const out = [];
  for (const scenario of readdirSync(base).sort()) {
    if (!/^CS-\d\d[a-z]?$/.test(scenario) || !statSync(join(base, scenario)).isDirectory()) continue;
    for (const mode of ['quick', 'standard']) {
      const dir = join(base, scenario, mode);
      if (existsSync(dir)) out.push({ scenario, mode, dir });
    }
  }
  return out;
}

function loadRuns(dir) {
  const names = existsSync(dir) ? readdirSync(dir).filter((f) => /^run-\d+\.jsonl$/.test(f)) : [];
  names.sort((a, b) => Number(a.match(/\d+/)[0]) - Number(b.match(/\d+/)[0]));
  return names.slice(0, MAX_RUNS).map((f) => parseRun(readFileSync(join(dir, f))));
}

/** The scenarios that have no client fixture by design, with the reason (README check 11). */
function exemptScenarios(scenariosFile) {
  const out = new Map();
  if (!existsSync(scenariosFile)) return out;
  for (const s of JSON.parse(readFileSync(scenariosFile, 'utf8'))) if (s?.webview === true) out.set(s.id, 'WebView smoke run (DL-100): no client fixture');
  return out;
}

/**
 * The client part of one generator run.
 *
 * @param {object} o
 * @param {string} o.dir fixture directory
 * @param {string[]} o.names the client fixture directory names
 * @param {(id: string) => Uint8Array | null} o.page page bytes
 * @param {(id: string) => object | null} o.serverFixture a canonical server case (for `responses.fixture`)
 * @param {string} o.evidenceDir evidence/traces/client
 * @param {string} o.clientPage docs/engineering/protocol/legacy-client.md
 * @param {string} o.labFunctions tools/upstream-lab/client/functions.json
 * @param {string} o.scenarios tools/upstream-lab/client/scenarios.json
 * @param {boolean} o.coverage the catalog checks (fixtures/legacy): the floor, then rule coverage and stored-run use
 * @param {number} o.floor the number of canonical client fixtures that the catalog must keep
 *   (CLIENT_FLOOR of gen.mjs); 0: none yet, so checks 10 (coverage) and 11 (stored runs) give notes
 * @param {(path: string, bytes: Buffer, label: string) => string | null} o.sync write or check one file
 * @returns {{ problems: string[], notes: string[], count: number }}
 */
export function clientPart(o) {
  const problems = [];
  const notes = [];
  const cases = new Map();
  for (const name of o.names) {
    try {
      cases.set(name, validateClientCase(name, JSON.parse(readFileSync(join(o.dir, name, 'case.json'), 'utf8'))));
    } catch (error) {
      if (error instanceof ClientCaseError) problems.push(`${name}: ${error.message}`);
      else if (error instanceof SyntaxError) problems.push(`${name}: case.json is not JSON`);
      else throw error;
    }
  }
  const env = { page: o.page, fixture: o.serverFixture };
  const expectedOf = new Map();
  const traceOf = new Map();
  const used = new Map();

  const traceClient = (c) => {
    if (traceOf.has(c.id)) return traceOf.get(c.id);
    let runs;
    try {
      runs = loadRuns(join(o.evidenceDir, 'android', c.trace.scenario, c.trace.mode));
    } catch (error) {
      if (!(error instanceof TraceProjectionError)) throw error;
      traceOf.set(c.id, { error: `a stored run of ${c.trace.scenario} ${c.trace.mode}: ${error.message}` });
      return traceOf.get(c.id);
    }
    let out;
    if (runs.length === 0) out = { error: `no stored run for ${c.trace.scenario} ${c.trace.mode}` };
    else {
      const want = c.schedule.filter((s) => s.do !== undefined);
      const bad = runs.findIndex((events) => !isDeepStrictEqual(traceSteps(events), want));
      out = bad >= 0 ? { error: `the steps differ from the steps of run ${bad + 1} of ${c.trace.scenario} ${c.trace.mode}` } : projectRuns(runs, { url: c.url, account: c.config.account });
    }
    traceOf.set(c.id, out);
    return out;
  };

  for (const c of cases.values()) {
    if (c.kind !== 'client-canonical') continue;
    const r = runModel('legacy-canonical', c.mode, c.raw, env);
    if (!r.ok) {
      problems.push(`${c.id}: ${r.error.code}: ${r.error.message}`);
      continue;
    }
    const expected = { id: c.id, kind: 'client', profile: c.profile, mode: c.mode, server_profile: c.server_profile, level: c.level, client: r.client, profiles_same: [] };
    if (c.evidence === 'trace') {
      const key = `${c.trace.scenario} ${c.trace.mode}`;
      if (used.has(key)) problems.push(`${c.id}: the stored runs of ${key} are already the trace of ${used.get(key)}`);
      used.set(key, c.id);
      const t = traceClient(c);
      if (t.error !== undefined) problems.push(`${c.id}: check 7: ${t.error}`);
      else if (compareClient(r.client, t.client) === null) expected.profiles_same.push('legacy-android');
      else if (!cases.has(`${c.id}--ANDROID`)) problems.push(`${c.id}: OVERRIDE_MISSING for legacy-android: ${compareClient(r.client, t.client)}`);
    }
    expectedOf.set(c.id, expected);
  }

  for (const p of cases.values()) {
    if (p.kind !== 'client-profile') continue;
    const base = cases.get(p.replaces);
    if (base === undefined || base.kind !== 'client-canonical' || base.evidence !== 'trace') {
      problems.push(`${p.id}: replaces ${p.replaces}, which is not a trace-backed canonical client fixture`);
      continue;
    }
    const r = runModel('legacy-android', base.mode, base.raw, env);
    if (!r.ok) {
      problems.push(`${p.id}: ${r.error.code}: ${r.error.message}`);
      continue;
    }
    const t = traceClient(base);
    if (t.error !== undefined) problems.push(`${p.id}: check 7: ${t.error}`);
    else {
      // DL-120: with not_reproduced "U-13", the U-13 fields of a response outside the scope are not compared, and the trace must have them.
      const d = compareClient(r.client, t.client, { notReproduced: p.notReproduced });
      if (d !== null) problems.push(`${p.id}: check 7: the legacy-android result differs from the trace: ${d}`);
    }
    const canon = expectedOf.get(base.id);
    if (canon !== undefined && isDeepStrictEqual(canon.client, r.client)) problems.push(`${p.id}: OVERRIDE_NOT_NEEDED: the client member equals ${base.id}`);
    expectedOf.set(p.id, { id: p.id, kind: 'client', profile: p.profile, mode: base.mode, server_profile: base.server_profile, level: base.level, client: r.client, trace: base.trace });
  }

  let count = 0;
  for (const name of o.names) {
    const expected = expectedOf.get(name);
    if (expected === undefined) continue;
    const p = o.sync(join(o.dir, name, GENERATED), fileBytes(expected), `${name}/${GENERATED}`);
    if (p !== null) problems.push(p);
    for (const file of readdirSync(join(o.dir, name))) if (file !== 'case.json' && file !== GENERATED) problems.push(`${name}/${file}: unknown file`);
    count++;
  }

  // Check 10: the rule names, the rows (checked by validateClientCase) and the rule coverage.
  // A directory without client or function fixtures does not need the client page.
  const needed = cases.size > 0 || existsSync(join(o.dir, 'functions'));
  const known = existsSync(o.clientPage) ? clientRules(o.clientPage) : new Map();
  if (needed && known.size === 0) problems.push(`${o.clientPage}: no rule CL-nn found`);
  const covered = new Set();
  for (const c of cases.values()) {
    if (c.kind !== 'client-canonical') continue;
    for (const rule of c.rules) {
      if (known.has(rule)) covered.add(rule);
      else problems.push(`${c.id}: rule ${rule} is not in legacy client section 9`);
    }
  }

  const fn = functionPart(o, problems, notes);
  for (const rule of fn.rules) covered.add(rule);
  // The floor (as CATALOG_SIZE of the server part): when it says that client fixtures must exist,
  // the catalog must keep that many canonical client fixtures, and checks 10 and 11 apply. So
  // removing client fixtures cannot switch the checks off.
  const canonicalCount = [...cases.values()].filter((c) => c.kind === 'client-canonical').length;
  const floor = o.floor;
  const enforce = o.coverage && floor > 0;
  if (enforce && canonicalCount < floor) problems.push(`the catalog has ${canonicalCount} canonical client fixtures, fewer than ${floor} (CLIENT_FLOOR)`);
  const missing = [...known].filter(([rule, fixture]) => fixture && !covered.has(rule)).map(([rule]) => rule);
  if (enforce) for (const rule of missing) problems.push(`rule ${rule} is not covered by any client or function fixture`);
  else if (o.coverage && missing.length > 0) notes.push(`client rule coverage is checked when CLIENT_FLOOR is above 0; not covered yet: ${missing.join(', ')}`);

  // Check 11: every stored scenario mode is the trace of one canonical client fixture, or exempt.
  if (o.coverage) {
    const exempt = exemptScenarios(o.scenarios);
    const unused = [];
    for (const { scenario, mode } of storedModes(o.evidenceDir)) {
      const key = `${scenario} ${mode}`;
      if (used.has(key)) continue;
      if (exempt.has(scenario)) notes.push(`${key}: stored runs not used: ${exempt.get(scenario)}`);
      else unused.push(key);
    }
    if (enforce) for (const key of unused) problems.push(`evidence/traces/client/android/${key.replace(' ', '/')}: stored client runs are not used by any client fixture`);
    else if (unused.length > 0) notes.push(`stored-run use is checked when CLIENT_FLOOR is above 0; ${unused.length} stored scenario modes have no client fixture yet`);
  }
  return { problems, notes, count: count + fn.count };
}

/** Checks 6 and 12 for fixtures/legacy/functions; returns the CL rules that the canonical file covers. */
function functionPart(o, problems, notes) {
  const dir = join(o.dir, 'functions');
  const rules = new Set();
  if (!existsSync(dir)) {
    if (o.coverage && existsSync(o.evidenceDir)) notes.push('functions: no function fixtures yet (check 12 skipped)');
    return { rules, count: 0 };
  }
  const known = new Set(['cases.json', ...FUNCTION_PROFILES.map((p) => `${p}.json`)]);
  for (const f of readdirSync(dir)) if (!known.has(f)) problems.push(`functions/${f}: unknown file`);
  const lab = readFileSync(o.labFunctions);
  const p = o.sync(join(dir, 'cases.json'), lab, 'functions/cases.json');
  if (p !== null) problems.push(`${p} (a byte copy of tools/upstream-lab/client/functions.json)`);
  const parsed = parseCases(lab.toString('utf8'));
  if (parsed.error !== undefined) {
    problems.push(`functions/cases.json: ${parsed.error}`);
    return { rules, count: 0 };
  }
  const evidence = {};
  for (const [key, path] of [['android', 'android/functions.jsonl'], ['ios', 'ios/functions.jsonl'], ['boolvalue', 'ios/boolvalue.jsonl']]) {
    const file = join(o.evidenceDir, path);
    if (!existsSync(file)) continue;
    const e = parseEvidence(readFileSync(file, 'utf8'));
    if (e.error !== undefined) problems.push(`evidence/traces/client/${path}: ${e.error}`);
    else evidence[key] = e.lines;
  }
  let count = 0;
  for (const profile of FUNCTION_PROFILES) {
    const { file, problems: found } = functionFixture(profile, parsed.cases, evidence);
    problems.push(...found);
    if (profile === 'legacy-canonical') for (const r of file.results) for (const rule of r.rules ?? []) rules.add(rule);
    // session_id: the `none` entries of legacy-canonical are the result of CL-43.
    if (profile === 'legacy-canonical' && file.results.some((r) => r.fn === 'session_id')) rules.add('CL-43');
    const q = o.sync(join(dir, `${profile}.json`), fileBytes(file), `functions/${profile}.json`);
    if (q !== null) problems.push(q);
    count++;
  }
  return { rules, count };
}

