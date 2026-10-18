// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// noscript inventory of one document for the H-42 study (WP-17 S3 T-02): where each
// noscript element in HTML context is (in the document head, before the head, or
// elsewhere) and what its content C holds. The result has counts and tag names
// only, never document text. Tag names are kept only when they match
// ^[a-z][a-z0-9-]{0,31}$; other names count as "?".

import { TEXT_MODE, VOID } from '../../baseline/names-s2.mjs';
import { TokenType, Tokenizer } from '../../../node/src/tokenizer.mjs';
import { HEAD_NOSCRIPT_H1, HEAD_NOSCRIPT_H2, headNoscriptFailure } from './head-study.mjs';

const MAX_STACK = 4096; // inventory only; a deeper document is marked "truncated"
const MAX_NAMES = 64; // distinct names kept for each location
const NAME = /^[a-z][a-z0-9-]{0,31}$/;
const NOSCRIPT_END = Buffer.from('</noscript>', 'latin1');

/** A tag name that is safe to put into the evidence. */
export function safeName(name) {
  return NAME.test(name) ? name : '?';
}

function onlySpace(b, from, to) {
  for (let p = from; p < to; p++) {
    const c = b[p];
    if (c !== 0x09 && c !== 0x0a && c !== 0x0c && c !== 0x0d && c !== 0x20) return false;
  }
  return true;
}

/** The start tag names, text and comments of one noscript content C. */
export function contentSummary(content) {
  const b = Buffer.concat([content, NOSCRIPT_END]);
  const tok = new Tokenizer(b);
  const tags = new Set();
  let text = false;
  let comment = false;
  let prevEnd = 0;
  let inText = false;
  let empty = true;
  for (;;) {
    const t = tok.next();
    if (!inText && !onlySpace(b, prevEnd, t.start)) text = true;
    inText = false;
    prevEnd = t.end;
    if (t.type === TokenType.EOF || (t.type === TokenType.END_TAG && t.name === 'noscript')) break;
    empty = false;
    if (t.type === TokenType.START_TAG) {
      tags.add(safeName(t.name));
      const mode = TEXT_MODE.get(t.name);
      if (mode !== undefined) {
        tok.switchTo(mode, t.name);
        inText = true;
      }
    } else if (t.type === TokenType.COMMENT) {
      comment = true;
    }
  }
  return { tags, text, comment, empty: empty && !text };
}

function addAll(target, names) {
  for (const n of names) if (target.size < MAX_NAMES || target.has(n)) target.add(n);
}

/**
 * @param {Uint8Array} input
 * @returns {{ head: number, preHead: number, other: number, headTags: string[], otherTags: string[],
 *   headText: number, headComment: number, headEmpty: number, h1Failures: string[], h2Failures: string[],
 *   otherText: number, truncated: boolean }}
 */
export function noscriptInventory(input) {
  const bytes = Buffer.from(input.buffer, input.byteOffset, input.byteLength);
  const tok = new Tokenizer(bytes);
  const stack = [];
  let region = -1;
  let headIndex = -1; // stack index of the first head element while it is open
  let headSeen = false;
  let cStart = -1;
  let where = '';
  const out = {
    head: 0, preHead: 0, other: 0, headText: 0, headComment: 0, headEmpty: 0, otherText: 0, truncated: false,
  };
  const headTags = new Set();
  const otherTags = new Set();
  const h1 = new Set();
  const h2 = new Set();
  for (;;) {
    const t = tok.next();
    if (t.type === TokenType.EOF) {
      if (cStart >= 0) finish(bytes.subarray(cStart));
      break;
    }
    if (t.type === TokenType.START_TAG) {
      const html = region < 0;
      if (html && t.name === 'noscript') {
        if (headIndex >= 0 && headIndex === stack.length - 1) where = 'head';
        else if (!headSeen) where = 'preHead';
        else where = 'other';
        out[where]++;
        cStart = t.end;
      }
      const foreignRoot = t.name === 'svg' || t.name === 'math';
      const push = html ? !(foreignRoot && t.selfClosing) && !VOID.has(t.name) : !t.selfClosing;
      if (push) {
        if (stack.length >= MAX_STACK) {
          out.truncated = true;
          break;
        }
        stack.push(t.name);
        if (html && foreignRoot) region = stack.length - 1;
        if (html && t.name === 'head' && !headSeen) {
          headSeen = true;
          headIndex = stack.length - 1;
        }
      }
      const mode = html ? TEXT_MODE.get(t.name) : undefined;
      if (mode !== undefined) tok.switchTo(mode, t.name);
    } else if (t.type === TokenType.END_TAG) {
      if (cStart >= 0 && t.name === 'noscript') {
        finish(bytes.subarray(cStart, t.start));
        cStart = -1;
      }
      const idx = stack.lastIndexOf(t.name);
      if (idx >= 0) {
        stack.length = idx;
        if (region >= stack.length) region = -1;
        if (headIndex >= stack.length) headIndex = -1;
      }
    }
  }
  return {
    ...out,
    headTags: [...headTags].sort(),
    otherTags: [...otherTags].sort(),
    h1Failures: [...h1].sort(),
    h2Failures: [...h2].sort(),
  };

  function finish(content) {
    const s = contentSummary(content);
    if (where === 'head') {
      addAll(headTags, s.tags);
      if (s.text) out.headText++;
      if (s.comment) out.headComment++;
      if (s.empty) out.headEmpty++;
      const f1 = headNoscriptFailure(content, HEAD_NOSCRIPT_H1);
      const f2 = headNoscriptFailure(content, HEAD_NOSCRIPT_H2);
      if (f1 !== null) addAll(h1, [labelOf(f1)]);
      if (f2 !== null) addAll(h2, [labelOf(f2)]);
    } else {
      addAll(otherTags, s.tags);
      if (s.text) out.otherText++;
    }
  }
}

/** A failure label of headNoscriptFailure with a safe tag name. */
function labelOf(f) {
  const m = /^<(\/?)([^>]*)>$/.exec(f);
  return m ? `<${m[1]}${safeName(m[2])}>` : f;
}
