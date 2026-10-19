// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// Patch safety of the S5 options (manifest-patch section 2.4 steps 8 and 9, section 4
// item 1). A patch brings the new content of an html block; the client checks the
// patched document with the markup analysis (step 8: the S5 option) and the content
// with the patch content rule (step 9). This file models step 9 with three variants:
//
//   shipped  the shipped rule (tools/rqp-protocol/src/markup.mjs patchContentFailure):
//            PATCH_FORBIDDEN, FOREIGN_ALLOWED and the attribute rule of section 4.2 on
//            the start tags that the walk reads (M-04: the noscript content C is raw
//            text and is not read)
//   p1       shipped, and `noscript` in PATCH_FORBIDDEN: a start tag `noscript` in HTML
//            context of the content rejects (a full document can have it, a patch not)
//   p2       noscript allowed in a patch: shipped, and the same three checks on the
//            start tags of each C (tokenized as M-11 does: the trace of the analysis)
//
// The apply oracle parses the content as a fragment with the block element as the
// context in the two modes of the WHATWG fragment parser that an SDK can reach
// (evidence/markup-s5/research-t01.md section 5): "inert" (innerHTML,
// insertAdjacentHTML, setHTMLUnsafe in a document with scripting enabled: noscript
// content is text) and "disabled" (DOMParser, parseHTMLUnsafe, or innerHTML in a
// document with scripting disabled: noscript content is markup). For content that a
// variant accepts:
//
//   patch-unseen-<mode>  an element of the parse has a start tag that the variant did
//                        not check (the noscript mXSS class when mode is disabled)
//   patch-mode-diff      the two parses have different elements (informative: a
//                        rendering difference, not a check gap)
//
// parse5 8.0.1 has no declarative shadow DOM; `shadowrootmode` is a markup rule of
// the options t1 and t2 (step 8), so no valid patched document has it in a block.

import {
  FOREIGN_ALLOWED,
  PATCH_FORBIDDEN,
  attributeRejected,
  attributesOf,
  patchContentFailure,
  startTagOffsets,
} from '../../../../rqp-protocol/src/markup.mjs';
import { fragmentIndex } from './oracle-s5.mjs';

export const PATCH_RULES = Object.freeze(['shipped', 'p1', 'p2']);
export const APPLY_MODES = Object.freeze(['inert', 'disabled']);

/** Step 9 failure for one start tag (the three checks of section 4 item 1). */
function tagFailure(bytes, at, end, name, html) {
  if (html && PATCH_FORBIDDEN.has(name)) return 'element';
  if (!html && !FOREIGN_ALLOWED.has(name)) return 'foreign';
  if (attributesOf(bytes, at, end).some(({ name: a, value }) => attributeRejected(a, value, name, html))) return 'attribute';
  return null;
}

/** The C entries of the trace inside [start, end). */
function noscriptsIn(trace, start, end) {
  return trace.noscripts.filter((ns) => ns.tag >= start && ns.tag < end);
}

/**
 * Step 9 of one variant for the content of one html block of a valid traced result.
 * @returns {{ failure: string | null, seen: Set<number> }} seen: input offsets of the start tags that the variant read
 */
export function patchRule(rule, bytes, trace, block) {
  const content = Buffer.from(bytes.subarray(block.start, block.end));
  let failure = patchContentFailure(content);
  const walk = startTagOffsets(content) ?? [];
  const seen = new Set(walk.map((x) => x + block.start));
  const inner = noscriptsIn(trace, block.start, block.end);
  if (rule === 'p1' && failure === null && inner.length > 0) failure = 'noscript';
  if (rule === 'p2') {
    for (const ns of inner) {
      for (const [at, foreign, name, , , end] of ns.tags) {
        if (at >= ns.end) continue;
        seen.add(at);
        // A start tag in HTML context of C (also an svg region root) or in a foreign region of C.
        if (failure === null) failure = tagFailure(bytes, at, end, name, !foreign);
      }
    }
  }
  return { failure, seen };
}

/**
 * The patch checks of one valid traced result: for each html block (not title), the
 * failure of each variant and the apply findings for the variants that accept it.
 * @returns {{ blocks: { id: string, rules: Record<string, string | null> }[], findings: { kind: string, rule: string, detail: string }[] }}
 */
export function patchFindings(bytes, result) {
  const blocks = [];
  const findings = [];
  const { trace } = result;
  for (const [i, block] of result.blocks.entries()) {
    const name = trace.blockNames[i];
    if (block.format !== 'html' || name === 'title') continue;
    const rules = {};
    const content = Buffer.from(bytes.subarray(block.start, block.end));
    let parses = null;
    for (const rule of PATCH_RULES) {
      const { failure, seen } = patchRule(rule, bytes, trace, block);
      rules[rule] = failure;
      if (failure !== null) continue;
      parses ??= Object.fromEntries(APPLY_MODES.map((m) => [m, fragmentIndex(content, name, m === 'inert', block.start)]));
      for (const m of APPLY_MODES) {
        const idx = parses[m];
        if (idx === null) continue;
        for (const [at, els] of idx.byStart) {
          if (!seen.has(at)) findings.push({ kind: `patch-unseen-${m}`, rule, detail: `block ${block.id}: <${els[0].tagName}> at ${at}` });
        }
      }
      const a = parses.inert;
      const b = parses.disabled;
      if (a !== null && b !== null) {
        const ka = [...a.byStart.keys()].sort((x, y) => x - y).join(',');
        const kb = [...b.byStart.keys()].sort((x, y) => x - y).join(',');
        if (ka !== kb) findings.push({ kind: 'patch-mode-diff', rule, detail: `block ${block.id}` });
      }
    }
    blocks.push({ id: block.id, rules });
  }
  return { blocks, findings };
}
