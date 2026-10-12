// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors
//
// Shared helpers of the demo tests. The oracles are independent of src/: the markup
// candidate for revisions, and the known-answer files of spec/rqp/examples/kat.

import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { analyze } from '../../../tools/rqp-markup/node/src/markup.mjs';

export const TARGET = 'https://shop.example.com/catalog';
const KAT = new URL('../../../spec/rqp/examples/kat/', import.meta.url);
const EXAMPLES = new URL('../../../spec/rqp/examples/', import.meta.url);

export const katBytes = (name) => readFileSync(new URL(name, KAT));
export const exampleJson = (path) => JSON.parse(readFileSync(new URL(path, EXAMPLES), 'utf8'));
export const REPO_ROOT = fileURLToPath(new URL('../../../', import.meta.url));

export const APP = Object.freeze({
  status: 200,
  headers: [
    ['content-type', 'text/html; charset=utf-8'],
    ['cache-control', 'no-cache'],
  ],
});

/** A small valid page with three blocks. */
export function page(price = '120.00', extra = '') {
  return Buffer.from(
    '<!DOCTYPE html><html><head><title data-rq-block="title">Shop</title></head><body>' +
      `${extra}<p data-rq-block="price">${price}</p>` +
      '<script type="application/json" data-rq-block="cart">{"n":1}</script></body></html>',
    'utf8',
  );
}

/** Revisions of a page for a manifest without policy and limits (the candidate result). */
export const revisionsOf = (bytes) => analyze(bytes);

/** Rq-Have value of a client that holds `bytes`. */
export function rqHave(bytes) {
  const r = analyze(bytes);
  const prefixes = Buffer.concat(r.blocks.map((b) => Buffer.from(b.sha256, 'base64url').subarray(0, 8)));
  return `t=${r.template_revision}, b=:${prefixes.toString('base64')}:`;
}

/** Request header lines of an RQP client that holds the page `held`. */
export function clientLines(held, overrides = {}) {
  const r = analyze(held);
  const fields = {
    'rq-version': '1',
    'rq-capabilities': '(block-patch)',
    accept: 'text/html, application/vnd.rivqen.delta+json',
    'if-none-match': `"${r.page_revision}"`,
    'rq-have': rqHave(held),
    ...overrides,
  };
  return Object.entries(fields).filter(([, v]) => v !== null).map(([n, v]) => [n, v]);
}

export const get = (headers = []) => ({ method: 'GET', target: TARGET, headers });

export const header = (result, name) => {
  const values = result.headers.filter(([n]) => n === name).map(([, v]) => v);
  return values.length === 0 ? null : values.join(', ');
};
