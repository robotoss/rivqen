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
//
// The counter applies the implied closure of markup.md M-09 (H-85a): before a start tag with a name
// in TABLE_PART or col in HTML context, while F is not an allowed F, it applies the end tag of F, at
// most three times. It does so outside block content and outside the noscript content C only (the
// copy of the pass of M-11 item 8 and the scripting-disabled view of C have no closure).
//
// The counter knows the block content (H-94): after a start tag in HTML context with a data-rq-block
// attribute, while no block element is on its stack, the elements above that element are the content.
// It uses this for the closure and for the rule classes of WP-17 S5 that parse5 cannot show:
// M-22 item 6 (a template with shadowrootmode in the content; parse5 8.0.1 has no declarative shadow
// DOM) and M-24 item g (a table part or col in a template of the content; parse5 8.0.1 stops its table
// scope only at table and html, not at template).

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
/** M-09: the allowed F of a table part or col start tag (markup.md, the table of M-09). */
export const ALLOWED_F = new Map([
  ["caption", names("table")],
  ["colgroup", names("table")],
  ["tbody", names("table")],
  ["thead", names("table")],
  ["tfoot", names("table")],
  ["col", names("table colgroup")],
  ["tr", names("table tbody thead tfoot")],
  ["td", names("table tbody thead tfoot tr")],
  ["th", names("table tbody thead tfoot tr")],
]);
/** M-09 (H-85a): the implied closure ends after at most three end tags (a cell, a row, a row group). */
export const MAX_CLOSURES = 3;
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
  templateFor: "M-10-template-for", // H-43: an HTML template start tag with a for attribute (content patching; parse5 8.0.1 does not implement it)
  noscriptDocumentTag: "M-11-document-tag", // H-71, M-11 item 7: an html, body or head start or end tag in C
  shadowRootMode: "M-22-shadowrootmode", // H-94, H-97, M-22 item 6: an HTML template start tag with a shadowrootmode attribute in block content
  templateTablePart: "M-24-template-table-part", // H-94, M-24 item g: a table part or col start tag in block content whose F search stops at a template
};

/** M-22 item 6 (H-97): the attribute of a declarative shadow root (any value, any case). */
const SHADOW_ROOT_MODE = "shadowrootmode";
/** M-14: the block attribute. */
const BLOCK_ATTR = "data-rq-block";

/** M-11 item 7 (H-71): tag names that C must not contain. */
const DOCUMENT_TAGS = names("html body head");

/**
 * The depth counter D of markup.md M-26 over the token stack of M-05.
 * Each stack entry: name, foreign (pushed in a foreign region), dirty, marked (special-crossed,
 * markup.md M-06 item 4), implied flags.
 */
export class DepthCounter {
  /**
   * @param {{ closure?: boolean }} [options] closure: apply the implied closure of M-09 (H-85a);
   *   default true. A caller that keeps its own token stack (the corpus classifier) turns it off and
   *   applies the closure through endTag.
   */
  constructor({ closure = true } = {}) {
    /** @type {{ name: string, foreign: boolean, dirty: boolean, marked: boolean, implied: Set<string> }[]} */
    this.stack = [];
    this.d = 0;
    this.maxD = 0;
    this.roots = 0; // svg and math elements pushed in HTML context that are on the stack
    /** @type {Set<string>} */
    this.classes = new Set();
    /** H-85a: apply the implied closure of M-09 (off in the noscript content C). */
    this.closure = closure;
    /** Number of end tags of F that the implied closure applied (test and statistics only). */
    this.closures = 0;
    /** Stack index of the open block element (HTML context), or -1: the elements above it are the content. */
    this.block = -1;
  }

  /** True while a block element is on the stack (the next token is in the block content). */
  get inContent() {
    return this.block >= 0;
  }

  get inForeign() {
    return this.roots > 0;
  }

  /**
   * A copy of the token stack, D and the marks (for the pass of markup.md M-11 item 8 on C, H-71).
   * The copy shares the set of rule classes, so a class found on the copy is a class of the document.
   * Its maxD starts at the current D, not at the maxD of this counter.
   * Memory: one entry for each element on the stack, at most D + 1 entries when D <= 256.
   */
  clone() {
    const c = new DepthCounter();
    c.stack = this.stack.map((e) => ({ ...e, implied: new Set(e.implied) }));
    c.d = this.d;
    c.maxD = this.d;
    c.roots = this.roots;
    c.classes = this.classes;
    c.closure = this.closure;
    c.block = this.block;
    return c;
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
    return this.#familySearch().f;
  }

  /** The search for F of M-09: F (or null), and whether a template stopped the search. */
  #familySearch() {
    for (let i = this.stack.length - 1; i >= 0; i--) {
      const e = this.stack[i];
      if (e.name === "template") return { f: null, template: true };
      if (TABLE_FAMILY.has(e.name)) return { f: e, template: false };
    }
    return { f: null, template: false };
  }

  /**
   * A start tag token (M-05 items 1 and 2).
   * @param {string} name
   * @param {boolean} selfClosing
   * @param {string[]} [attrNames] attribute names in lower case (M-10 item 3)
   */
  startTag(name, selfClosing, attrNames = []) {
    if (this.inForeign) {
      if (!selfClosing) {
        this.stack.push({ name, foreign: true, dirty: false, marked: false, implied: new Set() });
        this.#raise(1);
      }
      return;
    }
    // H-85a: no implied closure in block content (M-09 rejects there) and in C (`closure` off).
    if (this.closure && !this.inContent) this.#impliedClosure(name);
    // Elements that the tree builder inserts implicitly for this start tag (M-26, M-09).
    const f = this.#tableContext();
    this.#classifyStart(name, f, attrNames);
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
    if (!this.inContent && attrNames.includes(BLOCK_ATTR)) this.block = this.stack.length - 1;
    this.#raise(1);
  }

  /**
   * The implied closure of M-09 (H-85a) for a start tag in HTML context: while F is not an allowed F
   * of the name, apply the end tag of F (M-05 item 3, the marks of M-06 item 4, M-26 items 4 and 5),
   * at most MAX_CLOSURES times. Every allowed-F set has table, so a table is never closed. The closure
   * of a row, a row group or a column group needs F as the current node (M-09 condition 2); when it is
   * not, the class of H-37 is set and the closure goes on as if the condition were removed. A crossing
   * over a guarded element (M-06 item 1) has no class here: candidates reject it, and the oracle only
   * checks documents that a candidate accepts.
   * @param {string} name
   */
  #impliedClosure(name) {
    const allowed = ALLOWED_F.get(name);
    if (allowed === undefined) return;
    for (let step = 0; step < MAX_CLOSURES; step++) {
      const f = this.#tableContext();
      if (f === null || allowed.has(f.name)) return;
      if (TABLE_CONTEXT.has(f.name) && f !== this.stack[this.stack.length - 1]) this.classes.add(CLASSES.tableCurrent);
      // F is the nearest entry with its name: no TABLE_FAMILY entry is above F.
      this.endTag(f.name);
      this.closures++;
    }
  }

  /** Rule classes of round 2 of the security review and of H-43 for a start tag in HTML context. */
  #classifyStart(name, f, attrNames) {
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
    if (name === "template" && attrNames.includes("for")) this.classes.add(CLASSES.templateFor);
    if (this.inContent) {
      // WP-17 S5: the content rules that parse5 cannot show (H-94, H-97).
      if (name === "template" && attrNames.includes(SHADOW_ROOT_MODE)) this.classes.add(CLASSES.shadowRootMode);
      if ((TABLE_PART.has(name) || name === "col") && this.#familySearch().template) this.classes.add(CLASSES.templateTablePart);
    }
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
    // marks the new current node. H-75: the end tag of the region root (the svg or math element
    // pushed in HTML context) crosses only elements of its region and sets no mark for them.
    const rootEnd = !popped[0].foreign && (name === "svg" || name === "math");
    if (((crossesSpecial && !rootEnd) || popped.some((e) => e.marked)) && this.stack.length > 0) {
      this.stack[this.stack.length - 1].marked = true;
    }
    for (const e of popped) if (!e.foreign && (e.name === "svg" || e.name === "math")) this.roots--;
    if (this.block >= this.stack.length) this.block = -1;
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
 * M-11 for the noscript content C: tokenize C followed by `</noscript>` (M-04 in HTML context of C;
 * no state change in a foreign region of C, item 9, H-74) and check:
 * - item 5 (H-27): C is balanced. Exception (H-75): in a foreign region of C, the end tag of the
 *   region root (the svg or math element pushed in HTML context of C) can cross elements of the region;
 * - item 6 (H-38): a table part or col needs a table on the C stack (also in a foreign region of C);
 * - item 7 (H-71): no html, body or head start or end tag;
 * - M-13 item 3 (H-28) for script start tags.
 * With `outer` (the main-pass counter while the noscript element is the current node), item 8
 * (H-71): the tags of C and the end tag noscript also go through a copy of that counter. Its rule
 * classes go to the classes of `outer` (shared set), and `maxD` is the largest D of that pass.
 * `outer` itself does not change. The pass ends at the first end tag noscript (item 1).
 *
 * Time: one tokenizer pass over C; per end tag a search of the C stack (bounded by the size of C)
 * and of the copy (at most D + 1 entries while D <= 256).
 * @param {string} c
 * @param {DepthCounter | null} [outer]
 * @returns {{ balanced: boolean, nonAsciiType: boolean, tablePart: boolean, documentTag: boolean, maxD: number }}
 */
export function checkNoscriptContent(c, outer = null) {
  const out = { balanced: true, nonAsciiType: false, tablePart: false, documentTag: false, maxD: 0 };
  const pass = outer === null ? null : outer.clone();
  if (pass !== null) pass.closure = false; // H-85a: no implied closure in C (M-09 rejects there)
  /** @type {string[]} */
  const stack = [];
  let region = -1; // index in `stack` of the root of the foreign region of C
  let done = false;
  const handler = {
    onStartTag(t) {
      if (done) return;
      const name = t.tagName;
      if (DOCUMENT_TAGS.has(name)) out.documentTag = true;
      if (name === "script") {
        const type = rawAttrValue(text, t.location, "type");
        if (type !== null && hasNonAscii(type)) out.nonAsciiType = true;
      }
      if ((TABLE_PART.has(name) || name === "col") && !stack.includes("table")) out.tablePart = true;
      pass?.startTag(name, t.selfClosing, (t.attrs ?? []).map((a) => a.name));
      if (region >= 0) {
        // Foreign region of C: M-05 item 2, no state change (M-04).
        if (!t.selfClosing) stack.push(name);
        return;
      }
      if (RCDATA.has(name)) tok.state = TokenizerMode.RCDATA;
      else if (RAWTEXT.has(name)) tok.state = TokenizerMode.RAWTEXT;
      else if (name === "script") tok.state = TokenizerMode.SCRIPT_DATA;
      else if (name === "plaintext") tok.state = TokenizerMode.PLAINTEXT;
      const root = name === "svg" || name === "math";
      if (root && t.selfClosing) return;
      if (!VOID.has(name)) stack.push(name);
      if (root) region = stack.length - 1;
    },
    onEndTag(t) {
      if (done) return;
      const name = t.tagName;
      if (name === "noscript") {
        done = true;
        pass?.endTag(name);
        return;
      }
      if (DOCUMENT_TAGS.has(name)) out.documentTag = true;
      pass?.endTag(name);
      const top = stack.length - 1;
      if (top >= 0 && stack[top] === name) {
        stack.pop();
      } else if (region >= 0 && stack[region] === name && stack.lastIndexOf(name) === region) {
        stack.length = region; // H-75: the end tag of the region root crosses elements of the region
      } else {
        out.balanced = false;
        return;
      }
      if (stack.length <= region) region = -1;
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
  if (pass !== null) out.maxD = pass.maxD;
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
 * Parse the document with parse5, compute D and check the invariant.
 *
 * Scripting enabled (default): the RQP view (markup.md M-04); the noscript content C is text, and
 * the pass of M-11 item 8 runs on a copy of the counter. Scripting disabled: C is markup, so the
 * counter runs on the tokens of C as they come (that is the pass of item 8, H-71), and after the
 * end tag noscript it continues with the main-pass state. The invariant then checks the claim of
 * M-26 for the scripting-disabled tree too.
 *
 * @param {Uint8Array} bytes
 * @param {{ fullStackLimit?: number, scripting?: boolean }} [options] fullStackLimit: check every
 *   stack element only while the stack is not larger than this (default 1024); else only new
 *   elements and the current node. scripting: the parse5 option scriptingEnabled (default true)
 * @returns {DepthAnalysis}
 */
export function analyzeDepth(bytes, { fullStackLimit = 1024, scripting = true } = {}) {
  const empty = { ok: false, maxD: 0, maxStack: 0, maxDepth: 0, violations: [], classes: [] };
  if (bytes.length > MAX_INPUT_BYTES) return empty;
  const decoded = decodeUtf8(bytes);
  if (decoded === null) return empty;
  const text = decoded.text;

  let counter = new DepthCounter();
  /** @type {DepthCounter | null} the main-pass state at the noscript start tag (scripting disabled) */
  let saved = null;
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
      counter.startTag(token.tagName, token.selfClosing, (token.attrs ?? []).map((a) => a.name));
      if (token.tagName === "script") {
        const type = rawAttrValue(text, token.location, "type");
        if (type !== null && hasNonAscii(type)) counter.classes.add(CLASSES.typeNonAscii);
      }
      if (htmlContext && token.tagName === "noscript" && token.location && noscriptStart < 0) {
        noscriptStart = token.location.endOffset;
        // Scripting disabled: the tokens of C come in this stream, and the counter runs on them as
        // the pass of M-11 item 8 (H-71). Keep the main-pass state to continue with it after C.
        if (!scripting) {
          saved = counter.clone();
          saved.maxD = counter.maxD;
          counter.closure = false; // H-85a: no implied closure in C; `saved` keeps the main-pass setting
        }
      }
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
          if (saved !== null) {
            // After the end tag noscript the main pass continues with its own state (M-11 item 8).
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
      const main = saved ?? counter; // the main-pass state while the noscript element is the current node
      const r = checkNoscriptContent(c, main);
      if (!r.balanced) counter.classes.add(CLASSES.noscriptBalance);
      if (r.nonAsciiType) counter.classes.add(CLASSES.typeNonAscii);
      if (r.tablePart) counter.classes.add(CLASSES.noscriptTablePart);
      if (r.documentTag) counter.classes.add(CLASSES.noscriptDocumentTag);
      // D counts in C (M-26 item 7, H-71).
      if (r.maxD > main.maxD) main.maxD = r.maxD;
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

  DepthParser.parse(text, { treeAdapter: adapter, scriptingEnabled: scripting, sourceCodeLocationInfo: true });
  const maxD = Math.max(counter.maxD, saved?.maxD ?? 0);
  return { ok: true, maxD, maxStack, maxDepth, violations, classes: [...counter.classes].sort() };
}
