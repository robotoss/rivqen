// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// The attribute rule of manifest-patch section 4.2 (H-56, H-59) and the items that
// the H-40 study proposes for foreign content (report section 5.4). Study code: the
// shipped rule is tools/rqp-protocol/src/markup.mjs (attributeRejected); the test
// test/attrs.test.mjs checks that attributeReason() agrees with it.

import { TEXT_MODE } from '../../../node/src/names.mjs';
import { TokenType, Tokenizer } from '../../../node/src/tokenizer.mjs';
import { attributesOf } from '../../../../rqp-protocol/src/markup.mjs';

export { attributesOf };

const SCHEMES = ['javascript:', 'vbscript:', 'data:'];

/** Item 3 of section 4.2 on a string: "&amp;" -> "&", no TAB, LF, CR, no leading 00-20. */
export function normalizeValue(text) {
  return text.replaceAll('&amp;', '&').replace(/[\t\n\r]/g, '').replace(/^[\x00-\x20]+/, '');
}

const lowerAscii = (s) => s.replace(/[A-Z]/g, (m) => m.toLowerCase());

/** Item 4 on a normalized string. */
function deniedScheme(copy) {
  const lower = lowerAscii(copy.slice(0, 11));
  return SCHEMES.some((s) => lower.startsWith(s));
}

/**
 * Section 4.2 for one attribute, with the item that rejects it.
 * @param {string} name lower case, as the tokenizer gives it
 * @param {Buffer} value raw value bytes
 * @returns {'on' | 'amp' | 'scheme' | null}
 */
export function attributeReason(name, value) {
  if (name.startsWith('on')) return 'on'; // item 1
  const text = value.toString('latin1');
  if (/&(?!amp;)/.test(text)) return 'amp'; // item 2
  return deniedScheme(normalizeValue(text)) ? 'scheme' : null; // items 3 and 4
}

/** SMIL animation elements (SVG Animations; tag names in lower case, as the tokenizer gives them). */
export const SMIL = Object.freeze(new Set(['animate', 'animatemotion', 'animatetransform', 'animatecolor', 'set', 'discard']));

/**
 * The proposed items for a start tag in a foreign region of html block content.
 *   F1 smil-target         a SMIL element has an href or xlink:href attribute (it would
 *                          target an element outside the block, by id)
 *   F2 smil-attributename  a SMIL element animates an attribute whose name contains
 *                          "href" or starts with "on" (after items 2 and 3, ASCII lower case)
 *   F3 values-scheme       a part of a ";"-separated values list starts with a denied scheme
 * @param {string} tag lower-case tag name
 * @param {{ name: string, value: Buffer }[]} attrs
 * @returns {string[]} the items that reject (empty when the tag passes)
 */
export function foreignReasons(tag, attrs) {
  const out = [];
  for (const { name, value } of attrs) {
    const text = value.toString('latin1');
    if (SMIL.has(tag)) {
      if (name === 'href' || name === 'xlink:href') out.push('smil-target');
      if (name === 'attributename') {
        const v = lowerAscii(normalizeValue(text).replace(/[\x00-\x20]+$/, ''));
        if (v.includes('href') || v.startsWith('on')) out.push('smil-attributename');
      }
    }
    if (name === 'values' && text.split(';').some((part) => deniedScheme(normalizeValue(part)))) out.push('values-scheme');
  }
  return out;
}

/**
 * The start tags of html content as the walk of section 4.2 reads them (the
 * reference walk, tools/rqp-protocol/src/markup.mjs passesAttributeRule):
 * tokenized from the data state, with the state changes of M-04 after every
 * start tag, also in a foreign region.
 * @param {Buffer} content
 * @returns {{ start: number, end: number, name: string }[]}
 */
export function walkStartTags(content) {
  const out = [];
  const tok = new Tokenizer(content);
  for (;;) {
    const t = tok.next();
    if (t.type === TokenType.EOF) return out;
    if (t.type !== TokenType.START_TAG) continue;
    out.push({ start: t.start, end: t.end, name: t.name });
    const mode = TEXT_MODE.get(t.name);
    if (mode !== undefined) tok.switchTo(mode, t.name);
  }
}

/**
 * Attribute statistics of the content of the blocks of a valid result with a trace:
 * what the current rule and the proposed foreign items reject. Names only, no values.
 * @param {Buffer} bytes
 * @param {object} result analyzeRelaxed(..., { trace: true }) result, valid
 */
export function blockAttributeStats(bytes, result) {
  const s = {
    htmlTags: 0,
    foreignTags: 0,
    current: { html: { on: 0, amp: 0, scheme: 0 }, foreign: { on: 0, amp: 0, scheme: 0 } },
    proposed: { 'smil-target': 0, 'smil-attributename': 0, 'values-scheme': 0 },
    foreignElements: new Map(),
    foreignAttributes: new Map(),
    rejectedCurrent: false,
    rejectedProposed: false,
  };
  for (const [start, inRegion, end, name] of result.trace.content) {
    if (!result.blocks.some((b) => start >= b.start && start < b.end)) continue;
    const foreign = inRegion || name === 'svg' || name === 'math';
    const attrs = attributesOf(bytes, start, end);
    if (foreign) {
      s.foreignTags++;
      s.foreignElements.set(name, (s.foreignElements.get(name) ?? 0) + 1);
    } else s.htmlTags++;
    for (const { name: a, value } of attrs) {
      const reason = attributeReason(a, value);
      if (reason !== null) {
        s.current[foreign ? 'foreign' : 'html'][reason]++;
        s.rejectedCurrent = true;
      }
      if (foreign) s.foreignAttributes.set(a, (s.foreignAttributes.get(a) ?? 0) + 1);
    }
    if (foreign) {
      for (const item of foreignReasons(name, attrs)) {
        s.proposed[item]++;
        s.rejectedProposed = true;
      }
    }
  }
  return s;
}
