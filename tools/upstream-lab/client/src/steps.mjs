// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// Step vocabulary (CONTRACT section 3) and scenario validation (section 2).

const isInt = (v, min = 0, max = Number.MAX_SAFE_INTEGER) => Number.isInteger(v) && v >= min && v <= max;
const isStr = (v) => typeof v === 'string' && v.length > 0;

/** Step name -> allowed members (besides `do`) and a check of them. */
const NONE = { members: [], check: () => null };
export const HARNESS_STEPS = {
  create_session: NONE,
  bind_client: NONE,
  client_ready: NONE,
  request_resource: NONE,
  page_finished: NONE,
  get_diff: NONE,
  refresh: NONE,
  destroy: NONE,
  snapshot_cache: NONE,
  set_account: { members: ['value'], check: (s) => (typeof s.value === 'string' ? null : 'value must be a string') },
  set_cookie: { members: ['value'], check: (s) => (typeof s.value === 'string' ? null : 'value must be a string') },
  set_network: { members: ['valid'], check: (s) => (typeof s.valid === 'boolean' ? null : 'valid must be a boolean') },
};
export const CONDUCTOR_STEPS = {
  hold: {
    members: ['point', 'bytes'],
    check: (s) => {
      if (!['request', 'head', 'body'].includes(s.point)) return 'point must be request, head or body';
      if (s.point === 'body' && !isInt(s.bytes)) return 'bytes must be a non-negative integer for point body';
      if (s.point !== 'body' && s.bytes !== undefined) return 'bytes is only for point body';
      return null;
    },
  },
  release: NONE,
  cut: { members: ['bytes'], check: (s) => (isInt(s.bytes) ? null : 'bytes must be a non-negative integer') },
  drop_field: { members: ['name'], check: (s) => (isStr(s.name) ? null : 'name must be a non-empty string') },
  server_down: NONE,
  server_up: NONE,
  wait: {
    members: ['for', 'timeout_ms'],
    check: (s) => {
      if (!isStr(s.for)) return 'for must be an event name';
      if (s.timeout_ms !== undefined && !isInt(s.timeout_ms, 1, 600_000)) return 'timeout_ms must be 1..600000';
      return null;
    },
  },
  settle: { members: ['ms'], check: (s) => (isInt(s.ms, 0, 60_000) ? null : 'ms must be 0..60000') },
};

export const isHarnessStep = (step) => Object.hasOwn(HARNESS_STEPS, step.do);

/** Problem text for one step object, or null. */
export function stepProblem(step) {
  if (step === null || typeof step !== 'object' || Array.isArray(step)) return 'step must be an object';
  const spec = Object.hasOwn(HARNESS_STEPS, step.do) ? HARNESS_STEPS[step.do] : Object.hasOwn(CONDUCTOR_STEPS, step.do) ? CONDUCTOR_STEPS[step.do] : null;
  if (!spec) return `unknown step "${step.do}"`;
  const extra = Object.keys(step).filter((k) => k !== 'do' && !spec.members.includes(k));
  if (extra.length > 0) return `${step.do}: unknown member ${extra.join(', ')}`;
  const problem = spec.check(step);
  return problem ? `${step.do}: ${problem}` : null;
}

/** The macro "first_load" (CONTRACT section 2). */
export const FIRST_LOAD = [
  { do: 'create_session' },
  { do: 'bind_client' },
  { do: 'client_ready' },
  { do: 'request_resource' },
  { do: 'wait', for: 'proxy.response_end' },
  { do: 'settle', ms: 2500 },
  { do: 'destroy' },
];

export const ORIGINS = { 'lab-node': 18081, 'lab-php': 18082, 'lab-java': 18083 };
export const PROFILES = { 'lab-node': 'legacy-node', 'lab-php': 'legacy-php', 'lab-java': 'legacy-java' };
const MODES = ['quick', 'standard'];
const ID = /^CS-\d{2}[a-z]?$/;
const PAGE = /^[a-z0-9_-]+$/;

function caseProblem(c) {
  if (c === null || typeof c !== 'object' || Array.isArray(c)) return 'case must be an object';
  if (typeof c.page !== 'string' || !PAGE.test(c.page)) return 'case.page must be a page name';
  if (c.status !== undefined && !isInt(c.status, 100, 599)) return 'case.status must be 100..599';
  if (c.headers !== undefined && !(Array.isArray(c.headers) && c.headers.every((p) => Array.isArray(p) && p.length === 2 && p.every((x) => typeof x === 'string')))) {
    return 'case.headers must be a list of [name, value]';
  }
  const extra = Object.keys(c).filter((k) => !['page', 'status', 'headers'].includes(k));
  return extra.length > 0 ? `case: unknown member ${extra.join(', ')}` : null;
}

/** Problems of one scenario object (empty list when valid). */
export function scenarioProblems(s) {
  const out = [];
  if (s === null || typeof s !== 'object') return ['scenario must be an object'];
  const at = (text) => out.push(`${s.id ?? '?'}: ${text}`);
  if (typeof s.id !== 'string' || !ID.test(s.id)) at('id must be CS-<NN> with an optional letter');
  if (typeof s.title !== 'string' || s.title === '') at('title is required');
  if (!Array.isArray(s.modes) || s.modes.length === 0 || !s.modes.every((m) => MODES.includes(m))) at('modes must be a non-empty subset of quick, standard');
  if (!Object.hasOwn(ORIGINS, s.origin)) at('origin must be lab-node, lab-php or lab-java');
  if (!Array.isArray(s.rows) || !s.rows.every((r) => typeof r === 'string')) at('rows must be a list of strings');
  const c = caseProblem(s.case);
  if (c) at(c);
  if (!isInt(s.repeat, 1, 100)) at('repeat must be 1..100');
  if (s.config === null || typeof s.config !== 'object' || Array.isArray(s.config)) at('config must be an object');
  if (!Array.isArray(s.steps) || s.steps.length === 0) at('steps must be a non-empty list');
  else s.steps.forEach((step, i) => { const p = stepProblem(step); if (p) at(`steps[${i}]: ${p}`); });
  if (!Array.isArray(s.setup)) at('setup must be a list');
  else {
    s.setup.forEach((item, i) => {
      if (item === 'first_load') return;
      if (item !== null && typeof item === 'object' && Object.keys(item).length === 1 && 'case' in item) {
        const p = caseProblem(item.case);
        if (p) at(`setup[${i}]: ${p}`);
        return;
      }
      const p = stepProblem(item);
      if (p) at(`setup[${i}]: ${p}`);
    });
  }
  return out;
}

/** Problems of a scenario file (an array of scenarios with unique ids). */
export function catalogProblems(list) {
  if (!Array.isArray(list)) return ['scenario file must be an array'];
  const out = list.flatMap(scenarioProblems);
  const seen = new Set();
  for (const s of list) {
    if (seen.has(s?.id)) out.push(`${s.id}: duplicate id`);
    seen.add(s?.id);
  }
  return out;
}

/**
 * The setup list as a sequence of items: { case } or { step }. The macro first_load becomes its steps.
 * @returns {({ case: object } | { step: object })[]}
 */
export function expandSetup(setup) {
  return setup.flatMap((item) => {
    if (item === 'first_load') return FIRST_LOAD.map((step) => ({ step }));
    if ('case' in item && !('do' in item)) return [{ case: item.case }];
    return [{ step: item }];
  });
}
