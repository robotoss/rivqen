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

/**
 * Generate one document.
 *
 * @param {number} seed       document seed (use docSeed for a sweep)
 * @param {{ maxTokens?: number }} [options] maxTokens: number of pieces (1 … 200, default 16)
 * @returns {string}
 */
export function generateDocument(seed, { maxTokens = 16 } = {}) {
  const limit = Math.max(1, Math.min(200, Math.floor(maxTokens)));
  const rng = makeRng(seed);
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
