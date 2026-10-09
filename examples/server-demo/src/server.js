// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors
//
// Rivqen protocol (RQP) demo server.
//   GET  /catalog              the demo page (full, 304, or patch)
//   POST /__demo/next-data     change price/stock (localhost only)
//   POST /__demo/next-template change the template (localhost only)
//   GET  /healthz
//
// Run: npm start   (PORT=8787 by default, binds to 127.0.0.1)

import { createServer } from 'node:http';
import { randomUUID } from 'node:crypto';
import { analyze, buildPatch, injectManifest, manifest, readRequest, RQP_VERSION, DELTA_TYPE } from './rqp.js';
import { renderCatalog, nextData, nextTemplate, demoState } from './pages.js';
import { log, logError, safeUrl } from './log.js';

const PORT = Number(process.env.PORT ?? 8787);
const HOST = process.env.HOST ?? '127.0.0.1';
const ORIGIN = process.env.ORIGIN ?? `http://${HOST}:${PORT}`;
const KEEP_REVISIONS = 16;

/** Recent analyses per path, so the server can answer patches from a known base. */
const history = new Map();
const sequences = new Map();

function remember(path, analysis) {
  const list = history.get(path) ?? [];
  if (!list.some((a) => a.pageRevision === analysis.pageRevision)) list.push(analysis);
  while (list.length > KEEP_REVISIONS) list.shift();
  history.set(path, list);
}

function findBase(path, revision) {
  return (history.get(path) ?? []).find((a) => a.pageRevision === revision) ?? null;
}

const COMMON = {
  'Cache-Control': 'no-cache, private',
  Vary: 'Rq-Version, Rq-Capabilities, Rq-Base-Revision',
  'X-Content-Type-Options': 'nosniff',
};

function send(res, status, headers, body) {
  const buf = body === undefined ? undefined : Buffer.from(body, 'utf8');
  res.writeHead(status, { ...COMMON, ...headers, ...(buf ? { 'Content-Length': buf.length } : {}) });
  res.end(buf);
  return buf ? buf.length : 0;
}

function handleCatalog(req, res, ctx) {
  const path = '/catalog';
  const html = renderCatalog();
  let analysis;
  try {
    analysis = analyze(html);
  } catch (err) {
    // ERR-07: never break the page. Send the original HTML unchanged.
    logError('rqp.analyze_failed', err, { request_id: ctx.id, path });
    ctx.mode = 'passthrough';
    ctx.code = 'SERVER_PASSTHROUGH';
    return send(res, 200, { 'Content-Type': 'text/html; charset=utf-8' }, html);
  }
  remember(path, analysis);
  const rq = readRequest(req.headers);
  const etag = `"${analysis.pageRevision}"`;
  const rqHeaders = rq.isRqp
    ? { 'Rq-Version': String(RQP_VERSION), 'Rq-Revision': `"${analysis.pageRevision}"`, 'Rq-Template-Revision': `"${analysis.templateRevision}"` }
    : {};

  if (rq.ifNoneMatch.has(analysis.pageRevision)) {
    ctx.mode = '304';
    return send(res, 304, { ETag: etag, ...rqHeaders });
  }

  if (rq.isRqp && rq.caps.has('block-patch') && rq.base) {
    const base = findBase(path, rq.base);
    if (base) {
      const seq = (sequences.get(path) ?? 0) + 1;
      sequences.set(path, seq);
      const patch = buildPatch({ pageId: 'catalog', base, current: analysis, sequence: seq });
      if (patch) {
        ctx.mode = 'patch';
        ctx.operations = patch.operations.length;
        return send(res, 200, {
          'Content-Type': `${DELTA_TYPE}; charset=utf-8`,
          ETag: etag,
          ...rqHeaders,
          'Rq-Base-Revision': `"${base.pageRevision}"`,
          'Cache-Control': 'private, no-store',
        }, JSON.stringify(patch));
      }
      ctx.reason = 'template_changed';
    } else {
      ctx.reason = 'unknown_base';
    }
  }

  if (!rq.isRqp) {
    // S-002: clients without Rivqen get the HTML unchanged.
    ctx.mode = 'plain';
    return send(res, 200, { 'Content-Type': 'text/html; charset=utf-8', ETag: etag }, html);
  }
  ctx.mode = 'full';
  const m = manifest({ origin: ORIGIN, path, analysis, policy: { mode: 'private', ttl_seconds: 0, offline: false } });
  return send(res, 200, { 'Content-Type': 'text/html; charset=utf-8', ETag: etag, ...rqHeaders }, injectManifest(html, m));
}

function isLocal(req) {
  const a = req.socket.remoteAddress ?? '';
  return a === '127.0.0.1' || a === '::1' || a === '::ffff:127.0.0.1';
}

const server = createServer((req, res) => {
  const ctx = { id: randomUUID().slice(0, 8), start: process.hrtime.bigint(), mode: '-' };
  let bytes = 0;
  try {
    const path = safeUrl(req.url ?? '/');
    if (req.method === 'GET' && path === '/catalog') {
      bytes = handleCatalog(req, res, ctx);
    } else if (req.method === 'POST' && path === '/__demo/next-data' && isLocal(req)) {
      bytes = send(res, 200, { 'Content-Type': 'application/json' }, JSON.stringify(nextData()));
    } else if (req.method === 'POST' && path === '/__demo/next-template' && isLocal(req)) {
      bytes = send(res, 200, { 'Content-Type': 'application/json' }, JSON.stringify(nextTemplate()));
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
      request_id: ctx.id, method: req.method, path: safeUrl(req.url ?? '/'), status: res.statusCode,
      mode: ctx.mode, reason: ctx.reason, operations: ctx.operations, bytes, duration_ms: Math.round(ms * 100) / 100,
    });
  }
});

server.listen(PORT, HOST, () => log('info', 'server.started', { origin: ORIGIN }));
