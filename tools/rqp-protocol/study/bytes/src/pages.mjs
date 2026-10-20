// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// Pages of the byte study (WP-17 S6 T-01). All pages are written by Rivqen
// code; no corpus page and no upstream page is used.
//
// - demoPage(step): the catalog page of examples/server-demo at data step
//   `step` (template step 0), the page of evidence/benchmarks/rqp-bytes-2026-10-19.json.
// - gridPage(spec): synthetic pages with N blocks of small (20 bytes) or large
//   (2048 bytes) content, deterministic (fixed seeds).
// - toLegacy(page): the same page with legacy comment markers in place of the
//   data-rq-block attributes (Rivqen code; the marker syntax is described in
//   docs/engineering/protocol/legacy-markers.md).
// - stripMarkup(page): the same page without any block markup.

import { analyze } from '../../../src/markup.mjs';
import { demoState, renderCatalog } from '../../../../../examples/server-demo/src/pages.js';
import { StudyError } from './errors.mjs';

export { CONFIG as DEMO_CONFIG } from '../../../../../examples/server-demo/src/app.js';

/** The target of the 2026-10-19 run: the demo server default origin (examples/server-demo/src/server.js). */
export const DEMO_TARGET = 'http://127.0.0.1:8787/catalog';
/** The target of the synthetic pages. */
export const GRID_TARGET = 'https://shop.example.com/catalog';

function replaceOnce(text, from, to) {
  const at = text.indexOf(from);
  if (at < 0 || text.indexOf(from, at + 1) >= 0) throw new StudyError('STUDY_PAGE', `expected exactly one "${from}"`);
  return text.slice(0, at) + to + text.slice(at + from.length);
}

/**
 * The demo catalog page at data step `step` and template step 0. It applies
 * the formulas of renderCatalog (price 120 + 5 step, stock 25 - step, items
 * step mod 4) to the page of step 0, so it does not depend on the module state
 * of the demo. The demo state must be the initial state.
 */
export function demoPage(step) {
  if (!Number.isInteger(step) || step < 0 || step > 25) throw new StudyError('STUDY_PAGE', 'demo step out of range');
  const s = demoState();
  if (s.data !== 0 || s.template !== 0) throw new StudyError('STUDY_PAGE', 'the demo state is not the initial state');
  const price = (120 + step * 5).toFixed(2);
  let p = renderCatalog();
  p = replaceOnce(p, 'Shop — Catalog (120.00)</title>', `Shop — Catalog (${price})</title>`);
  p = replaceOnce(p, '<span data-rq-block="price">120.00</span>', `<span data-rq-block="price">${price}</span>`);
  p = replaceOnce(p, 'In stock: 25</p>', `In stock: ${Math.max(0, 25 - step)}</p>`);
  p = replaceOnce(p, '{"items":0,', `{"items":${step % 4},`);
  return Buffer.from(p, 'utf8');
}

/** mulberry32: a small fixed PRNG, so every page is the same in every run. */
export function prng(seed) {
  let a = seed >>> 0;
  return () => {
    a = (a + 0x6d2b79f5) >>> 0;
    let t = a;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

const digits = (rand, n) => {
  let s = '';
  for (let i = 0; i < n; i++) s += String(Math.floor(rand() * 10));
  return s;
};

export const SMALL_BYTES = 20;
export const LARGE_BYTES = 2048;

/** Block content of `size` "small" (exactly 20 bytes) or "large" (exactly 2048 bytes). */
export function blockContent(size, index, version) {
  const rand = prng(1000003 * (index + 1) + 7919 * (version + 1));
  if (size === 'small') return `<b>Qty ${digits(rand, 5)} pcs</b>`;
  if (size !== 'large') throw new StudyError('STUDY_PAGE', `unknown block size ${size}`);
  const open = '<ul class="list">';
  const close = '</ul>';
  const minPad = '<li></li>'.length;
  let s = open;
  for (;;) {
    const id = digits(rand, 4);
    const item = `<li><a href="/p/${id}">Item ${id}</a> <span class="v">${digits(rand, 3)}.${digits(rand, 2)}</span></li>`;
    if (s.length + item.length + minPad + close.length > LARGE_BYTES) break;
    s += item;
  }
  s += `<li>${'x'.repeat(LARGE_BYTES - s.length - minPad - close.length)}</li>${close}`;
  return s;
}

/** Block id of block `i` of a synthetic page. */
export const gridId = (i) => `b${String(i).padStart(2, '0')}`;

/** Static template text of the synthetic pages: 48 lines, the same for every N. */
function staticLines() {
  let s = '';
  for (let i = 1; i <= 48; i++) s += `<p class="t">Static catalog line ${String(i).padStart(4, '0')} of the template.</p>\n`;
  return s;
}

/**
 * A synthetic page. spec = { blocks: N, size: "small"|"large", version: 0|1, changed: "one"|"all" }.
 * Version 1 changes the content of block 0 ("one") or of every block ("all").
 */
export function gridPage({ blocks, size, version = 0, changed = 'all' }) {
  if (!Number.isInteger(blocks) || blocks < 1 || blocks > 256) throw new StudyError('STUDY_PAGE', 'block count out of range');
  let body = '';
  for (let i = 0; i < blocks; i++) {
    const v = version === 1 && (changed === 'all' || i === 0) ? 1 : 0;
    body += `<div class="slot" data-rq-block="${gridId(i)}">${blockContent(size, i, v)}</div>\n`;
  }
  const page =
    '<!DOCTYPE html>\n<html lang="en">\n<head>\n<meta charset="utf-8">\n<title>Grid page</title>\n</head>\n<body>\n' +
    '<header><nav><a href="/">Home</a> <a href="/catalog">Catalog</a></nav></header>\n<main>\n' +
    staticLines() +
    body +
    '</main>\n<footer><p>Static footer.</p></footer>\n</body>\n</html>\n';
  return Buffer.from(page, 'utf8');
}

/**
 * The element of each block: offsets of its start tag and of the end of its
 * end tag, from the markup analysis (content offsets) and the tag bytes around
 * them. Only for the study pages: their start tags have no "<" in a value.
 */
function blockElements(page) {
  const a = analyze(page);
  if (!a.valid) throw new StudyError('STUDY_PAGE', `page is invalid for RQP: ${a.error}`);
  return a.blocks.map((b) => {
    const tagStart = page.lastIndexOf(0x3c, b.start - 1);
    const attr = Buffer.from(` data-rq-block="${b.id}"`, 'utf8');
    const startTag = page.subarray(tagStart, b.start);
    const attrAt = startTag.indexOf(attr);
    if (attrAt < 0 || startTag.indexOf(attr, attrAt + 1) >= 0) throw new StudyError('STUDY_PAGE', `block ${b.id}: attribute not found once`);
    const endTagEnd = page.indexOf(0x3e, b.end) + 1;
    const name = /^<([a-z]+)/.exec(startTag.toString('latin1'))[1];
    return { id: b.id, name, tagStart, attrAt: tagStart + attrAt, attrLen: attr.length, start: b.start, end: b.end, endTagEnd };
  });
}

/**
 * The page with each block element changed by `wrap(block, elementBytes)`,
 * where elementBytes is the element without its data-rq-block attribute.
 */
function rewrite(page, wrap) {
  const parts = [];
  let at = 0;
  for (const b of blockElements(page)) {
    parts.push(page.subarray(at, b.tagStart));
    const element = Buffer.concat([page.subarray(b.tagStart, b.attrAt), page.subarray(b.attrAt + b.attrLen, b.endTagEnd)]);
    parts.push(wrap(b, element));
    at = b.endTagEnd;
  }
  parts.push(page.subarray(at));
  return Buffer.concat(parts);
}

/** The page without block markup: the data-rq-block attributes removed. */
export function stripMarkup(page) {
  return rewrite(page, (_b, element) => element);
}

/**
 * The same page with legacy markers: each block element without its
 * data-rq-block attribute, between <!--sonicdiff-ID--> and
 * <!--sonicdiff-ID-end-->. A `title` block gets no markers by default,
 * because the legacy servers take the first title element as the data block
 * "title" (legacy-markers section 2). titleMarkers: true writes markers around
 * the title too: the conversion of the 2026-10-19 run (measure.js).
 */
export function toLegacy(page, { titleMarkers = false } = {}) {
  let seenTitle = false;
  return rewrite(page, (b, element) => {
    if (b.name === 'title' && !seenTitle && !titleMarkers) {
      seenTitle = true;
      return element;
    }
    return Buffer.concat([Buffer.from(`<!--sonicdiff-${b.id}-->`), element, Buffer.from(`<!--sonicdiff-${b.id}-end-->`)]);
  });
}
