// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// Header field bytes on HTTP/2. Header fields are never content-coded (gzip
// and Brotli do not apply to them); HTTP/2 compresses them with HPACK (RFC
// 7541). This module measures the size of the HPACK header block of each
// request and response that Node.js (nghttp2) writes: a local h2c server, a
// local client, and a TCP proxy between them that reads the HTTP/2 frames and
// adds the header block fragment lengths of HEADERS and CONTINUATION frames
// for each stream. Everything binds to 127.0.0.1; no other network is used.

import http2 from 'node:http2';
import net from 'node:net';
import { StudyError } from './errors.mjs';

/** A fixed Date value, so the response header blocks are the same in every run. */
export const FIXED_DATE = 'Tue, 20 Oct 2026 00:00:00 GMT';

const PREFACE_BYTES = 24; // "PRI * HTTP/2.0\r\n\r\nSM\r\n\r\n"
const FRAME_HEADER = 9;
const TYPE_HEADERS = 0x1;
const TYPE_CONTINUATION = 0x9;
const FLAG_PADDED = 0x8;
const FLAG_PRIORITY = 0x20;
/** Bound of the bytes that the counter buffers (one frame of the default maximum size is 16 KiB). */
const MAX_BUFFER = 1 << 20;

/** Reads HTTP/2 frames from one direction and sums the header block bytes per stream. */
export class FrameCounter {
  constructor(skipPreface) {
    this.buf = Buffer.alloc(0);
    this.skip = skipPreface ? PREFACE_BYTES : 0;
    this.blocks = new Map(); // stream id -> bytes of its first header block
    this.open = new Set(); // streams whose first header block is still open (no END_HEADERS)
  }

  feed(chunk) {
    this.buf = Buffer.concat([this.buf, chunk]);
    if (this.buf.length > MAX_BUFFER) throw new StudyError('STUDY_HPACK', 'frame buffer larger than the study bound');
    if (this.skip > 0) {
      const n = Math.min(this.skip, this.buf.length);
      this.buf = this.buf.subarray(n);
      this.skip -= n;
    }
    while (this.buf.length >= FRAME_HEADER) {
      const len = this.buf.readUIntBE(0, 3);
      if (this.buf.length < FRAME_HEADER + len) break;
      const type = this.buf[3];
      const flags = this.buf[4];
      const stream = this.buf.readUInt32BE(5) & 0x7fffffff;
      const payload = this.buf.subarray(FRAME_HEADER, FRAME_HEADER + len);
      if (type === TYPE_HEADERS && !this.blocks.has(stream)) {
        let n = len;
        if (flags & FLAG_PADDED) n -= 1 + payload[0];
        if (flags & FLAG_PRIORITY) n -= 5;
        this.blocks.set(stream, n);
        if ((flags & 0x4) === 0) this.open.add(stream);
      } else if (type === TYPE_CONTINUATION && this.open.has(stream)) {
        this.blocks.set(stream, this.blocks.get(stream) + len);
        if (flags & 0x4) this.open.delete(stream);
      }
      this.buf = this.buf.subarray(FRAME_HEADER + len);
    }
  }
}

function listen(server) {
  return new Promise((resolve, reject) => {
    server.once('error', reject);
    server.listen(0, '127.0.0.1', () => resolve(server.address().port));
  });
}

function closeServer(server) {
  return new Promise((resolve) => server.close(() => resolve()));
}

/**
 * Send the exchanges in order on ONE HTTP/2 connection. exchange = { request:
 * header object, response: header object with ':status' }. Returns, for each
 * exchange, the HPACK header block bytes of the request and of the response.
 */
export async function headerBlocks(exchanges) {
  if (exchanges.length === 0 || exchanges.length > 64) throw new StudyError('STUDY_HPACK', '1 to 64 exchanges per connection');
  const server = http2.createServer();
  let next = 0;
  server.on('stream', (stream) => {
    const ex = exchanges[next++];
    stream.respond({ ...ex.response, date: FIXED_DATE }, { endStream: true });
  });
  const serverPort = await listen(server);
  const toServer = new FrameCounter(true);
  const toClient = new FrameCounter(false);
  const sockets = new Set();
  const proxy = net.createServer((c) => {
    const s = net.connect(serverPort, '127.0.0.1');
    sockets.add(c).add(s);
    c.on('data', (d) => {
      toServer.feed(d);
      s.write(d);
    });
    s.on('data', (d) => {
      toClient.feed(d);
      c.write(d);
    });
    c.on('end', () => s.end());
    s.on('end', () => c.end());
    c.on('error', () => s.destroy());
    s.on('error', () => c.destroy());
  });
  const proxyPort = await listen(proxy);
  const client = http2.connect(`http://127.0.0.1:${proxyPort}`);
  try {
    for (const ex of exchanges) {
      await new Promise((resolve, reject) => {
        const req = client.request(ex.request, { endStream: true });
        req.on('response', () => {});
        req.on('data', () => {});
        req.on('end', resolve);
        req.on('error', reject);
      });
    }
  } finally {
    await new Promise((resolve) => client.close(resolve));
    for (const s of sockets) s.destroy();
    await closeServer(proxy);
    await closeServer(server);
  }
  return exchanges.map((_, i) => {
    const stream = 2 * i + 1;
    const request = toServer.blocks.get(stream);
    const response = toClient.blocks.get(stream);
    if (request === undefined || response === undefined) throw new StudyError('STUDY_HPACK', `no header block for stream ${stream}`);
    return { request, response };
  });
}

/** Header object from field lines (names in lower case). Repeated names are joined with ", ". */
export function headerObject(lines) {
  const out = {};
  for (const [n, v] of lines) out[n] = out[n] === undefined ? v : `${out[n]}, ${v}`;
  return out;
}

/** Bytes of field lines in HTTP/1.1 form: name ": " value CRLF. */
export function http1Bytes(lines) {
  return lines.reduce((n, [name, value]) => n + Buffer.byteLength(`${name}: ${value}\r\n`, 'latin1'), 0);
}
