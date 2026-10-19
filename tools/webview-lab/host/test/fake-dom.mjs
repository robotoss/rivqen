// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// A fake DOM built from parse5 nodes. As in a browser, the values are behind getters on prototypes
// (Node, Element, CharacterData, Attr, DocumentType, HTMLTemplateElement), so the tests use the
// same read path as a real WebView. host/serialize.js reads: nodeType, nodeName, localName,
// namespaceURI, attributes (name, localName, prefix, value), childNodes, content, data, name,
// publicId, systemId.

import { readFileSync } from "node:fs";
import { join } from "node:path";
import { runInNewContext } from "node:vm";
import { HOST_DIR } from "../lab.mjs";

const HTML_NS = "http://www.w3.org/1999/xhtml";

/** Internal state of the fake objects; the getters read it, as a browser reads its internal slots. */
const slots = new WeakMap();
/** @param {object} o */
const slot = (o) => slots.get(o);

export class Node {
  get nodeType() {
    return slot(this).nodeType;
  }
  get nodeName() {
    return slot(this).nodeName;
  }
  get childNodes() {
    return slot(this).children();
  }
}
export class CharacterData extends Node {
  get data() {
    return slot(this).data;
  }
}
export class DocumentType extends Node {
  get name() {
    return slot(this).name;
  }
  get publicId() {
    return slot(this).publicId;
  }
  get systemId() {
    return slot(this).systemId;
  }
}
export class Element extends Node {
  get localName() {
    return slot(this).localName;
  }
  get namespaceURI() {
    return slot(this).namespaceURI;
  }
  get attributes() {
    return slot(this).attributes;
  }
}
export class HTMLTemplateElement extends Element {
  get content() {
    return slot(this).content;
  }
}
export class Attr {
  get name() {
    return slot(this).name;
  }
  get localName() {
    return slot(this).localName;
  }
  get prefix() {
    return slot(this).prefix;
  }
  get value() {
    return slot(this).value;
  }
}

/** The globals a WebView has and serialize.js looks up. */
export const DOM_GLOBALS = { Node, CharacterData, DocumentType, Element, HTMLTemplateElement, Attr };

/** @param {any} Class @param {object} state */
function make(Class, state) {
  const o = new Class();
  if (typeof state.children === "function") {
    // Computed once, so that a test can reach the same child objects the walker will read.
    const build = state.children;
    let cached;
    state.children = () => (cached ??= build());
  }
  slots.set(o, state);
  return o;
}

/** @param {any} p parse5 node @returns {object} */
export function fakeNode(p) {
  if (p.nodeName === "#text") return make(CharacterData, { nodeType: 3, nodeName: "#text", data: p.value, children: () => [] });
  if (p.nodeName === "#comment") return make(CharacterData, { nodeType: 8, nodeName: "#comment", data: p.data, children: () => [] });
  if (p.nodeName === "#documentType") {
    return make(DocumentType, { nodeType: 10, nodeName: p.name, name: p.name, publicId: p.publicId, systemId: p.systemId, children: () => [] });
  }
  const state = {
    nodeType: 1,
    nodeName: p.namespaceURI === HTML_NS ? p.tagName.toUpperCase() : p.tagName,
    localName: p.tagName,
    namespaceURI: p.namespaceURI,
    attributes: p.attrs.map((/** @type {any} */ a) =>
      make(Attr, { name: a.prefix ? `${a.prefix}:${a.name}` : a.name, localName: a.name, prefix: a.prefix ?? null, value: a.value }),
    ),
    // Lazy, so that a very deep tree does not need a deep recursion here.
    children: () => p.childNodes.map(fakeNode),
    content: null,
  };
  if (p.content) {
    state.content = make(Node, { nodeType: 11, nodeName: "#document-fragment", children: () => p.content.childNodes.map(fakeNode) });
  }
  const el = make(p.content ? HTMLTemplateElement : Element, state);
  // A real DOM reflects the content attribute of meta as a string property "content".
  if (p.tagName === "meta" && p.namespaceURI === HTML_NS) {
    Object.defineProperty(el, "content", { value: p.attrs.find((/** @type {any} */ a) => a.name === "content" && !a.prefix)?.value ?? "" });
  }
  return el;
}

/** @param {any} doc parse5 document @returns {object} */
export function fakeDocument(doc) {
  return make(Node, { nodeType: 9, nodeName: "#document", children: () => doc.childNodes.map(fakeNode) });
}

const SOURCE = readFileSync(join(HOST_DIR, "serialize.js"), "utf8");

/**
 * Run serialize.js (a plain script) with `document` set. `globals` are the DOM constructors of the
 * context; pass {} for a context without prototypes (the plain-property fallback).
 * @param {object} document @param {object} [globals] @returns {string}
 */
export function runSerialize(document, globals = DOM_GLOBALS) {
  const value = runInNewContext(SOURCE, { document, ...globals });
  if (typeof value !== "string") throw new Error("serialize.js did not return a string");
  return value;
}
