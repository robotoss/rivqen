// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// JSON text check for json blocks (docs/engineering/protocol/markup.md M-25,
// RFC 8259 section 2). It works on the raw content bytes, which are valid UTF-8
// (M-02). It does not build values. No recursion: the nesting depth is a counter
// with the limit MAX_DEPTH, so memory is constant and time is linear.

export const MAX_DEPTH = 64;

const TAB = 0x09;
const LF = 0x0a;
const CR = 0x0d;
const SPACE = 0x20;
const QUOTE = 0x22;
const PLUS = 0x2b;
const COMMA = 0x2c;
const MINUS = 0x2d;
const DOT = 0x2e;
const SLASH = 0x2f;
const ZERO = 0x30;
const COLON = 0x3a;
const LBRACKET = 0x5b;
const BACKSLASH = 0x5c;
const RBRACKET = 0x5d;
const LBRACE = 0x7b;
const RBRACE = 0x7d;

// Characters after a backslash, other than 'u': " \ / b f n r t
const SIMPLE_ESCAPES = new Set([QUOTE, BACKSLASH, SLASH, 0x62, 0x66, 0x6e, 0x72, 0x74]);

function skipWs(b, p, end) {
  while (p < end) {
    const c = b[p];
    if (c !== SPACE && c !== LF && c !== CR && c !== TAB) break;
    p++;
  }
  return p;
}

function isDigitAt(b, p, end) {
  return p < end && b[p] >= ZERO && b[p] <= 0x39;
}

function skipDigits(b, p, end) {
  while (isDigitAt(b, p, end)) p++;
  return p;
}

function hexValue(c) {
  if (c >= ZERO && c <= 0x39) return c - ZERO;
  const l = c | 0x20;
  if (l >= 0x61 && l <= 0x66) return l - 0x57;
  return -1;
}

// Four hex digits at p, or -1.
function hex4(b, p, end) {
  if (p + 4 > end) return -1;
  let v = 0;
  for (let i = 0; i < 4; i++) {
    const h = hexValue(b[p + i]);
    if (h < 0) return -1;
    v = v * 16 + h;
  }
  return v;
}

// String at p (b[p] is '"'). Returns the position after the closing quote, or -1.
function scanString(b, p, end) {
  p++;
  while (p < end) {
    const c = b[p];
    if (c === QUOTE) return p + 1;
    if (c < SPACE) return -1; // control characters must be escaped
    if (c !== BACKSLASH) {
      p++;
      continue;
    }
    p++;
    if (p >= end) return -1;
    const e = b[p];
    if (SIMPLE_ESCAPES.has(e)) {
      p++;
      continue;
    }
    if (e !== 0x75) return -1; // 'u'
    const unit = hex4(b, p + 1, end);
    if (unit < 0) return -1;
    p += 5;
    if (unit >= 0xdc00 && unit <= 0xdfff) return -1; // lone low surrogate
    if (unit >= 0xd800 && unit <= 0xdbff) {
      // A high surrogate needs a following \u escape of a low surrogate.
      if (p + 1 >= end || b[p] !== BACKSLASH || b[p + 1] !== 0x75) return -1;
      const low = hex4(b, p + 2, end);
      if (low < 0xdc00 || low > 0xdfff) return -1;
      p += 6;
    }
  }
  return -1;
}

// Number at p. Returns the position after it, or -1.
function scanNumber(b, p, end) {
  if (b[p] === MINUS) p++;
  if (!isDigitAt(b, p, end)) return -1;
  p = b[p] === ZERO ? p + 1 : skipDigits(b, p, end);
  if (p < end && b[p] === DOT) {
    p++;
    if (!isDigitAt(b, p, end)) return -1;
    p = skipDigits(b, p, end);
  }
  if (p < end && (b[p] | 0x20) === 0x65) {
    // 'e' or 'E'
    p++;
    if (p < end && (b[p] === PLUS || b[p] === MINUS)) p++;
    if (!isDigitAt(b, p, end)) return -1;
    p = skipDigits(b, p, end);
  }
  return p;
}

function scanLiteral(b, p, end, word) {
  if (p + word.length > end) return -1;
  for (let i = 0; i < word.length; i++) {
    if (b[p + i] !== word.charCodeAt(i)) return -1;
  }
  return p + word.length;
}

// Scalar value at p (not an object or array). Returns the position after it, or -1.
function scanScalar(b, p, end) {
  const c = b[p];
  if (c === QUOTE) return scanString(b, p, end);
  if (c === MINUS || (c >= ZERO && c <= 0x39)) return scanNumber(b, p, end);
  if (c === 0x74) return scanLiteral(b, p, end, 'true');
  if (c === 0x66) return scanLiteral(b, p, end, 'false');
  if (c === 0x6e) return scanLiteral(b, p, end, 'null');
  return -1;
}

// Member name and ':' at p. Returns the position of the member value, or -1.
function scanMemberName(b, p, end) {
  if (p >= end || b[p] !== QUOTE) return -1;
  p = scanString(b, p, end);
  if (p < 0) return -1;
  p = skipWs(b, p, end);
  if (p >= end || b[p] !== COLON) return -1;
  return skipWs(b, p + 1, end);
}

/**
 * True when bytes [start, end) of b are one JSON text (RFC 8259 section 2) with a
 * nesting depth of at most MAX_DEPTH.
 */
export function isJsonText(b, start, end) {
  // closers[i] is the byte that closes the container at depth i + 1.
  const closers = new Uint8Array(MAX_DEPTH);
  let depth = 0;
  let p = skipWs(b, start, end);
  for (;;) {
    // A value starts at p.
    if (p < 0 || p >= end) return false;
    const c = b[p];
    if (c === LBRACE || c === LBRACKET) {
      if (depth === MAX_DEPTH) return false;
      const closer = c === LBRACE ? RBRACE : RBRACKET;
      closers[depth++] = closer;
      p = skipWs(b, p + 1, end);
      if (p < end && b[p] === closer) {
        depth--;
        p++;
      } else {
        if (c === LBRACE) p = scanMemberName(b, p, end);
        continue;
      }
    } else {
      p = scanScalar(b, p, end);
      if (p < 0) return false;
    }
    // After a value: close containers, or go to the next value.
    for (;;) {
      p = skipWs(b, p, end);
      if (depth === 0) return p === end;
      if (p >= end) return false;
      const d = b[p];
      const closer = closers[depth - 1];
      if (d === closer) {
        depth--;
        p++;
        continue;
      }
      if (d !== COMMA) return false;
      p = skipWs(b, p + 1, end);
      if (closer === RBRACE) p = scanMemberName(b, p, end);
      break;
    }
  }
}
