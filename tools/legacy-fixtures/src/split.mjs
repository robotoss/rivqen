// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// Template and data split of a legacy page (docs/engineering/protocol/legacy-markers.md
// sections 1 to 4, legacy-wire.md rules C-06 to C-08 and C-16). The split is textual and pure.
//
// split never throws for a string input and runs in time linear in the text length
// (C-16, DL-077). Profiles:
//   legacy-canonical  title once; empty or longer than 128 names stay in the template;
//                     a repeated name makes the split fail (DL-075)
//   legacy-java       empty name gives the key {}; the last value of a repeated name wins
//   legacy-php        as legacy-java
//   legacy-node       empty names get {auto0}, {auto1}, ...; the last value of a repeated name wins
// No profile reproduces the several-title defect of Java and PHP (U-11, DL-069).
//
// The results are exactly those of the regular expressions of C-06 and C-07:
//   title  /<title([^\n]*?)<\/title>/i                                        (first match)
//   block  /<!--sonicdiff-?(\w*)-->([\s\S]+?)<!--sonicdiff-?(\w*)-end-->/gi   (all matches)
// A backtracking engine takes quadratic time on many unclosed openers, so this module
// scans forward instead. The property test in test/split.test.mjs compares both.
// Case folding is ASCII only (the expressions have the i flag without the u flag), and
// \w is [A-Za-z0-9_]. Indexes are UTF-16 code units, as in the expressions.

import { asciiLower } from './fields.mjs';

export const MAX_NAME = 128;

const TITLE_OPEN = '<title';
const TITLE_CLOSE = '</title>';
const MARKER = '<!--sonicdiff';
const OPEN_TAIL = '-->';
const END_TAIL = '-end-->';
const PLACEHOLDER = /\{\w*\}/g;

/** True for the code units of \w without the u flag: [A-Za-z0-9_]. */
function isWord(code) {
  return (code >= 0x61 && code <= 0x7a) || (code >= 0x41 && code <= 0x5a) || (code >= 0x30 && code <= 0x39) || code === 0x5f;
}

/** The index after the run of \w code units that starts at `from`. */
function wordEnd(text, from) {
  let at = from;
  while (at < text.length && isWord(text.charCodeAt(at))) at++;
  return at;
}

/**
 * C-06: the first match of the title expression in `text`, as { start, end }, or null.
 * `lower` is asciiLower(text) (same length, same indexes).
 *
 * An opener at i matches when the first `</title>` at or after i + 6 comes before the
 * first LF at or after i + 6 (the content is [^\n]*?, lazy). Both "next" positions only
 * move forward, so each part of the text is searched a bounded number of times.
 */
export function findTitle(text, lower = asciiLower(text)) {
  let close = -1;
  let lf = -1;
  for (let i = lower.indexOf(TITLE_OPEN); i >= 0; i = lower.indexOf(TITLE_OPEN, i + 1)) {
    const from = i + TITLE_OPEN.length;
    if (close < from) {
      close = lower.indexOf(TITLE_CLOSE, from);
      if (close < 0) return null; // no later opener can match either
    }
    if (lf !== Infinity && lf < from) {
      lf = text.indexOf('\n', from);
      if (lf < 0) lf = Infinity;
    }
    if (close < lf) return { start: i, end: close + TITLE_CLOSE.length };
  }
  return null;
}

/**
 * The opener `<!--sonicdiff-?(\w*)-->` at `at` (where `lower` has MARKER).
 * Returns { name, end } or null. The two ways to read `-?` cannot both match, and
 * \w* must take the whole word run because `-` is not a word character.
 */
function openerAt(text, lower, at) {
  const q = at + MARKER.length;
  if (text.charCodeAt(q) === 0x2d) {
    const r = wordEnd(text, q + 1);
    if (text.startsWith(OPEN_TAIL, r)) return { name: text.slice(q + 1, r), end: r + OPEN_TAIL.length };
  }
  const r = wordEnd(text, q);
  if (text.startsWith(OPEN_TAIL, r)) return { name: text.slice(q, r), end: r + OPEN_TAIL.length };
  return null;
}

/**
 * The end of the end marker `<!--sonicdiff-?(\w*)-end-->` at `at` (where `lower` has
 * MARKER), or -1. `end` compares ASCII case-insensitively, so it is read from `lower`.
 */
function endMarkerAt(lower, at) {
  const q = at + MARKER.length;
  if (lower.charCodeAt(q) === 0x2d) {
    const r = wordEnd(lower, q + 1);
    if (lower.startsWith(END_TAIL, r)) return r + END_TAIL.length;
  }
  const r = wordEnd(lower, q);
  if (lower.startsWith(END_TAIL, r)) return r + END_TAIL.length;
  return -1;
}

/**
 * C-07: the matches of the block expression in `text` from left to right, without
 * overlap, as [{ start, end, name }]. Linear time:
 *  - the opener scan visits each MARKER once;
 *  - the end-marker scan only moves forward (its start, the opener end + 1, grows),
 *    so it visits each MARKER at most once;
 *  - the word runs that the two scans read belong to different MARKERs and do not
 *    overlap (a word run has no `<`);
 *  - when no end marker follows an opener, no later opener can match: the scan stops.
 */
export function findBlocks(text, lower = asciiLower(text)) {
  const out = [];
  let endStart = -1; // start of the next end marker found so far
  let endEnd = -1; // its end
  let at = lower.indexOf(MARKER);
  while (at >= 0) {
    const opener = openerAt(text, lower, at);
    if (opener === null) {
      at = lower.indexOf(MARKER, at + 1);
      continue;
    }
    const from = opener.end + 1; // [\s\S]+? needs at least one code unit
    if (endStart < from) {
      endStart = -1;
      let e = lower.indexOf(MARKER, from); // from > the last endStart: no MARKER is read twice
      while (e >= 0) {
        const end = endMarkerAt(lower, e);
        if (end >= 0) {
          endStart = e;
          endEnd = end;
          break;
        }
        e = lower.indexOf(MARKER, e + 1);
      }
      if (endStart < 0) return out; // no end marker after this opener: no later match
    }
    out.push({ start: at, end: endEnd, name: opener.name });
    at = lower.indexOf(MARKER, endEnd);
  }
  return out;
}

/**
 * Split page text. Returns { ok: true, template, data, blocks } where data is a
 * Map of key (with braces) to value, {title} first, then the blocks in document
 * order, and blocks is the number of extracted blocks; or { ok: false, reason }.
 *
 * @param {string} profile
 * @param {string} text
 */
export function split(profile, text) {
  const data = new Map();
  const title = findTitle(text);
  let template = text;
  if (title !== null) template = text.slice(0, title.start) + '{title}' + text.slice(title.end);
  data.set('{title}', title === null ? '' : text.slice(title.start, title.end));

  const canonical = profile === 'legacy-canonical';
  let blocks = 0;
  let auto = 0;
  let failed = false;
  const pieces = [];
  let last = 0;
  for (const match of findBlocks(template)) {
    const whole = template.slice(match.start, match.end);
    const { name } = match;
    let key;
    if (name === '' && profile === 'legacy-node') {
      key = `{auto${auto++}}`;
    } else if (canonical && (name === '' || name.length > MAX_NAME)) {
      continue; // the match stays in the template
    } else {
      key = `{${name}}`;
    }
    if (canonical && data.has(key)) failed = true;
    data.set(key, whole);
    blocks++;
    pieces.push(template.slice(last, match.start), key);
    last = match.end;
  }
  if (failed) return { ok: false, reason: 'DUPLICATE_BLOCK_NAME' };
  pieces.push(template.slice(last));
  return { ok: true, template: pieces.join(''), data, blocks };
}

/**
 * Rebuild a page: each placeholder of the template is replaced once, in template
 * order, by its value (legacy-markers.md section 5). A placeholder without a value
 * and a second occurrence stay as they are. Linear: \w* cannot run past the next `{`.
 *
 * @param {string} template
 * @param {Map<string, string>} data
 */
export function rebuild(template, data) {
  const used = new Set();
  return template.replace(PLACEHOLDER, (placeholder) => {
    if (!data.has(placeholder) || used.has(placeholder)) return placeholder;
    used.add(placeholder);
    return data.get(placeholder);
  });
}
