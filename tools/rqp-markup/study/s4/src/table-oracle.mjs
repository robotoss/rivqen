// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// parse5 8.0.1 oracle of the H-85 study (WP-17 S4 T-03). For a result that the
// study rule accepts, it reports a finding when the WHATWG tree disagrees:
//
//   parse5-<view>-<category>  the checks of the cross-check tool (block elements,
//                             offsets, namespace, end tags, manifest-like scripts)
//   depth-<view>              the claim of markup.md M-26 with the implied closure
//                             (src/depth-s4.mjs), in that scripting view
//   table-<view>-missing      a table part or col start tag of the trace has no element
//   table-<view>-parent       the element of a traced start tag has another parent than
//                             F of the token stack (implied tbody, tr and colgroup
//                             elements of the tree are skipped): so an implied
//                             closure put the next cell, row or row group where the
//                             token stack says
//   rewrite-invalid           the shipped candidate rejects the rewrite D' (the
//                             implied end tags inserted as bytes, see rewrite())
//   rewrite-blocks            the shipped candidate gives other blocks for D'
//   rewrite-<view>-dom        parse5 builds another tree for D' than for D
//
// With no rewrite finding, the study result for D is the shipped result for D' and
// the browser builds the same tree for both: the closure reduces to the shipped rules.
//
// The findings never hold document text: only kinds, offsets and tag names.

import { parse, serialize } from '../../../crosscheck/node_modules/parse5/dist/index.js';
import { compareWithView } from '../../../crosscheck/src/compare.mjs';
import { decodeUtf8 } from '../../../crosscheck/src/utf8.mjs';
import { analyze } from '../../../node/src/markup.mjs';
import { parse5View } from '../../../crosscheck/src/view.mjs';
import { treeIndex } from '../../relax/src/oracle.mjs';
import { analyzeDepthClosure } from './depth-s4.mjs';
import { analyzeS4, contractFields } from './markup-s4.mjs';

export const VIEWS = Object.freeze(['enabled', 'disabled']);

const HTML_NS = 'http://www.w3.org/1999/xhtml';

/** The parent element of `el`, skipping elements without a start tag (implied by the tree builder). */
function explicitParent(el) {
  let p = el.parentNode;
  while (p && typeof p.tagName === 'string' && !p.sourceCodeLocation?.startTag) p = p.parentNode;
  return p && typeof p.tagName === 'string' ? p : null;
}

/**
 * Table findings of one view.
 * @param {{ byStart: Map<number, object[]>, map: number[] }} index treeIndex
 * @param {[number, string, number][]} trace
 */
export function tableFindings(index, trace) {
  const out = [];
  for (const [at, name, parentAt] of trace) {
    const el = (index.byStart.get(at) ?? []).find((e) => e.tagName === name && e.namespaceURI === HTML_NS);
    if (el === undefined) {
      out.push({ kind: 'missing', detail: `<${name}> at ${at} has no element` });
      continue;
    }
    const p = explicitParent(el);
    const pAt = p === null ? -1 : index.map[p.sourceCodeLocation.startTag.startOffset];
    if (pAt !== parentAt) out.push({ kind: 'parent', detail: `<${name}> at ${at}: parent at ${pAt}, token stack F at ${parentAt}` });
  }
  return out;
}

/**
 * The rewrite D' of a document D: the implied end tags of the closures inserted as
 * bytes, each at the "<" of the start tag that caused it.
 * @param {Uint8Array} bytes D
 * @param {[number, string][]} virtual [at, name], in document order
 * @returns {{ bytes: Buffer, shift: (offset: number) => number }}
 */
export function rewrite(bytes, virtual) {
  const parts = [];
  let last = 0;
  const marks = []; // [at, inserted bytes before and at this offset]
  let added = 0;
  for (const [at, name] of virtual) {
    parts.push(bytes.subarray(last, at));
    const tag = Buffer.from(`</${name}>`, 'latin1');
    parts.push(tag);
    added += tag.length;
    marks.push([at, added]);
    last = at;
  }
  parts.push(bytes.subarray(last));
  const shift = (offset) => {
    let s = 0;
    for (const [at, n] of marks) if (at <= offset) s = n;
    return offset + s;
  };
  return { bytes: Buffer.concat(parts), shift };
}

/**
 * Rewrite findings: the shipped candidate accepts D' with the same blocks (offsets
 * mapped), and parse5 builds the same tree for D and D' in both views. Then the
 * study result for D reduces to a shipped result: the implied closure is the end tag.
 * @param {Uint8Array} bytes
 * @param {object} result a valid study result with `virtual`
 */
export function rewriteFindings(bytes, result) {
  if (result.virtual.length === 0) return [];
  const out = [];
  const { bytes: d2, shift } = rewrite(bytes, result.virtual);
  // b3 also changes D for explicit cell end tags: D' is checked with that D rule
  // (the study rule b3, which then needs no closure), the other modes with the shipped rules.
  const ref = result.mode === 'b3' ? analyzeS4(d2, { close: 'b3' }) : analyze(d2);
  if (!ref.valid) out.push({ kind: 'rewrite-invalid', detail: ref.error });
  else if (result.mode === 'b3' && ref.virtual.length > 0) out.push({ kind: 'rewrite-closure', detail: 'the rewrite still needs a closure' });
  else {
    const same = ref.blocks.length === result.blocks.length
      && ref.blocks.every((b, i) => b.id === result.blocks[i].id && b.start === shift(result.blocks[i].start) && b.end === shift(result.blocks[i].end));
    if (!same) out.push({ kind: 'rewrite-blocks', detail: 'blocks differ' });
  }
  for (const v of VIEWS) {
    const scripting = v === 'enabled';
    const t1 = decodeUtf8(Buffer.from(bytes));
    const t2 = decodeUtf8(d2);
    if (t1 === null || t2 === null) continue;
    const s1 = serialize(parse(t1.text, { scriptingEnabled: scripting }));
    const s2 = serialize(parse(t2.text, { scriptingEnabled: scripting }));
    if (s1 !== s2) out.push({ kind: `rewrite-${v}-dom`, detail: 'tree of D and of the rewrite differ' });
  }
  return out;
}

/**
 * All findings for a study result (analyzeS4 with trace). An invalid result has no
 * findings: the rules are stricter than the tree on purpose.
 * @param {Uint8Array} bytes
 * @param {object} result
 * @returns {{ kind: string, detail: string }[]}
 */
export function checkResult(bytes, result) {
  if (!result.valid) return [];
  const out = [];
  for (const v of VIEWS) {
    const scripting = v === 'enabled';
    const c = compareWithView(contractFields(result), parse5View(bytes, { scripting }));
    if (c.status === 'disagree') out.push({ kind: `parse5-${v}-${c.categories.join('+')}`, detail: c.details[0] ?? '' });
    const d = analyzeDepthClosure(bytes, { scripting, release: result.mode === 'b3' });
    if (d.violations.length > 0) out.push({ kind: `depth-${v}`, detail: d.violations[0] });
    if (result.trace !== null && result.trace.length > 0) {
      const index = treeIndex(bytes, scripting);
      if (index !== null) for (const f of tableFindings(index, result.trace)) out.push({ kind: `table-${v}-${f.kind}`, detail: f.detail });
    }
  }
  out.push(...rewriteFindings(bytes, result));
  return out;
}
