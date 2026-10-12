// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// Reference client step: the request fields (negotiation.md section 6.1, 6.2)
// and the response handling (section 6.3) with the manifest checks of
// manifest-patch.md sections 1.7 and 1.8 and the delta checks of section 2.4 in
// the order of diff-engine.md section 3. It is the source of the expected
// results of the client fixtures (FX-RQ-C*), not a client SDK.
//
// Modelled context: the request URL (the cache key of the snapshot), the final
// URL after redirects, the build type and the http flag (H-49, H-58), whether
// the request carried credentials and whether the app declares the origin
// public (the partition of a new snapshot, H-61).
//
// Not modelled: the origin allowlist, the partition keys and a partition
// change (AUTH_PARTITION_CHANGED), the other cache identity inputs (Vary,
// variant, ignored query parameters), no-store, and the policy effects
// (freshness, offline). A context snapshot is in the request partition. The
// manifest members policy and limits are modelled as part of the page revision
// (H-55), as the patch size hint and, for mode, as the partition choice.

import {
  Code,
  asciiLower,
  DELTA_TYPE,
  LIMITS,
  digestPrefix,
  membersOf,
  pageRevision,
  sha256b64u,
} from './common.mjs';
import { combined, mediaType, normalizeHeaders, splitList } from './http.mjs';
import { analyze, findManifestElements, passesAttributeRule } from './markup.mjs';
import { parseStrict } from './json-strict.mjs';
import { loadSchema, validate } from './schema.mjs';
import { parseItem } from './sf.mjs';

const SCHEMAS = { manifest: loadSchema('manifest'), patch: loadSchema('patch') };

/** The partition of a snapshot: the partition of the request, or the shared public partition (H-61). */
export const Partition = Object.freeze({ REQUEST: 'request', PUBLIC: 'public' });

/**
 * A snapshot: the original document, its analysis, the policy and limits of
 * its manifest, its page revision, and its cache key (canonical URL and
 * partition).
 */
function snapshot(document, analysis, members, key = { url: null, partition: Partition.REQUEST }) {
  const m = membersOf(members);
  return { document, analysis, members: m, page_revision: pageRevision(analysis.template_revision, analysis.blocks, m), ...key };
}

/**
 * A client snapshot from an original document (without the manifest) that is
 * valid for RQP, and the members policy and limits of its manifest. `url` is
 * the URL of its cache key (null: not known).
 */
export function snapshotOf(document, members = {}, url = null) {
  const analysis = analyze(document);
  if (!analysis.valid) throw new Error('a snapshot needs a document that is valid for RQP');
  return snapshot(document, analysis, members, { url: url === null ? null : canonicalUrl(url), partition: Partition.REQUEST });
}

/**
 * The URL part of the cache key (H-58): scheme, host, port, path and query as
 * the WHATWG URL parser serializes them, without the fragment. The other steps
 * of cache identity section 2 (percent-encoding, ignored parameters) are not
 * modelled.
 */
export function canonicalUrl(url) {
  const u = new URL(url);
  u.hash = '';
  return u.href;
}

/** Block content up to this size is listed in a result; sha256 binds every block. */
export const CONTENT_LISTED_BYTES = 4096;

/**
 * The new snapshot in a result: revisions, blocks (content listed up to
 * CONTENT_LISTED_BYTES), the SHA-256 of the stored original document, and the
 * policy and limits of the manifest.
 */
function describe(snap) {
  const { document, analysis } = snap;
  return {
    url: snap.url,
    partition: snap.partition,
    template_revision: analysis.template_revision,
    page_revision: snap.page_revision,
    ...snap.members,
    blocks: analysis.blocks.map(({ id, format, sha256, start, end }) => ({
      id,
      format,
      sha256,
      ...(end - start <= CONTENT_LISTED_BYTES ? { content: document.toString('utf8', start, end) } : {}),
    })),
    document_sha256: sha256b64u(document),
  };
}

/**
 * Section 6.1, rule 5 (H-49) and section 6.3 (H-58): RQP for an https URL; for
 * an http URL only in a debug build with the app flag. `url` defaults to the
 * request URL.
 */
export function rqpAllowed(ctx, url = ctx.url) {
  const scheme = new URL(url).protocol;
  if (scheme === 'https:') return true;
  return scheme === 'http:' && ctx.build === 'debug' && ctx.allow_http === true;
}

/** The final URL of the response (after redirects); the request URL without a redirect. */
function finalUrlOf(ctx) {
  return ctx.final_url ?? ctx.url;
}

/** The base64 Byte Sequence content of Rq-Have member b (H-44). */
export function havePrefixes(blocks) {
  return Buffer.concat(blocks.map((b) => digestPrefix(b.sha256))).toString('base64');
}

/**
 * Section 6.2: the RQP request fields. navigation is "first-visit", "delta" or
 * "fallback" (section 6.3, item 5). Header lines that are not RQP fields are not shown.
 */
export function requestHeaders(ctx) {
  if (!rqpAllowed(ctx)) return [['accept', 'text/html']];
  const base = [
    ['rq-version', '1'],
    ['rq-capabilities', '(block-patch)'],
  ];
  if (ctx.navigation !== 'delta' || ctx.snapshot === null) return [...base, ['accept', 'text/html']];
  const { analysis, page_revision: pageRev } = ctx.snapshot;
  return [
    ...base,
    ['accept', `text/html, ${DELTA_TYPE}`],
    ['if-none-match', `"${pageRev}"`],
    ['rq-have', `t=${analysis.template_revision}, b=:${havePrefixes(analysis.blocks)}:`],
  ];
}

const result = (outcome, fields = {}) => ({
  outcome,
  error: null,
  next: 'show',
  metric: false,
  snapshot: null,
  ...fields,
});

/**
 * Section 6.3, item 1 (H-58): the client keeps its snapshot on a 304 only when
 * the request sent If-None-Match with exactly the strong tag of the snapshot
 * page revision R, the final URL has the cache key of the snapshot, and the
 * ETag of the 304, when present, matches R by weak comparison.
 */
export function notModifiedAccepted(ctx, sentHeaders, responseHeaders) {
  if (ctx.snapshot === null || canonicalUrl(finalUrlOf(ctx)) !== canonicalUrl(ctx.url)) return false;
  const tag = `"${ctx.snapshot.page_revision}"`;
  if (combined(sentHeaders, 'if-none-match') !== tag) return false;
  const etag = combined(responseHeaders, 'etag');
  if (etag === null) return true;
  return (etag.startsWith('W/') ? etag.slice(2) : etag) === tag;
}

/** The directive names of a Cache-Control value, in lower case. */
export function cacheDirectives(value) {
  if (value === null) return new Set();
  return new Set(splitList(value).map((d) => asciiLower(d.split('=')[0].replace(/[ \t]+$/, ''))));
}

/**
 * Manifest-patch section 1.4, mode (H-61): the shared public partition only
 * for a manifest with mode "public", an origin that the app declares public,
 * no Cache-Control directive "private", and an explicit "public" directive or
 * a request without credentials. Else the partition of the request.
 */
export function partitionFor(ctx, responseHeaders, members) {
  if (members.policy?.mode !== 'public' || ctx.public_origin !== true) return Partition.REQUEST;
  const directives = cacheDirectives(combined(responseHeaders, 'cache-control'));
  if (directives.has('private')) return Partition.REQUEST;
  return directives.has('public') || ctx.credentials === false ? Partition.PUBLIC : Partition.REQUEST;
}

/**
 * Section 6.3. Returns { outcome, error, next, metric, snapshot }.
 * outcome: "not-modified", "patched", "stored", "normal" or "rejected".
 * next: "keep", "show", "fallback-load" or "webview-load".
 *
 * @param {{ url: string, final_url?: string, build: string, allow_http: boolean, navigation: string,
 *   snapshot: object|null, credentials?: boolean, public_origin?: boolean }} ctx
 * @param {{ status: number, headers: Array<[string, string]>, body: Uint8Array|null }} response
 */
export function handleResponse(ctx, response) {
  const headers = normalizeHeaders(response.headers);
  const body = response.body === null ? Buffer.alloc(0) : Buffer.from(response.body);
  if (!rqpAllowed(ctx)) return result('normal'); // the client sent no RQP field
  if (response.status === 304) {
    // H-58: a 304 that the client cannot match to its snapshot gives a normal load.
    if (notModifiedAccepted(ctx, requestHeaders(ctx), headers)) return result('not-modified', { next: 'keep' });
    return result('normal', { next: 'webview-load' });
  }
  const type = mediaType(combined(headers, 'content-type'));
  if (response.status === 200 && type === DELTA_TYPE) return handleDelta(ctx, body);
  if (response.status === 200 && type === 'text/html' && rqpAllowed(ctx, finalUrlOf(ctx))) {
    const v = combined(headers, 'rq-version');
    const parsed = v === null ? null : parseItem(v);
    if (parsed !== null && parsed.item.type === 'integer' && parsed.item.value === 1) {
      return handleFull(ctx, headers, body);
    }
  }
  return result('normal');
}

// ---------------------------------------------------------------------------
// Full document: manifest-patch sections 1.7 (client) and 1.8.

/**
 * The checks of sections 1.7 and 1.8 against the final URL. Returns the new
 * snapshot (in the request partition) or a failure.
 */
export function checkFull(ctx, body) {
  const found = findManifestElements(body);
  if (found === null) return { error: 'walk' };
  // Section 1.7, client items 2 and 3.
  if (found.elements.length !== 1) return { error: 'count' };
  const el = found.elements[0];
  if (el.end < 0 || el.end !== found.headAt) return { error: 'position' };
  const text = body.subarray(el.textStart, el.textEnd);
  // Section 1.8, items 1 to 4.
  if (text.length > LIMITS.maxManifestBytes) return { error: 'size' };
  const json = parseStrict(text);
  if (!json.ok) return { error: `json-${json.rule}` };
  if (validate(SCHEMAS.manifest, json.value).length > 0) return { error: 'schema' };
  const m = json.value;
  if (new Set(m.blocks.map((b) => b.id)).size !== m.blocks.length) return { error: 'ids' };
  const url = new URL(finalUrlOf(ctx));
  if (m.origin !== url.origin || m.path !== url.pathname) return { error: 'url' };
  // Item 5 (H-48): the markup analysis of the original document.
  const original = Buffer.concat([body.subarray(0, el.start), body.subarray(el.end)]);
  const a = analyze(original);
  if (!a.valid) return { error: 'analysis' };
  const same =
    a.template_revision === m.template_revision &&
    a.blocks.length === m.blocks.length &&
    a.blocks.every((b, i) => b.id === m.blocks[i].id && b.format === m.blocks[i].format && b.sha256 === m.blocks[i].sha256);
  if (!same) return { error: 'analysis' };
  // H-55: the page revision with the policy line of this manifest.
  // H-58: the snapshot is bound to the cache key of the final URL.
  const snap = snapshot(original, a, m, { url: canonicalUrl(url.href), partition: Partition.REQUEST });
  if (snap.page_revision !== m.page_revision) return { error: 'revision' };
  return { snapshot: snap };
}

function handleFull(ctx, headers, body) {
  const checked = checkFull(ctx, body);
  if (checked.snapshot === undefined) return result('rejected', { error: Code.MANIFEST_INVALID });
  const snap = { ...checked.snapshot, partition: partitionFor(ctx, headers, checked.snapshot.members) };
  return result('stored', { snapshot: describe(snap) });
}

// ---------------------------------------------------------------------------
// Delta: diff-engine section 3, steps 2 and 4 to 13 (manifest-patch section 2.4).

/** Rules 1 to 4 of manifest-patch section 2.3: null, or the block index of each operation. */
function operationIndices(blocks, operations) {
  const out = [];
  let last = -1;
  for (const op of operations) {
    const i = blocks.findIndex((b) => b.id === op.block_id);
    if (i < 0 || i <= last || op.format !== blocks[i].format) return null;
    out.push(i);
    last = i;
  }
  return out;
}

/** The patched document: the snapshot document with the content of each patched block replaced. */
export function splice(document, blocks, replacements) {
  const parts = [];
  let at = 0;
  blocks.forEach((b, i) => {
    parts.push(document.subarray(at, b.start));
    parts.push(replacements.has(i) ? replacements.get(i) : document.subarray(b.start, b.end));
    at = b.end;
  });
  parts.push(document.subarray(at));
  return Buffer.concat(parts);
}

/**
 * Verify a delta against a snapshot. Returns { code } for the first failed
 * check, or { snapshot } with the new snapshot. `offered` is diff-engine step 2.
 * `maxPatchBytes` is the client resource limit (initial value 256 KiB; the
 * fixtures use the initial value).
 */
export function verifyDelta(snap, body, offered, maxPatchBytes = LIMITS.maxPatchBytes) {
  if (!offered) return { code: Code.PATCH_INVALID };
  const limit = Math.min(maxPatchBytes, snap.members.limits?.max_patch_bytes ?? Infinity);
  if (body.length > limit) return { code: Code.PATCH_TOO_LARGE };
  const json = parseStrict(body);
  if (!json.ok) return { code: Code.PATCH_INVALID };
  if (validate(SCHEMAS.patch, json.value).length > 0) return { code: Code.PATCH_INVALID };
  const env = json.value;
  const { analysis, document, members } = snap;
  if (env.template_revision !== analysis.template_revision) return { code: Code.TEMPLATE_MISMATCH };
  if (env.base_revision !== snap.page_revision) return { code: Code.PATCH_BASE_MISMATCH };
  const indices = operationIndices(analysis.blocks, env.operations);
  if (indices === null) return { code: Code.PATCH_INVALID };
  const replacements = new Map();
  for (const [k, op] of env.operations.entries()) {
    const bytes = Buffer.from(op.content, 'utf8');
    if (bytes.length > LIMITS.maxBlockBytes) return { code: Code.PATCH_TOO_LARGE };
    if (sha256b64u(bytes) !== op.sha256) return { code: Code.INTEGRITY_ERROR };
    replacements.set(indices[k], bytes);
  }
  // Steps 5 to 7: the full snapshot hash for each block without an operation,
  // and the policy line of the snapshot manifest (H-55).
  const next = analysis.blocks.map((b, i) => ({
    id: b.id,
    format: b.format,
    sha256: replacements.has(i) ? sha256b64u(replacements.get(i)) : b.sha256,
  }));
  if (pageRevision(analysis.template_revision, next, members) !== env.next_revision) return { code: Code.INTEGRITY_ERROR };
  // Step 8 (H-48).
  const patched = splice(document, analysis.blocks, replacements);
  if (patched.length > LIMITS.maxDocumentBytes) return { code: Code.PATCH_TOO_LARGE };
  const after = analyze(patched);
  if (!after.valid || after.template_revision !== analysis.template_revision) {
    return { code: Code.PATCH_CONTENT_REJECTED };
  }
  const sameShape =
    after.blocks.length === analysis.blocks.length &&
    after.blocks.every((b, i) => b.id === analysis.blocks[i].id && b.format === analysis.blocks[i].format);
  if (!sameShape) return { code: Code.PATCH_CONTENT_REJECTED };
  const patchedSnapshot = snapshot(patched, after, members, { url: snap.url, partition: snap.partition });
  if (patchedSnapshot.page_revision !== env.next_revision) return { code: Code.INTEGRITY_ERROR };
  // Step 9: the attribute rule on each patched html block.
  for (const i of replacements.keys()) {
    if (analysis.blocks[i].format === 'html' && !passesAttributeRule(replacements.get(i))) {
      return { code: Code.PATCH_CONTENT_REJECTED };
    }
  }
  return { snapshot: patchedSnapshot };
}

function handleDelta(ctx, body) {
  // Diff-engine step 1 (H-58): the final URL has the cache key of the snapshot.
  // A release build sent RQP fields only for https, so the same key is https too.
  const sameKey = canonicalUrl(finalUrlOf(ctx)) === canonicalUrl(ctx.url);
  // Step 2: the request offered a delta. Steps 1 and 2 have the same code (PATCH_INVALID).
  const offered = sameKey && ctx.navigation === 'delta' && ctx.snapshot !== null;
  const verified = ctx.snapshot === null ? { code: Code.PATCH_INVALID } : verifyDelta(ctx.snapshot, body, offered);
  if (verified.code !== undefined) {
    return result('rejected', {
      error: verified.code,
      next: ctx.navigation === 'fallback' ? 'webview-load' : 'fallback-load',
      metric: verified.code === Code.PATCH_CONTENT_REJECTED,
    });
  }
  return result('patched', { snapshot: describe(verified.snapshot) });
}

// ---------------------------------------------------------------------------
// Harness interface (check.mjs --client). A context snapshot is plain data:
// { document, template_revision, page_revision, blocks, policy?, limits? }.

function internal(context) {
  if (context.snapshot === null) return { ...context, snapshot: null };
  const snap = snapshotOf(Buffer.from(context.snapshot.document), membersOf(context.snapshot), context.url);
  if (snap.page_revision !== context.snapshot.page_revision) throw new Error('the context snapshot does not match its document');
  return { ...context, snapshot: snap };
}

/** The request fields that the client sends for a context (negotiation section 6.2). */
export function clientRequest(context) {
  return requestHeaders(internal(context));
}

/** The client step for a context and a response (negotiation section 6.3). */
export function clientStep(context, response) {
  return handleResponse(internal(context), response);
}
