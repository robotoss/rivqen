// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// The byte study (WP-17 S6 T-01): computes every table of the results file
// from the pages of pages.mjs, the reference server step and the options.
// compute() is deterministic: the same Node.js and zlib versions give the
// same object (tests check this).

import { manifestMarginal, manifestRaw, deltaMarginal, deltaRaw } from './breakdown.mjs';
import { headerBlocks, headerObject, http1Bytes } from './hpack.mjs';
import { DELTA_OPTIONS, MANIFEST_OPTIONS, REQUEST_OPTIONS } from './options.mjs';
import { DEMO_CONFIG, DEMO_TARGET, GRID_TARGET, demoPage, gridPage, stripMarkup, toLegacy } from './pages.mjs';
import { handle } from '../../../src/server.mjs';
import { DELTA_TYPE, exchange } from './rqp.mjs';
import { environment, minus, sizes } from './sizes.mjs';

export const GRID_BLOCKS = Object.freeze([1, 4, 16, 64]);
export const GRID_SIZES = Object.freeze(['small', 'large']);
export const GRID_CHANGED = Object.freeze(['one', 'all']);

/** All scenarios: the demo page and the synthetic grid. */
export function scenarios() {
  const list = [{ key: 'demo', kind: 'demo', pageA: demoPage(0), pageB: demoPage(1), target: DEMO_TARGET, config: DEMO_CONFIG }];
  for (const blocks of GRID_BLOCKS) {
    for (const size of GRID_SIZES) {
      for (const changed of GRID_CHANGED) {
        list.push({
          key: `grid-${blocks}-${size}-${changed}`,
          kind: 'grid',
          blocks,
          size,
          changed,
          pageA: gridPage({ blocks, size }),
          pageB: gridPage({ blocks, size, version: 1, changed }),
          target: GRID_TARGET,
          config: {},
        });
      }
    }
  }
  return list;
}

const labelOf = (s) => (s.kind === 'demo' ? { scenario: 'demo' } : { scenario: s.key, blocks: s.blocks, size: s.size, changed_blocks: s.changed });

/** Page, markup, F and D tables of one scenario. */
function bodyTables(s, ex) {
  const plain = stripMarkup(s.pageA);
  const legacy = toLegacy(s.pageA);
  const page = sizes(s.pageA);
  const out = {
    ...labelOf(s),
    page_rqp_markup: page,
    page_no_markup: sizes(plain),
    page_legacy_markers: sizes(legacy),
    markup_cost_data_rq_block: minus(page, sizes(plain)),
    markup_cost_legacy_markers: minus(sizes(legacy), sizes(plain)),
    full: sizes(ex.cold.body),
    manifest_raw: manifestRaw(ex.manifestA),
    manifest_marginal: manifestMarginal(ex),
    revisit_unchanged: { status: ex.unchanged.status, ...sizes(ex.unchanged.body ?? Buffer.alloc(0)) },
    changed_kind: ex.changed.kind,
    changed_reason: ex.changed.reason,
    changed: sizes(ex.changed.body),
    full_after_change: sizes(ex.fullB.body),
  };
  if (s.kind === 'demo') out.page_legacy_markers_h82 = sizes(toLegacy(s.pageA, { titleMarkers: true }));
  if (ex.delta !== null) {
    out.delta_raw = deltaRaw(ex.delta);
    out.delta_marginal = deltaMarginal(ex.delta);
  }
  return out;
}

function optionTable(options, list, pick) {
  return Object.entries(options).map(([id, fn]) => ({
    id,
    results: list.flatMap(({ s, ex }) => {
      const picked = pick(ex);
      if (picked === null) return [];
      const variant = sizes(fn(ex));
      return [{ ...labelOf(s), ...picked, variant, gain: minus(picked.base, variant) }];
    }),
  }));
}

/** A browser-like request without RQP fields (fixed values). */
const BASE_REQUEST = Object.freeze([
  [':method', 'GET'],
  [':path', '/catalog'],
  [':authority', 'shop.example.com'],
  ['accept', 'text/html'],
  ['accept-encoding', 'gzip, br'],
  ['user-agent', 'Mozilla/5.0 (Linux; Android 15) RivqenByteStudy/1.0'],
]);

function withFields(base, fields) {
  const names = new Set(fields.map(([n]) => n));
  return [...base.filter(([n]) => !names.has(n)), ...fields];
}

const status = (n) => [[':status', n]];
const BASE_FULL = [['content-type', 'text/html; charset=utf-8'], ['cache-control', 'no-cache'], ['content-encoding', 'gzip']];

/** Each header set on its own new connection (a revisit after a pause opens a new connection). */
async function hpackEach(sets) {
  const out = {};
  for (const [name, { request, response }] of Object.entries(sets)) {
    const [r] = await headerBlocks([{ request: headerObject(request), response: headerObject(response) }]);
    out[name] = {
      request_http1: http1Bytes(request.filter(([n]) => !n.startsWith(':'))),
      request_hpack: r.request,
      response_http1: http1Bytes(response.filter(([n]) => !n.startsWith(':'))),
      response_hpack: r.response,
    };
  }
  return out;
}

/** The fields of a lab response, without the per-response content-length. */
const labFields = (res) => res.fields.filter(([n]) => n !== 'content-length');

/** Header tables: raw (HTTP/1.1 form) and HPACK (HTTP/2) for RQP, legacy and the request options. */
async function headerTables(list, lab) {
  const out = [];
  for (const { s, ex } of list) {
    if (s.kind === 'grid' && (s.size !== 'small' || s.changed !== 'one')) continue; // the fields depend only on N
    const rqpRevisitFields = ex.requestRevisit;
    const weak = `W/"${ex.manifestA.page_revision}"`;
    const sets = {
      base: { request: BASE_REQUEST, response: [...status(200), ...BASE_FULL] },
      rqp_first: { request: withFields(BASE_REQUEST, ex.requestFirst), response: [...status(200), ...BASE_FULL, ['etag', weak], ['rq-version', '1']] },
      rqp_revisit_304: { request: withFields(BASE_REQUEST, rqpRevisitFields), response: [...status(304), ['etag', weak], ['cache-control', 'no-cache']] },
      rqp_revisit_delta: {
        request: withFields(BASE_REQUEST, rqpRevisitFields),
        response: [...status(200), ['content-type', DELTA_TYPE], ['rq-version', '1'], ['cache-control', 'no-store'], ['content-encoding', 'gzip']],
      },
    };
    for (const [id, fn] of Object.entries(REQUEST_OPTIONS)) {
      sets[`option_${id}`] = { request: withFields(BASE_REQUEST, fn(ex)), response: [...status(304), ['etag', weak], ['cache-control', 'no-cache']] };
    }
    const obs = lab?.find((o) => o.name === labName(s));
    if (obs !== undefined) {
      const req = (pairs) => withFields(BASE_REQUEST, pairs.filter(([n]) => n !== 'accept-encoding'));
      sets.legacy_first = { request: req(obs.request_cold), response: [...status(obs.cold.status), ...labFields(obs.cold)] };
      sets.legacy_revisit_304 = { request: req(obs.request_revisit), response: [...status(obs.unchanged.status), ...labFields(obs.unchanged)] };
      sets.legacy_revisit_data = { request: req(obs.request_revisit), response: [...status(obs.data_changed.status), ...labFields(obs.data_changed)] };
    }
    const measured = await hpackEach(sets);
    out.push({
      ...labelOf(s),
      ...(s.kind === 'grid' ? { scenario: `grid-${s.blocks}` } : {}),
      rq_have_value_bytes: Buffer.byteLength(rqpRevisitFields.find(([n]) => n === 'rq-have')[1]),
      fields: Object.fromEntries(rqpRevisitFields.map(([n, v]) => [n, Buffer.byteLength(v)])),
      sets: measured,
      q2_response_all_blocks: q2Response(s, ex),
    });
  }
  return out;
}

/** Option Q2 (no b): the server sends every block (H-7 fallback). The delta of that case. */
function q2Response(s, ex) {
  const fields = REQUEST_OPTIONS.Q2_have_without_b(ex);
  const r = exchangeWith(s, fields);
  return { kind: r.kind, reason: r.reason, ...sizes(r.body ?? Buffer.alloc(0)) };
}

function exchangeWith(s, fields) {
  return handle({ method: 'GET', target: s.target, headers: fields }, { status: 200, headers: [['content-type', 'text/html; charset=utf-8']] }, s.pageB, s.config);
}

/** Lab page name of a scenario: letters only (the lab app keeps only a-z). */
export function labName(s) {
  if (s.kind === 'demo') return 'demo';
  const n = { 1: 'one', 4: 'four', 16: 'sixteen', 64: 'sixtyfour' }[s.blocks];
  return `grid${n}${s.size === 'small' ? 's' : 'l'}${s.changed === 'one' ? 'o' : 'a'}`;
}

/** Lab scenarios: each study scenario with legacy markers, and the demo with the conversion of the 2026-10-19 run. */
export function labScenarios() {
  const list = scenarios().map((s) => ({ name: labName(s), pageA: toLegacy(s.pageA), pageB: toLegacy(s.pageB) }));
  list.push({ name: 'demohkeightytwo', pageA: toLegacy(demoPage(0), { titleMarkers: true }), pageB: toLegacy(demoPage(1), { titleMarkers: true }) });
  return list;
}

/** Like-for-like comparison of RQP and legacy for each scenario with lab data. */
function comparison(bodies, lab) {
  if (lab === null) return null;
  return bodies.flatMap((b) => {
    const name = b.scenario === 'demo' ? 'demo' : labName({ kind: 'grid', blocks: b.blocks, size: b.size, changed: b.changed_blocks });
    const o = lab.find((x) => x.name === name);
    if (o === undefined) return [];
    const pick = ({ raw, gzip, br }) => ({ raw, gzip, br });
    return [
      {
        scenario: b.scenario,
        cold: { rqp: b.full, legacy: pick(o.cold), rqp_minus_legacy: minus(b.full, pick(o.cold)) },
        changed: { rqp_kind: b.changed_kind, rqp: b.changed, legacy: pick(o.data_changed), rqp_minus_legacy: minus(b.changed, pick(o.data_changed)) },
        legacy_cold_body_is_page: o.cold.body_is_page,
      },
    ];
  });
}

/** Compute the results object. `lab` = the legacy lab observations or null. */
export async function compute({ lab = null, withHeaders = true } = {}) {
  const list = scenarios().map((s) => ({ s, ex: exchange(s.pageA, s.pageB, s.target, s.config) }));
  const bodies = list.map(({ s, ex }) => bodyTables(s, ex));
  const coldList = list.filter(({ s }) => s.kind === 'demo' || s.changed === 'one');
  return {
    study: 'WP-17 S6 T-01: RQP byte study',
    environment: environment(),
    method: {
      sizes: 'raw = bytes of the body; gzip and br = Node.js zlib defaults on the decoded body (the method of examples/server-demo/scripts/measure.js)',
      manifest_raw: 'exact and additive: each byte of the manifest element in one row',
      marginal: 'size(F or D) - size(F or D without the component); not additive',
      baseline: 'reference server step tools/rqp-protocol/src/server.mjs handle()',
      headers: 'request_http1/response_http1 = name ": " value CRLF of the non-pseudo fields; *_hpack = HPACK header block bytes that Node.js nghttp2 writes on a new HTTP/2 connection',
    },
    bodies,
    options: {
      manifest: optionTable(MANIFEST_OPTIONS, coldList, (ex) => ({ base: sizes(ex.cold.body) })),
      delta: optionTable(DELTA_OPTIONS, list, (ex) => {
        if (ex.candidate === null) return null;
        // The baseline of a delta option is the candidate envelope; the server sent it only when sent is true.
        const base = sizes(JSON.stringify(ex.candidate));
        return { base, sent: ex.changed.kind === 'delta', full_after_change: sizes(ex.fullB.body) };
      }),
    },
    headers: withHeaders ? await headerTables(list, lab) : null,
    legacy_lab: lab === null ? null : 'results/legacy-lab.json',
    comparison: comparison(bodies, lab),
  };
}
