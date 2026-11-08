// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// Catalog rules of CONTRACT.md section 7: field shapes, allowed values and the capability verdict.

export const PLATFORMS = ['android', 'ios', 'ffi', 'url'];
export const RESULT_VERDICTS = ['supported', 'conditional', 'impossible', 'not-tested'];
export const CAPABILITY_VERDICTS = [...RESULT_VERDICTS, 'pending'];
// H-128: a capability verdict other than pending needs a result for every listed version.
export const REQUIRED_OS = { android: ['37', '29'], ios: ['27.0', '26.5', '18.5'] };
// Weakest first: impossible < conditional < supported.
const WEAKEST_FIRST = ['impossible', 'conditional', 'supported'];

const isObject = (v) => v !== null && typeof v === 'object' && !Array.isArray(v);
const nonEmpty = (v) => typeof v === 'string' && v.length > 0;
const strings = (v) => Array.isArray(v) && v.every((s) => typeof s === 'string');

/** Verdict of a capability from its results. Every result must be valid. */
export function expectedVerdict(platform, results) {
  if (results.length === 0 || results.some((r) => r.verdict === 'not-tested')) return 'pending';
  const required = REQUIRED_OS[platform] ?? [];
  if (required.some((os) => !results.some((r) => r.os === os))) return 'pending';
  const ranks = results.map((r) => WEAKEST_FIRST.indexOf(r.verdict));
  return WEAKEST_FIRST[Math.min(...ranks)];
}

/** Problems of one result object, without the capability label. */
function resultProblems(r) {
  if (!isObject(r)) return ['not an object'];
  const out = [];
  if (!nonEmpty(r.os)) out.push('os must be a non-empty string');
  if (!RESULT_VERDICTS.includes(r.verdict)) out.push(`verdict ${JSON.stringify(r.verdict)} is not allowed`);
  if (!strings(r.runs)) out.push('runs must be an array of strings');
  const condition = r.condition ?? null;
  if (condition !== null && !nonEmpty(condition)) out.push('condition must be null or a non-empty string');
  if (r.verdict === 'conditional' && condition === null) out.push('a conditional result needs a condition');
  for (const key of ['recording', 'metrics', 'webview']) {
    if (r[key] !== undefined && r[key] !== null && !nonEmpty(r[key])) out.push(`${key} must be a non-empty string`);
  }
  const hasSource = nonEmpty(r.source) && URL.canParse(r.source);
  if (r.source !== undefined && r.source !== null && !hasSource) out.push('source must be a URL string');
  const hasRun = strings(r.runs) && r.runs.length > 0;
  if (r.verdict === 'impossible' && !hasSource && !hasRun) out.push('an impossible result needs a source or a run');
  if (r.kind !== undefined && r.kind !== 'benchmark') out.push('kind must be "benchmark" when it is set');
  if (r.kind === 'benchmark' && !nonEmpty(r.metrics)) out.push('a benchmark result needs a metrics file');
  if (r.kind === 'benchmark' && hasRun) out.push('a benchmark result has no runs');
  return out;
}

/** Problems of the catalog rows (CONTRACT section 7). Each line starts with the capability id. */
export function catalogProblems(catalog) {
  if (!Array.isArray(catalog)) return ['catalog: not a JSON array'];
  const problems = [];
  const ids = new Set();
  catalog.forEach((row, index) => {
    if (!isObject(row)) {
      problems.push(`row ${index + 1}: not an object`);
      return;
    }
    const label = nonEmpty(row.id) ? row.id : `row ${index + 1}`;
    const add = (text) => problems.push(`${label}: ${text}`);
    if (!nonEmpty(row.id)) add('id must be a non-empty string');
    else if (ids.has(row.id)) add('duplicate id');
    else ids.add(row.id);
    for (const key of ['title', 'prototype', 'sprint']) {
      if (!nonEmpty(row[key])) add(`${key} must be a non-empty string`);
    }
    if (!PLATFORMS.includes(row.platform)) add(`platform ${JSON.stringify(row.platform)} is not allowed`);
    if (!strings(row.questions)) add('questions must be an array of strings');
    if (!CAPABILITY_VERDICTS.includes(row.verdict)) add(`verdict ${JSON.stringify(row.verdict)} is not allowed`);
    if (!Array.isArray(row.results)) {
      add('results must be an array');
      return;
    }
    const oses = new Set();
    row.results.forEach((r, j) => {
      resultProblems(r).forEach((text) => add(`result ${j + 1}: ${text}`));
      if (isObject(r) && nonEmpty(r.os)) {
        if (oses.has(r.os)) add(`duplicate result os ${r.os}`);
        oses.add(r.os);
      }
    });
    const valid = row.results.every((r) => resultProblems(r).length === 0);
    if (valid && CAPABILITY_VERDICTS.includes(row.verdict)) {
      const expected = expectedVerdict(row.platform, row.results);
      if (row.verdict !== expected) add(`verdict is ${row.verdict}, the results give ${expected}`);
    }
  });
  return problems;
}
