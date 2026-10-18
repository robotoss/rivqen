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
//   - the head start tag of the document (manifest-patch section 1.7, H-72);
//   - the attribute rule for changed html blocks (manifest-patch section 4.2, H-56, H-59).

import { analyze as analyzeMarkup } from '../../../tools/rqp-markup/node/src/markup.mjs';
import { TEXT_MODE } from '../../../tools/rqp-markup/node/src/names.mjs';
import { TokenType, Tokenizer, isWs } from '../../../tools/rqp-markup/node/src/tokenizer.mjs';

/**
 * Markup analysis of the page bytes. Returns the result object of the candidate:
 * { valid, error, blocks: [{ id, format, start, end, sha256 }], template_revision,
 * page_revision }. The page revision is the one for a manifest without policy and limits.
 */
export function analyzePage(bytes) {
  return analyzeMarkup(bytes);
}

/**
 * DL-034 (item 1 of the head rule): an end tag other than head, body, html and br is
 * "any other end tag" of the HTML parser (parse error, ignored) before the head.
 */
const NOT_IGNORED_END_TAGS = new Set(['head', 'body', 'html', 'br']);

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
 * Byte offset immediately after the ">" of the head start tag of the document, or -1
 * (manifest-patch section 1.7, "Document head"; H-72). Before the head start tag only
 * white space, comments, DOCTYPE tokens, html start tags and ignored end tags may come.
 * The walk stops at the head start tag: what comes after it does not matter.
 */
export function documentHeadStart(bytes) {
  const tokenizer = new Tokenizer(bytes);
  let prevEnd = 0;
  for (;;) {
    const token = tokenizer.next();
    if (!onlyWhiteSpace(bytes, prevEnd, token.start)) return -1;
    const { type, name } = token;
    if (type === TokenType.START_TAG && name === 'head') return token.end;
    const before =
      type === TokenType.COMMENT ||
      type === TokenType.DOCTYPE ||
      (type === TokenType.START_TAG && name === 'html') ||
      (type === TokenType.END_TAG && !NOT_IGNORED_END_TAGS.has(name));
    if (!before) return -1; // another token, or the end of input
    prevEnd = token.end;
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
