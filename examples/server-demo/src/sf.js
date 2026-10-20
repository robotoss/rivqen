// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors
//
// Structured Field Values parser (RFC 9651 section 4.2) for the three fields that
// the demo reads: Rq-Version (Item), Rq-Capabilities (List) and Rq-Have (Dictionary).
// Every parse function returns null when the field value does not parse; RQP then
// treats the field as absent (negotiation section 3.1). It never throws for a string.
//
// Parsed shapes:
//   item   { kind: 'item', type, value, params }   type: integer, decimal, string, token,
//                                                  bytes, boolean, date, displaystring
//   inner  { kind: 'inner', items: [item], params }
// params and dictionaries are Maps (a repeated key keeps the last value, RFC 9651 4.2.2).

import { Buffer } from 'node:buffer';

class SfError extends Error {
  constructor(message) {
    super(message);
    this.name = 'SfError';
    this.code = 'SF_PARSE';
  }
}

const DIGIT = /^[0-9]$/;
const ALPHA = /^[A-Za-z]$/;
const KEY_START = /^[a-z*]$/;
const KEY_CHAR = /^[a-z0-9_\-.*]$/;
const TCHAR = /^[A-Za-z0-9!#$%&'*+\-.^_`|~]$/;
const BASE64 = /^[A-Za-z0-9+/]*={0,2}$/;

class Parser {
  constructor(text) {
    this.s = text;
    this.i = 0;
  }

  done() {
    return this.i >= this.s.length;
  }

  peek() {
    return this.s[this.i];
  }

  fail(message) {
    throw new SfError(message);
  }

  skipSp() {
    while (this.peek() === ' ') this.i++;
  }

  skipOws() {
    while (this.peek() === ' ' || this.peek() === '\t') this.i++;
  }

  /** Separator of List and Dictionary members. Returns false at the end of the input. */
  nextMember() {
    this.skipOws();
    if (this.done()) return false;
    if (this.peek() !== ',') this.fail('expected a comma');
    this.i++;
    this.skipOws();
    if (this.done()) this.fail('trailing comma');
    return true;
  }

  list() {
    const members = [];
    while (!this.done()) {
      members.push(this.itemOrInner());
      if (!this.nextMember()) break;
    }
    return members;
  }

  dictionary() {
    const map = new Map();
    while (!this.done()) {
      const key = this.key();
      if (this.peek() === '=') {
        this.i++;
        map.set(key, this.itemOrInner());
      } else {
        map.set(key, { kind: 'item', type: 'boolean', value: true, params: this.params() });
      }
      if (!this.nextMember()) break;
    }
    return map;
  }

  itemOrInner() {
    return this.peek() === '(' ? this.inner() : this.item();
  }

  inner() {
    this.i++; // (
    const items = [];
    for (;;) {
      this.skipSp();
      if (this.done()) this.fail('unclosed inner list');
      if (this.peek() === ')') {
        this.i++;
        return { kind: 'inner', items, params: this.params() };
      }
      items.push(this.item());
      const c = this.peek();
      if (c !== ' ' && c !== ')') this.fail('expected a space or )');
    }
  }

  item() {
    const { type, value } = this.bare();
    return { kind: 'item', type, value, params: this.params() };
  }

  params() {
    const params = new Map();
    while (this.peek() === ';') {
      this.i++;
      this.skipSp();
      const key = this.key();
      if (this.peek() === '=') {
        this.i++;
        const { type, value } = this.bare();
        params.set(key, { type, value });
      } else {
        params.set(key, { type: 'boolean', value: true });
      }
    }
    return params;
  }

  key() {
    const c = this.peek();
    if (c === undefined || !KEY_START.test(c)) this.fail('bad key');
    const start = this.i;
    this.i++;
    while (!this.done() && KEY_CHAR.test(this.peek())) this.i++;
    return this.s.slice(start, this.i);
  }

  bare() {
    const c = this.peek();
    if (c === undefined) this.fail('missing value');
    if (c === '-' || DIGIT.test(c)) return this.number();
    if (c === '"') return { type: 'string', value: this.string() };
    if (ALPHA.test(c) || c === '*') return { type: 'token', value: this.token() };
    if (c === ':') return { type: 'bytes', value: this.bytes() };
    if (c === '?') return { type: 'boolean', value: this.boolean() };
    if (c === '@') return this.date();
    if (c === '%') return { type: 'displaystring', value: this.displayString() };
    return this.fail('unknown value type');
  }

  number() {
    let sign = 1;
    if (this.peek() === '-') {
      sign = -1;
      this.i++;
    }
    if (this.done() || !DIGIT.test(this.peek())) this.fail('number needs a digit');
    let text = '';
    let decimal = false;
    while (!this.done()) {
      const c = this.peek();
      if (DIGIT.test(c)) {
        text += c;
      } else if (!decimal && c === '.') {
        if (text.length > 12) this.fail('decimal integer part too long');
        text += c;
        decimal = true;
      } else {
        break;
      }
      this.i++;
      if (!decimal && text.length > 15) this.fail('integer too long');
      if (decimal && text.length > 16) this.fail('decimal too long');
    }
    if (!decimal) return { type: 'integer', value: sign * Number(text) };
    if (text.endsWith('.')) this.fail('decimal ends with a dot');
    if (text.length - text.indexOf('.') - 1 > 3) this.fail('decimal fraction too long');
    return { type: 'decimal', value: sign * Number(text) };
  }

  string() {
    this.i++; // opening quote
    let out = '';
    for (;;) {
      if (this.done()) this.fail('unclosed string');
      const c = this.peek();
      this.i++;
      if (c === '\\') {
        const n = this.peek();
        if (n !== '"' && n !== '\\') this.fail('bad escape');
        out += n;
        this.i++;
      } else if (c === '"') {
        return out;
      } else {
        const code = c.charCodeAt(0);
        if (code < 0x20 || code > 0x7e) this.fail('bad character in string');
        out += c;
      }
    }
  }

  token() {
    const start = this.i;
    this.i++;
    while (!this.done() && (TCHAR.test(this.peek()) || this.peek() === ':' || this.peek() === '/')) this.i++;
    return this.s.slice(start, this.i);
  }

  bytes() {
    const end = this.s.indexOf(':', this.i + 1);
    if (end < 0) this.fail('unclosed byte sequence');
    const text = this.s.slice(this.i + 1, end);
    this.i = end + 1;
    if (!BASE64.test(text)) this.fail('bad base64');
    // RFC 9651 4.2.7: padding may be missing and pad bits may be non-zero.
    if (text.replace(/=+$/, '').length % 4 === 1) this.fail('bad base64 length');
    return Buffer.from(text, 'base64');
  }

  boolean() {
    this.i++;
    const c = this.peek();
    if (c !== '0' && c !== '1') this.fail('bad boolean');
    this.i++;
    return c === '1';
  }

  date() {
    this.i++;
    const n = this.number();
    if (n.type !== 'integer') this.fail('date needs an integer');
    return { type: 'date', value: n.value };
  }

  displayString() {
    this.i++;
    if (this.peek() !== '"') this.fail('bad display string');
    this.i++;
    const bytes = [];
    for (;;) {
      if (this.done()) this.fail('unclosed display string');
      const c = this.peek();
      this.i++;
      if (c === '%') {
        const hex = this.s.slice(this.i, this.i + 2);
        if (!/^[0-9a-f]{2}$/.test(hex)) this.fail('bad percent escape');
        bytes.push(parseInt(hex, 16));
        this.i += 2;
      } else if (c === '"') {
        try {
          return new TextDecoder('utf-8', { fatal: true }).decode(Uint8Array.from(bytes));
        } catch {
          return this.fail('display string is not UTF-8');
        }
      } else {
        const code = c.charCodeAt(0);
        if (code < 0x20 || code > 0x7e) this.fail('bad character in display string');
        bytes.push(code);
      }
    }
  }
}

function run(text, parse) {
  const p = new Parser(text);
  try {
    p.skipSp();
    const out = parse(p);
    p.skipSp();
    if (!p.done()) return null;
    return out;
  } catch (error) {
    if (error instanceof SfError) return null;
    throw error;
  }
}

/** An Item (parameters kept), or null. */
export const parseItem = (text) => run(text, (p) => p.item());

/** A List: an array of items and inner lists, or null. */
export const parseList = (text) => run(text, (p) => p.list());

/** A Dictionary: a Map of key to item or inner list, or null. */
export const parseDictionary = (text) => run(text, (p) => p.dictionary());
