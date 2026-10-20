// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// Reference client step: the request fields (negotiation.md section 6.1, 6.2)
// and the response handling (section 6.3) with the manifest checks of
// manifest-patch.md sections 1.7 and 1.8 and the delta checks of section 2.4 in
// the order of diff-engine.md section 3. It is the source of the expected
// results of the client fixtures (FX-RQ-C*), not a client SDK.
//
// Modelled context: the request URL (the cache key of the snapshot), the final
// URL after redirects and whether the response came through a redirect (H-67),
// the build type and the http flag (H-49, H-58), the origin allowlist as a
// list of serialized origins (RQP-05, H-62 item 1, DL-038, H-69), whether the
// request carried credentials (true, false or unknown; H-62 item 2), whether
// the native SDK saw all header fields of the response (H-64), whether the app
// declares the origin public (the partition of a new snapshot, H-61), whether
// the app turned the public partition on (H-81), and the partition of the
// context snapshot (request or public; H-65; a public context snapshot with
// the setting off is not used, usableSnapshot), and the client nonce of the
// request (H-108, H-109: the field Rq-Nonce; the request keeps it for the
// delta check). A URL is the
// WHATWG parse of its string (DL-040 L-3: an SDK takes the URL object of its
// network stack).
//
// Not modelled: the partition keys and a partition change
// (AUTH_PARTITION_CHANGED, also for a 304), the other cache identity inputs
// (Vary, variant, ignored query parameters), no-store, the policy effects
// (freshness, offline; also DL-040 I-3: a 304 that does not meet RQP-25 does
// not update the stored metadata of a public entry), the choice between two
// entries of one cache key (DL-039), the configuration step that rejects an
// allowlist entry (H-69; validAllowlistEntry is the check), and the delivery to
// the WebView (H-66, DL-040 L-2: text/html with the encoding utf-8; a platform
// adapter rule). The manifest members policy
// and limits are modelled as part of the page revision (H-55), as the patch
// size hint and, for mode, as the partition choice.

import { randomBytes } from 'node:crypto';
import {
  Code,
  asciiLower,
  DELTA_TYPE,
  LIMITS,
  NONCE_BYTES,
  canonicalUrl,
  deltaBinding,
  digestPrefix,
  isNonceText,
  membersOf,
  nonceText,
  pageRevision,
  sha256b64u,
} from './common.mjs';
import {
  Charset,
  combined,
  contentTypeCharset,
  has,
  mediaType,
  normalizeHeaders,
  splitList,
  startsWithUtf8Bom,
} from './http.mjs';
import { analyze, documentHeadStart, findManifestElements, passesPatchContentRule } from './markup.mjs';
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
 * the URL of its cache key (null: not known); `partition` is the partition of
 * its cache key (H-61, H-65).
 */
export function snapshotOf(document, members = {}, url = null, partition = Partition.REQUEST) {
  const analysis = analyze(document);
  if (!analysis.valid) throw new Error('a snapshot needs a document that is valid for RQP');
  if (partition !== Partition.REQUEST && partition !== Partition.PUBLIC) throw new Error(`unknown partition ${partition}`);
  return snapshot(document, analysis, members, { url: url === null ? null : canonicalUrl(url), partition });
}

/**
 * canonicalUrl (common.mjs) is the URL part of the cache key (H-58) and the
 * URL U of the delta binding (H-107): scheme, host, port, path and query as
 * the WHATWG URL parser serializes them, without the fragment. The other steps
 * of cache identity section 2 (percent-encoding, ignored parameters) are not
 * modelled. The reference keeps the user information in the string; this is
 * not what H-68 relies on: rqpAllowed refuses a URL with user information on
 * every path (F, 304, delta), so an SDK may also remove it from its keys
 * (cache identity section 2).
 */
export { canonicalUrl };

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
 * Section 6.1, rule 1 (RQP-05, H-62 item 1, DL-038): the origin of `url` is in
 * the origin allowlist. The client compares the serialized origin of the URL
 * (WHATWG URL Standard: scheme, lower-case host in its IDNA form, no default
 * port; cache identity section 2) with each entry by exact string match. There
 * are no wildcards and no suffix match; an entry that is not a serialized
 * origin never matches. Without a list (ctx.allowlist not an array) no origin
 * is in the allowlist (fail closed).
 */
export function inAllowlist(ctx, url) {
  return Array.isArray(ctx.allowlist) && ctx.allowlist.includes(new URL(url).origin);
}

/**
 * Section 6.1, rule 1 (H-69, RQP-05): an allowlist entry is valid only when it
 * is a string that is equal to its own serialized origin (WHATWG URL parse,
 * then the ASCII serialization of the origin). So upper case, a default port,
 * a trailing "/", a path, user information, white space, a Unicode IDN host
 * and "*" are invalid. H-70: the host also has no "*" (the WHATWG host parser
 * accepts it, so "https://*.example.com" is its own origin, but it is never a
 * wildcard). DL-041: the scheme is https or http (the only schemes that can
 * get RQP, rule 5). An SDK rejects a configuration with an invalid entry; the
 * reference has no configuration step, so the fixture loader uses this check.
 * Never throws.
 */
export function validAllowlistEntry(entry) {
  if (typeof entry !== 'string' || !URL.canParse(entry)) return false;
  const url = new URL(entry);
  return url.origin === entry && (url.protocol === 'https:' || url.protocol === 'http:') && !url.hostname.includes('*');
}

/**
 * Section 6.1, rule 1 (H-68): the URL has a user name or a password (the
 * WHATWG URL "includes credentials"). An empty user information ("https://@host/")
 * is not kept by the parser and does not count.
 */
export function hasUserinfo(url) {
  const u = new URL(url);
  return u.username !== '' || u.password !== '';
}

/**
 * Section 6.1, rules 1 and 5 (RQP-05, H-49, H-68) and section 6.3 (H-58,
 * H-62): the client sends RQP fields to `url` only when the URL has no user
 * name and no password, its origin is in the allowlist, and only for https;
 * for http only in a debug build with the app flag. `url` defaults to the
 * request URL. A value that the WHATWG URL parser rejects (or no value) is
 * not allowed (fail closed; S6 security re-review B6): never throws.
 */
export function rqpAllowed(ctx, url = ctx.url) {
  if (url === null || url === undefined || !URL.canParse(url)) return false;
  if (hasUserinfo(url) || !inAllowlist(ctx, url)) return false;
  const scheme = new URL(url).protocol;
  if (scheme === 'https:') return true;
  return scheme === 'http:' && ctx.build === 'debug' && ctx.allow_http === true;
}

/** The final URL of the response (after redirects); the request URL without a redirect. */
function finalUrlOf(ctx) {
  return ctx.final_url ?? ctx.url;
}

/**
 * H-67: the response came through at least one redirect, also a redirect to
 * the same URL. Fails closed: only ctx.redirected === false with a final URL
 * equal to the request URL (the same string) means no redirect.
 */
export function redirected(ctx) {
  return ctx.redirected !== false || finalUrlOf(ctx) !== ctx.url;
}

/** The base64 Byte Sequence content of Rq-Have member b (H-44). */
export function havePrefixes(blocks) {
  return Buffer.concat(blocks.map((b) => digestPrefix(b.sha256))).toString('base64');
}

/**
 * H-81 (S4 security review L1): the context snapshot that the client may use,
 * or null. With the app setting of the public partition off (public_enabled
 * not true), an entry of the public partition is not read, offered
 * (If-None-Match, Rq-Have), revalidated or shown: the client acts as if it had
 * no snapshot. Such an entry can exist from a time when the setting was on; an
 * SDK SHOULD delete it (not modelled). Every path that reads the context
 * snapshot (request fields, 304, delta) goes through this function.
 */
export function usableSnapshot(ctx) {
  const snap = ctx.snapshot ?? null;
  if (snap !== null && snap.partition === Partition.PUBLIC && ctx.public_enabled !== true) return null;
  return snap;
}

/**
 * H-108: a new client nonce, the text N of 16 bytes from the CSPRNG of
 * Node.js (crypto.randomBytes). An SDK makes one for every delta request and
 * never uses it again.
 */
export function newNonce() {
  return nonceText(randomBytes(NONCE_BYTES));
}

/**
 * H-108: the nonce N of the request of the context (ctx.nonce, the text of
 * 16 bytes; isNonceText), or null when the context has no valid nonce (for
 * example an SDK whose CSPRNG failed). The client keeps N with the request:
 * the delta check of section 6.3 uses the N that this request sent.
 */
export function requestNonce(ctx) {
  return isNonceText(ctx.nonce) ? ctx.nonce : null;
}

/**
 * Section 6.2: the RQP request fields. navigation is "first-visit", "delta" or
 * "fallback" (section 6.3, item 5). Header lines that are not RQP fields are
 * not shown. Validators come only from a usable snapshot (H-81). A delta
 * request has the nonce N of the context in the field Rq-Nonce, one field
 * line with a Byte Sequence of 16 bytes (H-108, H-109); Rq-Have has only t
 * and b, so its value is the same for every request from one snapshot. A
 * context without a valid nonce offers no delta (no Rq-Have, no Rq-Nonce, no
 * delta media type) and keeps If-None-Match, so a 304 stays possible (fail
 * closed).
 */
export function requestHeaders(ctx) {
  if (!rqpAllowed(ctx)) return [['accept', 'text/html']];
  const base = [
    ['rq-version', '1'],
    ['rq-capabilities', '(block-patch)'],
  ];
  const snap = usableSnapshot(ctx);
  if (ctx.navigation !== 'delta' || snap === null) return [...base, ['accept', 'text/html']];
  const { analysis, page_revision: pageRev } = snap;
  const nonce = requestNonce(ctx);
  if (nonce === null) return [...base, ['accept', 'text/html'], ['if-none-match', `"${pageRev}"`]];
  return [
    ...base,
    ['accept', `text/html, ${DELTA_TYPE}`],
    ['if-none-match', `"${pageRev}"`],
    ['rq-have', `t=${analysis.template_revision}, b=:${havePrefixes(analysis.blocks)}:`],
    ['rq-nonce', `:${Buffer.from(nonce, 'base64url').toString('base64')}:`],
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
 * ETag of the 304, when present, matches R by weak comparison. H-81: a public
 * snapshot with the app setting off is no snapshot (usableSnapshot).
 */
export function notModifiedAccepted(ctx, sentHeaders, responseHeaders) {
  const snap = usableSnapshot(ctx);
  if (snap === null || canonicalUrl(finalUrlOf(ctx)) !== canonicalUrl(ctx.url)) return false;
  const tag = `"${snap.page_revision}"`;
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
 * Manifest-patch section 1.4, mode (H-62 item 2): the request had no
 * credentials only when the client knows it (credentials === false). Unknown
 * credentials (null or absent) count as present. A Set-Cookie field in the
 * response also counts as credentials; partitionFor checks it (one place).
 */
export function withoutCredentials(ctx) {
  return ctx.credentials === false;
}

/**
 * DL-038: a Set-Cookie or a Set-Cookie2 field (RFC 2965, obsolete) in the
 * response. Field names are compared ASCII case-insensitively
 * (normalizeHeaders gives lower-case names).
 */
export function setsCookie(responseHeaders) {
  return has(responseHeaders, 'set-cookie') || has(responseHeaders, 'set-cookie2');
}

/**
 * Manifest-patch section 1.4, mode (H-61 to H-64, H-67, RQP-25): the shared
 * public partition only for a manifest with mode "public", an origin that the
 * app declares public, a response that did not come through a redirect (H-67;
 * a redirect response can set a cookie that the next request carries), a
 * response whose header fields the native SDK saw in full (H-64; any other
 * response counts as a response with a Set-Cookie field), no Set-Cookie field
 * (H-63), no Cache-Control directive "private", and an explicit "public"
 * directive or a request without credentials. Else the partition of the
 * request. deltaPartition uses it, so the redirect rule also covers a delta.
 * H-81: the public partition is off by default; only an app that turns it on
 * (ctx.public_enabled === true; an absent flag is off) uses it.
 */
export function partitionFor(ctx, responseHeaders, members) {
  if (ctx.public_enabled !== true) return Partition.REQUEST;
  if (members.policy?.mode !== 'public' || ctx.public_origin !== true) return Partition.REQUEST;
  if (redirected(ctx)) return Partition.REQUEST;
  if (ctx.response_fields_visible !== true || setsCookie(responseHeaders)) return Partition.REQUEST;
  const directives = cacheDirectives(combined(responseHeaders, 'cache-control'));
  if (directives.has('private')) return Partition.REQUEST;
  return directives.has('public') || withoutCredentials(ctx) ? Partition.PUBLIC : Partition.REQUEST;
}

/**
 * H-65: the partition of the result of a delta. A snapshot in the partition of
 * the request gives a result in the partition of the request. The result of a
 * delta on a public snapshot stays public only when the delta response meets
 * RQP-25 (partitionFor with the policy of the snapshot manifest; so also no
 * redirect, H-67) and the
 * client knows that the request had no credentials: a delta has
 * Cache-Control no-store, so an explicit "public" directive does not count.
 * Else the result goes to the partition of the request, and the public
 * snapshot does not change.
 */
export function deltaPartition(ctx, responseHeaders, snap) {
  if (snap.partition !== Partition.PUBLIC) return Partition.REQUEST;
  const stays = withoutCredentials(ctx) && partitionFor(ctx, responseHeaders, snap.members) === Partition.PUBLIC;
  return stays ? Partition.PUBLIC : Partition.REQUEST;
}

/**
 * Manifest-patch section 1.7, client item 6 (H-62 item 3): the browser decodes
 * F as UTF-8. True when the body starts with a UTF-8 byte order mark (BOM
 * sniffing comes first in the HTML encoding sniffing algorithm) or the
 * Content-Type has a charset parameter that is a UTF-8 label.
 */
export function decodedAsUtf8(responseHeaders, body) {
  return startsWithUtf8Bom(body) || contentTypeCharset(responseHeaders) === Charset.UTF8;
}

/**
 * Section 6.3. Returns { outcome, error, next, metric, snapshot }.
 * outcome: "not-modified", "patched", "stored", "normal" or "rejected".
 * next: "keep", "show", "fallback-load" or "webview-load".
 *
 * H-62 (1): the client uses F, a 304 or D only when the final request carried
 * RQP fields (rqpAllowed for the final URL). For F this is checked below. A 304
 * and D need the cache key of the snapshot, so the final URL has the origin of
 * the request URL, which passed rqpAllowed at the start. canonicalUrl keeps
 * the user information, so a final URL with a user name or a password (H-68)
 * never has the cache key of a snapshot: no snapshot is stored for such a URL.
 *
 * @param {{ url: string, final_url?: string, redirected?: boolean, build: string, allow_http: boolean, navigation: string,
 *   snapshot: object|null, allowlist: string[], credentials?: boolean|null,
 *   response_fields_visible?: boolean, public_enabled?: boolean, public_origin?: boolean, nonce?: string|null }} ctx
 * @param {{ status: number, headers: Array<[string, string]>, body: Uint8Array|null }} response
 */
export function handleResponse(ctx, response) {
  const headers = normalizeHeaders(response.headers);
  const body = response.body === null ? Buffer.alloc(0) : Buffer.from(response.body);
  if (!rqpAllowed(ctx)) return result('normal'); // the client sent no RQP field
  if (response.status === 304) {
    // H-58: a 304 that the client cannot match to its snapshot gives a normal load.
    // H-68: also checked here, not only through the cache key (an SDK may strip user information from keys).
    if (rqpAllowed(ctx, finalUrlOf(ctx)) && notModifiedAccepted(ctx, requestHeaders(ctx), headers)) return result('not-modified', { next: 'keep' });
    return result('normal', { next: 'webview-load' });
  }
  const type = mediaType(combined(headers, 'content-type'));
  if (response.status === 200 && type === DELTA_TYPE) return handleDelta(ctx, headers, body);
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
  if (el.end < 0) return { error: 'position' };
  // Item 5: the original document O is F without the bytes of the element.
  const original = Buffer.concat([body.subarray(0, el.start), body.subarray(el.end)]);
  // Item 3 (H-72, H-79a, client rule C2): the insertion point P of the server,
  // computed on O, is the offset of the manifest start tag in F. A walk on F
  // would stop at the manifest script itself (an implied head). P is -1 when O
  // has no head start tag and no implied head; el.start is never -1.
  if (documentHeadStart(original) !== el.start) return { error: 'position' };
  const text = body.subarray(el.textStart, el.textEnd);
  // Section 1.8, items 1, 2 and 4 (item 3 is removed: no block list, H-103).
  if (text.length > LIMITS.maxManifestBytes) return { error: 'size' };
  const json = parseStrict(text);
  if (!json.ok) return { error: `json-${json.rule}` };
  if (validate(SCHEMAS.manifest, json.value).length > 0) return { error: 'schema' };
  const m = json.value;
  const url = new URL(finalUrlOf(ctx));
  if (m.origin !== url.origin || m.path !== url.pathname) return { error: 'url' };
  // Item 5 (H-48, H-103): the markup analysis of the original document gives the
  // template revision and the block digests. The manifest has neither (H-103):
  // the page revision that they give with the policy line of this manifest
  // (H-55) must be page_revision. After this check the client uses its computed
  // values (snapshot, Rq-Have).
  const a = analyze(original);
  if (!a.valid) return { error: 'analysis' };
  // H-58: the snapshot is bound to the cache key of the final URL.
  const snap = snapshot(original, a, m, { url: canonicalUrl(url.href), partition: Partition.REQUEST });
  if (snap.page_revision !== m.page_revision) return { error: 'revision' };
  return { snapshot: snap };
}

function handleFull(ctx, headers, body) {
  // H-62 (3): a document that the browser can decode with another encoding is not used.
  if (!decodedAsUtf8(headers, body)) return result('rejected', { error: Code.MANIFEST_INVALID });
  const checked = checkFull(ctx, body);
  if (checked.snapshot === undefined) return result('rejected', { error: Code.MANIFEST_INVALID });
  const snap = { ...checked.snapshot, partition: partitionFor(ctx, headers, checked.snapshot.members) };
  return result('stored', { snapshot: describe(snap) });
}

// ---------------------------------------------------------------------------
// Delta: diff-engine section 3, steps 2 and 4 to 13 (manifest-patch section 2.4).

/**
 * Rules 1 to 3 of manifest-patch section 2.3 (step 3): null, or the block
 * index of each operation. Each block_id names a block of the snapshot, a block
 * has at most one operation, and the operations are in document order (a
 * strictly growing index gives the last two rules). Rule 4 (format) is removed
 * by H-104: the block keeps the format of the snapshot.
 */
function operationIndices(blocks, operations) {
  const out = [];
  let last = -1;
  for (const op of operations) {
    const i = blocks.findIndex((b) => b.id === op.block_id);
    if (i <= last) return null; // also i === -1: not a block of the snapshot
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
 * The binding that the client expects (manifest-patch section 2.2, step 7;
 * H-107, H-108), or null when an input is missing or has no valid form: `url`
 * is not a string that the WHATWG URL parser accepts, `base` is not a page
 * revision, `nonce` is not the text of 16 bytes. Never throws (S6 security
 * re-review B6): the caller turns null into INTEGRITY_ERROR.
 */
export function expectedBinding(url, base, nonce, next) {
  if (typeof url !== 'string' || !URL.canParse(url)) return null;
  try {
    return deltaBinding(canonicalUrl(url), base, nonce, next);
  } catch (error) {
    if (error instanceof TypeError) return null; // an input without its form: no binding can match
    throw error;
  }
}

/**
 * Verify a delta against a snapshot. Returns { code } for the first failed
 * check, or { snapshot } with the new snapshot. Never throws for any body,
 * URL or nonce. `offered` is diff-engine step 2. `maxPatchBytes` is the
 * client resource limit (initial value 256 KiB; the fixtures use the initial
 * value). `url` is the final URL of the delta response, the U of the binding
 * (H-107); the default is the cache key URL of the snapshot, which
 * handleDelta has checked to be the same (step 1, H-58). `nonce` is the text
 * N of the nonce that the request of this response sent (H-108). When the
 * client cannot compute an input of the binding (no URL, a URL that does not
 * parse, no valid nonce), no binding can match: INTEGRITY_ERROR at step 7
 * (fail closed, S6 security re-review B6), and the caller does one fallback
 * load.
 */
export function verifyDelta(snap, body, offered, maxPatchBytes = LIMITS.maxPatchBytes, url = snap.url, nonce = null) {
  if (!offered) return { code: Code.PATCH_INVALID };
  const limit = Math.min(maxPatchBytes, snap.members.limits?.max_patch_bytes ?? Infinity);
  if (body.length > limit) return { code: Code.PATCH_TOO_LARGE };
  const json = parseStrict(body);
  if (!json.ok) return { code: Code.PATCH_INVALID };
  if (validate(SCHEMAS.patch, json.value).length > 0) return { code: Code.PATCH_INVALID };
  const env = json.value;
  const { analysis, document, members } = snap;
  // Steps 1 and 2 are removed (H-104): the envelope has no template_revision and
  // no base_revision. A delta for another template, another base or another URL
  // fails step 7 (the binding, H-107).
  const indices = operationIndices(analysis.blocks, env.operations);
  if (indices === null) return { code: Code.PATCH_INVALID };
  const replacements = new Map();
  for (const [k, op] of env.operations.entries()) {
    const bytes = Buffer.from(op.content, 'utf8');
    if (bytes.length > LIMITS.maxBlockBytes) return { code: Code.PATCH_TOO_LARGE };
    replacements.set(indices[k], bytes);
  }
  // Steps 4 to 7 (H-44, H-104, H-107, H-108): the client computes the hash of
  // each new content and keeps the full snapshot hash for each block without an
  // operation; with the template revision and the policy line of the snapshot
  // (H-55) they give the next page revision R'. The binding of U (the final
  // URL), B (the page revision of the snapshot, the value of If-None-Match), N
  // (the nonce that this request sent) and R' must be the binding of the
  // envelope. This is the only integrity check of the envelope.
  const next = analysis.blocks.map((b, i) => ({
    id: b.id,
    format: b.format,
    sha256: replacements.has(i) ? sha256b64u(replacements.get(i)) : b.sha256,
  }));
  const nextRevision = pageRevision(analysis.template_revision, next, members);
  const expected = expectedBinding(url, snap.page_revision, nonce, nextRevision);
  if (expected === null || expected !== env.binding) return { code: Code.INTEGRITY_ERROR };
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
  // The page revision of the patched document is R' (step 8); the new snapshot has it.
  if (patchedSnapshot.page_revision !== nextRevision) return { code: Code.INTEGRITY_ERROR };
  // Step 9 (H-48, H-78, MUST): the patch content rule on each patched html
  // block. Since H-78 the analysis of step 8 accepts script, style, iframe … in
  // a block, so this check is the only gate for them. json content is data.
  for (const i of replacements.keys()) {
    if (analysis.blocks[i].format === 'html' && !passesPatchContentRule(replacements.get(i))) {
      return { code: Code.PATCH_CONTENT_REJECTED };
    }
  }
  return { snapshot: patchedSnapshot };
}

function handleDelta(ctx, headers, body) {
  // Diff-engine step 1 (H-58): the final URL has the cache key of the snapshot.
  // A release build sent RQP fields only for https, so the same key is https too.
  // H-68: a final URL with user information never gets a delta, also when a key would match.
  const sameKey = rqpAllowed(ctx, finalUrlOf(ctx)) && canonicalUrl(finalUrlOf(ctx)) === canonicalUrl(ctx.url);
  // Step 2: the request offered a delta. Steps 1 and 2 have the same code (PATCH_INVALID).
  // H-81: a public snapshot with the app setting off is no snapshot, so no delta was offered.
  // H-108, H-109: a request without a valid nonce sent no Rq-Have and no Rq-Nonce (requestHeaders), so no delta was offered.
  const snap = usableSnapshot(ctx);
  const nonce = requestNonce(ctx);
  const offered = sameKey && ctx.navigation === 'delta' && snap !== null && nonce !== null;
  // H-107: U of the binding is the final URL of the response (the URL of the
  // request that the server answered). Step 1 has made sure it has the cache key
  // of the snapshot. H-108: N is the nonce that this request sent.
  const verified = snap === null ? { code: Code.PATCH_INVALID } : verifyDelta(snap, body, offered, LIMITS.maxPatchBytes, finalUrlOf(ctx), nonce);
  if (verified.code !== undefined) {
    return result('rejected', {
      error: verified.code,
      next: ctx.navigation === 'fallback' ? 'webview-load' : 'fallback-load',
      metric: verified.code === Code.PATCH_CONTENT_REJECTED,
    });
  }
  // H-65: the partition of the result; a public snapshot itself does not change.
  const partition = deltaPartition(ctx, headers, snap);
  return result('patched', { snapshot: describe({ ...verified.snapshot, partition }) });
}

// ---------------------------------------------------------------------------
// Harness interface (check.mjs --client). A context snapshot is plain data:
// { document, template_revision, page_revision, blocks, policy?, limits?,
// partition? } (partition "request", the default, or "public"; H-65).
// context.nonce is the text N of the nonce of the request (H-108). The
// fixtures give it, so that the request fields and the binding are fixed
// values; a context without the member gets a new nonce from newNonce (as an
// SDK makes one for every request), and a context with another value (for
// example null) has no valid nonce (requestNonce).

function internal(context) {
  const nonce = context.nonce === undefined ? newNonce() : context.nonce;
  if (context.snapshot === null) return { ...context, nonce, snapshot: null };
  const s = context.snapshot;
  const snap = snapshotOf(Buffer.from(s.document), membersOf(s), context.url, s.partition ?? Partition.REQUEST);
  if (snap.page_revision !== s.page_revision) throw new Error('the context snapshot does not match its document');
  return { ...context, nonce, snapshot: snap };
}

/** The request fields that the client sends for a context (negotiation section 6.2). */
export function clientRequest(context) {
  return requestHeaders(internal(context));
}

/** The client step for a context and a response (negotiation section 6.3). */
export function clientStep(context, response) {
  return handleResponse(internal(context), response);
}
