// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// Study copy of the head walk of manifest-patch section 1.7 ("Document head",
// H-72; tools/rqp-protocol/src/markup.mjs documentHeadStart) for WP-17 S4 T-03
// (H-79). Study code only: it changes no rule.
//
// Rules (option `rule`):
//   shipped  H-72: only an explicit head start tag gives an insertion point.
//   h79      H-79 as decided for the study: when item 1 stops at a start tag in
//            IMPLIED_H79 before any head start tag, the insertion point is the "<"
//            of that tag (the manifest script implies the head).
//   l1       h79 and the two other head content start tags, noscript and template.
//   l2       every start tag other than html and head at which item 1 stops.
//
// Time: one tokenizer pass over the tokens before the stop point, each token read
// once. Memory: constant (no stack). The walk never reads past the stop token.

import { TokenType, Tokenizer } from '../../../node/src/tokenizer.mjs';

export const HEAD_RULES = Object.freeze(['shipped', 'h79', 'l1', 'l2']);

/** H-79: the head content start tags of the decision (no noscript, no template). */
export const IMPLIED_H79 = Object.freeze(new Set([
  'base', 'basefont', 'bgsound', 'link', 'meta', 'noframes', 'script', 'style', 'title',
]));

const IMPLIED_L1 = Object.freeze(new Set([...IMPLIED_H79, 'noscript', 'template']));

/** End tags that item 1 does not ignore (DL-034). */
const HEAD_END_TAGS = new Set(['head', 'body', 'html', 'br']);

/** White space of item 1; a UTF-8 byte order mark at offset 0 counts too (markup M-02). */
export function onlyWhiteSpace(bytes, from, to) {
  let p = from;
  if (p === 0 && to >= 3 && bytes[0] === 0xef && bytes[1] === 0xbb && bytes[2] === 0xbf) p = 3;
  for (; p < to; p++) {
    const c = bytes[p];
    if (c !== 0x09 && c !== 0x0a && c !== 0x0c && c !== 0x0d && c !== 0x20) return false;
  }
  return true;
}

function beforeHead(t) {
  if (t.type === TokenType.COMMENT || t.type === TokenType.DOCTYPE) return true;
  if (t.type === TokenType.START_TAG) return t.name === 'html';
  if (t.type === TokenType.END_TAG) return !HEAD_END_TAGS.has(t.name);
  return false;
}

function impliedBy(rule, name) {
  if (rule === 'h79') return IMPLIED_H79.has(name);
  if (rule === 'l1') return IMPLIED_L1.has(name);
  if (rule === 'l2') return name !== 'html' && name !== 'head';
  return false;
}

/** A label of the token that ends item 1 (tag names only, never page text). */
function stopLabel(t, gapOk) {
  if (!gapOk) return 'text';
  if (t.type === TokenType.START_TAG) return `<${t.name}>`;
  if (t.type === TokenType.END_TAG) return `</${t.name}>`;
  if (t.type === TokenType.EOF) return 'eof';
  return 'other';
}

/**
 * The insertion point of the manifest element (server item 2) under a rule.
 *
 * Returns { at, kind, stop }: `at` is the byte offset (after the ">" of the head
 * start tag for kind "explicit", the "<" of the stop tag for kind "implied"), or -1
 * for kind "none"; `stop` is the label of the token that ended item 1 (for kind
 * "explicit" it is "<head>").
 *
 * @param {Uint8Array} input
 * @param {'shipped' | 'h79' | 'l1' | 'l2'} [rule]
 */
export function headPosition(input, rule = 'h79') {
  if (!HEAD_RULES.includes(rule)) throw new RangeError(`unknown head rule ${rule}`);
  const bytes = Buffer.from(input.buffer, input.byteOffset, input.byteLength);
  const tok = new Tokenizer(bytes);
  let prevEnd = 0;
  for (;;) {
    const t = tok.next();
    const gapOk = onlyWhiteSpace(bytes, prevEnd, t.start);
    if (gapOk && t.type === TokenType.START_TAG && t.name === 'head') return { at: t.end, kind: 'explicit', stop: '<head>' };
    if (!gapOk || !beforeHead(t)) {
      const stop = stopLabel(t, gapOk);
      if (gapOk && t.type === TokenType.START_TAG && impliedBy(rule, t.name)) return { at: t.start, kind: 'implied', stop };
      return { at: -1, kind: 'none', stop };
    }
    prevEnd = t.end;
  }
}

export const MANIFEST_TYPE = 'application/rivqen-manifest+json';
export const MANIFEST_START = `<script type="${MANIFEST_TYPE}">`;

/** The full document F: the manifest element at `at` (server items 1 and 2). */
export function fullDocument(original, at, manifestText = '{}') {
  const el = Buffer.from(`${MANIFEST_START}${manifestText}</script>`, 'latin1');
  return { full: Buffer.concat([original.subarray(0, at), el, original.subarray(at)]), length: el.length };
}

/**
 * The client position rules for a manifest element at [start, end) of F.
 *   c1  walk on F (client item 3 as written for H-72): the manifest start tag is at
 *       the position that the walk finds on F.
 *   c2  walk on the original document O (F without the element, client item 5):
 *       the manifest start tag is at the insertion point that the server finds on O.
 * Returns { c1, c2 } booleans.
 *
 * @param {Uint8Array} full F
 * @param {number} start the "<" of the manifest start tag
 * @param {number} end the byte after the ">" of its end tag
 * @param {'shipped' | 'h79' | 'l1' | 'l2'} rule
 */
export function clientAccepts(full, start, end, rule = 'h79') {
  const onF = headPosition(full, rule);
  const original = Buffer.concat([full.subarray(0, start), full.subarray(end)]);
  const onO = headPosition(original, rule);
  return { c1: onF.at === start, c2: onO.at === start };
}
