// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// The stored exchanges of a canonical client fixture (fixtures/legacy/README.md "Client
// exchanges.json", DL-216): the bytes that the canonical model gives the client in one run, so
// that a test runs the fixture without this model.
//
// exchangeFile() makes the file from the `wire` member of a model run (src/client-model.mjs).
// explainProjection() checks that the projection of the same run (expected.json `client`)
// follows from the stored bytes; the tool tests run it over every canonical fixture.
//
// Both functions are pure and do not throw for the output of a model run. explainProjection()
// reads a stored file and returns problems for a value of the wrong shape.

import { canonicalSplit } from './client-canonical.mjs';
import { sha1hex, sha256hex } from './client-engine.mjs';
import { REQUEST_FIELDS, RESPONSE_FIELDS } from './client-trace.mjs';
import { asciiLower } from './fields.mjs';
import { rebuild } from './split.mjs';

/** The file name in the fixture directory. */
export const EXCHANGES = 'exchanges.json';
/** The `format` member: a reader refuses another value. */
export const FORMAT = 1;

/** A body form: a reference to pages/ when `page` names the same bytes, else base64. */
function bodyForm(bytes, page) {
  const meta = { len: bytes.length, sha256: sha256hex(bytes) };
  return page === null ? { base64: bytes.toString('base64'), ...meta } : { page, ...meta };
}

const pairs = (list) => list.map(([n, v]) => [n, v]);

function recordOf({ record, page }) {
  return {
    partition: record.partition,
    url: record.url,
    vary: pairs(record.vary),
    etag: record.etag,
    template_tag: record.template_tag,
    page: bodyForm(Buffer.from(record.page, 'utf8'), page),
    template: record.template,
    data: pairs(record.data),
    fields: pairs(record.fields),
  };
}

function exchangeOf(e) {
  const r = e.response;
  return {
    exchange: e.exchange,
    step: e.step,
    session: e.session,
    request: { method: e.request.method, target: e.request.target, fields: pairs(e.request.fields) },
    response: r === null ? null : { status: r.status, fields: pairs(r.fields), body: r.body === null ? null : bodyForm(r.body.bytes, r.body.page) },
    stop: e.stop === null ? null : { ...e.stop },
  };
}

/**
 * The exchanges.json value of a canonical client fixture.
 *
 * @param {string} id the fixture ID
 * @param {{ cacheBefore: object[], exchanges: object[] }} wire the `wire` member of a model run
 */
export function exchangeFile(id, wire) {
  return { id, format: FORMAT, cache_before: wire.cacheBefore.map(recordOf), exchanges: wire.exchanges.map(exchangeOf) };
}

/**
 * The bytes of a body form, or null when the form is not valid. `pageOf(id)` gives the bytes of
 * pages/<id>.html or null.
 */
export function bodyBytes(form, pageOf) {
  if (form === null || typeof form !== 'object') return null;
  let bytes = null;
  if (typeof form.page === 'string' && form.base64 === undefined) bytes = pageOf(form.page);
  else if (typeof form.base64 === 'string' && form.page === undefined) bytes = Buffer.from(form.base64, 'base64');
  if (bytes === null) return null;
  const b = Buffer.from(bytes);
  return b.length === form.len && sha256hex(b) === form.sha256 ? b : null;
}

const key = (len, sha) => `${len}:${sha}`;
const picked = (fields, names) => fields.map(([n, v]) => [asciiLower(n), v]).filter(([n]) => names.includes(n));
const same = (a, b) => JSON.stringify(a) === JSON.stringify(b);

/** The data entries of a legacy data body (CL-29 form), or null. */
function dataOf(bytes) {
  let body;
  try {
    body = JSON.parse(bytes.toString('utf8'));
  } catch {
    return null;
  }
  if (body === null || typeof body !== 'object' || body.data === null || typeof body.data !== 'object' || Array.isArray(body.data)) return null;
  const entries = Object.entries(body.data);
  return entries.every(([, v]) => typeof v === 'string') && typeof body['html-sha1'] === 'string' ? { entries, sha1: body['html-sha1'] } : null;
}

/**
 * Check that a `client` projection follows from the stored exchanges of the same run
 * (README "Client exchanges.json", rule 1).
 *
 * - Every request of a phase is a stored exchange with the same method and target; its listed
 *   request fields, its status and its listed response fields come from the stored lines;
 *   `complete: true` only for an exchange without a stop. The numbered exchanges are exactly the
 *   requests of the projection.
 * - Every `load_data` and every complete intercept has the length and SHA-256 of a stored body,
 *   of a `cache_before` page, or of a page rebuilt from a stored template (a `cache_before`
 *   template, or the split of a stored full body) and the data of a stored data body whose
 *   `html-sha1` the page has. An incomplete intercept has the bytes of a prefix of a stored body.
 *
 * @returns {{ problems: string[], counts: { exchange: number, cache: number, rebuilt: number, prefix: number } }}
 */
export function explainProjection(client, file, pageOf) {
  const problems = [];
  const counts = { exchange: 0, cache: 0, rebuilt: 0, prefix: 0 };
  const byNumber = new Map();
  const bodies = [];
  for (const e of file.exchanges) {
    if (e.exchange !== null) byNumber.set(e.exchange, e);
    if (e.response === null || e.response.body === null) continue;
    const b = bodyBytes(e.response.body, pageOf);
    if (b === null) problems.push(`exchange ${e.exchange}: the body does not match its len and sha256`);
    else bodies.push(b);
  }
  const fromExchange = new Set(bodies.map((b) => key(b.length, sha256hex(b))));
  const fromCache = new Set();
  const templates = new Set();
  for (const r of file.cache_before) {
    const b = bodyBytes(r.page, pageOf);
    if (b === null) problems.push(`cache_before ${r.url}: the page does not match its len and sha256`);
    else fromCache.add(key(b.length, sha256hex(b)));
    templates.add(r.template);
  }
  for (const b of bodies) {
    const parts = canonicalSplit(b.toString('utf8'));
    if (parts !== null) templates.add(parts.template);
  }
  const rebuilt = new Set();
  for (const b of bodies) {
    const d = dataOf(b);
    if (d === null) continue;
    for (const t of templates) {
      const page = Buffer.from(rebuild(t, new Map(d.entries)), 'utf8');
      if (sha1hex(page) === d.sha1) rebuilt.add(key(page.length, sha256hex(page)));
    }
  }

  let requests = 0;
  for (const phase of client.phases) {
    for (const q of phase.requests) {
      requests += 1;
      const e = byNumber.get(q.exchange);
      if (e === undefined) {
        problems.push(`exchange ${q.exchange}: not stored`);
        continue;
      }
      if (e.request.method !== q.method || e.request.target !== q.target) problems.push(`exchange ${q.exchange}: the method or target differs`);
      if (!same(picked(e.request.fields, REQUEST_FIELDS), q.fields)) problems.push(`exchange ${q.exchange}: the request fields differ`);
      if (q.response === null) continue; // held before the head until the end of the run
      if (e.response.status !== q.response.status) problems.push(`exchange ${q.exchange}: the status differs`);
      if (!same(picked(e.response.fields, RESPONSE_FIELDS), q.response.fields)) problems.push(`exchange ${q.exchange}: the response fields differ`);
      // complete false without a stop: a hold that is never released (nothing to compare).
      if (e.stop !== null && q.response.complete) problems.push(`exchange ${q.exchange}: complete, but the stored exchange stops`);
    }
  }
  if (requests !== byNumber.size) problems.push(`${byNumber.size} stored exchanges, ${requests} requests in the projection`);

  const explain = (where, len, sha, complete) => {
    const k = key(len, sha);
    if (!complete) {
      if (bodies.some((b) => len <= b.length && sha256hex(b.subarray(0, len)) === sha)) counts.prefix += 1;
      else problems.push(`${where}: an incomplete body that is no prefix of a stored body`);
    } else if (fromExchange.has(k)) counts.exchange += 1;
    else if (fromCache.has(k)) counts.cache += 1;
    else if (rebuilt.has(k)) counts.rebuilt += 1;
    else problems.push(`${where}: body_len ${len} and body_sha256 follow from no stored bytes`);
  };
  client.phases.forEach((phase, i) => {
    for (const a of phase.actions) if (a.do === 'load_data') explain(`phase ${i + 1} load_data`, a.body_len, a.body_sha256, true);
    for (const x of phase.intercepts) if (x.null !== true) explain(`phase ${i + 1} intercept`, x.body_len, x.body_sha256, x.complete);
  });
  return { problems, counts };
}
