// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// Study copy of the head walk of tools/rqp-protocol/src/markup.mjs (documentHeadEnd,
// manifest-patch section 1.7 items 1 to 3, DL-031, DL-034) for WP-17 S3 T-02 (H-42,
// head). Study code only.
//
// Rules (option `rule`):
//   shipped  the walk as shipped: a noscript in the head is head content, and its
//            content C (raw text with scripting enabled) is not checked.
//   h0       a noscript start tag while the document head is the current node
//            means: the document has no head end tag.
//   h1       the tokens of C (tokenized as in markup M-11) are only white space,
//            comments, DOCTYPE tokens, the start tags that the "in head noscript"
//            insertion mode keeps (basefont, bgsound, link, meta, noframes, style)
//            with their raw text, and ignored end tags (DL-034). Else: no head end tag.
//   h2       as h1, but the start tags are the head content of item 2 without
//            noscript and template: base, basefont, bgsound, link, meta, noframes,
//            script, style, title. With scripting disabled, base, script and title
//            close the noscript element early ("anything else" of "in head
//            noscript") and the rest of C is in "in head", so the head stays open.

import { TEXT_MODE, VOID } from '../../../node/src/names.mjs';
import { TokenType, Tokenizer } from '../../../node/src/tokenizer.mjs';

export const HEAD_RULES = Object.freeze(['shipped', 'h0', 'h1', 'h2']);

const MAX_STACK = 256;

const HEAD_BEFORE = 0;
const HEAD_OPEN = 1;
const HEAD_NONE = 2;

const HEAD_CONTENT = new Set([
  'base', 'basefont', 'bgsound', 'link', 'meta', 'noframes', 'noscript', 'script', 'style', 'template', 'title',
]);

const HEAD_END_TAGS = new Set(['head', 'body', 'html', 'br']);

/** h1: the start tags that "in head noscript" processes with the "in head" rules. */
export const HEAD_NOSCRIPT_H1 = Object.freeze(new Set(['basefont', 'bgsound', 'link', 'meta', 'noframes', 'style']));

/** h2: head content of item 2 without noscript and template. */
export const HEAD_NOSCRIPT_H2 = Object.freeze(new Set([
  'base', 'basefont', 'bgsound', 'link', 'meta', 'noframes', 'script', 'style', 'title',
]));

const NOSCRIPT_END = Buffer.from('</noscript>', 'latin1');

function ignoredEndTag(t) {
  return t.type === TokenType.END_TAG && !HEAD_END_TAGS.has(t.name);
}

function onlyWhiteSpace(bytes, from, to) {
  let p = from;
  if (p === 0 && to >= 3 && bytes[0] === 0xef && bytes[1] === 0xbb && bytes[2] === 0xbf) p = 3;
  for (; p < to; p++) {
    const c = bytes[p];
    if (c !== 0x09 && c !== 0x0a && c !== 0x0c && c !== 0x0d && c !== 0x20) return false;
  }
  return true;
}

/** White space only, without the BOM rule (C never starts at offset 0 of the document). */
function onlySpace(bytes, from, to) {
  for (let p = from; p < to; p++) {
    const c = bytes[p];
    if (c !== 0x09 && c !== 0x0a && c !== 0x0c && c !== 0x0d && c !== 0x20) return false;
  }
  return true;
}

/**
 * h1 and h2: true when the noscript content C of a noscript in the document head
 * keeps the head open in the scripting-disabled view. Returns a label for the first
 * token that fails (for the corpus inventory), or null when C passes.
 *
 * @param {Buffer} content C
 * @param {ReadonlySet<string>} allowed start tag names
 * @returns {string | null}
 */
export function headNoscriptFailure(content, allowed) {
  const b = Buffer.concat([content, NOSCRIPT_END]);
  const tok = new Tokenizer(b);
  let prevEnd = 0;
  let inText = false; // the previous token was a start tag of a raw text or RCDATA element
  for (;;) {
    const t = tok.next();
    if (!inText && !onlySpace(b, prevEnd, t.start)) return 'text';
    const textEnd = inText;
    inText = false;
    prevEnd = t.end;
    if (t.type === TokenType.START_TAG) {
      if (!allowed.has(t.name)) return `<${t.name}>`;
      const mode = TEXT_MODE.get(t.name);
      if (mode !== undefined) {
        tok.switchTo(mode, t.name);
        inText = true;
      }
    } else if (t.type === TokenType.END_TAG) {
      if (t.name === 'noscript' && !textEnd) return t.start === content.length ? null : '</noscript>';
      if (!textEnd && !ignoredEndTag(t)) return `</${t.name}>`;
    } else if (t.type === TokenType.EOF) {
      return 'eof';
    }
    // COMMENT and DOCTYPE: allowed (both modes keep the head open).
  }
}

function headStep(state, headIndex, stackLength, t, gapOk, rule) {
  if (state === HEAD_BEFORE) {
    if (!gapOk) return { state: HEAD_NONE, end: false };
    if (t.type === TokenType.COMMENT || t.type === TokenType.DOCTYPE || t.type === TokenType.EOF) return { state, end: false };
    if (ignoredEndTag(t)) return { state, end: false };
    if (t.type === TokenType.START_TAG && (t.name === 'html' || t.name === 'head')) return { state, end: false };
    return { state: HEAD_NONE, end: false };
  }
  if (state !== HEAD_OPEN || headIndex !== stackLength - 1) return { state, end: false };
  if (!gapOk) return { state: HEAD_NONE, end: false };
  if (t.type === TokenType.COMMENT || t.type === TokenType.DOCTYPE) return { state, end: false };
  if (rule === 'h0' && t.type === TokenType.START_TAG && t.name === 'noscript') return { state: HEAD_NONE, end: false };
  if (t.type === TokenType.START_TAG && HEAD_CONTENT.has(t.name)) return { state, end: false };
  if (ignoredEndTag(t)) return { state, end: false };
  return { state: HEAD_NONE, end: t.type === TokenType.END_TAG && t.name === 'head' };
}

/**
 * Head walk with a noscript rule. Returns { at, headNoscripts, failure, headStart }:
 * `at` is the byte offset of the "<" of the head end tag, or -1; `headNoscripts`
 * counts the noscript elements opened while the document head was the current node;
 * `failure` is the label of the first C token that failed (h1, h2), or null;
 * `headStart` is the offset after the head start tag that item 1 accepted, or -1
 * (the insertion point of option hc of the report, a protocol change).
 *
 * @param {Uint8Array} input
 * @param {{ rule?: 'shipped' | 'h0' | 'h1' | 'h2' }} [opts]
 */
export function headWalk(input, { rule = 'shipped' } = {}) {
  if (!HEAD_RULES.includes(rule)) throw new RangeError(`unknown head rule ${rule}`);
  const bytes = Buffer.from(input.buffer, input.byteOffset, input.byteLength);
  const allowed = rule === 'h1' ? HEAD_NOSCRIPT_H1 : HEAD_NOSCRIPT_H2;
  const tok = new Tokenizer(bytes);
  const stack = [];
  let region = -1;
  let head = HEAD_BEFORE;
  let headIndex = -1;
  let prevEnd = 0;
  let headNoscripts = 0;
  let cStart = -1; // start of C of the noscript of the document head, while it is open
  let failure = null;
  let at = -1;
  let headStart = -1; // offset after the head start tag that item 1 accepts (option hc of the report)
  for (;;) {
    const t = tok.next();
    const step = headStep(head, headIndex, stack.length, t, onlyWhiteSpace(bytes, prevEnd, t.start), rule);
    const opensHead = head === HEAD_BEFORE && step.state === HEAD_BEFORE && t.type === TokenType.START_TAG && t.name === 'head';
    const headNoscript = head === HEAD_OPEN && step.state === HEAD_OPEN && headIndex === stack.length - 1
      && t.type === TokenType.START_TAG && t.name === 'noscript';
    head = step.state;
    prevEnd = t.end;
    if (step.end && at < 0) at = t.start;
    if (t.type === TokenType.EOF) break;
    if (t.type === TokenType.START_TAG) {
      const html = region < 0;
      const foreignRoot = t.name === 'svg' || t.name === 'math';
      const push = html ? !(foreignRoot && t.selfClosing) && !VOID.has(t.name) : !t.selfClosing;
      if (push) {
        if (stack.length >= MAX_STACK) break;
        stack.push(t.name);
        if (html && foreignRoot) region = stack.length - 1;
      }
      if (opensHead) {
        head = HEAD_OPEN;
        headIndex = stack.length - 1;
        headStart = t.end;
      }
      if (headNoscript) {
        headNoscripts++;
        cStart = t.end;
      }
      const mode = html ? TEXT_MODE.get(t.name) : undefined;
      if (mode !== undefined) tok.switchTo(mode, t.name);
    } else if (t.type === TokenType.END_TAG) {
      if (cStart >= 0 && t.name === 'noscript') {
        // The end of C (RAWTEXT ends at the first appropriate end tag).
        if (rule === 'h1' || rule === 'h2') {
          const why = headNoscriptFailure(bytes.subarray(cStart, t.start), allowed);
          if (why !== null) {
            if (failure === null) failure = why;
            if (head === HEAD_OPEN) head = HEAD_NONE;
          }
        }
        cStart = -1;
      }
      const idx = stack.lastIndexOf(t.name);
      if (idx >= 0) {
        stack.length = idx;
        if (region >= stack.length) region = -1;
        if (head === HEAD_OPEN && headIndex >= stack.length) head = HEAD_NONE;
      }
    }
  }
  return { at, headNoscripts, failure, headStart };
}

/** Byte offset of the head end tag under a rule, or -1. */
export function documentHeadEndStudy(input, rule = 'shipped') {
  return headWalk(input, { rule }).at;
}
