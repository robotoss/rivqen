// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// parse5 8.0.1 oracle of the H-40 study. For a result that the study rule says
// is valid, it reports a finding when the WHATWG tree disagrees:
//
//   parse5-<view>-<category>  the checks of the cross-check tool (blocks, offsets,
//                             namespace, end tags, manifest-like scripts), both views
//   depth-invariant           the claim of markup.md M-26 (scripting enabled)
//   tree-<view>-<kind>        new for r1: each start tag of html block content
//                             makes an element inside the block element, in the
//                             namespace that the token stack says (HTML or foreign)
//   noscript-<kind>[-head]    new for r2 (scripting disabled): the noscript element
//                             ends at the end tag that the token pass found, and each
//                             start tag of C makes an element inside it, in the
//                             namespace that the C pass says; "-head" when the
//                             noscript element is a child of head (H-42 head, T-02)
//
// The oracle never prints document text. Details hold offsets and tag names only.

import { parse } from '../../../crosscheck/node_modules/parse5/dist/index.js';
import { compareWithView } from '../../../crosscheck/src/compare.mjs';
import { analyzeDepth } from '../../../crosscheck/src/depth.mjs';
import { decodeUtf8, utf16ToUtf8Map } from '../../../crosscheck/src/utf8.mjs';
import { HTML_NS, parse5View } from '../../../crosscheck/src/view.mjs';

const VIEWS = Object.freeze(['enabled', 'disabled']);

/**
 * The elements of the parse5 tree by the byte offset of their start tag.
 * @param {Uint8Array} bytes valid UTF-8, at most 5 MiB (the caller checked the result)
 * @param {boolean} scripting
 */
export function treeIndex(bytes, scripting) {
  const decoded = decodeUtf8(bytes);
  if (decoded === null) return null;
  const map = utf16ToUtf8Map(decoded.text, decoded.bomBytes);
  const doc = parse(decoded.text, { sourceCodeLocationInfo: true, scriptingEnabled: scripting });
  /** @type {Map<number, object[]>} */
  const byStart = new Map();
  /** @type {Map<number, object>} element by the byte offset after its start tag */
  const byTagEnd = new Map();
  const stack = [doc];
  while (stack.length > 0) {
    const node = stack.pop();
    if (typeof node.tagName === 'string') {
      const loc = node.sourceCodeLocation;
      if (loc && loc.startTag) {
        const at = map[loc.startTag.startOffset];
        const list = byStart.get(at);
        if (list === undefined) byStart.set(at, [node]);
        else list.push(node);
        byTagEnd.set(map[loc.startTag.endOffset], node);
      }
    }
    // A loop, not a spread: a node can have more children than a call can take arguments.
    if (node.content && Array.isArray(node.content.childNodes)) for (const c of node.content.childNodes) stack.push(c);
    if (Array.isArray(node.childNodes)) for (const c of node.childNodes) stack.push(c);
  }
  return { byStart, byTagEnd, map };
}

/** True when `node` is `ancestor` or below it (parentNode chain). */
export function isInside(node, ancestor) {
  for (let n = node; n; n = n.parentNode) if (n === ancestor) return true;
  return false;
}

const isForeign = (el) => el.namespaceURI !== HTML_NS;

/** r1: the start tags of html block content against the tree of one view. */
export function contentFindings(index, result, trace) {
  const out = [];
  for (const block of result.blocks) {
    const el = index.byTagEnd.get(block.start);
    if (el === undefined) continue; // the block comparison reports this
    for (const [at, inRegion, , name] of trace.content) {
      const foreign = inRegion || name === 'svg' || name === 'math'; // a region root is a foreign element too
      if (at < block.start || at >= block.end) continue;
      const els = index.byStart.get(at) ?? [];
      if (els.length === 0) {
        out.push({ kind: 'missing', detail: `start tag at ${at} has no element` });
        continue;
      }
      const inside = els.find((e) => isInside(e, el));
      if (inside === undefined) out.push({ kind: 'outside', detail: `<${els[0].tagName}> at ${at} is not inside block ${block.id}` });
      else if (isForeign(inside) !== foreign) {
        out.push({ kind: 'namespace', detail: `<${inside.tagName}> at ${at}: token stack ${foreign ? 'foreign' : 'html'}, tree ${isForeign(inside) ? 'foreign' : 'html'}` });
      }
    }
  }
  return out;
}

/** r2: each noscript element and the start tags of its content C (scripting disabled). */
export function noscriptFindings(index, trace, length) {
  const out = [];
  for (const ns of trace.noscripts) {
    const el = (index.byStart.get(ns.tag) ?? []).find((e) => e.tagName === 'noscript' && !isForeign(e));
    if (el === undefined) {
      out.push({ kind: 'missing', head: false, detail: `noscript at ${ns.tag} has no element` });
      continue;
    }
    const head = el.parentNode?.tagName === 'head';
    if (ns.end < length) {
      const endTag = el.sourceCodeLocation?.endTag;
      const end = endTag ? index.map[endTag.startOffset] : null;
      if (end !== ns.end) out.push({ kind: 'end', head, detail: `noscript at ${ns.tag}: end tag ${ns.end}, tree ${end}` });
    }
    for (const [at, inRegion, name] of ns.tags) {
      const foreign = inRegion || name === 'svg' || name === 'math';
      if (at >= ns.end) continue; // the "</noscript>" of the C pass
      const els = index.byStart.get(at) ?? [];
      const inside = els.find((e) => isInside(e, el));
      if (inside === undefined) out.push({ kind: 'outside', head, detail: `start tag at ${at} is not inside noscript at ${ns.tag}` });
      else if (isForeign(inside) !== foreign) out.push({ kind: 'namespace', head, detail: `<${inside.tagName}> at ${at} in noscript` });
    }
  }
  return out;
}

/** The parse5 views, trees and depth analysis of one document, computed once and on demand. */
export class Oracle {
  /** @param {Uint8Array} bytes */
  constructor(bytes) {
    this.bytes = bytes;
    this.views = new Map();
    this.trees = new Map();
    this.depthResult = null;
  }

  view(v) {
    if (!this.views.has(v)) this.views.set(v, parse5View(this.bytes, { scripting: v === 'enabled' }));
    return this.views.get(v);
  }

  tree(v) {
    if (!this.trees.has(v)) this.trees.set(v, treeIndex(this.bytes, v === 'enabled'));
    return this.trees.get(v);
  }

  depth() {
    if (this.depthResult === null) this.depthResult = analyzeDepth(this.bytes);
    return this.depthResult;
  }

  /**
   * Findings for a result with a trace (analyzeRelaxed with trace: true).
   * An invalid result has no findings: the rules are stricter than the tree.
   * @returns {{ kind: string, detail: string }[]}
   */
  check(result) {
    if (!result.valid) return [];
    const out = [];
    for (const v of VIEWS) {
      const c = compareWithView(result, this.view(v));
      if (c.status === 'disagree') out.push({ kind: `parse5-${v}-${c.categories.join('+')}`, detail: c.details[0] ?? '' });
      const index = this.tree(v);
      if (index === null) continue;
      for (const f of contentFindings(index, result, result.trace)) out.push({ kind: `tree-${v}-${f.kind}`, detail: f.detail });
      if (v === 'disabled') {
        for (const f of noscriptFindings(index, result.trace, this.bytes.length)) {
          out.push({ kind: `noscript-${f.kind}${f.head ? '-head' : ''}`, detail: f.detail });
        }
      }
    }
    const d = this.depth();
    if (d.violations.length > 0) out.push({ kind: 'depth-invariant', detail: d.violations[0] });
    return out;
  }
}
