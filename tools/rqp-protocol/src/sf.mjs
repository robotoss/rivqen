// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// Parser for Structured Field Values (RFC 9651 section 4.2): Item, List and
// Dictionary, with all bare item types. RQP uses it for Rq-Version (Item),
// Rq-Capabilities (List), Rq-Have (Dictionary) and Rq-Nonce (Item, H-109).
//
// The functions return null on a parse failure ("the entire field is ignored",
// RFC 9651 section 2.2) and never throw for a string input. Work is linear in
// the input length: every loop consumes at least one character. The callers
// bound the input length before they parse (negotiation section 7).

const DIGIT = /[0-9]/;
const LCALPHA = /[a-z]/;
const ALPHA = /[A-Za-z]/;
const KEY_CHAR = /[a-z0-9_\-.*]/;
// tchar of RFC 9110 section 5.6.2, plus ":" and "/" (RFC 9651 section 3.3.4).
const TOKEN_CHAR = /[!#$%&'*+\-.^_`|~0-9A-Za-z:/]/;
const B64_CHAR = /[A-Za-z0-9+/=]/;

class Input {
  constructor(s) {
    this.s = s;
    this.i = 0;
  }

  get empty() {
    return this.i >= this.s.length;
  }

  peek() {
    return this.s[this.i];
  }

  take() {
    return this.s[this.i++];
  }

  skipSp() {
    while (this.s[this.i] === ' ') this.i++;
  }

  skipOws() {
    while (this.s[this.i] === ' ' || this.s[this.i] === '\t') this.i++;
  }
}

class Fail extends Error {}

function fail() {
  throw new Fail();
}

/** RFC 9651 section 4.2: the top-level steps around a parser for one field type. */
function parseField(value, parse) {
  if (typeof value !== 'string') return null;
  // Step 1: the field value must be ASCII.
  for (let k = 0; k < value.length; k++) if (value.charCodeAt(k) > 0x7f) return null;
  const input = new Input(value);
  try {
    input.skipSp();
    const out = parse(input);
    input.skipSp();
    if (!input.empty) return null;
    return out;
  } catch (error) {
    if (error instanceof Fail) return null;
    throw error;
  }
}

/** Parse a List. Members: { inner: true, items, params } or { inner: false, item, params }. */
export function parseList(value) {
  return parseField(value, (input) => {
    const members = [];
    while (!input.empty) {
      members.push(parseItemOrInnerList(input));
      input.skipOws();
      if (input.empty) return members;
      if (input.take() !== ',') fail();
      input.skipOws();
      if (input.empty) fail(); // trailing comma
    }
    return members;
  });
}

/** Parse a Dictionary into a Map key -> member. A later key overwrites an earlier one (section 4.2.2). */
export function parseDictionary(value) {
  return parseField(value, (input) => {
    const dict = new Map();
    while (!input.empty) {
      const key = parseKey(input);
      let member;
      if (input.peek() === '=') {
        input.take();
        member = parseItemOrInnerList(input);
      } else {
        member = { inner: false, item: { type: 'boolean', value: true }, params: parseParams(input) };
      }
      dict.set(key, member);
      input.skipOws();
      if (input.empty) return dict;
      if (input.take() !== ',') fail();
      input.skipOws();
      if (input.empty) fail();
    }
    return dict;
  });
}

/** Parse an Item: { item, params }. */
export function parseItem(value) {
  return parseField(value, (input) => {
    const item = parseBareItem(input);
    return { item, params: parseParams(input) };
  });
}

function parseItemOrInnerList(input) {
  if (input.peek() === '(') return parseInnerList(input);
  const item = parseBareItem(input);
  return { inner: false, item, params: parseParams(input) };
}

function parseInnerList(input) {
  input.take(); // "("
  const items = [];
  while (!input.empty) {
    input.skipSp();
    if (input.peek() === ')') {
      input.take();
      return { inner: true, items, params: parseParams(input) };
    }
    const item = parseBareItem(input);
    items.push({ item, params: parseParams(input) });
    const c = input.peek();
    if (c !== ' ' && c !== ')') fail();
  }
  return fail(); // no ")" before the end of the input
}

function parseParams(input) {
  const params = new Map();
  while (input.peek() === ';') {
    input.take();
    input.skipSp();
    const key = parseKey(input);
    let value = { type: 'boolean', value: true };
    if (input.peek() === '=') {
      input.take();
      value = parseBareItem(input);
    }
    params.set(key, value);
  }
  return params;
}

function parseKey(input) {
  const c = input.peek();
  if (c === undefined || !(LCALPHA.test(c) || c === '*')) fail();
  let key = '';
  while (!input.empty && KEY_CHAR.test(input.peek())) key += input.take();
  return key;
}

function parseBareItem(input) {
  const c = input.peek();
  if (c === undefined) return fail();
  if (c === '-' || DIGIT.test(c)) return parseNumber(input);
  if (c === '"') return parseString(input);
  if (c === '*' || ALPHA.test(c)) return parseToken(input);
  if (c === ':') return parseByteSequence(input);
  if (c === '?') return parseBoolean(input);
  if (c === '@') return parseDate(input);
  if (c === '%') return parseDisplayString(input);
  return fail();
}

// Section 4.2.4.
function parseNumber(input) {
  let type = 'integer';
  let sign = 1;
  let num = '';
  if (input.peek() === '-') {
    input.take();
    sign = -1;
  }
  if (input.empty || !DIGIT.test(input.peek())) fail();
  while (!input.empty) {
    const c = input.peek();
    if (DIGIT.test(c)) {
      num += input.take();
    } else if (type === 'integer' && c === '.') {
      if (num.length > 12) fail();
      num += input.take();
      type = 'decimal';
    } else {
      break;
    }
    if (type === 'integer' && num.length > 15) fail();
    if (type === 'decimal' && num.length > 16) fail();
  }
  if (type === 'integer') return { type, value: sign * Number.parseInt(num, 10) };
  if (num.endsWith('.')) fail();
  if (num.length - num.indexOf('.') - 1 > 3) fail();
  return { type, value: sign * Number.parseFloat(num) };
}

// Section 4.2.5.
function parseString(input) {
  input.take(); // DQUOTE
  let out = '';
  while (!input.empty) {
    const c = input.take();
    if (c === '\\') {
      if (input.empty) fail();
      const next = input.take();
      if (next !== '"' && next !== '\\') fail();
      out += next;
    } else if (c === '"') {
      return { type: 'string', value: out };
    } else {
      const code = c.charCodeAt(0);
      if (code <= 0x1f || code >= 0x7f) fail();
      out += c;
    }
  }
  return fail();
}

// Section 4.2.6.
function parseToken(input) {
  let out = input.take();
  while (!input.empty && TOKEN_CHAR.test(input.peek())) out += input.take();
  return { type: 'token', value: out };
}

// Section 4.2.7. Missing padding is accepted; "=" only at the end.
function parseByteSequence(input) {
  input.take(); // ":"
  const end = input.s.indexOf(':', input.i);
  if (end < 0) fail();
  const b64 = input.s.slice(input.i, end);
  input.i = end + 1;
  for (const c of b64) if (!B64_CHAR.test(c)) fail();
  const data = b64.replace(/=+$/, '');
  if (data.includes('=') || b64.length - data.length > 2 || data.length % 4 === 1) fail();
  return { type: 'bytes', value: Buffer.from(data, 'base64') };
}

// Section 4.2.8.
function parseBoolean(input) {
  input.take(); // "?"
  const c = input.take();
  if (c === '1') return { type: 'boolean', value: true };
  if (c === '0') return { type: 'boolean', value: false };
  return fail();
}

// Section 4.2.9.
function parseDate(input) {
  input.take(); // "@"
  const n = parseNumber(input);
  if (n.type !== 'integer') fail();
  return { type: 'date', value: n.value };
}

// Section 4.2.10.
function parseDisplayString(input) {
  input.take(); // "%"
  if (input.take() !== '"') fail();
  const bytes = [];
  while (!input.empty) {
    const c = input.take();
    const code = c.charCodeAt(0);
    if (code <= 0x1f || code >= 0x7f) fail();
    if (c === '%') {
      const hex = input.s.slice(input.i, input.i + 2);
      if (!/^[0-9a-f]{2}$/.test(hex)) fail();
      input.i += 2;
      bytes.push(Number.parseInt(hex, 16));
    } else if (c === '"') {
      try {
        return { type: 'displaystring', value: new TextDecoder('utf-8', { fatal: true }).decode(Uint8Array.from(bytes)) };
      } catch {
        return fail();
      }
    } else {
      bytes.push(code);
    }
  }
  return fail();
}
