// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// Pages of the protocol fixtures (docs/engineering/protocol/fixtures.md section 4.1).
// Hand-written pages are files in fixtures/rqp/protocol/pages/. Generated pages
// are built here by a recipe; gen.mjs writes them and `gen.mjs --check`
// verifies the committed bytes. Each page has an expected markup result, which
// the generator checks before it uses the page.

import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';

export const PAGES_DIR = fileURLToPath(new URL('../../../fixtures/rqp/protocol/pages/', import.meta.url));
export const KAT_PAGE = fileURLToPath(new URL('../../../spec/rqp/examples/kat/page.html', import.meta.url));

const HEAD = (title) => `<!DOCTYPE html>\n<html>\n<head>\n  <title>${title}</title>\n</head>\n<body>\n`;
const TAIL = '</body>\n</html>\n';

/** Static lines of template text: `count` paragraphs, numbered, about 50 bytes each. */
function filler(count) {
  let s = '';
  for (let i = 1; i <= count; i++) {
    s += `  <p>Static catalog line ${String(i).padStart(4, '0')} of the template.</p>\n`;
  }
  return s;
}

function big(price) {
  return (
    '<!DOCTYPE html>\n<html>\n<head>\n  <title data-rq-block="title">Shop</title>\n</head>\n<body>\n' +
    '  <header>Static header</header>\n' +
    filler(400) +
    `  <p class="price" data-rq-block="price">${price}</p>\n` +
    '  <script type="application/json" data-rq-block="cart">{"items":3}</script>\n' +
    TAIL
  );
}

const HUGE_CONTENT_BYTES = 300 * 1024;

function huge(last) {
  const content = 'abcdefghij'.repeat(HUGE_CONTENT_BYTES / 10 + 1).slice(0, HUGE_CONTENT_BYTES - 1) + last;
  return HEAD('Huge') + filler(80) + `  <div data-rq-block="huge">${content}</div>\n` + TAIL;
}

function esc(last) {
  return HEAD('Escapes') + `  <p data-rq-block="quotes">${'"'.repeat(2047)}${last}</p>\n` + TAIL;
}

/**
 * Generated pages: id -> { bytes, why }. The recipes are fixed: a change of a
 * recipe changes committed fixture bytes and needs protocol-owner review.
 */
export const GENERATED = Object.freeze({
  'page-big-a': () => big('120.00'),
  'page-big-b': () => big('130.00'),
  'page-huge-a': () => huge('j'),
  'page-huge-b': () => huge('k'),
  'page-esc-a': () => esc('"'),
  'page-esc-b': () => esc("'"),
});

/**
 * Expected markup result of every page: blocks (ids in order) or the error code.
 * The generator stops when a page does not give this result.
 */
export const EXPECTED = Object.freeze({
  'page-a': { blocks: ['title', 'price', 'cart'] },
  'page-b': { blocks: ['title', 'price', 'cart'] },
  'page-b-onclick': { blocks: ['title', 'price', 'cart'] },
  'page-b-data': { blocks: ['title', 'price', 'cart'] },
  'page-c': { error: 'RQP_MARKUP_DUPLICATE' },
  // An author manifest (the bytes of spec/rqp/examples/kat/full.html): M-13 (DL-030).
  'page-forged': { error: 'RQP_MARKUP_RESERVED' },
  // Valid for RQP, but without a head end tag of the document (DL-031).
  'page-head-stray': { blocks: ['title', 'price', 'cart'] },
  'page-head-template': { blocks: ['title', 'price', 'cart'] },
  'page-no-head-start': { blocks: ['title', 'price', 'cart'] },
  'page-t2': { blocks: ['title', 'price', 'cart'] },
  'page-zero': { blocks: [] },
  'page-big-a': { blocks: ['title', 'price', 'cart'] },
  'page-big-b': { blocks: ['title', 'price', 'cart'] },
  'page-huge-a': { blocks: ['huge'] },
  'page-huge-b': { blocks: ['huge'] },
  'page-esc-a': { blocks: ['quotes'] },
  'page-esc-b': { blocks: ['quotes'] },
});

export const PAGE_IDS = Object.freeze(Object.keys(EXPECTED));

/** A page id that is not in EXPECTED. */
export class UnknownPage extends Error {
  constructor(id) {
    super(`unknown page ${id}`);
    this.name = 'UnknownPage';
    this.code = 'RQP_FIXTURE_PAGE';
  }
}

/** The bytes of a page: the recipe output for a generated page, else the committed file. */
export function pageBytes(id) {
  if (!Object.hasOwn(EXPECTED, id)) throw new UnknownPage(id);
  const recipe = GENERATED[id];
  if (recipe !== undefined) return Buffer.from(recipe(), 'utf8');
  return readFileSync(`${PAGES_DIR}${id}.html`);
}
