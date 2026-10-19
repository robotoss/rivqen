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
//   - the insertion point of the manifest: the head start tag of the document, or the start
//     tag that implies the head (manifest-patch section 1.7, H-72, H-79a);
//   - the patch content rule for changed html blocks (manifest-patch section 4, item 1:
//     PATCH_FORBIDDEN, FOREIGN_ALLOWED and the attribute rule of section 4.2; H-56, H-59,
//     H-73, H-77, H-78, H-83).
// It is a second implementation on purpose (DL-036, DL-046): it is written from the spec,
// not from tools/rqp-protocol, and the protocol fixtures and the differential tests keep
// the two equal.

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
 * H-79a: head content that implies the head when it comes before any head start tag
 * (the "before head" insertion mode treats it as "anything else").
 */
const HEAD_CONTENT = new Set(['base', 'basefont', 'bgsound', 'link', 'meta', 'noframes', 'script', 'style', 'title']);

/**
 * The insertion point of the manifest element, or -1 (manifest-patch section 1.7,
 * "Document head"; H-72, H-79a): the byte offset immediately after the ">" of the head
 * start tag of the document, or the offset of the "<" of the first head content start
 * tag (HEAD_CONTENT) when that comes first (an implied head). Before it only white space,
 * comments, DOCTYPE tokens, html start tags and ignored end tags may come. The walk
 * stops there: what comes after it does not matter.
 */
export function documentHeadStart(bytes) {
  const tokenizer = new Tokenizer(bytes);
  let prevEnd = 0;
  for (;;) {
    const token = tokenizer.next();
    if (!onlyWhiteSpace(bytes, prevEnd, token.start)) return -1;
    const { type, name } = token;
    if (type === TokenType.START_TAG && name === 'head') return token.end;
    if (type === TokenType.START_TAG && HEAD_CONTENT.has(name)) return token.start; // the implied head
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

/** Item 1.1 of the patch content rule (H-78): start tags that a changed block must not have in HTML context. */
export const PATCH_FORBIDDEN = Object.freeze(
  new Set(['script', 'style', 'link', 'meta', 'iframe', 'object', 'embed', 'base', 'basefont', 'bgsound']),
);

/** Item 1.2 (H-73, H-78): the start tags that a foreign region of a changed block may have. */
export const FOREIGN_ALLOWED = Object.freeze(
  new Set(
    (
      'svg g path circle ellipse line polyline polygon rect defs symbol use title desc lineargradient radialgradient ' +
      'stop clippath mask pattern marker text tspan filter feblend fecolormatrix fecomponenttransfer fecomposite ' +
      'fedisplacementmap fedropshadow feflood fefunca fefuncb fefuncg fefuncr fegaussianblur femerge femergenode ' +
      'femorphology feoffset feturbulence ' +
      'math mrow mi mo mn ms mtext mspace msub msup msubsup mfrac msqrt mroot mstyle munder mover munderover ' +
      'mmultiscripts mprescripts none mtable mtr mtd mpadded mphantom merror semantics annotation'
    ).split(' '),
  ),
);

// The attribute rule works on the Latin-1 text of the raw value: one character per byte.

/**
 * A numeric character reference at the start of `s` (markup M-13): "&#" and decimal
 * digits, or "&#x"/"&#X" and hex digits, all of them, ";" optional. Returns its value,
 * where any value above 10FFFF is 0x110000, or -1 when `s` does not start with one.
 */
function numericReferenceValue(s) {
  const m = /^&#(?:[xX]([0-9A-Fa-f]+)|([0-9]+))/.exec(s);
  if (m === null) return -1;
  const [digits, radix] = m[1] !== undefined ? [m[1], 16] : [m[2], 10];
  const significant = digits.replace(/^0+/, '');
  if (significant.length > 7) return 0x110000; // more than 7 significant digits is above 10FFFF in both radixes
  return Math.min(Number.parseInt(significant || '0', radix), 0x110000);
}

/**
 * Item 2 (H-77): the character reference at the scheme decision byte can continue or
 * complete a scheme. The decision byte comes after the leading bytes 00 to 20 and after
 * the bytes tab, LF, CR, ASCII letters, digits, "+", "-" and ".". The reference is read
 * on the raw value (a tab or LF can end its digits), never on the normalized copy.
 */
function decisionReferenceRejected(value) {
  const at = /^[\x00-\x20]*[\t\n\rA-Za-z0-9+.-]*/.exec(value)[0].length;
  if (value[at] !== '&') return false;
  const rest = value.slice(at);
  const code = numericReferenceValue(rest);
  if (code >= 0) {
    if (code <= 0x20) return true;
    return /^[A-Za-z0-9:+.-]$/.test(String.fromCharCode(code)) && code < 0x80;
  }
  return /^&(?:Tab|NewLine|colon|fjlig|period|plus);/.test(rest);
}

/** Item 3 and 4: the value without tab, LF and CR and without leading bytes 00 to 20, ASCII lower case. */
function normalizedUrl(value) {
  return value
    .replace(/[\t\n\r]/g, '')
    .replace(/^[\x00-\x20]+/, '')
    .replace(/[A-Z]/g, (c) => c.toLowerCase());
}

/** H-83: in HTML context, the attributes per element where a raster data: image may stay. */
const IMAGE_ATTRIBUTES = Object.freeze({ img: ['src', 'srcset'], source: ['src', 'srcset'], video: ['poster'] });

/**
 * Item 4.2 (H-83, H-83a, DL-048 b): a raster data: image. "data:image/", not
 * "data:image/svg+xml", and no "%" in the type part (after "data:" up to ";" or ","). The
 * demo also rejects a "&" there, like the reference: a character reference can decode to
 * the SVG type (S4 T-08 finding).
 */
function isRasterImage(url) {
  const type = /^data:([^;,]*)/.exec(url)[1];
  return type.startsWith('image/') && !url.startsWith('data:image/svg+xml') && !/[%&]/.test(type);
}

/** Items 2 to 4.2 for one URL value. `imageOk` is true where H-83 allows a raster data: image. */
function urlRejected(value, imageOk) {
  if (decisionReferenceRejected(value)) return true;
  const url = normalizedUrl(value);
  if (/^(?:javascript|vbscript):/.test(url)) return true;
  if (!url.startsWith('data:')) return false;
  return !(imageOk && isRasterImage(url));
}

/** Item 4.3 (DL-048 c): a reference that decodes to ASCII white space, ",", "(" or ")". */
const SRCSET_SPLIT_REFERENCE =
  /&#(?:0*(?:9|1[023]|32|4[014])(?![0-9])|[xX]0*(?:[9aAcCdD]|2[0cC89])(?![0-9A-Fa-f]))|&(?:Tab|NewLine|comma|lpar|rpar);/;

/**
 * Item 4.3 (H-83): the candidate URLs of a srcset value as "parse a srcset attribute" finds
 * them: skip white space and commas, take the run up to white space, drop its trailing
 * commas; descriptors follow when the run had no trailing comma and end at a comma that is
 * not in parentheses.
 */
function srcsetCandidates(value) {
  const urls = [];
  const skip = /[\t\n\f\r ,]*/y;
  const run = /[^\t\n\f\r ]+/y;
  let i = 0;
  while (i < value.length) {
    skip.lastIndex = i;
    i += skip.exec(value)[0].length;
    if (i >= value.length) break;
    run.lastIndex = i;
    const raw = run.exec(value)[0];
    i += raw.length;
    const url = raw.replace(/,+$/, '');
    urls.push(url);
    if (url.length < raw.length) continue;
    let depth = false;
    while (i < value.length && (depth || value[i] !== ',')) {
      if (value[i] === '(') depth = true;
      else if (value[i] === ')') depth = false;
      i++;
    }
    i++; // the comma
  }
  return urls;
}

/**
 * Manifest-patch section 4.2 for one attribute of a start tag `element` (lower case).
 * `inHtml` is false in a foreign region. Returns true when the rule rejects it.
 */
function rejected({ name, value }, element, inHtml) {
  if (name.startsWith('on')) return true;
  const places = inHtml && Object.hasOwn(IMAGE_ATTRIBUTES, element) ? IMAGE_ATTRIBUTES[element] : [];
  if (name !== 'srcset') return urlRejected(value, places.includes(name));
  if (SRCSET_SPLIT_REFERENCE.test(value)) return true;
  return srcsetCandidates(value).some((url) => urlRejected(url, places.includes('src')));
}

/** Depth limit of the open-element stack; a deeper block fails the rule (fail closed). */
const MAX_OPEN = 256;

/**
 * Manifest-patch section 4, item 1: true when the html block content passes the patch
 * content rule. No start tag of PATCH_FORBIDDEN in HTML context, no start tag outside
 * FOREIGN_ALLOWED in a foreign region (svg, math; H-73), and every attribute of every
 * start tag passes the attribute rule of section 4.2. `content` is a Buffer. The rule
 * reads the tags as the analysis does: the stack of open elements finds the region, and
 * the text-mode state changes of M-04 apply only in HTML context (in a region a title or
 * style start tag opens no RCDATA or RAWTEXT state).
 */
export function passesPatchContentRule(content) {
  if (content.length > MAX_BLOCK_BYTES) return false;
  const tokenizer = new Tokenizer(content);
  const open = [];
  let region = -1; // index in `open` of the svg or math element that started the region
  for (;;) {
    const token = tokenizer.next();
    if (token.type === TokenType.EOF) return true;
    if (token.type === TokenType.END_TAG) {
      const at = open.lastIndexOf(token.name);
      if (at >= 0) {
        open.length = at;
        if (region >= at) region = -1;
      }
      continue;
    }
    if (token.type !== TokenType.START_TAG) continue;
    const inHtml = region < 0;
    if (inHtml ? PATCH_FORBIDDEN.has(token.name) : !FOREIGN_ALLOWED.has(token.name)) return false;
    const tag = content.toString('latin1', token.start, token.end);
    if (attributesOf(tag).some((attribute) => rejected(attribute, token.name, inHtml))) return false;
    const root = token.name === 'svg' || token.name === 'math';
    const opens = inHtml ? !(root && token.selfClosing) && !VOID.has(token.name) : !token.selfClosing;
    if (opens) {
      if (open.length >= MAX_OPEN) return false;
      open.push(token.name);
      if (inHtml && root) region = open.length - 1;
    }
    const mode = inHtml ? TEXT_MODE.get(token.name) : undefined;
    if (mode !== undefined) tokenizer.switchTo(mode, token.name);
  }
}
