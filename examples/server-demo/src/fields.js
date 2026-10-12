// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors
//
// Request fields that the response selection reads (negotiation section 3):
// Rq-Version, Rq-Capabilities, Rq-Have, If-None-Match and Accept.
// Header lines are [name, value] pairs with lower-case names. A field that does not
// parse, or that has another shape than the rules allow, is absent (null or an empty set).

import { parseDictionary, parseItem, parseList } from './sf.js';

export const MAX_RQ_HAVE_BYTES = 4096; // negotiation section 3.4
export const MAX_RQ_CAPABILITIES_BYTES = 1024; // negotiation section 3.3
export const DELTA_TYPE = 'application/vnd.rivqen.delta+json';

/** Lower-case the names of header lines. */
export function normalizeHeaders(headers) {
  return headers.map(([name, value]) => [name.toLowerCase(), value]);
}

/** Header lines of a Node.js request (`rawHeaders`): [lower-case name, value], one line per field line, in order. */
export function requestLines(rawHeaders) {
  const lines = [];
  for (let i = 0; i + 1 < rawHeaders.length; i += 2) lines.push([rawHeaders[i].toLowerCase(), rawHeaders[i + 1]]);
  return lines;
}

/** All lines of a field joined with ", " (RFC 9651 section 4.2; DL-029), or null. */
export function fieldValue(headers, name) {
  const values = headers.filter(([n]) => n === name).map(([, value]) => value);
  return values.length === 0 ? null : values.join(', ');
}

export function hasField(headers, name) {
  return headers.some(([n]) => n === name);
}

/** Header lines without any Rq-* field. The RQP server step owns these fields (DL-030). */
export function withoutRqFields(headers) {
  return headers.filter(([n]) => !n.startsWith('rq-'));
}

/** Header lines without the named fields. */
export function without(headers, ...names) {
  return headers.filter(([n]) => !names.includes(n));
}

/** Section 3.2: the request version, or null when absent or not an Integer. */
export function requestVersion(headers) {
  const value = fieldValue(headers, 'rq-version');
  if (value === null) return null;
  const item = parseItem(value);
  return item !== null && item.type === 'integer' ? item.value : null;
}

/** Section 3.3: the set of capability tokens. Any other shape gives an empty set. */
export function capabilities(headers) {
  const value = fieldValue(headers, 'rq-capabilities');
  if (value === null || Buffer.byteLength(value) > MAX_RQ_CAPABILITIES_BYTES) return new Set();
  const list = parseList(value);
  if (list === null || list.length !== 1 || list[0].kind !== 'inner') return new Set();
  if (!list[0].items.every((item) => item.type === 'token')) return new Set();
  return new Set(list[0].items.map((item) => item.value));
}

/**
 * Section 3.4, steps 1 to 3: { t, b } or null when Rq-Have is absent. `t` is the template
 * revision token. `b` is the decoded Buffer, or null when the member is missing or is not
 * a Byte Sequence. The length of `b` is checked by the caller (it needs the block count).
 */
export function rqHave(headers) {
  const value = fieldValue(headers, 'rq-have');
  if (value === null || Buffer.byteLength(value) > MAX_RQ_HAVE_BYTES) return null;
  const dict = parseDictionary(value);
  if (dict === null) return null;
  const t = dict.get('t');
  if (t === undefined || t.kind !== 'item' || t.type !== 'token') return null;
  const b = dict.get('b');
  return { t: t.value, b: b !== undefined && b.kind === 'item' && b.type === 'bytes' ? b.value : null };
}

const ETAGC = /^[\x21\x23-\x7e\x80-\xff]*$/;

/**
 * RFC 9110 section 13.1.2: the value of If-None-Match as { star, tags: [{ weak, opaque }] },
 * or null when it does not parse. `opaque` is the text between the quotes.
 */
export function parseIfNoneMatch(value) {
  const text = value.trim();
  if (text === '*') return { star: true, tags: [] };
  const tags = [];
  let i = 0;
  while (text[i] === ',' || text[i] === ' ' || text[i] === '\t') i++; // empty list elements are allowed
  while (i < text.length) {
    const weak = text.startsWith('W/', i);
    if (weak) i += 2;
    if (text[i] !== '"') return null;
    const close = text.indexOf('"', i + 1);
    if (close < 0) return null;
    const opaque = text.slice(i + 1, close);
    if (!ETAGC.test(opaque)) return null;
    tags.push({ weak, opaque });
    i = close + 1;
    while (text[i] === ' ' || text[i] === '\t') i++;
    if (i >= text.length) break;
    if (text[i] !== ',') return null;
    while (text[i] === ',' || text[i] === ' ' || text[i] === '\t') i++; // empty list elements are allowed
  }
  return tags.length === 0 ? null : { star: false, tags };
}

/** Step 4: true when the If-None-Match condition is false for the ETag `etag` (weak comparison). */
export function noneMatchFails(headers, etag) {
  const value = fieldValue(headers, 'if-none-match');
  if (value === null) return false;
  const parsed = parseIfNoneMatch(value);
  if (parsed === null) return false;
  if (parsed.star) return true;
  const opaque = etag.replace(/^W\//, '').slice(1, -1);
  return parsed.tags.some((tag) => tag.opaque === opaque);
}

const BASE_REVISION = /^r1\.[A-Za-z0-9_-]{43}$/;

/** Section 3.5: the delta base B, or null. Only one entity-tag of the form "r1.…" gives a base. */
export function deltaBase(headers) {
  const value = fieldValue(headers, 'if-none-match');
  if (value === null) return null;
  const parsed = parseIfNoneMatch(value);
  if (parsed === null || parsed.star || parsed.tags.length !== 1) return null;
  return BASE_REVISION.test(parsed.tags[0].opaque) ? parsed.tags[0].opaque : null;
}

/** Split a header value at the commas that are not inside a quoted string. */
function splitList(value) {
  const parts = [];
  let current = '';
  let quoted = false;
  for (let i = 0; i < value.length; i++) {
    const c = value[i];
    if (quoted && c === '\\') {
      current += c + (value[i + 1] ?? '');
      i++;
    } else if (c === '"') {
      quoted = !quoted;
      current += c;
    } else if (c === ',' && !quoted) {
      parts.push(current);
      current = '';
    } else {
      current += c;
    }
  }
  parts.push(current);
  return parts;
}

/** Section 3.6: true when Accept lists the delta media type itself, with a weight above 0. */
export function acceptsDelta(headers) {
  const value = fieldValue(headers, 'accept');
  if (value === null) return false;
  return splitList(value).some((range) => {
    const [media, ...params] = range.split(';').map((s) => s.trim());
    if (media.toLowerCase() !== DELTA_TYPE) return false;
    const q = params.findLast((param) => /^q\s*=/i.test(param)); // the last q parameter wins
    if (q === undefined) return true;
    const weight = q.replace(/^q\s*=\s*/i, '');
    return /^(0(\.[0-9]{0,3})?|1(\.0{0,3})?)$/.test(weight) && Number(weight) > 0;
  });
}

/** The media type of a Content-Type value, in lower case without parameters, or null. */
export function mediaType(value) {
  if (value === null) return null;
  return value.split(';')[0].trim().toLowerCase();
}

/** Labels that the WHATWG Encoding Standard maps to the UTF-8 encoding (lower case). */
const UTF8_LABELS = new Set(['unicode-1-1-utf-8', 'unicode11utf8', 'unicode20utf8', 'utf-8', 'utf8', 'x-unicode20utf8']);

/**
 * The value of the first charset parameter of a Content-Type value, or null when there is
 * none. Parameters are split at ";" outside double quotes. The name is ASCII case-insensitive.
 * A quoted value loses its surrounding quotes.
 */
export function charsetParameter(value) {
  if (value === null) return null;
  const parts = [];
  let current = '';
  let quoted = false;
  for (let i = 0; i < value.length; i++) {
    const c = value[i];
    if (c === '"') quoted = !quoted;
    if (c === '\\' && quoted && i + 1 < value.length) {
      current += c + value[++i];
      continue;
    }
    if (c === ';' && !quoted) {
      parts.push(current);
      current = '';
    } else current += c;
  }
  parts.push(current);
  for (const part of parts.slice(1)) {
    const eq = part.indexOf('=');
    if (eq < 0 || part.slice(0, eq).trim().toLowerCase() !== 'charset') continue;
    const raw = part.slice(eq + 1).trim();
    return raw.length >= 2 && raw.startsWith('"') && raw.endsWith('"') ? raw.slice(1, -1) : raw;
  }
  return null;
}

/** True when a charset value is a label of the UTF-8 encoding (H-62). */
export const isUtf8Label = (label) => UTF8_LABELS.has(label.toLowerCase());
