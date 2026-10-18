// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// Study copy of the Node.js candidate (tools/rqp-markup/node/src/markup.mjs) for
// WP-17 S3 T-02 (H-42): two models for the noscript content C. Study code only;
// it is not a candidate and it is not in the differential runner.
//
// Models (option `noscript`):
//   shipped  markup.md M-11 items 1 to 6 as shipped (the candidate behavior).
//   main     model (a): items 1 to 6, and the main-pass rules (M-05 ... M-10,
//            M-12, M-13, M-26 D) also run on the tokens of C, on a copy of the
//            token stack at the noscript start tag; plus item 7 (no html, body
//            or head tag in C). The copy is dropped after the end tag noscript.
//   allow    model (b): items 1 to 6, and every start tag in C has a name in an
//            allowlist (option `allow`).
//
// The only changes against the candidate: the options, the model step after the
// M-11 check, a rule label for each rejection (`why`, for the corpus reasons) and
// a state comparison after the model (a) pass. With `noscript: 'shipped'` the
// result is the candidate result (test/equality.test.mjs).

import { createHash } from 'node:crypto';
import { isUtf8 } from 'node:buffer';
import { Tokenizer, TokenType, matchesLower } from '../../../node/src/tokenizer.mjs';
import { isJsonText } from '../../../node/src/json.mjs';
import {
  BLOCK_FAMILY,
  BLOCK_HTML,
  BREAKOUT,
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
} from '../../../node/src/names.mjs';

export const LIMITS = Object.freeze({
  maxDocumentBytes: 5 * 1024 * 1024,
  maxBlocks: 256,
  maxBlockBytes: 1024 * 1024,
  maxOpenElements: 256,
});

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

/** Names of the noscript models. */
export const MODELS = Object.freeze(['shipped', 'main', 'allow']);

/**
 * Model (b), list b0: the example list of the brief. Text, comments and end
 * tags are not in the list: text and comments are always allowed, and end tags
 * are matched by M-11 item 5.
 */
export const ALLOW_B0 = Object.freeze(new Set(['a', 'div', 'iframe', 'img', 'link', 'meta', 'p', 'span', 'style']));

/**
 * Model (b), list b1: b0 and the phrasing and media names that real noscript
 * fallbacks use (corpus inventory). Each name is an "any other start tag", a
 * void element or a raw text element in "in body", or a formatting element
 * (b, em, i, strong, small, u, font, s), or picture/source/video/audio.
 */
export const ALLOW_B1 = Object.freeze(new Set([
  ...ALLOW_B0, 'b', 'br', 'em', 'i', 'strong', 'small', 'u', 'font', 's', 'picture', 'source', 'video', 'audio', 'label',
  'center', 'h1', 'h2', 'h3', 'h4', 'h5', 'h6', 'ul', 'li', 'ol', 'section', 'article', 'nav', 'header', 'footer', 'hr', 'input',
]));

/** Model (a), item 7: tags in C that the tree builder never pushes or that change the insertion mode. */
export const NOSCRIPT_DOCUMENT_TAGS = Object.freeze(new Set(['html', 'body', 'head']));

const STRUCTURE = ErrorCode.STRUCTURE;
const JSON_TYPE = Buffer.from('application/json', 'latin1');
const MANIFEST_MARK = 'rivqen-manifest';
const NOSCRIPT_END = Buffer.from('</noscript>', 'latin1');
const AMPERSAND = 0x26;
const NON_ASCII = 0x80;

const DIRTY = 1;
const MARKED = 2;
const ROW_GROUP = 4;
const ROW = 8;
const COLUMN_GROUP = 16;

const NO_FAMILY = -1;
const IN_TEMPLATE = -2;

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

function partCount(state) {
  return ((state & ROW_GROUP) >> 2) + ((state & ROW) >> 3) + ((state & COLUMN_GROUP) >> 4);
}

export function invalid(code) {
  return { valid: false, error: code, blocks: [], template_revision: null, page_revision: null };
}

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

function isReservedType(b, span) {
  for (let i = span.start; i < span.end; i++) {
    if (b[i] === AMPERSAND || b[i] >= NON_ASCII) return true;
  }
  for (let i = span.start; i + MANIFEST_MARK.length <= span.end; i++) {
    if (matchesLower(b, i, MANIFEST_MARK)) return true;
  }
  return false;
}

function isJsonType(b, span) {
  return b.subarray(span.start, span.end).equals(JSON_TYPE);
}

function isCdataSafe(b, t) {
  return !t.closed || (b[t.end - 3] === 0x5d && b[t.end - 2] === 0x5d);
}

function increment(map, key) {
  map.set(key, (map.get(key) ?? 0) + 1);
}

function decrement(map, key) {
  const count = (map.get(key) ?? 0) - 1;
  if (count > 0) map.set(key, count);
  else map.delete(key);
}

/** M-11 item 3 without svg and math (variant "a-fx": foreign content in C, study only). */
const NOSCRIPT_FORBIDDEN_NO_FOREIGN = Object.freeze(new Set([...NOSCRIPT_FORBIDDEN].filter((n) => n !== 'svg' && n !== 'math')));

/**
 * M-11 items 1 to 6 as shipped. Returns { error, why } or null. `why` is the
 * item label (study only).
 *
 * With `foreign` (variant "a-fx" only), item 3 allows svg and math, and the stack
 * of item 5 follows M-05 in a foreign region of C: a start tag there is pushed
 * unless it has the self-closing flag, and it never changes the tokenizer state.
 * The other rules for the foreign region (M-07, M-12) come from the model (a) pass.
 */
export function checkNoscript(content, { foreign = false } = {}) {
  const b = Buffer.concat([content, NOSCRIPT_END]);
  const tok = new Tokenizer(b);
  const forbidden = foreign ? NOSCRIPT_FORBIDDEN_NO_FOREIGN : NOSCRIPT_FORBIDDEN;
  const open = [];
  let tables = 0;
  let region = -1; // index in `open` of the svg or math element of a foreign region of C
  for (;;) {
    const t = tok.next();
    if (t.type === TokenType.START_TAG) {
      const name = internName(t.name);
      if (name === 'script' && t.typeAttr !== null && isReservedType(b, t.typeAttr)) {
        return { error: ErrorCode.RESERVED, why: 'M-11.4' };
      }
      if (t.blockAttr !== null) return { error: STRUCTURE, why: 'M-11.2' };
      if (forbidden.has(name)) return { error: STRUCTURE, why: `M-11.3:${name}` };
      if (region >= 0) {
        if (!t.selfClosing) open.push(name);
        continue;
      }
      if (tables === 0 && TABLE_PARENTS.has(name)) return { error: STRUCTURE, why: 'M-11.6' };
      if (name === 'svg' || name === 'math') {
        if (!t.selfClosing) {
          open.push(name);
          region = open.length - 1;
        }
        continue;
      }
      if (!VOID.has(name)) {
        open.push(name);
        if (name === 'table') tables++;
      }
      const mode = TEXT_MODE.get(name);
      if (mode !== undefined) tok.switchTo(mode, name);
    } else if (t.type === TokenType.END_TAG) {
      if (t.name === 'noscript' && region < 0) {
        if (t.start !== content.length) return { error: STRUCTURE, why: 'M-11.1' };
        return open.length === 0 ? null : { error: STRUCTURE, why: 'M-11.5' };
      }
      if (open.at(-1) !== t.name) return { error: STRUCTURE, why: 'M-11.5' };
      if (open.pop() === 'table' && region < 0) tables--;
      if (open.length === region) region = -1;
    } else if (t.type === TokenType.EOF) {
      return { error: STRUCTURE, why: 'M-11.1' };
    }
  }
}

/**
 * Model (b): every start tag of C has a name in `allow`. Run after
 * checkNoscript, so C is balanced and ends in the data state. Returns the label
 * of the first start tag that is not allowed, or null.
 */
export function checkAllowlist(content, allow) {
  const b = Buffer.concat([content, NOSCRIPT_END]);
  const tok = new Tokenizer(b);
  for (;;) {
    const t = tok.next();
    if (t.type === TokenType.START_TAG) {
      const name = internName(t.name);
      if (!allow.has(name)) return `B:${name}`;
      const mode = TEXT_MODE.get(name);
      if (mode !== undefined) tok.switchTo(mode, name);
    } else if (t.type === TokenType.EOF || (t.type === TokenType.END_TAG && t.name === 'noscript')) {
      return null;
    }
  }
}

class Scanner {
  constructor(bytes, opts = {}) {
    this.b = bytes;
    this.tok = new Tokenizer(bytes);
    this.names = [];
    this.inHtml = [];
    this.family = [];
    this.listMark = [];
    this.state = [];
    this.depth = 0;
    this.byName = new Map();
    this.htmlOpen = new Map();
    this.contentOpen = new Map();
    this.region = -1;
    this.charsOnly = null;
    this.noscriptStart = -1;
    this.pending = null;
    this.block = null;
    this.blocks = [];
    this.ids = new Set();
    // Study fields.
    this.model = opts.noscript ?? 'shipped';
    this.allow = opts.allow ?? ALLOW_B0;
    this.foreignInContent = opts.foreignInContent === true && this.model === 'main'; // variant a-fx
    this.why = null; // rule label of the rejection
    this.sub = null; // the model (a) scanner of the last noscript content
    this.divergences = 0; // model (a): main state after the end tag noscript differs from the C pass
    this.noscripts = 0; // noscript elements in HTML context
  }

  fail(why, code = STRUCTURE) {
    this.why = why;
    return code;
  }

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

  inSelect() {
    return this.isOpen('select');
  }

  familyIndex() {
    const n = this.family.length;
    return n === 0 ? NO_FAMILY : this.family[n - 1];
  }

  currentFamily() {
    const f = this.familyIndex();
    return f < 0 ? null : this.names[f];
  }

  addDepth(n) {
    if (this.depth + n > LIMITS.maxOpenElements) return this.fail('M-26', ErrorCode.LIMIT);
    this.depth += n;
    return null;
  }

  push(name, inHtml) {
    const error = this.addDepth(1);
    if (error !== null) return error;
    const i = this.names.length;
    const below = this.familyIndex();
    this.names.push(name);
    this.inHtml.push(inHtml);
    this.state.push(0);
    this.family.push(inHtml && TABLE_FAMILY.has(name) ? i : name === 'template' ? IN_TEMPLATE : below);
    const mark = LISTS.has(name) || LIST_ITEMS.has(name) ? name : this.listMark.at(-1);
    this.listMark.push(mark);
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
    if (this.charsOnly !== null) return this.fail('M-07.3');
    if (t.name === 'frameset') return this.fail('M-10.1');
    if (t.name === 'script' && t.typeAttr !== null && isReservedType(this.b, t.typeAttr)) {
      return this.fail('M-13', ErrorCode.RESERVED);
    }
    if (this.inSelect() && !SELECT_START.has(t.name)) return this.fail('M-08');
    const foreign = this.region >= 0;
    if (t.blockAttr !== null) {
      const error = this.blockStart(t, foreign);
      if (error !== null) return error;
    }
    return foreign ? this.foreignStart(t) : this.htmlStart(t);
  }

  blockStart(t, foreign) {
    const b = this.b;
    if (this.block !== null) return this.fail('M-17', ErrorCode.NESTED);
    if (!isValidId(b, t.blockAttr)) return this.fail('M-15', ErrorCode.INVALID_ID);
    let format;
    if (foreign) return this.fail('M-16', ErrorCode.FORBIDDEN_ELEMENT);
    if (BLOCK_HTML.has(t.name)) format = 'html';
    else if (t.name === 'script' && t.typeAttr !== null && isJsonType(b, t.typeAttr)) format = 'json';
    else return this.fail('M-16', ErrorCode.FORBIDDEN_ELEMENT);
    if (t.selfClosing) return this.fail('M-18');
    if (this.isOpen('template') || this.inSelect()) return this.fail('M-19.1');
    const f = this.currentFamily();
    if (f !== null && !BLOCK_FAMILY.has(f)) return this.fail('M-19.2');
    const id = b.toString('latin1', t.blockAttr.start, t.blockAttr.end);
    if (this.ids.has(id)) return this.fail('M-21', ErrorCode.DUPLICATE);
    if (this.blocks.length >= LIMITS.maxBlocks) return this.fail('M-26', ErrorCode.LIMIT);
    this.ids.add(id);
    this.pending = { id, format };
    return null;
  }

  foreignStart(t) {
    const name = t.name;
    if (BREAKOUT.has(name)) return this.fail('M-07.1');
    const text = TEXT.has(name);
    if (text && (t.selfClosing || name === 'plaintext')) return this.fail('M-07.4');
    if (!t.selfClosing) {
      const error = this.push(name, false);
      if (error !== null) return error;
      if (text || INTEGRATION.has(name)) this.charsOnly = name;
    }
    return null;
  }

  htmlStart(t) {
    const name = t.name;
    if (this.block !== null) {
      const error = this.contentStart(t);
      if (error !== null) return error;
    }
    if (name === 'form' && this.isOpen('form')) return this.fail('M-10.2');
    if (name === 'template' && t.hasFor) return this.fail('M-10.3');
    const f = this.familyIndex();
    if (name === 'col' && f === IN_TEMPLATE) return this.fail('M-09.1');
    if (f >= 0) {
      const fName = this.names[f];
      const parents = TABLE_PARENTS.get(name);
      if (parents !== undefined && !parents.has(fName)) return this.fail('M-09');
      const clears = parents !== undefined || name === 'table';
      if (clears && TABLE_CONTEXT.has(fName) && f !== this.names.length - 1) return this.fail('M-09.2');
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
      this.tok.switchTo(mode, name);
      if (name === 'noscript') {
        this.noscriptStart = t.end;
        this.noscripts++;
      }
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

  contentStart(t) {
    const name = t.name;
    if (FORBIDDEN_CONTENT.has(name)) return this.fail('M-22.2');
    if (t.selfClosing && !VOID.has(name)) return this.fail('M-22.3');
    const blockName = this.names[this.block.index];
    if (PCLOSE.has(name) && (blockName === 'p' || blockName === 'span')) return this.fail('M-24.a');
    if (HEADINGS.has(name) && (HEADINGS.has(blockName) || this.headingInContent())) {
      return this.fail('M-24.b');
    }
    if (LIST_ITEMS.has(name)) {
      if (!this.listInContent()) return this.fail('M-24.c');
      if (LIST_ITEMS.has(this.listMark.at(-1))) return this.fail('M-24.j');
    }
    if ((name === 'button' || name === 'a' || name === 'nobr') && this.isOpen(name)) {
      return this.fail('M-24.def');
    }
    const tablePart = TABLE_PART.has(name) || name === 'col';
    if (tablePart && !this.isOpenInContent('table')) return this.fail('M-24.g');
    if (!tablePart && TABLE_CONTEXT.has(this.currentFamily())) return this.fail('M-24.k');
    if (name === 'rb' || name === 'rtc' || name === 'rp' || name === 'rt') {
      if (!this.isOpenInContent('ruby')) return this.fail('M-24.hi');
      const open = this.isOpen('rb') || this.isOpen('rp') || this.isOpen('rt');
      if (open || ((name === 'rb' || name === 'rtc') && this.isOpen('rtc'))) return this.fail('M-24.hi');
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

  /** M-11 as shipped, then the model step. Returns an error code or null. */
  noscriptContent(content) {
    const shipped = checkNoscript(content, { foreign: this.foreignInContent });
    if (shipped !== null) return this.fail(shipped.why, shipped.error);
    if (this.model === 'allow') {
      const why = checkAllowlist(content, this.allow);
      return why === null ? null : this.fail(why);
    }
    if (this.model === 'main') return this.mainPassOnContent(content);
    return null;
  }

  /**
   * Model (a): the tokens of C and the end tag noscript, through the main-pass
   * rules, on a copy of the token stack (the noscript element is the current
   * node). The copy keeps all state of M-05 to M-26. The block state is empty:
   * noscript is in FORBIDDEN_CONTENT, so C is never in block content.
   */
  mainPassOnContent(content) {
    const b = Buffer.concat([content, NOSCRIPT_END]);
    const sub = new Scanner(b, { noscript: 'shipped' });
    sub.names = [...this.names];
    sub.inHtml = [...this.inHtml];
    sub.family = [...this.family];
    sub.listMark = [...this.listMark];
    sub.state = [...this.state];
    sub.depth = this.depth;
    sub.byName = new Map([...this.byName].map(([k, v]) => [k, [...v]]));
    sub.htmlOpen = new Map(this.htmlOpen);
    sub.region = this.region;
    sub.ids = new Set(this.ids);
    this.sub = sub;
    for (;;) {
      const t = sub.tok.next();
      let error = null;
      if (t.type === TokenType.START_TAG || t.type === TokenType.END_TAG) {
        t.name = internName(t.name);
        if (NOSCRIPT_DOCUMENT_TAGS.has(t.name)) return this.fail('A:M-11.7');
        const last = t.type === TokenType.END_TAG && t.name === 'noscript' && t.start === content.length;
        error = t.type === TokenType.START_TAG ? sub.startTag(t) : sub.endTag(t);
        if (error !== null) return this.fail(`A:${sub.why ?? '?'}`, error);
        if (last) return null;
      } else if (t.type === TokenType.COMMENT) {
        error = sub.comment(t);
      } else if (t.type === TokenType.DOCTYPE) {
        error = sub.doctype();
      } else {
        return null; // EOF: not reached, checkNoscript item 1 holds
      }
      if (error !== null) return this.fail(`A:${sub.why ?? '?'}`, error);
    }
  }

  endTag(t) {
    const name = t.name;
    let ranSub = false;
    if (this.noscriptStart >= 0) {
      const content = this.b.subarray(this.noscriptStart, t.start);
      this.noscriptStart = -1;
      this.sub = null;
      const error = this.noscriptContent(content);
      if (error !== null) return error;
      ranSub = this.sub !== null;
    }
    if (this.charsOnly !== null) {
      if (name !== this.charsOnly) return this.fail('M-07.3');
      this.charsOnly = null;
    }
    if (this.inSelect() && !SELECT_END.has(name)) return this.fail('M-08');
    if (this.block !== null) return this.contentEnd(t);
    const idx = this.nearest(name);
    if (this.region >= 0) {
      if (idx < this.region) return this.fail('M-07.2');
    } else if (idx < 0) {
      return STRAY_TABLE_END.has(name) ? this.fail('M-06.3') : null;
    }
    const error = this.close(idx, name);
    if (error === null && ranSub && !this.sameState(this.sub)) this.divergences++;
    return error;
  }

  /** Model (a) self-check: the C pass ends with the token stack of the main pass. */
  sameState(other) {
    if (other.depth !== this.depth || other.names.length !== this.names.length) return false;
    for (let i = 0; i < this.names.length; i++) {
      if (other.names[i] !== this.names[i] || other.state[i] !== this.state[i]) return false;
    }
    return true;
  }

  close(idx, name) {
    const top = this.names.length - 1;
    const formatting = FORMATTING.has(name);
    const tableEnd = TABLE_FAMILY.has(name);
    let crossesSpecial = false;
    let marked = (this.state[idx] & MARKED) !== 0;
    for (let i = top; i > idx; i--) {
      const crossed = this.names[i];
      const html = this.inHtml[i];
      if (html && GUARDED.has(crossed) && !(tableEnd && TABLE_PART.has(crossed))) {
        return this.fail('M-06.1');
      }
      if (!html || SPECIAL.has(crossed)) crossesSpecial = true;
      if ((this.state[i] & MARKED) !== 0) marked = true;
    }
    if (formatting && (crossesSpecial || marked)) return this.fail(crossesSpecial ? 'M-06.2' : 'M-06.4');
    const state = this.state[idx];
    const exact = idx === top && (state & DIRTY) === 0 && name !== 'html' && name !== 'body';
    if (exact) this.depth -= 1 + partCount(state);
    while (this.names.length > idx) this.pop();
    const current = this.names.length - 1;
    if (current >= 0) {
      if (!exact) this.state[current] |= DIRTY;
      if (crossesSpecial || marked) this.state[current] |= MARKED;
    }
    return null;
  }

  contentEnd(t) {
    const top = this.names.length - 1;
    if (this.names[top] !== t.name) return this.fail('M-23');
    if (top === this.block.index) {
      const { id, format, start } = this.block;
      const end = t.start;
      if (end - start > LIMITS.maxBlockBytes) return this.fail('M-26', ErrorCode.LIMIT);
      if (format === 'json' && !isJsonText(this.b, start, end)) return this.fail('M-25', ErrorCode.BAD_JSON);
      this.blocks.push({ id, format, start, end });
      this.block = null;
    }
    return this.close(top, t.name);
  }

  comment(t) {
    if (t.cdata) {
      return isCdataSafe(this.b, t) ? null : this.fail('M-12');
    }
    if (this.charsOnly !== null) return this.fail('M-07.5');
    return null;
  }

  doctype() {
    if (this.charsOnly !== null) return this.fail('M-07.3');
    if (this.block !== null) return this.fail('M-22.1');
    if (this.inSelect()) return this.fail('M-08');
    return null;
  }

  eof(t) {
    if (this.noscriptStart >= 0) {
      const error = this.noscriptContent(this.b.subarray(this.noscriptStart, t.start));
      if (error !== null) return error;
    }
    if (this.block !== null) return this.fail('M-20');
    if (this.inSelect()) return this.fail('M-08');
    return null;
  }
}

function base64urlSha256(...parts) {
  const h = createHash('sha256');
  for (const p of parts) h.update(p);
  return h.digest('base64url');
}

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
 * Study analysis. Returns { result, why, divergences, noscripts }: `result` is
 * the CONTRACT.md result object; `why` is the rule label of the rejection
 * (null when valid; "A:" for model (a), "B:" for model (b)).
 *
 * @param {Uint8Array} input
 * @param {{ noscript?: 'shipped' | 'main' | 'allow', allow?: Set<string>, foreignInContent?: boolean }} [opts]
 *   foreignInContent: with model (a) only, svg and math are allowed in C (variant a-fx)
 */
export function analyzeStudy(input, opts = {}) {
  if (!(input instanceof Uint8Array)) throw new TypeError('analyzeStudy() needs a Uint8Array');
  if (opts.noscript !== undefined && !MODELS.includes(opts.noscript)) throw new RangeError(`unknown model ${opts.noscript}`);
  const bytes = Buffer.from(input.buffer, input.byteOffset, input.byteLength);
  if (bytes.length > LIMITS.maxDocumentBytes) return { result: invalid(ErrorCode.LIMIT), why: 'M-01', divergences: 0, noscripts: 0 };
  if (!isUtf8(bytes)) return { result: invalid(ErrorCode.ENCODING), why: 'M-02', divergences: 0, noscripts: 0 };
  const scanner = new Scanner(bytes, opts);
  const scan = scanner.run();
  const base = { divergences: scanner.divergences, noscripts: scanner.noscripts };
  if (scan.error !== undefined) return { result: invalid(scan.error), why: scanner.why ?? '?', ...base };
  return { result: buildResult(bytes, scan.blocks), why: null, ...base };
}
