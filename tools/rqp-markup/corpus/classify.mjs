#!/usr/bin/env node
// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// All-rules classifier for the markup corpus (WP-17 S1 T-15). Measurement tool only: it is
// not a candidate and it does not decide validity.
//
// A candidate stops at the first rule violation. This pass does not stop: it records every
// violation of the document rules of markup.md (M-01, M-02, M-06 … M-13, M-26) with the
// rule item, then continues with the plain token-stack behavior of M-05. So "the document
// breaks only rule X" means: if rule X were removed (and nothing else changed), the
// candidates would accept the document.
//
// The rules of security review round 2 and of H-43 (M-09 conditions 1 and 2, M-10 items 2
// and 3, M-11 item 6) come from the cross-check oracle (../crosscheck/src/depth.mjs:
// DepthCounter and checkNoscriptContent); this pass only adds details. It also finds
// elements for the simulated blocks, and (with blocks) checks the block rules M-14 … M-24
// with their items.
//
// The implied closure of M-09 (H-85a): outside block content (known with blocks only) and
// outside the noscript content C, a table part or col start tag first applies the end tag of F
// while F is not an allowed F, at most three times. This pass does it on its own token stack
// (M-06 item 1 with the detail "implied </x>", the marks of M-06 item 4) and gives each end tag
// to the depth counter, whose own closure is off. A closure of a row, a row group or a column
// group while F is not the current node is M-09.2; the pass records it and closes anyway, as if
// the condition were removed.
//
// The tokens come from the parse5 8.0.1 tokenizer (the WHATWG tokenizer, M-03), driven
// without a tree builder: this pass sets the tokenizer state after a start tag by M-04 and
// the foreign-region flag of M-05, like a candidate does. A `<![CDATA[` is always read as a
// bogus comment (M-04 allows it; M-12 makes both readings end at the same byte).
//
// Usage: node classify.mjs <file> [--charset <label>] [--blocks]
// Prints one JSON object. The output never contains document text: only tag names,
// rule ids, counts and offsets.

import { readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { Tokenizer, TokenizerMode } from "../crosscheck/node_modules/parse5/dist/index.js";
import { CLASSES, DepthCounter, FORMATTING, SPECIAL, VOID, checkNoscriptContent, rawAttrValue } from "../crosscheck/src/depth.mjs";
import { decodeUtf8, utf16ToUtf8Map } from "../crosscheck/src/utf8.mjs";

export const MAX_INPUT_BYTES = 5 * 1024 * 1024;
const MAX_DETAIL_KEYS = 12;

const names = (s) => new Set(s.split(/\s+/).filter((x) => x !== ""));
const H = "h1 h2 h3 h4 h5 h6";
const HEADINGS = names(H);
const RCDATA = names("title textarea");
const RAWTEXT = names("iframe noembed noframes noscript style xmp");
const TEXT = names("title textarea iframe noembed noframes noscript style xmp script plaintext");
const PCLOSE = names(`address article aside blockquote center dd details dialog dir div dl dt fieldset figcaption
  figure footer form ${H} header hgroup hr li listing main menu nav ol p plaintext pre search section summary table ul xmp`);
const GUARDED = names("a applet button caption colgroup marquee math nobr object select svg table tbody td template tfoot th thead tr");
const TABLE_PART = names("caption colgroup tbody td tfoot th thead tr");
const TABLE_FAMILY = names("table caption colgroup tbody td tfoot th thead tr");
const BREAKOUT = names(`b big blockquote body br center code dd div dl dt em embed font ${H} head hr i img li listing menu
  meta nobr ol p pre ruby s small span strike strong sub sup table tt u ul var`);
const INTEGRATION = names("annotation-xml desc foreignobject mi mn mo ms mtext title");
const BLOCK_HTML = names(`article aside div footer ${H} header main nav p section span title`);
// M-22 item 2 after H-78, H-94 and H-95: the names that M-22 keeps (template and noscript left it).
const FORBIDDEN_CONTENT = names(`body frame frameset head html noembed noframes optgroup option plaintext select
  textarea title xmp`);
// M-22 item 6 (H-94, H-97): the attribute of a declarative shadow root on an HTML template start tag.
const SHADOW_ROOT_MODE = "shadowrootmode";
// The patch content rule (manifest-patch.md section 4 item 1, H-78): not markup rules. A block with
// these names is valid; a patch that brings them is rejected. PATCH_FORBIDDEN: start tags in HTML
// context of the content (noscript: H-96); FOREIGN_ALLOWED (H-73): the only start tags in a foreign
// region of it.
const PATCH_FORBIDDEN = names("base basefont bgsound embed iframe link meta noscript object script style");
const FOREIGN_ALLOWED = names(`svg g path circle ellipse line polyline polygon rect defs symbol use title desc lineargradient
  radialgradient stop clippath mask pattern marker text tspan filter feblend fecolormatrix fecomponenttransfer fecomposite
  fedisplacementmap fedropshadow feflood fefunca fefuncb fefuncg fefuncr fegaussianblur femerge femergenode femorphology
  feoffset feturbulence math mrow mi mo mn ms mtext mspace msub msup msubsup mfrac msqrt mroot mstyle munder mover munderover
  mmultiscripts mprescripts none mtable mtr mtd mpadded mphantom merror semantics annotation`);
// M-16 json types (exact, case-sensitive; H-86 adds application/ld+json).
const JSON_TYPES = names("application/json application/ld+json");
const STRAY_TABLE_END = names("tbody thead tfoot tr table");
const SELECT_START = names("option optgroup hr");
const SELECT_END = names("option optgroup select");
const NOSCRIPT_FORBIDDEN = names("frameset math noscript plaintext select template"); // M-11 item 3 (svg: item 9, H-74)
const DOCUMENT_TAGS = names("html body head"); // M-11 item 7 (H-71)
const ROW_CONTEXT = names("table tbody thead tfoot tr colgroup");
const LISTS = names("ul ol menu dl");
const LIST_ITEMS = names("li dd dt");
const BLOCK_ID = /^[a-z0-9][a-z0-9_-]{0,63}$/;
const PICK_MATCH = /price|title|content/i;

/** A tag name for a detail: names from the page that do not look like tag names become "?". */
const safeName = (name) => (/^[a-z][a-z0-9-]{0,31}$/.test(name) ? name : "?");

/** M-09 (H-85a): the implied closure ends after at most three end tags (a cell, a row, a row group). */
const MAX_CLOSURES = 3;

/** M-09: the F values that allow a table part or col start tag (null: the name is not a table part or col). */
function allowedParents(name) {
  if (name === "col") return ["table", "colgroup"];
  if (name === "tr") return ["table", "tbody", "thead", "tfoot"];
  if (name === "td" || name === "th") return ["table", "tbody", "thead", "tfoot", "tr"];
  if (TABLE_PART.has(name)) return ["table"];
  return null;
}

/** The rule id of a class: "M-06.1" → "M-06", "M-26-dirty" → "M-26", "M-22.2:script" → "M-22". */
export const ruleOf = (cls) => /^M-\d\d/.exec(cls)?.[0] ?? cls;

/**
 * Decode the input. Strict UTF-8 first (M-02). When that fails, decode with the declared
 * charset (or windows-1252) without errors, so the structure rules can still be measured.
 * @param {Uint8Array} bytes
 * @param {string | null} charset
 */
export function decodeInput(bytes, charset) {
  const strict = decodeUtf8(bytes);
  if (strict !== null) return { utf8: true, text: strict.text, bomBytes: strict.bomBytes };
  let label = charset ?? sniffCharset(bytes) ?? "windows-1252";
  if (/^utf-?8$/i.test(label)) label = "windows-1252";
  let decoder;
  try {
    decoder = new TextDecoder(label, { fatal: false });
  } catch {
    decoder = new TextDecoder("windows-1252", { fatal: false });
  }
  return { utf8: false, text: decoder.decode(bytes), bomBytes: 0 };
}

/** A `<meta charset>` or `http-equiv` charset in the first 1024 bytes (bounded scan). */
export function sniffCharset(bytes) {
  const head = Buffer.from(bytes.subarray(0, 1024)).toString("latin1");
  const m = /<meta[^>]{0,200}?charset\s*=\s*["']?([A-Za-z0-9._:-]{1,40})/i.exec(head);
  return m ? m[1].toLowerCase() : null;
}

/** Counts of rule classes, with a bounded map of details for each class. */
class Findings {
  constructor() {
    /** @type {Map<string, number>} */
    this.counts = new Map();
    /** @type {Map<string, Map<string, number>>} */
    this.details = new Map();
  }

  add(cls, detail = null) {
    this.counts.set(cls, (this.counts.get(cls) ?? 0) + 1);
    if (detail === null) return;
    let d = this.details.get(cls);
    if (!d) {
      d = new Map();
      this.details.set(cls, d);
    }
    if (d.has(detail) || d.size < MAX_DETAIL_KEYS) d.set(detail, (d.get(detail) ?? 0) + 1);
  }

  toJSON() {
    const classes = Object.fromEntries([...this.counts].sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0)));
    const details = {};
    for (const [k, v] of [...this.details].sort(([a], [b]) => (a < b ? -1 : 1))) {
      details[k] = Object.fromEntries([...v].sort((x, y) => y[1] - x[1]));
    }
    return { classes, details };
  }
}

const hasNonAscii = (s) => /[^\x00-\x7f]/.test(s);

/**
 * M-13 for the raw value of the first `type` attribute of a script start tag.
 * @returns {string | null} the item that fails ("1", "2", "3") or null
 */
export function scriptTypeItem(type) {
  if (type === null) return null;
  if (type.toLowerCase().includes("rivqen-manifest")) return "1";
  for (let i = type.indexOf("&"); i >= 0; i = type.indexOf("&", i + 1)) if (rejectedAmpersand(type, i)) return "2";
  if (hasNonAscii(type)) return "3";
  return null;
}

const ASCII_LETTER = /^[A-Za-z]$/;
const ASCII_DIGIT = /^[0-9]$/;
const HEX_DIGIT = /^[0-9A-Fa-f]$/;
const TYPE_PUNCTUATION = new Set(["/", "+", "-", "."]);

/**
 * The value of the numeric character reference that starts at the `&` at `at`, read as the WHATWG
 * tokenizer reads it (`&#` with decimal digits, or `&#x`/`&#X` with hex digits; leading zeros;
 * `;` optional), or -1 when no numeric reference starts there. Values above 10FFFF are 110000.
 * @param {string} s
 * @param {number} at
 */
export function numericReference(s, at) {
  if (s[at + 1] !== "#") return -1;
  let j = at + 2;
  const hex = s[j] === "x" || s[j] === "X";
  if (hex) j++;
  const digit = hex ? HEX_DIGIT : ASCII_DIGIT;
  if (j >= s.length || !digit.test(s[j])) return -1;
  let v = 0;
  for (; j < s.length && digit.test(s[j]); j++) v = Math.min(v * (hex ? 16 : 10) + Number.parseInt(s[j], 16), 0x110000);
  return v;
}

/**
 * M-13 item 2 (H-84): the `&` at `at` rejects when an ASCII letter follows it (every named
 * reference), or when it starts a numeric reference whose value is 00-20, an ASCII letter or digit,
 * "/", "+", "-", ".", or 80 and above (a non-ASCII character, the H-28 shape through a reference).
 * @param {string} s
 * @param {number} at
 */
function rejectedAmpersand(s, at) {
  if (at + 1 < s.length && ASCII_LETTER.test(s[at + 1])) return true;
  const v = numericReference(s, at);
  if (v < 0) return false;
  if (v <= 0x20 || v >= 0x80) return true;
  const c = String.fromCharCode(v);
  return ASCII_LETTER.test(c) || ASCII_DIGIT.test(c) || TYPE_PUNCTUATION.has(c);
}

/** M-12: the first `>` after `<![CDATA[` at `start` must end a `]]>` that starts after it. */
export function cdataOk(text, start) {
  const gt = text.indexOf(">", start + 9);
  if (gt < 0) return true;
  return gt - 2 >= start + 9 && text[gt - 1] === "]" && text[gt - 2] === "]";
}

function setState(tok, name) {
  if (RCDATA.has(name)) tok.state = TokenizerMode.RCDATA;
  else if (RAWTEXT.has(name)) tok.state = TokenizerMode.RAWTEXT;
  else if (name === "script") tok.state = TokenizerMode.SCRIPT_DATA;
  else if (name === "plaintext") tok.state = TokenizerMode.PLAINTEXT;
}

function noop() {}

/**
 * M-11 for the noscript content C (items 1 to 9). Item 6 (H-38), the D of item 8 (H-71), and the
 * classes of M-09 conditions 1 and 2 and M-10 items 2 and 3 of item 8 come from the cross-check
 * oracle (checkNoscriptContent of depth.mjs with the main-pass counter: its classes go to the class
 * set of that counter). This pass adds items 1 to 5 and 7 with details, and the other main-pass
 * rules of item 8 on the tokens of C: M-07 in the foreign region of C (item 9, H-74), M-09 for the
 * table parts (F can be an element around the noscript element) and M-12. The details of the
 * item 8 classes end with "in noscript". M-06 in C needs no class: a crossing or a stray end tag
 * in C is already M-11 item 5.
 *
 * Item 10 (H-95): when the noscript element is in the content of an html block, the caller gives
 * `content`. It gets each DOCTYPE token of C (M-22 item 1) and each start tag in HTML context of C
 * (M-22 items 2, 3 and 6, M-24), with the names of the open elements of C below it (all pushed in
 * HTML context: a start tag in a foreign region of C is not a content-rule tag). Item 5 is M-23
 * for C, so end tags need no call.
 * @param {string} c
 * @param {Findings} f
 * @param {{ inCell: boolean, outer?: DepthCounter | null, outerFamily?: string | null, tableCurrentOver?: Set<string>,
 *   content?: ((tag: { name: string, selfClosing: boolean, attrNames: string[] } | null, open: string[]) => void) | null }} ctx
 *   outer: the main-pass counter while the noscript element is the current node; outerFamily: F of
 *   the main pass (M-09; null for none or a template); tableCurrentOver: details of M-09 condition 2;
 *   content: the content rules for the tokens of C (item 10; a null tag is a DOCTYPE token)
 */
export function checkNoscript(c, f, ctx) {
  const r = checkNoscriptContent(c, ctx.outer ?? null);
  if (r.tablePart) f.add("M-11.6", ctx.inCell ? "noscript in a cell or caption" : "noscript elsewhere");
  if (r.maxD > 256) f.add("M-26-noscript", `D ${r.maxD}`);
  const text = `${c}</noscript>`;
  /** @type {string[]} names of the open elements of C */
  const stack = [];
  let region = -1; // index of the region root of C (item 9)
  let restrict = -1; // index of the foreign INTEGRATION or TEXT element whose content is text only (M-07 items 3, 4)
  let end = -1;
  let unbalanced = false;
  /** @type {string | null} */
  let firstIssue = null;
  const restrictClass = () => (TEXT.has(stack[restrict]) && !INTEGRATION.has(stack[restrict]) ? "M-07.4" : "M-07.3");
  /** F of M-09 for a start tag in HTML context of C: in C, else around the noscript element. */
  const family = () => {
    for (let i = stack.length - 1; i >= 0; i--) {
      if (stack[i] === "template") return { name: null, current: false };
      if (TABLE_FAMILY.has(stack[i])) return { name: stack[i], current: i === stack.length - 1 };
    }
    return { name: ctx.outerFamily ?? null, current: false };
  };
  const popTo = (i) => {
    stack.length = i;
    if (restrict >= stack.length) restrict = -1;
    if (region >= stack.length) region = -1;
  };
  const handler = {
    onStartTag(t) {
      if (end >= 0) return;
      const name = t.tagName;
      const sc = t.selfClosing;
      if (restrict >= 0) f.add(restrictClass(), `<${safeName(name)}> in noscript`);
      if (t.attrs.some((a) => a.name === "data-rq-block")) f.add("M-11.2");
      if (NOSCRIPT_FORBIDDEN.has(name)) f.add("M-11.3", `<${name}>`);
      if (DOCUMENT_TAGS.has(name)) f.add("M-11.7", `<${name}>`);
      if (name === "script") {
        const item = scriptTypeItem(rawAttrValue(text, t.location, "type"));
        if (item !== null) f.add("M-11.4", `M-13 item ${item}`);
      }
      if (region >= 0) {
        // Foreign region of C: M-05 item 2, no state change (M-04); M-07 items 1 and 4 through item 8.
        if (BREAKOUT.has(name)) f.add("M-07.1", `<${name}> in noscript`);
        if (TEXT.has(name) && (sc || name === "plaintext")) f.add("M-07.4", `<${name}${sc ? "/" : ""}> in noscript`);
        if (!sc) {
          stack.push(name);
          if (restrict < 0 && (INTEGRATION.has(name) || TEXT.has(name))) restrict = stack.length - 1;
        }
        return;
      }
      // Item 10 (H-95): the content rules on a start tag in HTML context of C, before its push.
      ctx.content?.({ name, selfClosing: sc, attrNames: t.attrs.map((a) => a.name) }, stack);
      const parents = allowedParents(name);
      if (parents !== null || name === "table") {
        const fam = family();
        if (parents !== null && fam.name !== null && !parents.includes(fam.name)) f.add("M-09", `<${name}> in <${fam.name}> in noscript`);
        // Detail of M-09 condition 2 (the class comes from the counter of item 8).
        if (fam.name !== null && ROW_CONTEXT.has(fam.name) && !fam.current) {
          ctx.tableCurrentOver?.add(`<${safeName(stack.at(-1) ?? "noscript")}> in noscript`);
        }
      }
      setState(tok, name);
      if (!((name === "svg" || name === "math") && sc) && !VOID.has(name)) {
        stack.push(name);
        if (name === "svg" || name === "math") region = stack.length - 1;
      }
    },
    onEndTag(t) {
      if (end >= 0) return;
      const name = t.tagName;
      if (name === "noscript") {
        end = t.location.startOffset;
        return;
      }
      if (DOCUMENT_TAGS.has(name)) f.add("M-11.7", `</${name}>`);
      if (restrict >= 0 && name !== stack[restrict]) f.add(restrictClass(), `</${safeName(name)}> in noscript`);
      const i = stack.lastIndexOf(name);
      // Item 5: the end tag closes the last open element of C, or (H-75) it is the end tag of the
      // region root, which closes the elements of the region above it.
      if (i >= 0 && (i === stack.length - 1 || i === region)) {
        popTo(i);
        return;
      }
      if (region >= 0 && i < region) f.add("M-07.2", `</${safeName(name)}> in noscript`);
      if (!unbalanced) firstIssue = `</${safeName(name)}> over <${safeName(stack.at(-1) ?? "none")}>`;
      unbalanced = true;
      if (i >= 0) popTo(i);
    },
    onComment(t) {
      if (end >= 0) return;
      const start = t.location.startOffset;
      const cdata = text.startsWith("<![CDATA[", start);
      if (cdata && !cdataOk(text, start)) f.add("M-12", "in noscript");
      if (restrict >= 0 && !cdata) f.add(restrictClass(), "comment in noscript");
    },
    onDoctype() {
      if (end >= 0) return;
      if (restrict >= 0) f.add(restrictClass(), "doctype in noscript");
      ctx.content?.(null, stack); // item 10: M-22 item 1
    },
    onCharacter: noop,
    onNullCharacter: noop,
    onWhitespaceCharacter: noop,
    onEof: noop,
  };
  const tok = new Tokenizer({ sourceCodeLocationInfo: true }, handler);
  tok.write(text, true);
  if (end !== c.length) f.add("M-11.1");
  if (unbalanced || stack.length > 0) {
    f.add(onlyPLi(text, end) ? "M-11.5-pli" : "M-11.5-other", firstIssue ?? `<${safeName(stack.at(-1) ?? "none")}> not closed`);
  }
}

const P_LI = names("p li");

/**
 * True when the noscript content is balanced if `p` and `li` elements may stay unclosed
 * (an end tag can cross them, and they can be open at the end).
 * @param {string} text C followed by "</noscript>"
 * @param {number} end offset of the first noscript end tag, or -1
 */
function onlyPLi(text, end) {
  const stack = [];
  let region = -1; // the region root of C: no state change after it, and its end tag can cross (H-74, H-75)
  let ok = true;
  const handler = {
    onStartTag(t) {
      if (t.location.startOffset >= end && end >= 0) return;
      if (region >= 0) {
        if (!t.selfClosing) stack.push(t.tagName);
        return;
      }
      setState(tok, t.tagName);
      if (!VOID.has(t.tagName) && !((t.tagName === "svg" || t.tagName === "math") && t.selfClosing)) {
        stack.push(t.tagName);
        if (t.tagName === "svg" || t.tagName === "math") region = stack.length - 1;
      }
    },
    onEndTag(t) {
      if (end >= 0 && t.location.startOffset >= end) return;
      const i = stack.lastIndexOf(t.tagName);
      if (i < 0 || (i !== region && stack.slice(i + 1).some((n) => !P_LI.has(n)))) ok = false;
      if (i >= 0) stack.length = i;
      if (region >= stack.length) region = -1;
    },
    onComment: noop,
    onDoctype: noop,
    onCharacter: noop,
    onNullCharacter: noop,
    onWhitespaceCharacter: noop,
    onEof: noop,
  };
  const tok = new Tokenizer({ sourceCodeLocationInfo: true }, handler);
  tok.write(text, true);
  return ok && stack.every((n) => P_LI.has(n));
}

/**
 * @typedef {object} Pick
 * @property {"main"|"h1"|"match"} kind
 * @property {string} name
 * @property {number} insertAt  byte offset after the tag name (where the attribute goes)
 * @property {number} start     string offset of the start tag
 * @property {number} end       string offset where the element is popped (or the input length)
 */

/**
 * Classify one document.
 * @param {Uint8Array} bytes
 * @param {{ charset?: string | null, blocks?: boolean }} [options] blocks: also check the block rules
 */
export function classify(bytes, { charset = null, blocks = false } = {}) {
  const f = new Findings();
  const blockFindings = new Findings();
  // The patch content rule (H-78): classes that do not make a block invalid, only its patches.
  const patchFindings = new Findings();
  const out = { size: bytes.length, utf8: true, maxD: 0, tokens: 0, existingBlocks: 0, blocks: [] };
  if (bytes.length > MAX_INPUT_BYTES) {
    f.add("M-01");
    return { ...out, ...f.toJSON(), picks: [] };
  }
  const decoded = decodeInput(bytes, charset);
  out.utf8 = decoded.utf8;
  if (!decoded.utf8) f.add("M-02");
  const text = decoded.text;

  // The pass applies the implied closure of M-09 on its own stack and gives the end tags to the counter.
  const depth = new DepthCounter({ closure: false });
  /** @type {{ name: string, foreign: boolean, marks: Set<string>, start: number, pick: Pick | null, inContent: boolean }[]} */
  const stack = [];
  let root = -1; // index of the svg or math element of the foreign region
  let restrict = -1; // index of the foreign INTEGRATION or TEXT element whose content is text only
  let selects = 0;
  let templates = 0;
  let noscriptStart = -1;
  let noscriptInCell = false;
  /** @type {Set<string>} the current node for M-09 condition 2 (detail: "<name>", or "<name> in noscript") */
  const tableCurrentOver = new Set();
  let closureNotCurrent = false; // H-85a: M-09 condition 2 for an implied closure (the counter does not see it)
  /** @type {Pick[]} */
  const candidates = [];
  const counts = { main: 0, h1: 0, match: 0 };
  // Block state (blocks mode).
  /** @type {{ id: string, name: string, index: number, start: number, html: boolean } | null} */
  let block = null;
  const seenIds = new Set();

  const map = decoded.utf8 ? utf16ToUtf8Map(text, decoded.bomBytes) : null;
  // D without the dirty mark (DL-019 R1): the units of the elements that are still on the
  // token stack. A crossing then gives back the units of every popped element.
  let maxUnits = 0;
  const stackUnits = () => {
    if (depth.d <= maxUnits) return; // the units are never more than D
    let u = 0;
    for (const e of depth.stack) u += 1 + e.implied.size;
    if (u > maxUnits) maxUnits = u;
  };

  const tableContext = () => {
    for (let i = stack.length - 1; i >= 0; i--) {
      const n = stack[i].name;
      if (n === "template") return null;
      if (TABLE_FAMILY.has(n)) return stack[i];
    }
    return null;
  };
  const restrictClass = () => (TEXT.has(stack[restrict].name) && !INTEGRATION.has(stack[restrict].name) ? "M-07.4" : "M-07.3");
  // M-11 item 8 (H-71): the main-pass state while the noscript element is the current node.
  // M-11 item 10 (H-95): in the content of an html block, the content rules also apply to C.
  const noscriptContext = () => ({
    inCell: noscriptInCell,
    outer: depth,
    outerFamily: tableContext()?.name ?? null,
    tableCurrentOver,
    content: block !== null && block.html ? noscriptContentRules : null,
  });

  const popTo = (i) => {
    for (let k = stack.length - 1; k >= i; k--) {
      const e = stack[k];
      if (!e.foreign && e.name === "select") selects--;
      if (!e.foreign && e.name === "template") templates--;
      if (e.pick !== null) e.pick.end = endOffsetOfToken;
    }
    stack.length = i;
    if (root >= stack.length) root = -1;
    if (restrict >= stack.length) restrict = -1;
  };
  let endOffsetOfToken = 0;

  // H-85a: the implied closure of M-09 for a start tag in HTML context outside block content and C.
  // Each step is the end tag of F (M-05 item 3) with M-06 item 1 and the marks of M-06 item 4; M-06
  // items 2 and 3 cannot apply (F is not a formatting name and is on the stack). Above F there is no
  // table-family element (F is the nearest) and no foreign element (HTML context).
  const impliedClosure = (name) => {
    const allowed = allowedParents(name);
    if (allowed === null) return;
    for (let step = 0; step < MAX_CLOSURES; step++) {
      const fam = tableContext();
      if (fam === null || allowed.includes(fam.name)) return;
      const i = stack.lastIndexOf(fam);
      const top = stack[stack.length - 1];
      if (ROW_CONTEXT.has(fam.name) && top !== fam) {
        tableCurrentOver.add(`<${safeName(top.name)}>`);
        closureNotCurrent = true;
      }
      const crossed = stack.slice(i + 1);
      const guarded = crossed.find((e) => !e.foreign && GUARDED.has(e.name) && !TABLE_PART.has(e.name));
      if (guarded) f.add("M-06.1", `implied </${fam.name}> over <${guarded.name}>`);
      const origins = new Set();
      if (crossed.some((e) => !e.foreign && SPECIAL.has(e.name))) origins.add("special");
      for (const e of stack.slice(i)) for (const o of e.marks) origins.add(o);
      popTo(i);
      if (origins.size > 0 && stack.length > 0) for (const o of origins) stack[stack.length - 1].marks.add(o);
      depth.endTag(fam.name);
    }
  };

  // Block rules M-22 items 2, 3 and 6 and M-24 for a start tag in HTML context of the content of an
  // html block. `stk` is the token stack before the start tag: the stack of the pass, or (M-11 item
  // 10, H-95) that stack with the open elements of the noscript content C above it. `add(cls,
  // detail)` records a class. Class names: "M-22.2:<name>" for a name that M-22 keeps (H-78);
  // "M-22.6" for a template with a shadowrootmode attribute (H-94, H-97); "M-24.<item>". The detail
  // of M-24.g says "in template" when F is none because the search for F stops at a template (H-94).
  const contentRules = (name, selfClosing, attrNames, stk, add) => {
    const b = /** @type {NonNullable<typeof block>} */ (block);
    const isOpenIn = (n) => stk.some((e) => e.name === n && !e.foreign);
    const openIn = (set) => {
      for (let i = stk.length - 1; i > b.index; i--) if (set.has(stk[i].name)) return true;
      return false;
    };
    /** M-09: the nearest table-family entry; the search stops at a template ("template"). */
    const family = () => {
      for (let i = stk.length - 1; i >= 0; i--) {
        if (stk[i].name === "template") return "template";
        if (TABLE_FAMILY.has(stk[i].name)) return stk[i].name;
      }
      return null;
    };
    if (FORBIDDEN_CONTENT.has(name)) add(`M-22.2:${name}`, null);
    if (name === "template" && attrNames.includes(SHADOW_ROOT_MODE)) add("M-22.6", null);
    if (selfClosing && !VOID.has(name) && name !== "svg" && name !== "math") add("M-22.3", `<${safeName(name)}/>`);
    if (PCLOSE.has(name) && (b.name === "p" || b.name === "span")) add("M-24.a", `<${name}> in <${b.name}>`);
    if (HEADINGS.has(name) && (HEADINGS.has(b.name) || openIn(HEADINGS))) add("M-24.b", null);
    if (LIST_ITEMS.has(name) && !openIn(LISTS)) add("M-24.c", `<${name}>`);
    if (name === "button" && isOpenIn("button")) add("M-24.d", null);
    if (name === "a" && isOpenIn("a")) add("M-24.e", null);
    if (name === "nobr" && isOpenIn("nobr")) add("M-24.f", null);
    const fam = family();
    if (TABLE_PART.has(name) || name === "col") {
      // M-24 item g (H-94): a table opened in the content is open, and F is not none.
      if (fam === "template") add("M-24.g", `<${name}> in template`);
      else if (fam === null || !openIn(names("table"))) add("M-24.g", `<${name}>`);
    }
    if ((name === "rb" || name === "rtc") && (!openIn(names("ruby")) || openIn(names("rb rp rt rtc")))) add("M-24.h", null);
    if ((name === "rp" || name === "rt") && (!openIn(names("ruby")) || openIn(names("rb rp rt")))) add("M-24.i", null);
    if (LIST_ITEMS.has(name)) {
      for (let i = stk.length - 1; i > b.index; i--) {
        if (LISTS.has(stk[i].name)) break;
        if (LIST_ITEMS.has(stk[i].name)) {
          add("M-24.j", null);
          break;
        }
      }
    }
    if (!TABLE_PART.has(name) && name !== "col" && fam !== null && ROW_CONTEXT.has(fam)) add("M-24.k", `<${name}> in <${fam}>`);
  };

  // Block rules for a start tag in the content of an html block (contentRules), and the patch
  // content rule (H-78), which goes to patchFindings, not to f2: "patch.element:<name>" for a name
  // in PATCH_FORBIDDEN in HTML context (noscript: H-96), "patch.foreign" for a start tag in a
  // foreign region of the content whose name is not in FOREIGN_ALLOWED (H-73).
  const contentStartTag = (name, selfClosing, attrNames, f2, inForeign) => {
    if (inForeign) {
      if (!FOREIGN_ALLOWED.has(name)) patchFindings.add("patch.foreign", `<${safeName(name)}>`);
      return;
    }
    if (PATCH_FORBIDDEN.has(name)) patchFindings.add(`patch.element:${name}`);
    contentRules(name, selfClosing, attrNames, stack, (cls, detail) => f2.add(cls, detail));
  };

  // M-11 item 10 (H-95): the content rules on the tokens of the noscript content C when the
  // noscript element is in the content of an html block. The classes are "M-11.10" with the
  // content-rule class as the detail ("M-24.a", "M-22.1", ...).
  const noscriptContentRules = (tag, open) => {
    const add = (cls) => blockFindings.add("M-11.10", cls);
    if (tag === null) {
      add("M-22.1");
      return;
    }
    const stk = [...stack, ...open.map((name) => ({ name, foreign: false }))];
    contentRules(tag.name, tag.selfClosing, tag.attrNames, stk, add);
  };

  const blockStart = (t, name, inForeign) => {
    const raw = rawAttrValue(text, t.location, "data-rq-block") ?? "";
    out.existingBlocks++;
    const bf = blockFindings;
    if (block !== null && block.html) {
      bf.add("M-17");
      return null;
    }
    if (!BLOCK_ID.test(raw)) bf.add("M-15");
    const json = name === "script" && JSON_TYPES.has(rawAttrValue(text, t.location, "type") ?? "");
    if (inForeign || (!BLOCK_HTML.has(name) && !json)) bf.add("M-16", `<${name}>`);
    if (t.selfClosing) bf.add("M-18");
    if (templates > 0 || selects > 0) bf.add("M-19.1");
    const fam = tableContext();
    if (fam !== null && !["td", "th", "caption"].includes(fam.name)) bf.add("M-19.2", `F=<${fam.name}>`);
    if (seenIds.has(raw)) bf.add("M-21");
    seenIds.add(raw);
    return { id: raw, name, html: !json && name !== "title" && BLOCK_HTML.has(name) && !inForeign, start: t.location.endOffset };
  };

  const handler = {
    onStartTag(t) {
      out.tokens++;
      const name = t.tagName;
      const sc = t.selfClosing;
      endOffsetOfToken = t.location.endOffset;
      const inForeign = root >= 0;
      const attrNames = t.attrs.map((a) => a.name);
      if (restrict >= 0) f.add(restrictClass(), `<${name}> in <${stack[restrict].name}>`);
      if (selects > 0 && !inForeign && !SELECT_START.has(name)) f.add("M-08", `<${name}> in select`);
      if (name === "frameset") f.add("M-10");
      if (name === "script") {
        const item = scriptTypeItem(rawAttrValue(text, t.location, "type"));
        if (item !== null) f.add(`M-13.${item}`);
      }
      let newBlock = null;
      const isBlockTag = t.attrs.some((a) => a.name === "data-rq-block");
      if (block !== null && block.html) {
        if (isBlockTag) blockFindings.add("M-17");
        else contentStartTag(name, sc, attrNames, blockFindings, inForeign);
      } else if (isBlockTag && blocks) {
        newBlock = blockStart(t, name, inForeign);
      } else if (isBlockTag) {
        out.existingBlocks++;
      }

      let pushed = false;
      if (inForeign) {
        if (BREAKOUT.has(name)) f.add("M-07.1", `<${name}>`);
        if (TEXT.has(name) && (sc || name === "plaintext")) f.add("M-07.4", sc ? `<${name}/>` : "<plaintext>");
        depth.startTag(name, sc, attrNames);
        stackUnits();
        if (!sc) {
          stack.push({ name, foreign: true, marks: new Set(), start: t.location.startOffset, pick: null });
          pushed = true;
          if (restrict < 0 && (INTEGRATION.has(name) || TEXT.has(name))) restrict = stack.length - 1;
        }
      } else {
        if (block === null || !block.html) impliedClosure(name);
        const fam = tableContext();
        if (TABLE_PART.has(name) || name === "col") {
          const allowed =
            name === "col" ? ["table", "colgroup"] : name === "tr" ? ["table", "tbody", "thead", "tfoot"] : name === "td" || name === "th" ? ["table", "tbody", "thead", "tfoot", "tr"] : ["table"];
          if (fam !== null && !allowed.includes(fam.name)) f.add("M-09", `<${name}> in <${fam.name}>`);
        }
        // Detail for M-09 condition 2 (the class itself comes from DepthCounter).
        if ((TABLE_PART.has(name) || name === "col" || name === "table") && fam !== null && ROW_CONTEXT.has(fam.name) && stack[stack.length - 1] !== fam) {
          tableCurrentOver.add(`<${safeName(stack[stack.length - 1].name)}>`);
        }
        depth.startTag(name, sc, attrNames);
        stackUnits();
        setState(tok, name);
        if (name === "noscript") {
          noscriptStart = t.location.endOffset;
          noscriptInCell = fam !== null && ["td", "th", "caption"].includes(fam.name);
        }
        if (!((name === "svg" || name === "math") && sc) && !VOID.has(name)) {
          let pick = null;
          if (BLOCK_HTML.has(name) && name !== "title" && !sc && block === null) {
            const id = rawAttrValue(text, t.location, "id") ?? "";
            const cls = rawAttrValue(text, t.location, "class") ?? "";
            const kind = name === "main" ? "main" : name === "h1" ? "h1" : PICK_MATCH.test(id) || PICK_MATCH.test(cls) ? "match" : null;
            if (kind !== null && counts[kind] < 200) {
              counts[kind]++;
              pick = { kind, name, insertAt: -1, start: t.location.startOffset, end: text.length };
              if (map !== null) pick.insertAt = map[t.location.startOffset + 1 + name.length];
              candidates.push(pick);
            }
          }
          stack.push({ name, foreign: false, marks: new Set(), start: t.location.startOffset, pick });
          pushed = true;
          if (name === "select") selects++;
          if (name === "template") templates++;
          if (name === "svg" || name === "math") root = stack.length - 1;
        }
      }
      if (newBlock !== null) {
        if (pushed && newBlock.html) block = { ...newBlock, index: stack.length - 1 };
        else if (pushed) block = { ...newBlock, index: stack.length - 1 };
        else blockFindings.add("M-20", "not pushed");
      }
      out.maxD = depth.maxD;
    },

    onEndTag(t) {
      out.tokens++;
      const name = t.tagName;
      endOffsetOfToken = t.location.endOffset;
      const inForeign = root >= 0;
      if (restrict >= 0 && name !== stack[restrict].name) f.add(restrictClass(), `</${name}> in <${stack[restrict].name}>`);
      if (selects > 0 && !inForeign && !SELECT_END.has(name)) f.add("M-08", `</${name}> in select`);
      if (name === "noscript" && noscriptStart >= 0) {
        checkNoscript(text.slice(noscriptStart, t.location.startOffset), f, noscriptContext());
        noscriptStart = -1;
      }
      let i = stack.length - 1;
      while (i >= 0 && stack[i].name !== name) i--;
      // H-75: the end tag of the region root crosses only elements of its region.
      const rootEnd = inForeign && i === root;
      if (block !== null && block.html) {
        if (VOID.has(name) && !inForeign) blockFindings.add("M-22.4", `</${name}>`);
        else if (i !== stack.length - 1 && !(rootEnd && root > block.index)) {
          blockFindings.add("M-23", i < 0 ? `stray </${safeName(name)}>` : `</${safeName(name)}> over <${safeName(stack[stack.length - 1].name)}>`);
        }
        if (i >= 0 && i <= block.index && i !== stack.length - 1) blockFindings.add("M-20", i === block.index ? "closed by a crossing" : "closed by an outer end tag");
      }
      if (inForeign && (i < 0 || i < root)) f.add("M-07.2", i < 0 ? `stray </${name}>` : `</${name}> leaves the region`);
      if (i < 0) {
        if (!inForeign && STRAY_TABLE_END.has(name)) f.add(name === "table" ? "M-06.3-table" : "M-06.3-row", `</${name}>`);
        depth.endTag(name);
        return;
      }
      const crossed = stack.slice(i + 1);
      const popped = stack.slice(i);
      for (const e of crossed) {
        if (!e.foreign && GUARDED.has(e.name) && !(TABLE_FAMILY.has(name) && TABLE_PART.has(e.name))) {
          f.add("M-06.1", `</${name}> over <${e.name}>`);
          break;
        }
      }
      const special = crossed.find((e) => !e.foreign && SPECIAL.has(e.name));
      const foreign = crossed.find((e) => e.foreign);
      if (FORMATTING.has(name)) {
        if (special || foreign) f.add("M-06.2", special ? `</${name}> over <${special.name}>` : `</${name}> over foreign <${foreign?.name}>`);
        const marked = popped.filter((e) => e.marks.size > 0);
        if (marked.length > 0) {
          const origins = new Set(marked.flatMap((e) => [...e.marks]));
          // "foreign": every mark on the popped elements comes from crossings over foreign
          // elements only (the known limit of markup.md section 5 item 3).
          f.add(origins.has("special") ? "M-06.4-special" : "M-06.4-foreign", `</${name}>`);
        }
      }
      const origins = new Set();
      if (special) origins.add("special");
      if (foreign && !rootEnd) origins.add("foreign"); // H-75: the crossing of the region root sets no mark
      for (const e of popped) for (const o of e.marks) origins.add(o);
      const blockEnded = block !== null && i === block.index;
      popTo(i);
      if (origins.size > 0 && stack.length > 0) for (const o of origins) stack[stack.length - 1].marks.add(o);
      if (blockEnded && block !== null) {
        out.blocks.push({ id: block.id, start: block.start, end: t.location.startOffset });
        block = null;
      } else if (block !== null && stack.length <= block.index) {
        block = null;
      }
      depth.endTag(name);
    },

    onComment(t) {
      out.tokens++;
      const start = t.location.startOffset;
      const cdata = text.startsWith("<![CDATA[", start);
      if (cdata && !cdataOk(text, start)) f.add("M-12");
      if (restrict >= 0 && !cdata) f.add(restrictClass(), "comment");
    },
    onDoctype() {
      out.tokens++;
      if (restrict >= 0) f.add(restrictClass(), "doctype");
      if (block !== null && block.html) blockFindings.add("M-22.1");
    },
    onCharacter: noop,
    onNullCharacter: noop,
    onWhitespaceCharacter: noop,
    onEof() {
      if (noscriptStart >= 0) {
        checkNoscript(text.slice(noscriptStart), f, noscriptContext());
        noscriptStart = -1;
      }
      if (selects > 0) f.add("M-08", "no </select>");
      if (block !== null) blockFindings.add("M-20", "end of input");
    },
  };

  const tok = new Tokenizer({ sourceCodeLocationInfo: true }, handler);
  tok.write(text, true);
  out.maxD = depth.maxD;
  out.maxUnits = maxUnits;
  if (depth.maxD > 256) f.add(maxUnits > 256 ? "M-26-stack" : "M-26-dirty", `D ${depth.maxD}, units ${maxUnits}`);
  // Round 2 of the security review and H-43: the classes of the cross-check oracle.
  if (depth.classes.has(CLASSES.templateCol)) f.add("M-09.1");
  if (depth.classes.has(CLASSES.tableCurrent) || closureNotCurrent) {
    if (tableCurrentOver.size === 0) f.add("M-09.2", "<?>");
    for (const n of tableCurrentOver) f.add("M-09.2", n);
  }
  if (depth.classes.has(CLASSES.form)) f.add("M-10.2");
  if (depth.classes.has(CLASSES.templateFor)) f.add("M-10.3");
  const main = f.toJSON();
  return {
    ...out,
    classes: main.classes,
    details: main.details,
    blockRules: blocks ? blockFindings.toJSON() : null,
    patchRules: blocks ? patchFindings.toJSON() : null,
    picks: candidates,
  };
}

/**
 * Choose the simulated blocks (DESIGN estimate, T-15 step 5).
 * Variant "small": the first h1, then up to two elements whose id or class contains
 * price, title or content; no pick overlaps another. Variant "main": the first main.
 * @param {Pick[]} picks
 * @returns {{ small: Pick[], main: Pick[] }}
 */
export function choosePicks(picks) {
  const usable = picks.filter((p) => p.insertAt >= 0);
  const overlaps = (a, b) => a.start < b.end && b.start < a.end;
  const small = [];
  const h1 = usable.find((p) => p.kind === "h1");
  if (h1) small.push(h1);
  for (const p of usable) {
    if (small.length >= 3) break;
    if (p.kind !== "match") continue;
    if (small.some((q) => overlaps(p, q))) continue;
    small.push(p);
  }
  small.sort((a, b) => a.start - b.start);
  const main = usable.filter((p) => p.kind === "main").slice(0, 1);
  return { small, main };
}

/**
 * Insert ` data-rq-block="<id>"` after the tag name of each pick (byte level).
 * Returns null when a pick does not point after a tag name (safety check).
 * @param {Uint8Array} bytes
 * @param {Pick[]} picks
 * @param {string} prefix
 */
export function insertBlocks(bytes, picks, prefix) {
  const sorted = [...picks].sort((a, b) => a.insertAt - b.insertAt);
  const parts = [];
  let at = 0;
  for (let k = 0; k < sorted.length; k++) {
    const off = sorted[k].insertAt;
    const next = bytes[off];
    // After a tag name: white space, "/" or ">".
    if (!(next === 0x20 || next === 0x09 || next === 0x0a || next === 0x0c || next === 0x0d || next === 0x2f || next === 0x3e)) return null;
    parts.push(bytes.subarray(at, off), Buffer.from(` data-rq-block="${prefix}${k + 1}"`, "latin1"));
    at = off;
  }
  parts.push(bytes.subarray(at));
  return Buffer.concat(parts);
}

function main(argv) {
  let file = null;
  let charset = null;
  let blocks = false;
  for (let i = 0; i < argv.length; i++) {
    if (argv[i] === "--charset") charset = argv[++i] ?? null;
    else if (argv[i] === "--blocks") blocks = true;
    else file = argv[i];
  }
  if (file === null) {
    process.stderr.write("usage: classify.mjs <file> [--charset label] [--blocks]\n");
    process.exit(2);
  }
  const bytes = readFileSync(file);
  process.stdout.write(`${JSON.stringify(classify(bytes, { charset, blocks }))}\n`);
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    main(process.argv.slice(2));
  } catch (err) {
    process.stderr.write(`classify: ${err instanceof Error ? err.message : String(err)}\n`);
    process.exit(2);
  }
}
