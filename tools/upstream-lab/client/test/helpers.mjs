// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

import http from 'node:http';
import { EventLog } from '../src/eventlog.mjs';
import { formatExchange } from '../src/httpfile.mjs';
import { Proxy } from '../src/proxy.mjs';

/** A fake lab server on a free loopback port. `handler(req, res, body)` answers. */
export async function startUpstream(handler) {
  const seen = [];
  const server = http.createServer((req, res) => {
    const chunks = [];
    req.on('data', (c) => chunks.push(c));
    req.on('end', () => {
      const body = Buffer.concat(chunks);
      seen.push({ method: req.method, url: req.url, rawHeaders: req.rawHeaders, body });
      handler(req, res, body);
    });
  });
  await new Promise((resolve) => server.listen(0, '127.0.0.1', resolve));
  return {
    port: server.address().port,
    seen,
    close: () => new Promise((resolve) => { server.close(() => resolve()); server.closeAllConnections(); }),
  };
}

/** Send one request. Resolves with the head, the body and whether the body was complete. */
export function send({ port, method = 'GET', path = '/', headers = [], body = null, onData = null }) {
  return new Promise((resolve, reject) => {
    const req = http.request({ host: '127.0.0.1', port, method, path, headers: ['Host', `127.0.0.1:${port}`, ...headers], agent: false }, (res) => {
      const chunks = [];
      res.on('data', (c) => {
        chunks.push(c);
        onData?.(Buffer.concat(chunks));
      });
      res.on('error', () => {});
      res.on('close', () => {
        resolve({ status: res.statusCode, rawHeaders: res.rawHeaders, body: Buffer.concat(chunks), complete: res.complete });
      });
    });
    req.on('error', reject);
    req.end(body);
  });
}

/** A proxy with its event log and the stored exchange files, on free ports. */
export async function startProxy(upstreamPort, options = {}) {
  const log = new EventLog();
  const files = new Map();
  const records = new Map();
  const proxy = new Proxy({
    upstreamPort,
    listenPort: 0,
    emit: (ev, members) => log.emit('proxy', ev, members),
    onExchange: (rec) => {
      records.set(rec.exchange, rec);
      files.set(rec.exchange, formatExchange(rec));
    },
    ...options,
  });
  await proxy.start();
  return { proxy, log, files, records };
}

export const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

/** Wait until `fn()` is truthy (poll every 5 ms, give up after 3 s). */
export async function until(fn) {
  const end = Date.now() + 3000;
  while (!fn()) {
    if (Date.now() > end) throw new Error('until: timeout');
    await sleep(5);
  }
}
