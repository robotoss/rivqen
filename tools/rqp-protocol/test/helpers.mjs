// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors
//
// Shared helpers of the rqp-protocol tests.

import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { analyze } from '../src/markup.mjs';
import { havePrefixes } from '../src/client.mjs';
import { pageBytes } from '../src/pages.mjs';

export const TARGET = 'https://shop.example.com/catalog';
/** The nonce N of the test requests (H-108): the bytes 00 to 0F, as the fixtures (DEFAULT_NONCE). */
export const NONCE = 'AAECAwQFBgcICQoLDA0ODw';
/** The Rq-Have member n of NONCE: the base64 Byte Sequence. */
export const NONCE_SF = ':AAECAwQFBgcICQoLDA0ODw==:';
export const KAT_DIR = fileURLToPath(new URL('../../../spec/rqp/examples/kat/', import.meta.url));
export const EXAMPLES_DIR = fileURLToPath(new URL('../../../spec/rqp/examples/', import.meta.url));

export const APP = Object.freeze({
  status: 200,
  headers: [
    ['content-type', 'text/html; charset=utf-8'],
    ['cache-control', 'no-cache'],
  ],
});

/** The origin allowlist of the client tests (DL-038): the https and the http origin of TARGET. */
export const ALLOWLIST = Object.freeze(['https://shop.example.com', 'http://shop.example.com']);

/**
 * A client context of the reference client: request URL TARGET, no redirect
 * (H-67), release build, ALLOWLIST, a native fetch (all response fields
 * visible, H-64), first visit without a snapshot, the request nonce NONCE
 * (H-108). `fields` replace members.
 */
export function clientContext(fields = {}) {
  return {
    url: TARGET,
    redirected: false,
    build: 'release',
    allow_http: false,
    allowlist: ALLOWLIST,
    response_fields_visible: true,
    navigation: 'first-visit',
    snapshot: null,
    nonce: NONCE,
    ...fields,
  };
}

export function readJson(path) {
  return JSON.parse(readFileSync(path, 'utf8'));
}

export function page(id) {
  return pageBytes(id);
}

export function revisions(id) {
  return analyze(pageBytes(id));
}

/**
 * The full delta request R(x) of fixtures.md section 4.1 as header lines,
 * with the nonce NONCE in Rq-Have (H-108).
 * `change` maps a field name to a new value, to a list of values (one line
 * each), or to null (field removed).
 */
export function deltaRequest(id, change = {}, method = 'GET') {
  const a = revisions(id);
  const lines = {
    'rq-version': '1',
    'rq-capabilities': '(block-patch)',
    accept: 'text/html, application/vnd.rivqen.delta+json',
    'if-none-match': `"${a.page_revision}"`,
    'rq-have': `t=${a.template_revision}, b=:${havePrefixes(a.blocks)}:, n=${NONCE_SF}`,
    ...change,
  };
  const headers = [];
  for (const [n, v] of Object.entries(lines)) {
    if (v === null) continue;
    for (const one of Array.isArray(v) ? v : [v]) headers.push([n, one]);
  }
  return { method, target: TARGET, headers };
}

export function firstVisit(method = 'GET') {
  return {
    method,
    target: TARGET,
    headers: [
      ['rq-version', '1'],
      ['rq-capabilities', '(block-patch)'],
      ['accept', 'text/html'],
    ],
  };
}

/** A page with one html block whose content is `content` (UTF-8). */
export function blockPage(content, id = 'x') {
  return Buffer.from(
    `<!DOCTYPE html>\n<html>\n<head>\n  <title>T</title>\n</head>\n<body>\n  <div data-rq-block="${id}">${content}</div>\n</body>\n</html>\n`,
    'utf8',
  );
}
