// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// Shared helpers of the tests.

import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { respond, sha1hex, split } from '../src/model.mjs';

const PAGES = new URL('../../../fixtures/legacy/pages/', import.meta.url);

export const CANONICAL = 'legacy-canonical';
export const APP = Object.freeze({ status: 200, headers: [['content-type', 'text/html; charset=utf-8']] });

export function page(id) {
  return readFileSync(new URL(`${id}.html`, PAGES));
}

export function pagesDir() {
  return fileURLToPath(PAGES);
}

export function etagOf(id) {
  return sha1hex(page(id));
}

export function tagOf(profile, id) {
  const parts = split(profile, page(id).toString('utf8'));
  return sha1hex(parts.template);
}

/** A legacy request: accept-diff true plus the given lines. */
export function legacy(...lines) {
  return { method: 'GET', headers: [['accept-diff', 'true'], ...lines] };
}

export function fieldValues(response, name) {
  return response.headers.filter(([n]) => n.toLowerCase() === name).map(([, v]) => v);
}

export function bodyText(response) {
  return response.body === null ? null : Buffer.from(response.body).toString('utf8');
}

export function run(profile, request, pageId = 'a', app = APP) {
  return respond(profile, request, app, page(pageId));
}

/**
 * The text of a trace file for a request and a response (the layout of evidence/traces/server).
 * `method` is the method of the request line; with `host`, the request has a `Host` first line
 * (the layout of a case request sent through the lab control endpoint).
 */
export function traceText(path, requestHeaders, response, encoding = 'identity', { method = 'GET', host = null } = {}) {
  const lines = host === null ? requestHeaders : [['Host', host], ...requestHeaders];
  const head = [`${method} ${path} HTTP/1.1`, ...lines.map(([n, v]) => `${n}: ${v}`)].join('\n');
  const status = [`HTTP ${response.status}`, ...response.headers.map(([n, v]) => `${n}: ${v}`)].join('\n');
  return Buffer.concat([
    Buffer.from(`${head}\n\n--- response ---\n${status}\n\n[body decoded from content-encoding=${encoding}]\n`),
    response.body === null ? Buffer.alloc(0) : Buffer.from(response.body),
  ]);
}
