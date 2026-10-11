// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// Reference computation of the depth counter D (markup.md M-26) and a small classifier for the
// rules of the security review (H-26, H-27, H-28, H-30, H-31). Test oracle only: it is not a
// candidate and it does not decide validity.
//
// The pass is driven by the parse5 tokens (a Parser subclass), so it sees the token stream of
// the WHATWG tree builder. For a valid document this is the token stream of markup.md M-03/M-04.
// After each token it checks the invariant of M-26:
//   1. the WHATWG stack of open elements has at most D + 2 elements;
//   2. every element on that stack has a DOM depth of at most D + 2;
//   3. every element that the token created and that is not on the stack after the token
//      (void elements, the empty p of </p>, ...) has a DOM depth of at most D + 3.
// DOM depth = number of element ancestors + 1 (html = 1). Elements in template contents count
// through their template element.

import { Parser, Tokenizer, TokenizerMode, defaultTreeAdapter } from "parse5";
import { decodeUtf8 } from "./utf8.mjs";
import { HTML_NS, MAX_INPUT_BYTES } from "./view.mjs";

export const MAX_D = 256;

const names = (s) => new Set(s.split(/\s+/).filter((x) => x !== ""));

export const VOID = names("area base basefont bgsound br col embed frame hr image img input keygen link meta param source track wbr");
export const FORMATTING = names("a b big code em font i nobr s small strike strong tt u");
// WHATWG HTML 13.2.4.2 "The stack of open elements", the special category, HTML namespace.
export const SPECIAL = names(`address applet area article aside base basefont bgsound blockquote body br button
  caption center col colgroup dd details dir div dl dt embed fieldset figcaption figure footer form frame
  frameset h1 h2 h3 h4 h5 h6 head header hgroup hr html iframe img input keygen li link listing main marquee
  menu meta nav noembed noframes noscript object ol p param plaintext pre script search section select source
  style summary table tbody td template textarea tfoot th thead title tr track ul wbr xmp`);
const TABLE_FAMILY = names("table caption colgroup tbody td tfoot th thead tr");
const TABLE_PART = names("caption colgroup tbody td tfoot th thead tr");
const TABLE_CONTEXT = names("table tbody thead tfoot tr colgroup"); // F values of M-24 item k and H-37
const ROW_GROUPS = names("tbody thead tfoot");
const STRAY_TABLE_END = names("tbody thead tfoot tr table");
const RCDATA = names("title textarea");
const RAWTEXT = names("iframe noembed noframes noscript style xmp");

/** Rule classes that the classifier reports. */
export const CLASSES = {
  formatting: "M-06-formatting", // H-26: a FORMATTING end tag crosses a special or foreign element
  formattingMarked: "M-06-formatting-marked", // H-32: a FORMATTING end tag pops a special-crossed element
  strayTable: "M-05-stray-table-end", // H-30
  noscriptBalance: "M-11-balance", // H-27
  typeNonAscii: "M-13-non-ascii", // H-28
  depth: "M-26-depth", // H-31: D > 256
  templateCol: "M-09-template-col", // H-35: col with template as the nearest table-family-or-template element
  tableCurrent: "M-09-table-current", // H-37: a table part, col or table start tag while F (a table context) is not the current node
  form: "M-10-form", // H-36: a form start tag while an HTML form is on the token stack
  noscriptTablePart: "M-11-table-part", // H-38: a table part or col start tag in C without a table on the C stack
};

/**
 * The depth counter D of markup.md M-26 over the token stack of M-05.
 * Each stack entry: name, foreign (pushed in a foreign region), dirty, marked (special-crossed,
 * markup.md M-06 item 4), implied flags.
 */
export class DepthCounter {
  constructor() {
    /** @type {{ name: string, foreign: boolean, dirty: boolean, marked: boolean, implied: Set<string> }[]} */
    this.stack = [];
    this.d = 0;
    this.maxD = 0;
    this.roots = 0; // svg and math elements pushed in HTML context that are on the stack
    /** @type {Set<string>} */
    this.classes = new Set();
  }

  get inForeign() {
    return this.roots > 0;
  }

  #raise(n) {
    this.d += n;
    if (this.d > this.maxD) this.maxD = this.d;
    if (this.d > MAX_D) this.classes.add(CLASSES.depth);
  }

  /**
   * Nearest TABLE_FAMILY element (M-09); the search stops at a template element.
   * Called in HTML context only, so no element on the stack was pushed in a foreign region.
   */
  #tableContext() {
    for (let i = this.stack.length - 1; i >= 0; i--) {
      const e = this.stack[i];
      if (e.name === "template") return null;
      if (TABLE_FAMILY.has(e.name)) return e;
    }
    return null;
  }

  /** A start tag token (M-05 items 1 and 2). */
  startTag(name, selfClosing) {
    if (this.inForeign) {
      if (!selfClosing) {
        this.stack.push({ name, foreign: true, dirty: false, marked: false, implied: new Set() });
        this.#raise(1);
      }
      return;
    }
    // Elements that the tree builder inserts implicitly for this start tag (M-26, M-09).
    const f = this.#tableContext();
    this.#classifyStart(name, f);
    if (f !== null) {
      const add = (kind) => {
        if (!f.implied.has(kind)) {
          f.implied.add(kind);
          this.#raise(1);
        }
      };
      if (name === "tr" && f.name === "table") add("rowgroup");
      if ((name === "td" || name === "th") && f.name === "table") {
        add("rowgroup");
        add("row");
      }
      if ((name === "td" || name === "th") && ROW_GROUPS.has(f.name)) add("row");
      if (name === "col" && f.name === "table") add("colgroup");
    }
    if ((name === "svg" || name === "math") && selfClosing) return;
    if (VOID.has(name)) return;
    this.stack.push({ name, foreign: false, dirty: false, marked: false, implied: new Set() });
    if (name === "svg" || name === "math") this.roots++;
    this.#raise(1);
  }

  /** Rule classes of round 2 of the security review for a start tag in HTML context. */
  #classifyStart(name, f) {
    if (name === "col") {
      for (let i = this.stack.length - 1; i >= 0; i--) {
        const n = this.stack[i].name;
        if (n === "template") {
          this.classes.add(CLASSES.templateCol);
          break;
        }
        if (TABLE_FAMILY.has(n)) break;
      }
    }
    if (name === "form" && this.stack.some((e) => e.name === "form")) this.classes.add(CLASSES.form);
    const top = this.stack[this.stack.length - 1];
    if ((TABLE_PART.has(name) || name === "col" || name === "table") && f !== null && TABLE_CONTEXT.has(f.name) && f !== top) {
      this.classes.add(CLASSES.tableCurrent);
    }
  }

  /** An end tag token (M-05 item 3) with the rule classes of H-26 and H-30. */
  endTag(name) {
    let i = this.stack.length - 1;
    while (i >= 0 && this.stack[i].name !== name) i--;
    if (i < 0) {
      if (!this.inForeign && STRAY_TABLE_END.has(name)) this.classes.add(CLASSES.strayTable);
      return;
    }
    const popped = this.stack.splice(i);
    const crossesSpecial = popped.slice(1).some((e) => e.foreign || SPECIAL.has(e.name));
    if (FORMATTING.has(name)) {
      if (crossesSpecial) this.classes.add(CLASSES.formatting);
      if (popped.some((e) => e.marked)) this.classes.add(CLASSES.formattingMarked);
    }
    // M-06 item 4: a crossing over a special or foreign element, or the pop of a marked element,
    // marks the new current node.
    if ((crossesSpecial || popped.some((e) => e.marked)) && this.stack.length > 0) {
      this.stack[this.stack.length - 1].marked = true;
    }
    for (const e of popped) if (!e.foreign && (e.name === "svg" || e.name === "math")) this.roots--;
    const top = popped[0];
    if (popped.length === 1 && !top.dirty && name !== "html" && name !== "body") {
      this.d -= 1 + top.implied.size;
    } else if (this.stack.length > 0) {
      this.stack[this.stack.length - 1].dirty = true;
    }
  }
}

/**
 * The raw value of the first attribute `name` of a start tag (markup.md 2.1 "Raw value").
 * @param {string} text the decoded document
 * @param {{ attrs?: Record<string, { startOffset: number, endOffset: number }> } | undefined} location
 */
export function rawAttrValue(text, location, name) {
  const loc = location?.attrs?.[name];
  if (!loc) return null;
  const src = text.slice(loc.startOffset, loc.endOffset);
  const eq = src.indexOf("=");
  if (eq < 0) return "";
  const v = src.slice(eq + 1).replace(/^[\t\n\f\r ]+/, "");
  if (v.startsWith('"') || v.startsWith("'")) return v.slice(1, -1);
  return v;
}

const hasNonAscii = (s) => /[^\x00-\x7f]/.test(s);

/**
 * M-11 with H-27: tokenize the noscript content C (HTML context, M-04) and check that it is
 * balanced. Also applies M-13 (H-28) to script start tags in C, and H-38 (a table part or col
 * needs a table on the C stack).
 * @param {string} c
 * @returns {{ balanced: boolean, nonAsciiType: boolean, tablePart: boolean }}
 */
export function checkNoscriptContent(c) {
  const out = { balanced: true, nonAsciiType: false, tablePart: false };
  const stack = [];
  let done = false;
  const handler = {
    onStartTag(t) {
      if (done) return;
      if (t.tagName === "script") {
        const type = rawAttrValue(text, t.location, "type");
        if (type !== null && hasNonAscii(type)) out.nonAsciiType = true;
      }
      if ((TABLE_PART.has(t.tagName) || t.tagName === "col") && !stack.includes("table")) out.tablePart = true;
      if (RCDATA.has(t.tagName)) tok.state = TokenizerMode.RCDATA;
      else if (RAWTEXT.has(t.tagName)) tok.state = TokenizerMode.RAWTEXT;
      else if (t.tagName === "script") tok.state = TokenizerMode.SCRIPT_DATA;
      else if (t.tagName === "plaintext") tok.state = TokenizerMode.PLAINTEXT;
      if (!VOID.has(t.tagName) && !((t.tagName === "svg" || t.tagName === "math") && t.selfClosing)) stack.push(t.tagName);
    },
    onEndTag(t) {
      if (done) return;
      if (t.tagName === "noscript") {
        done = true;
        return;
      }
      if (stack.length === 0 || stack[stack.length - 1] !== t.tagName) out.balanced = false;
      else stack.pop();
    },
    onComment() {},
    onDoctype() {},
    onCharacter() {},
    onNullCharacter() {},
    onWhitespaceCharacter() {},
    onEof() {},
  };
  const text = `${c}</noscript>`;
  const tok = new Tokenizer({ sourceCodeLocationInfo: true }, handler);
  tok.write(text, true);
  if (stack.length > 0) out.balanced = false;
  return out;
}

function isElement(node) {
  return node !== null && typeof node === "object" && typeof node.tagName === "string";
}

/**
 * @typedef {object} DepthAnalysis
 * @property {boolean} ok           false when the input has no view (encoding, size)
 * @property {number} maxD          the largest value of D
 * @property {number} maxStack      the largest WHATWG stack of open elements (html included)
 * @property {number} maxDepth      the largest DOM depth of an element
 * @property {string[]} violations  invariant violations (at most 5, no document text)
 * @property {string[]} classes     rule classes (CLASSES) found in the document
 */

/**
 * Parse the document with parse5 (scripting enabled), compute D and check the invariant.
 *
 * @param {Uint8Array} bytes
 * @param {{ fullStackLimit?: number }} [options] fullStackLimit: check every stack element only
 *   while the stack is not larger than this (default 1024); else only new elements and the current node
 * @returns {DepthAnalysis}
 */
export function analyzeDepth(bytes, { fullStackLimit = 1024 } = {}) {
  const empty = { ok: false, maxD: 0, maxStack: 0, maxDepth: 0, violations: [], classes: [] };
  if (bytes.length > MAX_INPUT_BYTES) return empty;
  const decoded = decodeUtf8(bytes);
  if (decoded === null) return empty;
  const text = decoded.text;

  const counter = new DepthCounter();
  const violations = [];
  const hosts = new WeakMap(); // template content fragment → template element
  /** @type {object[]} */
  const attached = [];
  let maxStack = 0;
  let maxDepth = 0;
  let noscriptStart = -1;
  const seen = new WeakSet(); // end tag tokens that the counter has seen

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

  const report = (message) => {
    if (violations.length < 5) violations.push(message);
  };

  class DepthParser extends Parser {
    #after(what, offset) {
      const items = this.openElements.items;
      const size = this.openElements.stackTop + 1;
      if (size > maxStack) maxStack = size;
      const d = counter.d;
      const at = `${what} at ${offset ?? "?"}`;
      // H-35: the claim of M-26 excludes template contents (inert, no block inside, M-19).
      for (let k = 0; k < size; k++) {
        if (items[k].tagName === "template" && items[k].namespaceURI === HTML_NS) {
          attached.length = 0;
          return;
        }
      }
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
      const from = size <= fullStackLimit ? 0 : size - 1;
      for (let k = 0; k < size; k++) open.add(items[k]);
      for (let k = from; k < size; k++) {
        const dep = depth(items[k]);
        if (dep > maxDepth) maxDepth = dep;
        if (dep > d + 2) report(`open <${items[k].tagName}> depth ${dep} > D ${d} + 2 after ${at}`);
      }
      for (const e of attached) {
        const dep = depth(e);
        if (dep > maxDepth) maxDepth = dep;
        const limit = open.has(e) ? d + 2 : d + 3;
        if (dep > limit) report(`<${e.tagName}> depth ${dep} > D ${d} + ${limit - d} after ${at}`);
      }
      attached.length = 0;
    }

    onStartTag(token) {
      const htmlContext = !counter.inForeign;
      counter.startTag(token.tagName, token.selfClosing);
      if (token.tagName === "script") {
        const type = rawAttrValue(text, token.location, "type");
        if (type !== null && hasNonAscii(type)) counter.classes.add(CLASSES.typeNonAscii);
      }
      if (htmlContext && token.tagName === "noscript" && token.location) noscriptStart = token.location.endOffset;
      super.onStartTag(token);
      this.#after(`<${token.tagName}>`, token.location?.startOffset);
    }

    onEndTag(token) {
      // parse5 calls onEndTag again when it reprocesses a token (_processToken, for example after
      // the "in table text" mode). The counter sees each tokenizer token once.
      if (!seen.has(token)) {
        seen.add(token);
        if (token.tagName === "noscript" && noscriptStart >= 0 && token.location) {
          this.#noscript(text.slice(noscriptStart, token.location.startOffset));
        }
        counter.endTag(token.tagName);
      }
      super.onEndTag(token);
      this.#after(`</${token.tagName}>`, token.location?.startOffset);
    }

    #noscript(c) {
      noscriptStart = -1;
      const r = checkNoscriptContent(c);
      if (!r.balanced) counter.classes.add(CLASSES.noscriptBalance);
      if (r.nonAsciiType) counter.classes.add(CLASSES.typeNonAscii);
      if (r.tablePart) counter.classes.add(CLASSES.noscriptTablePart);
    }

    onCharacter(token) {
      super.onCharacter(token);
      this.#after("text", token.location?.startOffset);
    }

    onNullCharacter(token) {
      super.onNullCharacter(token);
      this.#after("text", token.location?.startOffset);
    }

    onWhitespaceCharacter(token) {
      super.onWhitespaceCharacter(token);
      this.#after("text", token.location?.startOffset);
    }

    onComment(token) {
      super.onComment(token);
      this.#after("comment", token.location?.startOffset);
    }

    onEof(token) {
      // E3: a noscript element without an end tag; C is the rest of the input.
      if (noscriptStart >= 0) this.#noscript(text.slice(noscriptStart));
      super.onEof(token);
      this.#after("EOF", token.location?.startOffset);
    }
  }

  DepthParser.parse(text, { treeAdapter: adapter, scriptingEnabled: true, sourceCodeLocationInfo: true });
  return { ok: true, maxD: counter.maxD, maxStack, maxDepth, violations, classes: [...counter.classes].sort() };
}
