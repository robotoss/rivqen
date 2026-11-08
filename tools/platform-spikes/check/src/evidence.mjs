// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// Evidence rules of CONTRACT.md sections 5, 7, 8 and 9: linked files exist inside the root, run logs have the
// event shape, results have stable runs, recordings stay within the caps.
import { readFile, realpath, stat } from 'node:fs/promises';
import path from 'node:path';
import { normalizeRunLog, parseRunLog } from './runs.mjs';

export const MIN_RUNS = 3;
export const RECORDING_MAX_BYTES = 1_000_000;
export const RECORDINGS_TOTAL_MAX_BYTES = 25_000_000;

const isObject = (v) => v !== null && typeof v === 'object' && !Array.isArray(v);

/** Resolves a catalog path under the real root. Symbolic links that leave the root count as outside. */
export async function locate(realRoot, rel) {
  if (typeof rel !== 'string' || rel === '' || path.isAbsolute(rel)) return { state: 'outside' };
  const lexical = path.resolve(realRoot, rel);
  if (!lexical.startsWith(realRoot + path.sep)) return { state: 'outside' };
  let real;
  try {
    real = await realpath(lexical);
  } catch (cause) {
    if (cause.code === 'ENOENT') return { state: 'missing' };
    throw cause;
  }
  if (!real.startsWith(realRoot + path.sep)) return { state: 'outside' };
  const info = await stat(real);
  if (!info.isFile()) return { state: 'missing' };
  return { state: 'ok', abs: real, size: info.size };
}

const describe = (loc, rel) => (loc.state === 'outside' ? `path leaves the root: ${rel}` : `missing file ${rel}`);

/**
 * Checks the linked files and the run logs of every result, and the recording caps.
 * Returns the problems and the counts of checked results, runs and recordings.
 */
export async function evidenceProblems(catalog, root) {
  const realRoot = await realpath(root);
  const problems = [];
  const counts = { results: 0, runs: 0, recordings: 0 };
  const recordings = new Map();
  for (const row of Array.isArray(catalog) ? catalog : []) {
    if (!isObject(row) || !Array.isArray(row.results)) continue;
    for (const [index, r] of row.results.entries()) {
      if (!isObject(r)) continue;
      counts.results += 1;
      const label = `${row.id}/${r.os ?? `result ${index + 1}`}`;
      for (const key of ['metrics', 'recording']) {
        if (typeof r[key] !== 'string') continue;
        const loc = await locate(realRoot, r[key]);
        if (loc.state !== 'ok') {
          problems.push(`${label}: ${describe(loc, r[key])}`);
        } else if (key === 'recording') {
          recordings.set(loc.abs, loc.size);
          if (loc.size > RECORDING_MAX_BYTES) {
            problems.push(`${label}: recording is ${loc.size} bytes, the limit is ${RECORDING_MAX_BYTES}`);
          }
        }
      }
      const runs = Array.isArray(r.runs) ? r.runs.filter((p) => typeof p === 'string') : [];
      const distinct = [...new Set(runs)];
      // A benchmark result is backed by its metrics file (CONTRACT section 10), not by run logs.
      const needsRuns = r.kind !== 'benchmark' && (r.verdict === 'supported' || r.verdict === 'conditional' || runs.length > 0);
      if (needsRuns && distinct.length < MIN_RUNS) {
        problems.push(`${label}: ${distinct.length} runs, at least ${MIN_RUNS} are needed`);
      }
      const normalized = [];
      for (const rel of distinct) {
        const loc = await locate(realRoot, rel);
        if (loc.state !== 'ok') {
          problems.push(`${label}: ${describe(loc, rel)}`);
          continue;
        }
        const parsed = parseRunLog(await readFile(loc.abs, 'utf8'));
        counts.runs += 1;
        for (const p of parsed.problems) problems.push(`${label}: ${rel}: ${p}`);
        if (parsed.problems.length === 0) {
          // The run must belong to this capability and result, and it must have ended ok.
          const start = parsed.events[0];
          for (const [key, want] of [['capability', row.id], ['platform', row.platform], ['os', r.os]]) {
            if (String(start[key]) !== String(want)) {
              problems.push(`${label}: ${rel}: run.start ${key} ${JSON.stringify(start[key])} is not ${JSON.stringify(want)}`);
            }
          }
          if (parsed.events.at(-1).ok !== true) problems.push(`${label}: ${rel}: run.end is not ok`);
        }
        if (parsed.problems.length === 0) {
          normalized.push({ rel, json: JSON.stringify(normalizeRunLog(parsed.events)) });
        }
      }
      for (const other of normalized.slice(1)) {
        if (other.json !== normalized[0].json) {
          problems.push(`${label}: ${other.rel} differs from ${normalized[0].rel} after normalization`);
        }
      }
    }
  }
  let total = 0;
  for (const size of recordings.values()) total += size;
  counts.recordings = recordings.size;
  if (total > RECORDINGS_TOTAL_MAX_BYTES) {
    problems.push(`recordings total ${total} bytes, the limit is ${RECORDINGS_TOTAL_MAX_BYTES}`);
  }
  return { problems, counts };
}
