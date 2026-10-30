// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// Model of the legacy server side: legacy-canonical (docs/engineering/protocol/legacy-wire.md
// section 10, rules C-01 to C-14) and the three upstream profiles legacy-java,
// legacy-node and legacy-php (docs/engineering/protocol/legacy-divergences.md D-01 to D-13).
// The profile results equal the stored upstream traces (gen.mjs checks it).
//
// The model is pure: no I/O, no clock, no shared state between calls (U-01 is not
// reproduced). It never throws for a request, an application response or page bytes
// of any shape; malformed input gives pass-through. An unknown profile name is a
// configuration error and throws ModelError. No profile reproduces U-01 or U-11 (DL-069).

import { createHash } from 'node:crypto';
import { isUtf8 } from 'node:buffer';
import { asciiLower, first, hasDirective, normalizeFields, parseContentType, single, trimOws, valuesOf, without } from './fields.mjs';
import { rebuild, split } from './split.mjs';

export { rebuild, split };

export const PROFILES = Object.freeze(['legacy-canonical', 'legacy-java', 'legacy-node', 'legacy-php']);

/** A configuration error (unknown profile). */
export class ModelError extends Error {
  constructor(code, message) {
    super(message);
    this.name = 'ModelError';
    this.code = code;
  }
}

/** Lower-case hex SHA-1 of bytes or text (UTF-8). */
export function sha1hex(data) {
  return createHash('sha1').update(data).digest('hex');
}

const FIELDS_OF_THE_MODEL = new Set(['etag', 'template-tag', 'template-change', 'cache-offline', 'sonic-etag-key', 'content-length']);
const OFFLINE_VALUES = new Set(['true', 'false', 'store', 'http']);

/** The application response with only well-formed members. */
function normalizeApp(app) {
  const status = app !== null && typeof app === 'object' && Number.isInteger(app.status) ? app.status : 200;
  const headers = app !== null && typeof app === 'object' ? normalizeFields(app.headers) : [];
  return { status, headers };
}

function normalizeRequest(request) {
  const object = request !== null && typeof request === 'object' ? request : {};
  return {
    method: typeof object.method === 'string' ? object.method : 'GET',
    headers: normalizeFields(object.headers),
  };
}

function normalizeBytes(bytes) {
  if (bytes instanceof Uint8Array) return Buffer.from(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  return Buffer.alloc(0);
}

/** C-01 and C-03: may this application response get legacy processing? Returns the page text or null. */
function eligibleText(method, app, bytes) {
  if (method !== 'GET' || app.status !== 200) return null;
  const types = valuesOf(app.headers, 'content-type');
  if (types.length !== 1) return null;
  const { type, params } = parseContentType(types[0]);
  if (type !== 'text/html') return null;
  const charset = params.get('charset');
  if (charset !== undefined && asciiLower(charset) !== 'utf-8') return null;
  if (!isUtf8(bytes)) return null;
  return bytes.toString('utf8');
}

/** C-10: the cache-offline value of a 200 response. */
function offlineOf(app) {
  const values = valuesOf(app.headers, 'cache-offline');
  return values.length === 1 && OFFLINE_VALUES.has(values[0]) ? values[0] : 'true';
}

/** C-05: the If-None-Match value without one leading W/ and one pair of quotes. */
function bareEtag(value) {
  let v = value;
  if (v.startsWith('W/')) v = v.slice(2);
  if (v.length >= 2 && v.startsWith('"') && v.endsWith('"')) v = v.slice(1, -1);
  return v;
}

function acceptsGzipOrDeflate(fields) {
  for (const value of valuesOf(fields, 'accept-encoding')) {
    for (const item of value.split(',')) {
      const coding = asciiLower(trimOws(item.split(';')[0]));
      if (coding === 'gzip' || coding === 'deflate') return true;
    }
  }
  return false;
}

/** Name and value comparison of PHP: the field name is compared as sent. */
function firstExactName(fields, name) {
  for (const [n, v] of fields) if (n === name) return trimOws(v);
  return null;
}

/**
 * Decide what the profile does with a page that was split.
 * Returns { kind: 'pass' } or { kind: 'not-modified' | 'data' | 'full', adds: [[name, value], ...] }.
 */
function plan(profile, request, app, ctx) {
  const { etag, tag, blocks } = ctx;
  const fields = request.headers;
  const noStore = hasDirective(app.headers, 'no-store');
  const dataOrFull = (given, extra) => {
    const kind = given === tag ? 'data' : 'full';
    return {
      kind,
      adds: [['etag', etag], ['template-tag', tag], ['template-change', kind === 'data' ? 'false' : 'true'], ...extra],
    };
  };

  if (profile === 'legacy-canonical') {
    if (single(fields, 'accept-diff') !== 'true') return { kind: 'pass' };
    const cache = noStore ? [] : [['cache-control', 'no-cache, private']];
    const matches = valuesOf(fields, 'if-none-match');
    if (matches.length === 1 && asciiLower(bareEtag(matches[0])) === etag) {
      return { kind: 'not-modified', adds: [['cache-offline', 'store'], ...cache] };
    }
    const tags = valuesOf(fields, 'template-tag');
    return dataOrFull(tags.length === 1 ? tags[0] : null, [['cache-offline', offlineOf(app)], ...cache]);
  }

  if (profile === 'legacy-java') {
    const legacy = first(fields, 'accept-diff') === 'true';
    const cache = legacy && !noStore ? [['cache-control', 'no-cache']] : [];
    const inm = first(fields, 'if-none-match');
    if (legacy && inm !== null && asciiLower(inm) === etag) {
      return { kind: 'not-modified', adds: [...cache, ['cache-offline', 'true'], ['cache-offline', 'store']] };
    }
    return dataOrFull(first(fields, 'template-tag'), [...(legacy ? [['cache-offline', offlineOf(app)]] : []), ...cache]);
  }

  if (profile === 'legacy-node') {
    const mode = first(fields, 'accept-diff');
    if (mode === null || mode === '' || !acceptsGzipOrDeflate(fields)) return { kind: 'pass' };
    if (first(fields, 'if-none-match') === etag) return { kind: 'not-modified', adds: [['cache-offline', 'store']] };
    const given = blocks === 0 ? null : first(fields, 'template-tag');
    return dataOrFull(given, [['cache-offline', offlineOf(app)]]);
  }

  // legacy-php
  if (firstExactName(fields, 'accept-diff') !== 'true') return { kind: 'pass' };
  const cache = noStore ? [] : [['cache-control', 'no-cache']];
  if (first(fields, 'if-none-match') === etag) return { kind: 'not-modified', adds: [['cache-offline', 'store'], ...cache] };
  return dataOrFull(first(fields, 'template-tag'), [['cache-offline', offlineOf(app)], ...cache]);
}

function jsonText(profile, value) {
  const text = JSON.stringify(value);
  if (profile === 'legacy-java') {
    // D-08: the Gson HTML-safe escaping
    return text.replace(/[<>&='\u2028\u2029]/g, (c) => `\\u${c.charCodeAt(0).toString(16).padStart(4, '0')}`);
  }
  if (profile === 'legacy-php') {
    // D-08: slash and non-ASCII are escaped
    return text.replace(/\//g, '\\/').replace(/[^\u0000-\u007f]/g, (c) => `\\u${c.charCodeAt(0).toString(16).padStart(4, '0')}`);
  }
  return text;
}

/** The UTF-8 JSON body of a data response (legacy-wire.md section 5.4). */
function dataBody(profile, ctx) {
  const members = [];
  for (const [key, value] of ctx.data) members.push(`${jsonText(profile, key)}:${jsonText(profile, value)}`);
  const text = `{"data":{${members.join(',')}},"template-tag":${jsonText(profile, ctx.tag)},"html-sha1":${jsonText(profile, ctx.etag)},"diff":""}`;
  return Buffer.from(text, 'utf8');
}

function passThrough(request, app, bytes) {
  return { status: app.status, headers: app.headers.map(([n, v]) => [n, v]), body: request.method === 'HEAD' ? null : Buffer.from(bytes) };
}

/**
 * Run the model and also return the page analysis.
 * Returns { response: { status, headers, body }, page, kind } where kind is 'pass', 'not-modified',
 * 'data' or 'full', and page is null when the
 * application response gets no split (C-01, C-03, or a failed split), else
 * { template, data, tag, etag, blocks }.
 */
export function analyze(profile, request, app, pageBytes) {
  if (!PROFILES.includes(profile)) throw new ModelError('LEGACY_UNKNOWN_PROFILE', `unknown legacy profile: ${String(profile)}`);
  const req = normalizeRequest(request);
  const a = normalizeApp(app);
  const bytes = normalizeBytes(pageBytes);
  const pass = passThrough(req, a, bytes);
  const text = eligibleText(req.method, a, bytes);
  if (text === null) return { response: pass, page: null, kind: 'pass' };
  const parts = split(profile, text);
  if (!parts.ok) return { response: pass, page: null, kind: 'pass' };
  const page = {
    template: parts.template,
    data: parts.data,
    blocks: parts.blocks,
    tag: sha1hex(parts.template),
    etag: sha1hex(bytes),
  };
  const decision = plan(profile, req, a, page);
  if (decision.kind === 'pass') return { response: pass, page, kind: 'pass' };
  const removed = new Set([...FIELDS_OF_THE_MODEL, ...decision.adds.map(([n]) => n)]);
  const headers = [...without(a.headers, removed), ...decision.adds];
  if (decision.kind === 'not-modified') return { response: { status: 304, headers, body: null }, page, kind: decision.kind };
  const body = decision.kind === 'data' ? dataBody(profile, page) : Buffer.from(bytes);
  return { response: { status: 200, headers, body }, page, kind: decision.kind };
}

/**
 * The response of a server in this profile.
 *
 * @param {string} profile one of PROFILES
 * @param {{ method: string, headers: Array<[string, string]> }} request
 * @param {{ status: number, headers: Array<[string, string]> }} app application response
 * @param {Uint8Array} pageBytes application body
 * @returns {{ status: number, headers: Array<[string, string]>, body: Uint8Array | null }}
 */
export function respond(profile, request, app, pageBytes) {
  return analyze(profile, request, app, pageBytes).response;
}
