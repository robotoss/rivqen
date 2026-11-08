// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// Run log rules of CONTRACT.md sections 5 and 8: shape of each line, and the normalized form that runs are compared by.

/** Known event names and the source each one is written by (CONTRACT section 5). */
export const SOURCE_OF = new Map([
  ['run.start', 'script'],
  ['run.end', 'script'],
  ['app.fetch.start', 'app'],
  ['app.fetch.head', 'app'],
  ['app.fetch.end', 'app'],
  ['app.intercept', 'app'],
  ['app.stream.read', 'app'],
  ['app.nav', 'app'],
  ['app.probe', 'app'],
  ['app.ssl_error', 'app'],
  ['app.error', 'app'],
  ['host.request', 'host'],
  ['host.response', 'host'],
  ['host.chunk', 'host'],
  ['host.beacon', 'host'],
  ['host.gate_timeout', 'host'],
  ['host.cut', 'host'],
]);

const isObject = (v) => v !== null && typeof v === 'object' && !Array.isArray(v);

/** Parses one JSON Lines run log. Returns the parsed objects and one message per problem. */
export function parseRunLog(text) {
  const problems = [];
  const events = [];
  if (text.includes('\r')) problems.push('contains CR (the log is LF only)');
  const lines = text.split('\n');
  if (lines.at(-1) === '') lines.pop();
  if (lines.length === 0) problems.push('empty log');
  lines.forEach((line, index) => {
    const n = index + 1;
    let obj;
    try {
      obj = JSON.parse(line);
    } catch {
      problems.push(`line ${n}: not JSON`);
      return;
    }
    if (!isObject(obj)) {
      problems.push(`line ${n}: not a JSON object`);
      return;
    }
    events.push(obj);
    const keys = Object.keys(obj);
    const sorted = [...keys].sort();
    if (keys.some((k, i) => k !== sorted[i])) problems.push(`line ${n}: keys are not sorted`);
    if (!SOURCE_OF.has(obj.ev)) {
      problems.push(`line ${n}: unknown ev ${JSON.stringify(obj.ev)}`);
    } else if (obj.src !== SOURCE_OF.get(obj.ev)) {
      problems.push(`line ${n}: src ${JSON.stringify(obj.src)} is wrong for ${obj.ev}`);
    }
    if (!Number.isInteger(obj.seq) || obj.seq < 0) problems.push(`line ${n}: seq must be an integer >= 0`);
    if (obj.ev === 'app.stream.read' && (!Number.isInteger(obj.bytes) || obj.bytes < 0)) {
      problems.push(`line ${n}: app.stream.read bytes must be an integer >= 0`);
    }
  });
  if (events.length > 0 && events[0].ev !== 'run.start') problems.push('the first event is not run.start');
  if (events.length > 0 && events[0].ev === 'run.start' && (typeof events[0].run !== 'string' || events[0].run === '')) {
    problems.push('run.start has no run id');
  }
  if (events.length > 0 && events.at(-1).ev !== 'run.end') problems.push('the last event is not run.end');
  return { events, problems };
}

/** Blanks the value of a field named Date (any letter case) inside a `fields` member (CONTRACT section 8). */
const isDate = (name) => typeof name === 'string' && name.toLowerCase() === 'date';
function blankDates(fields) {
  if (Array.isArray(fields)) {
    return fields.map((f) => {
      if (Array.isArray(f) && isDate(f[0])) return [f[0], ''];
      if (!isObject(f)) return f;
      if (isDate(f.name)) return { ...f, value: '' };
      return Object.fromEntries(Object.entries(f).map(([k, v]) => [k, isDate(k) ? '' : v]));
    });
  }
  if (isObject(fields)) return Object.fromEntries(Object.entries(fields).map(([k, v]) => [k, isDate(k) ? '' : v]));
  return fields;
}

/**
 * Replaces every occurrence of the run id, and of its CSP nonce form (`.` replaced by `_`), in every string
 * value with `<run>` (CONTRACT sections 3 and 8).
 */
function replaceRunId(value, runId) {
  if (typeof value === 'string') return value.split(runId).join('<run>').split(runId.replaceAll('.', '_')).join('<run>');
  if (Array.isArray(value)) return value.map((v) => replaceRunId(v, runId));
  if (isObject(value)) return Object.fromEntries(Object.entries(value).map(([k, v]) => [k, replaceRunId(v, runId)]));
  return value;
}

/**
 * Normalized form of a run (CONTRACT section 8). `t` and `seq` are removed, `k` is removed from `run.start`,
 * the run id is replaced by `<run>` in every string value, `app.stream.read` events are replaced by the sum
 * of their bytes, `metric.*` probes are removed and `Date` values are blanked.
 * The events are grouped by source; each group keeps its order.
 */
export function normalizeRunLog(events) {
  const runId = events[0]?.run;
  const out = { app: [], host: [], script: [], stream_bytes: 0 };
  for (const e of events) {
    if (e.ev === 'app.stream.read') {
      out.stream_bytes += e.bytes;
      continue;
    }
    if (e.ev === 'app.probe' && typeof e.name === 'string' && e.name.startsWith('metric.')) continue;
    const { t, seq, ...rest } = e;
    if (e.ev === 'run.start') delete rest.k;
    if ('fields' in rest) rest.fields = blankDates(rest.fields);
    const named = typeof runId === 'string' && runId !== '' ? replaceRunId(rest, runId) : rest;
    out[e.src].push(named);
  }
  return out;
}
