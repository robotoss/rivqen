// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// parse5 8.0.1 oracle of the S5 study (template and noscript in block content). For
// a result that a study option says is valid (analyzeS5 with trace: true), it
// reports a finding when a WHATWG tree disagrees. Kinds (`<view>` is enabled or
// disabled, the two full-document scripting views of markup.md section 2):
//
//   parse5-<view>-<category>  the cross-check comparison (block elements, ids,
//                             offsets, explicit end tags, HTML namespace, format,
//                             manifest-like scripts); crosscheck/src/compare.mjs
//   content-<view>-<kind>     each start tag of html block content (main pass, not
//                             C) makes an element inside the block element (through
//                             a template contents fragment to its template), in the
//                             namespace of the token stack (missing, outside,
//                             namespace), whose parent is the token-stack parent
//                             (parent; see "known classes")
//   template-<view>-end       each template of block content ends at the end tag
//                             that the token pass found
//   noscript-<kind>           scripting disabled: each noscript element ends at the
//                             end tag that the token pass found (end), each start tag
//                             of its content C makes an element inside it (missing,
//                             outside), in the namespace of the C pass (namespace),
//                             with the parent of the C pass (parent); "-block" when the
//                             noscript is in block content
//   fragment-<kind>           the content of each html block (not title) parsed as a
//                             fragment with the block element as the context (parse5
//                             parseFragment, scripting enabled: innerHTML in a
//                             WebView): the same elements (missing, extra, namespace,
//                             parent)
//   (the view "spec" is scripting enabled with the WHATWG table scope, see
//   withSpecTableScope: parse5 8.0.1 deviates there inside template contents)
//   depth-<view>              the claim of M-26 (no template open), both views
//   depth-template-<view>     the same bounds while a template is open: not part of the
//                             claim; reported for the template options
//
// Known classes (counted, not findings): a parent difference while a `p` opened in
// the content is open ("p-closure": a start tag in PCLOSE, or a table, closes the p in
// the tree builder; M-24 item a covers it only for p and span blocks; the shipped
// rules accept it, and it does not change block bytes) is labelled `known-p-closure`.
//
// The oracle never prints document text: details have offsets and tag names only.

import { parse, parseFragment } from '../../../crosscheck/node_modules/parse5/dist/index.js';
import { OpenElementStack } from '../../../crosscheck/node_modules/parse5/dist/parser/open-element-stack.js';
import { TAG_ID } from '../../../crosscheck/node_modules/parse5/dist/common/html.js';
import { compareWithView } from '../../../crosscheck/src/compare.mjs';
import { decodeUtf8, utf16ToUtf8Map } from '../../../crosscheck/src/utf8.mjs';
import { HTML_NS, parse5View } from '../../../crosscheck/src/view.mjs';
import { analyzeDepthS5 } from './depth-s5.mjs';

/**
 * The full-document views: parse5 8.0.1 with scripting enabled and disabled, and "spec":
 * scripting enabled with the WHATWG table scope (withSpecTableScope).
 */
export const VIEWS = Object.freeze(['enabled', 'disabled', 'spec']);

const isElement = (n) => n !== null && typeof n === 'object' && typeof n.tagName === 'string';
const isForeign = (el) => el.namespaceURI !== HTML_NS;

/**
 * Index of a parse5 tree: elements by the byte offset of their start tag, the
 * template contents hosts, and the byte offset of each element.
 */
export function indexTree(root, map, base = 0) {
  /** @type {Map<number, object[]>} */
  const byStart = new Map();
  const startOf = new Map(); // element -> byte offset of its start tag, or -1 (implied)
  const endOf = new Map(); // element -> byte offset of its end tag, or null
  const hosts = new Map(); // template contents fragment -> template element
  const stack = [root];
  while (stack.length > 0) {
    const node = stack.pop();
    if (isElement(node)) {
      const loc = node.sourceCodeLocation;
      const at = loc && loc.startTag ? map[loc.startTag.startOffset] + base : -1;
      startOf.set(node, at);
      endOf.set(node, loc && loc.endTag ? map[loc.endTag.startOffset] + base : null);
      if (at >= 0) {
        const list = byStart.get(at);
        if (list === undefined) byStart.set(at, [node]);
        else list.push(node);
      }
      if (node.content && Array.isArray(node.content.childNodes)) {
        hosts.set(node.content, node);
        for (const c of node.content.childNodes) stack.push(c);
      }
    }
    if (Array.isArray(node.childNodes)) for (const c of node.childNodes) stack.push(c);
  }
  const parentOf = (n) => {
    const p = n.parentNode;
    if (p === null || p === undefined) return null;
    return hosts.get(p) ?? p;
  };
  return { byStart, startOf, endOf, parentOf };
}

/** The parse5 tree of a full document, indexed. */
export function treeIndex(bytes, scripting) {
  const decoded = decodeUtf8(bytes);
  if (decoded === null) return null;
  const map = utf16ToUtf8Map(decoded.text, decoded.bomBytes);
  const doc = parse(decoded.text, { sourceCodeLocationInfo: true, scriptingEnabled: scripting });
  return indexTree(doc, map);
}

/**
 * parse5 8.0.1 deviates from the WHATWG definition of "has an element in table scope"
 * (html, table and template in the HTML namespace): its hasInTableScope and
 * hasTableBodyContextInTableScope stop only at table and html. Inside template contents
 * in a table insertion mode ("in template" after caption, colgroup, tbody, tfoot, thead,
 * tr, td, th), parse5 then pops out of the template where a browser ignores the token.
 * `withSpecTableScope` runs `fn` with both functions stopping at template too (a local
 * patch of the prototype, restored after the call; parse5 is synchronous).
 */
export function withSpecTableScope(fn) {
  const proto = OpenElementStack.prototype;
  const saved = [proto.hasInTableScope, proto.hasTableBodyContextInTableScope];
  const stop = (stack, i) => {
    const id = stack.tagIDs[i];
    return id === TAG_ID.TABLE || id === TAG_ID.HTML || id === TAG_ID.TEMPLATE;
  };
  proto.hasInTableScope = function hasInTableScope(tagName) {
    for (let i = this.stackTop; i >= 0; i--) {
      if (this.treeAdapter.getNamespaceURI(this.items[i]) !== HTML_NS) continue;
      if (this.tagIDs[i] === tagName) return true;
      if (stop(this, i)) return false;
    }
    return true;
  };
  proto.hasTableBodyContextInTableScope = function hasTableBodyContextInTableScope() {
    for (let i = this.stackTop; i >= 0; i--) {
      if (this.treeAdapter.getNamespaceURI(this.items[i]) !== HTML_NS) continue;
      const id = this.tagIDs[i];
      if (id === TAG_ID.TBODY || id === TAG_ID.THEAD || id === TAG_ID.TFOOT) return true;
      if (stop(this, i)) return false;
    }
    return true;
  };
  try {
    return fn();
  } finally {
    [proto.hasInTableScope, proto.hasTableBodyContextInTableScope] = saved;
  }
}

/** True when `node` is `ancestor` or below it (also through template contents). */
function isInside(index, node, ancestor) {
  for (let n = node; n; n = index.parentOf(n)) if (n === ancestor) return true;
  return false;
}

/**
 * The effective parent of `el` below `top`: the nearest ancestor whose start tag is in
 * [lo, hi) or is `top` itself. Ancestors without a start tag (implied html, head, body,
 * tbody, tr, colgroup) and ancestors that start outside the range (formatting elements
 * that the tree builder reconstructs from before the block) are skipped. Returns the
 * byte offset of the parent start tag, or null when `top` is not reached.
 */
function effectiveParent(index, el, top, lo, hi) {
  return effectiveParentWalk(index, el, top, lo, hi).at;
}

/** effectiveParent, and whether the walk skipped an element that starts before the range (a reconstruction). */
function effectiveParentWalk(index, el, top, lo, hi) {
  let reconstructed = false;
  for (let n = index.parentOf(el); n; n = index.parentOf(n)) {
    if (n === top) return { at: index.startOf.get(n) ?? null, reconstructed };
    if (!isElement(n)) continue;
    const at = index.startOf.get(n);
    if (at >= lo && at < hi) return { at, reconstructed };
    if (at >= 0) reconstructed = true;
  }
  return { at: null, reconstructed };
}

/** The element at `at` that is inside `top`, or the first element at `at`, or undefined. */
function elementAt(index, at, top) {
  const els = index.byStart.get(at) ?? [];
  return els.find((e) => isInside(index, e, top)) ?? els[0];
}

/** Content checks of one view for the html blocks of a valid traced result. */
export function contentFindings(index, result, view) {
  const out = [];
  const { trace } = result;
  for (const [i, block] of result.blocks.entries()) {
    const name = trace.blockNames[i];
    if (block.format !== 'html' || name === 'title') continue;
    const tag = trace.blockTags[i];
    const blockEl = (index.byStart.get(tag) ?? []).find((e) => !isForeign(e) && e.tagName === name);
    if (blockEl === undefined) continue; // the block comparison reports this
    for (const [at, foreign, , tname, parentAt, inTemplate, pOpen] of trace.content) {
      if (at < block.start || at >= block.end) continue;
      const sfx = inTemplate ? '-tpl' : '';
      const el = elementAt(index, at, blockEl);
      if (el === undefined) {
        out.push({ kind: `content-${view}-missing${sfx}`, detail: `<${tname}> at ${at} has no element` });
        continue;
      }
      if (!isInside(index, el, blockEl)) {
        out.push({ kind: `content-${view}-outside${sfx}`, detail: `<${tname}> at ${at} is not inside block ${block.id}` });
        continue;
      }
      const wantForeign = foreign || tname === 'svg' || tname === 'math';
      if (isForeign(el) !== wantForeign) out.push({ kind: `content-${view}-namespace${sfx}`, detail: `<${tname}> at ${at}` });
      const p = effectiveParentWalk(index, el, blockEl, block.start, block.end);
      if (p.at !== parentAt) {
        const kind = pOpen ? 'known-p-closure' : p.reconstructed ? 'known-reconstruct' : `content-${view}-parent${sfx}`;
        out.push({ kind, detail: `<${tname}> at ${at}: token parent ${parentAt}, tree ${p.at}` });
      }
    }
    for (const tpl of trace.templates) {
      if (!tpl.inBlock || tpl.tag < block.start || tpl.tag >= block.end) continue;
      const el = (index.byStart.get(tpl.tag) ?? []).find((e) => e.tagName === 'template' && !isForeign(e));
      const end = el === undefined ? null : index.endOf.get(el);
      if (end !== tpl.end) out.push({ kind: `template-${view}-end`, detail: `template at ${tpl.tag}: end tag ${tpl.end}, tree ${end}` });
    }
  }
  return out;
}

/** Scripting disabled: each noscript element and the start tags of its content C. */
export function noscriptFindings(index, trace, length) {
  const out = [];
  for (const ns of trace.noscripts) {
    const sfx = ns.inBlock ? '-block' : '';
    const el = (index.byStart.get(ns.tag) ?? []).find((e) => e.tagName === 'noscript' && !isForeign(e));
    if (el === undefined) {
      out.push({ kind: `noscript-missing${sfx}`, detail: `noscript at ${ns.tag} has no element` });
      continue;
    }
    // Known class: with scripting disabled a start tag of C in PCLOSE closes a p that is open
    // around the noscript, and the noscript element with it (the shipped rules accept this
    // outside blocks, markup.md section 5 item 2).
    const pClosed = ns.pAround;
    // Known class: with scripting disabled, "in head noscript" leaves the head at the first tag
    // of C other than link, meta, style, noframes, basefont, bgsound (markup.md M-11 item 9 note,
    // H-72); the noscript element and C go to the body. The shipped rules accept this.
    if (el.parentNode?.tagName === 'head' && !ns.inBlock) {
      const end = index.endOf.get(el) ?? null;
      const tagsInside = ns.tags.every(([at]) => at >= ns.end || (index.byStart.get(at) ?? []).some((e) => isInside(index, e, el)));
      if (end !== ns.end || !tagsInside) out.push({ kind: 'known-head-noscript', detail: `noscript at ${ns.tag} in head` });
      continue;
    }
    if (ns.end < length) {
      const end = index.endOf.get(el) ?? null;
      if (end !== ns.end) out.push({ kind: pClosed ? 'known-p-closure' : `noscript-end${sfx}`, detail: `noscript at ${ns.tag}: end tag ${ns.end}, tree ${end}` });
    }
    for (const [at, foreign, name, parentAt, pOpen] of ns.tags) {
      if (at >= ns.end) continue;
      const els = index.byStart.get(at) ?? [];
      const inside = els.find((e) => isInside(index, e, el));
      if (inside === undefined) {
        out.push({ kind: pOpen || pClosed ? 'known-p-closure' : `noscript-outside${sfx}`, detail: `<${name}> at ${at} is not inside noscript at ${ns.tag}` });
        continue;
      }
      const wantForeign = foreign || name === 'svg' || name === 'math';
      if (isForeign(inside) !== wantForeign) out.push({ kind: `noscript-namespace${sfx}`, detail: `<${name}> at ${at}` });
      const p = effectiveParent(index, inside, el, ns.start, ns.end);
      if (p !== parentAt) out.push({ kind: pOpen ? 'known-p-closure' : `noscript-parent${sfx}`, detail: `<${name}> at ${at}: C parent ${parentAt}, tree ${p}` });
    }
  }
  return out;
}

/** The context element <name> of a fragment parse (html > body > element). */
export function contextElement(name) {
  return parse(`<${name}>`).childNodes[0].childNodes[1].childNodes[0];
}

/**
 * Parse html content as a fragment with the block element as the context.
 * @returns {ReturnType<typeof indexTree> & { root: object }}
 */
export function fragmentIndex(content, blockName, scripting, base) {
  const decoded = decodeUtf8(content);
  if (decoded === null) return null;
  const map = utf16ToUtf8Map(decoded.text, 0);
  const frag = parseFragment(contextElement(blockName), decoded.text, { sourceCodeLocationInfo: true, scriptingEnabled: scripting });
  return { ...indexTree(frag, map, base), root: frag };
}

/** innerHTML model: each html block content as a fragment (scripting enabled). */
export function fragmentFindings(bytes, result) {
  const out = [];
  const { trace } = result;
  for (const [i, block] of result.blocks.entries()) {
    const name = trace.blockNames[i];
    if (block.format !== 'html' || name === 'title') continue;
    const idx = fragmentIndex(Buffer.from(bytes.subarray(block.start, block.end)), name, true, block.start);
    if (idx === null) continue;
    const want = trace.content.filter(([at]) => at >= block.start && at < block.end);
    // The same elements: the fragment has an element for each start tag of the content and no
    // element from another start tag. A formatting element that the tree builder reconstructs
    // has the start tag of its original, so start tag offsets are compared, not element counts.
    const wantAt = new Set(want.map(([at]) => at));
    const extra = [...idx.byStart.keys()].filter((at) => !wantAt.has(at));
    if (extra.length > 0) out.push({ kind: 'fragment-extra', detail: `block ${block.id}: elements at ${extra.slice(0, 3).join(',')} are no start tag of the content` });
    const tag = trace.blockTags[i];
    for (const [at, foreign, , tname, parentAt, inTemplate, pOpen] of want) {
      const sfx = inTemplate ? '-tpl' : '';
      const el = (idx.byStart.get(at) ?? [])[0];
      if (el === undefined) {
        out.push({ kind: `fragment-missing${sfx}`, detail: `block ${block.id}: <${tname}> at ${at}` });
        continue;
      }
      const wantForeign = foreign || tname === 'svg' || tname === 'math';
      if (isForeign(el) !== wantForeign) out.push({ kind: `fragment-namespace${sfx}`, detail: `block ${block.id}: <${tname}> at ${at}` });
      // The fragment root stands for the block element (its start tag offset).
      let p = null;
      for (let n = idx.parentOf(el); n; n = idx.parentOf(n)) {
        if (n === idx.root) {
          p = tag;
          break;
        }
        if (!isElement(n)) continue;
        const s = idx.startOf.get(n);
        if (s >= block.start && s < block.end) {
          p = s;
          break;
        }
      }
      if (p !== parentAt) out.push({ kind: pOpen ? 'known-p-closure' : `fragment-parent${sfx}`, detail: `block ${block.id}: <${tname}> at ${at}: token parent ${parentAt}, fragment ${p}` });
    }
  }
  return out;
}

/** The parse5 views, trees and depth analyses of one document, computed once and on demand. */
export class Oracle {
  /** @param {Uint8Array} bytes */
  constructor(bytes) {
    this.bytes = bytes;
    this.views = new Map();
    this.trees = new Map();
    this.depths = new Map();
  }

  view(v) {
    if (!this.views.has(v)) {
      const make = () => parse5View(this.bytes, { scripting: v !== 'disabled' });
      this.views.set(v, v === 'spec' ? withSpecTableScope(make) : make());
    }
    return this.views.get(v);
  }

  tree(v) {
    if (!this.trees.has(v)) {
      const make = () => treeIndex(this.bytes, v !== 'disabled');
      this.trees.set(v, v === 'spec' ? withSpecTableScope(make) : make());
    }
    return this.trees.get(v);
  }

  depth(v) {
    if (!this.depths.has(v)) this.depths.set(v, analyzeDepthS5(this.bytes, { scripting: v === 'enabled' }));
    return this.depths.get(v);
  }

  /**
   * Findings for a valid traced result (an invalid result has none: the rules are
   * stricter than the tree on purpose).
   * @returns {{ kind: string, detail: string }[]}
   */
  check(result) {
    if (!result.valid) return [];
    const out = [];
    for (const v of VIEWS) {
      const c = compareWithView(result, this.view(v));
      if (c.status === 'disagree') out.push({ kind: `parse5-${v}-${c.categories.join('+')}`, detail: c.details[0] ?? '' });
      const index = this.tree(v);
      if (index !== null) {
        out.push(...contentFindings(index, result, v));
        if (v === 'disabled') out.push(...noscriptFindings(index, result.trace, this.bytes.length));
      }
      if (v === 'spec') continue;
      const d = this.depth(v);
      if (d.violations.length > 0) out.push({ kind: `depth-${v}`, detail: d.violations[0] });
      if (d.templateViolations.length > 0) out.push({ kind: `depth-template-${v}`, detail: d.templateViolations[0] });
    }
    out.push(...fragmentFindings(this.bytes, result));
    return out;
  }
}

/** True for the kinds that are not findings (known classes and the informative template depth). */
export function isInformative(kind) {
  return kind.startsWith('known-') || kind.startsWith('depth-template-');
}
