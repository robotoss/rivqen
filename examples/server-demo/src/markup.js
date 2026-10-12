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
//   - the first </head> end tag in HTML context (manifest-patch section 1.7, H-49);
//   - the attribute rule for changed html blocks (manifest-patch section 4.2, H-56).

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
 * Byte offset of the "<" of the first </head> end tag in HTML context, or -1.
 * The page must be valid for RQP (the caller has run analyzePage). Elements in an
 * svg or math region are not HTML context; text modes (script, style, title, ...)
 * are handled by the tokenizer after the mode switch.
 */
export function firstHeadEnd(bytes) {
  const tokenizer = new Tokenizer(bytes);
  const foreign = []; // names of the open elements inside an svg or math region
  for (;;) {
    const token = tokenizer.next();
    if (token.type === TokenType.EOF) return -1;
    const inForeign = foreign.length > 0;
    if (token.type === TokenType.START_TAG) {
      if (inForeign) {
        if (!token.selfClosing) foreign.push(token.name);
      } else {
        if ((token.name === 'svg' || token.name === 'math') && !token.selfClosing) foreign.push(token.name);
        const mode = TEXT_MODE.get(token.name);
        if (mode !== undefined) tokenizer.switchTo(mode, token.name);
      }
    } else if (token.type === TokenType.END_TAG) {
      if (!inForeign) {
        if (token.name === 'head') return token.start;
      } else {
        const at = foreign.lastIndexOf(token.name);
        if (at >= 0) foreign.length = at;
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

/** Manifest-patch section 4.2 for one attribute. Returns true when the rule rejects it. */
function rejected({ name, value }) {
  if (name.startsWith('on')) return true;
  if (/&(?!amp;)/.test(value)) return true;
  const url = value
    .replaceAll('&amp;', '&')
    .replace(/[\t\n\r]/g, '')
    .replace(/^[\x00-\x20]+/, '');
  return url.slice(0, 11).toLowerCase() === 'javascript:';
}

/**
 * Manifest-patch section 4.2: true when every attribute of every start tag in the html
 * block content passes the attribute rule. `content` is a Buffer.
 */
export function passesAttributeRule(content) {
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
