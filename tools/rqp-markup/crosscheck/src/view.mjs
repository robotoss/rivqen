// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// The parse5 view of a document: the elements with a data-rq-block attribute in the
// WHATWG tree, with the byte offsets of their content, and the manifest-like scripts.
// parse5 8.0.1 implements the WHATWG tree builder; it is the reference, not a candidate.

import { Parser, html, parse } from "parse5";
import { decodeUtf8, utf16ToUtf8Map } from "./utf8.mjs";

export const HTML_NS = "http://www.w3.org/1999/xhtml";
export const MAX_INPUT_BYTES = 5 * 1024 * 1024;

/**
 * Run `fn` with the WHATWG definition of "has an element in table scope": the walk stops at an
 * HTML `html`, `table` or `template` element (WHATWG HTML 13.2.4.2). parse5 8.0.1 stops only at
 * `html` and `table` (`hasInTableScope` and `hasTableBodyContextInTableScope` of its stack of open
 * elements), so in template contents in a table insertion mode it pops out of the template where a
 * WHATWG parser ignores the token (evidence/markup-s5/report.md section 5). The patch replaces the
 * two prototype functions for the call and restores them after it; parse5 is synchronous, so no
 * other parse sees the patch.
 *
 * @template T
 * @param {() => T} fn
 * @returns {T}
 */
export function withSpecTableScope(fn) {
  const proto = Object.getPrototypeOf(new Parser().openElements);
  const saved = [proto.hasInTableScope, proto.hasTableBodyContextInTableScope];
  const { TAG_ID } = html;
  const boundary = (id) => id === TAG_ID.TABLE || id === TAG_ID.HTML || id === TAG_ID.TEMPLATE;
  proto.hasInTableScope = function hasInTableScope(tagId) {
    for (let i = this.stackTop; i >= 0; i--) {
      if (this.treeAdapter.getNamespaceURI(this.items[i]) !== HTML_NS) continue;
      if (this.tagIDs[i] === tagId) return true;
      if (boundary(this.tagIDs[i])) return false;
    }
    return true;
  };
  proto.hasTableBodyContextInTableScope = function hasTableBodyContextInTableScope() {
    for (let i = this.stackTop; i >= 0; i--) {
      if (this.treeAdapter.getNamespaceURI(this.items[i]) !== HTML_NS) continue;
      const id = this.tagIDs[i];
      if (id === TAG_ID.TBODY || id === TAG_ID.THEAD || id === TAG_ID.TFOOT) return true;
      if (boundary(id)) return false;
    }
    return true;
  };
  try {
    return fn();
  } finally {
    [proto.hasInTableScope, proto.hasTableBodyContextInTableScope] = saved;
  }
}

/**
 * @typedef {object} ViewBlock
 * @property {string} id      decoded attribute value (parse5 decodes character references)
 * @property {string} tag     tag name
 * @property {boolean} html   true when the element is in the HTML namespace
 * @property {number|null} start  byte offset after the start tag, null when parse5 has no start tag position
 * @property {number|null} end    byte offset of the end tag, null when the end tag is implied or missing
 * @property {string|null} type   raw `type` attribute (decoded by parse5), for json blocks
 *
 * @typedef {object} View
 * @property {boolean} ok
 * @property {string} [reason]   why there is no view (encoding, size)
 * @property {ViewBlock[]} blocks  in order of `start` (elements without a position last)
 * @property {number} manifests    script elements whose type contains "rivqen-manifest"
 */

/**
 * Parse the input with parse5 and collect the block elements.
 *
 * @param {Uint8Array} bytes
 * @param {{ scripting?: boolean, specTableScope?: boolean }} [options] scripting: the parse5 option
 *   scriptingEnabled (default true); specTableScope: parse with the WHATWG table scope
 *   (withSpecTableScope; default false)
 * @returns {View}
 */
export function parse5View(bytes, { scripting = true, specTableScope = false } = {}) {
  if (bytes.length > MAX_INPUT_BYTES) return { ok: false, reason: "input larger than 5 MiB", blocks: [], manifests: 0 };
  const decoded = decodeUtf8(bytes);
  if (decoded === null) return { ok: false, reason: "input is not valid UTF-8", blocks: [], manifests: 0 };
  const map = utf16ToUtf8Map(decoded.text, decoded.bomBytes);
  const run = () => parse(decoded.text, { sourceCodeLocationInfo: true, scriptingEnabled: scripting });
  const doc = specTableScope ? withSpecTableScope(run) : run();

  /** @type {ViewBlock[]} */
  const blocks = [];
  let manifests = 0;
  // Iterative walk: the tree depth is bounded only by the input size.
  /** @type {object[]} parse5 nodes */
  const stack = [doc];
  while (stack.length > 0) {
    const node = stack.pop();
    if (typeof node.tagName === "string") {
      const attrs = Array.isArray(node.attrs) ? node.attrs : [];
      const html = node.namespaceURI === HTML_NS;
      const type = attrValue(attrs, "type");
      // Every namespace: M-13 applies in every context, and a CSS selector script[type] also matches an SVG script.
      if (node.tagName === "script" && type !== null && looksLikeManifest(type)) {
        manifests++;
      }
      const id = attrValue(attrs, "data-rq-block");
      if (id !== null) {
        const loc = node.sourceCodeLocation;
        const startTag = loc?.startTag;
        const endTag = loc?.endTag;
        blocks.push({
          id,
          tag: node.tagName,
          html,
          start: startTag ? map[startTag.endOffset] : null,
          end: endTag ? map[endTag.startOffset] : null,
          type,
        });
      }
    }
    if (node.content && Array.isArray(node.content.childNodes)) pushChildren(stack, node.content.childNodes);
    if (Array.isArray(node.childNodes)) pushChildren(stack, node.childNodes);
  }
  blocks.sort((a, b) => (a.start ?? Infinity) - (b.start ?? Infinity));
  return { ok: true, blocks, manifests };
}

/**
 * A script type that a client could take for the manifest type: ASCII case-insensitive, and also
 * after Unicode case mapping (dotless i and long s map to ASCII i and s in upper case), because
 * markup.md M-13 must hold for a client that compares with Unicode case folding (H-28).
 * @param {string} type decoded attribute value
 */
export function looksLikeManifest(type) {
  const needle = "rivqen-manifest";
  return type.toLowerCase().includes(needle) || type.toUpperCase().toLowerCase().includes(needle);
}

/** @param {object[]} stack @param {object[]} children */
function pushChildren(stack, children) {
  for (let i = children.length - 1; i >= 0; i--) stack.push(children[i]);
}

/** @param {{ name: string, value: string, prefix?: string }[]} attrs @param {string} name */
function attrValue(attrs, name) {
  const a = attrs.find((x) => x.name === name && !x.prefix);
  return a ? a.value : null;
}
