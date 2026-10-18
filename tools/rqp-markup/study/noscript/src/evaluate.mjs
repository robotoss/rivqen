// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// Evaluation of one document for the H-42 study (WP-17 S3 T-02): each noscript
// model of src/markup-study.mjs and each head rule of src/head-study.mjs, against
// parse5 8.0.1 in the scripting-enabled and the scripting-disabled view.
//
// A disagreement is a document that a variant accepts (valid markup, or a head end
// tag found) where parse5 has other blocks or block bytes (the crosscheck
// comparison, ../../../crosscheck/src/compare.mjs), or another head end, or where
// the inserted manifest element is not the last element of the head. A variant
// that rejects a document is not compared: rejection is the safe fallback (H-12).

import { analyze as analyzeCandidate } from '../../../node/src/markup.mjs';
import { compareWithView } from '../../../crosscheck/src/compare.mjs';
import { parse5View } from '../../../crosscheck/src/view.mjs';
import { ALLOW_B0, ALLOW_B1, analyzeStudy } from './markup-study.mjs';
import { headWalk } from './head-study.mjs';
import { headView, manifestView } from './views.mjs';

/**
 * Markup variants: the shipped M-11, model (a), model (a) with svg and math in C
 * (input for H-40 relaxation 2, not a T-02 option), and model (b) with two lists.
 */
export const MARKUP_VARIANTS = Object.freeze([
  { name: 'shipped', opts: { noscript: 'shipped' } },
  { name: 'a', opts: { noscript: 'main' } },
  { name: 'a-fx', opts: { noscript: 'main', foreignInContent: true } },
  { name: 'b0', opts: { noscript: 'allow', allow: ALLOW_B0 } },
  { name: 'b1', opts: { noscript: 'allow', allow: ALLOW_B1 } },
]);

export const HEAD_VARIANTS = Object.freeze(['shipped', 'h0', 'h1', 'h2']);

const VIEWS = Object.freeze([
  ['enabled', true],
  ['disabled', false],
]);

/** Same validity and the same blocks (id, format, start, end, sha256) and revisions. */
export function sameResult(x, y) {
  return JSON.stringify(x) === JSON.stringify(y);
}

/**
 * @param {Uint8Array} bytes
 * @param {{ head?: boolean, markup?: boolean }} [what]
 */
export function evaluateDocument(bytes, { head = true, markup = true } = {}) {
  const out = { markup: {}, head: {}, candidateEqual: true, divergences: 0 };
  if (markup) {
    const views = {};
    const viewOf = (name, scripting) => (views[name] ??= parse5View(bytes, { scripting }));
    for (const v of MARKUP_VARIANTS) {
      const s = analyzeStudy(bytes, v.opts);
      const r = { valid: s.result.valid, blocks: s.result.blocks.length, why: s.why };
      if (v.name === 'shipped') out.candidateEqual = sameResult(s.result, analyzeCandidate(bytes));
      if (v.name === 'a') out.divergences = s.divergences;
      if (s.result.valid) {
        for (const [name, scripting] of VIEWS) {
          const c = compareWithView(s.result, viewOf(name, scripting));
          r[name] = c.status;
          if (c.status !== 'agree') r[`${name}Details`] = c.details.slice(0, 3);
        }
      }
      out.markup[v.name] = r;
    }
  }
  if (head) {
    let hv = null;
    const manifests = new Map();
    for (const rule of HEAD_VARIANTS) {
      const w = headWalk(bytes, { rule });
      const r = { at: w.at, headNoscripts: w.headNoscripts, failure: w.failure };
      if (w.at >= 0) {
        hv ??= { enabled: headView(bytes, true), disabled: headView(bytes, false) };
        if (!manifests.has(w.at)) {
          manifests.set(w.at, { enabled: manifestView(bytes, w.at, true), disabled: manifestView(bytes, w.at, false) });
        }
        const m = manifests.get(w.at);
        for (const [name] of VIEWS) {
          if (!hv[name].ok || !m[name].ok) {
            r[name] = 'no-view'; // not UTF-8 or larger than 5 MiB: never sent with RQP (M-01, M-02, H-62)
            continue;
          }
          const ok = hv[name].end === w.at && m[name].inHead && m[name].headEnd;
          r[name] = ok ? 'agree' : 'disagree';
          if (!ok) r[`${name}Details`] = { parse5End: hv[name].end, manifestInHead: m[name].inHead };
        }
      }
      out.head[rule] = r;
    }
  }
  return out;
}
