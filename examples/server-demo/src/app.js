// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors
//
// HTTP adapter of the Rivqen protocol (RQP) demo server. It is thin: it maps a Node.js
// request to the request of handle() (src/rqp.js) and writes the result.
//   GET  /catalog              the demo page (full document, 304, or delta)
//   POST /__demo/next-data     change price/stock (localhost only)
//   POST /__demo/next-template change the template (localhost only)
// The POST endpoints need the header "Rq-Demo-Action: 1" and, when the request has an
// Origin header, the origin of the demo. Otherwise: 403, no state change.
// Every route answers only a request whose Host header is the host (with port) of the
// configured origin. Otherwise: 421, no body, no state change (DL-036 I4).
//   GET  /healthz

import { createServer } from 'node:http';
import { randomUUID } from 'node:crypto';
import { requestLines } from './fields.js';
import { handle } from './rqp.js';
import { renderCatalog, nextData, nextTemplate, demoState } from './pages.js';
import { log, logError, safeUrl } from './log.js';

/** Manifest members of this server. They are part of the page revision (H-55). */
export const CONFIG = Object.freeze({
  policy: { mode: 'private', ttl_seconds: 0, offline: false },
  limits: { max_patch_bytes: 262144 },
});

/** Fields of every response of the demo. */
const COMMON = { 'X-Content-Type-Options': 'nosniff' };

/** The response of the application for the catalog page, before the RQP step. */
const appResponse = () => ({
  status: 200,
  headers: [
    ['content-type', 'text/html; charset=utf-8'],
    ['cache-control', 'no-cache'],
    ['x-content-type-options', 'nosniff'],
  ],
});

/** Write a response of handle() and return the number of body bytes. */
function sendResult(res, result) {
  res.writeHead(result.status, result.headers.flat());
  res.end(result.body ?? undefined);
  return result.body ? result.body.length : 0;
}

function send(res, status, headers, body) {
  const buf = body === undefined ? undefined : Buffer.from(body, 'utf8');
  res.writeHead(status, { ...COMMON, ...headers, ...(buf ? { 'Content-Length': buf.length } : {}) });
  res.end(buf);
  return buf ? buf.length : 0;
}

function isLocal(req) {
  const a = req.socket.remoteAddress ?? '';
  return a === '127.0.0.1' || a === '::1' || a === '::ffff:127.0.0.1';
}

/** The custom request header of the POST endpoints. A web page of another site cannot send it without a CORS preflight, which the demo never answers. */
export const ACTION_HEADER = 'rq-demo-action';
export const ACTION_VALUE = '1';

const DEMO_PATHS = new Set(['/__demo/next-data', '/__demo/next-template']);
const KNOWN_PATHS = new Set(['/catalog', '/healthz', ...DEMO_PATHS]);

/** The path as a log value: a known route, or a fixed word. A raw path can hold personal data. */
export const pathTemplate = (path) => (KNOWN_PATHS.has(path) ? path : '<unmatched>');

/**
 * Cross-site protection of the state-changing endpoints: the custom header must be present,
 * and an Origin header, when present, must be the origin of this demo.
 */
export function isSameSiteAction(req, origin) {
  if (req.headers[ACTION_HEADER] !== ACTION_VALUE) return false;
  return req.headers.origin === undefined || req.headers.origin === origin;
}

/** DL-036 I4: true when the Host header is the host (with port) of the configured origin. Host names are case-insensitive. */
export const isOwnHost = (req, host) => typeof req.headers.host === 'string' && req.headers.host.toLowerCase() === host;

/**
 * Create the demo server (not listening yet).
 * @param {{ origin: string }} options the public origin of the site: the manifest takes
 *   `origin` from here, not from the connection (manifest-patch section 1.6)
 */
export function createDemoServer({ origin }) {
  const parsed = new URL(origin);
  const ownOrigin = parsed.origin; // normalized once: the Origin header of a browser is in this form
  const ownHost = parsed.host;

  function handleCatalog(req, res, ctx) {
    const request = { method: req.method, target: `${origin}/catalog`, headers: requestLines(req.rawHeaders) };
    const result = handle(request, appResponse(), Buffer.from(renderCatalog(), 'utf8'), CONFIG);
    ctx.mode = result.mode;
    ctx.reason = result.reason;
    if (result.error) logError('rqp.passthrough', result.error, { request_id: ctx.id, path: '/catalog' });
    return sendResult(res, result);
  }

  return createServer((req, res) => {
    const ctx = { id: randomUUID().slice(0, 8), start: process.hrtime.bigint(), mode: '-' };
    let bytes = 0;
    try {
      const path = safeUrl(req.url ?? '/');
      if (!isOwnHost(req, ownHost)) {
        res.writeHead(421, { ...COMMON, 'Content-Length': 0 }); // no detail, no state change
        res.end();
      } else if ((req.method === 'GET' || req.method === 'HEAD') && path === '/catalog') {
        bytes = handleCatalog(req, res, ctx);
      } else if (req.method === 'POST' && DEMO_PATHS.has(path) && isLocal(req)) {
        if (!isSameSiteAction(req, ownOrigin)) {
          bytes = send(res, 403, { 'Content-Type': 'text/plain' }, 'forbidden'); // no state change
        } else {
          const next = path === '/__demo/next-data' ? nextData : nextTemplate;
          bytes = send(res, 200, { 'Content-Type': 'application/json' }, JSON.stringify(next()));
        }
      } else if (req.method === 'GET' && path === '/healthz') {
        bytes = send(res, 200, { 'Content-Type': 'application/json' }, JSON.stringify({ ok: true, state: demoState() }));
      } else {
        bytes = send(res, 404, { 'Content-Type': 'text/plain' }, 'not found');
      }
    } catch (err) {
      logError('http.unhandled', err, { request_id: ctx.id });
      if (!res.headersSent) bytes = send(res, 500, { 'Content-Type': 'text/plain' }, 'internal error');
      else res.destroy();
    } finally {
      const ms = Number(process.hrtime.bigint() - ctx.start) / 1e6;
      log('info', 'http.request', {
        request_id: ctx.id, method: req.method, path: pathTemplate(safeUrl(req.url ?? '/')), status: res.statusCode,
        mode: ctx.mode, reason: ctx.reason, bytes, duration_ms: Math.round(ms * 100) / 100,
      });
    }
  });
}
