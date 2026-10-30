// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// Conformance harness: serves a `handle()` module over HTTP and lets the
// runner set the case through the control endpoint (CONTRACT.md sections 2, 5).
// TEST CODE ONLY. It must never ship or run on a public interface.

import http from 'node:http';
import { resolve } from 'node:path';
import { pathToFileURL } from 'node:url';
import { parseArgs } from 'node:util';
import { escapeText } from '../src/escape.mjs';

export const CONTROL_PREFIX = '/__rivqen/conformance/';
export const INFO_PATH = `${CONTROL_PREFIX}info`;
export const CASE_PATH = `${CONTROL_PREFIX}case`;
export const MAX_CASE_BYTES = 8 * 1024 * 1024;
export const DEFAULT_ORIGIN = 'https://shop.example.com';
export const LEGACY_PROFILES = ['legacy-canonical', 'legacy-java', 'legacy-node', 'legacy-php'];

export class HarnessError extends Error {
  constructor(code, message, options) {
    super(message, options);
    this.name = 'HarnessError';
    this.code = code;
  }
}

/** Pure check of the remote address (CONTRACT.md section 2). */
export function isLoopback(remoteAddress) {
  return remoteAddress === '127.0.0.1' || remoteAddress === '::1' || remoteAddress === '::ffff:127.0.0.1';
}

/**
 * Gate of a control request. Returns the HTTP status that refuses it, or 0 when it may go on.
 * Off without the flag: 404 (the path is unknown). Not from loopback: 403.
 */
export function controlGate(conformance, remoteAddress) {
  if (!conformance) return 404;
  if (!isLoopback(remoteAddress)) return 403;
  return 0;
}

/** Pure check of the Host of a control request: loopback name with the port of the harness (DL-078, L1). */
export function isControlHost(host, port) {
  if (typeof host !== 'string') return false;
  const h = host.replace(/[A-Z]/g, (c) => c.toLowerCase());
  return h === `127.0.0.1:${port}` || h === `localhost:${port}` || h === `[::1]:${port}`;
}

/** Pure check of a Content-Type value: media type application/json (ASCII case-insensitive), parameters allowed. */
export function isJsonContentType(value) {
  if (typeof value !== 'string') return false;
  const type = value.split(';')[0].replace(/^[ \t]+|[ \t]+$/g, '');
  return type.replace(/[A-Z]/g, (c) => c.toLowerCase()) === 'application/json';
}

export function profilesOf(suite) {
  return suite === 'legacy' ? [...LEGACY_PROFILES] : [];
}

const isObject = (v) => typeof v === 'object' && v !== null && !Array.isArray(v);
const isPairList = (v) =>
  Array.isArray(v) && v.every((p) => Array.isArray(p) && p.length === 2 && typeof p[0] === 'string' && typeof p[1] === 'string');
// RFC 9110 section 5.6.2 token; section 5.5 field value: HTAB, SP, VCHAR, obs-text.
const TOKEN = /^[!#$%&'*+\-.^_`|~0-9A-Za-z]+$/;
const FIELD_VALUE = /^[\t\x20-\x7e\x80-\xff]*$/;
const isWirePairList = (v) => isPairList(v) && v.every(([name, value]) => TOKEN.test(name) && FIELD_VALUE.test(value));

function decodeBase64(text) {
  if (typeof text !== 'string' || text.length % 4 !== 0 || !/^[A-Za-z0-9+/]*={0,2}$/.test(text)) return null;
  return Buffer.from(text, 'base64');
}

/**
 * Parse a case body. Returns { case } or { error: 'BAD_CASE' | 'UNSUPPORTED' }.
 * BAD_CASE (invalid JSON or member) is checked before UNSUPPORTED.
 */
export function parseCase(text, suite) {
  let json;
  try {
    json = JSON.parse(text);
  } catch {
    return { error: 'BAD_CASE' };
  }
  if (!isObject(json)) return { error: 'BAD_CASE' };
  if (json.suite !== 'rqp' && json.suite !== 'legacy') return { error: 'BAD_CASE' };
  if (typeof json.case_id !== 'string') return { error: 'BAD_CASE' };
  if (json.suite === 'legacy' && typeof json.profile !== 'string') return { error: 'BAD_CASE' };
  const page = decodeBase64(json.page_base64);
  if (page === null) return { error: 'BAD_CASE' };
  const app = json.app;
  if (!isObject(app) || !Number.isInteger(app.status) || app.status < 100 || app.status > 599 || !isWirePairList(app.headers)) {
    return { error: 'BAD_CASE' };
  }
  const serverConfig = json.server_config === undefined ? {} : json.server_config;
  if (!isObject(serverConfig)) return { error: 'BAD_CASE' };
  if (json.suite !== suite) return { error: 'UNSUPPORTED' };
  if (json.suite === 'legacy' && !LEGACY_PROFILES.includes(json.profile)) return { error: 'UNSUPPORTED' };
  return {
    case: {
      suite: json.suite,
      caseId: json.case_id,
      profile: json.suite === 'legacy' ? json.profile : undefined,
      page: new Uint8Array(page.buffer, page.byteOffset, page.length),
      app: { status: app.status, headers: app.headers },
      serverConfig,
    },
  };
}

/** Turn the flat Node raw header array into [[name, value], ...], keeping order, case and repeats. */
export function pairsOfRaw(rawHeaders) {
  const pairs = [];
  for (let i = 0; i + 1 < rawHeaders.length; i += 2) pairs.push([rawHeaders[i], rawHeaders[i + 1]]);
  return pairs;
}

function sendJson(res, status, value) {
  const body = Buffer.from(JSON.stringify(value), 'utf8');
  res.writeHead(status, ['Content-Type', 'application/json', 'Content-Length', String(body.length)]);
  res.end(body);
}

// Reads the whole body but keeps at most `limit` bytes. An over-limit body is read to its end and
// dropped, so the 413 reaches the client before the connection closes (an early close would reset it).
function readBody(req, limit) {
  return new Promise((resolveBody, reject) => {
    const chunks = [];
    let size = 0;
    req.on('data', (chunk) => {
      size += chunk.length;
      if (size <= limit) chunks.push(chunk);
      else chunks.length = 0;
    });
    req.on('end', () => {
      if (size > limit) resolveBody({ tooLarge: true });
      else resolveBody({ text: Buffer.concat(chunks).toString('utf8') });
    });
    req.on('error', (cause) => reject(new HarnessError('READ_FAILED', 'cannot read the request body', { cause })));
  });
}

/**
 * Create the harness server (not listening yet).
 * @param {{ handle: Function, suite: 'rqp'|'legacy', origin?: string, conformance?: boolean }} options
 */
export function createHarness({ handle, suite, origin = DEFAULT_ORIGIN, conformance = false }) {
  if (typeof handle !== 'function') throw new HarnessError('BAD_MODULE', 'handle must be a function');
  if (suite !== 'rqp' && suite !== 'legacy') throw new HarnessError('BAD_SUITE', `unknown suite: ${String(suite)}`);
  const base = origin.replace(/\/+$/, '');
  let stored = null;

  async function control(req, res, path) {
    const refused = controlGate(conformance, req.socket.remoteAddress);
    if (refused !== 0) {
      sendJson(res, refused, { error: refused === 404 ? 'NOT_FOUND' : 'FORBIDDEN' });
      return;
    }
    if (!isControlHost(req.headers.host, req.socket.localPort)) return sendJson(res, 403, { error: 'FORBIDDEN' });
    if (path === INFO_PATH) {
      if (req.method !== 'GET') return sendJson(res, 405, { error: 'METHOD_NOT_ALLOWED' });
      return sendJson(res, 200, { contract: 1, suites: [suite], profiles: profilesOf(suite) });
    }
    if (path === CASE_PATH) {
      if (req.method !== 'POST') return sendJson(res, 405, { error: 'METHOD_NOT_ALLOWED' });
      if (!isJsonContentType(req.headers['content-type'])) return sendJson(res, 415, { error: 'UNSUPPORTED_MEDIA_TYPE' });
      const body = await readBody(req, MAX_CASE_BYTES);
      if (body.tooLarge) {
        return sendJson(res, 413, { error: 'TOO_LARGE' });
      }
      const parsed = parseCase(body.text, suite);
      if (parsed.error === 'BAD_CASE') return sendJson(res, 400, { error: 'BAD_CASE' });
      if (parsed.error === 'UNSUPPORTED') return sendJson(res, 422, { error: 'UNSUPPORTED' });
      stored = parsed.case;
      res.writeHead(204);
      res.end();
      return;
    }
    return sendJson(res, 404, { error: 'NOT_FOUND' });
  }

  async function serveCase(req, res) {
    if (stored === null) return sendJson(res, 503, { error: 'NO_CASE' });
    const c = stored;
    try {
      const request = { method: req.method, target: base + req.url, headers: pairsOfRaw(req.rawHeaders) };
      const config = c.suite === 'legacy' ? { ...c.serverConfig, profile: c.profile } : c.serverConfig;
      const result = await handle(request, c.app, c.page, config);
      if (!isObject(result) || !isPairList(result.headers)) {
        process.stderr.write(`handle result is not valid for ${escapeText(c.caseId)}: BAD_RESULT\n`);
        return sendJson(res, 500, { error: 'HANDLE_FAILED' });
      }
      const lines = [];
      let hasLength = false;
      for (const [name, value] of result.headers) {
        if (name.toLowerCase() === 'content-length') hasLength = true;
        lines.push(name, value);
      }
      const body = result.body ?? null;
      const noBody = req.method === 'HEAD' || result.status === 304 || body === null;
      if (!hasLength && !noBody) lines.push('Content-Length', String(body.length));
      res.sendDate = false;
      res.writeHead(result.status, lines);
      if (noBody) res.end();
      else res.end(body);
    } catch (cause) {
      // Only the case id and an error code go to the log; page bytes never do.
      process.stderr.write(`handle failed for ${escapeText(c.caseId)}: ${escapeText(cause?.code ?? cause?.name ?? 'Error')}\n`);
      if (!res.headersSent) sendJson(res, 500, { error: 'HANDLE_FAILED' });
      else res.destroy();
    }
  }

  return http.createServer((req, res) => {
    const path = (req.url ?? '').split('?')[0];
    const run = path.startsWith(CONTROL_PREFIX) ? control(req, res, path) : serveCase(req, res);
    run.catch((cause) => {
      process.stderr.write(`harness error: ${escapeText(cause?.code ?? 'Error')}\n`);
      if (!res.headersSent) sendJson(res, 500, { error: 'HARNESS_FAILED' });
      else res.destroy();
    });
  });
}

export async function loadHandle(modulePath) {
  let mod;
  try {
    mod = await import(pathToFileURL(resolve(modulePath)).href);
  } catch (cause) {
    throw new HarnessError('BAD_MODULE', `cannot load module ${modulePath}`, { cause });
  }
  if (typeof mod.handle !== 'function') throw new HarnessError('BAD_MODULE', `module ${modulePath} has no handle() export`);
  return mod.handle;
}

export function parseCli(argv, env = {}) {
  let values;
  try {
    ({ values } = parseArgs({
      args: argv,
      options: {
        module: { type: 'string' },
        suite: { type: 'string' },
        port: { type: 'string', default: '0' },
        origin: { type: 'string', default: DEFAULT_ORIGIN },
        conformance: { type: 'boolean', default: false },
      },
      strict: true,
    }));
  } catch (cause) {
    throw new HarnessError('USAGE', cause.message, { cause });
  }
  if (!values.module) throw new HarnessError('USAGE', '--module is required');
  if (values.suite !== 'rqp' && values.suite !== 'legacy') throw new HarnessError('USAGE', '--suite must be rqp or legacy');
  const port = Number(values.port);
  if (!Number.isInteger(port) || port < 0 || port > 65535) throw new HarnessError('USAGE', '--port must be 0..65535');
  return {
    module: values.module,
    suite: values.suite,
    port,
    origin: values.origin,
    conformance: values.conformance || env.RIVQEN_CONFORMANCE === '1',
  };
}

/** Listen on 127.0.0.1 and print `listening <port>` as the first stdout line. */
export function listen(server, port, out = process.stdout) {
  return new Promise((resolveListen, reject) => {
    server.once('error', (cause) => reject(new HarnessError('LISTEN_FAILED', 'cannot listen', { cause })));
    server.listen(port, '127.0.0.1', () => {
      out.write(`listening ${server.address().port}\n`);
      resolveListen(server);
    });
  });
}

export async function main(argv, env, out) {
  const opts = parseCli(argv, env);
  const handle = await loadHandle(opts.module);
  const server = createHarness({ handle, suite: opts.suite, origin: opts.origin, conformance: opts.conformance });
  await listen(server, opts.port, out);
  return server;
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  main(process.argv.slice(2), process.env, process.stdout).catch((err) => {
    process.stderr.write(`harness: ${escapeText(err.code ?? 'ERROR')}: ${escapeText(err.message)}\n`);
    process.exit(2);
  });
}
