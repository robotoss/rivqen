// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// The body fault `set_body_member` (CONTRACT section 3, S2, H-126). Pure functions: the proxy calls
// `setBodyMember` with the buffered upstream answer and delivers what it returns.
//
// Invariants:
// - At most `max` (1 MiB) wire bytes are looked at, and at most `max` bytes are decoded (zlib
//   `maxOutputLength`); a larger body is delivered unchanged with `applied: false`.
// - Every outcome that is not an edit returns the input body and fields unchanged.
// - An edit changes only the value of the top-level member `name`. Every other byte of the decoded
//   text stays as it was (the members keep their order and their raw text).
// - Never throws for any body: every failure is a `reason`.
import zlib from 'node:zlib';

export const MAX_BODY_MEMBER_BYTES = 1024 * 1024;

const lower = (s) => s.toLowerCase();
const WS = new Set([' ', '\t', '\n', '\r']);
const DROP = new Set(['content-encoding', 'transfer-encoding', 'content-length']);

/** The content coding of a field list: '' when absent; the value of the last field, lowercase. */
function coding(fields) {
  const values = fields.filter(([n]) => lower(n) === 'content-encoding').map(([, v]) => lower(v.trim()));
  return values.length === 0 ? '' : values[values.length - 1];
}

/** Remove the content coding. Returns { bytes } or { reason }. Never throws. */
export function decodeBounded(body, encoding, max = MAX_BODY_MEMBER_BYTES) {
  const options = { maxOutputLength: max };
  try {
    if (encoding === '' || encoding === 'identity') return { bytes: body };
    if (encoding === 'gzip' || encoding === 'x-gzip') return { bytes: zlib.gunzipSync(body, options) };
    if (encoding === 'deflate') return { bytes: zlib.inflateSync(body, options) };
    if (encoding === 'br') return { bytes: zlib.brotliDecompressSync(body, options) };
  } catch (cause) {
    if (cause?.code === 'ERR_BUFFER_TOO_LARGE') return { reason: `decoded body over ${max} bytes` };
    return { reason: `content-encoding ${encoding}: ${cause?.code ?? 'invalid data'}` };
  }
  return { reason: `content-encoding ${encoding} not supported` };
}

/** The index after the JSON value that starts at `i` of a text that JSON.parse accepted. */
function valueEnd(text, i) {
  const c = text[i];
  if (c === '"') return stringEnd(text, i);
  if (c === '{' || c === '[') {
    let depth = 0;
    let k = i;
    while (k < text.length) {
      const ch = text[k];
      if (ch === '"') {
        k = stringEnd(text, k);
        continue;
      }
      if (ch === '{' || ch === '[') depth += 1;
      else if (ch === '}' || ch === ']') {
        depth -= 1;
        if (depth === 0) return k + 1;
      }
      k += 1;
    }
    return k;
  }
  let k = i;
  while (k < text.length && !WS.has(text[k]) && text[k] !== ',' && text[k] !== '}' && text[k] !== ']') k += 1;
  return k;
}

/** The index after the JSON string that starts with the quote at `i`. */
function stringEnd(text, i) {
  let k = i + 1;
  while (k < text.length && text[k] !== '"') k += text[k] === '\\' ? 2 : 1;
  return k + 1;
}

const ESCAPES = { '"': '"', '\\': '\\', '/': '/', b: '\b', f: '\f', n: '\n', r: '\r', t: '\t' };

/**
 * The value of a JSON string token (quotes included) of a text that JSON.parse accepted.
 * Not JSON.parse: Node.js 26.8.2 returns the key "\\" for a later one-character escaped key such as
 * "\n" once it has parsed a key "\\" (seen in the property test below; Node.js 22.20.0 is correct).
 */
export function decodeJsonString(token) {
  let out = '';
  let k = 1;
  const end = token.length - 1;
  while (k < end) {
    const c = token[k];
    if (c !== '\\') {
      out += c;
      k += 1;
    } else if (token[k + 1] === 'u') {
      out += String.fromCharCode(Number.parseInt(token.slice(k + 2, k + 6), 16));
      k += 6;
    } else {
      out += ESCAPES[token[k + 1]];
      k += 2;
    }
  }
  return out;
}

function skipWs(text, i) {
  let k = i;
  while (k < text.length && WS.has(text[k])) k += 1;
  return k;
}

/**
 * The top-level members of a JSON object text that JSON.parse accepted:
 * [{ key, valueStart, valueEnd }], and the index of the closing brace.
 */
export function topLevelMembers(text) {
  const members = [];
  let k = skipWs(text, 0) + 1; // after '{'
  for (;;) {
    k = skipWs(text, k);
    if (text[k] === '}') return { members, close: k };
    const keyEnd = stringEnd(text, k);
    const key = decodeJsonString(text.slice(k, keyEnd));
    k = skipWs(text, keyEnd) + 1; // after ':'
    const valueStart = skipWs(text, k);
    const end = valueEnd(text, valueStart);
    members.push({ key, valueStart, valueEnd: end });
    k = skipWs(text, end);
    if (text[k] === ',') k += 1;
  }
}

/** Replace (or add) the top-level member `name` of a JSON object text. Returns { text } or { reason }. */
export function replaceMember(text, name, value) {
  let parsed;
  try {
    parsed = JSON.parse(text);
  } catch {
    return { reason: 'body is not JSON' };
  }
  if (parsed === null || typeof parsed !== 'object' || Array.isArray(parsed)) return { reason: 'body is not a JSON object' };
  const { members, close } = topLevelMembers(text);
  const encoded = JSON.stringify(value);
  const hits = members.filter((m) => m.key === name);
  if (hits.length === 0) {
    const sep = members.length === 0 ? '' : ',';
    return { text: `${text.slice(0, close)}${sep}${JSON.stringify(name)}:${encoded}${text.slice(close)}` };
  }
  // Every member with the name gets the value (a repeated key keeps its count); right to left keeps the indexes valid.
  let out = text;
  for (const m of [...hits].reverse()) out = out.slice(0, m.valueStart) + encoded + out.slice(m.valueEnd);
  return { text: out };
}

/**
 * The fault `set_body_member` on one buffered answer.
 * @param {{ fields: [string, string][], body: Buffer, name: string, value: string, max?: number }} input
 * @returns {{ applied: true, fields: [string, string][], body: Buffer } | { applied: false, reason: string, fields: [string, string][], body: Buffer }}
 */
export function setBodyMember({ fields, body, name, value, max = MAX_BODY_MEMBER_BYTES }) {
  const unchanged = (reason) => ({ applied: false, reason, fields, body });
  if (body.length === 0) return unchanged('empty body');
  if (body.length > max) return unchanged(`body over ${max} bytes`);
  const decoded = decodeBounded(body, coding(fields), max);
  if (decoded.reason) return unchanged(decoded.reason);
  let text;
  try {
    text = new TextDecoder('utf-8', { fatal: true, ignoreBOM: true }).decode(decoded.bytes);
  } catch {
    return unchanged('body is not UTF-8');
  }
  const edit = replaceMember(text, name, value);
  if (edit.reason) return unchanged(edit.reason);
  const out = Buffer.from(edit.text, 'utf8');
  const kept = fields.filter(([n]) => !DROP.has(lower(n)));
  return { applied: true, fields: [...kept, ['Content-Length', String(out.length)]], body: out };
}
