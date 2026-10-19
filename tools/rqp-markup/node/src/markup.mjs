// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// RQP markup rules rqp/1 (docs/engineering/protocol/markup.md section 2) and the
// result object, hashes and revisions of tools/rqp-markup/CONTRACT.md.
//
// One pass over the tokens of src/tokenizer.mjs with the token stack of M-05.
// Validity is decided on tokens only (DL-008). The first rule violation ends the
// pass. Time is linear in the input length: every stack entry is pushed and
// popped once, and each check is a constant number of map lookups. Every entry
// of the token stack holds at least one unit of the depth counter D, and
// D <= LIMITS.maxOpenElements (M-26), so the stack has at most 256 entries. The
// noscript content C (M-11) is scanned a second time, in place, on the same
// stack above the noscript element, so its elements count in the same bound.

import { createHash } from 'node:crypto';
import { isUtf8 } from 'node:buffer';
import { Tokenizer, TokenType, matchesLower } from './tokenizer.mjs';
import { isJsonText } from './json.mjs';
import {
  BLOCK_FAMILY,
  BLOCK_HTML,
  BREAKOUT,
  DOCUMENT_TAGS,
  FORBIDDEN_CONTENT,
  FORMATTING,
  GUARDED,
  HEADINGS,
  INTEGRATION,
  LISTS,
  LIST_ITEMS,
  NOSCRIPT_FORBIDDEN,
  PCLOSE,
  SELECT_END,
  SELECT_START,
  SPECIAL,
  STRAY_TABLE_END,
  TABLE_CONTEXT,
  TABLE_FAMILY,
  TABLE_PARENTS,
  TABLE_PART,
  TEXT,
  TEXT_MODE,
  VOID,
  internName,
} from './names.mjs';

/** Limits of M-01 and M-26 (docs/engineering/core/resource-limits.md). */
export const LIMITS = Object.freeze({
  maxDocumentBytes: 5 * 1024 * 1024,
  maxBlocks: 256,
  maxBlockBytes: 1024 * 1024,
  maxOpenElements: 256, // the largest value of the depth counter D (H-31)
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
const JSON_TYPES = [
  Buffer.from('application/json', 'latin1'),
  Buffer.from('application/ld+json', 'latin1'),
];
const MANIFEST_MARK = 'rivqen-manifest';
const NOSCRIPT_END = Buffer.from('</noscript>', 'latin1');
const AMPERSAND = 0x26;
const NON_ASCII = 0x80;

// State bits of a token-stack entry.
const DIRTY = 1; // M-26 item 5
const MARKED = 2; // M-06 item 4: special-crossed mark
const ROW_GROUP = 4; // M-26 item 3: implied parts of F
const ROW = 8;
const COLUMN_GROUP = 16;

// Values of the per-entry F index (M-09) that are not a stack index.
const NO_FAMILY = -1; // no TABLE_FAMILY entry below
const IN_TEMPLATE = -2; // a template entry is nearer than every TABLE_FAMILY entry (M-09 condition 1)

/** M-26 item 3: implied parts that a start tag in HTML context gives to F. */
function impliedParts(name, f) {
  if (f === 'table') {
    if (name === 'tr') return ROW_GROUP;
    if (name === 'td' || name === 'th') return ROW_GROUP | ROW;
    if (name === 'col') return COLUMN_GROUP;
    return 0;
  }
  if ((name === 'td' || name === 'th') && (f === 'tbody' || f === 'thead' || f === 'tfoot')) return ROW;
  return 0;
}

/** Number of implied parts in the state bits of an entry. */
function partCount(state) {
  return ((state & ROW_GROUP) >> 2) + ((state & ROW) >> 3) + ((state & COLUMN_GROUP) >> 4);
}

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

const CODE_CAP = 0x110000; // M-13 item 2: values above 10FFFF count as 110000

function isAsciiLetter(c) {
  return (c >= 0x41 && c <= 0x5a) || (c >= 0x61 && c <= 0x7a);
}

function isDigit(c) {
  return c >= 0x30 && c <= 0x39;
}

function hexValue(c) {
  if (isDigit(c)) return c - 0x30;
  if (c >= 0x41 && c <= 0x46) return c - 0x41 + 10;
  if (c >= 0x61 && c <= 0x66) return c - 0x61 + 10;
  return -1;
}

// M-13 item 2 (H-84): the value of the numeric character reference that starts
// after the '&' at i (the bytes '#', digits), read on the raw bytes. Returns -1
// when it is not a reference (no digit). The value stops at CODE_CAP.
function numericReference(b, i, end) {
  let p = i + 1; // after '#'
  let radix = 10;
  if (p < end && (b[p] === 0x78 || b[p] === 0x58)) {
    radix = 16;
    p++;
  }
  let value = 0;
  let digits = 0;
  for (; p < end; p++) {
    const d = radix === 16 ? hexValue(b[p]) : isDigit(b[p]) ? b[p] - 0x30 : -1;
    if (d < 0) break;
    value = Math.min(value * radix + d, CODE_CAP);
    digits++;
  }
  return digits === 0 ? -1 : value;
}

// The value gives a byte that a reader could take for a part of the manifest
// type, white space or a control (M-13 item 2, DL-048 a).
function isRiskyCode(v) {
  return v <= 0x20 || v >= NON_ASCII || isAsciiLetter(v) || isDigit(v)
    || v === 0x2f || v === 0x2b || v === 0x2d || v === 0x2e;
}

// M-13: the raw type value has a '&' that item 2 rejects, a byte >= 0x80 (item
// 3, decision H-28) or "rivqen-manifest" in ASCII case-insensitive form (item 1).
function isReservedType(b, span) {
  for (let i = span.start; i < span.end; i++) {
    const c = b[i];
    if (c >= NON_ASCII) return true;
    if (c !== AMPERSAND || i + 1 >= span.end) continue;
    if (isAsciiLetter(b[i + 1])) return true;
    if (b[i + 1] === 0x23) {
      const v = numericReference(b, i + 1, span.end);
      if (v >= 0 && isRiskyCode(v)) return true;
    }
  }
  for (let i = span.start; i + MANIFEST_MARK.length <= span.end; i++) {
    if (matchesLower(b, i, MANIFEST_MARK)) return true;
  }
  return false;
}

function isJsonType(b, span) {
  const value = b.subarray(span.start, span.end);
  return JSON_TYPES.some((t) => value.equals(t));
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

// A count that becomes 0 is removed, so a map has at most one key for each
// element on the token stack.
function decrement(map, key) {
  const count = (map.get(key) ?? 0) - 1;
  if (count > 0) map.set(key, count);
  else map.delete(key);
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
    this.family = []; // index of F at or below the entry (M-09): a stack index, NO_FAMILY or IN_TEMPLATE
    this.listMark = []; // nearest LISTS or LIST_ITEMS name at or below the entry (M-24 j)
    this.state = []; // DIRTY, MARKED and implied-part bits (M-06 item 4, M-26)
    this.depth = 0; // the depth counter D of M-26
    this.byName = new Map(); // name -> stack indices, for the nearest match of an end tag
    this.htmlOpen = new Map(); // name -> count of open elements pushed in HTML context
    this.contentOpen = new Map(); // name -> count of open elements opened in the block content
    this.region = -1; // stack index of the svg or math element of the foreign region
    this.charsOnly = null; // M-07: name of the element whose content must be text only
    this.noscriptStart = -1; // M-11: offset of the noscript content
    this.cBase = -1; // M-11: stack index of the noscript element while C is scanned, else -1
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
          t.name = internName(t.name);
          error = this.startTag(t);
          break;
        case TokenType.END_TAG:
          t.name = internName(t.name);
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
    return this.htmlOpen.has(name);
  }

  isOpenInContent(name) {
    return this.contentOpen.has(name);
  }

  // M-08. Only a select pushed in HTML context counts. While it is open, no
  // foreign region can start (svg and math are not allowed in it), so the check
  // needs no context test.
  inSelect() {
    return this.isOpen('select');
  }

  // Index of F of M-09 and M-19, else NO_FAMILY or IN_TEMPLATE (both < 0).
  familyIndex() {
    const n = this.family.length;
    return n === 0 ? NO_FAMILY : this.family[n - 1];
  }

  // Name of F of M-09 and M-19, or null.
  currentFamily() {
    const f = this.familyIndex();
    return f < 0 ? null : this.names[f];
  }

  // M-26: add n units to D. A step that makes D larger than the limit is an
  // error. The check comes first, so a rejected step changes no state.
  addDepth(n) {
    if (this.depth + n > LIMITS.maxOpenElements) return ErrorCode.LIMIT;
    this.depth += n;
    return null;
  }

  // M-05 push and M-26 item 2. Elements that M-05 does not push never come here.
  push(name, inHtml) {
    const error = this.addDepth(1);
    if (error !== null) return error;
    const i = this.names.length;
    const below = this.familyIndex();
    this.names.push(name);
    this.inHtml.push(inHtml);
    this.state.push(0);
    this.family.push(inHtml && TABLE_FAMILY.has(name) ? i : name === 'template' ? IN_TEMPLATE : below);
    // M-24 j. The entry can come from outside the block content: j is checked
    // only after c, which needs a list opened in the content, so the nearest
    // entry from the top is then always an element opened in the content.
    // svg and math regions never push these names (they are in BREAKOUT).
    const mark = LISTS.has(name) || LIST_ITEMS.has(name) ? name : this.listMark.at(-1);
    this.listMark.push(mark); // undefined for none
    const list = this.byName.get(name);
    if (list === undefined) this.byName.set(name, [i]);
    else list.push(i);
    if (inHtml) increment(this.htmlOpen, name);
    if (this.block !== null) increment(this.contentOpen, name);
    if (inHtml && (name === 'svg' || name === 'math')) this.region = i;
    return null;
  }

  pop() {
    const i = this.names.length - 1;
    const name = this.names.pop();
    const inHtml = this.inHtml.pop();
    this.family.pop();
    this.listMark.pop();
    this.state.pop();
    const list = this.byName.get(name);
    list.pop();
    if (list.length === 0) this.byName.delete(name);
    if (inHtml) decrement(this.htmlOpen, name);
    if (this.block !== null && i > this.block.index) decrement(this.contentOpen, name);
    if (i === this.region) this.region = -1;
  }

  nearest(name) {
    const list = this.byName.get(name);
    return list === undefined ? -1 : list[list.length - 1];
  }

  startTag(t) {
    if (this.charsOnly !== null) return STRUCTURE; // M-07 rules 3 and 4
    if (t.name === 'frameset') return STRUCTURE; // M-10
    if (t.name === 'script' && t.typeAttr !== null && isReservedType(this.b, t.typeAttr)) {
      return ErrorCode.RESERVED; // M-13
    }
    if (this.cBase >= 0 && this.noscriptRejects(t)) return STRUCTURE; // M-11 items 2, 3, 6, 7
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
      const error = this.push(name, false);
      if (error !== null) return error;
      if (text || INTEGRATION.has(name)) this.charsOnly = name;
    }
    return null;
  }

  // M-05 rule 1, M-04, M-09, M-10 items 2 and 3, M-26 item 3, and M-22 to M-24 in
  // block content. All checks come before the implied parts and the push.
  htmlStart(t) {
    const name = t.name;
    if (this.block !== null) {
      const error = this.contentStart(t);
      if (error !== null) return error;
    }
    // M-10 item 2 (H-36): the form element pointer of the tree builder.
    if (name === 'form' && this.isOpen('form')) return STRUCTURE;
    // M-10 item 3 (H-43): content patching (<template for>) would act on the
    // nodes between the markers, which the token model reads as comments.
    if (name === 'template' && t.hasFor) return STRUCTURE;
    const f = this.familyIndex();
    // M-09 condition 1 (H-35): "in column group" in a template ignores the
    // later start tags, also the raw text ones.
    if (name === 'col' && f === IN_TEMPLATE) return STRUCTURE;
    if (f >= 0) {
      const fName = this.names[f];
      const parents = TABLE_PARENTS.get(name);
      if (parents !== undefined && !parents.has(fName)) return STRUCTURE; // M-09
      // M-09 condition 2 (H-37): a table part, col or table start tag must not
      // make the tree builder pop foster-parented elements above F.
      const clears = parents !== undefined || name === 'table';
      if (clears && TABLE_CONTEXT.has(fName) && f !== this.names.length - 1) return STRUCTURE;
      // M-26 item 3: each implied part counts once for each F.
      const parts = impliedParts(name, fName) & ~this.state[f];
      const error = this.addDepth(partCount(parts));
      if (error !== null) return error;
      this.state[f] |= parts;
    }
    const foreignRoot = name === 'svg' || name === 'math';
    if (!(foreignRoot && t.selfClosing) && !VOID.has(name)) {
      const error = this.push(name, true);
      if (error !== null) return error;
    }
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
    // M-22 rule 3: a self-closing svg or math opens no region (M-05 item 1).
    if (t.selfClosing && !VOID.has(name) && name !== 'svg' && name !== 'math') return STRUCTURE;
    const blockName = this.names[this.block.index];
    if (PCLOSE.has(name) && (blockName === 'p' || blockName === 'span')) return STRUCTURE; // a
    if (HEADINGS.has(name) && (HEADINGS.has(blockName) || this.headingInContent())) {
      return STRUCTURE; // b
    }
    if (LIST_ITEMS.has(name)) {
      if (!this.listInContent()) return STRUCTURE; // c
      // j: the nearest list or list item opened in the content is an item, so
      // the WHATWG parser could close it (and the elements above it).
      if (LIST_ITEMS.has(this.listMark.at(-1))) return STRUCTURE;
    }
    if ((name === 'button' || name === 'a' || name === 'nobr') && this.isOpen(name)) {
      return STRUCTURE; // d, e, f
    }
    const tablePart = TABLE_PART.has(name) || name === 'col';
    if (tablePart && !this.isOpenInContent('table')) return STRUCTURE; // g
    // k: in table context the WHATWG parser foster-parents every other start
    // tag, and a table start tag closes the open table.
    if (!tablePart && TABLE_CONTEXT.has(this.currentFamily())) return STRUCTURE;
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
      const error = this.scanNoscript(this.b.subarray(this.noscriptStart, t.start));
      if (error !== null) return error;
    }
    if (this.charsOnly !== null) {
      if (name !== this.charsOnly) return STRUCTURE; // M-07 rules 3 and 4
      this.charsOnly = null;
    }
    if (this.inSelect() && !SELECT_END.has(name)) return STRUCTURE; // M-08
    if (this.cBase >= 0) return this.noscriptEnd(t);
    if (this.block !== null) return this.contentEnd(t);
    const idx = this.nearest(name);
    if (this.region >= 0) {
      if (idx < this.region) return STRUCTURE; // M-07 rule 2: stray end tag
    } else if (idx < 0) {
      // M-06 item 3 (H-30); other end tags without a match are ignored (M-05 rule 3).
      return STRAY_TABLE_END.has(name) ? STRUCTURE : null;
    }
    return this.close(idx, name);
  }

  // M-05 rule 3 for the matching entry idx: M-06 items 1, 2 and 4, then the
  // pops, then the change of D (M-26 items 4 and 5) and of the marks (M-06
  // item 4). The loop visits only the entries that the end tag pops.
  close(idx, name) {
    const top = this.names.length - 1;
    const formatting = FORMATTING.has(name);
    const tableEnd = TABLE_FAMILY.has(name);
    let crossesSpecial = false; // M-06 item 2: a crossed special or foreign element
    let setsMark = false; // M-06 item 4 (a): the crossing marks the new current node
    // M-06 item 4 (H-75): the end tag of the region root sets no mark by (a).
    const rootEnd = idx === this.region;
    let marked = (this.state[idx] & MARKED) !== 0;
    for (let i = top; i > idx; i--) {
      const crossed = this.names[i];
      const html = this.inHtml[i];
      if (html && GUARDED.has(crossed) && !(tableEnd && TABLE_PART.has(crossed))) {
        return STRUCTURE; // M-06 item 1
      }
      // M-06 item 2: a special element pushed in HTML context, or any element
      // pushed in a foreign region.
      if (!html || SPECIAL.has(crossed)) crossesSpecial = true;
      if (html ? SPECIAL.has(crossed) : !rootEnd) setsMark = true;
      if ((this.state[i] & MARKED) !== 0) marked = true;
    }
    if (formatting && (crossesSpecial || marked)) return STRUCTURE; // M-06 items 2 and 4
    const state = this.state[idx];
    const exact = idx === top && (state & DIRTY) === 0 && name !== 'html' && name !== 'body';
    if (exact) this.depth -= 1 + partCount(state); // M-26 item 4
    while (this.names.length > idx) this.pop();
    const current = this.names.length - 1;
    if (current >= 0) {
      if (!exact) this.state[current] |= DIRTY; // M-26 item 5
      if (setsMark || marked) this.state[current] |= MARKED; // M-06 item 4
    }
    return null;
  }

  // End tag while a block is open: M-20, M-22 rule 4, M-23.
  //
  // M-23: the end tag must have the name of the current node. This also gives
  // M-22 rule 4: a void element is never on the stack, so its end tag never
  // matches. A title or json block has no tags in its content (RCDATA, script
  // data), so its appropriate end tag has the name of the block element, which
  // is the current node. A stray table end tag (M-06 item 3) never matches.
  contentEnd(t) {
    const top = this.names.length - 1;
    // M-23 exception (H-75): the end tag of the region root pops the region.
    if (this.region >= 0 && this.nearest(t.name) === this.region) return this.close(this.region, t.name);
    if (this.names[top] !== t.name) return STRUCTURE; // M-23
    if (top === this.block.index) {
      const { id, format, start } = this.block;
      const end = t.start; // the '<' of the block end tag (M-20)
      if (end - start > LIMITS.maxBlockBytes) return ErrorCode.LIMIT; // M-26
      if (format === 'json' && !isJsonText(this.b, start, end)) return ErrorCode.BAD_JSON; // M-25
      this.blocks.push({ id, format, start, end });
      this.block = null;
    }
    return this.close(top, t.name);
  }

  // M-11 items 2, 3, 6 and 7 for a start tag of C, in HTML context and in a
  // foreign region of C. Item 4 is M-13 (checked before).
  noscriptRejects(t) {
    const name = t.name;
    if (t.blockAttr !== null || NOSCRIPT_FORBIDDEN.has(name) || DOCUMENT_TAGS.has(name)) return true;
    // Item 6 (H-38): a table part needs a table of C that is still open. The keys
    // of TABLE_PARENTS are the names of TABLE_PART and col.
    return TABLE_PARENTS.has(name) && this.nearest('table') <= this.cBase;
  }

  /**
   * M-11 items 1 and 5 to 9 (H-71, H-74, H-75): scan the noscript content C
   * followed by "</noscript>" alone, from the data state, with the rules of the
   * main pass, in place on the stack above the noscript element (the current
   * node). Returns an error code or null. The caller has pushed the noscript
   * element (M-26 item 2). After a valid C, the stack, D and the dirty mark of
   * the noscript element are those that the main pass had before C (M-11, note
   * of item 8). On an error, the scan ends and no state is used again.
   */
  scanNoscript(content) {
    this.noscriptStart = -1; // the end tag of C is handled here, not again in endTag()
    const base = this.names.length - 1;
    const saved = { b: this.b, tok: this.tok, depth: this.depth, state: this.state[base] };
    this.b = Buffer.concat([content, NOSCRIPT_END]);
    this.tok = new Tokenizer(this.b);
    this.cBase = base;
    const error = this.scanNoscriptTokens(content.length);
    this.b = saved.b;
    this.tok = saved.tok;
    this.cBase = -1;
    this.charsOnly = null;
    this.depth = saved.depth;
    this.state[base] = saved.state;
    return error;
  }

  scanNoscriptTokens(contentLength) {
    for (;;) {
      const t = this.tok.next();
      let error;
      switch (t.type) {
        case TokenType.START_TAG:
          t.name = internName(t.name);
          error = this.startTag(t);
          break;
        case TokenType.END_TAG:
          t.name = internName(t.name);
          if (t.name === 'noscript') {
            // Item 1: the first end tag noscript ends C exactly. Item 5: no
            // element of C is open, and no text-only element is open (M-07).
            const ended = t.start === contentLength && this.names.length - 1 === this.cBase;
            return ended && this.charsOnly === null ? null : STRUCTURE;
          }
          error = this.endTag(t);
          break;
        case TokenType.COMMENT:
          error = this.comment(t);
          break;
        case TokenType.DOCTYPE:
          error = this.doctype();
          break;
        default:
          return STRUCTURE; // item 1: the end tag noscript was not reached (raw text)
      }
      if (error !== null) return error;
    }
  }

  // M-11 items 5 and 7 for an end tag of C: it has the name of the current node,
  // which is an element of C. Exception (H-75): an end tag of the region root
  // closes the region.
  noscriptEnd(t) {
    const name = t.name;
    if (DOCUMENT_TAGS.has(name)) return STRUCTURE; // item 7
    const top = this.names.length - 1;
    if (this.region >= 0 && this.nearest(name) === this.region) return this.close(this.region, name);
    if (top <= this.cBase || this.names[top] !== name) return STRUCTURE; // item 5
    return this.close(top, name);
  }

  comment(t) {
    if (t.cdata) {
      // M-12. A CDATA section that holds M-12 is allowed also in the content
      // of M-07 rules 3 and 4 (decision H-21, edge case E5): it ends at the
      // same byte as a bogus comment, and its characters are text.
      return isCdataSafe(this.b, t) ? null : STRUCTURE;
    }
    if (this.charsOnly !== null) return STRUCTURE; // M-07 rules 3 and 4
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
      const error = this.scanNoscript(this.b.subarray(this.noscriptStart, t.start));
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
