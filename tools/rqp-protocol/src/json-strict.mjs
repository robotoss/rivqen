// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// JSON text parser with the extra rules of manifest-patch.md section 3.2:
// UTF-8 without a byte order mark, no duplicate member names, no lone surrogate
// escape, integers as digits only, nesting depth at most maxJsonDepth.
//
// parseStrict never throws for a byte input. Time is linear in the input
// length; recursion depth is at most `maxDepth`.

import { isUtf8 } from 'node:buffer';
import { LIMITS } from './common.mjs';

class JsonError extends Error {
  constructor(rule) {
    super(rule);
    this.rule = rule;
  }
}

const WS = new Set([' ', '\t', '\n', '\r']);
const INTEGER = /^(0|[1-9][0-9]*)$/;
const NUMBER = /^-?(0|[1-9][0-9]*)(\.[0-9]+)?([eE][+-]?[0-9]+)?/;

/**
 * Parse a JSON text. Returns { ok: true, value } or { ok: false, rule }, where
 * rule names the broken rule: encoding, bom, syntax, duplicate, surrogate,
 * integer, depth.
 *
 * @param {Uint8Array} bytes
 */
export function parseStrict(bytes, maxDepth = LIMITS.maxJsonDepth) {
  const buf = Buffer.from(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  if (!isUtf8(buf)) return { ok: false, rule: 'encoding' };
  if (buf.length >= 3 && buf[0] === 0xef && buf[1] === 0xbb && buf[2] === 0xbf) return { ok: false, rule: 'bom' };
  const p = new Parser(buf.toString('utf8'), maxDepth);
  try {
    p.ws();
    const value = p.value(1);
    p.ws();
    if (p.i !== p.s.length) throw new JsonError('syntax');
    return { ok: true, value };
  } catch (error) {
    if (error instanceof JsonError) return { ok: false, rule: error.rule };
    throw error;
  }
}

class Parser {
  constructor(s, maxDepth) {
    this.s = s;
    this.i = 0;
    this.maxDepth = maxDepth;
  }

  ws() {
    while (this.i < this.s.length && WS.has(this.s[this.i])) this.i++;
  }

  expect(c) {
    if (this.s[this.i] !== c) throw new JsonError('syntax');
    this.i++;
  }

  value(depth) {
    const c = this.s[this.i];
    if (c === '{') return this.object(depth);
    if (c === '[') return this.array(depth);
    if (c === '"') return this.string();
    if (c === '-' || (c >= '0' && c <= '9')) return this.number();
    for (const [word, v] of [['true', true], ['false', false], ['null', null]]) {
      if (this.s.startsWith(word, this.i)) {
        this.i += word.length;
        return v;
      }
    }
    throw new JsonError('syntax');
  }

  object(depth) {
    if (depth > this.maxDepth) throw new JsonError('depth');
    this.i++; // "{"
    const out = {};
    const seen = new Set();
    this.ws();
    if (this.s[this.i] === '}') {
      this.i++;
      return out;
    }
    for (;;) {
      this.ws();
      if (this.s[this.i] !== '"') throw new JsonError('syntax');
      const key = this.string();
      if (seen.has(key)) throw new JsonError('duplicate');
      seen.add(key);
      this.ws();
      this.expect(':');
      this.ws();
      // defineProperty: a member "__proto__" is an own member, as in JSON.parse.
      Object.defineProperty(out, key, { value: this.value(depth + 1), enumerable: true, writable: true, configurable: true });
      this.ws();
      if (this.s[this.i] === ',') {
        this.i++;
        continue;
      }
      this.expect('}');
      return out;
    }
  }

  array(depth) {
    if (depth > this.maxDepth) throw new JsonError('depth');
    this.i++; // "["
    const out = [];
    this.ws();
    if (this.s[this.i] === ']') {
      this.i++;
      return out;
    }
    for (;;) {
      this.ws();
      out.push(this.value(depth + 1));
      this.ws();
      if (this.s[this.i] === ',') {
        this.i++;
        continue;
      }
      this.expect(']');
      return out;
    }
  }

  number() {
    const m = NUMBER.exec(this.s.slice(this.i, this.i + 400));
    if (m === null) throw new JsonError('syntax');
    this.i += m[0].length;
    // Rule 4: the only numbers in rqp/1 documents are integers written as digits.
    if (!INTEGER.test(m[0])) throw new JsonError('integer');
    return Number(m[0]);
  }

  string() {
    this.i++; // opening quote
    let out = '';
    let from = this.i;
    const s = this.s;
    while (this.i < s.length) {
      const c = s.charCodeAt(this.i);
      if (c === 0x22) {
        out += s.slice(from, this.i);
        this.i++;
        return out;
      }
      if (c < 0x20) throw new JsonError('syntax');
      if (c !== 0x5c) {
        this.i++;
        continue;
      }
      out += s.slice(from, this.i);
      const e = s[this.i + 1];
      this.i += 2;
      if (e === 'u') {
        const unit = this.hex4();
        if (unit >= 0xd800 && unit <= 0xdbff) {
          if (s[this.i] !== '\\' || s[this.i + 1] !== 'u') throw new JsonError('surrogate');
          this.i += 2;
          const low = this.hex4();
          if (low < 0xdc00 || low > 0xdfff) throw new JsonError('surrogate');
          out += String.fromCharCode(unit, low);
        } else if (unit >= 0xdc00 && unit <= 0xdfff) {
          throw new JsonError('surrogate');
        } else {
          out += String.fromCharCode(unit);
        }
      } else {
        const simple = { '"': '"', '\\': '\\', '/': '/', b: '\b', f: '\f', n: '\n', r: '\r', t: '\t' }[e];
        if (simple === undefined) throw new JsonError('syntax');
        out += simple;
      }
      from = this.i;
    }
    throw new JsonError('syntax');
  }

  hex4() {
    const h = this.s.slice(this.i, this.i + 4);
    if (!/^[0-9A-Fa-f]{4}$/.test(h)) throw new JsonError('syntax');
    this.i += 4;
    return Number.parseInt(h, 16);
  }
}
