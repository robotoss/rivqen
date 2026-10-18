// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// Seeded generator of noscript documents for the H-42 study (WP-17 S3 T-02).
// Synthetic documents only. Two profiles:
//   body  a context chain (p, table cells and captions, formatting elements,
//         template, object, button, form, lists, foster-parenting table modes,
//         foreign content), a noscript with generated content C, then blocks.
//   head  a document head with a noscript whose content is head content, text,
//         body tags, comments, stray end tags or document tags, then a body block.
// Every document is bounded: at most MAX_PIECES pieces of C at each of at most
// MAX_C_DEPTH levels, and at most 3 contexts.

import { makeRng, docSeed } from '../../../crosscheck/src/generator.mjs';

export { docSeed };

export const PROFILES = Object.freeze(['body', 'head']);
const MAX_PIECES = 5;
const MAX_C_DEPTH = 3;

/** [open, close, label]. The label is the context name in a signature. */
export const CONTEXTS = Object.freeze([
  ['<p>', '</p>', 'p'],
  ['<b>', '</b>', 'b'],
  ['<a href="1">', '</a>', 'a'],
  ['<nobr>', '</nobr>', 'nobr'],
  ['<object>', '</object>', 'object'],
  ['<applet>', '</applet>', 'applet'],
  ['<marquee>', '</marquee>', 'marquee'],
  ['<button>', '</button>', 'button'],
  ['<form>', '</form>', 'form'],
  ['<ul><li>', '</li></ul>', 'li'],
  ['<li>', '</li>', 'li-nolist'],
  ['<dl><dd>', '</dd></dl>', 'dd'],
  ['<div>', '</div>', 'div'],
  ['<span>', '</span>', 'span'],
  ['<h1>', '</h1>', 'h1'],
  ['<em><i>', '</i></em>', 'em-i'],
  ['<center>', '</center>', 'center'],
  ['<ruby>', '</ruby>', 'ruby'],
  ['<table><tr><td>', '</td></tr></table>', 'td'],
  ['<table><tr><th>', '</th></tr></table>', 'th'],
  ['<table><caption>', '</caption></table>', 'caption'],
  ['<table>', '</table>', 'table'],
  ['<table><tbody>', '</tbody></table>', 'tbody'],
  ['<table><tr>', '</tr></table>', 'tr'],
  ['<table><colgroup>', '</colgroup></table>', 'colgroup'],
  ['<template>', '</template>', 'template'],
  ['<select>', '</select>', 'select'],
  ['<svg>', '</svg>', 'svg'],
  ['<math><mi>', '</mi></math>', 'mi'],
]);

const C_TEXT = ['x', ' ', '\n', '&nbsp;', 'Enable JavaScript', '\r\n'];
const C_VOID = ['<img src=x>', '<br>', '<hr>', '<input>', '<link rel=x>', '<meta name=x>', '<wbr>', '<source>', '<col>', '<embed>', '<area>', '<param>'];
const C_RAW = [
  '<iframe src=x></iframe>', '<style>a{}</style>', '<title>t</title>', '<textarea>t</textarea>', '<xmp>x</xmp>',
  '<script>1</script>', '<noembed>x</noembed>', '<noframes>x</noframes>',
];
const C_PAIRS = [
  'div', 'p', 'span', 'a', 'b', 'i', 'em', 'strong', 'nobr', 'font', 'button', 'form', 'object', 'marquee', 'applet',
  'table', 'ul', 'li', 'ol', 'dl', 'dd', 'dt', 'h1', 'h2', 'address', 'center', 'picture', 'video', 'label', 'section',
  'tbody', 'tr', 'td', 'th', 'caption', 'colgroup', 'ruby', 'rb', 'rt', 'html', 'body', 'head', 'select', 'svg',
  'template', 'pre', 'listing', 'small', 'u', 'fieldset', 'details', 'summary', 'figure', 'main', 'nav', 'header',
];
const C_MOTIFS = [
  '<form><object><form></form></object></form>', // H-36
  '<table><marquee><table></table></marquee></table>', // H-37 (T-16 Java example)
  '<table><object><td>x</td></object></table>',
  '<table><tr><marquee><td>x</td></marquee></tr></table>', // H-37
  '<table><tr><td>x</td></tr></table>',
  '<table><td>x</td></table>',
  '<p><div>x</div></p>',
  '<a href=x><div>y</div></a>', // H-26 shape, balanced
  '<b><p>x</p></b>',
  '<ul><li><div><li>x</li></div></li></ul>',
  '<button><button>x</button></button>',
  '<a href=x><a href=y>z</a></a>',
  '<nobr><nobr>x</nobr></nobr>',
  '<table><caption>x</caption></table>',
  '<table><colgroup><col></colgroup></table>',
  '<form>x</form>',
  '<object><form></form></object>',
];
const C_FOREIGN = [
  '<svg><path d=M0 /></svg>', '<svg><path d=M0></path></svg>', '<svg><title>t</title></svg>', '<svg><desc>a</desc></svg>',
  '<math><mi>x</mi></math>', '<svg><foreignObject><div>x</div></foreignObject></svg>', '<svg><path d=M0></svg>',
  '<svg><g><rect/></g></svg>', '<svg><style>a{}</style></svg>', '<svg><a href=x><text>t</text></a></svg>',
  '<svg><p>x</p></svg>', '<math><mtext><b>x</b></mtext></math>', '<svg/>', '<svg><![CDATA[x]]></svg>',
];
const C_UNBALANCED = ['<p>', '</div>', '<li>', '<td>', '</p>', '<b>', '</a>', '<table>'];

/** The blocks after the noscript: the shapes that a closed or moved element would change. */
export const BLOCKS = Object.freeze([
  (id) => `<span data-rq-block="${id}">y<a href="2">z</a>w</span>`,
  (id) => `<div data-rq-block="${id}">a<table><tr><td>q</td></tr></table>b<button>c</button>d</div>`,
  (id) => `<p data-rq-block="${id}">t<b>u</b>v</p>`,
  (id) => `<div data-rq-block="${id}">x<form>y</form>z</div>`,
  (id) => `<div data-rq-block="${id}">a<ul><li>x</li></ul>b<nobr>n</nobr></div>`,
  (id) => `<section data-rq-block="${id}"><h2>x</h2><a href="3">l</a></section>`,
  (id) => `<span data-rq-block="${id}">s</span>`,
]);

const HEAD_C = [
  ' ', '\n', '\r\n', '\t', '<!--c-->', '<?pi?>', '<!DOCTYPE html>', '<link rel=stylesheet href=x>', '<link/>',
  '<meta http-equiv=refresh content=0>', '<meta name=x>', '<style>a{}</style>', '<noframes>x</noframes>', '<basefont>',
  '<bgsound>', '<base href=x>', '<script>1</script>', '<title>t</title>', '<template></template>', 'x', '&nbsp;', '&#32;',
  '<img src=x>', '<iframe src=x></iframe>', '<div>x</div>', '<p>x</p>', '<a href=x>y</a>', '<span>s</span>', '<head>',
  '<html>', '<body>', '</head>', '</body>', '</html>', '</br>', '</meta>', '</p>', '</div>', '<noscript>', '<svg></svg>',
  '<input>', '<br>', '<frameset>', '<object></object>', '<table></table>', '<h1>x</h1>', '<textarea>t</textarea>',
];
const HEAD_AROUND = [
  '<meta charset=utf-8>', '<title data-rq-block="t">T</title>', '<link rel=x>', '<script>1</script>', '<style>a{}</style>',
  ' ', '\n', '<!--h-->', '<base href=x>', '<template><p>x</p></template>', '<noscript><link rel=y></noscript>',
];

function pick(rng, list) {
  return list[Math.floor(rng() * list.length)];
}

function between(rng, lo, hi) {
  return lo + Math.floor(rng() * (hi - lo + 1));
}

/** Generated noscript content C. `tags` collects the start tag names (for the signature). */
function content(rng, level, tags) {
  const n = between(rng, 0, MAX_PIECES);
  let out = '';
  for (let i = 0; i < n; i++) {
    const r = rng();
    if (r < 0.18) out += pick(rng, C_TEXT);
    else if (r < 0.24) out += '<!--c-->';
    else if (r < 0.38) {
      const v = pick(rng, C_VOID);
      tags.add(/^<([a-z]+)/.exec(v)[1]);
      out += v;
    } else if (r < 0.48) {
      const v = pick(rng, C_RAW);
      tags.add(/^<([a-z]+)/.exec(v)[1]);
      out += v;
    } else if (r < 0.6) {
      const v = pick(rng, C_MOTIFS);
      for (const m of v.matchAll(/<([a-z]+)/g)) tags.add(m[1]);
      out += v;
    } else if (r < 0.61) {
      const v = pick(rng, C_FOREIGN);
      for (const m of v.matchAll(/<([a-zA-Z]+)/g)) tags.add(m[1].toLowerCase());
      out += v;
    } else if (r < 0.64) {
      const v = pick(rng, C_UNBALANCED);
      tags.add(v);
      out += v;
    } else if (level < MAX_C_DEPTH) {
      const name = pick(rng, C_PAIRS);
      tags.add(name);
      out += `<${name}>${content(rng, level + 1, tags)}</${name}>`;
    } else {
      out += 'y';
    }
  }
  return out;
}

/**
 * One body-profile document. Returns { input, signature }: the signature names the
 * contexts and the start tags of C (no document text), for grouping disagreements.
 */
function bodyDocument(rng) {
  const ctxCount = between(rng, 0, 3);
  const ctx = [];
  for (let i = 0; i < ctxCount; i++) ctx.push(pick(rng, CONTEXTS));
  const tags = new Set();
  let id = 0;
  const nextId = () => `b${++id}`;
  let out = rng() < 0.5 ? '<!DOCTYPE html><html><head></head><body>' : '';
  if (rng() < 0.2) out += pick(rng, BLOCKS)(nextId());
  for (const [open] of ctx) out += open;
  const noscripts = rng() < 0.15 ? 2 : 1;
  const eof = rng() < 0.05;
  for (let k = 0; k < noscripts; k++) {
    out += `<noscript>${content(rng, 0, tags)}`;
    if (eof && k === noscripts - 1) {
      return { input: out, signature: signature(ctx, tags, 'eof') };
    }
    out += '</noscript>';
  }
  out += 'x';
  // The block inside the innermost context (cell, caption, div ...) or after all contexts.
  const inside = rng() < 0.4;
  if (inside) out += pick(rng, BLOCKS)(nextId());
  for (let i = ctx.length - 1; i >= 0; i--) {
    if (rng() < 0.12) continue; // an unclosed context
    out += ctx[i][1];
  }
  if (!inside || rng() < 0.5) out += pick(rng, BLOCKS)(nextId());
  return { input: out, signature: signature(ctx, tags, inside ? 'inside' : 'after') };
}

function signature(ctx, tags, where) {
  return `${ctx.map((c) => c[2]).join('>') || '-'}|${[...tags].sort().join(',') || '-'}|${where}`;
}

/** One head-profile document. */
function headDocument(rng) {
  const tags = new Set();
  let out = rng() < 0.9 ? '<!DOCTYPE html><html><head>' : '<!DOCTYPE html><html>';
  const before = between(rng, 0, 2);
  for (let i = 0; i < before; i++) out += pick(rng, HEAD_AROUND);
  let c = '';
  const n = between(rng, 0, MAX_PIECES);
  for (let i = 0; i < n; i++) {
    const p = pick(rng, HEAD_C);
    const m = /^<\/?([a-z!?]+)/i.exec(p);
    tags.add(m ? (p.startsWith('</') ? `/${m[1]}` : m[1]) : p.trim() === '' ? 'ws' : 'text');
    c += p;
  }
  out += `<noscript>${c}`;
  if (rng() < 0.04) return { input: out, signature: `head|${[...tags].sort().join(',') || '-'}|eof` };
  out += '</noscript>';
  const after = between(rng, 0, 2);
  for (let i = 0; i < after; i++) out += pick(rng, HEAD_AROUND);
  out += rng() < 0.95 ? '</head><body>' : '<body>';
  out += pick(rng, BLOCKS)('b1');
  out += '</body></html>';
  return { input: out, signature: `head|${[...tags].sort().join(',') || '-'}` };
}

/**
 * Document `index` of a sweep with `seed` and `profile`. Deterministic.
 * @param {number} seed
 * @param {number} index
 * @param {'body' | 'head'} profile
 */
export function generateNoscriptDocument(seed, index, profile) {
  if (!PROFILES.includes(profile)) throw new RangeError(`unknown profile ${profile}`);
  const rng = makeRng(docSeed(seed, index));
  return profile === 'body' ? bodyDocument(rng) : headDocument(rng);
}
