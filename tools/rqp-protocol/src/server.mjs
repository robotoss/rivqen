// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// Reference server step: the response selection of negotiation.md section 4,
// written as one readable function per step. It is the source of the expected
// responses of the server fixtures (FX-RQ-P*), not a server SDK.
//
// Inputs that the algorithm text does not cover in rqp/1 fixtures (content
// codings, If-Match, If-Unmodified-Since, If-Range, If-Modified-Since without
// If-None-Match) throw UnsupportedInput, so no fixture can depend on them by
// accident.

import {
  DELTA_TYPE,
  LIMITS,
  MANIFEST_END,
  MANIFEST_START,
  NONCE_BYTES,
  POLICY_DEFAULTS,
  UnsupportedInput,
  canonicalUrl,
  deltaBinding,
  digestPrefix,
  membersOf,
  nonceText,
  pageRevision,
} from './common.mjs';
import {
  Charset,
  acceptsMediaType,
  combined,
  contentTypeCharset,
  has,
  mediaType,
  normalizeHeaders,
  parseEntityTags,
  without,
} from './http.mjs';
import { analyze, documentHeadStart, patchContentFailure } from './markup.mjs';
import { parseDictionary, parseItem, parseList } from './sf.mjs';

const BASE_TAG = /^"(r1\.[A-Za-z0-9_-]{43})"$/;

/** Byte length of a field value. */
function byteLength(value) {
  return Buffer.byteLength(value, 'utf8');
}

/** Section 3.2: the request version is supported only when it is the Integer 1. */
export function requestVersion(headers) {
  const v = combined(headers, 'rq-version');
  if (v === null) return null;
  const parsed = parseItem(v);
  return parsed !== null && parsed.item.type === 'integer' ? parsed.item.value : null;
}

/** Section 3.3: the capability set, or an empty set when the field is absent or has another shape. */
export function capabilities(headers) {
  const v = combined(headers, 'rq-capabilities');
  if (v === null || byteLength(v) > LIMITS.maxRqCapabilitiesBytes) return new Set();
  const list = parseList(v);
  if (list === null || list.length !== 1 || !list[0].inner) return new Set();
  if (!list[0].items.every(({ item }) => item.type === 'token')) return new Set();
  return new Set(list[0].items.map(({ item }) => item.value));
}

/**
 * Section 3.4: { t, b } of Rq-Have, where t is the template revision token and
 * b the decoded bytes or null (absent). Returns null when Rq-Have is absent.
 * The check of b against the block count is done by the caller (steps 5 and 6
 * of section 3.4 need the current page). A key that appears two times has the
 * later value (RFC 9651 section 4.2.2). Every other member is ignored (step 7,
 * RQP-06), also a member n: since H-109 the client nonce is the field
 * Rq-Nonce (rqNonce), and n in Rq-Have is an unknown member.
 */
export function rqHave(headers) {
  const v = combined(headers, 'rq-have');
  if (v === null || byteLength(v) > LIMITS.maxRqHaveBytes) return null;
  const dict = parseDictionary(v);
  if (dict === null) return null;
  const t = dict.get('t');
  if (t === undefined || t.inner || t.item.type !== 'token') return null;
  const b = dict.get('b');
  const bytes = b !== undefined && !b.inner && b.item.type === 'bytes' ? b.item.value : null;
  return { t: t.item.value, b: bytes };
}

/**
 * Section 3.9 (H-109): the client nonce of the request, the 16 bytes of the
 * field Rq-Nonce, or null (no nonce). The field must have exactly one field
 * line (two lines, also with equal values, are no nonce: fail closed), its
 * value must parse as an Item (RFC 9651 section 4.2) whose bare item is a Byte
 * Sequence of exactly 16 bytes; parameters are ignored. A Byte Sequence
 * without padding or with non-zero pad bits is accepted (section 4.2.7), and
 * the caller uses only the decoded bytes. The parse is linear in the length
 * of the field line, which the HTTP stack bounds, as for Rq-Version.
 */
export function rqNonce(headers) {
  const lines = headers.filter(([n]) => n === 'rq-nonce');
  if (lines.length !== 1) return null;
  const parsed = parseItem(combined(lines, 'rq-nonce'));
  if (parsed === null || parsed.item.type !== 'bytes') return null;
  return parsed.item.value.length === NONCE_BYTES ? parsed.item.value : null;
}

/** Section 3.5: the base revision B, or null. */
export function deltaBase(headers) {
  const v = combined(headers, 'if-none-match');
  if (v === null) return null;
  const parsed = parseEntityTags(v);
  if (parsed === null || parsed.star || parsed.tags.length !== 1) return null;
  const m = BASE_TAG.exec(parsed.tags[0].opaque);
  return m === null ? null : m[1];
}

/** Step 4: true when the If-None-Match condition is false (a 304 for GET and HEAD). */
export function notModified(headers, etag) {
  const v = combined(headers, 'if-none-match');
  if (v === null) return false;
  const parsed = parseEntityTags(v);
  if (parsed === null) return false;
  if (parsed.star) return true;
  const opaque = etag.startsWith('W/') ? etag.slice(2) : etag;
  return parsed.tags.some((tag) => tag.opaque === opaque); // weak comparison
}

/** Step 1: RQP applies only to GET and HEAD, status 200 and text/html. */
function inScope(request, app, appHeaders) {
  if (request.method !== 'GET' && request.method !== 'HEAD') return false;
  if (app.status !== 200) return false;
  return mediaType(combined(appHeaders, 'content-type')) === 'text/html';
}

/**
 * The members `policy` and `limits` that the server writes in the manifest
 * (manifest-patch sections 1.2, 1.4 and 1.5; H-103): the server configuration
 * without the members whose value is the default (MUST), without a hint
 * `max_patch_bytes` that is not smaller than the server limit (SHOULD: such a
 * hint bounds nothing, section 2.5 item 5 uses the smaller value), and without
 * an object that has no member left. The members keep the order of the
 * configuration; the policy line has a fixed order anyway (section 1.3).
 */
export function writtenMembers(config, serverLimit = LIMITS.maxPatchBytes) {
  const m = membersOf(config);
  const out = {};
  if (m.policy !== undefined) {
    const policy = Object.fromEntries(
      Object.entries(m.policy).filter(([k, v]) => !(Object.hasOwn(POLICY_DEFAULTS, k) && POLICY_DEFAULTS[k] === v)),
    );
    if (Object.keys(policy).length > 0) out.policy = policy;
  }
  if (m.limits !== undefined) {
    const limits = Object.fromEntries(Object.entries(m.limits).filter(([k, v]) => !(k === 'max_patch_bytes' && v >= serverLimit)));
    if (Object.keys(limits).length > 0) out.limits = limits;
  }
  return out;
}

/**
 * The page manifest of an analyzed page (manifest-patch section 1.2; H-103):
 * protocol, origin, path, page_revision and the members of `members` as they
 * are. No block list and no template revision: the client computes them with
 * its markup analysis and checks them through the page revision (section
 * 1.8). The page revision includes the policy line of `members` (section 1.3,
 * H-55); the markup analysis gives the page revision without a policy line,
 * so it is computed again here. The server step passes writtenMembers(config).
 */
export function manifestOf(analysis, target, members = {}) {
  const url = new URL(target);
  return {
    protocol: 'rqp/1',
    origin: url.origin,
    path: url.pathname,
    page_revision: pageRevision(analysis.template_revision, analysis.blocks, members),
    ...membersOf(members),
  };
}

/**
 * Step 3: F = P with the manifest element at the insertion point (H-72,
 * H-79a): immediately after the head start tag of the document, or before the
 * start tag that implies the head.
 */
export function fullDocument(page, insertAt, manifest) {
  const text = JSON.stringify(manifest);
  if (text.includes('<')) return null; // manifest-patch section 1.7, server item 3
  return Buffer.concat([
    page.subarray(0, insertAt),
    Buffer.from(MANIFEST_START + text + MANIFEST_END, 'utf8'),
    page.subarray(insertAt),
  ]);
}

/** Step 6: the indices of the blocks in the change set C. */
export function changeSet(blocks, b) {
  if (b === null) return blocks.map((_, i) => i);
  const out = [];
  blocks.forEach((block, i) => {
    if (!digestPrefix(block.sha256).equals(b.subarray(8 * i, 8 * i + 8))) out.push(i);
  });
  return out;
}

/**
 * Step 7: the delta envelope (manifest-patch section 2; H-104, H-107): the
 * binding value and one operation (block id and content) for each block in
 * the change set, in document order. No base or template revision, no type,
 * no format and no hash: the client computes the block hashes and the next
 * page revision, and checks the binding. `binding` is deltaBinding(U, B, N, R'):
 * U the canonical URL of this request, B the base revision of If-None-Match
 * (section 3.5), N the client nonce of this request (Rq-Nonce, H-108, H-109),
 * R' the page revision of the current page.
 */
export function deltaEnvelope(page, analysis, binding, indices) {
  return {
    binding,
    operations: indices.map((i) => {
      const b = analysis.blocks[i];
      return { block_id: b.id, content: page.toString('utf8', b.start, b.end) };
    }),
  };
}

/**
 * DL-030: the RQP server step owns every Rq-* field. The fields of the
 * application response A without its Rq-* fields, in every outcome.
 */
export function withoutRqFields(headers) {
  return headers.filter(([name]) => !name.startsWith('rq-'));
}

/** H-66: the exact Content-Type field value of every F. */
export const FULL_CONTENT_TYPE = 'text/html; charset=utf-8';

/**
 * Steps 2 and 3 (H-62 item 3, H-66): the header fields of A with the
 * Content-Type of F, or null when the charset rule makes the page invalid for
 * RQP (a charset that is not a UTF-8 label, or more than one field line or
 * media type). The charset rule only decides the passthrough: F always has
 * exactly one field line "Content-Type: text/html; charset=utf-8", whatever
 * the Content-Type text of A was (no charset, a quoted or upper-case UTF-8
 * label, other parameters). A has at least one Content-Type line here (step 1
 * found the media type text/html). The one-line guarantee comes from
 * contentTypeCharset, which returns OTHER for more than one line, so such a
 * page is passed through; step 1 alone does not give it (it reads the combined
 * value). The one line keeps its position.
 */
export function fullContentType(appHeaders) {
  if (contentTypeCharset(appHeaders) === Charset.OTHER) return null;
  return appHeaders.map(([n, v]) => [n, n === 'content-type' ? FULL_CONTENT_TYPE : v]);
}

function unsupported(headers) {
  for (const name of ['if-match', 'if-unmodified-since', 'if-range', 'content-encoding']) {
    if (has(headers, name)) throw new UnsupportedInput(`the reference server does not model ${name}`);
  }
}

/**
 * Negotiation section 4. Returns { status, headers, body, kind, reason } where
 * body is a Buffer or null and kind is "app", "full", "delta" or "none".
 *
 * @param {{ method: string, target: string, headers: Array<[string, string]> }} request
 * @param {{ status: number, headers: Array<[string, string]> }} app the application response A
 * @param {Uint8Array} pageBytes the page bytes P
 * @param {{ policy?: object, limits?: object, maxPatchBytes?: number }} [config] server
 *   configuration: the manifest members policy and limits, and the server limit
 */
export function handle(request, app, pageBytes, config = {}) {
  const serverLimit = config.maxPatchBytes ?? LIMITS.maxPatchBytes;
  // H-103: the manifest has no member with its default value.
  const members = writtenMembers(config, serverLimit);
  // Manifest-patch section 2.5, item 5: also the hint max_patch_bytes of the server.
  const maxPatchBytes = Math.min(serverLimit, members.limits?.max_patch_bytes ?? Infinity);
  const reqHeaders = normalizeHeaders(request.headers);
  // DL-030: no outcome (passthrough, F, D, 304) carries an Rq-* field of A.
  const appHeaders = withoutRqFields(normalizeHeaders(app.headers));
  const page = Buffer.from(pageBytes.buffer, pageBytes.byteOffset, pageBytes.byteLength);
  unsupported(appHeaders);
  const passthrough = (reason) => ({ status: app.status, headers: appHeaders, body: page, kind: 'app', reason });

  // Step 1.
  if (!inScope(request, app, appHeaders)) return passthrough('out-of-scope');
  unsupported(reqHeaders);
  if (has(reqHeaders, 'if-modified-since') && !has(reqHeaders, 'if-none-match')) {
    throw new UnsupportedInput('the reference server does not model If-Modified-Since alone');
  }

  // Step 2. H-62 (3): UTF-8 only. A charset that is not a UTF-8 label makes the page invalid for RQP.
  const typed = fullContentType(appHeaders);
  if (typed === null) return passthrough('charset');
  const analysis = analyze(page);
  if (!analysis.valid) return passthrough('invalid-page');
  const insertAt = documentHeadStart(page);
  if (insertAt < 0) return passthrough('no-head-start');

  // Step 3.
  const manifest = manifestOf(analysis, request.target, members);
  const full = fullDocument(page, insertAt, manifest);
  if (full === null) return passthrough('server-passthrough');
  const etag = `"${manifest.page_revision}"`;
  // DL-032: the ETag r1. is the only validator of F, so A's Last-Modified goes.
  // H-66: F has exactly "Content-Type: text/html; charset=utf-8" (fullContentType).
  const fullHeaders = [
    ...without(typed, 'etag', 'last-modified', 'content-length'),
    ['etag', etag],
    ['rq-version', '1'],
    ['content-length', String(full.length)],
  ];
  const sendFull = (reason) => ({
    status: 200,
    headers: fullHeaders,
    body: request.method === 'HEAD' ? null : full,
    kind: request.method === 'HEAD' ? 'none' : 'full',
    reason,
  });

  // Step 4 and step 9.
  if (notModified(reqHeaders, etag)) {
    const keep = new Set(['etag', 'cache-control', 'expires', 'vary', 'date', 'content-location']);
    return { status: 304, headers: fullHeaders.filter(([n]) => keep.has(n)), body: null, kind: 'none', reason: 'not-modified' };
  }

  // Step 5.
  const base = deltaBase(reqHeaders);
  const have = rqHave(reqHeaders);
  const eligible =
    request.method === 'GET' &&
    !has(reqHeaders, 'range') &&
    requestVersion(reqHeaders) === 1 &&
    capabilities(reqHeaders).has('block-patch') &&
    acceptsMediaType(combined(reqHeaders, 'accept'), DELTA_TYPE) &&
    base !== null &&
    have !== null &&
    have.t === analysis.template_revision;
  if (!eligible) return sendFull('not-eligible');
  // H-108, H-109: a delta only for a request with exactly one valid Rq-Nonce
  // (step 5.7); else F. A member n of Rq-Have is ignored (an unknown member).
  const nonce = rqNonce(reqHeaders);
  if (nonce === null) return sendFull('no-nonce');

  // Step 6. Section 3.4, steps 5 and 6: a b of the wrong length is absent.
  const n = analysis.blocks.length;
  const b = have.b !== null && have.b.length === 8 * n ? have.b : null;
  const indices = changeSet(analysis.blocks, b);
  if (indices.length === 0) return sendFull('empty-change-set');

  // Step 7.
  // H-56, H-78 (SHOULD): a changed html block that fails the client patch content
  // rule (PATCH_FORBIDDEN, FOREIGN_ALLOWED, the attribute rule) gets F. The same
  // function as the client step 9.
  for (const i of indices) {
    const b = analysis.blocks[i];
    if (b.format === 'html' && patchContentFailure(page.subarray(b.start, b.end)) !== null) return sendFull('patch-content-rule');
  }
  // H-107, H-108, H-109: U is the canonical URL of the request that this server
  // answers (the final request of the client), B the base of step 5, N the nonce
  // of this request (the field Rq-Nonce), R' the current page revision.
  const binding = deltaBinding(canonicalUrl(request.target), base, nonceText(nonce), manifest.page_revision);
  const envelope = deltaEnvelope(page, analysis, binding, indices);
  const delta = Buffer.from(JSON.stringify(envelope), 'utf8');
  if (delta.length > maxPatchBytes) return sendFull('delta-too-large');
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
    kind: 'delta',
    reason: 'delta',
  };
}
