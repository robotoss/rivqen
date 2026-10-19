// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// Component breakdown of F (the manifest in the full document) and of the
// delta envelope D.
//
// Two views:
// - raw: every byte of the manifest element (or of D) belongs to exactly one
//   row, so the rows add up to the total (exact, additive);
// - marginal: for gzip and Brotli a byte has no single cost. The marginal
//   cost of a component is size(document) - size(document without the
//   component), computed on the whole compressed body (F or D). Marginal
//   costs do not add up: they overlap and interact through the compressor.

import { segments, tally, tallySum } from './json-segments.mjs';
import { MANIFEST_END, MANIFEST_START, withManifest } from './rqp.mjs';
import { minus, sizes } from './sizes.mjs';
import { StudyError } from './errors.mjs';

const sumKeys = (keys, pred = () => true) => Object.entries(keys).reduce((n, [k, b]) => n + (pred(k) ? b : 0), 0);
const part = (values, label, name) => values[label]?.[name] ?? 0;
const all = (values, label) => Object.values(values[label] ?? {}).reduce((n, b) => n + b, 0);
const underPrefix = (values, prefix) =>
  Object.entries(values).reduce((n, [label, parts]) => n + (label === prefix || label.startsWith(`${prefix}.`) ? Object.values(parts).reduce((a, b) => a + b, 0) : 0), 0);

/** Raw rows of the manifest element. The rows add up to `total`. */
export function manifestRaw(manifest) {
  const t = tally(segments(manifest));
  if (tallySum(t) !== t.total) throw new StudyError('STUDY_TALLY', 'manifest tally does not add up');
  const v = t.values;
  const rows = {
    wrapper: Buffer.byteLength(MANIFEST_START + MANIFEST_END),
    json_structure: t.structure,
    member_names: sumKeys(t.keys),
    protocol: all(v, 'protocol'),
    origin: all(v, 'origin'),
    path: all(v, 'path'),
    template_revision_prefix_quotes: part(v, 'template_revision', 'prefix') + part(v, 'template_revision', 'quotes'),
    template_revision_digest: part(v, 'template_revision', 'digest'),
    page_revision_prefix_quotes: part(v, 'page_revision', 'prefix') + part(v, 'page_revision', 'quotes'),
    page_revision_digest: part(v, 'page_revision', 'digest'),
    block_ids: all(v, 'blocks[].id'),
    block_formats: all(v, 'blocks[].format'),
    block_digests: all(v, 'blocks[].sha256'),
    policy_limits_values: underPrefix(v, 'policy') + underPrefix(v, 'limits'),
  };
  const total = Object.values(rows).reduce((a, b) => a + b, 0);
  if (total !== rows.wrapper + t.total) throw new StudyError('STUDY_TALLY', 'manifest rows do not add up');
  return { total, rows, member_names_detail: t.keys };
}

/** Raw rows of the delta envelope. The rows add up to `total`. */
export function deltaRaw(envelope) {
  const t = tally(segments(envelope), new Set(['operations[].content']));
  if (tallySum(t) !== t.total) throw new StudyError('STUDY_TALLY', 'delta tally does not add up');
  const v = t.values;
  const rev = (label) => ({ prefix_quotes: part(v, label, 'prefix') + part(v, label, 'quotes'), digest: part(v, label, 'digest') });
  const rows = {
    json_structure: t.structure,
    member_names: sumKeys(t.keys),
    type: all(v, 'type'),
    protocol: all(v, 'protocol'),
    base_revision_prefix_quotes: rev('base_revision').prefix_quotes,
    base_revision_digest: rev('base_revision').digest,
    next_revision_prefix_quotes: rev('next_revision').prefix_quotes,
    next_revision_digest: rev('next_revision').digest,
    template_revision_prefix_quotes: rev('template_revision').prefix_quotes,
    template_revision_digest: rev('template_revision').digest,
    op_values: all(v, 'operations[].op'),
    block_id_values: all(v, 'operations[].block_id'),
    format_values: all(v, 'operations[].format'),
    content_quotes: part(v, 'operations[].content', 'quotes'),
    content_bytes: part(v, 'operations[].content', 'raw'),
    content_escapes: part(v, 'operations[].content', 'escape'),
    op_hashes: all(v, 'operations[].sha256'),
  };
  const total = Object.values(rows).reduce((a, b) => a + b, 0);
  if (total !== t.total) throw new StudyError('STUDY_TALLY', 'delta rows do not add up');
  return { total, rows, operations: envelope.operations.length };
}

/** A manifest with each member name replaced by a short unique name (one or two letters). */
export function shortKeys(value, map) {
  if (Array.isArray(value)) return value.map((x) => shortKeys(x, map));
  if (value === null || typeof value !== 'object') return value;
  const out = {};
  for (const [k, x] of Object.entries(value)) {
    const s = map[k];
    if (s === undefined) throw new StudyError('STUDY_KEYS', `no short name for ${k}`);
    out[s] = shortKeys(x, map);
  }
  return out;
}

export const MANIFEST_SHORT = Object.freeze({
  protocol: 'v', origin: 'o', path: 'p', template_revision: 't', page_revision: 'r', blocks: 'b', id: 'i', format: 'f', sha256: 'h',
  policy: 'P', mode: 'm', ttl_seconds: 'ttl', offline: 'off', limits: 'L', max_patch_bytes: 'mpb',
});
export const DELTA_SHORT = Object.freeze({
  type: 'y', protocol: 'v', base_revision: 'b', next_revision: 'n', template_revision: 't', operations: 'o',
  op: 'k', block_id: 'i', format: 'f', content: 'c', sha256: 'h',
});

const omit = (obj, ...names) => Object.fromEntries(Object.entries(obj).filter(([k]) => !names.includes(k)));

/** Marginal cost of each manifest component on the whole F (gzip and Brotli need context). */
export function manifestMarginal(ex) {
  const { pageA: page, insertAtA: at, manifestA: m } = ex;
  const f = (obj) => withManifest(page, at, JSON.stringify(obj));
  const base = sizes(ex.cold.body);
  const cost = (variant) => minus(base, sizes(variant));
  return {
    manifest_element: cost(page),
    wrapper: cost(Buffer.concat([page.subarray(0, at), Buffer.from(JSON.stringify(m)), page.subarray(at)])),
    member_names_vs_short: cost(f(shortKeys(m, MANIFEST_SHORT))),
    protocol: cost(f(omit(m, 'protocol'))),
    origin_path: cost(f(omit(m, 'origin', 'path'))),
    template_revision: cost(f(omit(m, 'template_revision'))),
    page_revision: cost(f(omit(m, 'page_revision'))),
    block_digests: cost(f({ ...m, blocks: m.blocks.map((b) => omit(b, 'sha256')) })),
    block_ids_formats: cost(f({ ...m, blocks: m.blocks.map((b) => omit(b, 'id', 'format')) })),
    blocks_member: cost(f(omit(m, 'blocks'))),
    policy_limits: cost(f(omit(m, 'policy', 'limits'))),
  };
}

/** Marginal cost of each delta component on D alone (D is the whole body). */
export function deltaMarginal(envelope) {
  const base = sizes(JSON.stringify(envelope));
  const cost = (variant) => minus(base, sizes(JSON.stringify(variant)));
  const ops = (fn) => ({ ...envelope, operations: envelope.operations.map(fn) });
  return {
    member_names_vs_short: cost(shortKeys(envelope, DELTA_SHORT)),
    type_protocol: cost(omit(envelope, 'type', 'protocol')),
    base_revision: cost(omit(envelope, 'base_revision')),
    next_revision: cost(omit(envelope, 'next_revision')),
    template_revision: cost(omit(envelope, 'template_revision')),
    op_and_format: cost(ops((o) => omit(o, 'op', 'format'))),
    op_hashes: cost(ops((o) => omit(o, 'sha256'))),
  };
}
