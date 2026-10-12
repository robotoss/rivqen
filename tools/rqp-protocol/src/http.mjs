// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// HTTP field helpers for the reference steps: field lines, combined field
// values, entity-tags (RFC 9110 section 8.8.3), Accept (section 12.5.1) and the
// media type of Content-Type. Header lines are [name, value] pairs; names are
// compared ASCII case-insensitively.

import { asciiLower } from './common.mjs';

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
  const values = headers.filter(([n]) => n === lower).map(([, v]) => v.replace(/^[ \t]+|[ \t]+$/g, ''));
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

/** Lower-case "type/subtype" of a Content-Type value, or null. */
export function mediaType(value) {
  if (value === null) return null;
  const essence = value.split(';')[0].replace(/^[ \t]+|[ \t]+$/g, '');
  return essence === '' ? null : asciiLower(essence);
}

const ETAGC = /[\x21\x23-\x7e\x80-\xff]/;

/**
 * Parse If-None-Match: "*" or a list of entity-tags. Returns { star: true },
 * { tags: [{ weak, opaque }] } (opaque with its quotes), or null for a syntax error.
 */
export function parseEntityTags(value) {
  const v = value.replace(/^[ \t]+|[ \t]+$/g, '');
  if (v === '*') return { star: true, tags: [] };
  const tags = [];
  let i = 0;
  const skip = () => {
    while (i < v.length && (v[i] === ' ' || v[i] === '\t' || v[i] === ',')) i++;
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
  return tags.length === 0 ? null : { star: false, tags };
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
  return parts.map((p) => p.replace(/^[ \t]+|[ \t]+$/g, '')).filter((p) => p !== '');
}

const QVALUE = /^(0(\.[0-9]{0,3})?|1(\.0{0,3})?)$/;

/**
 * Negotiation section 3.6: Accept has a media range application/vnd.rivqen.delta+json
 * (ASCII case-insensitive, no wildcard) with no q parameter or q > 0.
 * A malformed q value does not accept.
 */
export function acceptsMediaType(acceptValue, type) {
  if (acceptValue === null) return false;
  const want = asciiLower(type);
  for (const range of splitList(acceptValue)) {
    const [essence, ...params] = range.split(';');
    if (asciiLower(essence.replace(/[ \t]+$/, '')) !== want) continue;
    let q = 1;
    for (const p of params) {
      const eq = p.indexOf('=');
      if (eq < 0) continue;
      const name = asciiLower(p.slice(0, eq).trim());
      if (name !== 'q') continue;
      const raw = p.slice(eq + 1).trim();
      q = QVALUE.test(raw) ? Number.parseFloat(raw) : 0;
    }
    if (q > 0) return true;
  }
  return false;
}
