// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// The client case of fixtures/legacy (README.md "Client fixtures"): the members that drive
// the client model (validateBehavior) and the full case.json of a client fixture
// (validateClientCase). Untrusted input: every function here returns a value or throws
// ClientCaseError with a code and the member name; nothing else escapes for input of any
// shape, and every list has a bound before the model reads it.

import { UPSTREAM } from './fixture.mjs';

/** The step words of CONTRACT section 3 and the members that each word takes. */
export const STEP_MEMBERS = Object.freeze({
  create_session: [],
  bind_client: ['session'],
  client_ready: ['session'],
  request_resource: ['session', 'wait_ms'],
  page_finished: ['session'],
  get_diff: ['session'],
  refresh: ['session'],
  destroy: ['session'],
  set_account: ['value'],
  set_cookie: ['value'],
  set_network: ['valid'],
  hold: ['point', 'bytes'],
  release: [],
  cut: ['bytes'],
  drop_field: ['name'],
  set_field: ['name', 'value'],
  set_body_member: ['name', 'value'],
  case: ['value'],
  server_down: [],
  server_up: [],
  wait: ['for', 'timeout_ms'],
  settle: ['ms'],
  snapshot_cache: [],
});

/** The events that a `wait` step can name (CONTRACT section 5) and that the model produces. */
export const WAIT_EVENTS = Object.freeze([
  'proxy.request', 'proxy.response_head', 'proxy.response_end', 'proxy.hold', 'proxy.release', 'proxy.cut',
  'proxy.drop_field', 'proxy.set_field', 'proxy.set_body_member', 'session.created', 'session.result', 'session.message', 'client.load_url',
  'client.load_data', 'client.intercept', 'runtime.cookie_get', 'runtime.cookie_set', 'cache.snapshot',
]);

/** Limits of README "Canonical client case.json". */
export const LIMITS = Object.freeze({
  steps: 256,
  sessions: 16,
  responses: 32,
  cacheBefore: 16,
  maxMs: 3_600_000,
  maxWaitMs: 30_000,
  maxUnavailableMs: 86_400_000,
  maxBytes: 16 * 1024 * 1024,
  maxText: 4096,
});

export const SERVER_PROFILES = Object.freeze(['legacy-canonical', 'legacy-node', 'legacy-java', 'legacy-php']);
export const MODES = Object.freeze(['quick', 'standard']);
export const DEFAULT_URL = 'https://shop.example.com/catalog';
export const DEFAULT_APP = Object.freeze({ status: 200, headers: Object.freeze([Object.freeze(['content-type', 'text/html; charset=utf-8'])]) });

/** Config keys, their check and their default (README table; the harness defaults). */
const CONFIG = Object.freeze({
  account: [(v) => v === null || isText(v), 'rq-test-account'],
  cookie: [(v) => v === null || isText(v), null],
  network_valid: [isBool, true],
  accept_diff: [isBool, true],
  account_related: [isBool, true],
  reload_in_bad_network: [isBool, false],
  support_cache_control: [isBool, false],
  support_local_server: [isBool, false],
  auto_start: [isBool, true],
  get_cookie_when_session_create: [isBool, true],
  cache_verify_with_sha1: [isBool, true],
  authenticated_cache: [isBool, false], // CL-03, CL-25 (DL-123): canonical only; no Android harness key
  unavailable_time_ms: [(v) => isIntIn(v, 0, LIMITS.maxUnavailableMs), 21_600_000],
  connect_timeout_ms: [(v) => isIntIn(v, 0, LIMITS.maxMs), 5000],
  read_timeout_ms: [(v) => isIntIn(v, 0, LIMITS.maxMs), 15000],
});

const RESPONSE_NAME = /^[a-z0-9-]{1,32}$/;
const PAGE_ID = /^[a-z0-9_-]{1,64}$/;
const SERVER_FIXTURE = /^FX-LEG-[A-Z0-9]+(-[A-Z0-9]+)*$/;
export const CLIENT_NAME = /^FX-LEG-CL-[A-Z0-9]+(-[A-Z0-9]+)*-(Q|S)(--ANDROID)?$/;
const RULE = /^CL-\d\d$/;
const ROW = /^(Q([1-9]|1\d|2[0-2])|S([1-9]|1[0-8])|M-(0[1-9]|1[0-6]))$/;
const DIVERGENCE = /^D-\d\d$/;
const SCENARIO = /^CS-\d\d[a-z]?$/;

/** A defect of a client case (README: "a case error ... with the member name"). */
export class ClientCaseError extends Error {
  constructor(code, message, options) {
    super(message, options);
    this.name = 'ClientCaseError';
    this.code = code;
  }
}

function isObject(v) {
  return v !== null && typeof v === 'object' && !Array.isArray(v);
}
function isText(v) {
  return typeof v === 'string' && v.length <= LIMITS.maxText;
}
function isBool(v) {
  return v === true || v === false;
}
function isIntIn(v, lo, hi) {
  return Number.isInteger(v) && v >= lo && v <= hi;
}
function fail(code, message) {
  throw new ClientCaseError(code, message);
}

function onlyKeys(object, allowed, where) {
  if (!isObject(object)) fail('CASE_TYPE', `${where} must be an object`);
  for (const key of Object.keys(object)) if (!allowed.includes(key)) fail('CASE_UNKNOWN_MEMBER', `unknown member ${where}.${key}`);
}

function fieldPairs(list, where) {
  if (!Array.isArray(list) || list.length > 256) fail('CASE_TYPE', `${where} must be an array of at most 256 [name, value] pairs`);
  for (const entry of list) {
    if (!Array.isArray(entry) || entry.length !== 2 || !isText(entry[0]) || !isText(entry[1]) || entry[0] === '') {
      fail('CASE_TYPE', `${where} must hold [name, value] pairs of strings`);
    }
  }
  return list.map(([n, v]) => [n, v]);
}

function appOf(app, where) {
  if (app === undefined) return { status: DEFAULT_APP.status, headers: DEFAULT_APP.headers.map(([n, v]) => [n, v]) };
  onlyKeys(app, ['status', 'headers'], where);
  if (!isIntIn(app.status, 100, 599)) fail('CASE_TYPE', `${where}.status must be an integer from 100 to 599`);
  return { status: app.status, headers: fieldPairs(app.headers, `${where}.headers`) };
}

function responseEntry(entry, where) {
  if (!isObject(entry)) fail('CASE_TYPE', `${where} must be an object`);
  if (Object.hasOwn(entry, 'fixture')) {
    onlyKeys(entry, ['fixture'], where);
    if (typeof entry.fixture !== 'string' || !SERVER_FIXTURE.test(entry.fixture)) fail('CASE_TYPE', `${where}.fixture must be a canonical server fixture ID`);
    return { fixture: entry.fixture };
  }
  onlyKeys(entry, ['page', 'app'], where);
  if (typeof entry.page !== 'string' || !PAGE_ID.test(entry.page)) fail('CASE_TYPE', `${where}.page must be a page ID`);
  return { page: entry.page, app: appOf(entry.app, `${where}.app`) };
}

/**
 * The `value` of a `case` step: the form of a scenario `case` (CONTRACT section 2 and 3; the checks
 * of the lab, tools/upstream-lab/client/src/steps.mjs): `page`, optional `status` and `headers`.
 * Returns the response entry { page, app } with the defaults of a `responses` entry.
 */
function stepCase(value, where) {
  onlyKeys(value, ['page', 'status', 'headers'], where);
  if (typeof value.page !== 'string' || !PAGE_ID.test(value.page)) fail('CASE_STEP', `${where}.page must be a page ID`);
  const app = { status: DEFAULT_APP.status, headers: DEFAULT_APP.headers.map(([n, v]) => [n, v]) };
  if (Object.hasOwn(value, 'status')) {
    if (!isIntIn(value.status, 100, 599)) fail('CASE_STEP', `${where}.status must be an integer from 100 to 599`);
    app.status = value.status;
  }
  if (Object.hasOwn(value, 'headers')) app.headers = fieldPairs(value.headers, `${where}.headers`);
  return { page: value.page, app };
}

function urlOf(value, where) {
  if (!isText(value)) fail('CASE_TYPE', `${where} must be a string`);
  let parsed;
  try {
    parsed = new URL(value);
  } catch {
    fail('CASE_URL', `${where} is not an absolute URL`);
  }
  const host = parsed.hostname;
  if ((parsed.protocol !== 'https:' && parsed.protocol !== 'http:') || !(host === 'example.com' || host.endsWith('.example.com'))) {
    fail('CASE_URL', `${where} must be an http or https URL of example.com or a subdomain`);
  }
  if (parsed.username !== '' || parsed.password !== '' || parsed.hash !== '') fail('CASE_URL', `${where} must have no user info and no fragment`);
  return value;
}

function stepOf(item, index) {
  const where = `schedule[${index}]`;
  if (!isObject(item)) fail('CASE_TYPE', `${where} must be an object`);
  if (Object.hasOwn(item, 'case')) {
    onlyKeys(item, ['case'], where);
    if (typeof item.case !== 'string' || !RESPONSE_NAME.test(item.case)) fail('CASE_TYPE', `${where}.case must be a response name`);
    return { case: item.case };
  }
  const word = item.do;
  if (typeof word !== 'string' || !Object.hasOwn(STEP_MEMBERS, word)) fail('CASE_STEP', `${where}.do is not a step word of CONTRACT section 3`);
  onlyKeys(item, ['do', ...STEP_MEMBERS[word]], where);
  const step = { do: word };
  const has = (k) => Object.hasOwn(item, k);
  if (has('session')) {
    if (!isIntIn(item.session, 1, LIMITS.sessions)) fail('CASE_STEP', `${where}.session must be an integer from 1 to ${LIMITS.sessions}`);
    step.session = item.session;
  }
  switch (word) {
    case 'request_resource':
      if (has('wait_ms')) {
        if (!isIntIn(item.wait_ms, 0, LIMITS.maxWaitMs)) fail('CASE_LIMIT', `${where}.wait_ms must be from 0 to ${LIMITS.maxWaitMs}`);
        step.wait_ms = item.wait_ms;
      }
      break;
    case 'set_account':
    case 'set_cookie':
      if (!has('value') || !(item.value === null || isText(item.value))) fail('CASE_STEP', `${where}.value must be a string or null`);
      step.value = item.value;
      break;
    case 'set_network':
      if (!isBool(item.valid)) fail('CASE_STEP', `${where}.valid must be a boolean`);
      step.valid = item.valid;
      break;
    case 'hold':
      if (!['request', 'head', 'body'].includes(item.point)) fail('CASE_STEP', `${where}.point must be request, head or body`);
      step.point = item.point;
      if (item.point === 'body') {
        if (!isIntIn(item.bytes, 0, LIMITS.maxBytes)) fail('CASE_LIMIT', `${where}.bytes must be an integer from 0`);
        step.bytes = item.bytes;
      } else if (has('bytes')) {
        fail('CASE_STEP', `${where}.bytes is only for point body`);
      }
      break;
    case 'cut':
      if (!isIntIn(item.bytes, 0, LIMITS.maxBytes)) fail('CASE_LIMIT', `${where}.bytes must be an integer from 0`);
      step.bytes = item.bytes;
      break;
    case 'drop_field':
      if (!isText(item.name) || item.name === '') fail('CASE_STEP', `${where}.name must be a field name`);
      step.name = item.name;
      break;
    case 'set_field':
    case 'set_body_member':
      if (!isText(item.name) || item.name === '' || !isText(item.value)) fail('CASE_STEP', `${where}.name and .value must be strings`);
      step.name = item.name;
      step.value = item.value;
      break;
    case 'case':
      step.response = stepCase(item.value, `${where}.value`);
      break;
    case 'wait':
      if (!WAIT_EVENTS.includes(item.for)) fail('CASE_STEP', `${where}.for must be an event that the model produces`);
      step.for = item.for;
      if (has('timeout_ms')) {
        if (!isIntIn(item.timeout_ms, 0, LIMITS.maxMs)) fail('CASE_LIMIT', `${where}.timeout_ms must be from 0 to ${LIMITS.maxMs}`);
        step.timeout_ms = item.timeout_ms;
      }
      break;
    case 'settle':
      if (!isIntIn(item.ms, 0, LIMITS.maxMs)) fail('CASE_LIMIT', `${where}.ms must be from 0 to ${LIMITS.maxMs}`);
      step.ms = item.ms;
      break;
    default:
      break;
  }
  return step;
}

function cacheEntry(entry, index) {
  const where = `cache_before[${index}]`;
  onlyKeys(entry, ['page', 'partition', 'url', 'vary', 'fields', 'etag', 'template_tag'], where);
  if (typeof entry.page !== 'string' || !PAGE_ID.test(entry.page)) fail('CASE_TYPE', `${where}.page must be a page ID`);
  const out = { page: entry.page };
  if (Object.hasOwn(entry, 'partition')) {
    if (!(entry.partition === null || isText(entry.partition))) fail('CASE_TYPE', `${where}.partition must be a string or null`);
    out.partition = entry.partition;
  }
  if (Object.hasOwn(entry, 'url')) out.url = urlOf(entry.url, `${where}.url`);
  if (Object.hasOwn(entry, 'vary')) {
    if (!Array.isArray(entry.vary) || entry.vary.length > 32) fail('CASE_TYPE', `${where}.vary must be an array of at most 32 entries`);
    for (const v of entry.vary) {
      if (!Array.isArray(v) || v.length !== 2 || !isText(v[0]) || v[0] === '' || !(v[1] === null || isText(v[1]))) fail('CASE_TYPE', `${where}.vary must hold [name, value or null] pairs`);
    }
    out.vary = entry.vary.map(([n, v]) => [n, v]);
  }
  if (Object.hasOwn(entry, 'fields')) out.fields = fieldPairs(entry.fields, `${where}.fields`);
  for (const k of ['etag', 'template_tag']) {
    if (Object.hasOwn(entry, k)) {
      if (!isText(entry[k])) fail('CASE_TYPE', `${where}.${k} must be a string`);
      out[k] = entry[k];
    }
  }
  return out;
}

/**
 * The members of a client case that drive the model, checked and with the defaults filled in.
 * Members that do not drive the model are not read. Throws ClientCaseError.
 */
export function validateBehavior(c) {
  if (!isObject(c)) fail('CASE_TYPE', 'the case must be an object');
  if (!MODES.includes(c.mode)) fail('CASE_TYPE', 'mode must be quick or standard');
  if (!SERVER_PROFILES.includes(c.server_profile)) fail('CASE_TYPE', `server_profile must be one of ${SERVER_PROFILES.join(', ')}`);
  const url = Object.hasOwn(c, 'url') ? urlOf(c.url, 'url') : DEFAULT_URL;
  const configIn = Object.hasOwn(c, 'config') ? c.config : {};
  onlyKeys(configIn, Object.keys(CONFIG), 'config');
  const config = {};
  for (const [key, [check, def]] of Object.entries(CONFIG)) {
    if (Object.hasOwn(configIn, key)) {
      if (!check(configIn[key])) fail('CASE_TYPE', `config.${key} has a wrong type or range`);
      config[key] = configIn[key];
    } else {
      config[key] = def;
    }
  }
  if (!isObject(c.responses)) fail('CASE_TYPE', 'responses must be an object');
  const names = Object.keys(c.responses);
  if (names.length < 1 || names.length > LIMITS.responses) fail('CASE_LIMIT', `responses must have 1 to ${LIMITS.responses} entries`);
  const responses = new Map();
  for (const name of names) {
    if (!RESPONSE_NAME.test(name)) fail('CASE_TYPE', `responses.${name}: the name must be 1 to 32 characters of a-z, 0-9, -`);
    responses.set(name, responseEntry(c.responses[name], `responses.${name}`));
  }
  if (!Array.isArray(c.cache_before) || c.cache_before.length > LIMITS.cacheBefore) fail('CASE_LIMIT', `cache_before must be an array of at most ${LIMITS.cacheBefore} entries`);
  const cacheBefore = c.cache_before.map(cacheEntry);
  if (!Array.isArray(c.schedule) || c.schedule.length > 4 * LIMITS.steps) fail('CASE_LIMIT', 'schedule must be an array');
  const schedule = c.schedule.map(stepOf);
  const steps = schedule.filter((s) => s.do !== undefined);
  if (steps.length < 1 || steps.length > LIMITS.steps) fail('CASE_LIMIT', `schedule must have 1 to ${LIMITS.steps} steps`);
  if (steps.filter((s) => s.do === 'create_session').length > LIMITS.sessions) fail('CASE_LIMIT', `schedule must have at most ${LIMITS.sessions} create_session steps`);
  for (const s of schedule) if (s.case !== undefined && !responses.has(s.case)) fail('CASE_TYPE', `schedule: case ${s.case} is not a name of responses`);
  return { mode: c.mode, server_profile: c.server_profile, url, config, responses, cache_before: cacheBefore, schedule };
}

function stringList(list, pattern, where, { nonEmpty }) {
  if (!Array.isArray(list) || list.length > 256 || (nonEmpty && list.length === 0) || !list.every((x) => typeof x === 'string' && pattern.test(x))) {
    fail('CASE_TYPE', `${where} must be a ${nonEmpty ? 'non-empty ' : ''}list of strings matching ${pattern}`);
  }
  return [...list];
}

const CANONICAL_MEMBERS = ['id', 'kind', 'description', 'profile', 'mode', 'server_profile', 'rules', 'rows', 'divergences', 'upstream', 'sources', 'evidence', 'trace', 'url', 'config', 'responses', 'cache_before', 'schedule', 'level'];
const PROFILE_MEMBERS = ['id', 'kind', 'description', 'profile', 'replaces', 'divergences', 'sources', 'not_reproduced'];

/** The `not_reproduced` items of a client profile fixture (DL-120): each has a check in compareClient. */
export const CLIENT_NOT_REPRODUCIBLE = Object.freeze(['U-13']);

/**
 * The full check of a client case.json (`name` is the directory name). Returns
 * { kind: 'client-canonical', ... } with the behavior members checked, or
 * { kind: 'client-profile', id, profile, replaces, divergences, sources, description }.
 * Throws ClientCaseError.
 */
export function validateClientCase(name, c) {
  if (!CLIENT_NAME.test(name)) fail('CASE_NAME', 'the directory name is not a client fixture ID (FX-LEG-CL-<NAME>-<Q|S>[--ANDROID])');
  if (!isObject(c)) fail('CASE_TYPE', 'case.json must be an object');
  if (c.id !== name) fail('CASE_TYPE', `id must be ${name}`);
  if (c.kind !== 'client') fail('CASE_TYPE', 'kind must be client');
  if (!isText(c.description) || c.description === '') fail('CASE_TYPE', 'description must be a non-empty string');
  const sources = stringList(c.sources, /^(upstream|docs):\S+$/, 'sources', { nonEmpty: true });
  if (name.endsWith('--ANDROID')) {
    onlyKeys(c, PROFILE_MEMBERS, 'case');
    if (c.profile !== 'legacy-android') fail('CASE_TYPE', 'profile must be legacy-android');
    if (c.replaces !== name.slice(0, -'--ANDROID'.length)) fail('CASE_TYPE', 'replaces must be the ID without the suffix');
    const divergences = stringList(c.divergences, DIVERGENCE, 'divergences', { nonEmpty: false });
    if (Object.hasOwn(c, 'not_reproduced') && !CLIENT_NOT_REPRODUCIBLE.includes(c.not_reproduced)) {
      fail('CASE_TYPE', `not_reproduced must be one of ${CLIENT_NOT_REPRODUCIBLE.join(', ')}`);
    }
    const notReproduced = Object.hasOwn(c, 'not_reproduced') ? c.not_reproduced : null;
    return { kind: 'client-profile', id: name, description: c.description, profile: c.profile, replaces: c.replaces, divergences, sources, notReproduced };
  }
  onlyKeys(c, CANONICAL_MEMBERS, 'case');
  for (const k of ['profile', 'mode', 'server_profile', 'rules', 'rows', 'upstream', 'evidence', 'responses', 'cache_before', 'schedule']) {
    if (!Object.hasOwn(c, k)) fail('CASE_MISSING_MEMBER', `missing member case.${k}`);
  }
  if (c.profile !== 'legacy-canonical') fail('CASE_TYPE', 'profile must be legacy-canonical');
  const behavior = validateBehavior(c);
  if ((behavior.mode === 'quick') !== name.endsWith('-Q')) fail('CASE_TYPE', 'mode must match the name suffix -Q (quick) or -S (standard)');
  const rules = stringList(c.rules, RULE, 'rules', { nonEmpty: true });
  const rows = stringList(c.rows, ROW, 'rows', { nonEmpty: false });
  const divergences = Object.hasOwn(c, 'divergences') ? stringList(c.divergences, DIVERGENCE, 'divergences', { nonEmpty: false }) : [];
  onlyKeys(c.upstream, ['repo', 'sha'], 'upstream');
  if (c.upstream.repo !== UPSTREAM.repo || c.upstream.sha !== UPSTREAM.sha) fail('CASE_TYPE', 'upstream must be the pinned repository and SHA');
  if (c.evidence !== 'trace' && c.evidence !== 'code-reading') fail('CASE_TYPE', 'evidence must be trace or code-reading');
  let trace = null;
  if (c.evidence === 'trace') {
    onlyKeys(c.trace, ['scenario', 'mode'], 'trace');
    if (typeof c.trace.scenario !== 'string' || !SCENARIO.test(c.trace.scenario)) fail('CASE_TYPE', 'trace.scenario must be CS-<NN>[a-z]');
    if (c.trace.mode !== behavior.mode) fail('CASE_TYPE', 'trace.mode must equal mode');
    if (behavior.cache_before.length > 0) fail('CASE_TYPE', 'cache_before must be [] with evidence trace (the lab starts each run with an empty app state)');
    trace = { scenario: c.trace.scenario, mode: c.trace.mode };
  } else if (Object.hasOwn(c, 'trace')) {
    fail('CASE_TYPE', 'trace is only for evidence trace');
  }
  const level = c.level ?? 'must';
  if (level !== 'must' && level !== 'should') fail('CASE_TYPE', 'level must be must or should');
  return { kind: 'client-canonical', id: name, description: c.description, profile: c.profile, rules, rows, divergences, sources, evidence: c.evidence, trace, level, ...behavior, raw: c };
}
