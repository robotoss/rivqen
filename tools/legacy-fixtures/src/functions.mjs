// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// Function fixtures of the client part (fixtures/legacy/README.md "Function fixtures"): one
// generated file per profile (legacy-canonical, legacy-android, legacy-ios) for the cases of
// tools/upstream-lab/client/functions.json, and the check of each file against the stored
// function evidence (README check 12).
//
// legacy-canonical results come from the CL rules (src/client-canonical.mjs). A platform result
// with evidence (`trace`) is the evidence line, reduced to the members of the README table; a
// platform result without evidence (`unsupported`) is `code-reading` from the profile model
// (src/client-android.mjs) or `none`. Where the Android session model uses a function (session
// id, split, diff, etag), the model is also compared with every evidence line of that function,
// so the session model and the function evidence cannot drift apart.
//
// Untrusted input: the case file and the evidence lines. Each line and each case is checked;
// a defect is a problem text, never an exception.

import {
  bareEtag, canonicalDiff, canonicalMessage, canonicalSplit, charsetOf, isDataUpdateValue, isLegacyResponse,
} from './client-canonical.mjs';
import { androidDiff, androidEtag, androidMessage, androidSessionId, androidSplit, androidTemplateChange } from './client-android.mjs';
import { split } from './split.mjs';

export const FUNCTION_PROFILES = Object.freeze(['legacy-canonical', 'legacy-android', 'legacy-ios']);
export const FUNCTIONS = Object.freeze(['session_id', 'split', 'diff', 'message', 'is_legacy_response', 'template_change', 'etag', 'charset']);
export const CASES_PATH = 'fixtures/legacy/functions/cases.json';
export const EVIDENCE = Object.freeze({
  'legacy-canonical': [],
  'legacy-android': ['evidence/traces/client/android/functions.jsonl'],
  'legacy-ios': ['evidence/traces/client/ios/functions.jsonl', 'evidence/traces/client/ios/boolvalue.jsonl'],
});
const MAX_CASES = 4096;

/** The members of a result that the check compares (README table). */
const RESULT_MEMBERS = Object.freeze({
  session_id: ['id'],
  split: ['template', 'data'],
  diff: ['diff'],
  message: ['json'],
  is_legacy_response: ['value'],
  template_change: ['value'],
  etag: ['value'],
  charset: ['value'],
});

const isObject = (v) => v !== null && typeof v === 'object' && !Array.isArray(v);
const pairsOf = (fields) => (Array.isArray(fields) ? fields.filter((p) => Array.isArray(p) && p.length === 2 && typeof p[0] === 'string' && typeof p[1] === 'string') : isObject(fields) ? Object.entries(fields).filter(([, v]) => typeof v === 'string') : []);
const entriesOf = (o) => (isObject(o) ? Object.entries(o) : []);
const pick = (fn, result) => (isObject(result) ? Object.fromEntries(RESULT_MEMBERS[fn].filter((k) => Object.hasOwn(result, k)).map((k) => [k, result[k]])) : result);

/** Parse the case file. Returns { cases } or { error }. */
export function parseCases(text) {
  let list;
  try {
    list = JSON.parse(text);
  } catch {
    return { error: 'the case file is not JSON' };
  }
  if (!Array.isArray(list) || list.length > MAX_CASES) return { error: `the case file must be an array of at most ${MAX_CASES} cases` };
  const seen = new Set();
  for (const [i, c] of list.entries()) {
    if (!isObject(c) || typeof c.id !== 'string' || !FUNCTIONS.includes(c.fn) || !isObject(c.input)) return { error: `case ${i + 1}: needs id, a known fn and an input object` };
    if (seen.has(c.id)) return { error: `case ${c.id}: the ID repeats` };
    seen.add(c.id);
  }
  return { cases: list };
}

/** Parse a JSON Lines evidence file into a Map by id (`key`: the member that names the case). */
export function parseEvidence(text, key = 'id') {
  const out = new Map();
  for (const [i, line] of text.split('\n').entries()) {
    if (line.trim() === '') continue;
    let o;
    try {
      o = JSON.parse(line);
    } catch {
      return { error: `line ${i + 1} is not JSON` };
    }
    if (!isObject(o) || typeof o[key] !== 'string') return { error: `line ${i + 1} has no ${key}` };
    out.set(o[key], o);
  }
  return { lines: out };
}

// --- legacy-canonical (CL-14, CL-15, CL-16, CL-24, CL-28, CL-41, CL-42, CL-43) ---

function canonicalEntry(c) {
  const input = c.input;
  const base = { id: c.id, fn: c.fn };
  switch (c.fn) {
    case 'session_id':
      return { ...base, evidence: 'none', result: null, note: 'CL-43: the canonical client computes no session id' };
    case 'split': {
      const parts = typeof input.html === 'string' ? canonicalSplit(input.html) : null;
      if (parts === null) {
        const reason = typeof input.html === 'string' ? split('legacy-canonical', input.html).reason : 'not-a-string';
        return { ...base, evidence: 'rule', rules: ['CL-41'], error: reason === 'DUPLICATE_BLOCK_NAME' ? 'duplicate-block-name' : reason };
      }
      return { ...base, evidence: 'rule', rules: ['CL-41'], result: { template: parts.template, data: Object.fromEntries(parts.data) } };
    }
    case 'diff': {
      const server = entriesOf(input.server);
      const diff = input.local === null ? canonicalDiff([], server) : canonicalDiff(entriesOf(input.local), server);
      if (diff === null) return { ...base, evidence: 'rule', rules: ['CL-42'], error: 'value-not-string' };
      return { ...base, evidence: 'rule', rules: ['CL-42'], result: { diff: Object.fromEntries(diff) } };
    }
    case 'message': {
      const diff = entriesOf(input.diff);
      const final = input.final_code;
      // CL-23 item 1 and CL-28: no message for a final -1 or for a final 200 with an empty diff.
      const none = final === -1 || (final === 200 && diff.length === 0);
      const json = none ? null : canonicalMessage({ src: input.src_code, final, diff, headers: pairsOf(input.fields), reload: false });
      return { ...base, evidence: 'rule', rules: ['CL-23', 'CL-24', 'CL-28'], result: { json } };
    }
    case 'is_legacy_response':
      return { ...base, evidence: 'rule', rules: ['CL-14'], result: { value: isLegacyResponse(pairsOf(input.fields)) } };
    case 'template_change':
      return { ...base, evidence: 'rule', rules: ['CL-15'], result: { value: !isDataUpdateValue(input.value) } };
    case 'etag':
      return { ...base, evidence: 'rule', rules: ['CL-15'], result: { value: bareEtag(String(input.value)) } };
    default:
      return { ...base, evidence: 'rule', rules: ['CL-16'], result: { value: charsetOf(input.content_type) } };
  }
}

// --- platform profiles ---

/** The Android model of a function, or undefined when the session model does not use it. */
function androidModel(c) {
  const input = c.input;
  switch (c.fn) {
    case 'session_id':
      return { id: androidSessionId(input.url, input.account, input.account_related) };
    case 'split': {
      const parts = androidSplit(String(input.html));
      return { template: parts.template, data: Object.fromEntries(parts.data) };
    }
    case 'diff': {
      const diff = androidDiff(input.local === null ? null : entriesOf(input.local), entriesOf(input.server));
      return { diff: diff === null ? null : Object.fromEntries(diff) };
    }
    case 'etag':
      return { value: androidEtag(input.value) };
    default:
      return undefined;
  }
}

/** A-05, A-06 and D-28: the Android value of a function that the evidence does not reach. */
function androidCodeReading(c) {
  const input = c.input;
  if (c.fn === 'message') {
    const json = androidMessage({ src: input.src_code, final: input.final_code, diff: entriesOf(input.diff), headers: pairsOf(input.fields), reload: false });
    return { evidence: 'code-reading', result: { json } };
  }
  if (c.fn === 'template_change') return { evidence: 'code-reading', result: { value: androidTemplateChange(input.value) } };
  if (c.fn === 'is_legacy_response') return { evidence: 'none', result: null, note: 'D-28: Android has no single legacy response test' };
  return { evidence: 'none', result: null, note: 'no evidence and no FACT row' };
}

function iosCodeReading(c) {
  if (c.fn === 'message') return { evidence: 'none', result: null, note: 'D-26: iOS key order and white space NOT FOUND' };
  return { evidence: 'none', result: null, note: 'no evidence and no FACT row' };
}

function traceEntry(base, line) {
  if (typeof line.error === 'string') return { ...base, evidence: 'trace', error: line.error.split(':')[0] };
  return { ...base, evidence: 'trace', result: pick(base.fn, line.result) };
}

/** Deep equality of JSON values where object member order does not count (the evidence lines have sorted keys). */
function sameValue(a, b) {
  if (Array.isArray(a) || Array.isArray(b)) return Array.isArray(a) && Array.isArray(b) && a.length === b.length && a.every((x, i) => sameValue(x, b[i]));
  if (isObject(a) && isObject(b)) {
    const ka = Object.keys(a).sort();
    const kb = Object.keys(b).sort();
    return sameValue(ka, kb) && ka.every((k) => sameValue(a[k], b[k]));
  }
  return a === b;
}

/**
 * The function fixture of a profile.
 *
 * @param {string} profile one of FUNCTION_PROFILES
 * @param {object[]} cases parsed case file
 * @param {{ android?: Map, ios?: Map, boolvalue?: Map }} evidence parsed evidence lines by id
 * @returns {{ file: object, problems: string[] }}
 */
export function functionFixture(profile, cases, evidence) {
  const problems = [];
  const results = [];
  for (const c of cases) {
    const base = { id: c.id, fn: c.fn };
    if (profile === 'legacy-canonical') {
      results.push(canonicalEntry(c));
      continue;
    }
    const platform = profile === 'legacy-android' ? evidence.android : evidence.ios;
    let line = platform?.get(c.id);
    if (profile === 'legacy-ios' && c.fn === 'template_change') {
      const b = evidence.boolvalue?.get(c.id);
      line = b === undefined ? undefined : typeof b.bool_value === 'boolean' ? { result: { value: b.bool_value } } : { unsupported: 'no bool_value' };
    }
    if (line === undefined) {
      problems.push(`functions/${profile}.json: ${c.id}: no evidence line`);
      continue;
    }
    if (typeof line.unsupported === 'string') {
      results.push({ ...base, ...(profile === 'legacy-android' ? androidCodeReading(c) : iosCodeReading(c)) });
      continue;
    }
    if (typeof line.error !== 'string' && !Object.hasOwn(line, 'result')) {
      problems.push(`functions/${profile}.json: ${c.id}: the evidence line has no result, error or unsupported`);
      continue;
    }
    const entry = traceEntry(base, line);
    results.push(entry);
    if (profile === 'legacy-android') {
      const model = androidModel(c);
      if (model !== undefined && (entry.error !== undefined || !sameValue(model, entry.result))) {
        problems.push(`functions/legacy-android.json: ${c.id}: the Android model gives ${JSON.stringify(model)}, the evidence ${JSON.stringify(entry.result ?? entry.error)}`);
      }
    }
  }
  return { file: { profile, cases: CASES_PATH, evidence: EVIDENCE[profile], results }, problems };
}
