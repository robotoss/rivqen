// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// The determinism rule (CONTRACT section 6). Normalization: remove `t`, `seq` and the `thread` of
// worker threads; blank the values of Date, X-Android-Sent-Millis and X-Android-Received-Millis;
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

/** Blank the value of the lines `Name: value` of a header file text for the blanked fields. */
export function normalizeHeaderText(text) {
  return text
    .split('\n')
    .map((line) => {
      const at = line.indexOf(':');
      return at > 0 && BLANKED.has(line.slice(0, at).trim().toLowerCase()) ? `${line.slice(0, at)}:` : line;
    })
    .join('\n');
}

function normalizeMessage(json) {
  try {
    const parsed = JSON.parse(json);
    if (parsed !== null && typeof parsed === 'object' && !Array.isArray(parsed)) {
      delete parsed.local_refresh_time;
      return JSON.stringify(parsed);
    }
  } catch {
    // not JSON: compared as it is
  }
  return json;
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
