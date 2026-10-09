// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// The parse5 view of a document: the elements with a data-rq-block attribute in the
// WHATWG tree, with the byte offsets of their content, and the manifest-like scripts.
// parse5 8.0.1 implements the WHATWG tree builder; it is the reference, not a candidate.

import { parse } from "parse5";
import { decodeUtf8, utf16ToUtf8Map } from "./utf8.mjs";

export const HTML_NS = "http://www.w3.org/1999/xhtml";
export const MAX_INPUT_BYTES = 5 * 1024 * 1024;

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
 * @param {{ scripting?: boolean }} [options] scripting: the parse5 option scriptingEnabled (default true)
 * @returns {View}
 */
export function parse5View(bytes, { scripting = true } = {}) {
  if (bytes.length > MAX_INPUT_BYTES) return { ok: false, reason: "input larger than 5 MiB", blocks: [], manifests: 0 };
  const decoded = decodeUtf8(bytes);
  if (decoded === null) return { ok: false, reason: "input is not valid UTF-8", blocks: [], manifests: 0 };
  const map = utf16ToUtf8Map(decoded.text, decoded.bomBytes);
  const doc = parse(decoded.text, { sourceCodeLocationInfo: true, scriptingEnabled: scripting });

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
      if (node.tagName === "script" && html && type !== null && type.toLowerCase().includes("rivqen-manifest")) {
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

/** @param {object[]} stack @param {object[]} children */
function pushChildren(stack, children) {
  for (let i = children.length - 1; i >= 0; i--) stack.push(children[i]);
}

/** @param {{ name: string, value: string, prefix?: string }[]} attrs @param {string} name */
function attrValue(attrs, name) {
  const a = attrs.find((x) => x.name === name && !x.prefix);
  return a ? a.value : null;
}
