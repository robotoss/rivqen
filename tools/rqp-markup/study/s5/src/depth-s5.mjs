// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// The depth claim of markup.md M-26 for the S5 study: a copy of analyzeDepth of
// ../../../crosscheck/src/depth.mjs (the counter DepthCounter and the pass of M-11
// item 8 are imported unchanged) with one addition. The claim of M-26 holds "after
// each token at which no template element is open" (H-35): the cross-check skips
// these positions. With `template` in block content (S5), the study also checks the
// same three bounds while a template is open and reports them apart:
//
//   violations          the claim of M-26 as written (no template open)
//   templateViolations  the same bounds while a template is open (inside the inert
//                       template contents; not part of the claim today)
//
// DOM depth counts through the template element (the contents fragment has the
// template as its host). Test oracle only; it does not decide validity.

import { Parser, defaultTreeAdapter } from '../../../crosscheck/node_modules/parse5/dist/index.js';
import { CLASSES, DepthCounter, checkNoscriptContent } from '../../../crosscheck/src/depth.mjs';
import { decodeUtf8 } from '../../../crosscheck/src/utf8.mjs';
import { HTML_NS, MAX_INPUT_BYTES } from '../../../crosscheck/src/view.mjs';

const MAX_REPORTED = 5;

function isElement(node) {
  return node !== null && typeof node === 'object' && typeof node.tagName === 'string';
}

/**
 * @param {Uint8Array} bytes
 * @param {{ scripting?: boolean }} [options]
 * @returns {{ ok: boolean, maxD: number, maxStack: number, violations: string[], templateViolations: string[], templatePositions: number }}
 */
export function analyzeDepthS5(bytes, { scripting = true } = {}) {
  const empty = { ok: false, maxD: 0, maxStack: 0, violations: [], templateViolations: [], templatePositions: 0 };
  if (bytes.length > MAX_INPUT_BYTES) return empty;
  const decoded = decodeUtf8(bytes);
  if (decoded === null) return empty;
  const text = decoded.text;

  let counter = new DepthCounter();
  let saved = null;
  const violations = [];
  const templateViolations = [];
  let templatePositions = 0;
  const hosts = new WeakMap();
  const attached = [];
  let maxStack = 0;
  let noscriptStart = -1;
  const seen = new WeakSet();

  const adapter = {
    ...defaultTreeAdapter,
    appendChild(parent, child) {
      defaultTreeAdapter.appendChild(parent, child);
      if (isElement(child)) attached.push(child);
    },
    insertBefore(parent, child, ref) {
      defaultTreeAdapter.insertBefore(parent, child, ref);
      if (isElement(child)) attached.push(child);
    },
    setTemplateContent(template, content) {
      defaultTreeAdapter.setTemplateContent(template, content);
      hosts.set(content, template);
    },
  };

  class DepthParser extends Parser {
    #after(what, offset) {
      const items = this.openElements.items;
      const size = this.openElements.stackTop + 1;
      if (size > maxStack) maxStack = size;
      const d = counter.d;
      const at = `${what} at ${offset ?? '?'}`;
      let inTemplate = false;
      for (let k = 0; k < size; k++) {
        if (items[k].tagName === 'template' && items[k].namespaceURI === HTML_NS) {
          inTemplate = true;
          break;
        }
      }
      if (inTemplate) templatePositions++;
      const out = inTemplate ? templateViolations : violations;
      const report = (m) => {
        if (out.length < MAX_REPORTED) out.push(m);
      };
      if (size > d + 2) report(`stack ${size} > D ${d} + 2 after ${at}`);
      const memo = new Map();
      const depth = (node) => {
        const chain = [];
        let n = node;
        let base = 0;
        while (n) {
          if (memo.has(n)) {
            base = memo.get(n);
            break;
          }
          if (isElement(n)) chain.push(n);
          else if (!hosts.has(n)) break;
          n = isElement(n) ? n.parentNode : hosts.get(n);
        }
        for (let k = chain.length - 1; k >= 0; k--) memo.set(chain[k], ++base);
        return memo.get(node) ?? 0;
      };
      const open = new Set();
      for (let k = 0; k < size; k++) open.add(items[k]);
      for (let k = 0; k < size; k++) {
        const dep = depth(items[k]);
        if (dep > d + 2) report(`open <${items[k].tagName}> depth ${dep} > D ${d} + 2 after ${at}`);
      }
      for (const e of attached) {
        const dep = depth(e);
        const limit = open.has(e) ? d + 2 : d + 3;
        if (dep > limit) report(`<${e.tagName}> depth ${dep} > D ${d} + ${limit - d} after ${at}`);
      }
      attached.length = 0;
    }

    onStartTag(token) {
      const htmlContext = !counter.inForeign;
      counter.startTag(token.tagName, token.selfClosing, (token.attrs ?? []).map((a) => a.name));
      if (htmlContext && token.tagName === 'noscript' && token.location && noscriptStart < 0) {
        noscriptStart = token.location.endOffset;
        if (!scripting) {
          saved = counter.clone();
          saved.maxD = counter.maxD;
          counter.closure = false;
        }
      }
      super.onStartTag(token);
      this.#after(`<${token.tagName}>`, token.location?.startOffset);
    }

    onEndTag(token) {
      if (!seen.has(token)) {
        seen.add(token);
        if (token.tagName === 'noscript' && noscriptStart >= 0 && token.location) {
          this.#noscript(text.slice(noscriptStart, token.location.startOffset));
          if (saved !== null) {
            saved.maxD = Math.max(saved.maxD, counter.maxD);
            counter = saved;
            saved = null;
          }
        }
        counter.endTag(token.tagName);
      }
      super.onEndTag(token);
      this.#after(`</${token.tagName}>`, token.location?.startOffset);
    }

    #noscript(c) {
      noscriptStart = -1;
      const main = saved ?? counter;
      const r = checkNoscriptContent(c, main);
      if (!r.balanced) counter.classes.add(CLASSES.noscriptBalance);
      if (r.maxD > main.maxD) main.maxD = r.maxD;
    }

    onCharacter(token) {
      super.onCharacter(token);
      this.#after('text', token.location?.startOffset);
    }

    onNullCharacter(token) {
      super.onNullCharacter(token);
      this.#after('text', token.location?.startOffset);
    }

    onWhitespaceCharacter(token) {
      super.onWhitespaceCharacter(token);
      this.#after('text', token.location?.startOffset);
    }

    onComment(token) {
      super.onComment(token);
      this.#after('comment', token.location?.startOffset);
    }

    onEof(token) {
      if (noscriptStart >= 0) this.#noscript(text.slice(noscriptStart));
      super.onEof(token);
      this.#after('EOF', token.location?.startOffset);
    }
  }

  DepthParser.parse(text, { treeAdapter: adapter, scriptingEnabled: scripting, sourceCodeLocationInfo: true });
  const maxD = Math.max(counter.maxD, saved?.maxD ?? 0);
  return { ok: true, maxD, maxStack, violations, templateViolations, templatePositions };
}
