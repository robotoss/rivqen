// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// Control channel (CONTRACT section 4). It listens on 127.0.0.1 only. The emulator reaches it as 10.0.2.2.
import http from 'node:http';
import { LabError } from './errors.mjs';
import { readBounded } from './proxy.mjs';

export const MAX_EVENT_BYTES = 1024 * 1024;
export const POLL_MS = 30_000;
const HOST_NAMES = ['127.0.0.1', 'localhost', '10.0.2.2'];

const json = (res, status, value) => {
  const text = JSON.stringify(value);
  res.writeHead(status, { 'content-type': 'application/json', 'content-length': Buffer.byteLength(text) });
  res.end(text);
};
const empty = (res, status) => {
  res.writeHead(status);
  res.end();
};

/**
 * @typedef {object} RunHandle
 * @property {string} id
 * @property {() => object} scenario  the body of GET /run/scenario
 * @property {(ms: number) => Promise<object|null>} poll
 * @property {(step: object) => void} [redeliver]  offer a polled step again (the answer was lost)
 * @property {(event: object) => void} accept  throws LabError BAD_EVENT
 */

export class Control {
  #port;
  #run;
  #server = null;
  #pollMs;

  /** @param {{ port: number, run: RunHandle, pollMs?: number }} options */
  constructor({ port, run, pollMs = POLL_MS }) {
    this.#port = port;
    this.#run = run;
    this.#pollMs = pollMs;
  }

  get port() {
    return this.#port;
  }

  async start() {
    const server = http.createServer((req, res) => {
      this.#route(req, res).catch(() => {
        if (!res.headersSent) json(res, 500, { error: 'INTERNAL' });
        else res.destroy();
      });
    });
    await new Promise((resolve, reject) => {
      server.once('error', (cause) => reject(new LabError('LISTEN', `cannot listen on 127.0.0.1:${this.#port}`, { cause })));
      server.listen(this.#port, '127.0.0.1', resolve);
    });
    this.#port = server.address().port;
    this.#server = server;
  }

  async stop() {
    const server = this.#server;
    if (!server) return;
    this.#server = null;
    await new Promise((resolve) => {
      server.close(() => resolve());
      server.closeAllConnections();
    });
  }

  async #route(req, res) {
    const url = new URL(req.url, 'http://control.invalid');
    const host = (req.headers.host ?? '').toLowerCase();
    // A browser page on another origin must not drive the lab (DNS rebinding): only the known host names.
    if (!HOST_NAMES.some((h) => host === `${h}:${this.#port}`)) return json(res, 403, { error: 'BAD_HOST' });
    const routes = { '/run/next': 'GET', '/run/event': 'POST', '/run/scenario': 'GET' };
    if (!Object.hasOwn(routes, url.pathname)) return json(res, 404, { error: 'NOT_FOUND' });
    if (req.method !== routes[url.pathname]) return json(res, 405, { error: 'METHOD' });
    const runParam = url.searchParams.get('run');
    if (runParam !== null && runParam !== this.#run.id) return json(res, 404, { error: 'UNKNOWN_RUN' });
    if (runParam === null && url.pathname !== '/run/event') return json(res, 400, { error: 'NO_RUN' });

    if (url.pathname === '/run/scenario') return json(res, 200, this.#run.scenario());
    if (url.pathname === '/run/next') {
      const step = await this.#run.poll(this.#pollMs);
      if (!step) return empty(res, 204);
      // The harness can drop the connection while it waits or before it reads the answer: the next poll gets the step again.
      if (res.destroyed) return this.#run.redeliver?.(step);
      res.once('close', () => {
        if (!res.writableFinished) this.#run.redeliver?.(step);
      });
      return json(res, 200, step);
    }
    if (!/^application\/json\s*(;|$)/i.test(req.headers['content-type'] ?? '')) return json(res, 415, { error: 'CONTENT_TYPE' });
    if (Number(req.headers['content-length']) > MAX_EVENT_BYTES) {
      // Read and drop the body (no buffer), so that the client gets the answer instead of a reset.
      res.setHeader('connection', 'close');
      await new Promise((resolve) => {
        req.on('end', resolve);
        req.on('error', resolve);
        req.resume();
      });
      return json(res, 413, { error: 'BODY' });
    }
    let body;
    try {
      body = await readBounded(req, MAX_EVENT_BYTES);
    } catch (cause) {
      return json(res, cause.code === 'TOO_LARGE' ? 413 : 400, { error: 'BODY' });
    }
    let event;
    try {
      event = JSON.parse(body.toString('utf8'));
    } catch {
      return json(res, 400, { error: 'BAD_EVENT' });
    }
    try {
      this.#run.accept(event);
    } catch (cause) {
      if (cause instanceof LabError && cause.code === 'BAD_EVENT') return json(res, 400, { error: 'BAD_EVENT', message: cause.message });
      throw cause;
    }
    return empty(res, 204);
  }
}
