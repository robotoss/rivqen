// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// The html5lib tree-construction format of a parse5 tree. This is independent of host/serialize.js
// (which walks a browser DOM); a test checks that both give the same string for the same tree.

const SVG_NS = "http://www.w3.org/2000/svg";
const MATH_NS = "http://www.w3.org/1998/Math/MathML";

/** @param {number} depth */
function pad(depth) {
  return `| ${"  ".repeat(depth)}`;
}

/**
 * @param {{ prefix?: string, name: string, value: string }} a
 * @returns {string}
 */
function attrName(a) {
  return a.prefix ? `${a.prefix} ${a.name}` : a.name;
}

/**
 * Serialize a parse5 document (or fragment) in the html5lib format.
 * Iterative: the tree depth is bounded only by the input size.
 * @param {any} doc
 * @returns {string}
 */
export function serializeParse5(doc) {
  /** @type {string[]} */
  const lines = [];
  /** @type {{ node: any, depth: number }[]} */
  const stack = [];
  const push = (/** @type {any[]} */ list, /** @type {number} */ depth) => {
    for (let i = list.length - 1; i >= 0; i--) stack.push({ node: list[i], depth });
  };
  push(doc.childNodes, 0);
  while (stack.length > 0) {
    const { node, depth } = /** @type {{ node: any, depth: number }} */ (stack.pop());
    if (node.nodeName === "#text") {
      lines.push(`${pad(depth)}"${node.value}"`);
    } else if (node.nodeName === "#comment") {
      lines.push(`${pad(depth)}<!-- ${node.data} -->`);
    } else if (node.nodeName === "#documentType") {
      const pub = node.publicId ?? "";
      const sys = node.systemId ?? "";
      const ids = pub !== "" || sys !== "" ? ` "${pub}" "${sys}"` : "";
      lines.push(`${pad(depth)}<!DOCTYPE ${node.name ?? ""}${ids}>`);
    } else if (typeof node.tagName === "string") {
      const ns = node.namespaceURI === SVG_NS ? "svg " : node.namespaceURI === MATH_NS ? "math " : "";
      lines.push(`${pad(depth)}<${ns}${node.tagName}>`);
      const attrs = node.attrs.map((/** @type {any} */ a) => ({ n: attrName(a), v: a.value }));
      attrs.sort((/** @type {any} */ a, /** @type {any} */ b) => (a.n < b.n ? -1 : a.n > b.n ? 1 : 0));
      for (const a of attrs) lines.push(`${pad(depth + 1)}${a.n}="${a.v}"`);
      if (node.content) {
        lines.push(`${pad(depth + 1)}content`);
        push(node.childNodes, depth + 1);
        push(node.content.childNodes, depth + 2);
      } else {
        push(node.childNodes, depth + 1);
      }
    } else {
      lines.push(`${pad(depth)}?nodeName=${String(node.nodeName)}`);
    }
  }
  return lines.join("\n");
}
