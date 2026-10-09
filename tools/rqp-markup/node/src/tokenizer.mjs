// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// Byte-level tokenizer for the part of the WHATWG HTML tokenizer (HTML Living
// Standard section 13.2.5) that docs/engineering/protocol/markup.md M-03 needs.
//
// Why bytes: the input is valid UTF-8 (M-02, checked before). Every state change
// of the tokenizer depends only on ASCII characters, and a UTF-8 multi-byte
// sequence never contains an ASCII byte. So a scan over bytes finds the same
// token boundaries as a scan over code points, and all offsets are UTF-8 byte
// offsets without a conversion.
//
// Differences from a full tokenizer, all without an effect on token boundaries:
// - Character tokens are not emitted. No markup.md rule depends on them.
// - Character references are not decoded. They never consume '<', '>', a quote
//   or white space, so they cannot move a boundary (markup.md section 3.1).
// - Input preprocessing (CR LF -> LF) is not done. CR is treated as white space,
//   which is what the preprocessed LF is in every state where it matters.
// - Only the first data-rq-block and the first type attribute of a tag are kept,
//   as raw source spans (DL-010). Other attributes are skipped.
// - '<![CDATA[' is always tokenized as a bogus comment (M-04 allows this; M-12
//   is checked on the returned token).
//
// Every call of next() consumes at least one byte or returns EOF, and every
// state loop advances the position. Time is linear in the input length.

export const TokenType = Object.freeze({
  START_TAG: 1,
  END_TAG: 2,
  COMMENT: 3,
  DOCTYPE: 4,
  EOF: 5,
});

/** Text modes that the consumer sets after a start tag (M-04). */
export const TextMode = Object.freeze({
  DATA: 0,
  RCDATA: 1,
  RAWTEXT: 2,
  SCRIPT_DATA: 3,
  PLAINTEXT: 4,
});

const TAB = 0x09;
const LF = 0x0a;
const FF = 0x0c;
const CR = 0x0d;
const SPACE = 0x20;
const BANG = 0x21;
const DQ = 0x22;
const SQ = 0x27;
const DASH = 0x2d;
const SLASH = 0x2f;
const LT = 0x3c;
const EQ = 0x3d;
const GT = 0x3e;
const QMARK = 0x3f;

/** WHATWG white space in the tokenizer (after preprocessing, CR is LF). */
export function isWs(c) {
  return c === SPACE || c === LF || c === TAB || c === FF || c === CR;
}

/** ASCII letter. */
export function isAlpha(c) {
  const l = c | 0x20;
  return l >= 0x61 && l <= 0x7a;
}

/** ASCII case-insensitive match of the bytes at p with a lower-case ASCII string. */
export function matchesLower(b, p, lower) {
  if (p + lower.length > b.length) return false;
  for (let i = 0; i < lower.length; i++) {
    let c = b[p + i];
    if (c >= 0x41 && c <= 0x5a) c |= 0x20;
    if (c !== lower.charCodeAt(i)) return false;
  }
  return true;
}

/** True when bytes [start, end) equal the lower-case ASCII string, ignoring ASCII case. */
function spanEqualsLower(b, start, end, lower) {
  return end - start === lower.length && matchesLower(b, start, lower);
}

const UPPER_OR_NUL = /[A-Z\0]/;

/**
 * Tag name of bytes [start, end) as the tokenizer gives it: ASCII upper case
 * becomes lower case, U+0000 becomes U+FFFD. Other characters stay.
 */
export function tagName(b, start, end) {
  const raw = b.toString('utf8', start, end);
  if (!UPPER_OR_NUL.test(raw)) return raw;
  return raw.replace(/[A-Z]+/g, (m) => m.toLowerCase()).replaceAll('\0', '�');
}

const BLOCK_ATTR = 'data-rq-block';
const TYPE_ATTR = 'type';
const SCRIPT = 'script';
const DOCTYPE = 'doctype';
const CDATA = '[CDATA[';

function eofToken(at) {
  return { type: TokenType.EOF, name: null, start: at, end: at };
}

function newTag(type, name, start) {
  return {
    type,
    name,
    start,
    end: start,
    selfClosing: false,
    // Raw value spans { start, end } of the first attribute with this name, or null.
    blockAttr: null,
    typeAttr: null,
  };
}

function newComment(start, end, closed, cdata) {
  return { type: TokenType.COMMENT, name: null, start, end, closed, cdata };
}

// Attribute states.
const BEFORE_ATTR_NAME = 0;
const ATTR_NAME = 1;
const AFTER_ATTR_NAME = 2;
const BEFORE_ATTR_VALUE = 3;
const ATTR_VALUE_UNQUOTED = 4;
const AFTER_ATTR_VALUE_QUOTED = 5;
const SELF_CLOSING_START_TAG = 6;

// Attribute slots.
const SLOT_NONE = 0;
const SLOT_BLOCK = 1;
const SLOT_TYPE = 2;

// Comment states.
const COMMENT_START = 0;
const COMMENT_START_DASH = 1;
const COMMENT = 2;
const COMMENT_LT = 3;
const COMMENT_LT_BANG = 4;
const COMMENT_LT_BANG_DASH = 5;
const COMMENT_LT_BANG_DASH_DASH = 6;
const COMMENT_END_DASH = 7;
const COMMENT_END = 8;
const COMMENT_END_BANG = 9;

// Script data states.
const SD = 0;
const SD_LT = 1;
const SD_END_TAG_OPEN = 2;
const SD_ESCAPE_START = 3;
const SD_ESCAPE_START_DASH = 4;
const ESC = 5;
const ESC_DASH = 6;
const ESC_DASH_DASH = 7;
const ESC_LT = 8;
const ESC_END_TAG_OPEN = 9;
const DESC_START = 10;
const DESC = 11;
const DESC_DASH = 12;
const DESC_DASH_DASH = 13;
const DESC_LT = 14;
const DESC_END = 15;

/** Pull tokenizer over a byte buffer. */
export class Tokenizer {
  #b;
  #pos;
  #mode = TextMode.DATA;
  #rawName = '';

  /**
   * @param {Buffer} bytes valid UTF-8
   * @param {number} start first byte to tokenize (3 after a BOM, M-02)
   */
  constructor(bytes, start = 0) {
    this.#b = bytes;
    this.#pos = start;
  }

  /**
   * Change the text mode after a start tag (M-04). `name` is the lower-case tag
   * name of that start tag; RCDATA, RAWTEXT and script data end at its
   * appropriate end tag.
   */
  switchTo(mode, name) {
    this.#mode = mode;
    this.#rawName = name;
  }

  /** Return the next non-character token. */
  next() {
    switch (this.#mode) {
      case TextMode.RCDATA:
      case TextMode.RAWTEXT:
        return this.#rawText();
      case TextMode.SCRIPT_DATA:
        return this.#scriptData();
      case TextMode.PLAINTEXT:
        return this.#eof();
      default:
        return this.#data();
    }
  }

  #eof() {
    this.#pos = this.#b.length;
    return eofToken(this.#b.length);
  }

  // Data state and tag open state.
  #data() {
    const b = this.#b;
    const end = b.length;
    let p = this.#pos;
    for (;;) {
      const lt = b.indexOf(LT, p);
      if (lt < 0) return this.#eof();
      const q = lt + 1;
      if (q >= end) return this.#eof();
      const c = b[q];
      if (c === BANG) return this.#markupDeclaration(lt);
      if (c === SLASH) {
        // End tag open state.
        const r = q + 1;
        if (r >= end) return this.#eof();
        const d = b[r];
        if (isAlpha(d)) return this.#tag(TokenType.END_TAG, lt, r);
        if (d === GT) {
          // "</>": missing end tag name, nothing is emitted.
          p = r + 1;
          continue;
        }
        return this.#bogusComment(lt, r, false);
      }
      if (isAlpha(c)) return this.#tag(TokenType.START_TAG, lt, q);
      if (c === QMARK) return this.#bogusComment(lt, q, false);
      // '<' is a character; reconsume c in the data state.
      p = q;
    }
  }

  // Markup declaration open state.
  #markupDeclaration(lt) {
    const b = this.#b;
    const p = lt + 2;
    if (b[p] === DASH && b[p + 1] === DASH) return this.#comment(lt, p + 2);
    if (matchesLower(b, p, DOCTYPE)) {
      // Every DOCTYPE state ends the token at the first '>' (or EOF).
      const gt = b.indexOf(GT, p + DOCTYPE.length);
      const tokenEnd = gt < 0 ? b.length : gt + 1;
      this.#pos = tokenEnd;
      return { type: TokenType.DOCTYPE, name: null, start: lt, end: tokenEnd };
    }
    if (b.toString('latin1', p, p + CDATA.length) === CDATA) {
      return this.#bogusComment(lt, p + CDATA.length, true);
    }
    return this.#bogusComment(lt, p, false);
  }

  // Bogus comment state: ends at the first '>' at or after `from`.
  #bogusComment(lt, from, cdata) {
    const gt = this.#b.indexOf(GT, from);
    if (gt < 0) {
      this.#pos = this.#b.length;
      return newComment(lt, this.#b.length, false, cdata);
    }
    this.#pos = gt + 1;
    return newComment(lt, gt + 1, true, cdata);
  }

  // Comment start state and the comment states after it.
  #comment(lt, from) {
    const b = this.#b;
    const end = b.length;
    let p = from;
    let state = COMMENT_START;
    while (p < end) {
      const c = b[p];
      switch (state) {
        case COMMENT_START:
          if (c === DASH) {
            p++;
            state = COMMENT_START_DASH;
          } else if (c === GT) {
            return this.#closeComment(lt, p);
          } else {
            state = COMMENT;
          }
          break;
        case COMMENT_START_DASH:
          if (c === DASH) {
            p++;
            state = COMMENT_END;
          } else if (c === GT) {
            return this.#closeComment(lt, p);
          } else {
            state = COMMENT;
          }
          break;
        case COMMENT:
          if (c === LT) state = COMMENT_LT;
          else if (c === DASH) state = COMMENT_END_DASH;
          p++;
          break;
        case COMMENT_LT:
          if (c === BANG) {
            p++;
            state = COMMENT_LT_BANG;
          } else if (c === LT) {
            p++;
          } else {
            state = COMMENT;
          }
          break;
        case COMMENT_LT_BANG:
          if (c === DASH) {
            p++;
            state = COMMENT_LT_BANG_DASH;
          } else {
            state = COMMENT;
          }
          break;
        case COMMENT_LT_BANG_DASH:
          if (c === DASH) {
            p++;
            state = COMMENT_LT_BANG_DASH_DASH;
          } else {
            state = COMMENT_END_DASH;
          }
          break;
        case COMMENT_LT_BANG_DASH_DASH:
          // '>' and anything else: reconsume in the comment end state.
          state = COMMENT_END;
          break;
        case COMMENT_END_DASH:
          if (c === DASH) {
            p++;
            state = COMMENT_END;
          } else {
            state = COMMENT;
          }
          break;
        case COMMENT_END:
          if (c === GT) return this.#closeComment(lt, p);
          if (c === BANG) {
            p++;
            state = COMMENT_END_BANG;
          } else if (c === DASH) {
            p++;
          } else {
            state = COMMENT;
          }
          break;
        default: // COMMENT_END_BANG
          if (c === GT) return this.#closeComment(lt, p);
          if (c === DASH) {
            p++;
            state = COMMENT_END_DASH;
          } else {
            state = COMMENT;
          }
          break;
      }
    }
    this.#pos = end;
    return newComment(lt, end, false, false);
  }

  #closeComment(lt, gt) {
    this.#pos = gt + 1;
    return newComment(lt, gt + 1, true, false);
  }

  // Tag name state, then the attribute states.
  #tag(type, lt, nameStart) {
    const b = this.#b;
    const end = b.length;
    let p = nameStart;
    while (p < end) {
      const c = b[p];
      if (isWs(c) || c === SLASH || c === GT) break;
      p++;
    }
    if (p >= end) return this.#eof(); // EOF in tag: the tag is dropped
    return this.#tagBody(newTag(type, tagName(b, nameStart, p), lt), p);
  }

  // Attribute states from the before attribute name state, reconsuming b[p].
  #tagBody(token, from) {
    const b = this.#b;
    const end = b.length;
    let p = from;
    let state = BEFORE_ATTR_NAME;
    let slot = SLOT_NONE;
    let mark = 0; // start of the current attribute name or unquoted value
    while (p < end) {
      const c = b[p];
      switch (state) {
        case BEFORE_ATTR_NAME:
          if (isWs(c)) {
            p++;
          } else if (c === SLASH || c === GT) {
            state = AFTER_ATTR_NAME;
          } else {
            // Also '=': it is the first character of the attribute name.
            mark = p;
            p++;
            state = ATTR_NAME;
          }
          break;
        case ATTR_NAME:
          if (isWs(c) || c === SLASH || c === GT) {
            slot = this.#leaveAttrName(token, mark, p);
            state = AFTER_ATTR_NAME;
          } else if (c === EQ) {
            slot = this.#leaveAttrName(token, mark, p);
            p++;
            state = BEFORE_ATTR_VALUE;
          } else {
            p++;
          }
          break;
        case AFTER_ATTR_NAME:
          if (isWs(c)) {
            p++;
          } else if (c === SLASH) {
            p++;
            state = SELF_CLOSING_START_TAG;
          } else if (c === EQ) {
            p++;
            state = BEFORE_ATTR_VALUE;
          } else if (c === GT) {
            return this.#emitTag(token, p);
          } else {
            mark = p;
            p++;
            state = ATTR_NAME;
          }
          break;
        case BEFORE_ATTR_VALUE:
          if (isWs(c)) {
            p++;
          } else if (c === DQ || c === SQ) {
            const close = b.indexOf(c, p + 1);
            if (close < 0) return this.#eof();
            setValue(token, slot, p + 1, close);
            p = close + 1;
            state = AFTER_ATTR_VALUE_QUOTED;
          } else if (c === GT) {
            // Missing attribute value: the value stays empty.
            return this.#emitTag(token, p);
          } else {
            mark = p;
            state = ATTR_VALUE_UNQUOTED;
          }
          break;
        case ATTR_VALUE_UNQUOTED:
          if (isWs(c)) {
            setValue(token, slot, mark, p);
            p++;
            state = BEFORE_ATTR_NAME;
          } else if (c === GT) {
            setValue(token, slot, mark, p);
            return this.#emitTag(token, p);
          } else {
            p++;
          }
          break;
        case AFTER_ATTR_VALUE_QUOTED:
          if (isWs(c)) {
            p++;
            state = BEFORE_ATTR_NAME;
          } else if (c === SLASH) {
            p++;
            state = SELF_CLOSING_START_TAG;
          } else if (c === GT) {
            return this.#emitTag(token, p);
          } else {
            state = BEFORE_ATTR_NAME; // missing white space: reconsume
          }
          break;
        default: // SELF_CLOSING_START_TAG
          if (c === GT) {
            token.selfClosing = true;
            return this.#emitTag(token, p);
          }
          state = BEFORE_ATTR_NAME; // unexpected solidus: reconsume
          break;
      }
    }
    return this.#eof(); // EOF in tag: the tag is dropped
  }

  // Duplicate attributes: only the first one with a name is kept (M-03).
  #leaveAttrName(token, start, end) {
    const b = this.#b;
    if (token.blockAttr === null && spanEqualsLower(b, start, end, BLOCK_ATTR)) {
      token.blockAttr = { start: end, end };
      return SLOT_BLOCK;
    }
    if (token.typeAttr === null && spanEqualsLower(b, start, end, TYPE_ATTR)) {
      token.typeAttr = { start: end, end };
      return SLOT_TYPE;
    }
    return SLOT_NONE;
  }

  #emitTag(token, gt) {
    token.end = gt + 1;
    this.#pos = gt + 1;
    return token;
  }

  // True when "</" + the raw text name + (white space | '/' | '>') starts at lt.
  #appropriateEndTagAt(lt) {
    const b = this.#b;
    const after = lt + 2 + this.#rawName.length;
    return (
      b[lt + 1] === SLASH &&
      after < b.length &&
      matchesLower(b, lt + 2, this.#rawName) &&
      (isWs(b[after]) || b[after] === SLASH || b[after] === GT)
    );
  }

  // The appropriate end tag at lt: continue in the attribute states after its name.
  #rawEndTag(lt) {
    this.#mode = TextMode.DATA;
    const name = this.#rawName;
    return this.#tagBody(newTag(TokenType.END_TAG, name, lt), lt + 2 + name.length);
  }

  // RCDATA and RAWTEXT states with their less-than sign and end tag states.
  #rawText() {
    const b = this.#b;
    let p = this.#pos;
    for (;;) {
      const lt = b.indexOf(LT, p);
      if (lt < 0) return this.#eof();
      if (this.#appropriateEndTagAt(lt)) return this.#rawEndTag(lt);
      p = lt + 1;
    }
  }

  // Script data states, including the escaped and double-escaped states.
  #scriptData() {
    const b = this.#b;
    const end = b.length;
    let p = this.#pos;
    let state = SD;
    while (p < end) {
      const c = b[p];
      switch (state) {
        case SD: {
          const lt = b.indexOf(LT, p);
          if (lt < 0) return this.#eof();
          p = lt + 1;
          state = SD_LT;
          break;
        }
        case SD_LT:
          if (c === SLASH) {
            p++;
            state = SD_END_TAG_OPEN;
          } else if (c === BANG) {
            p++;
            state = SD_ESCAPE_START;
          } else {
            state = SD;
          }
          break;
        case SD_END_TAG_OPEN:
          if (this.#appropriateEndTagAt(p - 2)) return this.#rawEndTag(p - 2);
          // Not the appropriate end tag: the name letters are text.
          state = SD;
          break;
        case SD_ESCAPE_START:
          if (c === DASH) {
            p++;
            state = SD_ESCAPE_START_DASH;
          } else {
            state = SD;
          }
          break;
        case SD_ESCAPE_START_DASH:
          if (c === DASH) {
            p++;
            state = ESC_DASH_DASH;
          } else {
            state = SD;
          }
          break;
        case ESC:
          if (c === DASH) state = ESC_DASH;
          else if (c === LT) state = ESC_LT;
          p++;
          break;
        case ESC_DASH:
          if (c === DASH) state = ESC_DASH_DASH;
          else if (c === LT) state = ESC_LT;
          else state = ESC;
          p++;
          break;
        case ESC_DASH_DASH:
          if (c === LT) state = ESC_LT;
          else if (c === GT) state = SD;
          else if (c !== DASH) state = ESC;
          p++;
          break;
        case ESC_LT:
          if (c === SLASH) {
            p++;
            state = ESC_END_TAG_OPEN;
          } else if (isAlpha(c)) {
            state = DESC_START;
          } else {
            state = ESC;
          }
          break;
        case ESC_END_TAG_OPEN:
          if (this.#appropriateEndTagAt(p - 2)) return this.#rawEndTag(p - 2);
          state = ESC;
          break;
        case DESC_START:
        case DESC_END: {
          // Script data double escape start / end state: read the letters into
          // the temporary buffer, then decide at the first other character.
          let q = p;
          while (q < end && isAlpha(b[q])) q++;
          if (q >= end) return this.#eof();
          const t = b[q];
          const fromStart = state === DESC_START;
          if (isWs(t) || t === SLASH || t === GT) {
            const isScript = spanEqualsLower(b, p, q, SCRIPT);
            // Start: "script" enters double escaped. End: "script" leaves it.
            state = isScript === fromStart ? DESC : ESC;
            p = q + 1;
          } else {
            state = fromStart ? ESC : DESC;
            p = q;
          }
          break;
        }
        case DESC:
          if (c === DASH) state = DESC_DASH;
          else if (c === LT) state = DESC_LT;
          p++;
          break;
        case DESC_DASH:
          if (c === DASH) state = DESC_DASH_DASH;
          else if (c === LT) state = DESC_LT;
          else state = DESC;
          p++;
          break;
        case DESC_DASH_DASH:
          if (c === LT) state = DESC_LT;
          else if (c === GT) state = SD;
          else if (c !== DASH) state = DESC;
          p++;
          break;
        default: // DESC_LT
          if (c === SLASH) {
            p++;
            state = DESC_END;
          } else {
            state = DESC;
          }
          break;
      }
    }
    return this.#eof();
  }
}

function setValue(token, slot, start, end) {
  if (slot === SLOT_BLOCK) token.blockAttr = { start, end };
  else if (slot === SLOT_TYPE) token.typeAttr = { start, end };
}
