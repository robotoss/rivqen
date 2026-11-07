// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// Helpers of the client tests: a client case from a stored lab scenario (the test samples of
// the client model; the real fixture directories are written by hand in fixtures/legacy).

import { existsSync, readFileSync, readdirSync } from 'node:fs';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { parseRun, traceSteps } from '../src/client-trace.mjs';

export const ROOT = fileURLToPath(new URL('../../../', import.meta.url));
export const PAGES = join(ROOT, 'fixtures/legacy/pages');
const LAB_PAGES = join(ROOT, 'tools/upstream-lab/pages');
const SCENARIOS = JSON.parse(readFileSync(join(ROOT, 'tools/upstream-lab/client/scenarios.json'), 'utf8'));
const ORIGIN = { 'lab-node': 'legacy-node', 'lab-java': 'legacy-java', 'lab-php': 'legacy-php' };
export const UPSTREAM = { repo: 'Tencent/VasSonic', sha: '59936beff656d4b5718ff6444d6c5e001a2c5231' };

/** Page bytes by ID (fixtures/legacy/pages, then the lab pages for a page that the fixtures do not have yet). */
export function pageOf(id) {
  for (const dir of [PAGES, LAB_PAGES]) {
    const path = join(dir, `${id}.html`);
    if (existsSync(path)) return readFileSync(path);
  }
  return null;
}

export const ENV = Object.freeze({ page: pageOf, fixture: () => null });

/** The parsed stored runs of a scenario mode. */
export function storedRuns(scenario, mode) {
  const dir = join(ROOT, 'evidence/traces/client/android', scenario, mode);
  if (!existsSync(dir)) return [];
  return readdirSync(dir).filter((f) => /^run-\d+\.jsonl$/.test(f)).sort((a, b) => Number(a.match(/\d+/)[0]) - Number(b.match(/\d+/)[0])).map((f) => parseRun(readFileSync(join(dir, f))));
}

/** The stored scenario modes (CS-19 excepted: a WebView smoke run). */
export function storedModes() {
  const out = [];
  for (const s of SCENARIOS) {
    if (s.webview === true) continue;
    for (const mode of s.modes) if (storedRuns(s.id, mode).length > 0) out.push({ scenario: s.id, mode });
  }
  return out;
}

const responseOf = (c) => ({ page: c.page, app: { status: c.status ?? 200, headers: c.headers ?? [['content-type', 'text/html; charset=utf-8']] } });

/** A canonical client case for a stored scenario mode: the steps of its first run and its case entries. */
export function scenarioCase(scenario, mode) {
  const s = SCENARIOS.find((x) => x.id === scenario);
  const steps = traceSteps(storedRuns(scenario, mode)[0]);
  const responses = {};
  const schedule = [];
  let n = 0;
  const caseName = (c) => {
    const key = JSON.stringify(c);
    for (const [name, r] of Object.entries(responses)) if (JSON.stringify(r) === JSON.stringify(responseOf(c))) return name;
    const name = `r${++n}`;
    responses[name] = responseOf(c);
    return key && name;
  };
  const setupCases = s.setup.filter((x) => typeof x === 'object' && x.case !== undefined);
  const setupSteps = steps.length - s.steps.length;
  if (setupCases.length > 0) schedule.push({ case: caseName(setupCases[0].case) });
  steps.forEach((step, i) => {
    if (i === setupSteps) schedule.push({ case: caseName(s.case) });
    schedule.push(step);
  });
  const tag = scenario.replace('-', '').toUpperCase();
  return {
    id: `FX-LEG-CL-${tag}-${mode === 'quick' ? 'Q' : 'S'}`,
    kind: 'client',
    description: s.title,
    profile: 'legacy-canonical',
    mode,
    server_profile: ORIGIN[s.origin],
    rules: ['CL-01'],
    rows: s.rows,
    upstream: UPSTREAM,
    sources: ['docs:engineering/protocol/legacy-client#_9-9-expected-results-of-the-stored-scenarios'],
    evidence: 'trace',
    trace: { scenario, mode },
    config: s.config,
    responses,
    cache_before: [],
    schedule,
  };
}
