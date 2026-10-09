// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// RQP markup rules rqp/1 (docs/engineering/protocol/markup.md section 2) and the
// result object, hashes and revisions of tools/rqp-markup/CONTRACT.md.
//
// One pass over the tokens of src/tokenizer.mjs with the token stack of M-05.
// Validity is decided on tokens only (DL-008). The first rule violation ends the
// pass. Time is linear in the input length: every stack entry is pushed and
// popped once, and each check is a constant number of map lookups.

import { createHash } from 'node:crypto';
import { isUtf8 } from 'node:buffer';
import { Tokenizer, TokenType, matchesLower } from './tokenizer.mjs';
import { isJsonText } from './json.mjs';
import {
  BLOCK_FAMILY,
  BLOCK_HTML,
  BREAKOUT,
  FORBIDDEN_CONTENT,
  GUARDED,
  HEADINGS,
  INTEGRATION,
  LISTS,
  NOSCRIPT_FORBIDDEN,
  PCLOSE,
  SELECT_END,
  SELECT_START,
  TABLE_FAMILY,
  TABLE_PARENTS,
  TABLE_PART,
  TEXT,
  TEXT_MODE,
  VOID,
} from './names.mjs';

/** Limits of M-01 and M-26 (docs/engineering/core/resource-limits.md). */
export const LIMITS = Object.freeze({
  maxDocumentBytes: 5 * 1024 * 1024,
  maxBlocks: 256,
  maxBlockBytes: 1024 * 1024,
});

/** Error codes of CONTRACT.md section 5. */
export const ErrorCode = Object.freeze({
  ENCODING: 'RQP_MARKUP_ENCODING',
  LIMIT: 'RQP_MARKUP_LIMIT',
  INVALID_ID: 'RQP_MARKUP_INVALID_ID',
  FORBIDDEN_ELEMENT: 'RQP_MARKUP_FORBIDDEN_ELEMENT',
  DUPLICATE: 'RQP_MARKUP_DUPLICATE',
  NESTED: 'RQP_MARKUP_NESTED',
  STRUCTURE: 'RQP_MARKUP_STRUCTURE',
  BAD_JSON: 'RQP_MARKUP_BAD_JSON',
  RESERVED: 'RQP_MARKUP_RESERVED',
});

const STRUCTURE = ErrorCode.STRUCTURE;
const JSON_TYPE = Buffer.from('application/json', 'latin1');
const MANIFEST_MARK = 'rivqen-manifest';
const NOSCRIPT_END = Buffer.from('</noscript>', 'latin1');
const AMPERSAND = 0x26;

/** The invalid result object (CONTRACT.md section 3). */
export function invalid(code) {
  return { valid: false, error: code, blocks: [], template_revision: null, page_revision: null };
}

// M-15: ^[a-z0-9][a-z0-9_-]{0,63}$ on the raw value bytes.
function isValidId(b, span) {
  const len = span.end - span.start;
  if (len < 1 || len > 64) return false;
  for (let i = span.start; i < span.end; i++) {
    const c = b[i];
    const alnum = (c >= 0x61 && c <= 0x7a) || (c >= 0x30 && c <= 0x39);
    if (!alnum && (i === span.start || (c !== 0x5f && c !== 0x2d))) return false;
  }
  return true;
}

// M-13: the raw type value contains '&' or "rivqen-manifest" (ASCII case-insensitive).
function isReservedType(b, span) {
  if (b.subarray(span.start, span.end).includes(AMPERSAND)) return true;
  for (let i = span.start; i + MANIFEST_MARK.length <= span.end; i++) {
    if (matchesLower(b, i, MANIFEST_MARK)) return true;
  }
  return false;
}

function isJsonType(b, span) {
  return b.subarray(span.start, span.end).equals(JSON_TYPE);
}

// M-12: a closed '<![CDATA[' bogus comment must end with ']]>'. That ']]>'
// always starts after '<![CDATA[': the bytes before its '>' at offsets 7 and 8
// are 'A' and '['. A comment that runs to EOF has no '>' (no rule applies).
function isCdataSafe(b, t) {
  return !t.closed || (b[t.end - 3] === 0x5d && b[t.end - 2] === 0x5d);
}

function increment(map, key) {
  map.set(key, (map.get(key) ?? 0) + 1);
}

function decrement(map, key) {
  map.set(key, (map.get(key) ?? 0) - 1);
}

/**
 * M-11: tokenize the noscript content C followed by "</noscript>" alone from
 * the data state (HTML context). Returns an error code or null.
 */
export function checkNoscript(content) {
  const tok = new Tokenizer(Buffer.concat([content, NOSCRIPT_END]));
  for (;;) {
    const t = tok.next();
    if (t.type === TokenType.START_TAG) {
      if (t.blockAttr !== null || NOSCRIPT_FORBIDDEN.has(t.name)) return STRUCTURE;
      const mode = TEXT_MODE.get(t.name);
      if (mode !== undefined) tok.switchTo(mode, t.name);
    } else if (t.type === TokenType.END_TAG) {
      if (t.name === 'noscript') return t.start === content.length ? null : STRUCTURE;
    } else if (t.type === TokenType.EOF) {
      return STRUCTURE;
    }
  }
}

/** One pass over the tokens: the token stack (M-05) and rules M-06 to M-26. */
class Scanner {
  constructor(bytes) {
    this.b = bytes;
    // M-02: the tokenizer must not see a BOM at offset 0. Its bytes EF BB BF are
    // not ASCII, so they never change a token boundary and need no skip.
    this.tok = new Tokenizer(bytes);
    // Token stack (M-05), one entry per index in each array.
    this.names = [];
    this.inHtml = []; // pushed in HTML context
    this.family = []; // nearest TABLE_FAMILY name at or below the entry, null after template (M-09)
    this.byName = new Map(); // name -> stack indices, for the nearest match of an end tag
    this.htmlOpen = new Map(); // name -> count of open elements pushed in HTML context
    this.contentOpen = new Map(); // name -> count of open elements opened in the block content
    this.region = -1; // stack index of the svg or math element of the foreign region
    this.charsOnly = null; // M-07: name of the element whose content must be text only
    this.noscriptStart = -1; // M-11: offset of the noscript content
    this.pending = null; // block of the start tag that is processed now
    this.block = null; // open block: { id, format, start, index }
    this.blocks = [];
    this.ids = new Set();
  }

  /** Returns { error } or { blocks }. */
  run() {
    for (;;) {
      const t = this.tok.next();
      let error;
      switch (t.type) {
        case TokenType.START_TAG:
          error = this.startTag(t);
          break;
        case TokenType.END_TAG:
          error = this.endTag(t);
          break;
        case TokenType.COMMENT:
          error = this.comment(t);
          break;
        case TokenType.DOCTYPE:
          error = this.doctype();
          break;
        default:
          error = this.eof(t);
          return error === null ? { blocks: this.blocks } : { error };
      }
      if (error !== null) return { error };
    }
  }

  isOpen(name) {
    return (this.htmlOpen.get(name) ?? 0) > 0;
  }

  isOpenInContent(name) {
    return (this.contentOpen.get(name) ?? 0) > 0;
  }

  // M-08. Only a select pushed in HTML context counts. While it is open, no
  // foreign region can start (svg and math are not allowed in it), so the check
  // needs no context test.
  inSelect() {
    return this.isOpen('select');
  }

  // F of M-09 and M-19.
  currentFamily() {
    const n = this.family.length;
    return n === 0 ? null : this.family[n - 1];
  }

  push(name, inHtml) {
    const i = this.names.length;
    this.names.push(name);
    this.inHtml.push(inHtml);
    const below = this.currentFamily();
    this.family.push(inHtml && TABLE_FAMILY.has(name) ? name : name === 'template' ? null : below);
    const list = this.byName.get(name);
    if (list === undefined) this.byName.set(name, [i]);
    else list.push(i);
    if (inHtml) increment(this.htmlOpen, name);
    if (this.block !== null) increment(this.contentOpen, name);
    if (inHtml && (name === 'svg' || name === 'math')) this.region = i;
  }

  pop() {
    const i = this.names.length - 1;
    const name = this.names.pop();
    const inHtml = this.inHtml.pop();
    this.family.pop();
    this.byName.get(name).pop();
    if (inHtml) decrement(this.htmlOpen, name);
    if (this.block !== null && i > this.block.index) decrement(this.contentOpen, name);
    if (i === this.region) this.region = -1;
  }

  nearest(name) {
    const list = this.byName.get(name);
    return list === undefined || list.length === 0 ? -1 : list[list.length - 1];
  }

  startTag(t) {
    if (this.charsOnly !== null) return STRUCTURE; // M-07 rules 3 and 4
    if (t.name === 'frameset') return STRUCTURE; // M-10
    if (t.name === 'script' && t.typeAttr !== null && isReservedType(this.b, t.typeAttr)) {
      return ErrorCode.RESERVED; // M-13
    }
    if (this.inSelect() && !SELECT_START.has(t.name)) return STRUCTURE; // M-08
    const foreign = this.region >= 0;
    if (t.blockAttr !== null) {
      const error = this.blockStart(t, foreign);
      if (error !== null) return error;
    }
    return foreign ? this.foreignStart(t) : this.htmlStart(t);
  }

  // M-14 to M-19, M-21, M-26 (count), in the order of markup.md section 2.10.
  blockStart(t, foreign) {
    const b = this.b;
    if (this.block !== null) return ErrorCode.NESTED; // M-17
    if (!isValidId(b, t.blockAttr)) return ErrorCode.INVALID_ID; // M-15
    let format;
    if (foreign) return ErrorCode.FORBIDDEN_ELEMENT; // M-16
    if (BLOCK_HTML.has(t.name)) format = 'html';
    else if (t.name === 'script' && t.typeAttr !== null && isJsonType(b, t.typeAttr)) format = 'json';
    else return ErrorCode.FORBIDDEN_ELEMENT; // M-16
    if (t.selfClosing) return STRUCTURE; // M-18
    if (this.isOpen('template') || this.inSelect()) return STRUCTURE; // M-19 rule 1
    const f = this.currentFamily();
    if (f !== null && !BLOCK_FAMILY.has(f)) return STRUCTURE; // M-19 rule 2
    const id = b.toString('latin1', t.blockAttr.start, t.blockAttr.end);
    if (this.ids.has(id)) return ErrorCode.DUPLICATE; // M-21
    if (this.blocks.length >= LIMITS.maxBlocks) return ErrorCode.LIMIT; // M-26
    this.ids.add(id);
    this.pending = { id, format };
    return null;
  }

  // M-05 rule 2 and M-07 rules 1, 3, 4.
  foreignStart(t) {
    const name = t.name;
    if (BREAKOUT.has(name)) return STRUCTURE;
    const text = TEXT.has(name);
    if (text && (t.selfClosing || name === 'plaintext')) return STRUCTURE;
    if (!t.selfClosing) {
      this.push(name, false);
      if (text || INTEGRATION.has(name)) this.charsOnly = name;
    }
    return null;
  }

  // M-05 rule 1, M-04, M-09, and M-22 to M-24 in block content.
  htmlStart(t) {
    const name = t.name;
    if (this.block !== null) {
      const error = this.contentStart(t);
      if (error !== null) return error;
    }
    const parents = TABLE_PARENTS.get(name);
    if (parents !== undefined) {
      const f = this.currentFamily();
      if (f !== null && !parents.has(f)) return STRUCTURE; // M-09
    }
    const foreignRoot = name === 'svg' || name === 'math';
    if (!(foreignRoot && t.selfClosing) && !VOID.has(name)) this.push(name, true);
    const mode = TEXT_MODE.get(name);
    if (mode !== undefined) {
      this.tok.switchTo(mode, name); // M-04, also with the self-closing flag
      if (name === 'noscript') this.noscriptStart = t.end;
    }
    if (this.pending !== null) {
      const { id, format } = this.pending;
      this.pending = null;
      this.block = {
        id,
        format,
        start: t.end,
        index: this.names.length - 1,
      };
    }
    return null;
  }

  // M-22 and M-24 for a start tag in the content of an html block.
  contentStart(t) {
    const name = t.name;
    if (FORBIDDEN_CONTENT.has(name)) return STRUCTURE; // M-22 rule 2
    if (t.selfClosing && !VOID.has(name)) return STRUCTURE; // M-22 rule 3
    const blockName = this.names[this.block.index];
    if (PCLOSE.has(name) && (blockName === 'p' || blockName === 'span')) return STRUCTURE; // a
    if (HEADINGS.has(name) && (HEADINGS.has(blockName) || this.headingInContent())) {
      return STRUCTURE; // b
    }
    if ((name === 'li' || name === 'dd' || name === 'dt') && !this.listInContent()) {
      return STRUCTURE; // c
    }
    if ((name === 'button' || name === 'a' || name === 'nobr') && this.isOpen(name)) {
      return STRUCTURE; // d, e, f
    }
    if ((TABLE_PART.has(name) || name === 'col') && !this.isOpenInContent('table')) {
      return STRUCTURE; // g
    }
    if (name === 'rb' || name === 'rtc' || name === 'rp' || name === 'rt') {
      if (!this.isOpenInContent('ruby')) return STRUCTURE; // h, i
      const open = this.isOpen('rb') || this.isOpen('rp') || this.isOpen('rt');
      if (open || ((name === 'rb' || name === 'rtc') && this.isOpen('rtc'))) return STRUCTURE;
    }
    return null;
  }

  headingInContent() {
    for (const h of HEADINGS) if (this.isOpenInContent(h)) return true;
    return false;
  }

  listInContent() {
    for (const l of LISTS) if (this.isOpenInContent(l)) return true;
    return false;
  }

  endTag(t) {
    const name = t.name;
    if (this.noscriptStart >= 0) {
      // The tokenizer was in RAWTEXT: this is the end tag of the noscript element.
      const error = checkNoscript(this.b.subarray(this.noscriptStart, t.start));
      this.noscriptStart = -1;
      if (error !== null) return error;
    }
    if (this.charsOnly !== null) {
      if (name !== this.charsOnly) return STRUCTURE; // M-07 rules 3 and 4
      this.charsOnly = null;
    }
    if (this.inSelect() && !SELECT_END.has(name)) return STRUCTURE; // M-08
    const foreign = this.region >= 0;
    if (this.block !== null) return this.contentEnd(t);
    const idx = this.nearest(name);
    if (foreign) {
      if (idx < this.region) return STRUCTURE; // M-07 rule 2: stray end tag
    } else {
      if (idx < 0) return null; // M-05 rule 3: no match, ignored
      for (let i = this.names.length - 1; i > idx; i--) {
        const crossed = this.names[i];
        if (this.inHtml[i] && GUARDED.has(crossed) && !(TABLE_FAMILY.has(name) && TABLE_PART.has(crossed))) {
          return STRUCTURE; // M-06
        }
      }
    }
    while (this.names.length > idx) this.pop();
    return null;
  }

  // End tag while a block is open: M-20, M-22 rule 4, M-23.
  //
  // M-23: the end tag must have the name of the current node. This also gives
  // M-22 rule 4: a void element is never on the stack, so its end tag never
  // matches. A title or json block has no tags in its content (RCDATA, script
  // data), so its appropriate end tag has the name of the block element, which
  // is the current node.
  contentEnd(t) {
    const top = this.names.length - 1;
    if (this.names[top] !== t.name) return STRUCTURE; // M-23
    if (top !== this.block.index) {
      this.pop();
      return null;
    }
    const { id, format, start } = this.block;
    const end = t.start; // the '<' of the block end tag (M-20)
    if (end - start > LIMITS.maxBlockBytes) return ErrorCode.LIMIT; // M-26
    if (format === 'json' && !isJsonText(this.b, start, end)) return ErrorCode.BAD_JSON; // M-25
    this.blocks.push({ id, format, start, end });
    this.block = null;
    this.pop();
    return null;
  }

  comment(t) {
    if (this.charsOnly !== null) return STRUCTURE; // M-07 rules 3 and 4
    if (t.cdata && !isCdataSafe(this.b, t)) return STRUCTURE; // M-12
    return null;
  }

  doctype() {
    if (this.charsOnly !== null) return STRUCTURE; // M-07 rules 3 and 4
    if (this.block !== null) return STRUCTURE; // M-22 rule 1
    if (this.inSelect()) return STRUCTURE; // M-08
    return null;
  }

  eof(t) {
    if (this.noscriptStart >= 0) {
      // The noscript content runs to the end of the input.
      const error = checkNoscript(this.b.subarray(this.noscriptStart, t.start));
      if (error !== null) return error;
    }
    if (this.block !== null) return STRUCTURE; // M-20
    if (this.inSelect()) return STRUCTURE; // M-08: no explicit end tag
    return null;
  }
}

function base64urlSha256(...parts) {
  const h = createHash('sha256');
  for (const p of parts) h.update(p);
  return h.digest('base64url');
}

/** Hashes and revisions of CONTRACT.md section 4 (M-27). */
export function buildResult(bytes, found) {
  const template = createHash('sha256');
  template.update('rqp-t1\n');
  const blocks = [];
  let last = 0;
  for (const { id, format, start, end } of found) {
    template.update(bytes.subarray(last, start));
    blocks.push({ id, format, start, end, sha256: base64urlSha256(bytes.subarray(start, end)) });
    last = end;
  }
  template.update(bytes.subarray(last));
  const templateRevision = `t1.${template.digest('base64url')}`;
  let preimage = `rqp-r1\n${templateRevision}\n`;
  for (const { id, format, sha256 } of blocks) preimage += `${id}\t${format}\t${sha256}\n`;
  return {
    valid: true,
    error: null,
    blocks,
    template_revision: templateRevision,
    page_revision: `r1.${base64urlSha256(preimage)}`,
  };
}

/**
 * Check a document against markup.md rqp/1 and compute the result object of
 * CONTRACT.md section 3. Never throws for a Uint8Array input.
 *
 * @param {Uint8Array} input the document bytes
 */
export function analyze(input) {
  if (!(input instanceof Uint8Array)) {
    throw new TypeError('analyze() needs a Uint8Array');
  }
  const bytes = Buffer.from(input.buffer, input.byteOffset, input.byteLength);
  if (bytes.length > LIMITS.maxDocumentBytes) return invalid(ErrorCode.LIMIT); // M-01
  if (!isUtf8(bytes)) return invalid(ErrorCode.ENCODING); // M-02
  const scan = new Scanner(bytes).run();
  if (scan.error !== undefined) return invalid(scan.error);
  return buildResult(bytes, scan.blocks);
}
