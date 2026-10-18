// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// Study copy of the Node.js candidate (tools/rqp-markup/node/src/markup.mjs) for
// WP-17 S3 T-01 (H-40). It is not a candidate and it is not part of the
// differential runner. It adds three relaxations behind flags:
//
//   r1  a balanced svg or math subtree in the content of an html block
//   r2  svg in the noscript content C (M-11 item 3 without svg)
//   r3  no special-crossed mark (M-06 item 4) for a crossing by the end tag of
//       the region root (scope "root"), or by any end tag in a foreign region
//       (scope "region"); with r1 or r2 the same crossing is also allowed in
//       block content (M-23) and in C (M-11 item 5)
//
// With every flag off, the result is the result of the shipped candidate
// (test/equivalence.test.mjs checks this on the golden fixtures, the sweep
// cases and generated documents). The tokenizer, the name sets, the JSON check
// and the result object are imported from the shipped candidate unchanged.
//
// Time and memory are as in the shipped candidate: one pass over the tokens,
// every stack bounded by D <= 256 (M-26) or, for C, by the size of C.

import { isUtf8 } from 'node:buffer';
import { Tokenizer, TokenType } from '../../../node/src/tokenizer.mjs';
import { isJsonText } from '../../../node/src/json.mjs';
import { ErrorCode, LIMITS, buildResult, invalid } from '../../../node/src/markup.mjs';
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

export { ErrorCode, LIMITS };

const STRUCTURE = ErrorCode.STRUCTURE;
const JSON_TYPE = Buffer.from('application/json', 'latin1');
const MANIFEST_MARK = 'rivqen-manifest';
const NOSCRIPT_END = Buffer.from('</noscript>', 'latin1');
const AMPERSAND = 0x26;
const NON_ASCII = 0x80;

/**
 * r1: start tags that stay forbidden inside a foreign region of html block
 * content. An SVG script element runs; an SVG style element applies CSS to the
 * whole page (proposal, report section 5).
 */
export const FOREIGN_CONTENT_FORBIDDEN = Object.freeze(new Set(['script', 'style']));

/** r2: the names of M-11 item 3 without svg. */
const NOSCRIPT_FORBIDDEN_R2 = Object.freeze(new Set([...NOSCRIPT_FORBIDDEN].filter((n) => n !== 'svg')));

/** All flags off: the shipped rules. */
export const BASE = Object.freeze({ r1: false, r2: false, r3: false, r3Scope: 'root' });

/**
 * Flags from a combination name: "base", or relaxations joined by "+", for
 * example "r1+r3". "r3w" is r3 with the scope "region".
 * @param {string} spec
 */
export function flagsOf(spec) {
  const f = { ...BASE };
  if (spec === 'base') return Object.freeze(f);
  for (const part of spec.split('+')) {
    if (part === 'r1') f.r1 = true;
    else if (part === 'r2') f.r2 = true;
    else if (part === 'r3') f.r3 = true;
    else if (part === 'r3w') {
      f.r3 = true;
      f.r3Scope = 'region';
    } else throw new RangeError(`unknown relaxation: ${part}`);
  }
  return Object.freeze(f);
}

/** The combinations of the study (report section 3). */
export const COMBOS = Object.freeze(['base', 'r1', 'r2', 'r3', 'r3w', 'r1+r3', 'r2+r3', 'r1+r2', 'r1+r2+r3', 'r1+r2+r3w']);

// State bits of a token-stack entry.
const DIRTY = 1; // M-26 item 5
const MARKED = 2; // M-06 item 4: special-crossed mark
const ROW_GROUP = 4; // M-26 item 3: implied parts of F
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

function matchesLowerAt(b, p, lower) {
  for (let i = 0; i < lower.length; i++) {
    let c = b[p + i];
    if (c >= 0x41 && c <= 0x5a) c |= 0x20;
    if (c !== lower.charCodeAt(i)) return false;
  }
  return true;
}

function isReservedType(b, span) {
  for (let i = span.start; i < span.end; i++) {
    if (b[i] === AMPERSAND || b[i] >= NON_ASCII) return true;
  }
  for (let i = span.start; i + MANIFEST_MARK.length <= span.end; i++) {
    if (matchesLowerAt(b, i, MANIFEST_MARK)) return true;
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

const fail = (why, code = STRUCTURE) => ({ code, why });

/**
 * M-11 for the noscript content C, with the r2 and r3 changes when the flags
 * are on. With r2 off this is the check of the shipped candidate.
 *
 * @param {Buffer} content C
 * @param {typeof BASE} flags
 * @param {{ tags: [number, boolean, string][] } | null} sink receives [offset in C, foreign, name] for each start tag of C
 * @returns {{ code: string, why: string } | null}
 */
export function checkNoscript(content, flags, sink = null) {
  const b = Buffer.concat([content, NOSCRIPT_END]);
  const tok = new Tokenizer(b);
  const open = [];
  let tables = 0;
  let region = -1; // r2: index in `open` of the svg element that opened a foreign region of C
  let charsOnly = null; // r2: M-07 items 3 and 4 inside that region
  const forbidden = flags.r2 ? NOSCRIPT_FORBIDDEN_R2 : NOSCRIPT_FORBIDDEN;
  for (;;) {
    const t = tok.next();
    if (t.type === TokenType.START_TAG) {
      const name = internName(t.name);
      if (sink !== null) sink.tags.push([t.start, region >= 0, name]);
      if (charsOnly !== null) return fail('M-11.r2-M-07.3'); // r2 only: M-07 items 3, 4 in C
      if (name === 'script' && t.typeAttr !== null && isReservedType(b, t.typeAttr)) {
        return fail('M-11.4', ErrorCode.RESERVED);
      }
      if (t.blockAttr !== null) return fail('M-11.2');
      if (forbidden.has(name)) return fail(`M-11.3:${name}`);
      if (tables === 0 && TABLE_PARENTS.has(name)) return fail('M-11.6');
      if (region >= 0) {
        // r2: a start tag in the foreign region of C (M-05 item 2, M-07 items 1 and 4).
        if (BREAKOUT.has(name)) return fail('M-11.r2-M-07.1');
        const text = TEXT.has(name);
        if (text && t.selfClosing) return fail('M-11.r2-M-07.4');
        if (!t.selfClosing) {
          open.push(name);
          if (text || INTEGRATION.has(name)) charsOnly = name;
        }
        continue; // no state change in a foreign region (M-04)
      }
      if (name === 'svg') {
        // r2 only (else item 3 rejected it): a foreign region of C starts.
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
      if (t.name === 'noscript') {
        return t.start === content.length && open.length === 0 ? null : fail(open.length > 0 ? 'M-11.5' : 'M-11.1');
      }
      if (charsOnly !== null) {
        if (t.name !== charsOnly) return fail('M-11.r2-M-07.3');
        charsOnly = null;
      }
      const top = open.length - 1;
      if (open[top] === t.name) {
        if (open.pop() === 'table' && region < 0) tables--;
        if (top === region) region = -1;
        continue;
      }
      if (flags.r2 && flags.r3 && region >= 0) {
        // r3 in C: the end tag of the region root (or, scope "region", of any
        // element of the region) crosses foreign elements of the region.
        const idx = open.lastIndexOf(t.name);
        if (idx >= region && (flags.r3Scope === 'region' || idx === region)) {
          open.length = idx;
          if (idx === region) region = -1;
          continue;
        }
      }
      return fail(region >= 0 ? 'M-11.5-foreign' : 'M-11.5');
    } else if (t.type === TokenType.COMMENT) {
      if (region >= 0) {
        // r2: M-12 and M-07 item 5 in the foreign region of C, where the
        // scripting-disabled tree builder reads a real CDATA section.
        if (t.cdata) {
          if (!isCdataSafe(b, t)) return fail('M-11.r2-M-12');
        } else if (charsOnly !== null) return fail('M-11.r2-M-07.5');
      }
    } else if (t.type === TokenType.DOCTYPE) {
      if (charsOnly !== null) return fail('M-11.r2-M-07.3');
    } else if (t.type === TokenType.EOF) {
      return fail('M-11.1');
    }
  }
}

/** One pass over the tokens: the token stack (M-05) and rules M-06 to M-26. */
class Scanner {
  /**
   * @param {Buffer} bytes
   * @param {typeof BASE} flags
   * @param {object | null} trace see analyzeRelaxed
   */
  constructor(bytes, flags, trace) {
    this.b = bytes;
    this.flags = flags;
    this.trace = trace;
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
    this.noscriptTag = -1;
    this.pending = null;
    this.block = null;
    this.blocks = [];
    this.ids = new Set();
    this.why = null;
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
          return error === null ? { blocks: this.blocks } : { error, why: this.why };
      }
      if (error !== null) return { error, why: this.why };
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

  // r3: true when an end tag that matches entry idx is the crossing of the
  // relaxation (in a foreign region; the region root, or any entry of the
  // region for the scope "region").
  isIconEnd(idx) {
    if (!this.flags.r3 || this.region < 0 || idx < this.region) return false;
    return this.flags.r3Scope === 'region' || idx === this.region;
  }

  startTag(t) {
    if (this.charsOnly !== null) return this.fail('M-07.3');
    if (t.name === 'frameset') return this.fail('M-10.1');
    if (t.name === 'script' && t.typeAttr !== null && isReservedType(this.b, t.typeAttr)) {
      return this.fail('M-13', ErrorCode.RESERVED);
    }
    if (this.inSelect() && !SELECT_START.has(t.name)) return this.fail('M-08');
    const foreign = this.region >= 0;
    if (this.trace !== null && this.block !== null) this.trace.content.push([t.start, foreign, t.end, t.name]);
    if (t.blockAttr !== null) {
      const error = this.blockStart(t, foreign);
      if (error !== null) return error;
    }
    if (foreign && this.block !== null && FOREIGN_CONTENT_FORBIDDEN.has(t.name)) {
      // r1 only: without r1 a foreign region never starts in block content.
      return this.fail(`r1-forbidden:${t.name}`);
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
        this.noscriptTag = t.start;
      }
    }
    if (this.pending !== null) {
      const { id, format } = this.pending;
      this.pending = null;
      this.block = { id, format, start: t.end, index: this.names.length - 1 };
      if (this.trace !== null) this.trace.blockNames.push(name);
    }
    return null;
  }

  contentStart(t) {
    const name = t.name;
    const foreignRoot = name === 'svg' || name === 'math';
    // r1: svg and math (also with the self-closing flag, M-05 item 1) open a
    // foreign region in the content.
    const allowedRoot = this.flags.r1 && foreignRoot;
    if (FORBIDDEN_CONTENT.has(name) && !allowedRoot) return this.fail(`M-22.2:${name}`);
    if (t.selfClosing && !VOID.has(name) && !allowedRoot) return this.fail('M-22.3');
    const blockName = this.names[this.block.index];
    if (PCLOSE.has(name) && (blockName === 'p' || blockName === 'span')) return this.fail('M-24.a');
    if (HEADINGS.has(name) && (HEADINGS.has(blockName) || this.headingInContent())) return this.fail('M-24.b');
    if (LIST_ITEMS.has(name)) {
      if (!this.listInContent()) return this.fail('M-24.c');
      if (LIST_ITEMS.has(this.listMark.at(-1))) return this.fail('M-24.j');
    }
    if ((name === 'button' || name === 'a' || name === 'nobr') && this.isOpen(name)) return this.fail('M-24.def');
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

  endTag(t) {
    const name = t.name;
    if (this.noscriptStart >= 0) {
      const error = this.noscript(t.start);
      if (error !== null) return error;
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
    return this.close(idx, name);
  }

  noscript(end) {
    const sink = this.trace === null ? null : { tags: [] };
    const r = checkNoscript(this.b.subarray(this.noscriptStart, end), this.flags, sink);
    if (sink !== null) {
      const base = this.noscriptStart;
      this.trace.noscripts.push({ tag: this.noscriptTag, start: base, end, tags: sink.tags.map(([o, f, n]) => [o + base, f, n]) });
    }
    this.noscriptStart = -1;
    if (r === null) return null;
    return this.fail(r.why, r.code);
  }

  close(idx, name) {
    const top = this.names.length - 1;
    const formatting = FORMATTING.has(name);
    const tableEnd = TABLE_FAMILY.has(name);
    // r3: this crossing does not mark (all crossed elements are in the region).
    const iconEnd = this.isIconEnd(idx);
    let crossesSpecial = false;
    let marked = (this.state[idx] & MARKED) !== 0;
    for (let i = top; i > idx; i--) {
      const crossed = this.names[i];
      const html = this.inHtml[i];
      if (html && GUARDED.has(crossed) && !(tableEnd && TABLE_PART.has(crossed))) return this.fail('M-06.1');
      if (!html || SPECIAL.has(crossed)) crossesSpecial = true;
      if ((this.state[i] & MARKED) !== 0) marked = true;
    }
    if (formatting && crossesSpecial) return this.fail('M-06.2');
    if (formatting && marked) return this.fail('M-06.4');
    const state = this.state[idx];
    const exact = idx === top && (state & DIRTY) === 0 && name !== 'html' && name !== 'body';
    if (exact) this.depth -= 1 + partCount(state);
    while (this.names.length > idx) this.pop();
    const current = this.names.length - 1;
    if (current >= 0) {
      if (!exact) this.state[current] |= DIRTY;
      if ((crossesSpecial && !iconEnd) || marked) this.state[current] |= MARKED;
    }
    return null;
  }

  contentEnd(t) {
    const top = this.names.length - 1;
    if (this.names[top] !== t.name) {
      // r1 with r3: the crossing of r3 is allowed in the foreign region of the
      // content (the region starts in the content: a block never starts in a
      // foreign region, M-16).
      if (this.flags.r1 && this.region > this.block.index) {
        const idx = this.nearest(t.name);
        if (idx >= this.region && this.isIconEnd(idx)) return this.close(idx, t.name);
      }
      return this.fail(this.region >= 0 ? 'M-23-foreign' : 'M-23');
    }
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
    if (t.cdata) return isCdataSafe(this.b, t) ? null : this.fail('M-12');
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
      const error = this.noscript(t.start);
      if (error !== null) return error;
    }
    if (this.block !== null) return this.fail('M-20');
    if (this.inSelect()) return this.fail('M-08');
    return null;
  }
}

/**
 * Check a document against markup.md rqp/1 with the given relaxations.
 * Never throws for a Uint8Array input.
 *
 * @param {Uint8Array} input
 * @param {typeof BASE} [flags]
 * @param {{ trace?: boolean }} [options] trace: also return the start tags of
 *   block content (`content`: [start, foreign, end, name]) and of each noscript
 *   content C (`noscripts`: { tag, start, end, tags: [offset, foreign, name][] })
 *   and the name of each block element in order (`blockNames`),
 *   for the tree oracle of src/oracle.mjs and the attribute walk of src/attrs.mjs
 * @returns {object} the result object of CONTRACT.md section 3, plus `why`
 *   (rule of the first violation) for an invalid result and `trace` on request
 */
export function analyzeRelaxed(input, flags = BASE, { trace = false } = {}) {
  if (!(input instanceof Uint8Array)) throw new TypeError('analyzeRelaxed() needs a Uint8Array');
  const bytes = Buffer.from(input.buffer, input.byteOffset, input.byteLength);
  if (bytes.length > LIMITS.maxDocumentBytes) return { ...invalid(ErrorCode.LIMIT), why: 'M-01' };
  if (!isUtf8(bytes)) return { ...invalid(ErrorCode.ENCODING), why: 'M-02' };
  const t = trace ? { content: [], noscripts: [], blockNames: [] } : null;
  const scan = new Scanner(bytes, flags, t).run();
  if (scan.error !== undefined) return { ...invalid(scan.error), why: scan.why ?? null, ...(t ? { trace: t } : {}) };
  return { ...buildResult(bytes, scan.blocks), ...(t ? { trace: t } : {}) };
}
