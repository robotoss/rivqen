// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// FROZEN COPY of tools/rqp-protocol/src/markup.mjs at commit 9bd8c32 (the
// pre-S4 reference: head start H-72, attribute rule H-48/H-59/H-73, before H-77,
// H-78, H-79a and H-83). Do not update it. The S3 and S4 study tests compare
// their "flags off" or older-rule options with this copy, not with the shipped
// reference, because the shipped reference moved on (DL-047). Changes against
// the original: the imports point to the study baseline name sets
// (names-s3.mjs) and to the shared modules; the `analyze` wrapper is removed
// (it called the shipped analysis, which the studies do not compare here).
//
// Original header:
// Markup functions of the reference steps. The markup analysis itself is the
// Node.js parser candidate (tools/rqp-markup/node, no runtime dependency). This
// module adds what the protocol needs on top of it, with the same tokenizer:
//
// - the head start tag of the document (manifest-patch section 1.7, H-72);
// - the manifest elements of a full document (section 1.7, client items 1 to 4);
// - the attribute rule for patched html content (section 4.2, H-48, H-59).

import { TEXT_MODE, VOID } from './names-s3.mjs';
import { TokenType, Tokenizer, isWs } from '../../node/src/tokenizer.mjs';
import { LIMITS, MANIFEST_TYPE } from '../../../rqp-protocol/src/common.mjs';

// The token stack of a valid document has at most maxOpenElements entries
// (markup M-26: every entry holds at least one unit of the depth counter D).
// The scan stops (fails closed) when a stack would be larger.
const MAX_STACK = 256;

/**
 * End tags that item 1 of the head rule does not ignore (DL-034). The HTML
 * parser ignores every other end tag in the "initial", "before html" and
 * "before head" insertion modes.
 */
const HEAD_END_TAGS = new Set(['head', 'body', 'html', 'br']);

/**
 * True when every byte in [from, to) is white space (09, 0A, 0C, 0D, 20). A
 * UTF-8 byte order mark at offset 0 counts as white space (markup M-02).
 */
function onlyWhiteSpace(bytes, from, to) {
  let p = from;
  if (p === 0 && to >= 3 && bytes[0] === 0xef && bytes[1] === 0xbb && bytes[2] === 0xbf) p = 3;
  for (; p < to; p++) {
    const c = bytes[p];
    if (c !== 0x09 && c !== 0x0a && c !== 0x0c && c !== 0x0d && c !== 0x20) return false;
  }
  return true;
}

/**
 * Item 1 of the head rule (manifest-patch section 1.7, "Document head"): true
 * for a token that can come before the head start tag of the document. These
 * are comments, DOCTYPE tokens, html start tags and ignored end tags (DL-034).
 * Every other token (also the end of input) means: no document head.
 */
function beforeHead(t) {
  if (t.type === TokenType.COMMENT || t.type === TokenType.DOCTYPE) return true;
  if (t.type === TokenType.START_TAG) return t.name === 'html';
  if (t.type === TokenType.END_TAG) return !HEAD_END_TAGS.has(t.name);
  return false;
}

/**
 * Byte offset immediately after the ">" of the head start tag of the document,
 * or -1 when the document has no head start tag (manifest-patch section 1.7,
 * "Document head"; server item 2 and client item 3; H-72). The server inserts
 * the manifest element at this offset; the client requires the manifest start
 * tag at this offset. Both call this function, so they find the same tag. The
 * walk reads each token before the head start tag once and stops there.
 */
export function documentHeadStart(bytes) {
  const tok = new Tokenizer(bytes);
  let prevEnd = 0;
  for (;;) {
    const t = tok.next();
    if (!onlyWhiteSpace(bytes, prevEnd, t.start)) return -1;
    if (t.type === TokenType.START_TAG && t.name === 'head') return t.end;
    if (!beforeHead(t)) return -1;
    prevEnd = t.end;
  }
}

/**
 * Walk the tokens of a document with the token stack of markup M-05 and the
 * state changes of M-04, and call `visit(token)` for each start and end tag.
 * Returns false when the walk stopped early (the stack limit was hit).
 */
function walk(bytes, visit) {
  const tok = new Tokenizer(bytes);
  const stack = [];
  let region = -1; // index of the svg or math element that opened the foreign region
  for (;;) {
    const t = tok.next();
    if (t.type === TokenType.EOF) return true;
    if (t.type === TokenType.START_TAG) {
      const html = region < 0;
      visit(t);
      const foreignRoot = t.name === 'svg' || t.name === 'math';
      const push = html ? !(foreignRoot && t.selfClosing) && !VOID.has(t.name) : !t.selfClosing;
      if (push) {
        if (stack.length >= MAX_STACK) return false;
        stack.push(t.name);
        if (html && foreignRoot) region = stack.length - 1;
      }
      const mode = html ? TEXT_MODE.get(t.name) : undefined;
      if (mode !== undefined) tok.switchTo(mode, t.name);
    } else if (t.type === TokenType.END_TAG) {
      visit(t);
      const at = stack.lastIndexOf(t.name);
      if (at >= 0) {
        stack.length = at;
        if (region >= stack.length) region = -1;
      }
    }
  }
}

const TYPE_BYTES = Buffer.from(MANIFEST_TYPE, 'latin1');

/** Manifest-patch section 1.7, client item 1: 32 bytes, A-Z mapped to a-z, nothing else. */
export function isManifestType(bytes, span) {
  if (span === null || span.end - span.start !== TYPE_BYTES.length) return false;
  for (let i = 0; i < TYPE_BYTES.length; i++) {
    let c = bytes[span.start + i];
    if (c >= 0x41 && c <= 0x5a) c += 0x20;
    if (c !== TYPE_BYTES[i]) return false;
  }
  return true;
}

/**
 * The manifest elements of a full document and the head start tag of the
 * document (H-72). Returns { elements: [{ start, textStart, textEnd, end }],
 * headStart }, or null when the walk could not finish (fail closed). `end` is
 * -1 for an element without an end tag; `headStart` is the value of
 * documentHeadStart (-1 when the document has no head start tag).
 */
export function findManifestElements(bytes) {
  const elements = [];
  let open = null;
  const done = walk(bytes, (t) => {
    if (open !== null && t.type === TokenType.END_TAG && t.name === 'script') {
      open.textEnd = t.start;
      open.end = t.end;
      open = null;
    }
    if (t.type === TokenType.START_TAG && t.name === 'script' && isManifestType(bytes, t.typeAttr)) {
      open = { start: t.start, textStart: t.end, textEnd: -1, end: -1 };
      elements.push(open);
    }
  });
  return done ? { elements, headStart: documentHeadStart(bytes) } : null;
}

/**
 * All attributes of one start tag [start, end) as the tokenizer reads them
 * (HTML section 13.2.5.32 to 13.2.5.40): names in ASCII lower case, raw values
 * (no character reference decoding), duplicates included.
 */
export function attributesOf(bytes, start, end) {
  const attrs = [];
  let p = start + 1;
  while (p < end && !isWs(bytes[p]) && bytes[p] !== 0x2f && bytes[p] !== 0x3e) p++; // tag name
  let name = null;
  const lower = (a, b) => bytes.toString('utf8', a, b).replace(/[A-Z]+/g, (m) => m.toLowerCase());
  const finish = (valueStart, valueEnd) => {
    attrs.push({ name, value: bytes.subarray(valueStart, valueEnd) });
    name = null;
  };
  while (p < end) {
    const c = bytes[p];
    if (isWs(c) || c === 0x2f) {
      p++;
      continue;
    }
    if (c === 0x3e) break;
    // Attribute name: the first character can be "=" (13.2.5.32).
    const nameStart = p;
    p++;
    while (p < end && !isWs(bytes[p]) && bytes[p] !== 0x2f && bytes[p] !== 0x3e && bytes[p] !== 0x3d) p++;
    name = lower(nameStart, p);
    while (p < end && isWs(bytes[p])) p++;
    if (bytes[p] !== 0x3d) {
      finish(p, p); // no value
      continue;
    }
    p++; // "="
    while (p < end && isWs(bytes[p])) p++;
    const q = bytes[p];
    if (q === 0x22 || q === 0x27) {
      const close = bytes.indexOf(q, p + 1);
      const valueEnd = close < 0 || close > end ? end : close;
      finish(p + 1, valueEnd);
      p = valueEnd + 1;
    } else if (q === 0x3e) {
      finish(p, p);
    } else {
      const valueStart = p;
      while (p < end && !isWs(bytes[p]) && bytes[p] !== 0x3e) p++;
      finish(valueStart, p);
    }
  }
  return attrs;
}

const AMP = Buffer.from('&amp;', 'latin1');

/** Item 4 (H-48, H-59): the URL schemes that the rule denies, with the colon, in lower case. */
export const DENIED_SCHEMES = Object.freeze(['javascript:', 'vbscript:', 'data:']);

/** The length of the longest denied scheme ("javascript:", 11 bytes). */
const SCHEME_BYTES = Math.max(...DENIED_SCHEMES.map((s) => s.length));

/** Manifest-patch section 4.2 for one attribute: true when it is rejected. */
export function attributeRejected(name, value) {
  if (name.startsWith('on')) return true; // item 1
  for (let i = value.indexOf(0x26); i >= 0; i = value.indexOf(0x26, i + 1)) {
    if (!value.subarray(i, i + AMP.length).equals(AMP)) return true; // item 2
  }
  // Item 3: "&amp;" -> "&", remove TAB, LF, CR, then leading bytes 00 to 20.
  let copy = value.toString('latin1').replaceAll('&amp;', '&').replace(/[\t\n\r]/g, '');
  copy = copy.replace(/^[\x00-\x20]+/, '');
  // Item 4: ASCII case-insensitive; only the first SCHEME_BYTES bytes matter.
  const lower = copy.slice(0, SCHEME_BYTES).replace(/[A-Z]/g, (m) => m.toLowerCase());
  return DENIED_SCHEMES.some((scheme) => lower.startsWith(scheme));
}

/**
 * The start tags that the attribute rule reads in the content: the byte offsets of
 * their "<", in order, or null when the walk cannot finish (fail closed).
 */
export function startTagOffsets(content) {
  const offsets = [];
  const done = walk(content, (t) => {
    if (t.type === TokenType.START_TAG) offsets.push(t.start);
  });
  return done ? offsets : null;
}

/**
 * Manifest-patch section 4.2: true when every start tag of the html content
 * passes the attribute rule, also a start tag in a foreign region (H-73). The
 * content is tokenized from the data state with the token stack of M-05 and the
 * state changes of M-04, which apply only in HTML context, as in the analysis
 * of the patched document. Fails closed when the walk cannot finish.
 */
export function passesAttributeRule(content) {
  if (content.length > LIMITS.maxBlockBytes) return false;
  let ok = true;
  const done = walk(content, (t) => {
    if (!ok || t.type !== TokenType.START_TAG) return;
    for (const { name, value } of attributesOf(content, t.start, t.end)) {
      if (attributeRejected(name, value)) ok = false;
    }
  });
  return done && ok;
}
