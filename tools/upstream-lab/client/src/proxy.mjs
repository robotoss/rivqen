// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// Recording proxy (CONTRACT sections 1, 3 and 5). It listens on 127.0.0.1 only and forwards to
// one lab server on 127.0.0.1. It buffers each upstream response (at most 8 MiB) and then
// delivers it, so that a fault step can hold or cut the delivery at an exact byte.
import http from 'node:http';
import { createHash } from 'node:crypto';
import { LabError } from './errors.mjs';

export const MAX_EXCHANGE_BYTES = 8 * 1024 * 1024;
const LOOPBACK = '127.0.0.1';
// Fields that belong to one connection. The proxy does not forward them; Node.js frames each side itself.
const HOP_BY_HOP = new Set(['connection', 'keep-alive', 'proxy-connection', 'te', 'trailer', 'transfer-encoding', 'upgrade']);

const lower = (s) => s.toLowerCase();
export const sha256 = (buf) => createHash('sha256').update(buf).digest('hex');

/** Flat raw header list of Node.js ([name, value, name, value, ...]) to [name, value] pairs. */
export function pairs(raw) {
  const out = [];
  for (let i = 0; i + 1 < raw.length; i += 2) out.push([raw[i], raw[i + 1]]);
  return out;
}

/** Fields to forward: no hop-by-hop field and no field named in `drop` (ASCII case-insensitive). */
export function forwardFields(fields, drop = []) {
  const dropped = new Set(drop.map(lower));
  return fields.filter(([name]) => !HOP_BY_HOP.has(lower(name)) && !dropped.has(lower(name)));
}

/** Collect a stream into one Buffer; fails with TOO_LARGE above `max` bytes. */
export function readBounded(stream, max) {
  return new Promise((resolve, reject) => {
    const chunks = [];
    let total = 0;
    stream.on('data', (chunk) => {
      total += chunk.length;
      if (total > max) {
        reject(new LabError('TOO_LARGE', `more than ${max} bytes`));
        stream.destroy();
        return;
      }
      chunks.push(chunk);
    });
    stream.on('end', () => resolve(Buffer.concat(chunks)));
    stream.on('error', (cause) => reject(new LabError('STREAM', 'stream error', { cause })));
    stream.on('aborted', () => reject(new LabError('STREAM', 'stream aborted')));
  });
}

function fetchUpstream({ port, method, target, fields, body, max, timeoutMs }) {
  return new Promise((resolve, reject) => {
    const fail = (cause) => reject(cause instanceof LabError ? cause : new LabError('UPSTREAM', `upstream ${LOOPBACK}:${port}: ${cause.code ?? cause.message}`, { cause }));
    // A flat header list gets no automatic Host field; HTTP/1.1 needs one.
    const withHost = fields.some(([n]) => lower(n) === 'host') ? fields : [...fields, ['Host', `${LOOPBACK}:${port}`]];
    const req = http.request(
      { host: LOOPBACK, port, method, path: target, headers: withHost.flat(), agent: false, timeout: timeoutMs },
      (res) => {
        readBounded(res, max).then(
          (data) => resolve({ status: res.statusCode, message: res.statusMessage, fields: pairs(res.rawHeaders), body: data }),
          fail,
        );
      },
    );
    req.on('timeout', () => req.destroy(new LabError('UPSTREAM_TIMEOUT', `upstream did not answer within ${timeoutMs} ms`)));
    req.on('error', fail);
    req.end(body);
  });
}

class Hold {
  released = false;
  exchange = null;
  #resolve;
  #reject;
  constructor(point, bytes) {
    this.point = point;
    this.bytes = bytes;
    this.promise = new Promise((resolve, reject) => {
      this.#resolve = resolve;
      this.#reject = reject;
    });
    this.promise.catch(() => {});
  }
  release() {
    this.released = true;
    this.#resolve();
  }
  abort(error) {
    this.#reject(error);
  }
}

const noFaults = () => ({ hold: null, cut: null, drop: [] });

export class Proxy {
  #upstreamPort;
  #listenPort;
  #emit;
  #onExchange;
  #max;
  #timeoutMs;
  #server = null;
  #count = 0;
  #pending = noFaults();
  #heldRef = null;
  #holds = new Set();
  #inflight = new Set();

  /**
   * @param {{
   *   upstreamPort: number, listenPort: number,
   *   emit: (ev: string, members: Record<string, unknown>) => unknown,
   *   onExchange?: (record: object) => void,
   *   maxBytes?: number, upstreamTimeoutMs?: number
   * }} options
   */
  constructor({ upstreamPort, listenPort, emit, onExchange = () => {}, maxBytes = MAX_EXCHANGE_BYTES, upstreamTimeoutMs = 20_000 }) {
    this.#upstreamPort = upstreamPort;
    this.#listenPort = listenPort;
    this.#emit = emit;
    this.#onExchange = onExchange;
    this.#max = maxBytes;
    this.#timeoutMs = upstreamTimeoutMs;
  }

  /** The port that the proxy listens on (known after `start`; useful with port 0). */
  get port() {
    return this.#listenPort;
  }

  get listening() {
    return this.#server !== null;
  }

  async start() {
    if (this.#server) return;
    const server = http.createServer((req, res) => {
      const job = this.#serve(req, res)
        .catch(() => res.destroy())
        .finally(() => this.#inflight.delete(job));
      this.#inflight.add(job);
    });
    await new Promise((resolve, reject) => {
      server.once('error', (cause) => reject(new LabError('LISTEN', `cannot listen on ${LOOPBACK}:${this.#listenPort}`, { cause })));
      server.listen(this.#listenPort, LOOPBACK, resolve);
    });
    this.#listenPort = server.address().port;
    this.#server = server;
  }

  /** Close the listener and every open connection (the `server_down` step). Held exchanges are aborted. */
  async stop() {
    const server = this.#server;
    if (!server) return;
    this.#server = null;
    for (const hold of this.#holds) hold.abort(new LabError('ABORTED', 'proxy stopped'));
    this.#holds.clear();
    this.#heldRef = null;
    await new Promise((resolve) => {
      server.close(() => resolve());
      server.closeAllConnections();
    });
    await Promise.allSettled([...this.#inflight]);
  }

  /** Arm a fault for the next exchange that starts after this call (steps hold, cut, drop_field). */
  arm(step) {
    switch (step.do) {
      case 'hold': {
        if (this.#heldRef) throw new LabError('HOLD_PENDING', 'a hold is already armed or active');
        const hold = new Hold(step.point, step.bytes ?? 0);
        this.#pending.hold = hold;
        this.#heldRef = hold;
        this.#holds.add(hold);
        return;
      }
      case 'cut':
        this.#pending.cut = { bytes: step.bytes };
        return;
      case 'drop_field':
        this.#pending.drop.push(step.name);
        return;
      default:
        throw new LabError('BAD_FAULT', `not a fault step: ${step.do}`);
    }
  }

  /** The `release` step: let the held exchange go on. */
  release() {
    const hold = this.#heldRef;
    if (!hold) throw new LabError('NO_HOLD', 'no hold to release');
    this.#heldRef = null;
    this.#holds.delete(hold);
    this.#emit('proxy.release', { exchange: hold.exchange, point: hold.point });
    hold.release();
  }

  async #hold(exchange, hold) {
    hold.exchange = exchange;
    this.#emit('proxy.hold', { exchange, point: hold.point, ...(hold.point === 'body' ? { bytes: hold.bytes } : {}) });
    await hold.promise;
  }

  async #serve(req, res) {
    const exchange = ++this.#count;
    const faults = this.#pending;
    this.#pending = noFaults();
    const rec = {
      exchange,
      method: req.method,
      target: req.url,
      requestFields: pairs(req.rawHeaders),
      requestBody: Buffer.alloc(0),
      response: null,
      delivered: null,
      error: null,
    };
    const state = { headSent: false, endSent: false, sent: Buffer.alloc(0) };
    this.#emit('proxy.request', { exchange, method: rec.method, target: rec.target, fields: rec.requestFields });
    try {
      rec.requestBody = await readBounded(req, this.#max);
      if (faults.hold?.point === 'request') await this.#hold(exchange, faults.hold);
      rec.response = await fetchUpstream({
        port: this.#upstreamPort,
        method: rec.method,
        target: rec.target,
        fields: forwardFields(rec.requestFields),
        body: rec.requestBody,
        max: this.#max,
        timeoutMs: this.#timeoutMs,
      });
      if (faults.hold?.point === 'head') await this.#hold(exchange, faults.hold);
      await this.#deliver(res, rec, faults, state);
    } catch (cause) {
      rec.error = cause instanceof LabError ? `${cause.code}: ${cause.message}` : 'INTERNAL';
      this.#fail(res, rec, state);
    } finally {
      this.#onExchange(rec);
    }
  }

  async #deliver(res, rec, faults, state) {
    const { exchange } = rec;
    const { status, message, body } = rec.response;
    const fields = forwardFields(rec.response.fields, faults.drop);
    for (const name of faults.drop) this.#emit('proxy.drop_field', { exchange, name });
    rec.delivered = { status, fields, body: Buffer.alloc(0) };
    res.sendDate = false;
    res.writeHead(status, message, fields.flat());
    this.#emit('proxy.response_head', { exchange, status, fields });
    state.headSent = true;
    res.flushHeaders();

    const stops = [];
    if (faults.hold?.point === 'body') stops.push({ at: Math.min(faults.hold.bytes, body.length), hold: faults.hold });
    if (faults.cut) stops.push({ at: Math.min(faults.cut.bytes, body.length), cut: faults.cut });
    stops.sort((a, b) => a.at - b.at);
    let pos = 0;
    const write = (chunk) => new Promise((resolve, reject) => res.write(chunk, (err) => (err ? reject(err) : resolve())));
    for (const stop of stops) {
      if (stop.at > pos) {
        await write(body.subarray(pos, stop.at));
        pos = stop.at;
        state.sent = body.subarray(0, pos);
      }
      if (stop.hold) {
        await this.#hold(exchange, stop.hold);
      } else {
        this.#emit('proxy.cut', { exchange, bytes: stop.cut.bytes });
        rec.delivered.body = body.subarray(0, pos);
        res.socket.end();
        this.#end(rec, state, rec.delivered.body, false);
        return;
      }
    }
    if (body.length > pos) await write(body.subarray(pos));
    await new Promise((resolve) => res.end(resolve));
    rec.delivered.body = body;
    // CONTRACT section 5: only a drop_field or cut exchange gets a `--- delivered ---` section.
    if (faults.drop.length === 0) rec.delivered = null;
    this.#end(rec, state, body, true);
  }

  #end(rec, state, body, complete) {
    state.endSent = true;
    this.#emit('proxy.response_end', { exchange: rec.exchange, body_len: body.length, body_sha256: sha256(body), complete });
  }

  #fail(res, rec, state) {
    if (!state.headSent) {
      this.#emit('proxy.response_head', { exchange: rec.exchange, status: 502, fields: [] });
      if (!res.headersSent && !res.destroyed) {
        res.sendDate = false;
        res.writeHead(502, { 'content-length': '0' });
        res.end();
      }
    } else {
      res.destroy();
    }
    if (!state.endSent) this.#end(rec, state, state.sent, false);
  }
}
