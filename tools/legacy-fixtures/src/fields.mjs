// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// Field-line helpers of the legacy model (docs/engineering/protocol/legacy-wire.md
// section 10). A field list is [[name, value], ...] in wire order. Names compare
// ASCII case-insensitively; values are trimmed of spaces and tabs.
//
// None of these functions throws for any input: a malformed list is read as empty.

import { asciiLower } from '../../rqp-protocol/src/common.mjs';

export { asciiLower };

/** Trim spaces and tabs at both ends (not other white space). */
export function trimOws(text) {
  return text.replace(/^[ \t]+|[ \t]+$/g, '');
}

/** Keep only the entries that are [string, string]; anything else is dropped. */
export function normalizeFields(list) {
  if (!Array.isArray(list)) return [];
  const out = [];
  for (const entry of list) {
    if (Array.isArray(entry) && entry.length === 2 && typeof entry[0] === 'string' && typeof entry[1] === 'string') {
      out.push([entry[0], entry[1]]);
    }
  }
  return out;
}

/** The trimmed values of every line with this name, in order. */
export function valuesOf(fields, name) {
  const wanted = asciiLower(name);
  const out = [];
  for (const [n, v] of fields) if (asciiLower(n) === wanted) out.push(trimOws(v));
  return out;
}

/** The trimmed value when exactly one line has this name; otherwise null. */
export function single(fields, name) {
  const values = valuesOf(fields, name);
  return values.length === 1 ? values[0] : null;
}

/** The trimmed value of the first line with this name; otherwise null. */
export function first(fields, name) {
  const values = valuesOf(fields, name);
  return values.length > 0 ? values[0] : null;
}

/** The lines whose names are not in `names` (lower-case set). */
export function without(fields, names) {
  return fields.filter(([n]) => !names.has(asciiLower(n)));
}

/** The media type (lower case, parameters removed) and the parameters of a content-type value. */
export function parseContentType(value) {
  const parts = value.split(';');
  const type = asciiLower(trimOws(parts[0]));
  const params = new Map();
  for (const part of parts.slice(1)) {
    const eq = part.indexOf('=');
    if (eq < 0) continue;
    const key = asciiLower(trimOws(part.slice(0, eq)));
    let val = trimOws(part.slice(eq + 1));
    if (val.length >= 2 && val.startsWith('"') && val.endsWith('"')) val = val.slice(1, -1);
    if (!params.has(key)) params.set(key, val);
  }
  return { type, params };
}

/** True when one of the cache-control lines has this directive (ASCII case-insensitive). */
export function hasDirective(fields, directive) {
  for (const value of valuesOf(fields, 'cache-control')) {
    for (const item of value.split(',')) {
      const name = trimOws(item.split('=')[0]);
      if (asciiLower(name) === directive) return true;
    }
  }
  return false;
}
