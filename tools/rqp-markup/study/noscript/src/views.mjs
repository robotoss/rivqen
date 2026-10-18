// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// parse5 8.0.1 views for the head study (WP-17 S3 T-02): where the head element of
// the document ends, in the scripting-enabled and the scripting-disabled view, and
// where a manifest element inserted before the RQP head end tag lands. The block
// views come from ../../../crosscheck/src/view.mjs (parse5View).

import { parse } from '../../../crosscheck/node_modules/parse5/dist/index.js';
import { decodeUtf8, utf16ToUtf8Map } from '../../../crosscheck/src/utf8.mjs';
import { MAX_INPUT_BYTES } from '../../../crosscheck/src/view.mjs';

export const MANIFEST_ELEMENT = '<script type="application/rivqen-manifest+json">{}</script>';
const MANIFEST_TYPE = 'application/rivqen-manifest+json';

function parseDoc(bytes, scripting) {
  if (bytes.length > MAX_INPUT_BYTES) return null;
  const decoded = decodeUtf8(bytes);
  if (decoded === null) return null;
  const doc = parse(decoded.text, { sourceCodeLocationInfo: true, scriptingEnabled: scripting });
  return { doc, map: utf16ToUtf8Map(decoded.text, decoded.bomBytes) };
}

function childElement(node, name) {
  if (node === undefined || !Array.isArray(node.childNodes)) return undefined;
  return node.childNodes.find((n) => n.tagName === name);
}

/**
 * The head element of the document in one view. `end` is the byte offset of the
 * "<" of its end tag, or -1 when parse5 closed it without an end tag (implied);
 * `start` is -1 when it has no start tag (implied head).
 *
 * @param {Uint8Array} bytes
 * @param {boolean} scripting
 * @returns {{ ok: boolean, start: number, end: number }}
 */
export function headView(bytes, scripting) {
  const p = parseDoc(bytes, scripting);
  if (p === null) return { ok: false, start: -1, end: -1 };
  const head = childElement(childElement(p.doc, 'html'), 'head');
  const loc = head?.sourceCodeLocation;
  const start = loc?.startTag ? p.map[loc.startTag.startOffset] : -1;
  const end = loc?.endTag ? p.map[loc.endTag.startOffset] : -1;
  return { ok: true, start, end };
}

/**
 * Insert the manifest element at `at` (the RQP head end tag) and check, in one
 * view, that the manifest script is a child of the document head and that the head
 * ends at the original end tag (manifest-patch section 1.7, server item 2 and client
 * item 3). The parser can add later elements to the head ("after head" sends some
 * head content there), so the manifest is not always the last child.
 *
 * @param {Uint8Array} bytes the original document
 * @param {number} at the RQP head end tag offset (not -1)
 * @param {boolean} scripting
 * @returns {{ ok: boolean, inHead: boolean, headEnd: boolean, manifests: number }}
 */
export function manifestView(bytes, at, scripting) {
  const full = Buffer.concat([bytes.subarray(0, at), Buffer.from(MANIFEST_ELEMENT, 'latin1'), bytes.subarray(at)]);
  const p = parseDoc(full, scripting);
  if (p === null) return { ok: false, inHead: false, headEnd: false, manifests: 0 };
  const head = childElement(childElement(p.doc, 'html'), 'head');
  const isManifest = (n) => n.tagName === 'script' && n.attrs.some((a) => a.name === 'type' && a.value === MANIFEST_TYPE);
  const inHead = head !== undefined && head.childNodes.some(isManifest);
  const endTag = head?.sourceCodeLocation?.endTag;
  const headEnd = endTag !== undefined && p.map[endTag.startOffset] === at + MANIFEST_ELEMENT.length;
  let manifests = 0;
  const stack = [p.doc];
  while (stack.length > 0) {
    const n = stack.pop();
    if (typeof n.tagName === 'string' && isManifest(n)) manifests++;
    // A loop, not a spread: a node can have more children than the argument limit.
    if (n.content && Array.isArray(n.content.childNodes)) for (const c of n.content.childNodes) stack.push(c);
    if (Array.isArray(n.childNodes)) for (const c of n.childNodes) stack.push(c);
  }
  return { ok: true, inHead, headEnd, manifests };
}
