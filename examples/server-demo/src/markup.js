// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors
//
// Markup functions of the demo server. The markup analysis (blocks, offsets, hashes,
// template revision) is the Node.js parser candidate of WP-17 S1,
// tools/rqp-markup/node (no runtime dependency). The demo calls its library functions
// in process, not its command line: the CLI reads a file and starts a process, which a
// request path cannot afford. The demo does not copy the candidate code.
//
// This module adds two things that the protocol needs on top of the analysis. Both use
// the tokenizer of the candidate, so they read the document as the analysis does:
//   - the head end tag of the document (manifest-patch section 1.7, H-49, DL-031);
//   - the attribute rule for changed html blocks (manifest-patch section 4.2, H-56, H-59).

import { analyze as analyzeMarkup } from '../../../tools/rqp-markup/node/src/markup.mjs';
import { TEXT_MODE, VOID } from '../../../tools/rqp-markup/node/src/names.mjs';
import { TokenType, Tokenizer, isWs } from '../../../tools/rqp-markup/node/src/tokenizer.mjs';

/**
 * Markup analysis of the page bytes. Returns the result object of the candidate:
 * { valid, error, blocks: [{ id, format, start, end, sha256 }], template_revision,
 * page_revision }. The page revision is the one for a manifest without policy and limits.
 */
export function analyzePage(bytes) {
  return analyzeMarkup(bytes);
}

// The document head (manifest-patch section 1.7, "Head end tag"; DL-031).
const HEAD_BEFORE = 0; // no head start tag yet
const HEAD_OPEN = 1; // the document head is on the token stack
const HEAD_NONE = 2; // the document has no head end tag (closed, found or never opened)

/** Start tags that can come while the document head is the current node (item 2). */
const HEAD_CONTENT = new Set(['base', 'basefont', 'bgsound', 'link', 'meta', 'noframes', 'noscript', 'script', 'style', 'template', 'title']);

/**
 * DL-034: an end tag other than head, body, html and br is "any other end tag" of the
 * HTML parser (parse error, ignored) before the head and while the head is the current node.
 */
const NOT_IGNORED_END_TAGS = new Set(['head', 'body', 'html', 'br']);
const IGNORED_END_TAG = (name) => !NOT_IGNORED_END_TAGS.has(name);

/** The token stack of a valid page is never deeper than this (markup M-26). A deeper walk fails closed. */
const MAX_STACK = 256;

/** True when every byte in [from, to) is white space. A UTF-8 byte order mark at offset 0 counts (M-02). */
function onlyWhiteSpace(bytes, from, to) {
  let p = from;
  if (p === 0 && to >= 3 && bytes[0] === 0xef && bytes[1] === 0xbb && bytes[2] === 0xbf) p = 3;
  for (; p < to; p++) {
    const b = bytes[p];
    if (b !== 0x09 && b !== 0x0a && b !== 0x0c && b !== 0x0d && b !== 0x20) return false;
  }
  return true;
}

/**
 * The head rule for one token, before the token stack changes. `gapOk` is true when the
 * bytes since the previous token are white space. Returns the new head state and whether
 * the token is the head end tag of the document.
 */
function headStep(state, headIndex, depth, token, gapOk) {
  if (state === HEAD_BEFORE) {
    // Item 1: only white space, comments, DOCTYPE and html start tags may come before <head>.
    if (!gapOk) return { state: HEAD_NONE, end: false };
    const { type, name } = token;
    if (type === TokenType.COMMENT || type === TokenType.DOCTYPE || type === TokenType.EOF) return { state, end: false };
    if (type === TokenType.START_TAG && (name === 'html' || name === 'head')) return { state, end: false };
    if (type === TokenType.END_TAG && IGNORED_END_TAG(name)) return { state, end: false };
    return { state: HEAD_NONE, end: false };
  }
  // Another element is the current node (title, script, template ...): no check here.
  if (state !== HEAD_OPEN || headIndex !== depth - 1) return { state, end: false };
  // Item 2: the document head is the current node.
  if (!gapOk) return { state: HEAD_NONE, end: false };
  if (token.type === TokenType.COMMENT || token.type === TokenType.DOCTYPE) return { state, end: false };
  if (token.type === TokenType.START_TAG && HEAD_CONTENT.has(token.name)) return { state, end: false };
  if (token.type === TokenType.END_TAG && IGNORED_END_TAG(token.name)) return { state, end: false };
  // Item 3: the first other token ends the head. There is at most one head end tag.
  return { state: HEAD_NONE, end: token.type === TokenType.END_TAG && token.name === 'head' };
}

/**
 * Byte offset of the "<" of the head end tag of the document, or -1 (manifest-patch
 * section 1.7, "Head end tag"; DL-031). The head start tag must follow only white space,
 * comments, DOCTYPE and html start tags; only head content may come inside the head;
 * a head end tag in the body, in a template, or in an svg or math region does not count.
 */
export function documentHeadEnd(bytes) {
  const tokenizer = new Tokenizer(bytes);
  const stack = [];
  let region = -1; // stack index of the svg or math element that opened the foreign region
  let head = HEAD_BEFORE;
  let headIndex = -1;
  let prevEnd = 0;
  for (;;) {
    const token = tokenizer.next();
    const step = headStep(head, headIndex, stack.length, token, onlyWhiteSpace(bytes, prevEnd, token.start));
    const opensHead = head === HEAD_BEFORE && step.state === HEAD_BEFORE && token.type === TokenType.START_TAG && token.name === 'head';
    head = step.state;
    if (head === HEAD_NONE && !step.end) return -1; // no head end tag can follow: stop the walk
    prevEnd = token.end;
    if (token.type === TokenType.EOF) return -1;
    if (token.type === TokenType.START_TAG) {
      const html = region < 0;
      const foreignRoot = token.name === 'svg' || token.name === 'math';
      const push = html ? !(foreignRoot && token.selfClosing) && !VOID.has(token.name) : !token.selfClosing;
      if (push) {
        if (stack.length >= MAX_STACK) return -1;
        stack.push(token.name);
        if (html && foreignRoot) region = stack.length - 1;
      }
      if (opensHead) {
        head = HEAD_OPEN;
        headIndex = stack.length - 1;
      }
      const mode = html ? TEXT_MODE.get(token.name) : undefined;
      if (mode !== undefined) tokenizer.switchTo(mode, token.name);
    } else if (token.type === TokenType.END_TAG) {
      if (step.end) return token.start;
      const at = stack.lastIndexOf(token.name);
      if (at >= 0) {
        stack.length = at;
        if (region >= stack.length) region = -1;
        if (head === HEAD_OPEN && headIndex >= stack.length) return -1; // the head left the stack another way
      }
    }
  }
}

/**
 * The attributes of one start tag, as the HTML tokenizer reads them: names in ASCII
 * lower case, raw values (no character reference decoding), duplicates kept.
 * `tag` is the latin1 text of the start tag from "<" to ">".
 */
function attributesOf(tag) {
  const attrs = [];
  let p = 1;
  while (p < tag.length && !isWs(tag.charCodeAt(p)) && tag[p] !== '/' && tag[p] !== '>') p++; // tag name
  while (p < tag.length) {
    const c = tag[p];
    if (isWs(tag.charCodeAt(p)) || c === '/') {
      p++;
      continue;
    }
    if (c === '>') break;
    const nameStart = p;
    p++; // the first character of a name can be "="
    while (p < tag.length && !isWs(tag.charCodeAt(p)) && !'/>='.includes(tag[p])) p++;
    const name = tag.slice(nameStart, p).replace(/[A-Z]+/g, (m) => m.toLowerCase());
    while (p < tag.length && isWs(tag.charCodeAt(p))) p++;
    if (tag[p] !== '=') {
      attrs.push({ name, value: '' });
      continue;
    }
    p++;
    while (p < tag.length && isWs(tag.charCodeAt(p))) p++;
    const quote = tag[p];
    if (quote === '"' || quote === "'") {
      const close = tag.indexOf(quote, p + 1);
      const end = close < 0 ? tag.length : close;
      attrs.push({ name, value: tag.slice(p + 1, end) });
      p = end + 1;
    } else {
      const valueStart = p;
      while (p < tag.length && !isWs(tag.charCodeAt(p)) && tag[p] !== '>') p++;
      attrs.push({ name, value: tag.slice(valueStart, p) });
    }
  }
  return attrs;
}

/** Resource limit maxBlockBytes (resource-limits.md): 1 MiB. A larger block fails the rule, as in the reference. */
export const MAX_BLOCK_BYTES = 1024 * 1024;

/** Item 4 (H-48, H-59): the URL schemes that the rule denies, with the colon, in lower case. */
export const DENIED_SCHEMES = Object.freeze(['javascript:', 'vbscript:', 'data:']);
const SCHEME_BYTES = Math.max(...DENIED_SCHEMES.map((scheme) => scheme.length));

/** Manifest-patch section 4.2 for one attribute. Returns true when the rule rejects it. */
function rejected({ name, value }) {
  if (name.startsWith('on')) return true;
  if (/&(?!amp;)/.test(value)) return true;
  const url = value
    .replaceAll('&amp;', '&')
    .replace(/[\t\n\r]/g, '')
    .replace(/^[\x00-\x20]+/, '');
  const start = url.slice(0, SCHEME_BYTES).toLowerCase();
  return DENIED_SCHEMES.some((scheme) => start.startsWith(scheme));
}

/**
 * Manifest-patch section 4.2: true when every attribute of every start tag in the html
 * block content passes the attribute rule. `content` is a Buffer.
 */
export function passesAttributeRule(content) {
  if (content.length > MAX_BLOCK_BYTES) return false;
  const tokenizer = new Tokenizer(content);
  for (;;) {
    const token = tokenizer.next();
    if (token.type === TokenType.EOF) return true;
    if (token.type !== TokenType.START_TAG) continue;
    const tag = content.toString('latin1', token.start, token.end);
    if (attributesOf(tag).some(rejected)) return false;
    const mode = TEXT_MODE.get(token.name);
    if (mode !== undefined) tokenizer.switchTo(mode, token.name);
  }
}
