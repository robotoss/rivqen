// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// A minimal CBOR encoder (RFC 8949) for the size measurement of a binary
// envelope (H-11: "Binary encodings (deterministic CBOR) only later, as an
// option, after measurement"). Measurement only: it is not a protocol
// encoder. It writes the preferred serialization (shortest argument) of
// unsigned integers, byte strings (Buffer), text strings, arrays and maps
// with text keys in insertion order, booleans and null.

import { StudyError } from './errors.mjs';

function head(major, n) {
  if (!Number.isSafeInteger(n) || n < 0) throw new StudyError('STUDY_CBOR', 'CBOR argument is not an unsigned integer');
  const m = major << 5;
  if (n < 24) return Buffer.from([m | n]);
  if (n < 0x100) return Buffer.from([m | 24, n]);
  if (n < 0x10000) {
    const b = Buffer.alloc(3);
    b[0] = m | 25;
    b.writeUInt16BE(n, 1);
    return b;
  }
  if (n < 0x100000000) {
    const b = Buffer.alloc(5);
    b[0] = m | 26;
    b.writeUInt32BE(n, 1);
    return b;
  }
  const b = Buffer.alloc(9);
  b[0] = m | 27;
  b.writeBigUInt64BE(BigInt(n), 1);
  return b;
}

const MAX_DEPTH = 32;

/** CBOR bytes of a value. */
export function encode(value, depth = 0) {
  if (depth > MAX_DEPTH) throw new StudyError('STUDY_CBOR', 'CBOR value is deeper than the study limit');
  if (value === null) return Buffer.from([0xf6]);
  if (value === false) return Buffer.from([0xf4]);
  if (value === true) return Buffer.from([0xf5]);
  if (typeof value === 'number') return head(0, value);
  if (Buffer.isBuffer(value)) return Buffer.concat([head(2, value.length), value]);
  if (typeof value === 'string') {
    const b = Buffer.from(value, 'utf8');
    return Buffer.concat([head(3, b.length), b]);
  }
  if (Array.isArray(value)) return Buffer.concat([head(4, value.length), ...value.map((v) => encode(v, depth + 1))]);
  if (typeof value === 'object') {
    const keys = Object.keys(value).filter((k) => value[k] !== undefined);
    return Buffer.concat([head(5, keys.length), ...keys.flatMap((k) => [encode(k, depth + 1), encode(value[k], depth + 1)])]);
  }
  throw new StudyError('STUDY_CBOR', `no CBOR form for ${typeof value}`);
}
