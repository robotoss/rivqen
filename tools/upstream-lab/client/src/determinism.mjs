// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// The determinism rule (CONTRACT section 6): after removing `t`, `thread` of worker threads and the
// values of `Date` fields, the event logs of the runs of one scenario and mode are equal.
import { canonicalLine } from './canon.mjs';

const isPair = (v) => Array.isArray(v) && v.length === 2 && typeof v[0] === 'string' && typeof v[1] === 'string';

function scrub(value) {
  if (isPair(value) && value[0].toLowerCase() === 'date') return [value[0], ''];
  if (Array.isArray(value)) return value.map(scrub);
  if (value !== null && typeof value === 'object') {
    return Object.fromEntries(Object.entries(value).map(([k, v]) => [k, scrub(v)]));
  }
  return value;
}

/** One event as the determinism rule compares it. */
export function normalizeEvent(event) {
  const copy = scrub(event);
  delete copy.t;
  if (copy.thread !== 'main') delete copy.thread;
  return copy;
}

/** The normalized lines of a run (array of canonical JSON strings). */
export function normalizeRun(events) {
  return events.map((e) => canonicalLine(normalizeEvent(e)));
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
