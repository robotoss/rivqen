// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// Test helpers: a fake server under test with the control endpoint of
// CONTRACT.md section 2 (enough for these tests; the real harness is
// server/conformance/harness, lane T-04), and an in-process runner call.
// Every server listens on 127.0.0.1, port 0.

import { createServer } from 'node:http';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { gzipSync } from 'node:zlib';
import { main } from '../src/runner.mjs';

export const LEGACY_DIR = fileURLToPath(new URL('./fixtures/legacy/', import.meta.url));
export const RQP_DIR = fileURLToPath(new URL('../../../fixtures/rqp/protocol/', import.meta.url));
export const ALL_PROFILES = ['legacy-canonical', 'legacy-java', 'legacy-node', 'legacy-php'];

const flat = (lines) => lines.flatMap(([n, v]) => [n, v]);
const pairs = (raw) => raw.flatMap((x, i) => (i % 2 === 0 ? [[x, raw[i + 1]]] : []));

/**
 * Start a fake server. `respond(request, kase)` answers a case request, where
 * request = { method, url, fields } (fields: the raw field lines) and kase is
 * the body of the last POST /case. It returns one of:
 *   { status, headers: [[name, value]], body }  written with node:http (Content-Length added)
 *   { raw: Buffer }                             written to the socket as is, then the socket ends
 *   { hang: true }                              no answer
 * `state.raw` has the raw request bytes of each connection; `state.cases` the
 * stored cases; `state.requests` the case requests.
 */
export async function startFake({ info = { contract: 1, suites: ['rqp', 'legacy'], profiles: ALL_PROFILES }, respond, caseStatus } = {}) {
  const state = { raw: [], cases: [], requests: [], current: null };
  const server = createServer({ requireHostHeader: false }, async (req, res) => {
    const chunks = [];
    for await (const c of req) chunks.push(c);
    const body = Buffer.concat(chunks);
    if (req.url === '/__rivqen/conformance/info' && req.method === 'GET') {
      if (info === null) return send(res, { status: 404, headers: [], body: null });
      return send(res, { status: 200, headers: [['content-type', 'application/json']], body: Buffer.from(JSON.stringify(info)) });
    }
    if (req.url === '/__rivqen/conformance/case' && req.method === 'POST') {
      const kase = JSON.parse(body.toString('utf8'));
      state.cases.push(kase);
      const status = caseStatus?.(kase) ?? (info.suites.includes(kase.suite) ? 204 : 422);
      if (status === 204) state.current = kase;
      return send(res, { status, headers: [], body: null });
    }
    const request = { method: req.method, url: req.url, fields: pairs(req.rawHeaders), body };
    state.requests.push(request);
    let answer;
    try {
      answer = await respond(request, state.current);
    } catch {
      answer = { status: 500, headers: [], body: null };
    }
    if (answer.hang) return undefined;
    if (answer.raw !== undefined) {
      req.socket.end(answer.raw);
      return undefined;
    }
    return send(res, answer);
  });
  server.on('connection', (socket) => {
    const chunks = [];
    state.raw.push(chunks);
    socket.on('data', (c) => chunks.push(c));
  });
  await new Promise((r) => server.listen(0, '127.0.0.1', r));
  const { port } = server.address();
  return {
    state,
    port,
    target: `http://127.0.0.1:${port}`,
    rawRequest: (i) => Buffer.concat(state.raw[i]).toString('latin1'),
    close: () =>
      new Promise((r) => {
        server.closeAllConnections();
        server.close(r);
      }),
  };
}

function send(res, { status, headers, body }) {
  const lines = [...headers];
  const has = (name) => lines.some(([n]) => n.toLowerCase() === name);
  if (!has('content-length') && !has('transfer-encoding')) lines.push(['content-length', String(body?.length ?? 0)]);
  res.writeHead(status, flat(lines));
  res.end(body ?? undefined);
}

/** Run the command line in process: { code, out, err, lines }. */
export async function runMain(args) {
  let out = '';
  let err = '';
  const code = await main(args, { stdout: { write: (s) => (out += s) }, stderr: { write: (s) => (err += s) } });
  return { code, out, err, lines: out.split('\n').filter((l) => l !== '') };
}

export const readExpected = (dir, id) => JSON.parse(readFileSync(`${dir}${id}/expected.json`, 'utf8'));

/**
 * A conforming legacy response built from expected.json of the stored case
 * (the fake knows the answers; the tests change one part of it).
 * Options: gzip (encode the body), change(response) (edit before sending).
 */
export function legacyResponder({ dir = LEGACY_DIR, gzip = false, change } = {}) {
  return (request, kase) => {
    const expected = readExpected(dir, kase.case_id);
    const headers = [];
    for (const [name, values] of Object.entries(expected.fields)) for (const v of values) headers.push([name, v]);
    let body = null;
    if (expected.body.kind === 'page') body = Buffer.from(kase.page_base64, 'base64');
    if (expected.body.kind === 'data') {
      const { data, template_tag, html_sha1, diff } = expected.body;
      body = Buffer.from(JSON.stringify({ data, 'template-tag': template_tag, 'html-sha1': html_sha1, diff }), 'utf8');
    }
    if (gzip && body !== null) {
      body = gzipSync(body);
      headers.push(['content-encoding', 'gzip']);
    }
    const response = { status: expected.status, headers, body };
    return change === undefined ? response : change(response, kase, request);
  };
}
