// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// A JSON serializer that writes the same bytes as JSON.stringify (no white
// space) and labels every byte range. The raw byte breakdown of the manifest
// and of the delta envelope is the sum of the labels, so it is exact and
// additive: the labels of one text add up to its length (tests check this).
//
// Kinds:
//   structure  { } [ ] : ,
//   key        a member name with its two quotes
//   value      a value; strings keep their quotes
// Label: the path of the value with each array index written as [] (for
// example "blocks[].sha256"). A key has the label of its value.

import { StudyError } from './errors.mjs';

/** The label of a path: array indices become "[]". */
export function labelOf(path) {
  let s = '';
  for (const part of path) {
    if (typeof part === 'number') s += '[]';
    else s += s === '' ? part : `.${part}`;
  }
  return s;
}

/** Bounded depth: the documents of this study have depth 3; 32 is a hard stop for bad input. */
const MAX_DEPTH = 32;

/**
 * The labelled segments of JSON.stringify(value).
 * @returns {Array<{ kind: 'structure'|'key'|'value', label: string, text: string }>}
 */
export function segments(value) {
  const out = [];
  walk(value, [], out, 0);
  return out;
}

function push(out, kind, label, text) {
  out.push({ kind, label, text });
}

function walk(value, path, out, depth) {
  if (depth > MAX_DEPTH) throw new StudyError('STUDY_JSON_DEPTH', 'JSON value is deeper than the study limit');
  const label = labelOf(path);
  if (Array.isArray(value)) {
    push(out, 'structure', label, '[');
    value.forEach((item, i) => {
      if (i > 0) push(out, 'structure', label, ',');
      walk(item, [...path, i], out, depth + 1);
    });
    push(out, 'structure', label, ']');
    return;
  }
  if (value !== null && typeof value === 'object') {
    push(out, 'structure', label, '{');
    const keys = Object.keys(value).filter((k) => value[k] !== undefined);
    keys.forEach((k, i) => {
      if (i > 0) push(out, 'structure', label, ',');
      push(out, 'key', labelOf([...path, k]), JSON.stringify(k));
      push(out, 'structure', label, ':');
      walk(value[k], [...path, k], out, depth + 1);
    });
    push(out, 'structure', label, '}');
    return;
  }
  const text = JSON.stringify(value);
  if (text === undefined) throw new StudyError('STUDY_JSON_VALUE', `no JSON form for a value at ${label}`);
  push(out, 'value', label, text);
}

const REVISION = /^"(t1\.|r1\.)([A-Za-z0-9_-]{43})"$/;
const DIGEST = /^"([A-Za-z0-9_-]{43})"$/;

/**
 * Split a string value into parts: a revision into quotes, prefix and digest;
 * a SHA-256 base64url value into quotes and digest; a content string into
 * quotes, the UTF-8 bytes of the decoded string ("raw") and the extra bytes of
 * the JSON escapes ("escape"). Other values are one part, "text".
 */
export function partsOf(text, isContent) {
  const len = Buffer.byteLength(text, 'utf8');
  const rev = REVISION.exec(text);
  if (rev !== null) return { quotes: 2, prefix: 3, digest: 43 };
  if (DIGEST.test(text)) return { quotes: 2, digest: 43 };
  if (isContent && text.startsWith('"')) {
    const raw = Buffer.byteLength(JSON.parse(text), 'utf8');
    return { quotes: 2, raw, escape: len - 2 - raw };
  }
  return { text: len };
}

/**
 * Tally of the segments.
 * @param segs segments()
 * @param contentLabels labels whose string values are block content
 * @returns {{ total: number, structure: number, keys: Record<string, number>, values: Record<string, Record<string, number>> }}
 */
export function tally(segs, contentLabels = new Set()) {
  const t = { total: 0, structure: 0, keys: {}, values: {} };
  for (const s of segs) {
    const n = Buffer.byteLength(s.text, 'utf8');
    t.total += n;
    if (s.kind === 'structure') t.structure += n;
    else if (s.kind === 'key') t.keys[s.label] = (t.keys[s.label] ?? 0) + n;
    else {
      const parts = partsOf(s.text, contentLabels.has(s.label));
      const v = (t.values[s.label] ??= {});
      for (const [k, b] of Object.entries(parts)) v[k] = (v[k] ?? 0) + b;
    }
  }
  return t;
}

/** Sum of all bytes in a tally (must equal tally.total). */
export function tallySum(t) {
  let n = t.structure;
  for (const b of Object.values(t.keys)) n += b;
  for (const parts of Object.values(t.values)) for (const b of Object.values(parts)) n += b;
  return n;
}
