// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// Markup functions of the reference steps. The markup analysis itself is the
// Node.js parser candidate (tools/rqp-markup/node, no runtime dependency). This
// module adds what the protocol needs on top of it, with the same tokenizer:
//
// - the head end tag of the document (manifest-patch section 1.7, H-49, DL-031);
// - the manifest elements of a full document (section 1.7, client items 1 to 4);
// - the attribute rule for patched html content (section 4.2, H-48, H-59).

import { analyze as analyzeMarkup } from '../../rqp-markup/node/src/markup.mjs';
import { TEXT_MODE, VOID } from '../../rqp-markup/node/src/names.mjs';
import { TokenType, Tokenizer, isWs } from '../../rqp-markup/node/src/tokenizer.mjs';
import { LIMITS, MANIFEST_TYPE } from './common.mjs';

/** Markup analysis (CONTRACT.md section 3 result object). */
export function analyze(bytes) {
  return analyzeMarkup(bytes);
}

// The token stack of a valid document has at most maxOpenElements entries
// (markup M-26: every entry holds at least one unit of the depth counter D).
// The scan stops (fails closed) when a stack would be larger.
const MAX_STACK = 256;

// The document head (manifest-patch section 1.7, "Head end tag"; DL-031).
const HEAD_BEFORE = 0; // no head start tag yet
const HEAD_OPEN = 1; // the document head is on the token stack
const HEAD_NONE = 2; // the document has no head end tag (closed, found or never opened)

/** Start tags that can come while the document head is the current node (item 2). */
const HEAD_CONTENT = new Set([
  'base', 'basefont', 'bgsound', 'link', 'meta', 'noframes', 'noscript', 'script', 'style', 'template', 'title',
]);

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
 * The head rule of manifest-patch section 1.7 for one token, before the token
 * stack changes. `gapOk` is true when the bytes since the previous token are
 * white space. Returns the new head state and whether the token is the head
 * end tag.
 */
function headStep(state, headIndex, stackLength, t, gapOk) {
  if (state === HEAD_BEFORE) {
    // Item 1: white space, comments, DOCTYPE and html start tags before <head>.
    if (!gapOk) return { state: HEAD_NONE, end: false };
    if (t.type === TokenType.COMMENT || t.type === TokenType.DOCTYPE || t.type === TokenType.EOF) return { state, end: false };
    if (t.type === TokenType.START_TAG && (t.name === 'html' || t.name === 'head')) return { state, end: false };
    return { state: HEAD_NONE, end: false };
  }
  // Another element is the current node (the content of a title, script,
  // template ...): no check. When an end tag pops the document head with that
  // element, walk() sets HEAD_NONE (item 3).
  if (state !== HEAD_OPEN || headIndex !== stackLength - 1) return { state, end: false };
  // Item 2: the document head is the current node.
  if (!gapOk) return { state: HEAD_NONE, end: false };
  if (t.type === TokenType.COMMENT || t.type === TokenType.DOCTYPE) return { state, end: false };
  if (t.type === TokenType.START_TAG && HEAD_CONTENT.has(t.name)) return { state, end: false };
  // Item 3. After it the state is HEAD_NONE: there is at most one head end tag.
  return { state: HEAD_NONE, end: t.type === TokenType.END_TAG && t.name === 'head' };
}

/**
 * Walk the tokens of a document with the token stack of markup M-05 and the
 * state changes of M-04, and call `visit(token, html, headEnd)` for each start
 * and end tag, where `html` is true when the tag is in HTML context and
 * `headEnd` is true only for the head end tag of the document (manifest-patch
 * section 1.7, DL-031). Returns false when the walk stopped early (visit
 * returned true, or the stack limit was hit).
 */
function walk(bytes, visit) {
  const tok = new Tokenizer(bytes);
  const stack = [];
  let region = -1; // index of the svg or math element that opened the foreign region
  let head = HEAD_BEFORE;
  let headIndex = -1; // stack index of the document head while it is open
  let prevEnd = 0;
  for (;;) {
    const t = tok.next();
    const step = headStep(head, headIndex, stack.length, t, onlyWhiteSpace(bytes, prevEnd, t.start));
    const opensHead = head === HEAD_BEFORE && step.state === HEAD_BEFORE && t.type === TokenType.START_TAG && t.name === 'head';
    head = step.state;
    prevEnd = t.end;
    if (t.type === TokenType.EOF) return true;
    if (t.type === TokenType.START_TAG) {
      const html = region < 0;
      if (visit(t, html, false)) return false;
      const foreignRoot = t.name === 'svg' || t.name === 'math';
      const push = html ? !(foreignRoot && t.selfClosing) && !VOID.has(t.name) : !t.selfClosing;
      if (push) {
        if (stack.length >= MAX_STACK) return false;
        stack.push(t.name);
        if (html && foreignRoot) region = stack.length - 1;
      }
      if (opensHead) {
        head = HEAD_OPEN;
        headIndex = stack.length - 1;
      }
      const mode = html ? TEXT_MODE.get(t.name) : undefined;
      if (mode !== undefined) tok.switchTo(mode, t.name);
    } else if (t.type === TokenType.END_TAG) {
      if (visit(t, region < 0, step.end)) return false;
      const at = stack.lastIndexOf(t.name);
      if (at >= 0) {
        stack.length = at;
        if (region >= stack.length) region = -1;
        if (head === HEAD_OPEN && headIndex >= stack.length) head = HEAD_NONE; // item 3: left the stack another way
      }
    }
  }
}

/**
 * Byte offset of the "<" of the head end tag of the document, or -1
 * (manifest-patch section 1.7, "Head end tag"; server item 2; DL-031).
 */
export function documentHeadEnd(bytes) {
  let at = -1;
  walk(bytes, (t, _html, headEnd) => {
    if (headEnd) at = t.start;
    return headEnd;
  });
  return at;
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
 * The manifest elements of a full document and the head end tag of the
 * document (DL-031). Returns { elements: [{ start, textStart, textEnd, end }],
 * headAt }, or null when the walk could not finish (fail closed). `end` is -1
 * for an element without an end tag; `headAt` is -1 when the document has no
 * head end tag.
 */
export function findManifestElements(bytes) {
  const elements = [];
  let headAt = -1;
  let open = null;
  const done = walk(bytes, (t, _html, headEnd) => {
    if (open !== null && t.type === TokenType.END_TAG && t.name === 'script') {
      open.textEnd = t.start;
      open.end = t.end;
      open = null;
    }
    if (t.type === TokenType.START_TAG && t.name === 'script' && isManifestType(bytes, t.typeAttr)) {
      open = { start: t.start, textStart: t.end, textEnd: -1, end: -1 };
      elements.push(open);
    }
    if (headEnd) headAt = t.start;
    return false;
  });
  return done ? { elements, headAt } : null;
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
 * Manifest-patch section 4.2: true when every start tag of the html content
 * passes the attribute rule. The content is tokenized from the data state with
 * the state changes of M-04, as in the analysis of the patched document.
 */
export function passesAttributeRule(content) {
  if (content.length > LIMITS.maxBlockBytes) return false;
  const tok = new Tokenizer(content);
  for (;;) {
    const t = tok.next();
    if (t.type === TokenType.EOF) return true;
    if (t.type !== TokenType.START_TAG) continue;
    for (const { name, value } of attributesOf(content, t.start, t.end)) {
      if (attributeRejected(name, value)) return false;
    }
    const mode = TEXT_MODE.get(t.name);
    if (mode !== undefined) tok.switchTo(mode, t.name);
  }
}
