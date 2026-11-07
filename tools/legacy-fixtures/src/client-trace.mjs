// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// Trace projection of the client part (fixtures/legacy/README.md "Trace projection and the
// comparison (DL-104)"): a stored run of the client lab (run-<k>.jsonl) becomes the `client`
// member of a client expected.json, and two `client` members are compared after the
// normalization of that section.
//
// The phases come from settlePhases and the message normalization from normalizeEvent of the
// client lab (tools/upstream-lab/client/src/determinism.mjs, imported, not copied), so the
// projection and the lab determinism rule cannot drift apart.
//
// Untrusted input: the stored files. Every function here returns a result or throws
// TraceProjectionError with a code; no other exception escapes for input of any shape.

import { isDeepStrictEqual } from 'node:util';
import { firstDifference, normalizeEvent, settlePhases } from '../../upstream-lab/client/src/determinism.mjs';
import { CLIENT_NOT_REPRODUCIBLE } from './client-case.mjs';
import { asciiLower } from './fields.mjs';
import { mediaTypeKey } from './trace.mjs';

/** The lab page origin of CONTRACT section 2: `http://10.0.2.2:18090/`. */
export const LAB_PREFIX = 'http://10.0.2.2:18090/';

/** Request fields of a projection request (lowercase names). */
export const REQUEST_FIELDS = Object.freeze(['accept-diff', 'if-none-match', 'template-tag', 'cookie', 'sonic-sdk-version']);
/** Response fields of a projection response (lowercase names). */
export const RESPONSE_FIELDS = Object.freeze(['etag', 'template-tag', 'template-change', 'cache-offline', 'sonic-etag-key', 'set-cookie']);

// Normalization item 2: transport fields and the fields that no server model reproduces.
const TRANSPORT = new Set(['date', 'connection', 'keep-alive', 'transfer-encoding', 'content-length', 'content-encoding', 'vary', 'cdn-cache-control', 'surrogate-control']);

/** Upper bounds of a stored run (resource limits: a larger file is a projection error). */
export const TRACE_LIMITS = Object.freeze({ bytes: 16 * 1024 * 1024, events: 100_000 });

/** A stored run that does not give a projection. */
export class TraceProjectionError extends Error {
  constructor(code, message, options) {
    super(message, options);
    this.name = 'TraceProjectionError';
    this.code = code;
  }
}

const isObject = (v) => v !== null && typeof v === 'object' && !Array.isArray(v);
const isPair = (v) => Array.isArray(v) && v.length === 2 && typeof v[0] === 'string' && typeof v[1] === 'string';
const isInt = (v) => Number.isInteger(v);

/** Parse the JSON Lines text of a run. Throws TraceProjectionError. */
export function parseRun(bytes) {
  if (!(bytes instanceof Uint8Array)) throw new TraceProjectionError('TRACE_INPUT', 'a run must be bytes');
  if (bytes.length > TRACE_LIMITS.bytes) throw new TraceProjectionError('TRACE_LIMIT', `a run has more than ${TRACE_LIMITS.bytes} bytes`);
  const text = Buffer.from(bytes.buffer, bytes.byteOffset, bytes.byteLength).toString('utf8');
  const lines = text.split('\n').filter((l) => l !== '');
  if (lines.length > TRACE_LIMITS.events) throw new TraceProjectionError('TRACE_LIMIT', `a run has more than ${TRACE_LIMITS.events} events`);
  return lines.map((line, i) => {
    let event;
    try {
      event = JSON.parse(line);
    } catch (error) {
      throw new TraceProjectionError('TRACE_JSON', `line ${i + 1} is not JSON`, { cause: error });
    }
    if (!isObject(event) || typeof event.ev !== 'string') throw new TraceProjectionError('TRACE_EVENT', `line ${i + 1} is not an event`);
    return event;
  });
}

/** Map a lab URL to the fixture URL (normalization item 1). Other URLs stay. */
export function labUrl(url, fixtureUrl) {
  if (typeof url !== 'string' || !url.startsWith(LAB_PREFIX)) return url;
  let origin;
  try {
    origin = new URL(fixtureUrl).origin;
  } catch {
    return url;
  }
  return `${origin}/${url.slice(LAB_PREFIX.length)}`;
}

/** [name, value] pairs with lowercase names, sorted by name (stable). */
export function sortedFields(pairs) {
  const list = Array.isArray(pairs) ? pairs.filter(isPair).map(([n, v]) => [asciiLower(n), v]) : [];
  return list.sort((a, b) => (a[0] < b[0] ? -1 : a[0] > b[0] ? 1 : 0));
}

function pickFields(pairs, names) {
  const wanted = new Set(names);
  return (Array.isArray(pairs) ? pairs : []).filter(isPair).map(([n, v]) => [asciiLower(n), v]).filter(([n]) => wanted.has(n));
}

/** Canonical JSON text of a projection entry (the key order of the entry, no white space). */
export const entryText = (entry) => JSON.stringify(entry);
/** A multiset as the README writes it: sorted by the canonical JSON text of each entry. */
export const sortedEntries = (list) => [...list].sort((a, b) => (entryText(a) < entryText(b) ? -1 : entryText(a) > entryText(b) ? 1 : 0));

// The `.header` text of the harness: one field per line, `<name> : <value>`.
function headerLines(text) {
  const out = [];
  for (const raw of text.split(/\r?\n/)) {
    if (raw === '') continue;
    const at = raw.indexOf(':');
    if (at < 0) throw new TraceProjectionError('TRACE_HEADER', `cache header line without a colon: ${JSON.stringify(raw.slice(0, 80))}`);
    out.push([raw.slice(0, at).trim().toLowerCase(), raw.slice(at + 1).trim()]);
  }
  return out;
}

const FILE_PART = Object.freeze({ '.html': 'page_sha256', '.tpl': 'template_sha256', '.data': 'data_sha256', '.header': 'header' });

/**
 * The rebuilt page that failed the `html-sha1` check stays as `<stem>.html.tmp` (trace report
 * section 4 note 8, section 6 finding 15; CS-28). It is a work file, not a part of the cache
 * record (CL-38, DL-110: page, template, data, header), so the projection does not use it. Its
 * stem must still be the id of a session of the run.
 */
export const WORK_FILE = '.html.tmp';

function snapshotOf(event, sessionsById) {
  if (!Array.isArray(event.files)) throw new TraceProjectionError('TRACE_SNAPSHOT', 'cache.snapshot without files');
  const stems = new Map();
  for (const file of event.files) {
    if (!isObject(file) || typeof file.name !== 'string') throw new TraceProjectionError('TRACE_SNAPSHOT', 'a snapshot file without a name');
    if (file.name.endsWith(WORK_FILE)) {
      const stem = file.name.slice(0, -WORK_FILE.length);
      if (!sessionsById.has(stem)) throw new TraceProjectionError('TRACE_SNAPSHOT', `cache file stem ${stem} is not the id of a session of the run`);
      continue;
    }
    const dot = file.name.lastIndexOf('.');
    const ext = dot < 0 ? '' : file.name.slice(dot);
    const part = FILE_PART[ext];
    if (part === undefined) throw new TraceProjectionError('TRACE_SNAPSHOT', `unknown cache file ${file.name}`);
    const stem = file.name.slice(0, dot);
    if (!stems.has(stem)) stems.set(stem, {});
    stems.get(stem)[part] = file;
  }
  const entries = [];
  for (const [stem, parts] of stems) {
    const session = sessionsById.get(stem);
    if (session === undefined) throw new TraceProjectionError('TRACE_SNAPSHOT', `cache file stem ${stem} is not the id of a session of the run`);
    let fields = [];
    if (parts.header !== undefined) {
      if (typeof parts.header.text !== 'string') throw new TraceProjectionError('TRACE_SNAPSHOT', `${stem}.header has no text`);
      fields = headerLines(parts.header.text);
    }
    const value = (name) => {
      const line = fields.find(([n]) => n === name);
      return line === undefined ? null : line[1];
    };
    const hash = (file) => (file === undefined ? null : typeof file.sha256 === 'string' ? file.sha256 : null);
    entries.push({
      partition: session.partition,
      url: session.url,
      vary: [],
      etag: value('etag'),
      template_tag: value('template-tag'),
      page_sha256: hash(parts.page_sha256),
      template_sha256: hash(parts.template_sha256),
      data_sha256: hash(parts.data_sha256),
      fields: sortedFields(fields),
    });
  }
  return { entries: sortRecords(entries) };
}

/** Records sorted by partition, url, vary (README "snapshot"). */
export function sortRecords(entries) {
  const key = (r) => JSON.stringify([r.partition, r.url, r.vary]);
  return [...entries].sort((a, b) => (key(a) < key(b) ? -1 : key(a) > key(b) ? 1 : 0));
}

function byNumber(member) {
  return (a, b) => (isInt(a[member]) ? a[member] : Infinity) - (isInt(b[member]) ? b[member] : Infinity);
}

/**
 * The `client` projection of one stored run.
 *
 * @param {object[]} events the parsed run (parseRun)
 * @param {{ url: string, account: string | null }} context the fixture URL and config.account
 * @returns {{ sessions: object[], phases: object[] }}
 */
export function projectRun(events, context) {
  if (!Array.isArray(events)) throw new TraceProjectionError('TRACE_INPUT', 'events must be an array');
  const fixtureUrl = context.url;
  const url = (u) => labUrl(u, fixtureUrl);
  const heads = new Map();
  const ends = new Map();
  for (const e of events) {
    if (e.ev === 'proxy.response_head' && isInt(e.exchange)) heads.set(e.exchange, e);
    if (e.ev === 'proxy.response_end' && isInt(e.exchange)) ends.set(e.exchange, e);
  }
  const created = events.filter((e) => e.ev === 'session.created').sort(byNumber('hseq'));
  let account = context.account;
  let stepNo = 0;
  let createdNo = 0;
  const sessions = [];
  const sessionsById = new Map();
  const phases = [];
  for (const phase of settlePhases(events)) {
    const steps = [];
    const out = { steps: [], requests: [], cookies: [], actions: [], intercepts: [], results: [], snapshots: [] };
    for (const e of phase) {
      if (e.ev === 'step.start' && e.src === 'conductor') {
        stepNo += 1;
        steps.push(stepNo);
        const step = isObject(e.step) ? e.step : {};
        if (step.do === 'set_account') account = step.value ?? null;
        if (step.do === 'create_session') {
          const c = created[createdNo++];
          if (c === undefined) throw new TraceProjectionError('TRACE_SESSION', `create_session step ${stepNo} has no session.created event`);
          if (c.session_id === null || c.session_id === undefined) {
            sessions.push({ step: stepNo, opened: false, partition: account, url: fixtureUrl });
          } else {
            const s = { step: stepNo, opened: true, session: c.session, partition: account, url: fixtureUrl };
            sessions.push(s);
            if (!sessionsById.has(c.session_id)) sessionsById.set(c.session_id, s);
          }
        }
      }
    }
    for (const e of phase.filter((x) => x.ev === 'proxy.request').sort(byNumber('exchange'))) {
      const head = heads.get(e.exchange);
      const end = ends.get(e.exchange);
      out.requests.push({
        exchange: e.exchange,
        method: e.method,
        target: e.target,
        fields: pickFields(e.fields, REQUEST_FIELDS),
        response: head === undefined ? null : { status: head.status, fields: pickFields(head.fields, RESPONSE_FIELDS), complete: end?.complete === true },
      });
    }
    for (const e of phase) {
      if (e.ev === 'runtime.cookie_get') out.cookies.push({ call: 'get', url: url(e.url), value: e.value ?? null });
      if (e.ev === 'runtime.cookie_set') {
        const values = Array.isArray(e.value) ? e.value : e.value === null || e.value === undefined ? [] : [e.value];
        out.cookies.push({ call: 'set', url: url(e.url), values });
      }
      if (e.ev === 'client.intercept') out.intercepts.push(interceptOf(e));
      if (e.ev === 'session.result') out.results.push({ session: e.session, src_code: e.src_code, final_code: e.final_code });
      if (e.ev === 'cache.snapshot') out.snapshots.push(snapshotOf(e, sessionsById));
    }
    const main = phase.filter((x) => x.src === 'harness' && x.thread === 'main').sort(byNumber('hseq'));
    for (const e of main) {
      if (e.ev === 'client.load_url') out.actions.push({ do: 'load_url', session: e.session, url: url(e.url) });
      if (e.ev === 'client.load_data') {
        out.actions.push({ do: 'load_data', session: e.session, url: url(e.base_url), fields: sortedFields(e.headers), body_len: e.body_len, body_sha256: e.body_sha256 });
      }
      if (e.ev === 'session.message') out.actions.push({ do: 'message', session: e.session, json: normalizeEvent(e).json });
    }
    out.steps = steps.length === 0 ? [] : [steps[0], steps.at(-1)];
    out.cookies = sortedEntries(out.cookies);
    out.intercepts = sortedEntries(out.intercepts);
    out.results = sortedEntries(out.results);
    if (steps.length > 0 || phases.length === 0) phases.push(out);
    else mergeInto(phases.at(-1), out);
  }
  return { sessions, phases };
}

// Events after the last step (run.end only) belong to the last phase.
function mergeInto(into, from) {
  for (const k of ['requests', 'actions', 'snapshots']) into[k].push(...from[k]);
  for (const k of ['cookies', 'intercepts', 'results']) into[k] = sortedEntries([...into[k], ...from[k]]);
}

function interceptOf(e) {
  if (e.null === true) return { session: e.session, null: true };
  return { session: e.session, status: e.status, fields: sortedFields(e.headers), body_len: e.body_len, body_sha256: e.body_sha256, complete: e.complete === true };
}

/** The step objects of the `step.start` events of a run, without `seq` (check 7). */
export function traceSteps(events) {
  return events.filter((e) => e.ev === 'step.start' && e.src === 'conductor').map((e) => {
    const { seq: _seq, ...step } = isObject(e.step) ? e.step : {};
    return step;
  });
}

/**
 * DL-119: a `content-type` line as a media type (the form of server check 2, mediaTypeKey):
 * the Tomcat form `text/html;charset=utf-8` equals `text/html; charset=utf-8`. Other lines stay.
 */
const mediaTypeLine = ([n, v]) => (n === 'content-type' ? [n, mediaTypeKey(v)] : [n, v]);

/** Normalization of a `client` member before the comparison (README items 2 to 5). */
export function normalizeClient(client) {
  const fields = (list) => (Array.isArray(list) ? list.filter((p) => isPair(p) && !TRANSPORT.has(p[0]) && !p[0].startsWith('x-android-')).map(mediaTypeLine) : list);
  const intercept = (i) => {
    if (i.null === true) return i;
    const out = { ...i, fields: fields(i.fields) };
    if (i.complete === false) {
      delete out.body_len;
      delete out.body_sha256;
    }
    return out;
  };
  const record = (r) => ({ ...r, data_sha256: r.data_sha256 !== null, fields: fields(r.fields) });
  return {
    sessions: client.sessions,
    phases: client.phases.map((p) => ({
      ...p,
      actions: p.actions.map((a) => (a.do === 'load_data' ? { ...a, fields: fields(a.fields) } : a)),
      intercepts: sortedEntries(p.intercepts.map(intercept)),
      snapshots: p.snapshots.map((s) => ({ entries: s.entries.map(record) })),
    })),
  };
}

/** One line per compared item, in the order of the README table "client.phases". */
export function clientLines(client) {
  const lines = client.sessions.map((s) => `session ${entryText(s)}`);
  client.phases.forEach((p, i) => {
    lines.push(`phase ${i + 1} steps ${entryText(p.steps)}`);
    for (const k of ['requests', 'cookies', 'actions', 'intercepts', 'results', 'snapshots']) {
      for (const item of p[k]) lines.push(`phase ${i + 1} ${k} ${entryText(item)}`);
    }
  });
  return lines;
}

// U-13 (legacy wire): the legacy fields that an upstream server adds outside the C-01 scope.
const U13_FIELDS = new Set(['etag', 'template-tag', 'template-change', 'cache-offline', 'sonic-etag-key']);

/**
 * DL-120: a response outside the C-01 scope for U-13. Its status is neither 200 nor 304: a 304 is
 * the legacy answer to a 200 (C-01 scope), and its fields are client input (CS-27).
 */
const outsideScope = (response) => response !== null && typeof response === 'object' && response.status !== 200 && response.status !== 304;

function withoutU13(client) {
  return {
    ...client,
    phases: client.phases.map((p) => ({
      ...p,
      requests: p.requests.map((r) => (outsideScope(r.response) ? { ...r, response: { ...r.response, fields: r.response.fields.filter(([n]) => !U13_FIELDS.has(n)) } } : r)),
    })),
  };
}

/** DL-120: the trace has a U-13 field on a response outside the scope that the expected response does not have. */
function showsU13(expected, trace) {
  const want = new Map();
  for (const p of expected.phases) for (const r of p.requests) want.set(r.exchange, r.response);
  for (const p of trace.phases) {
    for (const r of p.requests) {
      if (!outsideScope(r.response)) continue;
      const names = new Set((want.get(r.exchange)?.fields ?? []).map(([n]) => n));
      if (r.response.fields.some(([n]) => U13_FIELDS.has(n) && !names.has(n))) return true;
    }
  }
  return false;
}

/**
 * Compare two `client` members after the normalization. Returns null when equal, else a
 * text that names the first difference.
 *
 * `notReproduced` "U-13" (DL-120, a client profile fixture): the U-13 fields (etag,
 * template-tag, template-change, cache-offline, sonic-etag-key) of a response whose status is
 * not 200 or 304 are not compared, and the trace must have at least one that the expected
 * response does not have (the documented upstream difference).
 */
export function compareClient(expected, actual, { notReproduced = null } = {}) {
  let a = normalizeClient(expected);
  let b = normalizeClient(actual);
  if (notReproduced !== null) {
    if (!CLIENT_NOT_REPRODUCIBLE.includes(notReproduced)) return `not_reproduced ${JSON.stringify(notReproduced)} has no client check`;
    if (!showsU13(a, b)) return 'U-13: the trace does not show the documented difference (no legacy field on a response whose status is not 200 or 304)';
    a = withoutU13(a);
    b = withoutU13(b);
  }
  if (isDeepStrictEqual(a, b)) return null;
  const d = firstDifference([clientLines(a), clientLines(b)]);
  if (d === null) return 'the projections differ';
  return `first difference at item ${d.index + 1}: expected ${d.a ?? '(nothing)'}; trace ${d.b ?? '(nothing)'}`;
}

/**
 * The projection of a scenario mode from all its runs. All runs must give one projection
 * (README check 7). Returns { client } or { error } (a text).
 */
export function projectRuns(runs, context) {
  const projections = [];
  for (const [i, events] of runs.entries()) {
    try {
      projections.push(projectRun(events, context));
    } catch (error) {
      if (!(error instanceof TraceProjectionError)) throw error;
      return { error: `run ${i + 1}: ${error.message}` };
    }
  }
  if (projections.length === 0) return { error: 'no stored run' };
  const d = firstDifference(projections.map((p) => clientLines(normalizeClient(p))));
  if (d !== null) return { error: `run ${d.run + 1} differs from run 1 at item ${d.index + 1}: ${d.a ?? '(nothing)'} / ${d.b ?? '(nothing)'}` };
  return { client: projections[0] };
}
