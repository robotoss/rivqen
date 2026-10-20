// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// Candidate wire changes of the byte study. Each option takes the baseline
// exchange of rqp.mjs and gives the bytes that the changed wire format would
// send. These are measurement models, not protocol code: no option is
// normative, and the security cost of each one is in the report.
//
// Manifest options change F (cold visit). Delta options change D. Request
// options change the request fields of a revisit.

import { pageRevision, sha256b64u } from '../baseline/common.mjs';
import { havePrefixes } from '../baseline/client.mjs';
import { DELTA_SHORT, MANIFEST_SHORT, shortKeys } from './breakdown.mjs';
import { encode as cbor } from './cbor.mjs';
import { MANIFEST_END, withManifest } from './rqp.mjs';
import { StudyError } from './errors.mjs';

const omit = (obj, ...names) => Object.fromEntries(Object.entries(obj).filter(([k]) => !names.includes(k)));

/** Policy and limits values that give the default client behavior (manifest-patch 1.4, 1.5). */
export function withoutDefaultMembers(members, maxPatchBytes = 262144) {
  const out = {};
  if (members.policy !== undefined) {
    const p = {};
    if (members.policy.mode !== undefined && members.policy.mode !== 'private') p.mode = members.policy.mode;
    if (members.policy.ttl_seconds !== undefined && members.policy.ttl_seconds !== 0) p.ttl_seconds = members.policy.ttl_seconds;
    if (members.policy.offline !== undefined && members.policy.offline !== false) p.offline = members.policy.offline;
    if (Object.keys(p).length > 0) out.policy = p;
  }
  if (members.limits?.max_patch_bytes !== undefined && members.limits.max_patch_bytes !== maxPatchBytes) {
    out.limits = { max_patch_bytes: members.limits.max_patch_bytes };
  }
  return out;
}

/** A manifest without default members, with the page revision that its policy line gives (H-55). */
function manifestWithoutDefaults(m) {
  const members = withoutDefaultMembers({ policy: m.policy, limits: m.limits });
  const base = omit(m, 'policy', 'limits');
  return { ...base, page_revision: pageRevision(m.template_revision, m.blocks, members), ...members };
}

/** The minimal manifest: no blocks and no template revision; the page revision binds them (H-48 analysis). */
const minimal = (m) => omit(m, 'template_revision', 'blocks');

/**
 * Stand-in for a page revision whose preimage also has the origin and the
 * path (option M4). Same length as r1 (46 characters). Not a spec: the exact
 * preimage is a T-02 question.
 */
const boundRevision = (m) => `r2.${sha256b64u(`rqp-r2\n${m.origin}\n${m.path}\n${m.page_revision}\n`)}`;

const truncate = (s, keep) => s.slice(0, keep);
const t128 = (rev) => truncate(rev, 3 + 22); // prefix + 22 base64url characters = 132 bits >= 128 bits

/** Manifest options: id -> function(exchange) -> F bytes. */
export const MANIFEST_OPTIONS = Object.freeze({
  M1_omit_default_members: (ex) => manifestF(ex, manifestWithoutDefaults(ex.manifestA)),
  M2_no_block_digests: (ex) => manifestF(ex, { ...ex.manifestA, blocks: ex.manifestA.blocks.map((b) => omit(b, 'sha256')) }),
  M3_minimal_manifest: (ex) => manifestF(ex, minimal(ex.manifestA)),
  M4_url_in_revision: (ex) => manifestF(ex, { ...omit(minimal(ex.manifestA), 'origin', 'path'), page_revision: boundRevision(ex.manifestA) }),
  M5_short_member_names: (ex) => manifestF(ex, shortKeys(ex.manifestA, MANIFEST_SHORT)),
  M6_no_protocol_member: (ex) => manifestF(ex, omit(ex.manifestA, 'protocol')),
  M7_no_revision_prefix: (ex) =>
    manifestF(ex, { ...ex.manifestA, template_revision: ex.manifestA.template_revision.slice(3), page_revision: ex.manifestA.page_revision.slice(3) }),
  M8_truncate_128: (ex) =>
    manifestF(ex, {
      ...ex.manifestA,
      template_revision: t128(ex.manifestA.template_revision),
      page_revision: t128(ex.manifestA.page_revision),
      blocks: ex.manifestA.blocks.map((b) => ({ ...b, sha256: truncate(b.sha256, 22) })),
    }),
  M9_column_blocks: (ex) => {
    const m = ex.manifestA;
    return manifestF(ex, {
      ...omit(m, 'blocks', 'policy', 'limits'),
      block_ids: m.blocks.map((b) => b.id),
      block_formats: m.blocks.map((b) => b.format),
      block_sha256: m.blocks.map((b) => b.sha256),
      ...(m.policy === undefined ? {} : { policy: m.policy }),
      ...(m.limits === undefined ? {} : { limits: m.limits }),
    });
  },
  M10_short_manifest_type: (ex) => withManifest(ex.pageA, ex.insertAtA, JSON.stringify(ex.manifestA), '<script type="application/rqm+json">', MANIFEST_END),
  MC1_defaults_and_minimal: (ex) => manifestF(ex, minimal(manifestWithoutDefaults(ex.manifestA))),
  MC2_compact: (ex) => {
    const m = manifestWithoutDefaults(ex.manifestA);
    return manifestF(ex, shortKeys({ ...omit(minimal(m), 'origin', 'path'), page_revision: boundRevision(m) }, MANIFEST_SHORT));
  },
});

function manifestF(ex, manifest) {
  const text = JSON.stringify(manifest);
  if (text.includes('<')) throw new StudyError('STUDY_OPTION', 'manifest text has "<"');
  return withManifest(ex.pageA, ex.insertAtA, text);
}

const json = (v) => Buffer.from(JSON.stringify(v), 'utf8');
const ops = (d, fn) => ({ ...d, operations: d.operations.map(fn) });
const indexOf = (ex, id) => ex.analysisB.blocks.findIndex((b) => b.id === id);
const minDelta = (d) => ({ next_revision: d.next_revision, operations: d.operations.map((o) => ({ block_id: o.block_id, content: o.content })) });

/** Delta options: id -> function(exchange) -> D bytes, from the candidate envelope of step 7. */
export const DELTA_OPTIONS = Object.freeze({
  D1_no_op_hash: (ex) => json(ops(ex.candidate, (o) => omit(o, 'sha256'))),
  D2_no_base_revision: (ex) => json(omit(ex.candidate, 'base_revision')),
  D3_no_template_revision: (ex) => json(omit(ex.candidate, 'template_revision')),
  D4_no_type_protocol: (ex) => json(omit(ex.candidate, 'type', 'protocol')),
  D5_no_op_format: (ex) => json(ops(ex.candidate, (o) => omit(o, 'op', 'format'))),
  D6_short_member_names: (ex) => json(shortKeys(ex.candidate, DELTA_SHORT)),
  D7_block_index: (ex) => json(ops(ex.candidate, (o) => ({ ...omit(o, 'block_id'), block: indexOf(ex, o.block_id) }))),
  D8_truncate_128: (ex) =>
    json({
      ...ex.candidate,
      base_revision: t128(ex.candidate.base_revision),
      next_revision: t128(ex.candidate.next_revision),
      template_revision: t128(ex.candidate.template_revision),
      operations: ex.candidate.operations.map((o) => ({ ...o, sha256: truncate(o.sha256, 22) })),
    }),
  DC1_minimal_object: (ex) => json(minDelta(ex.candidate)),
  DC2_minimal_short_names: (ex) => json(shortKeys(minDelta(ex.candidate), DELTA_SHORT)),
  DC3_minimal_array: (ex) => json([ex.candidate.next_revision, ex.candidate.operations.map((o) => [o.block_id, o.content])]),
  DC4_content_outside_json: (ex) => {
    const raw = ex.candidate.operations.map((o) => Buffer.from(o.content, 'utf8'));
    const head = JSON.stringify([ex.candidate.next_revision, ex.candidate.operations.map((o, i) => [o.block_id, raw[i].length])]);
    return Buffer.concat([Buffer.from(`${head}\n`, 'utf8'), ...raw]);
  },
  DC5_minimal_cbor: (ex) => cbor(minDelta(ex.candidate)),
  DC6_minimal_cbor_binary_revision: (ex) =>
    cbor([Buffer.from(ex.candidate.next_revision.slice(3), 'base64url'), ex.candidate.operations.map((o) => [o.block_id, o.content])]),
});

/** Request options for a revisit: id -> function(exchange) -> field lines. */
export const REQUEST_OPTIONS = Object.freeze({
  Q1_have_32bit_prefixes: (ex) =>
    replaceField(ex.requestRevisit, 'rq-have', `t=${ex.manifestA.template_revision}, b=:${prefixes(ex.manifestA.blocks, 4)}:`),
  Q2_have_without_b: (ex) => replaceField(ex.requestRevisit, 'rq-have', `t=${ex.manifestA.template_revision}`),
  Q3_have_t_64bit: (ex) =>
    replaceField(ex.requestRevisit, 'rq-have', `t=${truncate(ex.manifestA.template_revision, 3 + 11)}, b=:${havePrefixes(ex.manifestA.blocks)}:`),
});

function prefixes(blocks, n) {
  return Buffer.concat(blocks.map((b) => Buffer.from(b.sha256, 'base64url').subarray(0, n))).toString('base64');
}

function replaceField(fields, name, value) {
  if (!fields.some(([n]) => n === name)) throw new StudyError('STUDY_OPTION', `no field ${name}`);
  return fields.map(([n, v]) => [n, n === name ? value : v]);
}
