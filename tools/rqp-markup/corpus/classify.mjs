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
// It also measures, as RESEARCH, the rules of the human decisions H-35 … H-38 (not in the
// candidates yet), finds elements for the simulated blocks, and (with blocks) checks the
// block rules M-14 … M-24 with their items.
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
import { DepthCounter, FORMATTING, SPECIAL, VOID, rawAttrValue } from "../crosscheck/src/depth.mjs";
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
const FORBIDDEN_CONTENT = names(`base basefont bgsound body embed frame frameset head html iframe link math meta noembed
  noframes noscript object optgroup option plaintext script select style svg template textarea title xmp`);
const STRAY_TABLE_END = names("tbody thead tfoot tr table");
const SELECT_START = names("option optgroup hr");
const SELECT_END = names("option optgroup select");
const NOSCRIPT_FORBIDDEN = names("frameset math noscript plaintext select svg template");
const ROW_CONTEXT = names("table tbody thead tfoot tr colgroup");
const LISTS = names("ul ol menu dl");
const LIST_ITEMS = names("li dd dt");
const BLOCK_ID = /^[a-z0-9][a-z0-9_-]{0,63}$/;
const PICK_MATCH = /price|title|content/i;

/** The rule id of a class: "M-06.1" → "M-06", "M-26-dirty" → "M-26", "M-22.2:svg" → "M-22". */
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
  if (type.includes("&")) return "2";
  if (hasNonAscii(type)) return "3";
  return null;
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
 * M-11 (items 1 to 5) for the noscript content C, plus the H-38 measurement.
 * @param {string} c
 * @param {Findings} f
 * @param {{ inCell: boolean }} ctx
 * @param {Findings} research
 */
export function checkNoscript(c, f, ctx, research) {
  const text = `${c}</noscript>`;
  const stack = [];
  let end = -1;
  let unbalanced = false;
  /** @type {string | null} */
  let firstIssue = null;
  const handler = {
    onStartTag(t) {
      if (end >= 0) return;
      const name = t.tagName;
      if (t.attrs.some((a) => a.name === "data-rq-block")) f.add("M-11.2");
      if (NOSCRIPT_FORBIDDEN.has(name)) f.add("M-11.3", `<${name}>`);
      if (name === "script") {
        const item = scriptTypeItem(rawAttrValue(text, t.location, "type"));
        if (item !== null) f.add("M-11.4", `M-13 item ${item}`);
      }
      if ((TABLE_PART.has(name) || name === "col") && !stack.includes("table")) {
        research.add("H-38", `<${name}>`);
        if (ctx.inCell) research.add("H-38-cell", `<${name}>`);
      }
      setState(tok, name);
      if (!VOID.has(name) && !((name === "svg" || name === "math") && t.selfClosing)) stack.push(name);
    },
    onEndTag(t) {
      if (end >= 0) return;
      if (t.tagName === "noscript") {
        end = t.location.startOffset;
        return;
      }
      if (stack.length > 0 && stack[stack.length - 1] === t.tagName) {
        stack.pop();
        return;
      }
      if (!unbalanced) firstIssue = `</${t.tagName}> over <${stack.at(-1) ?? "none"}>`;
      unbalanced = true;
      const i = stack.lastIndexOf(t.tagName);
      if (i >= 0) stack.length = i;
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
  if (end !== c.length) f.add("M-11.1");
  if (unbalanced || stack.length > 0) {
    f.add(onlyPLi(text, end) ? "M-11.5-pli" : "M-11.5-other", firstIssue ?? `<${stack.at(-1)}> not closed`);
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
  let ok = true;
  const handler = {
    onStartTag(t) {
      if (t.location.startOffset >= end && end >= 0) return;
      setState(tok, t.tagName);
      if (!VOID.has(t.tagName) && !((t.tagName === "svg" || t.tagName === "math") && t.selfClosing)) stack.push(t.tagName);
    },
    onEndTag(t) {
      if (end >= 0 && t.location.startOffset >= end) return;
      const i = stack.lastIndexOf(t.tagName);
      if (i < 0 || stack.slice(i + 1).some((n) => !P_LI.has(n))) ok = false;
      if (i >= 0) stack.length = i;
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
  const research = new Findings();
  const blockFindings = new Findings();
  const out = { size: bytes.length, utf8: true, maxD: 0, tokens: 0, existingBlocks: 0, blocks: [] };
  if (bytes.length > MAX_INPUT_BYTES) {
    f.add("M-01");
    return { ...out, ...f.toJSON(), research: research.toJSON().classes, researchDetails: research.toJSON().details, picks: [] };
  }
  const decoded = decodeInput(bytes, charset);
  out.utf8 = decoded.utf8;
  if (!decoded.utf8) f.add("M-02");
  const text = decoded.text;

  const depth = new DepthCounter();
  /** @type {{ name: string, foreign: boolean, marks: Set<string>, start: number, pick: Pick | null, inContent: boolean }[]} */
  const stack = [];
  let root = -1; // index of the svg or math element of the foreign region
  let restrict = -1; // index of the foreign INTEGRATION or TEXT element whose content is text only
  let selects = 0;
  let templates = 0;
  let forms = 0;
  let noscriptStart = -1;
  let noscriptInCell = false;
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
  const nearestTableOrTemplate = () => {
    for (let i = stack.length - 1; i >= 0; i--) {
      const n = stack[i].name;
      if (n === "template" || TABLE_FAMILY.has(n)) return n;
    }
    return null;
  };
  const restrictClass = () => (TEXT.has(stack[restrict].name) && !INTEGRATION.has(stack[restrict].name) ? "M-07.4" : "M-07.3");
  const isOpen = (n) => stack.some((e) => e.name === n && !e.foreign);
  const openInContent = (set) => {
    for (let i = stack.length - 1; i > (block?.index ?? stack.length); i--) if (set.has(stack[i].name)) return true;
    return false;
  };

  const popTo = (i) => {
    for (let k = stack.length - 1; k >= i; k--) {
      const e = stack[k];
      if (!e.foreign && e.name === "select") selects--;
      if (!e.foreign && e.name === "template") templates--;
      if (!e.foreign && e.name === "form") forms--;
      if (e.pick !== null) e.pick.end = endOffsetOfToken;
    }
    stack.length = i;
    if (root >= stack.length) root = -1;
    if (restrict >= stack.length) restrict = -1;
  };
  let endOffsetOfToken = 0;

  // Block rules M-22 to M-24 for a start tag in the content of an html block.
  // Class names: "M-22.2:<name>" for a forbidden element in HTML context; "M-22-foreign" for
  // a forbidden or self-closing tag inside an svg or math subtree of the content (a
  // consequence of "M-22.2:svg" or "M-22.2:math").
  const contentStartTag = (name, selfClosing, f2, inForeign) => {
    if (inForeign) {
      if (FORBIDDEN_CONTENT.has(name) || (selfClosing && !VOID.has(name))) f2.add("M-22-foreign", `<${name}${selfClosing ? "/" : ""}>`);
      return;
    }
    if (FORBIDDEN_CONTENT.has(name)) f2.add(`M-22.2:${name}`);
    if (selfClosing && !VOID.has(name)) f2.add("M-22.3", `<${name}/>`);
    const b = /** @type {NonNullable<typeof block>} */ (block);
    if (PCLOSE.has(name) && (b.name === "p" || b.name === "span")) f2.add("M-24.a", `<${name}> in <${b.name}>`);
    if (HEADINGS.has(name) && (HEADINGS.has(b.name) || openInContent(HEADINGS))) f2.add("M-24.b");
    if (LIST_ITEMS.has(name) && !openInContent(LISTS)) f2.add("M-24.c", `<${name}>`);
    if (name === "button" && isOpen("button")) f2.add("M-24.d");
    if (name === "a" && isOpen("a")) f2.add("M-24.e");
    if (name === "nobr" && isOpen("nobr")) f2.add("M-24.f");
    if ((TABLE_PART.has(name) || name === "col") && !openInContent(names("table"))) f2.add("M-24.g", `<${name}>`);
    if ((name === "rb" || name === "rtc") && (!openInContent(names("ruby")) || openInContent(names("rb rp rt rtc")))) f2.add("M-24.h");
    if ((name === "rp" || name === "rt") && (!openInContent(names("ruby")) || openInContent(names("rb rp rt")))) f2.add("M-24.i");
    if (LIST_ITEMS.has(name)) {
      for (let i = stack.length - 1; i > b.index; i--) {
        if (LISTS.has(stack[i].name)) break;
        if (LIST_ITEMS.has(stack[i].name)) {
          f2.add("M-24.j");
          break;
        }
      }
    }
    if (!TABLE_PART.has(name) && name !== "col") {
      const fam = tableContext();
      if (fam !== null && ROW_CONTEXT.has(fam.name)) f2.add("M-24.k", `<${name}> in <${fam.name}>`);
    }
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
    const json = name === "script" && rawAttrValue(text, t.location, "type") === "application/json";
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
        else contentStartTag(name, sc, blockFindings, inForeign);
      } else if (isBlockTag && blocks) {
        newBlock = blockStart(t, name, inForeign);
      } else if (isBlockTag) {
        out.existingBlocks++;
      }

      let pushed = false;
      if (inForeign) {
        if (BREAKOUT.has(name)) f.add("M-07.1", `<${name}>`);
        if (TEXT.has(name) && (sc || name === "plaintext")) f.add("M-07.4", sc ? `<${name}/>` : "<plaintext>");
        depth.startTag(name, sc);
        stackUnits();
        if (!sc) {
          stack.push({ name, foreign: true, marks: new Set(), start: t.location.startOffset, pick: null });
          pushed = true;
          if (restrict < 0 && (INTEGRATION.has(name) || TEXT.has(name))) restrict = stack.length - 1;
        }
      } else {
        const fam = tableContext();
        if (TABLE_PART.has(name) || name === "col") {
          const allowed =
            name === "col" ? ["table", "colgroup"] : name === "tr" ? ["table", "tbody", "thead", "tfoot"] : name === "td" || name === "th" ? ["table", "tbody", "thead", "tfoot", "tr"] : ["table"];
          if (fam !== null && !allowed.includes(fam.name)) f.add("M-09", `<${name}> in <${fam.name}>`);
        }
        if (name === "col" && nearestTableOrTemplate() === "template") research.add("H-35");
        if (name === "form" && forms > 0) research.add("H-36");
        if ((TABLE_PART.has(name) || name === "col" || name === "table") && fam !== null && ROW_CONTEXT.has(fam.name) && stack[stack.length - 1] !== fam) {
          const top = stack[stack.length - 1].name;
          research.add("H-37", `<${name}> over <${top}>`);
          research.add(top === "form" ? "H-37-form" : "H-37-other", `<${top}>`);
        }
        depth.startTag(name, sc);
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
          if (name === "form") forms++;
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
        checkNoscript(text.slice(noscriptStart, t.location.startOffset), f, { inCell: noscriptInCell }, research);
        noscriptStart = -1;
      }
      let i = stack.length - 1;
      while (i >= 0 && stack[i].name !== name) i--;
      if (block !== null && block.html) {
        if (VOID.has(name)) blockFindings.add("M-22.4", `</${name}>`);
        else if (i !== stack.length - 1) blockFindings.add("M-23", i < 0 ? `stray </${name}>` : `</${name}> over <${stack[stack.length - 1].name}>`);
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
      if (foreign) origins.add("foreign");
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
        checkNoscript(text.slice(noscriptStart), f, { inCell: noscriptInCell }, research);
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
  const main = f.toJSON();
  const rs = research.toJSON();
  return {
    ...out,
    classes: main.classes,
    details: main.details,
    research: rs.classes,
    researchDetails: rs.details,
    blockRules: blocks ? blockFindings.toJSON() : null,
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
