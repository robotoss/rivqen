// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// Study copy of the Node.js candidate (tools/rqp-markup/node/src/markup.mjs at
// commit 270278c, the rules before S5) for WP-17 S5 T-02: `template` and
// `noscript` in the content of an html block. It is not a candidate and it is not
// part of the differential runner. Today M-22 item 2 rejects both names
// (FORBIDDEN_CONTENT, H-78). The options of the study are flags:
//
//   template: 'off'  shipped: a template start tag in the content is invalid (M-22)
//   template: 't1'   a template start tag in HTML context of the content is valid,
//                    except with an attribute named `shadowrootmode` (any value,
//                    ASCII case-insensitive, also a duplicate attribute). All other
//                    rules apply in the template content as in the rest of the
//                    content (M-09 condition 1, M-10 item 3, M-17, M-19, M-22 to M-24).
//   template: 't2'   t1, and a start tag with a name in TABLE_PART or `col` in the
//                    content needs an F (M-09) that is not none. The search for F
//                    stops at a template, so a table part in a template needs a table
//                    opened in that template ("in template" never switches to a
//                    table insertion mode).
//   template: 'tu'   control for the oracle, not a proposal: t1 without the
//                    shadowrootmode rule and without M-09 condition 1 in the content.
//
//   noscript: 'off'  shipped: a noscript start tag in the content is invalid (M-22)
//   noscript: 'n1'   a noscript start tag in HTML context of the content is valid.
//                    Its content C follows M-11 (items 1 to 9). The pass of M-11 item
//                    8 runs inside the block, so the content rules M-22 to M-24 apply
//                    to the start tags of C as to the other start tags of the content
//                    (the scripting-disabled tree builder sees the tokens of C in the
//                    content). M-23 in C is M-11 item 5.
//   noscript: 'n2'   n1, and in C of a noscript in the content a start tag in PCLOSE
//                    is invalid while an HTML `p` is on the token stack above the
//                    nearest template (M-24 item a for every block element, in C
//                    only). Then the scripting-disabled tree builder never closes
//                    the noscript element before its end tag.
//   noscript: 'nu'   control for the oracle, not a proposal: n1 without M-22 to M-24
//                    on the tokens of C.
//
// With both flags 'off' the result is the result of the pre-S5 candidate
// (test/equality.test.mjs: baseline/baseline-s4.json). The tokenizer, the JSON
// check and the result object are imported from the shipped candidate; the name
// sets from the frozen copy ../baseline/names-s4.mjs.
//
// Study fields: `why` (label of the first rule that rejected, for the rules of
// block content and of the options; null for the other rules) and `trace` (with
// the option trace): the start tags of block content, of each noscript content C
// and the templates, with offsets and parents, for the parse5 oracle.
//
// Time and memory are as in the shipped candidate: one pass over the tokens, and
// one more pass over each C. The trace has one entry per start tag (bounded by the
// input size, M-01). The attribute scan of a template start tag reads the bytes of
// that tag once.

import { isUtf8 } from 'node:buffer';
import { Tokenizer, TokenType, matchesLower } from '../../../node/src/tokenizer.mjs';
import { isJsonText } from '../../../node/src/json.mjs';
import { ErrorCode, LIMITS, buildResult, invalid } from '../../../node/src/markup.mjs';
import { attributesOf } from '../../../../rqp-protocol/src/markup.mjs';
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
} from '../baseline/names-s4.mjs';

export { ErrorCode, LIMITS };

export const TEMPLATE_MODES = Object.freeze(['off', 't1', 't2', 'tu']);
export const NOSCRIPT_MODES = Object.freeze(['off', 'n1', 'n2', 'nu']);

/** The attribute of the declarative shadow root (WHATWG HTML 13.2.6.4.4, "in head", template start tag). */
export const SHADOW_ROOT_MODE = 'shadowrootmode';

const STRUCTURE = ErrorCode.STRUCTURE;
const JSON_TYPES = [
  Buffer.from('application/json', 'latin1'),
  Buffer.from('application/ld+json', 'latin1'),
];
const MANIFEST_MARK = 'rivqen-manifest';
const NOSCRIPT_END = Buffer.from('</noscript>', 'latin1');
const AMPERSAND = 0x26;
const NON_ASCII = 0x80;
const CLOSURE_STEPS = 3; // M-09 implied closure: at most three closures (H-85a)

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

function isRiskyCode(v) {
  return v <= 0x20 || v >= NON_ASCII || isAsciiLetter(v) || isDigit(v)
    || v === 0x2f || v === 0x2b || v === 0x2d || v === 0x2e;
}

// M-13 items 1 to 3.
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

// M-12.
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

/** True when the start tag [start, end) has an attribute named `shadowrootmode` (duplicates included). */
export function hasShadowRootMode(bytes, start, end) {
  return attributesOf(bytes, start, end).some((a) => a.name === SHADOW_ROOT_MODE);
}

/** One pass over the tokens: the token stack (M-05) and rules M-06 to M-26. */
class Scanner {
  constructor(bytes, tmode, nmode, trace) {
    this.b = bytes;
    this.tmode = tmode;
    this.nmode = nmode;
    this.tok = new Tokenizer(bytes);
    this.names = [];
    this.inHtml = [];
    this.family = [];
    this.listMark = [];
    this.state = [];
    this.starts = []; // study: offset of the start tag of each entry (in the input)
    this.depth = 0;
    this.byName = new Map();
    this.htmlOpen = new Map();
    this.contentOpen = new Map();
    this.region = -1;
    this.charsOnly = null;
    this.noscriptStart = -1;
    this.cBase = -1;
    this.cOffset = 0; // study: input offset of byte 0 of the buffer that is scanned now
    this.pending = null;
    this.block = null;
    this.blocks = [];
    this.ids = new Set();
    this.why = null;
    this.trace = trace
      ? { blockNames: [], blockTags: [], content: [], noscripts: [], templates: [] }
      : null;
    this.noscript = null; // trace entry of the noscript content that is scanned now
    this.openTemplates = []; // trace entries of the templates on the stack (study)
  }

  /** Remember the first label of a study-relevant rejection and return the code. */
  fail(label, code = STRUCTURE) {
    if (this.why === null) this.why = this.cBase >= 0 ? `C:${label}` : label;
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
    if (this.depth + n > LIMITS.maxOpenElements) return ErrorCode.LIMIT;
    this.depth += n;
    return null;
  }

  push(name, inHtml, at) {
    const error = this.addDepth(1);
    if (error !== null) return error;
    const i = this.names.length;
    const below = this.familyIndex();
    this.names.push(name);
    this.inHtml.push(inHtml);
    this.state.push(0);
    this.starts.push(at);
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
    this.starts.pop();
    const list = this.byName.get(name);
    list.pop();
    if (list.length === 0) this.byName.delete(name);
    if (inHtml) decrement(this.htmlOpen, name);
    if (this.block !== null && i > this.block.index) decrement(this.contentOpen, name);
    if (i === this.region) this.region = -1;
    if (this.trace !== null && name === 'template' && inHtml && this.cBase < 0) {
      const tpl = this.openTemplates.pop();
      if (tpl !== undefined && tpl.index === i) tpl.popped = true;
    }
  }

  nearest(name) {
    const list = this.byName.get(name);
    return list === undefined ? -1 : list[list.length - 1];
  }

  /** Study trace of a start tag in the content or in C: before the push, the current node is the parent. */
  traceStart(t, foreign) {
    if (this.trace === null) return;
    const at = this.cOffset + t.start;
    const top = this.names.length - 1;
    const parent = top >= 0 ? this.starts[top] : -1;
    const inTemplate = this.openTemplates.length > 0;
    const pOpen = this.pAboveTemplate();
    if (this.cBase >= 0) {
      if (this.noscript !== null) this.noscript.tags.push([at, foreign, t.name, parent, pOpen, this.cOffset + t.end]);
    } else if (this.block !== null) {
      this.trace.content.push([at, foreign, this.cOffset + t.end, t.name, parent, inTemplate, pOpen]);
    }
  }

  /**
   * Study trace: an HTML `p` is on the token stack above the nearest template (or
   * anywhere when no template is open). A start tag in PCLOSE can close it in the tree
   * builder; a template is a scope boundary, so a `p` below it cannot close.
   */
  pAboveTemplate(from = this.names.length - 1) {
    for (let i = from; i >= 0; i--) {
      if (!this.inHtml[i]) continue;
      if (this.names[i] === 'template') return false;
      if (this.names[i] === 'p') return true;
    }
    return false;
  }

  startTag(t) {
    if (this.charsOnly !== null) return STRUCTURE;
    if (t.name === 'frameset') return STRUCTURE;
    if (t.name === 'script' && t.typeAttr !== null && isReservedType(this.b, t.typeAttr)) {
      return ErrorCode.RESERVED;
    }
    if (this.cBase >= 0 && this.noscriptRejects(t)) return this.fail('M-11-content');
    if (this.inSelect() && !SELECT_START.has(t.name)) return STRUCTURE;
    const foreign = this.region >= 0;
    if (t.blockAttr !== null) {
      const error = this.blockStart(t, foreign);
      if (error !== null) return error;
    }
    if (this.pending === null) this.traceStart(t, foreign);
    return foreign ? this.foreignStart(t) : this.htmlStart(t);
  }

  blockStart(t, foreign) {
    const b = this.b;
    if (this.block !== null) return ErrorCode.NESTED;
    if (!isValidId(b, t.blockAttr)) return ErrorCode.INVALID_ID;
    let format;
    if (foreign) return ErrorCode.FORBIDDEN_ELEMENT;
    if (BLOCK_HTML.has(t.name)) format = 'html';
    else if (t.name === 'script' && t.typeAttr !== null && isJsonType(b, t.typeAttr)) format = 'json';
    else return ErrorCode.FORBIDDEN_ELEMENT;
    if (t.selfClosing) return STRUCTURE;
    if (this.isOpen('template') || this.inSelect()) return STRUCTURE;
    const f = this.currentFamily();
    if (f !== null && !BLOCK_FAMILY.has(f)) return STRUCTURE;
    const id = b.toString('latin1', t.blockAttr.start, t.blockAttr.end);
    if (this.ids.has(id)) return ErrorCode.DUPLICATE;
    if (this.blocks.length >= LIMITS.maxBlocks) return ErrorCode.LIMIT;
    this.ids.add(id);
    this.pending = { id, format, tag: this.cOffset + t.start };
    return null;
  }

  foreignStart(t) {
    const name = t.name;
    if (BREAKOUT.has(name)) return STRUCTURE;
    const text = TEXT.has(name);
    if (text && (t.selfClosing || name === 'plaintext')) return STRUCTURE;
    if (!t.selfClosing) {
      const error = this.push(name, false, this.cOffset + t.start);
      if (error !== null) return error;
      if (text || INTEGRATION.has(name)) this.charsOnly = name;
    }
    return null;
  }

  htmlStart(t) {
    const name = t.name;
    // n1 (and the shipped rule): M-22 to M-24 apply to the tags of C in the content.
    // nu (control): not on the tags of C.
    if (this.block !== null && !(this.cBase >= 0 && this.nmode === 'nu')) {
      const error = this.contentStart(t);
      if (error !== null) return error;
    }
    if (name === 'form' && this.isOpen('form')) return STRUCTURE;
    if (name === 'template' && t.hasFor) return STRUCTURE;
    if (this.block === null && this.cBase < 0 && (TABLE_PART.has(name) || name === 'col')) {
      const error = this.impliedClosure(name);
      if (error !== null) return error;
    }
    const f = this.familyIndex();
    // M-09 condition 1 (H-35). Control tu: not in the content of a block.
    if (name === 'col' && f === IN_TEMPLATE && !(this.tmode === 'tu' && this.block !== null)) {
      return this.block !== null ? this.fail('M-09.1') : STRUCTURE;
    }
    if (f >= 0) {
      const fName = this.names[f];
      const parents = TABLE_PARENTS.get(name);
      if (parents !== undefined && !parents.has(fName)) return this.block !== null ? this.fail('M-09') : STRUCTURE;
      const clears = parents !== undefined || name === 'table';
      if (clears && TABLE_CONTEXT.has(fName) && f !== this.names.length - 1) return this.block !== null ? this.fail('M-09.2') : STRUCTURE;
      const parts = impliedParts(name, fName) & ~this.state[f];
      const error = this.addDepth(partCount(parts));
      if (error !== null) return error;
      this.state[f] |= parts;
    }
    const foreignRoot = name === 'svg' || name === 'math';
    if (!(foreignRoot && t.selfClosing) && !VOID.has(name)) {
      const error = this.push(name, true, this.cOffset + t.start);
      if (error !== null) return error;
      if (this.trace !== null && name === 'template' && this.cBase < 0) {
        const tpl = { tag: this.cOffset + t.start, end: -1, inBlock: this.block !== null, index: this.names.length - 1, popped: false };
        this.trace.templates.push(tpl);
        this.openTemplates.push(tpl);
      }
    }
    const mode = TEXT_MODE.get(name);
    if (mode !== undefined) {
      this.tok.switchTo(mode, name);
      if (name === 'noscript') {
        this.noscriptStart = t.end;
        if (this.trace !== null) {
          this.trace.noscripts.push({
            tag: this.cOffset + t.start,
            start: this.cOffset + t.end,
            end: -1,
            inBlock: this.block !== null,
            pAround: this.pAboveTemplate(this.names.length - 2),
            tags: [],
          });
        }
      }
    }
    if (this.pending !== null) {
      const { id, format, tag } = this.pending;
      this.pending = null;
      this.block = { id, format, start: t.end, index: this.names.length - 1, tag };
    }
    return null;
  }

  impliedClosure(name) {
    const parents = TABLE_PARENTS.get(name);
    for (let step = 0; step < CLOSURE_STEPS; step++) {
      const f = this.familyIndex();
      if (f < 0) return null;
      const fName = this.names[f];
      if (parents.has(fName)) return null;
      if (TABLE_CONTEXT.has(fName) && f !== this.names.length - 1) return STRUCTURE;
      const error = this.close(f, fName);
      if (error !== null) return error;
    }
    return null;
  }

  // M-22 and M-24 for a start tag in the content of an html block, with the S5 options.
  contentStart(t) {
    const name = t.name;
    if (FORBIDDEN_CONTENT.has(name)) {
      if (name === 'template' && this.tmode !== 'off') {
        if (this.tmode !== 'tu' && hasShadowRootMode(this.b, t.start, t.end)) return this.fail('S5-shadowrootmode');
      } else if (name === 'noscript' && this.nmode !== 'off') {
        // allowed: C is checked at its end (M-11)
      } else {
        return this.fail(name === 'template' || name === 'noscript' ? `M-22.2-${name}` : 'M-22.2');
      }
    }
    if (t.selfClosing && !VOID.has(name) && name !== 'svg' && name !== 'math') return this.fail('M-22.3');
    const blockName = this.names[this.block.index];
    if (PCLOSE.has(name) && (blockName === 'p' || blockName === 'span')) return this.fail('M-24a');
    // n2: in C, a start tag in PCLOSE must not close a p around the noscript element.
    if (this.nmode === 'n2' && this.cBase >= 0 && PCLOSE.has(name) && this.pAboveTemplate(this.cBase - 1)) {
      return this.fail('S5-noscript-pclose');
    }
    if (HEADINGS.has(name) && (HEADINGS.has(blockName) || this.headingInContent())) {
      return this.fail('M-24b');
    }
    if (LIST_ITEMS.has(name)) {
      if (!this.listInContent()) return this.fail('M-24c');
      if (LIST_ITEMS.has(this.listMark.at(-1))) return this.fail('M-24j');
    }
    if ((name === 'button' || name === 'a' || name === 'nobr') && this.isOpen(name)) {
      return this.fail('M-24def');
    }
    const tablePart = TABLE_PART.has(name) || name === 'col';
    // t2: a table part in a template needs a table opened in the template.
    if (tablePart && this.tmode === 't2' && this.familyIndex() === IN_TEMPLATE) return this.fail('S5-template-table-part');
    if (tablePart && !this.isOpenInContent('table')) return this.fail(this.familyIndex() === IN_TEMPLATE ? 'M-24g-template' : 'M-24g');
    if (!tablePart && TABLE_CONTEXT.has(this.currentFamily())) return this.fail('M-24k');
    if (name === 'rb' || name === 'rtc' || name === 'rp' || name === 'rt') {
      if (!this.isOpenInContent('ruby')) return this.fail('M-24hi');
      const open = this.isOpen('rb') || this.isOpen('rp') || this.isOpen('rt');
      if (open || ((name === 'rb' || name === 'rtc') && this.isOpen('rtc'))) return this.fail('M-24hi');
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
      const error = this.scanNoscript(this.b.subarray(this.noscriptStart, t.start), this.noscriptStart);
      if (error !== null) return error;
    }
    if (this.charsOnly !== null) {
      if (name !== this.charsOnly) return STRUCTURE;
      this.charsOnly = null;
    }
    if (this.inSelect() && !SELECT_END.has(name)) return STRUCTURE;
    if (this.cBase >= 0) return this.noscriptEnd(t);
    if (this.trace !== null && name === 'template' && this.openTemplates.length > 0) {
      // The end tag of the template that is the current node (an exact pop) ends it.
      const tpl = this.openTemplates.at(-1);
      if (tpl.index === this.names.length - 1 && this.names[tpl.index] === 'template') tpl.end = this.cOffset + t.start;
    }
    if (this.block !== null) return this.contentEnd(t);
    const idx = this.nearest(name);
    if (this.region >= 0) {
      if (idx < this.region) return STRUCTURE;
    } else if (idx < 0) {
      return STRAY_TABLE_END.has(name) ? STRUCTURE : null;
    }
    return this.close(idx, name);
  }

  close(idx, name) {
    const top = this.names.length - 1;
    const formatting = FORMATTING.has(name);
    const tableEnd = TABLE_FAMILY.has(name);
    let crossesSpecial = false;
    let setsMark = false;
    const rootEnd = idx === this.region;
    let marked = (this.state[idx] & MARKED) !== 0;
    for (let i = top; i > idx; i--) {
      const crossed = this.names[i];
      const html = this.inHtml[i];
      if (html && GUARDED.has(crossed) && !(tableEnd && TABLE_PART.has(crossed))) {
        return STRUCTURE;
      }
      if (!html || SPECIAL.has(crossed)) crossesSpecial = true;
      if (html ? SPECIAL.has(crossed) : !rootEnd) setsMark = true;
      if ((this.state[i] & MARKED) !== 0) marked = true;
    }
    if (formatting && (crossesSpecial || marked)) return STRUCTURE;
    const state = this.state[idx];
    const exact = idx === top && (state & DIRTY) === 0 && name !== 'html' && name !== 'body';
    if (exact) this.depth -= 1 + partCount(state);
    while (this.names.length > idx) this.pop();
    const current = this.names.length - 1;
    if (current >= 0) {
      if (!exact) this.state[current] |= DIRTY;
      if (setsMark || marked) this.state[current] |= MARKED;
    }
    return null;
  }

  contentEnd(t) {
    const top = this.names.length - 1;
    if (this.region >= 0 && this.nearest(t.name) === this.region) return this.close(this.region, t.name);
    if (this.names[top] !== t.name) return this.fail('M-23');
    if (top === this.block.index) {
      const { id, format, start, tag } = this.block;
      const end = t.start;
      if (end - start > LIMITS.maxBlockBytes) return ErrorCode.LIMIT;
      if (format === 'json' && !isJsonText(this.b, start, end)) return ErrorCode.BAD_JSON;
      this.blocks.push({ id, format, start, end });
      if (this.trace !== null) {
        this.trace.blockNames.push(this.names[top]);
        this.trace.blockTags.push(tag);
      }
      this.block = null;
    }
    return this.close(top, t.name);
  }

  noscriptRejects(t) {
    const name = t.name;
    if (t.blockAttr !== null || NOSCRIPT_FORBIDDEN.has(name) || DOCUMENT_TAGS.has(name)) return true;
    return TABLE_PARENTS.has(name) && this.nearest('table') <= this.cBase;
  }

  /** M-11 items 1 and 5 to 9 on C, in place (as the shipped candidate). `at` is the input offset of C. */
  scanNoscript(content, at) {
    this.noscriptStart = -1;
    const base = this.names.length - 1;
    const saved = { b: this.b, tok: this.tok, depth: this.depth, state: this.state[base], cOffset: this.cOffset };
    this.b = Buffer.concat([content, NOSCRIPT_END]);
    this.tok = new Tokenizer(this.b);
    this.cBase = base;
    this.cOffset = saved.cOffset + at;
    if (this.trace !== null) {
      this.noscript = this.trace.noscripts.at(-1) ?? null;
      if (this.noscript !== null) this.noscript.end = this.cOffset + content.length;
    }
    const error = this.scanNoscriptTokens(content.length);
    this.b = saved.b;
    this.tok = saved.tok;
    this.cBase = -1;
    this.cOffset = saved.cOffset;
    this.charsOnly = null;
    this.depth = saved.depth;
    this.state[base] = saved.state;
    this.noscript = null;
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
            const ended = t.start === contentLength && this.names.length - 1 === this.cBase;
            return ended && this.charsOnly === null ? null : this.fail('M-11-balance');
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
          return this.fail('M-11-end');
      }
      if (error !== null) return error;
    }
  }

  noscriptEnd(t) {
    const name = t.name;
    if (DOCUMENT_TAGS.has(name)) return this.fail('M-11-content');
    const top = this.names.length - 1;
    if (this.region >= 0 && this.nearest(name) === this.region) return this.close(this.region, name);
    if (top <= this.cBase || this.names[top] !== name) return this.fail('M-11-balance');
    return this.close(top, name);
  }

  comment(t) {
    if (t.cdata) {
      return isCdataSafe(this.b, t) ? null : STRUCTURE;
    }
    if (this.charsOnly !== null) return STRUCTURE;
    return null;
  }

  doctype() {
    if (this.charsOnly !== null) return STRUCTURE;
    if (this.block !== null) return this.fail('M-22.1');
    if (this.inSelect()) return STRUCTURE;
    return null;
  }

  eof(t) {
    if (this.noscriptStart >= 0) {
      const error = this.scanNoscript(this.b.subarray(this.noscriptStart, t.start), this.noscriptStart);
      if (error !== null) return error;
    }
    if (this.block !== null) return this.fail('M-20');
    if (this.inSelect()) return STRUCTURE;
    return null;
  }
}

/**
 * The study rule: the result object of CONTRACT.md section 3 (as the shipped
 * analyze()) and the study fields `why`, `trace` and the option names. Never throws
 * for a Uint8Array input and known flags.
 *
 * @param {Uint8Array} input the document bytes
 * @param {{ template?: string, noscript?: string, trace?: boolean }} [options]
 */
export function analyzeS5(input, { template = 'off', noscript = 'off', trace = false } = {}) {
  if (!(input instanceof Uint8Array)) throw new TypeError('analyzeS5() needs a Uint8Array');
  if (!TEMPLATE_MODES.includes(template)) throw new RangeError(`unknown template mode ${template}`);
  if (!NOSCRIPT_MODES.includes(noscript)) throw new RangeError(`unknown noscript mode ${noscript}`);
  const bytes = Buffer.from(input.buffer, input.byteOffset, input.byteLength);
  const none = { template, noscript, why: null, trace: null };
  if (bytes.length > LIMITS.maxDocumentBytes) return { ...invalid(ErrorCode.LIMIT), ...none };
  if (!isUtf8(bytes)) return { ...invalid(ErrorCode.ENCODING), ...none };
  const scanner = new Scanner(bytes, template, noscript, trace);
  const scan = scanner.run();
  const extra = { template, noscript, trace: scanner.trace };
  if (scan.error !== undefined) return { ...invalid(scan.error), why: scanner.why, ...extra };
  return { ...buildResult(bytes, scan.blocks), why: null, ...extra };
}

/** The fields of CONTRACT.md section 3 of a study result. */
export function contractFields(r) {
  const { valid, error, blocks, template_revision: t, page_revision: p } = r;
  return { valid, error, blocks, template_revision: t, page_revision: p };
}
