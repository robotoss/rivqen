// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors
//
// Server side of rqp/1 for the demo: the stateless response selection of
// docs/engineering/protocol/negotiation.md section 4. The server keeps no state per
// client or per page: the request holds the client's revisions and block digest prefixes.
// It is a demo, not the production SDK: clarity over speed.
//
// handle(request, appResponse, pageBytes, config) is the interface of the conformance
// harness (tools/rqp-protocol/check.mjs, --server).

import { createHash } from 'node:crypto';
import {
  DELTA_TYPE,
  acceptsDelta,
  capabilities,
  deltaBase,
  fullContentType,
  fieldValue,
  hasField,
  mediaType,
  noneMatchFails,
  normalizeHeaders,
  requestVersion,
  rqHave,
  without,
  withoutRqFields,
} from './fields.js';
import { analyzePage, documentHeadStart, passesPatchContentRule } from './markup.js';

export { DELTA_TYPE };
export const MANIFEST_TYPE = 'application/rivqen-manifest+json';
export const MAX_PATCH_BYTES = 262144; // server limit maxPatchBytes (resource limits, default)

const sha256b64u = (data) => createHash('sha256').update(data).digest('base64url');

/** Text S of the policy line (manifest-patch section 1.3, H-55). '' when the manifest has neither member. */
export function policyLineText({ policy, limits }) {
  const items = [];
  if (policy !== undefined) {
    items.push('policy');
    if (policy.mode !== undefined) items.push(`mode=${policy.mode}`);
    if (policy.ttl_seconds !== undefined) items.push(`ttl_seconds=${policy.ttl_seconds}`);
    if (policy.offline !== undefined) items.push(`offline=${policy.offline}`);
  }
  if (limits !== undefined) {
    items.push('limits');
    if (limits.max_patch_bytes !== undefined) items.push(`max_patch_bytes=${limits.max_patch_bytes}`);
  }
  return items.join(';');
}

/**
 * Page revision of manifest-patch section 1.3, steps 4 and 5, with the policy line.
 * The markup analysis gives the revision without the line, so the demo computes it again.
 */
export function pageRevision(templateRevision, blocks, members = {}) {
  let preimage = `rqp-r1\n${templateRevision}\n`;
  const text = policyLineText(members);
  if (text !== '') preimage += `PL\t${text}\n`;
  for (const b of blocks) preimage += `${b.id}\t${b.format}\t${b.sha256}\n`;
  return `r1.${sha256b64u(preimage)}`;
}

const POLICY_DEFAULTS = { mode: 'private', ttl_seconds: 0, offline: false };

/**
 * The manifest members policy and limits of a configuration object, without default members
 * (H-103, DL-057, DL-058): a member with the default value is dropped, an object without members is
 * dropped, and a max_patch_bytes hint that is not smaller than the server limit is dropped.
 */
export function membersOf(config, serverLimit = MAX_PATCH_BYTES) {
  const members = {};
  if (config.policy !== undefined) {
    const policy = {};
    for (const [name, value] of Object.entries(config.policy)) {
      if (value !== POLICY_DEFAULTS[name]) policy[name] = value;
    }
    if (Object.keys(policy).length > 0) members.policy = policy;
  }
  const hint = config.limits?.max_patch_bytes;
  if (hint !== undefined && hint < serverLimit) members.limits = { max_patch_bytes: hint };
  return members;
}

/** The page manifest (manifest-patch section 1.1, H-103): no block list, no template revision. */
export function buildManifest(analysis, target, members) {
  const url = new URL(target);
  const blocks = analysis.blocks.map(({ id, format, sha256 }) => ({ id, format, sha256 }));
  return {
    protocol: 'rqp/1',
    origin: url.origin,
    path: url.pathname,
    page_revision: pageRevision(analysis.template_revision, blocks, members),
    ...members,
  };
}

/**
 * F: the page with the manifest element at the insertion point: immediately after the head
 * start tag of the document, or immediately before the start tag that implies the head
 * (manifest-patch section 1.7, H-72, H-79a). Returns null when the manifest text has a "<".
 */
export function insertManifest(page, insertAt, manifest) {
  const text = JSON.stringify(manifest);
  if (text.includes('<')) return null;
  return Buffer.concat([
    page.subarray(0, insertAt),
    Buffer.from(`<script type="${MANIFEST_TYPE}">${text}</script>`, 'utf8'),
    page.subarray(insertAt),
  ]);
}

/** The first 8 bytes of the raw SHA-256 digest of a block (H-44), from its base64url hash. */
const digestPrefix = (sha256) => Buffer.from(sha256, 'base64url').subarray(0, 8);

/**
 * Step 6: the indices of the blocks in the change set C. `b` is the decoded Rq-Have
 * member b, or null for the all-blocks fallback (H-7).
 */
export function changeSet(blocks, b) {
  const changed = [];
  blocks.forEach((block, i) => {
    if (b === null || !digestPrefix(block.sha256).equals(b.subarray(8 * i, 8 * i + 8))) changed.push(i);
  });
  return changed;
}

/** Step 7: the delta envelope D (manifest-patch section 2, H-104). One operation per changed block. */
export function buildDelta(page, analysis, nextRevision, indices) {
  return {
    next_revision: nextRevision,
    operations: indices.map((i) => {
      const b = analysis.blocks[i];
      return { block_id: b.id, content: page.toString('utf8', b.start, b.end) };
    }),
  };
}

const KEEP_ON_304 = new Set(['etag', 'cache-control', 'expires', 'vary', 'date', 'content-location']);

/**
 * Negotiation section 4. Returns { status, headers, body, mode, reason }:
 * headers is a list of [name, value] lines, body a Buffer or null, mode one of
 * 'passthrough', 'full', '304' or 'delta', reason a short log word.
 *
 * An internal error never reaches the client: the application response is sent
 * unchanged with mode 'passthrough' and reason 'SERVER_PASSTHROUGH' (errors.md).
 *
 * @param {{ method: string, target: string, headers: Array<[string, string]> }} request
 *   target is the absolute URL of the request (the server takes the public origin from its configuration)
 * @param {{ status: number, headers: Array<[string, string]> }} appResponse
 * @param {Uint8Array} pageBytes the page bytes P
 * @param {{ policy?: object, limits?: object, maxPatchBytes?: number, contentCoding?: boolean }} [config]
 *   policy and limits are the manifest members. contentCoding is true when the caller applies a
 *   content coding to the response: the ETag is then weak (H-52).
 */
export function handle(request, appResponse, pageBytes, config = {}) {
  const page = Buffer.from(pageBytes.buffer, pageBytes.byteOffset, pageBytes.byteLength);
  // DL-030: the RQP step owns every Rq-* field. No outcome carries an Rq-* field of A.
  const appHeaders = withoutRqFields(normalizeHeaders(appResponse.headers));
  const passthrough = (reason) => ({ status: appResponse.status, headers: appHeaders, body: page, mode: 'passthrough', reason });
  try {
    return select(request, appResponse.status, appHeaders, page, config) ?? passthrough('invalid-page');
  } catch (error) {
    return { ...passthrough('SERVER_PASSTHROUGH'), error };
  }
}

/** Steps 1 to 9. Returns null when the page is invalid for RQP (step 2). */
function select(request, status, appHeaders, page, config) {
  // Step 1: scope.
  const inScope =
    (request.method === 'GET' || request.method === 'HEAD') && status === 200 && mediaType(fieldValue(appHeaders, 'content-type')) === 'text/html';
  if (!inScope) return { status, headers: appHeaders, body: page, mode: 'passthrough', reason: 'out-of-scope' };

  // Step 2: validity. H-62: the page is UTF-8; a charset parameter of another encoding makes it invalid.
  const typed = fullContentType(appHeaders);
  if (typed === null) return null;
  const analysis = analyzePage(page);
  if (!analysis.valid) return null;
  const insertAt = documentHeadStart(page);
  if (insertAt < 0) return null;

  // Step 3: the full document F, the same for every request.
  const reqHeaders = normalizeHeaders(request.headers);
  const serverLimit = config.maxPatchBytes ?? MAX_PATCH_BYTES;
  const members = membersOf(config, serverLimit);
  const manifest = buildManifest(analysis, request.target, members);
  const full = insertManifest(page, insertAt, manifest);
  if (full === null) return null;
  const etag = `${config.contentCoding ? 'W/' : ''}"${manifest.page_revision}"`;
  const fullHeaders = [
    // DL-032: the ETag r1. is the only validator of F, so Last-Modified of A goes.
    ...without(typed, 'etag', 'last-modified', 'content-length'),
    ['etag', etag],
    ['rq-version', '1'],
    ['content-length', String(full.length)],
  ];
  const head = request.method === 'HEAD';
  const sendFull = (reason) => ({ status: 200, headers: fullHeaders, body: head ? null : full, mode: 'full', reason });

  // Step 4: conditions. Step 9: not modified.
  if (noneMatchFails(reqHeaders, etag)) {
    return { status: 304, headers: fullHeaders.filter(([n]) => KEEP_ON_304.has(n)), body: null, mode: '304', reason: 'not-modified' };
  }

  // Step 5: delta eligibility.
  const base = deltaBase(reqHeaders);
  const have = rqHave(reqHeaders);
  const eligible =
    request.method === 'GET' &&
    !hasField(reqHeaders, 'range') &&
    requestVersion(reqHeaders) === 1 &&
    capabilities(reqHeaders).has('block-patch') &&
    acceptsDelta(reqHeaders) &&
    base !== null &&
    have !== null &&
    have.t === analysis.template_revision;
  if (!eligible) return sendFull('not-eligible');

  // Step 6: change set. A b with another length than 8 N is absent (section 3.4, step 6).
  const b = have.b !== null && have.b.length === 8 * analysis.blocks.length ? have.b : null;
  const changed = changeSet(analysis.blocks, b);
  if (changed.length === 0) return sendFull('empty-change-set');

  // Step 7: delta. H-56, H-78: F when a changed html block fails the patch content rule.
  const failsRule = changed.some((i) => {
    const block = analysis.blocks[i];
    return block.format === 'html' && !passesPatchContentRule(page.subarray(block.start, block.end));
  });
  if (failsRule) return sendFull('patch-content-rule');
  const delta = Buffer.from(JSON.stringify(buildDelta(page, analysis, manifest.page_revision, changed)), 'utf8');
  const limit = Math.min(serverLimit, members.limits?.max_patch_bytes ?? Infinity);
  if (delta.length > limit) return sendFull('delta-too-large');
  if (delta.length >= full.length) return sendFull('delta-not-smaller');
  return {
    status: 200,
    headers: [
      ...without(appHeaders, 'etag', 'last-modified', 'expires', 'content-type', 'cache-control', 'content-length'),
      ['content-type', DELTA_TYPE],
      ['rq-version', '1'],
      ['cache-control', 'no-store'],
      ['content-length', String(delta.length)],
    ],
    body: delta,
    mode: 'delta',
    reason: 'delta',
  };
}
