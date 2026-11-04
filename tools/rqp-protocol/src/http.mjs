// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// HTTP field helpers for the reference steps: field lines, combined field
// values, entity-tags (RFC 9110 section 8.8.3), Accept (section 12.5.1) and the
// media type of Content-Type. Header lines are [name, value] pairs; names are
// compared ASCII case-insensitively.
//
// Work is linear in the field value. White space is removed with index scans
// (trimChars), not with a regular expression: a pattern such as
// /^[ \t]+|[ \t]+$/g is quadratic for a long run of white space inside a value,
// because the second alternative is tried at each position of the run (64 KB:
// 1.4 s per call; found with the M1 fix of the S6 final security review,
// DL-063).

import { asciiLower } from './common.mjs';

const isOws = (c) => c === 0x20 || c === 0x09; // SP, HTAB (RFC 9110 section 5.6.3)
const isHttpWs = (c) => c === 0x20 || c === 0x09 || c === 0x0a || c === 0x0d; // WHATWG MIME Sniffing: HTTP whitespace
const isAsciiWs = (c) => isHttpWs(c) || c === 0x0c; // WHATWG Infra: ASCII whitespace

/**
 * `s` without the leading (when `start`) and trailing (when `end`) characters
 * for which `isWs(charCode)` is true. Linear: two index scans, no regular
 * expression.
 */
export function trimChars(s, isWs, { start = true, end = true } = {}) {
  let i = 0;
  let j = s.length;
  if (start) while (i < j && isWs(s.charCodeAt(i))) i++;
  if (end) while (j > i && isWs(s.charCodeAt(j - 1))) j--;
  return i === 0 && j === s.length ? s : s.slice(i, j);
}

/** `s` without leading and trailing OWS (SP and HTAB). Linear. */
export function trimOws(s) {
  return trimChars(s, isOws);
}

/** `s` without trailing OWS (SP and HTAB). Linear. */
export function trimOwsEnd(s) {
  return trimChars(s, isOws, { start: false });
}

/** Header lines in a canonical form: [lower-case name, value] pairs, order kept. */
export function normalizeHeaders(headers) {
  if (headers === null || headers === undefined) return [];
  if (Array.isArray(headers)) return headers.map(([n, v]) => [asciiLower(String(n)), String(v)]);
  if (typeof headers.entries === 'function') {
    // A Map or a WHATWG Headers object.
    return [...headers.entries()].map(([n, v]) => [asciiLower(String(n)), String(v)]);
  }
  const out = [];
  for (const [n, v] of Object.entries(headers)) {
    for (const one of Array.isArray(v) ? v : [v]) out.push([asciiLower(n), String(one)]);
  }
  return out;
}

/**
 * The combined field value of all lines with this name (RFC 9110 section 5.3:
 * the values joined with ", "), or null when the field is absent.
 */
export function combined(headers, name) {
  const lower = asciiLower(name);
  const values = headers.filter(([n]) => n === lower).map(([, v]) => trimOws(v));
  return values.length === 0 ? null : values.join(', ');
}

export function has(headers, name) {
  const lower = asciiLower(name);
  return headers.some(([n]) => n === lower);
}

/** Header lines without the named fields. */
export function without(headers, ...names) {
  const drop = new Set(names.map(asciiLower));
  return headers.filter(([n]) => !drop.has(n));
}

/**
 * The members of the Vary field lines of `headers` (normalized lines): field
 * names (or "*") in ASCII lower case, without white space and without empty
 * list elements, in order. Vary has no quoted strings (RFC 9110 section
 * 12.5.5), so a split on "," is exact.
 */
export function varyMembers(headers) {
  const vary = combined(headers, 'vary');
  if (vary === null) return [];
  return vary
    .split(',')
    .map((m) => asciiLower(trimOws(m)))
    .filter((m) => m !== '');
}

/** Lower-case "type/subtype" of a Content-Type value, or null. */
export function mediaType(value) {
  if (value === null) return null;
  const essence = trimOws(value.split(';')[0]);
  return essence === '' ? null : asciiLower(essence);
}

const ETAGC = /[\x21\x23-\x7e\x80-\xff]/;

/**
 * Parse If-None-Match: "*" or a list of entity-tags. Returns { star: true },
 * { tags: [{ weak, opaque }], emptyElement } (opaque with its quotes), or null
 * for a syntax error. `emptyElement` is true when the list has an empty
 * element (a leading, a trailing or a doubled comma, RFC 9110 section 5.6.1):
 * the tags still count for the 304 test, but they give no delta base
 * (negotiation section 4, open input 10).
 */
export function parseEntityTags(value) {
  const v = trimOws(value);
  if (v === '*') return { star: true, tags: [] };
  const tags = [];
  let i = 0;
  let commas = 0;
  const skip = () => {
    while (i < v.length && (v[i] === ' ' || v[i] === '\t' || v[i] === ',')) {
      if (v[i] === ',') commas++;
      i++;
    }
  };
  skip();
  while (i < v.length) {
    let weak = false;
    if (v.startsWith('W/', i)) {
      weak = true;
      i += 2;
    }
    if (v[i] !== '"') return null;
    let j = i + 1;
    while (j < v.length && v[j] !== '"') {
      if (!ETAGC.test(v[j])) return null;
      j++;
    }
    if (j >= v.length) return null;
    tags.push({ weak, opaque: v.slice(i, j + 1) });
    i = j + 1;
    // After a tag: optional white space, then a comma or the end.
    while (i < v.length && (v[i] === ' ' || v[i] === '\t')) i++;
    if (i < v.length && v[i] !== ',') return null;
    skip();
  }
  return tags.length === 0 ? null : { star: false, tags, emptyElement: commas !== tags.length - 1 };
}

/** Split a field value on commas outside quoted strings. */
export function splitList(value) {
  const parts = [];
  let cur = '';
  let quoted = false;
  for (let i = 0; i < value.length; i++) {
    const c = value[i];
    if (quoted) {
      cur += c;
      if (c === '\\' && i + 1 < value.length) cur += value[++i];
      else if (c === '"') quoted = false;
    } else if (c === '"') {
      quoted = true;
      cur += c;
    } else if (c === ',') {
      parts.push(cur);
      cur = '';
    } else {
      cur += c;
    }
  }
  parts.push(cur);
  return parts.map(trimOws).filter((p) => p !== '');
}

/**
 * The labels of the encoding UTF-8 in the WHATWG Encoding Standard, section
 * 4.2 "Names and labels" (checked on 2026-10-17; the standard was last updated
 * on 2026-05-21).
 */
export const UTF8_LABELS = Object.freeze(['unicode-1-1-utf-8', 'unicode11utf8', 'unicode20utf8', 'utf-8', 'utf8', 'x-unicode20utf8']);

/**
 * "Get an encoding" (Encoding Standard section 4.2) restricted to UTF-8: true
 * when the label without leading and trailing ASCII white space (09, 0A, 0C,
 * 0D, 20) is a UTF-8 label by ASCII case-insensitive match.
 */
export function isUtf8Label(label) {
  return UTF8_LABELS.includes(asciiLower(trimChars(label, isAsciiWs)));
}

const HTTP_WS = new Set(['\t', '\n', '\r', ' ']);
const QUOTED_STRING_TOKEN = /^[\t\x20-\x7e\x80-\xff]*$/;

/**
 * The value of the first valid charset parameter of one MIME type, or null.
 * The parameter steps of "parse a MIME type" (WHATWG MIME Sniffing section
 * 4.4, steps 11.1 to 11.10): a quoted value is an HTTP quoted string with
 * backslash escapes (the quotes are removed); an unquoted value loses its
 * trailing HTTP white space and is skipped when it is empty; the first valid
 * parameter of a name wins. The essence is not checked here (step 1 of the
 * server does that).
 */
export function charsetParameter(mime) {
  let i = mime.indexOf(';');
  if (i < 0) return null;
  while (i < mime.length) {
    i++; // past ";"
    while (i < mime.length && HTTP_WS.has(mime[i])) i++;
    const nameStart = i;
    while (i < mime.length && mime[i] !== ';' && mime[i] !== '=') i++;
    const name = asciiLower(mime.slice(nameStart, i));
    if (i < mime.length) {
      if (mime[i] === ';') continue;
      i++; // past "="
    }
    if (i >= mime.length) break;
    let value = '';
    if (mime[i] === '"') {
      i++;
      while (i < mime.length) {
        const c = mime[i++];
        if (c === '"') break;
        if (c !== '\\') value += c;
        else if (i >= mime.length) value += '\\';
        else value += mime[i++];
      }
      while (i < mime.length && mime[i] !== ';') i++;
    } else {
      const valueStart = i;
      while (i < mime.length && mime[i] !== ';') i++;
      value = trimChars(mime.slice(valueStart, i), isHttpWs, { start: false });
      if (value === '') continue;
    }
    if (name === 'charset' && QUOTED_STRING_TOKEN.test(value)) return value;
  }
  return null;
}

/** The charset classes of a Content-Type field (H-62, item 3). */
export const Charset = Object.freeze({ NONE: 'none', UTF8: 'utf-8', OTHER: 'other' });

/**
 * H-62 (3): the charset of the Content-Type field lines. NONE: no field or no
 * charset parameter. UTF8: one field line with one media type whose charset is
 * a UTF-8 label. OTHER: any other charset, and every field with more than one
 * line or more than one media type (the Fetch Standard takes the last media
 * type; this function does not model that and fails closed).
 */
export function contentTypeCharset(headers) {
  const lines = headers.filter(([n]) => n === 'content-type');
  if (lines.length === 0) return Charset.NONE;
  if (lines.length > 1) return Charset.OTHER;
  const values = splitList(lines[0][1]);
  if (values.length !== 1) return Charset.OTHER;
  const label = charsetParameter(values[0]);
  if (label === null) return Charset.NONE;
  return isUtf8Label(label) ? Charset.UTF8 : Charset.OTHER;
}

/** True when the bytes start with the UTF-8 byte order mark EF BB BF. */
export function startsWithUtf8Bom(bytes) {
  return bytes.length >= 3 && bytes[0] === 0xef && bytes[1] === 0xbb && bytes[2] === 0xbf;
}

const QVALUE = /^(0(\.[0-9]{0,3})?|1(\.0{0,3})?)$/;
const TCHAR = /[!#$%&'*+\-.^_`|~0-9A-Za-z]/;
const QDTEXT = /[\t\x20\x21\x23-\x5b\x5d-\x7e\x80-\xff]/;

/**
 * The parameters of one media range (RFC 9110 section 5.6.6, "parameters =
 * *( OWS ";" OWS [ parameter ] )"), from the text that starts at the first
 * ";". Returns a list of { name, value, quoted } or null for a syntax error:
 * a parameter without "=", an empty or non-token name, an empty value, a
 * quoted string that does not end, or text after a value that is not OWS and
 * ";". No white space is allowed around "=". Linear: one index scan.
 */
function mediaRangeParameters(text) {
  const out = [];
  let i = 0;
  while (i < text.length) {
    i++; // past ";"
    while (i < text.length && isOws(text.charCodeAt(i))) i++;
    if (i >= text.length) break;
    if (text[i] === ';') continue; // an empty parameter
    const nameStart = i;
    while (i < text.length && TCHAR.test(text[i])) i++;
    if (i === nameStart || text[i] !== '=') return null;
    const name = text.slice(nameStart, i);
    i++; // past "="
    let value = '';
    const quoted = text[i] === '"';
    if (quoted) {
      i++;
      let closed = false;
      while (i < text.length) {
        const c = text[i++];
        if (c === '"') {
          closed = true;
          break;
        }
        if (c === '\\') {
          if (i >= text.length || !/[\t\x20-\x7e\x80-\xff]/.test(text[i])) return null;
          value += text[i++];
        } else if (QDTEXT.test(c)) {
          value += c;
        } else {
          return null;
        }
      }
      if (!closed) return null;
    } else {
      const valueStart = i;
      while (i < text.length && TCHAR.test(text[i])) i++;
      if (i === valueStart) return null;
      value = text.slice(valueStart, i);
    }
    out.push({ name, value, quoted });
    while (i < text.length && isOws(text.charCodeAt(i))) i++;
    if (i < text.length && text[i] !== ';') return null;
  }
  return out;
}

/**
 * Negotiation section 3.6: Accept has a media range application/vnd.rivqen.delta+json
 * (ASCII case-insensitive, no wildcard) with no q parameter or q > 0. One such
 * range is enough, also when another one has q=0 (open input 9). A range accepts
 * nothing when its parameters have a syntax error, when it has two or more q
 * parameters (the name "Q" is the same as "q"), or when q is quoted or is not
 * a qvalue (RFC 9110 section 12.4.2; open input 4). Empty list elements are
 * ignored (section 5.6.1).
 */
export function acceptsMediaType(acceptValue, type) {
  if (acceptValue === null) return false;
  const want = asciiLower(type);
  for (const range of splitList(acceptValue)) {
    const semi = range.indexOf(';');
    const essence = semi < 0 ? range : range.slice(0, semi);
    if (asciiLower(trimOwsEnd(essence)) !== want) continue;
    const params = semi < 0 ? [] : mediaRangeParameters(range.slice(semi));
    if (params === null) continue;
    const weights = params.filter((p) => asciiLower(p.name) === 'q');
    if (weights.length > 1) continue;
    if (weights.length === 0) return true;
    const [{ value, quoted }] = weights;
    if (!quoted && QVALUE.test(value) && Number.parseFloat(value) > 0) return true;
  }
  return false;
}
