// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// The .http file of one proxy exchange, in the format of evidence/traces/server/
// (tools/upstream-lab/run_traces.py `save`), with the CONTRACT section 5 `--- delivered ---` addition.
import zlib from 'node:zlib';

const MAX_DECODED = 8 * 1024 * 1024;

const lower = (s) => s.toLowerCase();

/** Content-Encoding of a field list: the last value of the last such field, lowercase, or '' */
export function contentEncoding(fields) {
  const values = fields.filter(([n]) => lower(n) === 'content-encoding').map(([, v]) => lower(v.trim()));
  return values.length === 0 ? '' : values[values.length - 1];
}

/** Decode a body for reading. Returns { text: Buffer, note } and never throws. */
export function decodeBody(body, encoding) {
  const options = { maxOutputLength: MAX_DECODED };
  try {
    if (encoding === 'gzip' || encoding === 'x-gzip') {
      return { body: zlib.gunzipSync(body, options), note: `[body decoded from content-encoding=${encoding}]` };
    }
    if (encoding === 'deflate') {
      return { body: zlib.inflateSync(body, options), note: `[body decoded from content-encoding=${encoding}]` };
    }
    if (encoding === 'br') {
      return { body: zlib.brotliDecompressSync(body, options), note: `[body decoded from content-encoding=${encoding}]` };
    }
  } catch (cause) {
    return { body, note: `[body not decoded: content-encoding=${encoding}, ${cause.code ?? 'invalid data'}]` };
  }
  if (encoding === '' || encoding === 'identity') {
    return { body, note: '[body decoded from content-encoding=identity]' };
  }
  return { body, note: `[body not decoded: content-encoding=${encoding}]` };
}

const fieldLines = (fields) => fields.map(([n, v]) => `${n}: ${v}\n`).join('');

/**
 * @param {{
 *   method: string, target: string, requestFields: [string,string][], requestBody: Buffer,
 *   response: { status: number, fields: [string,string][], body: Buffer } | null,
 *   delivered: { status: number, fields: [string,string][], body: Buffer } | null,
 *   error: string | null
 * }} rec
 * @returns {Buffer}
 */
export function formatExchange(rec) {
  const parts = [];
  const text = (s) => parts.push(Buffer.from(s, 'utf8'));
  text(`${rec.method} ${rec.target} HTTP/1.1\n${fieldLines(rec.requestFields)}\n`);
  if (rec.requestBody.length > 0) {
    text(`[request body: ${rec.requestBody.length} bytes]\n`);
    parts.push(rec.requestBody);
    text('\n');
  }
  text('--- response ---\n');
  if (rec.response) {
    const { body, note } = decodeBody(rec.response.body, contentEncoding(rec.response.fields));
    text(`HTTP ${rec.response.status}\n${fieldLines(rec.response.fields)}\n${note}\n`);
    parts.push(body);
  } else {
    text('HTTP none\n\n');
  }
  if (rec.delivered) {
    const encoding = contentEncoding(rec.delivered.fields) || 'identity';
    const total = rec.response ? rec.response.body.length : 0;
    text(`\n--- delivered ---\nHTTP ${rec.delivered.status}\n${fieldLines(rec.delivered.fields)}\n`);
    text(`[body delivered: ${rec.delivered.body.length} of ${total} wire bytes, content-encoding=${encoding}]\n`);
    parts.push(rec.delivered.body);
  }
  if (rec.error) text(`\n--- proxy error ---\n${rec.error}\n`);
  return Buffer.concat(parts);
}
