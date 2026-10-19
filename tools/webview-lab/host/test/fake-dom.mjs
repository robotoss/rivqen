// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// A fake DOM built from parse5 nodes. It has only the properties that host/serialize.js reads
// (README, "What serialize.js reads"): nodeType, nodeName, localName, namespaceURI, attributes
// (name, localName, prefix, value), childNodes, content, data, name, publicId, systemId.

import { readFileSync } from "node:fs";
import { join } from "node:path";
import { runInNewContext } from "node:vm";
import { HOST_DIR } from "../lab.mjs";

/** @param {any} p parse5 node @returns {object} */
export function fakeNode(p) {
  if (p.nodeName === "#text") return { nodeType: 3, nodeName: "#text", data: p.value };
  if (p.nodeName === "#comment") return { nodeType: 8, nodeName: "#comment", data: p.data };
  if (p.nodeName === "#documentType") {
    return { nodeType: 10, nodeName: p.name, name: p.name, publicId: p.publicId, systemId: p.systemId };
  }
  const el = {
    nodeType: 1,
    nodeName: p.namespaceURI === "http://www.w3.org/1999/xhtml" ? p.tagName.toUpperCase() : p.tagName,
    localName: p.tagName,
    namespaceURI: p.namespaceURI,
    attributes: p.attrs.map((/** @type {any} */ a) => ({
      name: a.prefix ? `${a.prefix}:${a.name}` : a.name,
      localName: a.name,
      prefix: a.prefix ?? null,
      value: a.value,
    })),
    // Lazy, so that a very deep tree does not need a deep recursion here.
    get childNodes() {
      return p.childNodes.map(fakeNode);
    },
  };
  if (p.content) {
    Object.defineProperty(el, "content", {
      get: () => ({ nodeType: 11, childNodes: p.content.childNodes.map(fakeNode) }),
    });
  }
  return el;
}

/** @param {any} doc parse5 document @returns {object} */
export function fakeDocument(doc) {
  return { nodeType: 9, childNodes: doc.childNodes.map(fakeNode) };
}

const SOURCE = readFileSync(join(HOST_DIR, "serialize.js"), "utf8");

/** Run serialize.js (a plain script) with `document` set. @param {object} document @returns {string} */
export function runSerialize(document) {
  const value = runInNewContext(SOURCE, { document });
  if (typeof value !== "string") throw new Error("serialize.js did not return a string");
  return value;
}
