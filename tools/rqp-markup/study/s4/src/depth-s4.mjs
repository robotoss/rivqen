// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// The depth claim of markup.md M-26 with the implied closure of H-85 (WP-17 S4
// T-03). Study code only.
//
// The cross-check tool (../../../crosscheck/src/depth.mjs) computes D on the token
// stack of M-05 while parse5 8.0.1 parses the document, and checks the claim after
// each token. The study rule (src/markup-s4.mjs, close 'b1' or 'b2') adds one step:
// before a start tag with a name in TABLE_PART or col in HTML context, while F is
// not an allowed F of M-09, the token stack applies the end tag of F. This module
// runs the same analysis with that step: during one call it wraps the startTag
// method of DepthCounter, so the counter of the main pass and the copies for the
// noscript content C (M-11 item 8) apply the closure too.
//
// The study rule never uses the closure in block content or in C. On a document
// that the study rule accepts, the closure therefore happens exactly where the study
// rule applies it, so this counter has the D of the study rule.

import { DepthCounter, analyzeDepth } from '../../../crosscheck/src/depth.mjs';

const TABLE_FAMILY = new Set(['table', 'caption', 'colgroup', 'tbody', 'td', 'tfoot', 'th', 'thead', 'tr']);

/** Allowed F of M-09 (markup.md, the table of M-09). */
export const ALLOWED_F = Object.freeze(new Map([
  ['caption', new Set(['table'])],
  ['colgroup', new Set(['table'])],
  ['tbody', new Set(['table'])],
  ['thead', new Set(['table'])],
  ['tfoot', new Set(['table'])],
  ['col', new Set(['table', 'colgroup'])],
  ['tr', new Set(['table', 'tbody', 'thead', 'tfoot'])],
  ['td', new Set(['table', 'tbody', 'thead', 'tfoot', 'tr'])],
  ['th', new Set(['table', 'tbody', 'thead', 'tfoot', 'tr'])],
]));

/** F of M-09 on a DepthCounter stack (HTML context): the nearest table-family entry, stop at template. */
function familyOf(stack) {
  for (let i = stack.length - 1; i >= 0; i--) {
    const n = stack[i].name;
    if (n === 'template') return null;
    if (TABLE_FAMILY.has(n)) return n;
  }
  return null;
}

/**
 * The end tags of the implied closure for a start tag `name` (at most three).
 * Applies them to the counter.
 * @param {DepthCounter} counter
 * @param {string} name
 */
export function applyClosure(counter, name) {
  const allowed = ALLOWED_F.get(name);
  if (allowed === undefined) return 0;
  let n = 0;
  for (let step = 0; step < 3; step++) {
    const f = familyOf(counter.stack);
    if (f === null || allowed.has(f)) break;
    counter.endTag(f); // F is the nearest entry with its name: no table-family entry is above F
    n++;
  }
  return n;
}

/** b3: the names of src/markup-s4.mjs RELEASE (the same set). */
const RELEASE = new Set(['td', 'th', 'caption']);

/** Units that a counter entry holds: 1, its implied parts, and units moved to it (option b3). */
const unitsOf = (e) => 1 + e.implied.size + (e.extra ?? 0);

/**
 * The end tag of DepthCounter with the release of option b3: the pop of an HTML element
 * with a name in RELEASE by its own end tag frees the units of every popped entry. Other pops are
 * those of the cross-check tool; a pop that does not subtract moves the units of the
 * popped entries to the new current node (`extra`), which does not change D.
 */
function endTagWithRelease(original) {
  return function endTag(name) {
    const stack = this.stack;
    let i = stack.length - 1;
    while (i >= 0 && stack[i].name !== name) i--;
    if (i < 0) return original.call(this, name);
    const popped = stack.slice(i);
    const units = popped.reduce((n, e) => n + unitsOf(e), 0);
    const release = !popped[0].foreign && RELEASE.has(name);
    const exact = popped.length === 1 && !popped[0].dirty && name !== 'html' && name !== 'body';
    const below = i > 0 ? stack[i - 1] : null;
    const wasDirty = below?.dirty ?? false;
    const d0 = this.d;
    original.call(this, name);
    if (release) {
      this.d = d0 - units;
      if (below !== null) below.dirty = wasDirty;
    } else if (!exact && below !== null) {
      below.extra = (below.extra ?? 0) + units;
    }
  };
}

/**
 * analyzeDepth of the cross-check tool with the implied closure, and with the
 * release of option b3 when `release` is true.
 * @param {Uint8Array} bytes
 * @param {{ scripting?: boolean, release?: boolean }} [options]
 */
export function analyzeDepthClosure(bytes, { scripting = true, release = false } = {}) {
  const proto = DepthCounter.prototype;
  const originalStart = proto.startTag;
  const originalEnd = proto.endTag;
  proto.startTag = function startTagWithClosure(name, selfClosing, attrNames) {
    if (!this.inForeign) applyClosure(this, name);
    return originalStart.call(this, name, selfClosing, attrNames);
  };
  if (release) proto.endTag = endTagWithRelease(originalEnd);
  try {
    return analyzeDepth(bytes, { scripting });
  } finally {
    proto.startTag = originalStart;
    proto.endTag = originalEnd;
  }
}
