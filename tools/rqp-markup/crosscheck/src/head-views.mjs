// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// parse5 8.0.1 checks of the manifest insertion point (H-72, H-79a; a copy of the S4 study
// `../study/s4/src/head-views.mjs`).
// For an original document O and an insertion point `at`, the full document F has
// the manifest element at `at`. In one scripting view the check reports:
//
//   first     the manifest element is the first child node of the head element of F
//   only      F has exactly one manifest-type script element (template contents included)
//   same      the tree of F without the manifest element serializes to the tree of O
//             (so no head content, no body start and no attribute of html, head or body
//             moved: the page's next tag stays where the parser put it in O)
//
// The checks never return document text: only booleans and tag names.

import { parse, serialize } from 'parse5';
import { decodeUtf8 } from './utf8.mjs';
import { MAX_INPUT_BYTES } from './view.mjs';
import { MANIFEST_TYPE } from '../../../rqp-protocol/src/common.mjs';

export const VIEWS = Object.freeze(['enabled', 'disabled']);

function parseText(bytes, scripting) {
  if (bytes.length > MAX_INPUT_BYTES) return null;
  const decoded = decodeUtf8(bytes);
  if (decoded === null) return null;
  return parse(decoded.text, { scriptingEnabled: scripting });
}

const isManifest = (n) =>
  n.tagName === 'script' && Array.isArray(n.attrs) && n.attrs.some((a) => a.name === 'type' && a.value.toLowerCase() === MANIFEST_TYPE);

function childElement(node, name) {
  if (node === undefined || !Array.isArray(node.childNodes)) return undefined;
  return node.childNodes.find((n) => n.tagName === name);
}

/** All manifest elements of a tree (iterative walk; template contents included). */
function manifests(doc) {
  const out = [];
  const stack = [doc];
  while (stack.length > 0) {
    const n = stack.pop();
    if (typeof n.tagName === 'string' && isManifest(n)) out.push(n);
    if (n.content && Array.isArray(n.content.childNodes)) for (const c of n.content.childNodes) stack.push(c);
    if (Array.isArray(n.childNodes)) for (const c of n.childNodes) stack.push(c);
  }
  return out;
}

/**
 * One view of F against O.
 * @param {Uint8Array} original O
 * @param {Uint8Array} full F
 * @param {boolean} scripting
 * @returns {{ ok: boolean, first: boolean, only: boolean, same: boolean, parent: string | null }}
 */
export function checkView(original, full, scripting) {
  const docF = parseText(full, scripting);
  const docO = parseText(original, scripting);
  if (docF === null || docO === null) return { ok: false, first: false, only: false, same: false, parent: null };
  const head = childElement(childElement(docF, 'html'), 'head');
  const found = manifests(docF);
  const first = head !== undefined && head.childNodes.length > 0 && isManifest(head.childNodes[0]);
  const parent = found.length > 0 ? (found[0].parentNode?.tagName ?? '#document') : null;
  // Remove every manifest element of F, then compare with O (O has none: M-13).
  for (const m of found) {
    const p = m.parentNode;
    p.childNodes.splice(p.childNodes.indexOf(m), 1);
  }
  const same = serialize(docF) === serialize(docO) && doctypeOf(docF) === doctypeOf(docO) && docF.mode === docO.mode;
  return { ok: true, first, only: found.length === 1, same, parent };
}

/** The DOCTYPE node of a document as a string (serialize writes only its name). */
function doctypeOf(doc) {
  const d = doc.childNodes.find((n) => n.nodeName === '#documentType');
  return d === undefined ? '' : `${d.name}|${d.publicId}|${d.systemId}`;
}

/**
 * The given views (default both). Returns { ok, pass, failures } where failures lists "<view>-<check>"
 * for each check that failed.
 */
export function checkViews(original, full, views = VIEWS) {
  const failures = [];
  let ok = true;
  for (const v of views) {
    const r = checkView(original, full, v === 'enabled');
    if (!r.ok) {
      ok = false;
      continue;
    }
    if (!r.first) failures.push(`${v}-first`);
    if (!r.only) failures.push(`${v}-only`);
    if (!r.same) failures.push(`${v}-same`);
  }
  return { ok, pass: ok && failures.length === 0, failures };
}
