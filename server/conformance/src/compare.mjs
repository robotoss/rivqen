// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// Comparison of a raw response with expected.json (CONTRACT.md section 3, item 4).
//
//   RQP:    compareServer and compareServerShould of tools/rqp-protocol/check.mjs
//           (one set of rules, DL-065), plus: a Content-Encoding other than
//           identity fails (the fixtures use no content coding).
//   Legacy: status; fields (fixtures/legacy/README.md: exact lists, [] = absent,
//           names case-insensitive); Content-Encoding absent or gzip (DL-073;
//           gzip is decoded before the body check); the body by kind.
//
// Content-Length: the raw client reads the body by its framing, so a body
// shorter than Content-Length (closed early) or bytes after it fail in the
// client (raw-http.mjs). For HEAD, 204 and 304 the framing has no body and
// Content-Length is not compared (RFC 9110 section 8.6).
//
// `response` = { status, fields: [[name, value]] (raw spelling and order),
// body: Buffer | null }. Each function returns { must: [...], should: [...] }:
// lists of problem texts. They never throw for any response.

import { gunzipSync } from 'node:zlib';
import { isDeepStrictEqual } from 'node:util';
import { compareServer, compareServerShould } from '../../../tools/rqp-protocol/check.mjs';
import { parseStrict } from '../../../tools/rqp-protocol/src/json-strict.mjs';
import { LIMITS } from './raw-http.mjs';
import { MAX_PROBLEMS } from './report.mjs';

// Past the report cap (MAX_PROBLEMS) a problem is counted with one shared string, not built:
// the report keeps the first ones and one `… N more` line, so a hostile body cannot make
// millions of problem strings (DL-078, L3).
const OMITTED = 'problem not listed';
const add = (problems, make) => problems.push(problems.length >= MAX_PROBLEMS ? OMITTED : make());

const valuesOf = (fields, name) => {
  const lower = name.toLowerCase();
  return fields.filter(([n]) => n.toLowerCase() === lower).map(([, v]) => v);
};

/** A short JSON text of a value for a problem text. */
export function brief(value, max = 120) {
  const text = JSON.stringify(value) ?? String(value);
  return text.length > max ? `${text.slice(0, max)}…` : text;
}

/** The server cases of RQP. */
export function compareRqp(expected, page, response) {
  const must = [];
  const encoding = valuesOf(response.fields, 'content-encoding');
  const codings = encoding.flatMap((v) => v.split(',')).map((c) => c.trim().toLowerCase());
  if (codings.some((c) => c !== 'identity')) must.push(`header content-encoding: ${brief(encoding)}, expected no content coding`);
  const forChecker = { status: response.status, headers: response.fields, body: response.body };
  try {
    must.push(...compareServer(expected, page, forChecker));
    return { must, should: compareServerShould(expected, forChecker) };
  } catch (error) {
    must.push(`the RQP comparison stopped (${error instanceof Error ? error.name : typeof error})`);
    return { must, should: [] };
  }
}

/** Decode the body by Content-Encoding: { body } or { problem }. */
function decodeBody(fields, body, limit) {
  const encoding = valuesOf(fields, 'content-encoding');
  if (encoding.length === 0) return { body };
  if (encoding.length !== 1 || encoding[0].toLowerCase() !== 'gzip') {
    return { problem: `header content-encoding: ${brief(encoding)}, expected absent or ["gzip"]` };
  }
  if (body === null || body.length === 0) return { body };
  try {
    return { body: gunzipSync(body, { maxOutputLength: limit }) };
  } catch (error) {
    if (error.code === 'ERR_BUFFER_TOO_LARGE') return { problem: `body: the decoded body is larger than ${limit} bytes` };
    return { problem: `body: gzip decoding failed (${error.code ?? error.name})` };
  }
}

function firstDifference(a, b) {
  const n = Math.min(a.length, b.length);
  for (let i = 0; i < n; i++) if (a[i] !== b[i]) return i;
  return n;
}

const DATA_MEMBERS = Object.freeze({ data: 'data', 'template-tag': 'template_tag', 'html-sha1': 'html_sha1', diff: 'diff' });

/** The data body (README: exactly the members data, template-tag, html-sha1, diff; JSON values, D-08). */
function compareData(want, body) {
  const json = parseStrict(body);
  if (!json.ok) return [`body: not a valid JSON text (${json.rule})`];
  const got = json.value;
  if (got === null || typeof got !== 'object' || Array.isArray(got)) return ['body: not a JSON object'];
  const problems = [];
  for (const name of Object.keys(got)) if (!Object.hasOwn(DATA_MEMBERS, name)) add(problems, () => `body: unknown member ${brief(name)}`);
  for (const [name, key] of Object.entries(DATA_MEMBERS)) {
    if (!Object.hasOwn(got, name)) problems.push(`body: member ${name} is absent`);
    else if (name === 'data' && isPlain(got.data) && isPlain(want.data)) problems.push(...compareDataMap(want.data, got.data));
    else if (!isDeepStrictEqual(got[name], want[key])) problems.push(`body: member ${name}: expected ${brief(want[key])}, got ${brief(got[name])}`);
  }
  return problems;
}

const isPlain = (v) => v !== null && typeof v === 'object' && !Array.isArray(v);

/** The data map, key by key (member order is free). */
function compareDataMap(want, got) {
  const problems = [];
  for (const key of Object.keys(want)) {
    if (!Object.hasOwn(got, key)) add(problems, () => `body: data ${brief(key)} is absent`);
    else if (!isDeepStrictEqual(got[key], want[key])) add(problems, () => `body: data ${brief(key)}: expected ${brief(want[key])}, got ${brief(got[key])}`);
  }
  for (const key of Object.keys(got)) if (!Object.hasOwn(want, key)) add(problems, () => `body: data ${brief(key)} is not expected`);
  return problems;
}

/** The legacy cases. */
export function compareLegacy(expected, page, response, { bodyLimit = LIMITS.body } = {}) {
  const must = [];
  if (response.status !== expected.status) must.push(`status ${response.status}, expected ${expected.status}`);
  for (const [name, want] of Object.entries(expected.fields)) {
    const got = valuesOf(response.fields, name);
    if (!isDeepStrictEqual(got, want)) must.push(`field ${name}: expected ${JSON.stringify(want)}, got ${JSON.stringify(got)}`);
  }
  const decoded = decodeBody(response.fields, response.body, bodyLimit);
  if (decoded.problem !== undefined) {
    must.push(decoded.problem);
    return { must, should: [] };
  }
  const body = decoded.body;
  const kind = expected.body.kind;
  if (kind === 'none') {
    if (body !== null && body.length > 0) must.push(`body: ${body.length} bytes, expected none`);
  } else if (body === null) {
    must.push(`body: none, expected ${kind}`);
  } else if (kind === 'page') {
    if (!body.equals(page)) must.push(`body: not the application body (${body.length} bytes, expected ${page.length}; first difference at byte ${firstDifference(body, page)})`);
  } else {
    must.push(...compareData(expected.body, body));
  }
  return { must, should: [] };
}
