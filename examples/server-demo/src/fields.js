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

const lowerAscii = (text) => text.replace(/[A-Z]/g, (c) => String.fromCharCode(c.charCodeAt(0) + 32));
/** HTTP white space (MIME Sniffing): tab, line feed, carriage return, space. */
const isHttpSpace = (c) => c === '\t' || c === '\n' || c === '\r' || c === ' ';
/** Code points that a parameter value may hold (MIME Sniffing "HTTP quoted-string token code point"). */
const VALUE_CODE_POINTS = /^[\t -~\u0080-ÿ]*$/;

/**
 * The value of the first valid charset parameter of one media type (H-62, item 2), or null.
 * Parameter steps of "parse a MIME type" (MIME Sniffing 4.4): split at ";", skip white space,
 * read the name up to ";" or "=", read a quoted or an unquoted value. A quoted value loses its
 * quotes and backslash escapes. An unquoted value loses its trailing white space and an empty
 * one does not count. The name must equal "charset" after ASCII lower-casing; it is not trimmed.
 */
export function charsetParameter(mime) {
  let at = mime.indexOf(';');
  if (at < 0) return null;
  // Invariant: at the top of the loop, `at` is on a ";" that starts a parameter.
  while (at < mime.length) {
    at++;
    while (at < mime.length && isHttpSpace(mime[at])) at++;
    const nameStart = at;
    while (at < mime.length && mime[at] !== ';' && mime[at] !== '=') at++;
    const name = lowerAscii(mime.slice(nameStart, at));
    if (mime[at] === ';') continue; // a name without a value
    at++; // past "="
    if (at >= mime.length) break;
    let value = '';
    if (mime[at] === '"') {
      at++;
      while (at < mime.length && mime[at] !== '"') {
        if (mime[at] === '\\') {
          at++;
          value += at < mime.length ? mime[at] : '\\';
        } else value += mime[at];
        at++;
      }
      while (at < mime.length && mime[at] !== ';') at++;
    } else {
      const start = at;
      while (at < mime.length && mime[at] !== ';') at++;
      let end = at;
      while (end > start && isHttpSpace(mime[end - 1])) end--;
      value = mime.slice(start, end);
      if (value === '') continue;
    }
    if (name === 'charset' && VALUE_CODE_POINTS.test(value)) return value;
  }
  return null;
}

/** The media types of one field line: split at "," outside quoted strings, trim, drop empty ones. */
function mediaTypes(line) {
  const types = [];
  let current = '';
  let quoted = false;
  for (let i = 0; i < line.length; i++) {
    const c = line[i];
    if (quoted && c === '\\' && i + 1 < line.length) {
      current += c + line[++i];
      continue;
    }
    if (c === '"') quoted = !quoted;
    if (c === ',' && !quoted) {
      types.push(current);
      current = '';
    } else current += c;
  }
  types.push(current);
  return types.map((t) => t.replace(/^[ \t]+|[ \t]+$/g, '')).filter((t) => t !== '');
}

/** True when a charset value is a label of the UTF-8 encoding (H-62, item 3). */
export function isUtf8Label(label) {
  const space = (c) => c === '\t' || c === '\n' || c === '\f' || c === '\r' || c === ' ';
  let start = 0;
  let end = label.length;
  while (start < end && space(label[start])) start++;
  while (end > start && space(label[end - 1])) end--;
  return UTF8_LABELS.has(lowerAscii(label.slice(start, end)));
}

export const CHARSET = Object.freeze({ NONE: 'none', UTF8: 'utf-8', OTHER: 'other' });

/**
 * The charset class of the Content-Type field lines of A (H-62, items 1 to 3). More than one
 * field line, or other than one media type, is OTHER (fails closed).
 */
export function contentTypeCharset(headers) {
  const lines = headers.filter(([n]) => n === 'content-type');
  if (lines.length === 0) return CHARSET.NONE;
  if (lines.length > 1) return CHARSET.OTHER;
  const types = mediaTypes(lines[0][1]);
  if (types.length !== 1) return CHARSET.OTHER;
  const label = charsetParameter(types[0]);
  if (label === null) return CHARSET.NONE;
  return isUtf8Label(label) ? CHARSET.UTF8 : CHARSET.OTHER;
}

/**
 * The header lines of the full document (H-62, item 4), or null when the page is invalid for RQP.
 * Without a charset, "; charset=utf-8" goes after the field value without its trailing white
 * space. The new value is read again; it must have a UTF-8 charset.
 */
export function fullContentType(headers) {
  const charset = contentTypeCharset(headers);
  if (charset === CHARSET.UTF8) return headers;
  if (charset === CHARSET.OTHER) return null;
  const appended = headers.map(([n, v]) => [n, n === 'content-type' ? `${v.replace(/[ \t]+$/, '')}; charset=utf-8` : v]);
  return contentTypeCharset(appended) === CHARSET.UTF8 ? appended : null;
}
