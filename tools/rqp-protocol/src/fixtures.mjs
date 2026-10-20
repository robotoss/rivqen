// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// Fixture model of fixtures/rqp/protocol (README.md there): reading case.json,
// the placeholders, the response recipes of client cases, and the expected
// files that gen.mjs writes. All expected values come from the reference steps
// (src/server.mjs, src/client.mjs).

import { readFileSync, readdirSync, statSync } from 'node:fs';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { MANIFEST_END, UnsupportedInput, canonicalUrl, deltaBinding, isNonceText, membersOf, nonceText, pageRevision, sha256b64u } from './common.mjs';
import { deltaBase, handle, manifestOf, rqNonce, writtenMembers } from './server.mjs';
import { Partition, handleResponse, hasUserinfo, havePrefixes, requestHeaders, snapshotOf, validAllowlistEntry } from './client.mjs';
import { combined, normalizeHeaders, varyMembers } from './http.mjs';
import { analyze, documentHeadStart } from './markup.mjs';
import { UnknownPage, pageBytes } from './pages.mjs';

export const FIXTURES_DIR = fileURLToPath(new URL('../../../fixtures/rqp/protocol/', import.meta.url));
export const DEFAULT_TARGET = 'https://shop.example.com/catalog';
export const RESPONSE_BODY = 'response.body';
/**
 * The nonce N of a client case without the member context.nonce (H-108): the
 * bytes 00 to 0F. The fixtures need fixed values; an SDK makes a new nonce
 * from a CSPRNG for every request and never uses a fixed value.
 */
export const DEFAULT_NONCE = 'AAECAwQFBgcICQoLDA0ODw';
const ALWAYS_FORBIDDEN = ['rq-revision', 'rq-template-revision']; // H-54, RQP-21

/** A defect in a case.json file. */
export class CaseError extends Error {
  constructor(id, message) {
    super(`${id}: ${message}`);
    this.name = 'CaseError';
    this.code = 'RQP_FIXTURE_CASE';
  }
}

/** Fixture directory names (FX-RQ-*) in byte order. */
export function fixtureNames(dir = FIXTURES_DIR) {
  return readdirSync(dir)
    .filter((n) => n.startsWith('FX-RQ-') && statSync(join(dir, n)).isDirectory())
    .sort((a, b) => Buffer.compare(Buffer.from(a), Buffer.from(b)));
}

export function readJsonFile(path) {
  return JSON.parse(readFileSync(path, 'utf8'));
}

function onlyKeys(id, obj, allowed, where) {
  if (obj === null || typeof obj !== 'object' || Array.isArray(obj)) throw new CaseError(id, `${where} must be an object`);
  for (const k of Object.keys(obj)) if (!allowed.includes(k)) throw new CaseError(id, `unknown member ${where}.${k}`);
}

function analysisOf(id, page) {
  const a = analyze(pageBytes(page));
  if (!a.valid) throw new CaseError(id, `page ${page} is not valid for RQP`);
  return a;
}

/**
 * Replace the placeholders of README.md: {{r1:<page>}} -> "r1.…" (with quotes),
 * {{t1:<page>}} -> t1.…, {{have:<page>}} -> :<base64 of the 64-bit prefixes>:.
 * `members` are the policy and limits of the manifest that the client holds
 * (case member client_members); they change r1 only (H-55).
 */
export function substitute(id, text, members = {}) {
  const out = text.replace(/\{\{([a-z0-9]+):([a-z0-9-]+)\}\}/g, (_, kind, page) => {
    const a = analysisOf(id, page);
    if (kind === 'r1') return `"${pageRevision(a.template_revision, a.blocks, members)}"`;
    if (kind === 't1') return a.template_revision;
    if (kind === 'have') return `:${havePrefixes(a.blocks)}:`;
    throw new CaseError(id, `unknown placeholder kind ${kind}`);
  });
  if (out.includes('{{') || out.includes('}}')) throw new CaseError(id, 'a placeholder is not closed or not known');
  return out;
}

function substituteHeaders(id, headers, members = {}) {
  if (!Array.isArray(headers)) throw new CaseError(id, 'headers must be a list of [name, value] lines');
  return headers.map((line) => {
    if (!Array.isArray(line) || line.length !== 2 || typeof line[0] !== 'string' || typeof line[1] !== 'string') {
      throw new CaseError(id, 'a header line must be [name, value]');
    }
    if (line[0] !== line[0].toLowerCase()) throw new CaseError(id, `header name ${line[0]} is not lower case`);
    return [line[0], substitute(id, line[1], members)];
  });
}

/** Load case.json of one fixture directory and check its shape. */
export function loadCase(dir, name) {
  const c = readJsonFile(join(dir, name, 'case.json'));
  if (c.id !== name) throw new CaseError(name, `case.json id ${c.id} is not the directory name`);
  if (typeof c.description !== 'string' || !Array.isArray(c.rules)) throw new CaseError(name, 'description and rules are required');
  if (c.kind === 'client') {
    onlyKeys(name, c, ['id', 'kind', 'description', 'rules', 'context', 'response'], 'case');
    onlyKeys(
      name,
      c.context,
      ['url', 'final_url', 'redirected', 'build', 'allow_http', 'allowlist', 'navigation', 'snapshot', 'credentials', 'response_fields_visible', 'public_enabled', 'public_origin', 'nonce'],
      'context',
    );
    onlyKeys(name, c.response, ['status', 'headers', 'body'], 'response');
  } else {
    onlyKeys(name, c, ['id', 'description', 'rules', 'page', 'client_page', 'client_members', 'server_config', 'app', 'request', 'level'], 'case');
    if (c.level !== undefined && c.level !== 'should') throw new CaseError(name, 'level must be "should" when present');
    onlyKeys(name, c.server_config ?? {}, ['policy', 'limits'], 'server_config');
    onlyKeys(name, c.client_members ?? {}, ['policy', 'limits'], 'client_members');
    onlyKeys(name, c.request, ['method', 'target', 'headers'], 'request');
    onlyKeys(name, c.app, ['status', 'headers'], 'app');
  }
  return c;
}

// ---------------------------------------------------------------------------
// Server fixtures.

/** The final request of a server case (README.md: the harness gets these bytes). */
export function serverRequest(c) {
  return {
    method: c.request.method,
    target: c.request.target ?? DEFAULT_TARGET,
    headers: substituteHeaders(c.id, c.request.headers, c.client_members ?? {}),
  };
}

function requiredHeaders(headers) {
  const out = {};
  for (const [name] of headers) {
    if (name === 'content-length' || name === 'date' || name === 'vary' || Object.hasOwn(out, name)) continue;
    out[name] = combined(headers, name);
  }
  return out;
}

/**
 * The member vary of expected.json (DL-063): the Vary members of the reference
 * response as lower-case field names. `required`: the members that every
 * conforming response lists (the members of A that are not Rq-*; MUST);
 * `should`: Rq-Nonce on D (L1, a SHOULD). The checker compares sets, so the
 * order and the case of the members and the number of field lines are free.
 * Written only when one list is not empty.
 */
function expectedVary(kind, headers) {
  const members = [...new Set(varyMembers(headers))];
  const should = kind === 'delta' ? members.filter((m) => m === 'rq-nonce') : [];
  const required = members.filter((m) => !should.includes(m));
  return required.length + should.length === 0 ? {} : { vary: { required, should } };
}

function forbiddenHeaders(kind, status, appHeaders, responseHeaders) {
  const out = new Set(ALWAYS_FORBIDDEN);
  if (kind === 'delta') for (const n of ['etag', 'last-modified', 'expires']) out.add(n); // RQP-01, step 7
  if (kind !== 'app') out.add('last-modified'); // DL-032: F, HEAD of F and 304; D above
  if (kind === 'app') {
    out.add('rq-version'); // RQP-09
    if (combined(appHeaders, 'etag') === null) out.add('etag');
  }
  // DL-030: no Rq-* field of A reaches the client. Rq-Version on a 304 is a MAY
  // of the server step (negotiation section 4, step 9), so it is not forbidden
  // there; headers_optional fixes its value (DL-036, I2).
  for (const [name] of appHeaders) {
    if (!name.startsWith('rq-') || combined(responseHeaders, name) !== null) continue;
    if (name === 'rq-version' && status === 304) continue;
    out.add(name);
  }
  return [...out].sort();
}

/**
 * The member headers_optional of expected.json: fields that a response MAY
 * have, name -> the exact combined value of exactly one field line when the
 * field is present. Only a 304 has it. DL-036 (I2): Rq-Version on a 304
 * (negotiation section 4, step 9) is exactly one line with the value 1.
 */
function optionalHeaders(status) {
  return status === 304 ? { headers_optional: { 'rq-version': '1' } } : {};
}

/** expected.json of a server case, from the reference server step. */
export function expectServer(c) {
  const request = serverRequest(c);
  const page = pageBytes(c.page);
  if (c.client_page !== undefined) pageBytes(c.client_page); // the page must exist
  const app = { status: c.app.status, headers: normalizeHeaders(c.app.headers) };
  const config = membersOf(c.server_config);
  const r = handle(request, app, page, config);
  let body;
  if (r.kind === 'full') {
    const a = analyze(page);
    // H-103: the manifest has the members of the configuration that are not the default.
    body = { kind: 'full', insert_at: documentHeadStart(page), manifest: manifestOf(a, request.target, writtenMembers(config)) };
  } else if (r.kind === 'delta') {
    // H-104, H-107: the envelope is the binding and the operations. `sha256` is
    // the hash of the current content of each changed block; the checker
    // compares it with the hash of the content that the server sent.
    // binding_inputs are U, B, N (H-108, the field Rq-Nonce of H-109) and R' of the binding, for the diagnosis of a
    // server under test; the checker compares only the binding.
    const env = JSON.parse(r.body.toString('utf8'));
    const a = analyze(page);
    const inputs = {
      url: canonicalUrl(request.target),
      base_revision: deltaBase(normalizeHeaders(request.headers)),
      nonce: nonceText(rqNonce(normalizeHeaders(request.headers))),
      next_revision: pageRevision(a.template_revision, a.blocks, writtenMembers(config)),
    };
    if (deltaBinding(inputs.url, inputs.base_revision, inputs.nonce, inputs.next_revision) !== env.binding) {
      throw new CaseError(c.id, 'the binding of the reference server is not the binding of its inputs');
    }
    body = {
      kind: 'delta',
      binding: env.binding,
      binding_inputs: inputs,
      blocks: env.operations.map((op) => ({ id: op.block_id, sha256: sha256b64u(Buffer.from(op.content, 'utf8')) })),
    };
  } else {
    body = { kind: r.kind };
  }
  return {
    files: {
      'expected.json': {
        id: c.id,
        ...(c.level === undefined ? {} : { level: c.level }),
        request,
        ...(Object.keys(config).length > 0 ? { server_config: config } : {}),
        status: r.status,
        headers_required: requiredHeaders(r.headers),
        headers_forbidden: forbiddenHeaders(r.kind, r.status, app.headers, r.headers),
        ...optionalHeaders(r.status),
        ...expectedVary(r.kind, r.headers),
        body,
      },
    },
    reason: r.reason,
  };
}

// ---------------------------------------------------------------------------
// Client fixtures.

/** The serialized origin of a URL (WHATWG URL Standard), or a CaseError. */
function originOf(id, url) {
  try {
    return new URL(url).origin;
  } catch {
    throw new CaseError(id, `${url} is not a URL`);
  }
}

function context(c) {
  const ctx = c.context;
  const navigation = ctx.navigation;
  if (!['first-visit', 'delta', 'fallback'].includes(navigation)) throw new CaseError(c.id, `navigation ${navigation}`);
  const build = ctx.build ?? 'release';
  if (build !== 'release' && build !== 'debug') throw new CaseError(c.id, `build ${build}`);
  // snapshot: null, a page id, or { page, members?, partition? } for a snapshot
  // manifest with policy or limits, or a snapshot in the public partition (H-65).
  const snap = ctx.snapshot ?? null;
  let snapshotPage = snap;
  let members = {};
  let partition = Partition.REQUEST;
  if (snap !== null && typeof snap === 'object') {
    onlyKeys(c.id, snap, ['page', 'members', 'partition'], 'context.snapshot');
    snapshotPage = snap.page;
    members = membersOf(snap.members);
    partition = snap.partition ?? Partition.REQUEST;
    if (partition !== Partition.REQUEST && partition !== Partition.PUBLIC) throw new CaseError(c.id, `context.snapshot.partition ${partition}`);
    // A public snapshot comes from a manifest with mode public (H-61).
    if (partition === Partition.PUBLIC && members.policy?.mode !== 'public') {
      throw new CaseError(c.id, 'a public context snapshot needs the member policy with mode public');
    }
  }
  for (const flag of ['allow_http', 'redirected', 'response_fields_visible', 'public_enabled', 'public_origin']) {
    if (ctx[flag] !== undefined && typeof ctx[flag] !== 'boolean') throw new CaseError(c.id, `context.${flag} must be a boolean`);
  }
  // H-81: the public partition is off unless the case turns it on. A public
  // context snapshot with the setting off is allowed: an SDK can hold such an
  // entry from a time when the setting was on, and must not use it (S4
  // security review L1; the client acts as if it had no snapshot).
  // H-62 (2): credentials true, false or unknown (absent or null; unknown counts as present).
  if (ctx.credentials !== undefined && ctx.credentials !== null && typeof ctx.credentials !== 'boolean') {
    throw new CaseError(c.id, 'context.credentials must be a boolean or null');
  }
  const url = ctx.url ?? DEFAULT_TARGET;
  const finalUrl = ctx.final_url ?? url;
  // H-67: a redirect, also one to the same URL. Default: the final URL differs
  // from the request URL (as strings). false with another final URL is a defect.
  const wasRedirected = ctx.redirected ?? finalUrl !== url;
  if (!wasRedirected && finalUrl !== url) throw new CaseError(c.id, 'context.redirected is false, but final_url differs from url');
  // DL-039: the allowlist is a list of serialized origins. Default: the origin
  // of the request URL. DL-040: a case with a cross-origin final URL lists the
  // allowlist explicitly.
  let allowlist = ctx.allowlist;
  if (allowlist === undefined) {
    const requestOrigin = originOf(c.id, url);
    if (originOf(c.id, finalUrl) !== requestOrigin) {
      throw new CaseError(c.id, 'context.allowlist is required when final_url has another origin than url (DL-040)');
    }
    allowlist = [requestOrigin];
  }
  if (!Array.isArray(allowlist) || !allowlist.every((o) => typeof o === 'string')) {
    throw new CaseError(c.id, 'context.allowlist must be a list of origin strings');
  }
  // H-69: an SDK rejects a configuration with an entry that is not its own serialized origin.
  for (const entry of allowlist) {
    if (!validAllowlistEntry(entry)) throw new CaseError(c.id, `context.allowlist entry ${JSON.stringify(entry)} is not a serialized origin (H-69)`);
  }
  // H-68: the client stores no snapshot for a URL with user information, so no context snapshot can have one.
  if (snapshotPage !== null && hasUserinfo(url)) throw new CaseError(c.id, 'a context snapshot needs a url without user information (H-68)');
  // H-108: the nonce of the request, a fixed value so that the request fields and the binding are fixed.
  const nonce = ctx.nonce === undefined ? DEFAULT_NONCE : ctx.nonce;
  if (!isNonceText(nonce)) throw new CaseError(c.id, 'context.nonce must be base64url without padding of 16 bytes (H-108)');
  return {
    nonce,
    url,
    final_url: finalUrl,
    redirected: wasRedirected,
    build,
    allow_http: ctx.allow_http ?? false,
    allowlist,
    credentials: ctx.credentials ?? null,
    // H-64: the reference client models a native fetch, so it sees every field by default.
    response_fields_visible: ctx.response_fields_visible ?? true,
    // H-81: the app setting of the public partition; off by default.
    public_enabled: ctx.public_enabled ?? false,
    public_origin: ctx.public_origin ?? false,
    navigation,
    snapshotPage,
    snapshot: snapshotPage === null ? null : snapshotOf(pageBytes(snapshotPage), members, url, partition),
  };
}

/** A JSON object text with `member` written first a second time, with `value` (manifest-patch section 3.2, rule 2). */
function withDuplicateMember(text, member, value) {
  return `{${JSON.stringify(member)}:${JSON.stringify(value)},${text.slice(1)}`;
}

/** Body recipe {"full": page, ...}: a full document F, or F with one defect. */
function fullRecipe(id, recipe, ctx) {
  onlyKeys(id, recipe, ['full', 'members', 'manifest_of', 'manifest_type', 'manifest_position', 'manifest_after', 'manifest_count', 'duplicate_member', 'old_manifest'], 'body');
  const page = pageBytes(recipe.full);
  const a = analysisOf(id, recipe.manifest_of ?? recipe.full);
  // The manifest is consistent with the final URL of the response (H-58). The
  // members are written as the case gives them, also default values (a client
  // accepts them; only a server must not write them, H-103).
  let manifest = manifestOf(a, ctx.final_url, membersOf(recipe.members));
  if (recipe.old_manifest === true) {
    // The manifest of rqp/1 before H-103: also template_revision and the block list.
    const { protocol, origin, path, page_revision: pageRev, ...rest } = manifest;
    const blocks = a.blocks.map(({ id: blockId, format, sha256 }) => ({ id: blockId, format, sha256 }));
    manifest = { protocol, origin, path, template_revision: a.template_revision, page_revision: pageRev, blocks, ...rest };
  } else if (recipe.old_manifest !== undefined) throw new CaseError(id, 'old_manifest must be true when present');
  let text = JSON.stringify(manifest);
  if (recipe.duplicate_member === true) text = withDuplicateMember(text, 'protocol', 'rqp/1');
  const type = recipe.manifest_type ?? 'application/rivqen-manifest+json';
  const element = Buffer.from(`<script type="${type}">${text}${MANIFEST_END}`, 'utf8');
  // Positions: the insertion point of the server (default: immediately after
  // the head start tag of the document, H-72, or before the start tag that
  // implies the head, H-79a), before the first "</head>" bytes of the page in any context
  // ("head-text"; the insertion point of rqp/1 before H-72), before "</body>",
  // or immediately after the first copy of the literal text of manifest_after.
  const bodyEnd = page.indexOf('</body>');
  let first;
  if (recipe.manifest_after !== undefined) {
    if (recipe.manifest_position !== undefined) throw new CaseError(id, 'manifest_position and manifest_after together');
    if (typeof recipe.manifest_after !== 'string' || recipe.manifest_after === '') throw new CaseError(id, 'manifest_after must be a text');
    const found = page.indexOf(recipe.manifest_after);
    first = found < 0 ? -1 : found + Buffer.byteLength(recipe.manifest_after, 'utf8');
  } else if (recipe.manifest_position === undefined) first = documentHeadStart(page);
  else if (recipe.manifest_position === 'head-text') first = page.indexOf('</head>');
  else if (recipe.manifest_position === 'body') first = bodyEnd;
  else throw new CaseError(id, `manifest_position ${recipe.manifest_position}`);
  if (first < 0 || bodyEnd < 0) throw new CaseError(id, 'the page has no such position or no </body>');
  const at = [first];
  if (recipe.manifest_count === 2) at.push(bodyEnd);
  else if (recipe.manifest_count !== undefined) throw new CaseError(id, 'manifest_count must be 2');
  const parts = [];
  let from = 0;
  for (const p of at) {
    parts.push(page.subarray(from, p), element);
    from = p;
  }
  parts.push(page.subarray(from));
  return Buffer.concat(parts);
}

/**
 * Body recipe {"delta": {...}}: a delta envelope (H-104, H-107) that a server
 * built from the page `from`, or one with one defect. Without `ops` the
 * operations are the blocks of `to` that differ from `from`; with
 * `all_blocks` they are all blocks of `to` (a request without `b`, H-7). The
 * binding (H-107, H-108) has U = the canonical URL of `binding_url` (default:
 * the final URL of the context), N = `nonce` (default: the nonce of the
 * context, the request that the server answered; another value is a delta
 * that the server built for another request), B = the page revision of `from` (with the members
 * of the snapshot when `from` is the snapshot page), and R' = the page
 * revision of the blocks of `from` with the contents of the operations and the
 * template revision of `from`, or the page revision of the page
 * `next_revision`. `old_envelope` writes the envelope of rqp/1 before H-104
 * (type, protocol, base_revision, next_revision, template_revision; op,
 * format and sha256 in each operation); `next_revision_envelope` writes the
 * envelope of H-104 (next_revision = R' in place of the binding);
 * `operation_sha256` adds only the hash of each operation (H-10, replaced by
 * H-104); `duplicate_member` writes the binding a second time, first, with the
 * binding of an empty change (R' = B; the value a first-wins parser would read).
 */
function deltaRecipe(id, recipe, ctx) {
  const d = recipe.delta;
  onlyKeys(id, recipe, ['delta'], 'body');
  onlyKeys(
    id,
    d,
    ['from', 'to', 'ops', 'all_blocks', 'binding_url', 'nonce', 'next_revision', 'next_members', 'size', 'duplicate_member', 'old_envelope', 'next_revision_envelope', 'operation_sha256'],
    'body.delta',
  );
  for (const flag of ['all_blocks', 'duplicate_member', 'old_envelope', 'next_revision_envelope', 'operation_sha256']) {
    if (d[flag] !== undefined && d[flag] !== true) throw new CaseError(id, `body.delta.${flag} must be true when present`);
  }
  if (d.old_envelope === true && d.next_revision_envelope === true) throw new CaseError(id, 'old_envelope and next_revision_envelope together');
  let bindingUrl;
  try {
    bindingUrl = canonicalUrl(d.binding_url ?? ctx.final_url);
  } catch {
    throw new CaseError(id, `body.delta.binding_url ${d.binding_url} is not a URL`);
  }
  // H-108: N of the binding is the nonce of the request that the server
  // answered; default: the nonce of the context (the request of this case).
  const nonce = d.nonce ?? ctx.nonce;
  if (!isNonceText(nonce)) throw new CaseError(id, 'body.delta.nonce must be base64url without padding of 16 bytes (H-108)');
  const from = analysisOf(id, d.from);
  // The policy and limits of the snapshot manifest (H-55): next_revision uses
  // next_members when they are given, else the members of the snapshot when the
  // delta is from the snapshot page.
  const baseMembers = ctx.snapshot !== null && ctx.snapshotPage === d.from ? ctx.snapshot.members : {};
  const nextMembers = d.next_members === undefined ? baseMembers : membersOf(d.next_members);
  // Only the envelope before H-104 needs the block of an operation (its format).
  const blockOf = (blockId) => {
    const b = from.blocks.find((x) => x.id === blockId);
    if (b === undefined) throw new CaseError(id, `block ${blockId} is not in ${d.from}`);
    return b;
  };
  let ops;
  if (d.ops !== undefined) {
    if (d.all_blocks !== undefined) throw new CaseError(id, 'body.delta.ops and all_blocks together');
    if (!Array.isArray(d.ops)) throw new CaseError(id, 'body.delta.ops must be a list');
    ops = d.ops.map((op) => {
      onlyKeys(id, op, ['block_id', 'content'], 'body.delta.ops[]');
      return { block_id: op.block_id, content: op.content };
    });
  } else {
    const toPage = pageBytes(d.to);
    const to = analysisOf(id, d.to);
    // `to` is a later version of the same page: the same block ids in the same order.
    if (to.blocks.map((b) => b.id).join('\n') !== from.blocks.map((b) => b.id).join('\n')) {
      throw new CaseError(id, `${d.to} does not have the blocks of ${d.from}`);
    }
    ops = to.blocks
      .filter((b, i) => d.all_blocks === true || b.sha256 !== from.blocks[i]?.sha256)
      .map((b) => ({ block_id: b.id, content: toPage.toString('utf8', b.start, b.end) }));
  }
  const hashOf = (content) => sha256b64u(Buffer.from(content, 'utf8'));
  const build = () => {
    // A consistent next_revision: the blocks of `from` with the hashes of the
    // contents. An operation for a block that `from` does not have is a
    // defect of the envelope (step 3); it does not change next_revision.
    const contentHash = new Map(ops.map((op) => [op.block_id, hashOf(op.content)]));
    const next = from.blocks.map((b) => ({ id: b.id, format: b.format, sha256: contentHash.get(b.id) ?? b.sha256 }));
    const nextRevision = d.next_revision === undefined ? pageRevision(from.template_revision, next, nextMembers) : analysisOf(id, d.next_revision).page_revision;
    const baseRevision = pageRevision(from.template_revision, from.blocks, baseMembers);
    let env;
    if (d.old_envelope === true) {
      env = {
        type: 'rivqen.patch',
        protocol: 'rqp/1',
        base_revision: baseRevision,
        next_revision: nextRevision,
        template_revision: from.template_revision,
        operations: ops.map((op) => ({
          op: 'replace_block',
          block_id: op.block_id,
          format: blockOf(op.block_id).format,
          content: op.content,
          sha256: hashOf(op.content),
        })),
      };
    } else {
      env = {
        ...(d.next_revision_envelope === true
          ? { next_revision: nextRevision }
          : { binding: deltaBinding(bindingUrl, baseRevision, nonce, nextRevision) }),
        operations: ops.map((op) => ({
          block_id: op.block_id,
          content: op.content,
          ...(d.operation_sha256 === true ? { sha256: hashOf(op.content) } : {}),
        })),
      };
    }
    const text = JSON.stringify(env);
    if (d.duplicate_member !== true) return Buffer.from(text, 'utf8');
    if (env.binding === undefined) throw new CaseError(id, 'duplicate_member needs the envelope with the binding');
    return Buffer.from(withDuplicateMember(text, 'binding', deltaBinding(bindingUrl, baseRevision, nonce, baseRevision)), 'utf8');
  };
  if (d.size === undefined) return build();
  // Fill the content of the only operation with "x" up to the exact envelope size.
  if (ops.length !== 1 || ops[0].content !== '') {
    throw new CaseError(id, 'size needs exactly one operation with empty content');
  }
  const fill = d.size - build().length;
  if (!(fill >= 0)) throw new CaseError(id, `size ${d.size} is smaller than the envelope without content`);
  ops[0].content = 'x'.repeat(fill);
  const out = build();
  if (out.length !== d.size) throw new CaseError(id, `size ${d.size} not reached`);
  return out;
}

/** The exact response body of a client case. */
export function responseBody(c, ctx) {
  const recipe = c.response.body;
  if (recipe === null || recipe === undefined) return null;
  if (recipe.page !== undefined) {
    onlyKeys(c.id, recipe, ['page'], 'body');
    return pageBytes(recipe.page);
  }
  if (recipe.full !== undefined) return fullRecipe(c.id, recipe, ctx);
  if (recipe.delta !== undefined) return deltaRecipe(c.id, recipe, ctx);
  throw new CaseError(c.id, 'body must be null, {"page"}, {"full"} or {"delta"}');
}

/** The snapshot of a client context as the harness gives it to a client under test. */
function describeSnapshot(page, snap) {
  if (snap === null) return null;
  const a = snap.analysis;
  return {
    page,
    template_revision: a.template_revision,
    page_revision: snap.page_revision,
    blocks: a.blocks.map(({ id, format, sha256 }) => ({ id, format, sha256 })),
    ...snap.members,
    partition: snap.partition,
  };
}

/** expected.json (and response.body) of a client case, from the reference client step. */
export function expectClient(c) {
  const ctx = context(c);
  const body = responseBody(c, ctx);
  const headers = substituteHeaders(c.id, c.response.headers);
  const res = handleResponse(ctx, { status: c.response.status, headers, body });
  const files = {
    'expected.json': {
      id: c.id,
      kind: 'client',
      context: {
        url: ctx.url,
        final_url: ctx.final_url,
        redirected: ctx.redirected,
        build: ctx.build,
        allow_http: ctx.allow_http,
        allowlist: ctx.allowlist,
        credentials: ctx.credentials,
        response_fields_visible: ctx.response_fields_visible,
        public_enabled: ctx.public_enabled,
        public_origin: ctx.public_origin,
        navigation: ctx.navigation,
        nonce: ctx.nonce,
        snapshot: describeSnapshot(ctx.snapshotPage, ctx.snapshot),
      },
      request: { method: 'GET', headers: requestHeaders(ctx) },
      response: {
        status: c.response.status,
        headers,
        body: body === null ? null : RESPONSE_BODY,
        body_bytes: body === null ? 0 : body.length,
        body_sha256: body === null ? null : sha256b64u(body),
      },
      result: res,
    },
  };
  if (body !== null) files[RESPONSE_BODY] = body;
  return { files };
}

/** Expected files of one fixture: { files: { name: object | Buffer } }. */
export function expectFixture(dir, name) {
  const c = loadCase(dir, name);
  try {
    return c.kind === 'client' ? expectClient(c) : expectServer(c);
  } catch (error) {
    if (error instanceof UnsupportedInput || error instanceof UnknownPage) throw new CaseError(name, error.message);
    throw error;
  }
}

/** JSON text with two-space indent; an array of strings or numbers stays on one line (header lines). */
export function formatJson(value, indent = '') {
  if (Array.isArray(value)) {
    if (value.every((v) => v === null || typeof v !== 'object')) return JSON.stringify(value);
    const inner = indent + '  ';
    return `[\n${value.map((v) => inner + formatJson(v, inner)).join(',\n')}\n${indent}]`;
  }
  if (value !== null && typeof value === 'object') {
    const entries = Object.entries(value);
    if (entries.length === 0) return '{}';
    const inner = indent + '  ';
    return `{\n${entries.map(([k, v]) => `${inner}${JSON.stringify(k)}: ${formatJson(v, inner)}`).join(',\n')}\n${indent}}`;
  }
  return JSON.stringify(value);
}

/** The bytes of an expected file as gen.mjs writes it. */
export function fileBytes(value) {
  if (Buffer.isBuffer(value)) return value;
  return Buffer.from(`${formatJson(value)}\n`, 'utf8');
}
