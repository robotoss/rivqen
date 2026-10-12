// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// Seeded generator of small HTML documents for the differential sweep.
// The documents are synthetic: tag soup with blocks, lists, tables, foreign content,
// raw text, comments, CDATA and processing instructions. No real data.
// Document i of seed s is the same on every run and every platform.

/**
 * mulberry32: a small 32-bit PRNG. Deterministic; not for cryptography.
 * @param {number} seed
 * @returns {() => number} uniform in [0, 1)
 */
export function makeRng(seed) {
  let a = seed >>> 0;
  return () => {
    a = (a + 0x6d2b79f5) >>> 0;
    let t = a;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

/** Seed of document `index` in a sweep with `seed`. */
export function docSeed(seed, index) {
  return (Math.imul(seed >>> 0, 0x9e3779b1) ^ Math.imul(index + 1, 0x85ebca6b)) >>> 0;
}

const BLOCK_NAMES = ["div", "p", "span", "section", "h1", "h2", "article"];
const OPEN_NAMES = [
  ...BLOCK_NAMES,
  "ul", "ol", "dl", "menu", "li", "li", "dd", "dt",
  "table", "table", "tbody", "thead", "tr", "tr", "td", "td", "th", "caption", "colgroup",
  "a", "b", "i", "em", "font", "button", "nobr", "ruby", "rt", "rp", "rb", "address", "form", "object",
  "select", "option", "optgroup", "template",
  "svg", "math", "desc", "title", "foreignObject", "g", "mi", "mtext", "annotation-xml", "style", "frameset",
];
const VOID_PIECES = ["<br>", "<img src=x>", "<hr>", "<col>", "<input type=hidden>", "<wbr>", "<path d=M0/>"];
const TEXTS = ["x", " ", "\n", "a < b", "1 <2", "&amp;", "\r\n", "\u{1F600}", "é", "<", "&lt;p&gt;", "]]>"];
const SPECIALS = [
  "<!--c-->", "<!-->", "<!--->", "<!--a--!>", "<!x>", "</ x>", "</>", "<?x>", "<?pi a?>",
  "<![CDATA[a]]>", "<![CDATA[a<b]]>", "<![CDATA[a>b]]>", "<!DOCTYPE html>",
  '<script type="application/rivqen-manifest+json">{}</script>',
  '<script type="application/json" data-rq-block="j0">{"a":[1]}</script>',
  '<script type="application/json" data-rq-block="j1">[1,]</script>',
  '<title data-rq-block="t0">T</title>',
  "<script>a</script>", "<style>p{}</style>", "<textarea></div></textarea>", "<xmp><p></xmp>",
];
const RAW_INNER = [
  "<img src=x>", "<iframe src=x></iframe>", '<p data-rq-block="n0">y</p>', "<!--", "<![CDATA[",
  '<script type="application/rivqen-manifest+json">{}</script>', "text", "<p>", "</p>", "<?x",
  "<svg>", "<frameset>", "<select>", "<style>", "<noscript>",
];
const EOF_TAILS = [
  "<![CDATA[x", "<?x", "<?", "<!--x", "<noscript>x", "<noscript><img src=x>", "<svg><desc>x", "<svg><title>x",
  "<svg><g>", "<math><mi>x", "<svg><style>x", "<x",
];

/**
 * @param {() => number} rng
 * @template T
 * @param {T[]} list
 * @returns {T}
 */
function pick(rng, list) {
  return list[Math.floor(rng() * list.length)];
}

/** @param {() => number} rng @param {number} lo @param {number} hi @returns {number} integer in [lo, hi] */
function between(rng, lo, hi) {
  return lo + Math.floor(rng() * (hi - lo + 1));
}

const FORMATTING_PIECES = ["a", "nobr", "b", "i", "em", "font"];
const SPECIAL_PIECES = ["div", "p", "li", "section", "ul", "form", "h1", "address", "dd"];
const TABLE_END_PIECES = ["</tbody>", "</tr>", "</thead>", "</tfoot>", "</table>", "</td>", "</th>", "</caption>", "</colgroup>", "</col>"];
const NOSCRIPT_UNBALANCED = [
  "<button>", "<table>", "<b>", "<div>", "</div>", "<p>x", "<a href=x><div>", "<li><ul></li>", "<span><div></span>",
  "<table><td>", "<div><span></div>", "<select>", "<i></b>", "<ul><li>a",
];
const NOSCRIPT_BALANCED = [
  "<div><a href=x>a</a></div>", "<table><tr><td>a</td></tr></table>", "<p><b>x</b></p>", "<ul><li>a</li></ul>",
  "<img src=x>", "<iframe src=x></iframe>", "<form><input></form>", "<h1><span>x</span></h1>",
];
const TYPE_LOOKALIKES = [
  "application/rıvqen-manifest+json", // dotless i
  "application/rivqen-manifeſt+json", // long s
  "application/RIVQEN-MANIFEST+JSON",
  "text/jävascript",
  "application/json",
];

/**
 * Review motifs (security review of WP-17 S1, decisions H-26 to H-31). Each motif is one piece:
 * deep crossings by formatting end tags, stray table-family end tags after implied tbody and tr,
 * noscript content in both views, depth-amplifying ignored end tags, and non-ASCII script types.
 * A block often follows, because the effects show in a later block.
 *
 * @param {() => number} rng
 * @param {() => string} nextId
 * @returns {string}
 */
function reviewMotif(rng, nextId, kinds = 9) {
  const block = () => {
    const inner = pick(rng, ["y", "y<a href=x>z</a>w", "y<nobr>z</nobr>w", "<button>y</button>", "<table></table>y", "<b>y</b>"]);
    const tag = pick(rng, ["span", "div", "p"]);
    return `<${tag} data-rq-block="${nextId()}">${inner}</${tag}>`;
  };
  // review: kinds 0 to 8 (unchanged since the first sweeps); review2 adds kinds 9 to 12; review3 adds kind 13.
  const kind = Math.floor(rng() * kinds);
  if (kind === 13) return templateForMotif(rng, nextId);
  if (kind >= 9) return round2Motif(kind, rng, nextId, block);
  if (kind === 0) {
    // H-26: a formatting end tag over special elements (often 8 or more: the adoption agency limit).
    const f = pick(rng, FORMATTING_PIECES);
    const n = rng() < 0.6 ? between(rng, 7, 10) : between(rng, 1, 3);
    const s = pick(rng, SPECIAL_PIECES);
    const via = rng() < 0.4 ? pick(rng, ["span", "em", "q"]) : null; // a crossing below the formatting element
    const open = `<${f}>${via ? `<${via}>` : ""}${`<${s}>`.repeat(n)}x`;
    const close = via ? `</${via}></${f}>` : `</${f}>`;
    return open + close + (rng() < 0.8 ? block() : "");
  }
  if (kind === 1) {
    // H-30: a stray table-family end tag after an implied tbody and tr.
    const start = pick(rng, ["<table><td>", "<table><tr><td>", "<table><tbody><td>", "<table><col><td>", "<table><caption>"]);
    const close = start === "<table><caption>" ? "</caption></table>" : "</td></table>";
    return `${start}${pick(rng, TABLE_END_PIECES)}${rng() < 0.8 ? block() : "x"}${close}`;
  }
  if (kind === 2) {
    // H-27: noscript content, balanced or not; a block with the same elements follows.
    const c = rng() < 0.6 ? pick(rng, NOSCRIPT_UNBALANCED) : pick(rng, NOSCRIPT_BALANCED);
    return `<noscript>${c}</noscript>${block()}`;
  }
  if (kind === 3 || kind === 4) {
    // H-31: shapes where the browser ignores an end tag that the token stack uses.
    const shape = pick(rng, [
      "<span><div></span>", "<h1><span><h2>x</h1>", "<li><ul></li>", "<body><div></body>", "<form><div></form>",
      "<b><div>x</b>", "<p><b>x</p>y", "<span><span><div></span></span>", "<i><span><dd></span></i>",
    ]);
    const reps = rng() < 0.08 ? between(rng, 100, 300) : between(rng, 1, 12);
    return shape.repeat(reps) + (rng() < 0.7 ? block() : "");
  }
  if (kind === 5) {
    // Reconstruction of formatting elements and the "steal" of a formatting end tag.
    const reps = between(rng, 1, 4);
    return `<p><b class=z>x</p>${"<b><b><b><b></b></b></b>y</b>".repeat(reps)}${block()}`;
  }
  if (kind === 6) {
    // Formatting start tags that run the adoption agency (a in the list; nobr in scope).
    const f = pick(rng, ["a", "nobr"]);
    const n = between(rng, 1, 9);
    return `<${f}>${"<div>".repeat(n)}<${f}>x</${f}>${"</div>".repeat(n)}</${f}>${block()}`;
  }
  if (kind === 7) {
    // H-28: script types with non-ASCII look-alikes, also in noscript content.
    const t = pick(rng, TYPE_LOOKALIKES);
    const s = `<script type="${t}">{}</script>`;
    return rng() < 0.3 ? `<noscript>${s}</noscript>` : s;
  }
  // Tables nested in cells: implied tbody and tr count in D.
  const n = rng() < 0.1 ? between(rng, 60, 70) : between(rng, 1, 5);
  return `${"<table><td>".repeat(n)}${block()}${"</td></table>".repeat(n)}`;
}

const FORM_SHAPES = [
  "<form><object><form></form></object></form>", "<form><div></form><form>x</form>", "<div><form></div><form>x</form>",
  "<form><input></form><form><input></form>", "<span><form></span><form>x</form>", "<form><table><form></form></table></form>",
];
const TEMPLATE_STARTS = ["<col>", "<colgroup>", "<tr><td>x</td></tr>", "<caption></caption><tr>", "<div><col>", "<td>"];
const RAW_NAMES = ["title", "textarea", "style", "xmp", "script", ""];
const FOSTER_WRAPPERS = ["object", "b", "a", "div"];
const FOSTER_ELEMENTS = ["marquee", "object", "applet", "div", "span", "form", "b"];
const CELL_CONTEXTS = [
  ["<table><tr><td>", "</td></tr></table>"], ["<table><tr><th>", "</th></tr></table>"], ["<table><caption>", "</caption></table>"],
];
const NOSCRIPT_TABLE_CONTENT = [
  "<td></td>", "<col>", "<tr></tr>", "<th>x</th>", "<caption></caption>", "<tbody></tbody>",
  "<table><tr><td>x</td></tr></table>", "<table><col></table>", "<img src=x>",
];

/**
 * Motifs of round 2 of the security review (H-35 to H-38): form pointer cycles, template with col
 * and raw text, foster-parented elements in tables, noscript in cells with table parts.
 * @param {number} kind 9 … 12
 * @param {() => number} rng
 * @param {() => string} nextId
 * @param {() => string} block
 */
function round2Motif(kind, rng, nextId, block) {
  if (kind === 9) {
    const reps = rng() < 0.08 ? between(rng, 100, 300) : between(rng, 1, 10);
    const body = pick(rng, FORM_SHAPES).repeat(reps);
    return rng() < 0.5 ? `<a href="1">${body}x</a>${block()}` : body + block();
  }
  if (kind === 10) {
    const raw = pick(rng, RAW_NAMES);
    const open = raw === "" ? "" : `<${raw}>`;
    const close = raw === "" ? "" : `</${raw}>`;
    return `<template>${pick(rng, TEMPLATE_STARTS)}${open}</template><p data-rq-block="${nextId()}">f</p>${close}</template>${block()}`;
  }
  if (kind === 11) {
    const w = pick(rng, FOSTER_WRAPPERS);
    const m = pick(rng, FOSTER_ELEMENTS);
    const unit = rng() < 0.5
      ? `<${w}><table><${m}><table></table></${m}></table></${w}>`
      : `<table><tr><${m}><td>x</td></${m}></tr></table>`;
    const reps = rng() < 0.08 ? between(rng, 100, 300) : between(rng, 1, 3);
    return unit.repeat(reps) + block();
  }
  const [open, close] = pick(rng, CELL_CONTEXTS);
  return `${open}<noscript>${pick(rng, NOSCRIPT_TABLE_CONTENT)}</noscript><div data-rq-block="${nextId()}">a<table></table>b</div>${close}`;
}

const MARKER_SHAPES = [
  ['<?start name="m">', "x<?end>"], ['<?start name="m">', "x"], ['<?marker name="m">', ""], ["", ""],
];
const TEMPLATE_FOR_SHAPES = [
  ['<template for="m">', "</template>"], ['<template FOR="m">', "</template>"], ["<template for>", "</template>"],
  ['<template for="">', "</template>"], ['<table><template for="m">', "</template></table>"],
  ['<svg><template for="m">', "</template></svg>"], ['<template id="m">', "</template>"],
];

/**
 * Motif of H-43 (conformance study T-17): processing-instruction markers in or around a block, then
 * a template with a for attribute (content patching), or a control (svg template, no for).
 * parse5 8.0.1 does not implement content patching, so a sweep finds no disagreement for this
 * motif; it measures which documents the candidates accept (class M-10-template-for).
 * @param {() => number} rng
 * @param {() => string} nextId
 */
function templateForMotif(rng, nextId) {
  const [m0, m1] = pick(rng, MARKER_SHAPES);
  const [t0, t1] = pick(rng, TEMPLATE_FOR_SHAPES);
  const tag = pick(rng, ["div", "p", "span"]);
  const id = nextId();
  const block = rng() < 0.5
    ? `<${tag} data-rq-block="${id}">${m0}${m1 || "y"}</${tag}>`
    : `${m0}<${tag} data-rq-block="${id}">y</${tag}>${m1.replace("x", "")}`;
  return `${block}${t0}${pick(rng, ["<b>new</b>", "z", ""])}${t1}`;
}

/**
 * Generate one document.
 *
 * @param {number} seed       document seed (use docSeed for a sweep)
 * @param {{ maxTokens?: number, profile?: "classic" | "review" | "review2" | "review3" }} [options] maxTokens: number of
 *   pieces (1 … 200, default 16); profile "review" adds the motifs of the security review, "review2" those of
 *   round 2, "review3" the template for motif of H-43 (default "classic": the documents of earlier sweeps, unchanged)
 * @returns {string}
 */
export function generateDocument(seed, { maxTokens = 16, profile = "classic" } = {}) {
  const limit = Math.max(1, Math.min(200, Math.floor(maxTokens)));
  const rng = makeRng(seed);
  const review = profile === "review" || profile === "review2" || profile === "review3";
  const kinds = profile === "review3" ? 14 : profile === "review2" ? 13 : 9;
  /** @type {string[]} */
  const out = [];
  /** @type {string[]} */
  const open = [];
  let ids = 0;
  // Two profiles: "structured" closes elements in order and has few special tokens, so more
  // documents are valid and have blocks; "soup" has more stray tokens.
  const structured = rng() < 0.6;
  const w = structured ? STRUCTURED : SOUP;
  if (rng() < 0.5) out.push("<!DOCTYPE html><html><head><title>T</title></head><body>");
  const count = 1 + Math.floor(rng() * limit);
  for (let k = 0; k < count; k++) {
    if (review && rng() < 0.3) {
      out.push(reviewMotif(rng, () => `r${ids++}`, kinds));
      continue;
    }
    const r = rng();
    if (r < w.open && rng() < w.motif) {
      // A short nesting pattern of lists and tables: reaches the M-24 j and k shapes.
      for (const name of pick(rng, MOTIFS)) {
        out.push(`<${name}>`);
        open.push(name);
      }
    } else if (r < w.open) {
      const blockish = rng() < w.blockish;
      const name = blockish ? pick(rng, BLOCK_NAMES) : pick(rng, OPEN_NAMES);
      const attr = blockish || rng() < 0.05 ? ` data-rq-block="b${rng() < 0.9 ? ids++ : 0}"` : "";
      const selfClosing = rng() < 0.04 ? "/" : "";
      out.push(`<${name}${attr}${selfClosing}>`);
      if (selfClosing === "") open.push(name);
    } else if (r < w.close) {
      const name = open.pop();
      if (name !== undefined) out.push(`</${name}>`);
    } else if (r < w.stray) {
      const name = open.length > 0 && rng() < 0.7 ? pick(rng, open) : pick(rng, OPEN_NAMES);
      out.push(`</${name}>`);
    } else if (r < w.text) {
      out.push(pick(rng, TEXTS));
    } else if (r < w.special) {
      out.push(pick(rng, SPECIALS));
    } else if (r < w.void) {
      out.push(pick(rng, VOID_PIECES));
    } else {
      const inner = pick(rng, RAW_INNER);
      const close = rng() < 0.9 ? "</noscript>" : "";
      out.push(`<noscript>${inner}${close}`);
    }
  }
  if (rng() < w.closeAll) {
    while (open.length > 0) out.push(`</${open.pop()}>`);
  }
  if (rng() < w.eofTail) out.push(pick(rng, EOF_TAILS));
  return out.join("");
}

const MOTIFS = [
  ["ul", "li"], ["ol", "li", "div"], ["li", "span"], ["dl", "dd"], ["dl", "dt", "div"], ["dd"], ["dt"],
  ["table"], ["table", "tr", "td"], ["table", "tbody", "tr"], ["table", "colgroup"], ["tr"], ["div", "table"],
  ["p", "span"], ["h1", "span"], ["a", "b"], ["button", "span"], ["ruby", "rt"], ["select"], ["svg", "desc"],
];

// Cumulative thresholds of the actions; the rest goes to a noscript piece.
// motif: share of "open" actions that open a motif instead of one element.
const SOUP = { open: 0.32, motif: 0.1, blockish: 0.3, close: 0.6, stray: 0.64, text: 0.78, special: 0.88, void: 0.93, closeAll: 0.85, eofTail: 0.1 };
const STRUCTURED = { open: 0.4, motif: 0.35, blockish: 0.4, close: 0.75, stray: 0.76, text: 0.89, special: 0.93, void: 0.97, closeAll: 0.97, eofTail: 0.03 };
