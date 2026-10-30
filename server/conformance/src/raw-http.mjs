// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// Raw HTTP/1.1 client of the conformance runner (CONTRACT.md section 3, items
// 2 and 3). It writes the request bytes exactly: the request line, then the
// field lines in the given order and spelling (a repeated name stays separate
// lines), and nothing else. It reads one response and returns the status, the
// field lines in order with their spelling, and the body bytes after the
// transfer framing (RFC 9112 section 6.3; chunked is decoded, it is framing).
// It never joins repeated fields and never decodes a content coding.
//
// The response is untrusted. Limits: the field section (status line, field
// lines and the empty line) at most `fields` bytes (default 64 KiB), the body
// after framing at most `body` bytes (default 64 MiB), one chunk-size line or
// trailer section at most `fields` bytes, and one deadline for the whole
// exchange. Every defect of the response is an HttpError; nothing else is
// thrown for any byte input.

import { connect } from 'node:net';

export const LIMITS = Object.freeze({ fields: 64 * 1024, body: 64 * 1024 * 1024 });
export const DEFAULT_TIMEOUT_MS = 10000;

/** Error codes of HttpError. */
export const HttpCode = Object.freeze({
  REQUEST: 'CONFORMANCE_HTTP_REQUEST', // the request cannot be sent exactly (a defect of the fixture)
  CONNECT: 'CONFORMANCE_HTTP_CONNECT', // no connection to the target
  NETWORK: 'CONFORMANCE_HTTP_NETWORK', // the connection failed after it was open
  TIMEOUT: 'CONFORMANCE_HTTP_TIMEOUT',
  CLOSED: 'CONFORMANCE_HTTP_CLOSED', // the connection closed before the end of the response
  MALFORMED: 'CONFORMANCE_HTTP_MALFORMED',
  TOO_LARGE: 'CONFORMANCE_HTTP_TOO_LARGE',
});

export class HttpError extends Error {
  constructor(code, message, options) {
    super(message, options);
    this.name = 'HttpError';
    this.code = code;
  }
}

const malformed = (m) => new HttpError(HttpCode.MALFORMED, `malformed response: ${m}`);

// RFC 9110 section 5.6.2: token.
const TOKEN = /^[!#$%&'*+\-.^_`|~0-9A-Za-z]+$/;
// The bytes that the runner sends in a request target and a field value:
// visible ASCII, SP and HTAB (RFC 9110 section 5.5 without obs-text, so the
// fixture string is the wire bytes without an encoding choice).
const SENDABLE = /^[\t\x20-\x7e]*$/;
const TARGET = /^[\x21-\x7e]+$/;
const isOws = (c) => c === 0x20 || c === 0x09;
const CRLF = Buffer.from('\r\n', 'latin1');
const END_OF_HEAD = Buffer.from('\r\n\r\n', 'latin1');

/**
 * The bytes of a request: `<method> <target> HTTP/1.1`, the field lines, an
 * empty line, the body. Throws HttpError REQUEST when a part cannot be sent
 * exactly as given.
 *
 * @param {{ method: string, target: string, fields: [string, string][], body?: Uint8Array }} request
 */
export function serializeRequest({ method, target, fields, body }) {
  if (typeof method !== 'string' || !TOKEN.test(method)) throw new HttpError(HttpCode.REQUEST, `request method ${JSON.stringify(method)} is not a token`);
  if (typeof target !== 'string' || !TARGET.test(target)) throw new HttpError(HttpCode.REQUEST, `request target ${JSON.stringify(target)} has a byte that cannot be sent`);
  let head = `${method} ${target} HTTP/1.1\r\n`;
  for (const line of fields) {
    if (!Array.isArray(line) || line.length !== 2) throw new HttpError(HttpCode.REQUEST, 'a request field line is not a [name, value] pair');
    const [name, value] = line;
    if (typeof name !== 'string' || !TOKEN.test(name)) throw new HttpError(HttpCode.REQUEST, `request field name ${JSON.stringify(name)} is not a token`);
    if (typeof value !== 'string' || !SENDABLE.test(value)) throw new HttpError(HttpCode.REQUEST, `request field ${name}: the value has a byte that cannot be sent exactly`);
    head += `${name}: ${value}\r\n`;
  }
  head += '\r\n';
  const bytes = Buffer.from(head, 'latin1');
  return body === undefined ? bytes : Buffer.concat([bytes, body]);
}

function parseStatusLine(line) {
  const m = /^HTTP\/1\.([01]) ([0-9]{3})(?: (.*))?$/s.exec(line);
  if (m === null) throw malformed(`status line ${JSON.stringify(line.slice(0, 80))}`);
  const reason = m[3] ?? '';
  for (let i = 0; i < reason.length; i++) {
    const c = reason.charCodeAt(i);
    if (c < 0x20 && c !== 0x09) throw malformed('control character in the reason phrase');
    if (c === 0x7f) throw malformed('control character in the reason phrase');
  }
  return { version: `HTTP/1.${m[1]}`, status: Number(m[2]), reason };
}

/** One field line `name: OWS value OWS` (RFC 9112 section 5). Linear: no regular expression on the value. */
function parseFieldLine(line) {
  const colon = line.indexOf(':');
  if (colon <= 0) throw malformed(`field line without a name ${JSON.stringify(line.slice(0, 80))}`);
  const name = line.slice(0, colon);
  if (!TOKEN.test(name)) throw malformed(`field name ${JSON.stringify(name.slice(0, 80))}`);
  let i = colon + 1;
  let j = line.length;
  while (i < j && isOws(line.charCodeAt(i))) i++;
  while (j > i && isOws(line.charCodeAt(j - 1))) j--;
  const value = line.slice(i, j);
  for (let k = 0; k < value.length; k++) {
    const c = value.charCodeAt(k);
    if ((c < 0x20 && c !== 0x09) || c === 0x7f) throw malformed(`field ${name}: control character in the value`);
  }
  return [name, value];
}

/** Parse the bytes of a field section without the final empty line. */
function parseHead(bytes) {
  const lines = bytes.toString('latin1').split('\r\n');
  const start = parseStatusLine(lines[0]);
  const fields = [];
  for (const line of lines.slice(1)) {
    if (line.length > 0 && isOws(line.charCodeAt(0))) throw malformed('obsolete line folding');
    fields.push(parseFieldLine(line));
  }
  return { ...start, fields };
}

const valuesOf = (fields, lower) => fields.filter(([n]) => n.toLowerCase() === lower).map(([, v]) => v);

/** The framing of the body: none, length, chunked or close (RFC 9112 section 6.3). */
function framingOf(head, method, limits) {
  if (head.status < 200) throw malformed(`interim ${head.status} response (not supported)`);
  if (method === 'HEAD' || head.status === 204 || head.status === 304) return { kind: 'none' };
  const te = valuesOf(head.fields, 'transfer-encoding');
  const cl = valuesOf(head.fields, 'content-length');
  if (te.length > 0) {
    if (cl.length > 0) throw malformed('both Transfer-Encoding and Content-Length');
    const codings = te.flatMap((v) => v.split(',')).map((c) => c.trim().toLowerCase());
    if (codings.length !== 1 || codings[0] !== 'chunked') throw malformed(`transfer coding ${JSON.stringify(te)} (only chunked is supported)`);
    return { kind: 'chunked' };
  }
  if (cl.length > 0) {
    if (!cl.every((v) => /^[0-9]+$/.test(v)) || !cl.every((v) => v === cl[0])) throw malformed(`content-length ${JSON.stringify(cl)}`);
    const length = cl[0].length > 16 ? Infinity : Number(cl[0]);
    if (length > limits.body) throw new HttpError(HttpCode.TOO_LARGE, `content-length ${cl[0]} is larger than the body limit of ${limits.body} bytes`);
    return { kind: 'length', length };
  }
  return { kind: 'close' };
}

/**
 * Incremental parser of one response. push(chunk) consumes bytes and returns
 * true when the response is complete; end() is called at the end of the
 * stream. Memory: the unconsumed bytes (at most one field section or one line)
 * and the body (at most limits.body).
 */
export class ResponseParser {
  constructor({ method = 'GET', limits = LIMITS } = {}) {
    this.method = method;
    this.limits = limits;
    this.state = 'head';
    this.pending = Buffer.alloc(0);
    this.scanned = 0;
    this.head = null;
    this.framing = null;
    this.body = [];
    this.bodyLength = 0;
    this.remaining = 0;
    this.trailerBytes = 0;
    this.extra = 0;
  }

  push(chunk) {
    if (this.state === 'done') {
      this.extra += chunk.length;
      return true;
    }
    this.pending = this.pending.length === 0 ? chunk : Buffer.concat([this.pending, chunk]);
    while (this.state !== 'done' && this.step()) {
      // step() returns false when it needs more bytes.
    }
    if (this.state === 'done') {
      this.extra += this.pending.length;
      this.pending = Buffer.alloc(0);
      return true;
    }
    return false;
  }

  /** The response after the end of the stream. Throws HttpError when it is not complete. */
  end() {
    if (this.state === 'close') this.state = 'done';
    if (this.state === 'done') return this.result();
    if (this.state === 'head' && this.pending.length === 0) throw new HttpError(HttpCode.CLOSED, 'the connection closed without a response');
    if (this.state === 'head') throw new HttpError(HttpCode.CLOSED, 'the connection closed inside the field section');
    throw new HttpError(HttpCode.CLOSED, `the connection closed before the end of the body (${this.bodyLength} bytes received)`);
  }

  result() {
    const body = this.framing.kind === 'none' ? null : Buffer.concat(this.body);
    return { version: this.head.version, status: this.head.status, reason: this.head.reason, fields: this.head.fields, body, extraBytes: this.extra };
  }

  take(n) {
    const out = this.pending.subarray(0, n);
    this.pending = this.pending.subarray(n);
    return out;
  }

  addBody(bytes) {
    this.bodyLength += bytes.length;
    if (this.bodyLength > this.limits.body) throw new HttpError(HttpCode.TOO_LARGE, `the body is larger than the limit of ${this.limits.body} bytes`);
    this.body.push(bytes);
  }

  /** A CRLF-terminated line at the start of `pending`, or null when it is not complete. */
  line(what) {
    const at = this.pending.indexOf(CRLF);
    if (at < 0) {
      if (this.pending.length > this.limits.fields) throw new HttpError(HttpCode.TOO_LARGE, `${what} is longer than ${this.limits.fields} bytes`);
      return null;
    }
    if (at > this.limits.fields) throw new HttpError(HttpCode.TOO_LARGE, `${what} is longer than ${this.limits.fields} bytes`);
    const text = this.take(at).toString('latin1');
    this.take(2);
    return text;
  }

  step() {
    switch (this.state) {
      case 'head':
        return this.stepHead();
      case 'length':
      case 'chunk-data': {
        const part = this.take(Math.min(this.remaining, this.pending.length));
        this.addBody(part);
        this.remaining -= part.length;
        if (this.remaining > 0) return false;
        this.state = this.state === 'length' ? 'done' : 'chunk-end';
        return true;
      }
      case 'close':
        this.addBody(this.take(this.pending.length));
        return false;
      case 'chunk-size':
        return this.stepChunkSize();
      case 'chunk-end':
        if (this.pending.length < 2) return false;
        if (!this.take(2).equals(CRLF)) throw malformed('no CRLF after the chunk data');
        this.state = 'chunk-size';
        return true;
      case 'trailers':
        return this.stepTrailer();
      default:
        return false;
    }
  }

  stepHead() {
    const at = this.pending.indexOf(END_OF_HEAD, Math.max(0, this.scanned - 3));
    if (at < 0) {
      if (this.pending.length > this.limits.fields) throw new HttpError(HttpCode.TOO_LARGE, `the field section is larger than ${this.limits.fields} bytes`);
      this.scanned = this.pending.length;
      return false;
    }
    if (at + 4 > this.limits.fields) throw new HttpError(HttpCode.TOO_LARGE, `the field section is larger than ${this.limits.fields} bytes`);
    const headBytes = this.take(at);
    this.take(4);
    // A bare CR or LF stays inside a line and fails the control character checks.
    this.head = parseHead(headBytes);
    this.framing = framingOf(this.head, this.method, this.limits);
    if (this.framing.kind === 'none') this.state = 'done';
    else if (this.framing.kind === 'length') {
      this.remaining = this.framing.length;
      this.state = this.remaining === 0 ? 'done' : 'length';
    } else if (this.framing.kind === 'chunked') this.state = 'chunk-size';
    else this.state = 'close';
    return true;
  }

  stepChunkSize() {
    const text = this.line('a chunk-size line');
    if (text === null) return false;
    const m = /^([0-9A-Fa-f]+)[ \t]*(?:;.*)?$/s.exec(text);
    if (m === null) throw malformed(`chunk-size line ${JSON.stringify(text.slice(0, 80))}`);
    const size = m[1].length > 12 ? Infinity : parseInt(m[1], 16);
    if (this.bodyLength + size > this.limits.body) throw new HttpError(HttpCode.TOO_LARGE, `the body is larger than the limit of ${this.limits.body} bytes`);
    if (size === 0) {
      this.state = 'trailers';
      return true;
    }
    this.remaining = size;
    this.state = 'chunk-data';
    return true;
  }

  stepTrailer() {
    const text = this.line('a trailer field line');
    if (text === null) return false;
    if (text === '') {
      this.state = 'done';
      return true;
    }
    this.trailerBytes += text.length + 2;
    if (this.trailerBytes > this.limits.fields) throw new HttpError(HttpCode.TOO_LARGE, `the trailer section is larger than ${this.limits.fields} bytes`);
    parseFieldLine(text);
    return true;
  }
}

/**
 * Send one request on a new connection and read one response.
 *
 * @param {{ host: string, port: number, method: string, target: string,
 *   fields: [string, string][], body?: Uint8Array, timeoutMs?: number,
 *   limits?: { fields: number, body: number } }} options
 * @returns {Promise<{ version: string, status: number, reason: string,
 *   fields: [string, string][], body: Buffer | null, extraBytes: number }>}
 *   body is null when the framing has no body (HEAD, 204, 304).
 */
export function exchange({ host, port, method, target, fields, body, timeoutMs = DEFAULT_TIMEOUT_MS, limits = LIMITS }) {
  let bytes;
  try {
    bytes = serializeRequest({ method, target, fields, body });
  } catch (error) {
    return Promise.reject(error);
  }
  return new Promise((resolve, reject) => {
    const parser = new ResponseParser({ method, limits });
    let connected = false;
    let settled = false;
    const socket = connect({ host, port });
    const finish = (error, value) => {
      if (settled) return;
      settled = true;
      clearTimeout(timer);
      socket.destroy();
      if (error === null) resolve(value);
      else reject(error);
    };
    const timer = setTimeout(() => finish(new HttpError(HttpCode.TIMEOUT, `no complete response within ${timeoutMs} ms`)), timeoutMs);
    socket.on('connect', () => {
      connected = true;
      socket.write(bytes);
    });
    socket.on('data', (chunk) => {
      try {
        if (parser.push(chunk)) finish(null, parser.result());
      } catch (error) {
        finish(error);
      }
    });
    socket.on('end', () => {
      try {
        finish(null, parser.end());
      } catch (error) {
        finish(error);
      }
    });
    socket.on('error', (error) => {
      const code = connected ? HttpCode.NETWORK : HttpCode.CONNECT;
      const what = connected ? 'the connection failed' : `cannot connect to ${host}:${port}`;
      finish(new HttpError(code, `${what} (${error.code ?? error.name})`, { cause: error }));
    });
    socket.on('close', () => finish(new HttpError(HttpCode.CLOSED, 'the connection closed without a response')));
  });
}
