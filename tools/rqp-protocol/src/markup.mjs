// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// Markup functions of the reference steps. The markup analysis itself is the
// Node.js parser candidate (tools/rqp-markup/node, no runtime dependency). This
// module adds what the protocol needs on top of it, with the same tokenizer:
//
// - the insertion point of the manifest: the head start tag of the document or
//   an implied head (manifest-patch section 1.7, H-72, H-79a);
// - the manifest elements of a full document (section 1.7, client items 1 to 4);
// - the patch content rule for patched html content (section 4, item 1: the
//   elements of PATCH_FORBIDDEN, FOREIGN_ALLOWED and the attribute rule of
//   section 4.2; H-48, H-59, H-73, H-77, H-78, H-83). The server step and the
//   client step call the same function, patchContentFailure.

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
 * H-79a: the start tags that end item 1 of the head rule with an implied
 * head. In the "before head" insertion mode each of them is "anything else":
 * the parser inserts a head element and reprocesses the tag in "in head".
 */
export const IMPLIED_HEAD = Object.freeze(new Set(['base', 'basefont', 'bgsound', 'link', 'meta', 'noframes', 'script', 'style', 'title']));

/**
 * The insertion point of the manifest element (manifest-patch section 1.7,
 * "Document head"; server item 2 and client item 3; H-72, H-79a): the byte
 * offset immediately after the ">" of the head start tag of the document, or,
 * for an implied head, the offset of the "<" of the start tag that ends item 1
 * (a name of IMPLIED_HEAD). -1 when the document has neither. The server
 * inserts the manifest element at this offset of the original document; the
 * client computes it on the original document O (F without the manifest
 * element, client rule C2) and requires the manifest start tag there. Both
 * call this function. The walk reads each token before the stop once and
 * stops there; it keeps no stack.
 */
export function documentHeadStart(bytes) {
  const tok = new Tokenizer(bytes);
  let prevEnd = 0;
  for (;;) {
    const t = tok.next();
    if (!onlyWhiteSpace(bytes, prevEnd, t.start)) return -1;
    if (t.type === TokenType.START_TAG && t.name === 'head') return t.end;
    if (t.type === TokenType.START_TAG && IMPLIED_HEAD.has(t.name)) return t.start;
    if (!beforeHead(t)) return -1;
    prevEnd = t.end;
  }
}

/**
 * Walk the tokens of a document with the token stack of markup M-05 and the
 * state changes of M-04, and call `visit(token, html)` for each start tag and
 * `visit(token)` for each end tag; `html` is true for a start tag in HTML
 * context and false for one in a foreign region. Returns false when the walk
 * stopped early (the stack limit was hit).
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
      visit(t, html);
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
 * The manifest elements of a full document. Returns { elements: [{ start,
 * textStart, textEnd, end }] }, or null when the walk could not finish (fail
 * closed). `end` is -1 for an element without an end tag. The position check
 * (client item 3, H-79a) is not here: it needs the original document.
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
  return done ? { elements } : null;
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

// ---------------------------------------------------------------------------
// Patch content rule (manifest-patch section 4, item 1, and section 4.2).

/** Item 1.1 (H-78): the start tags in HTML context that patched html content must not have. */
export const PATCH_FORBIDDEN = Object.freeze(
  new Set(['base', 'basefont', 'bgsound', 'embed', 'iframe', 'link', 'meta', 'object', 'script', 'style']),
);

/** Item 1.2 (H-73, H-78): the only start tags allowed in a foreign region of patched html content. */
export const FOREIGN_ALLOWED = Object.freeze(
  new Set([
    'svg', 'g', 'path', 'circle', 'ellipse', 'line', 'polyline', 'polygon', 'rect', 'defs', 'symbol', 'use',
    'title', 'desc', 'lineargradient', 'radialgradient', 'stop', 'clippath', 'mask', 'pattern', 'marker', 'text',
    'tspan', 'filter', 'feblend', 'fecolormatrix', 'fecomponenttransfer', 'fecomposite', 'fedisplacementmap',
    'fedropshadow', 'feflood', 'fefunca', 'fefuncb', 'fefuncg', 'fefuncr', 'fegaussianblur', 'femerge',
    'femergenode', 'femorphology', 'feoffset', 'feturbulence', 'math', 'mrow', 'mi', 'mo', 'mn', 'ms', 'mtext',
    'mspace', 'msub', 'msup', 'msubsup', 'mfrac', 'msqrt', 'mroot', 'mstyle', 'munder', 'mover', 'munderover',
    'mmultiscripts', 'mprescripts', 'none', 'mtable', 'mtr', 'mtd', 'mpadded', 'mphantom', 'merror',
    'semantics', 'annotation',
  ]),
);

const isDigit = (c) => c >= 0x30 && c <= 0x39;
const isLetter = (c) => (c >= 0x41 && c <= 0x5a) || (c >= 0x61 && c <= 0x7a);
const isHexDigit = (c) => isDigit(c) || (c >= 0x41 && c <= 0x46) || (c >= 0x61 && c <= 0x66);

/** Markup M-13: a numeric reference value larger than 10FFFF reads as 110000. */
const REFERENCE_CAP = 0x110000;

/**
 * The value of the numeric character reference that starts at the "&" at
 * offset `at` of the raw bytes (the callers find that "&"), read as markup
 * M-13 says (the WHATWG tokenizer): "&#" and ASCII digits, or "&#x" or "&#X"
 * and ASCII hex digits; all digits, also leading zeros; ";" optional; capped at
 * 110000. -1 when no numeric reference starts there ("&#", "&#x" or "&#X"
 * without a digit is none).
 */
function numericReference(raw, at) {
  if (raw[at + 1] !== 0x23) return -1;
  let p = at + 2;
  const hex = raw[p] === 0x78 || raw[p] === 0x58;
  if (hex) p++;
  const first = p;
  let value = 0;
  for (; p < raw.length && (hex ? isHexDigit(raw[p]) : isDigit(raw[p])); p++) {
    value = Math.min(value * (hex ? 16 : 10) + Number.parseInt(String.fromCharCode(raw[p]), 16), REFERENCE_CAP);
  }
  return p === first ? -1 : value;
}

/** Item 2 (H-77): after the leading bytes 00 to 20, the walk to the decision byte skips these bytes. */
const skippedBeforeDecision = (c) =>
  c === 0x09 || c === 0x0a || c === 0x0d || isDigit(c) || isLetter(c) || c === 0x2b || c === 0x2d || c === 0x2e;

/** Item 2.1 (H-77): a reference value that the URL parser strips, or reads as part of a scheme or as its ":". */
const schemeValue = (v) => v <= 0x20 || v === 0x3a || v === 0x2b || v === 0x2d || v === 0x2e || isDigit(v) || isLetter(v);

/** Item 2.2 (H-77): the named references that decode to such a character (each needs its ";"; case-sensitive). */
const SCHEME_NAMED = Object.freeze(['Tab;', 'NewLine;', 'colon;', 'fjlig;', 'period;', 'plus;'].map((n) => Buffer.from(n, 'latin1')));

/** True when the bytes of `raw` at offset `at` are the bytes of `name`. */
const bytesAt = (raw, at, name) => raw.subarray(at, at + name.length).equals(name);

/**
 * Item 2 (H-77): true when the decision byte of the raw value is a "&" whose
 * character reference can continue or complete a scheme. The reference is read
 * on the raw bytes, never on the copy of item 3 (the trap `java&#x73<LF>cript:`).
 */
function decisionReferenceRejects(raw) {
  let p = 0;
  while (p < raw.length && raw[p] <= 0x20) p++;
  while (p < raw.length && skippedBeforeDecision(raw[p])) p++;
  if (raw[p] !== 0x26) return false; // no decision byte, or not "&"
  const value = numericReference(raw, p);
  if (value >= 0) return schemeValue(value);
  return SCHEME_NAMED.some((name) => bytesAt(raw, p + 1, name));
}

/** ASCII lower case of a Latin-1 text: only A-Z change. */
const asciiLowerText = (s) => s.replace(/[A-Z]+/g, (m) => m.toLowerCase());

/**
 * Item 3: the copy of the raw value without the bytes 09, 0A and 0D and without
 * the leading bytes 00 to 20, in ASCII lower case for item 4 (one Latin-1
 * character per byte). Character references stay as they are (H-77).
 */
const schemeCopy = (raw) => asciiLowerText(raw.toString('latin1').replace(/[\t\n\r]/g, '').replace(/^[\x00-\x20]+/, ''));

/**
 * H-83: the elements and attributes (HTML context only) where a raster data:
 * image can stay. srcset is not listed: item 4.3 reads each of its candidate
 * URLs as the value of src of the same element (img and source).
 */
const DATA_IMAGE_ATTRIBUTES = Object.freeze(
  new Map([
    ['img', ['src']],
    ['source', ['src']],
    ['video', ['poster']],
  ]),
);

/** H-83: true for an element and an attribute of DATA_IMAGE_ATTRIBUTES in HTML context. */
function dataImageAllowed(element, name, html) {
  return html === true && DATA_IMAGE_ATTRIBUTES.has(element) && DATA_IMAGE_ATTRIBUTES.get(element).includes(name);
}

/**
 * Item 4.2 (H-83, H-83a, DL-048 b): the copy is a data: URL of a raster image.
 * It starts with "data:image/" and not with "data:image/svg+xml", and its type
 * part (the bytes after "data:" up to the first ";" or ",") has no "%". The type
 * part also has no "&": a reference there (`data:image/svg&#43;xml`) decodes to
 * the SVG type in the browser (DL-051).
 */
function rasterDataImage(copy) {
  if (!copy.startsWith('data:image/') || copy.startsWith('data:image/svg+xml')) return false;
  const typePart = /^data:([^;,]*)/.exec(copy)[1];
  return !typePart.includes('%') && !typePart.includes('&');
}

/** Items 2 to 4.2 for one URL value: the whole value, or one candidate URL of srcset. */
function urlRejected(raw, dataImage) {
  if (decisionReferenceRejects(raw)) return true; // item 2
  const copy = schemeCopy(raw); // item 3
  if (copy.startsWith('javascript:') || copy.startsWith('vbscript:')) return true; // item 4.1
  return copy.startsWith('data:') && !(dataImage && rasterDataImage(copy)); // item 4.2
}

/** ASCII white space of the srcset parser (WHATWG Infra): 09, 0A, 0C, 0D, 20. */
const isAsciiWhiteSpace = (c) => c === 0x09 || c === 0x0a || c === 0x0c || c === 0x0d || c === 0x20;

/** Item 4.3 (DL-048 c): the reference values that decode to ASCII white space, ",", "(" or ")". */
const SPLIT_VALUES = Object.freeze(new Set([0x09, 0x0a, 0x0c, 0x0d, 0x20, 0x2c, 0x28, 0x29]));
const SPLIT_NAMED = Object.freeze(['&Tab;', '&NewLine;', '&comma;', '&lpar;', '&rpar;'].map((n) => Buffer.from(n, 'latin1')));

/** Item 4.3 (DL-048 c): true when the raw value has a reference that decodes to a byte of the srcset split. */
function splitReference(raw) {
  for (let i = raw.indexOf(0x26); i >= 0; i = raw.indexOf(0x26, i + 1)) {
    if (SPLIT_VALUES.has(numericReference(raw, i)) || SPLIT_NAMED.some((name) => bytesAt(raw, i, name))) return true;
  }
  return false;
}

/**
 * Item 4.3 (H-83): the candidate URLs of a srcset value, split on the raw bytes
 * as the WHATWG algorithm "parse a srcset attribute" splits the decoded value:
 * skip ASCII white space and commas; the URL is the next run of bytes that are
 * not ASCII white space, without its final commas; when the run did not end
 * with a comma, the descriptors after it end at a comma outside parentheses.
 */
export function srcsetUrls(raw) {
  const urls = [];
  let p = 0;
  for (;;) {
    while (p < raw.length && (isAsciiWhiteSpace(raw[p]) || raw[p] === 0x2c)) p++;
    if (p >= raw.length) return urls;
    const start = p;
    while (p < raw.length && !isAsciiWhiteSpace(raw[p])) p++;
    let end = p;
    while (raw[end - 1] === 0x2c) end--; // stops at `start`: the run starts with a byte that is not a comma
    urls.push(raw.subarray(start, end));
    if (end < p) continue; // the run ended with a comma: no descriptors
    let inParens = false;
    for (; p < raw.length; p++) {
      const c = raw[p];
      if (inParens) inParens = c !== 0x29;
      else if (c === 0x2c) break;
      else inParens = c === 0x28;
    }
  }
}

/**
 * Manifest-patch section 4.2 for one attribute of a start tag: true when the
 * rule rejects it. `name` is the attribute name in lower case, `value` the raw
 * value bytes, `element` the tag name in lower case and `html` true for a start
 * tag in HTML context. The data: exception of H-83 needs both; without them
 * (undefined) the rule fails closed.
 */
export function attributeRejected(name, value, element, html) {
  if (name.startsWith('on')) return true; // item 1
  if (name === 'srcset') {
    // Item 4.3: each candidate URL as the value of src of the same element.
    const dataImage = dataImageAllowed(element, 'src', html);
    return splitReference(value) || srcsetUrls(value).some((url) => urlRejected(url, dataImage));
  }
  return urlRejected(value, dataImageAllowed(element, name, html)); // items 2 to 4.2
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
 * Manifest-patch section 4, item 1 (H-48, H-73, H-78): one function for the
 * server step (H-56: F instead of a delta) and the client step (section 2.4,
 * step 9). Returns null when the html content passes, else the reason:
 * "element" (a start tag of PATCH_FORBIDDEN in HTML context), "foreign" (a
 * start tag outside FOREIGN_ALLOWED in a foreign region), "attribute" (section
 * 4.2), "size" (more than maxBlockBytes) or "walk" (the token stack limit). The
 * content is tokenized from the data state with the token stack of M-05 and
 * the state changes of M-04 only in HTML context, as in the analysis of the
 * patched document. Names are compared in lower case, as the tokenizer gives
 * them; a self-closing flag does not change the result; end tags do not matter.
 */
export function patchContentFailure(content) {
  if (content.length > LIMITS.maxBlockBytes) return 'size';
  let failure = null;
  const done = walk(content, (t, html) => {
    if (failure !== null || t.type !== TokenType.START_TAG) return;
    if (html && PATCH_FORBIDDEN.has(t.name)) failure = 'element';
    else if (!html && !FOREIGN_ALLOWED.has(t.name)) failure = 'foreign';
    else if (attributesOf(content, t.start, t.end).some(({ name, value }) => attributeRejected(name, value, t.name, html))) failure = 'attribute';
  });
  return done ? failure : 'walk';
}

/** True when the html content passes the patch content rule (patchContentFailure gives null). */
export function passesPatchContentRule(content) {
  return patchContentFailure(content) === null;
}
