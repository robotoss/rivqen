// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// Fixture model of fixtures/rqp/protocol (README.md there): reading case.json,
// the placeholders, the response recipes of client cases, and the expected
// files that gen.mjs writes. All expected values come from the reference steps
// (src/server.mjs, src/client.mjs).

import { readFileSync, readdirSync, statSync } from 'node:fs';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { MANIFEST_END, UnsupportedInput, membersOf, pageRevision, sha256b64u } from './common.mjs';
import { handle, manifestOf } from './server.mjs';
import { Partition, handleResponse, hasUserinfo, havePrefixes, requestHeaders, snapshotOf, validAllowlistEntry } from './client.mjs';
import { combined, normalizeHeaders } from './http.mjs';
import { analyze, documentHeadStart } from './markup.mjs';
import { UnknownPage, pageBytes } from './pages.mjs';

export const FIXTURES_DIR = fileURLToPath(new URL('../../../fixtures/rqp/protocol/', import.meta.url));
export const DEFAULT_TARGET = 'https://shop.example.com/catalog';
export const RESPONSE_BODY = 'response.body';
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
      ['url', 'final_url', 'redirected', 'build', 'allow_http', 'allowlist', 'navigation', 'snapshot', 'credentials', 'response_fields_visible', 'public_enabled', 'public_origin'],
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
    if (name === 'content-length' || name === 'date' || Object.hasOwn(out, name)) continue;
    out[name] = combined(headers, name);
  }
  return out;
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
    body = { kind: 'full', insert_at: documentHeadStart(page), manifest: manifestOf(a, request.target, config) };
  } else if (r.kind === 'delta') {
    const env = JSON.parse(r.body.toString('utf8'));
    body = {
      kind: 'delta',
      base_revision: env.base_revision,
      next_revision: env.next_revision,
      template_revision: env.template_revision,
      blocks: env.operations.map((op) => ({ id: op.block_id, format: op.format, sha256: op.sha256 })),
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
  // H-81: the public partition is off unless the case turns it on, so a public
  // context snapshot needs public_enabled (an SDK with it off never has one).
  if (partition === Partition.PUBLIC && ctx.public_enabled !== true) {
    throw new CaseError(c.id, 'a public context snapshot needs context.public_enabled true (H-81)');
  }
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
  return {
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

const DUPLICATE_PREFIX = '{"protocol":"rqp/1",';

function withDuplicateMember(text) {
  return DUPLICATE_PREFIX + text.slice(1);
}

/** Body recipe {"full": page, ...}: a full document F, or F with one defect. */
function fullRecipe(id, recipe, ctx) {
  onlyKeys(id, recipe, ['full', 'members', 'manifest_of', 'manifest_type', 'manifest_position', 'manifest_after', 'manifest_count', 'duplicate_member'], 'body');
  const page = pageBytes(recipe.full);
  const a = analysisOf(id, recipe.manifest_of ?? recipe.full);
  // The manifest is consistent with the final URL of the response (H-58).
  let text = JSON.stringify(manifestOf(a, ctx.final_url, membersOf(recipe.members)));
  if (recipe.duplicate_member === true) text = withDuplicateMember(text);
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

/** Body recipe {"delta": {...}}: a delta envelope from a snapshot page, or one with one defect. */
function deltaRecipe(id, recipe, ctx) {
  const d = recipe.delta;
  onlyKeys(id, recipe, ['delta'], 'body');
  onlyKeys(id, d, ['from', 'to', 'ops', 'base_revision', 'next_revision', 'next_members', 'template_revision', 'size', 'duplicate_member'], 'body.delta');
  const from = analysisOf(id, d.from);
  // The policy and limits of the snapshot manifest (H-55): base_revision is the
  // snapshot page revision; next_revision uses next_members when they are given.
  const baseMembers = ctx.snapshot !== null && ctx.snapshotPage === d.from ? ctx.snapshot.members : {};
  const nextMembers = d.next_members === undefined ? baseMembers : membersOf(d.next_members);
  const blockOf = (blockId) => {
    const b = from.blocks.find((x) => x.id === blockId);
    if (b === undefined) throw new CaseError(id, `block ${blockId} is not in ${d.from}`);
    return b;
  };
  let ops;
  if (d.ops !== undefined) {
    ops = d.ops.map((op) => {
      onlyKeys(id, op, ['block_id', 'content', 'sha256'], 'body.delta.ops[]');
      return { block_id: op.block_id, content: op.content, sha256: op.sha256 };
    });
  } else {
    const toPage = pageBytes(d.to);
    const to = analysisOf(id, d.to);
    ops = to.blocks
      .filter((b, i) => b.sha256 !== from.blocks[i]?.sha256)
      .map((b) => ({ block_id: b.id, content: toPage.toString('utf8', b.start, b.end) }));
  }
  const build = () => {
    const operations = ops.map((op) => ({
      op: 'replace_block',
      block_id: op.block_id,
      format: blockOf(op.block_id).format,
      content: op.content,
      sha256: op.sha256 ?? sha256b64u(Buffer.from(op.content, 'utf8')),
    }));
    // A consistent next_revision: the snapshot blocks with the hashes of the real contents.
    const contentHash = new Map(ops.map((op) => [op.block_id, sha256b64u(Buffer.from(op.content, 'utf8'))]));
    const next = from.blocks.map((b) => ({ id: b.id, format: b.format, sha256: contentHash.get(b.id) ?? b.sha256 }));
    const env = {
      type: 'rivqen.patch',
      protocol: 'rqp/1',
      base_revision:
        d.base_revision === undefined
          ? pageRevision(from.template_revision, from.blocks, baseMembers)
          : analysisOf(id, d.base_revision).page_revision,
      next_revision:
        d.next_revision === undefined
          ? pageRevision(from.template_revision, next, nextMembers)
          : analysisOf(id, d.next_revision).page_revision,
      template_revision: d.template_revision === undefined ? from.template_revision : analysisOf(id, d.template_revision).template_revision,
      operations,
    };
    const text = JSON.stringify(env);
    return Buffer.from(d.duplicate_member === true ? withDuplicateMember(text) : text, 'utf8');
  };
  if (d.size === undefined) return build();
  // Fill the content of the only operation with "x" up to the exact envelope size.
  if (ops.length !== 1 || ops[0].content !== '' || ops[0].sha256 !== undefined) {
    throw new CaseError(id, 'size needs exactly one operation with empty content and no sha256');
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
