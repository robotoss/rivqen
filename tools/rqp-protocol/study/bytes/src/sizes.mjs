// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// Byte sizes of a body: raw, gzip and Brotli with the Node.js zlib defaults.
// The method is the one of examples/server-demo/scripts/measure.js, so the
// numbers can be compared with evidence/benchmarks/rqp-bytes-2026-10-19.json:
// gzip includes its 18 bytes of header and trailer; Brotli uses quality 11.

import { brotliCompressSync, gzipSync } from 'node:zlib';

/** UTF-8 bytes of a string, or the bytes themselves. */
export function bytesOf(body) {
  return typeof body === 'string' ? Buffer.from(body, 'utf8') : Buffer.from(body);
}

/** { raw, gzip, br } of a body. An empty body is 0 bytes in every column (a 304 has no body). */
export function sizes(body) {
  const b = bytesOf(body);
  if (b.length === 0) return { raw: 0, gzip: 0, br: 0 };
  return { raw: b.length, gzip: gzipSync(b).length, br: brotliCompressSync(b).length };
}

/** a - b for each column. */
export function minus(a, b) {
  return { raw: a.raw - b.raw, gzip: a.gzip - b.gzip, br: a.br - b.br };
}

/** The compression environment. zlib output can change between versions, so results record it. */
export function environment() {
  return { node: process.versions.node, zlib: process.versions.zlib, brotli: process.versions.brotli ?? null, nghttp2: process.versions.nghttp2 ?? null };
}
