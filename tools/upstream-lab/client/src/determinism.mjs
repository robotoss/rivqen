// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// The determinism rule (CONTRACT section 6). Normalization: remove `t`, `seq` and the `thread` of
// worker threads and the run number `repeat` of run.start; blank the values of Date, X-Android-Sent-Millis and X-Android-Received-Millis;
// remove `local_refresh_time` from the JSON of `session.message`; compare a `*.header` cache file by
// its normalized `text`. Comparison: per step window, the harness events in `hseq` order and the
// proxy events in `exchange` order, each source on its own; the interleaving is not compared.
import { canonicalLine } from './canon.mjs';

const BLANKED = new Set(['date', 'x-android-sent-millis', 'x-android-received-millis']);
const isPair = (v) => Array.isArray(v) && v.length === 2 && typeof v[0] === 'string' && typeof v[1] === 'string';

function scrub(value) {
  if (isPair(value) && BLANKED.has(value[0].toLowerCase())) return [value[0], ''];
  if (Array.isArray(value)) return value.map(scrub);
  if (value !== null && typeof value === 'object') {
    return Object.fromEntries(Object.entries(value).map(([k, v]) => [k, scrub(v)]));
  }
  return value;
}

/**
 * Blank the value of the blanked fields in the text of a header file. The harness writes one field per
 * line as `name : value` (lower-case name, CRLF line end); other spacing and LF line ends work too.
 */
export function normalizeHeaderText(text) {
  return text.replace(/^([^:\r\n]+?)(\s*:)[^\r\n]*/gm, (line, name, colon) => (BLANKED.has(name.trim().toLowerCase()) ? `${name}${colon}` : line));
}

const REMOVED_KEY = 'local_refresh_time';

/** End index (exclusive) of the JSON string that starts at `i` (a quote). */
function stringEnd(text, i) {
  let j = i + 1;
  while (j < text.length && text[j] !== '"') j += text[j] === '\\' ? 2 : 1;
  return j + 1;
}

/**
 * Remove the member `local_refresh_time` of the top-level object from the JSON text. The rest of the text stays
 * byte for byte, so a changed value type or number form still shows (a parse and stringify would hide it).
 * Text that is not a JSON object, or has no such member, is returned as it is.
 */
function normalizeMessage(json) {
  try {
    const parsed = JSON.parse(json);
    if (parsed === null || typeof parsed !== 'object' || Array.isArray(parsed) || !Object.hasOwn(parsed, REMOVED_KEY)) return json;
  } catch {
    return json; // not JSON: compared as it is
  }
  // [from, to) of each member of the top-level object, split at the commas of depth 1.
  const open = json.indexOf('{');
  const members = [];
  let depth = 0;
  let from = open + 1;
  for (let i = open; i < json.length; i += 1) {
    const c = json[i];
    if (c === '"') {
      i = stringEnd(json, i) - 1;
    } else if (c === '{' || c === '[') {
      depth += 1;
    } else if (c === '}' || c === ']') {
      depth -= 1;
      if (depth === 0) {
        members.push([from, i]);
        break;
      }
    } else if (c === ',' && depth === 1) {
      members.push([from, i]);
      from = i + 1;
    }
  }
  const keyOf = ([start]) => {
    const q = json.indexOf('"', start);
    return JSON.parse(json.slice(q, stringEnd(json, q)));
  };
  const kept = members.filter((m) => keyOf(m) !== REMOVED_KEY);
  return `${json.slice(0, open + 1)}${kept.map(([a, b]) => json.slice(a, b)).join(',')}${json.slice(members.at(-1)[1])}`;
}

function normalizeSnapshotFile(file) {
  if (typeof file?.name !== 'string' || !file.name.endsWith('.header') || typeof file.text !== 'string') return file;
  return { name: file.name, text: normalizeHeaderText(file.text) };
}

/** One event as the determinism rule compares it. */
export function normalizeEvent(event) {
  const copy = scrub(event);
  delete copy.t;
  delete copy.seq;
  delete copy.hseq;
  if (copy.thread !== 'main') delete copy.thread;
  // `repeat` of run.start is the run number k (the file run-<k>.jsonl); it differs by construction.
  if (copy.ev === 'run.start') delete copy.repeat;
  if (copy.ev === 'session.message' && typeof copy.json === 'string') copy.json = normalizeMessage(copy.json);
  if (copy.ev === 'cache.snapshot' && Array.isArray(copy.files)) copy.files = copy.files.map(normalizeSnapshotFile);
  return copy;
}

/** Events split at each `step.start`; the events before the first one form the first window. */
export function stepWindows(events) {
  const windows = [[]];
  for (const e of events) {
    if (e.ev === 'step.start') windows.push([]);
    windows.at(-1).push(e);
  }
  return windows;
}

const byKey = (key) => (a, b) => key(a) - key(b); // Array.prototype.sort is stable

/** The events of one window in the order that the comparison uses. */
export function orderWindow(window) {
  const harness = window.filter((e) => e.src === 'harness').sort(byKey((e) => (Number.isInteger(e.hseq) ? e.hseq : Infinity)));
  const proxy = window.filter((e) => e.src === 'proxy').sort(byKey((e) => (Number.isInteger(e.exchange) ? e.exchange : Infinity)));
  const rest = window.filter((e) => e.src !== 'harness' && e.src !== 'proxy');
  return [...rest, ...harness, ...proxy];
}

/** The normalized lines of a run (array of canonical JSON strings). */
export function normalizeRun(events) {
  return stepWindows(events).flatMap(orderWindow).map((e) => canonicalLine(normalizeEvent(e)));
}

/**
 * Outcome projection (an addition of WP-01 S1 T-05; a proposal for CONTRACT section 6). The strict
 * rule above compares the harness events of a step window in `hseq` order. The SDK emits events on
 * its own threads (session thread, file thread, intercept readers) while the script goes on, so an
 * event can fall into the next step window, and two harness threads can interleave, without any
 * change of what the client did. The projection compares what the client did:
 *
 * - phases instead of step windows: a phase ends with the step after a `settle` (the script waits
 *   there for the SDK to finish), so asynchronous events stay inside one phase;
 * - inside a phase: the conductor events in order, the main-thread harness events in `hseq` order
 *   (one thread, so their order is behavior), the other harness events as a sorted multiset, the
 *   proxy events in `exchange` order.
 */
export function settlePhases(events) {
  const phases = [[]];
  let settling = false;
  for (const e of events) {
    if (e.ev === 'step.start' && settling) {
      phases.push([]);
      settling = false;
    }
    phases.at(-1).push(e);
    if (e.ev === 'step.start' && e.src === 'conductor' && e.step?.do === 'settle') settling = true;
  }
  return phases;
}

/** The normalized lines of a run in the outcome projection. */
export function outcomeRun(events) {
  return settlePhases(events).flatMap((phase) => {
    const line = (e) => canonicalLine(normalizeEvent(e));
    const conductor = phase.filter((e) => e.src !== 'harness' && e.src !== 'proxy').map(line);
    const main = phase.filter((e) => e.src === 'harness' && e.thread === 'main').sort(byKey((e) => (Number.isInteger(e.hseq) ? e.hseq : Infinity))).map(line);
    const others = phase.filter((e) => e.src === 'harness' && e.thread !== 'main').map(line).sort();
    const proxy = phase.filter((e) => e.src === 'proxy').sort(byKey((e) => (Number.isInteger(e.exchange) ? e.exchange : Infinity))).map(line);
    return ['--- phase', ...conductor, ...main, ...others, ...proxy];
  });
}

/**
 * Compare normalized runs with the first one.
 * @param {string[][]} runs
 * @returns {null | { run: number, index: number, a: string | null, b: string | null }}
 *   the first difference: run number (index in `runs`), line index, and the two lines
 */
export function firstDifference(runs) {
  const [base, ...rest] = runs;
  for (let r = 0; r < rest.length; r++) {
    const other = rest[r];
    const n = Math.max(base.length, other.length);
    for (let i = 0; i < n; i++) {
      if (base[i] !== other[i]) return { run: r + 1, index: i, a: base[i] ?? null, b: other[i] ?? null };
    }
  }
  return null;
}
