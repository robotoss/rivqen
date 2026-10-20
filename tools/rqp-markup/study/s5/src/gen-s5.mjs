// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// Seeded generator of small synthetic documents for the S5 study: `template` and
// `noscript` in the content of html blocks. Profiles:
//
//   tpl  template-heavy: templates in block content with every start of the "in
//        template" insertion mode (table parts, col, head elements, raw text),
//        nested templates, shadowrootmode and for attributes, forms, formatting
//        elements, lists, foreign content, stray and missing end tags, block-like
//        markup inside
//   nos  noscript-heavy: noscript in block content with balanced and unbalanced
//        content C, start tags that close elements around the noscript with
//        scripting disabled (PCLOSE, a, button, nobr, li, headings, ruby, table
//        parts), the end tag of the block element and a block start tag in C,
//        comments that hide "</noscript>", svg in C, manifest-like scripts, C to
//        the end of the input
//   mix  both, also a noscript in a template and a template in C
//
// Each document puts one to three blocks in an outer context (a cell, a list item, a
// paragraph, a formatting element, a form, an object, a reconstruction of a
// formatting element before the block), and often a probe block after them whose
// boundaries a wrong tree would cut. Document i of seed s is the same on every run
// and every platform. No real data.

import { makeRng } from '../../../crosscheck/src/generator.mjs';

/** @template T @param {() => number} rng @param {T[]} list @returns {T} */
const pick = (rng, list) => list[Math.floor(rng() * list.length)];
const chance = (rng, p) => rng() < p;
const between = (rng, lo, hi) => lo + Math.floor(rng() * (hi - lo + 1));

export const PROFILES = Object.freeze(['tpl', 'nos', 'mix']);

const BLOCK_TAGS = ['div', 'div', 'div', 'section', 'article', 'span', 'p', 'h2', 'main'];
const CONTEXTS = [
  ['', ''], ['', ''], ['', ''],
  ['<table><tr><td>', '</td></tr></table>'], ['<table><caption>', '</caption></table>'],
  ['<ul><li>', '</li></ul>'], ['<p>', '</p>'], ['<b>', '</b>'], ['<a href="#">', '</a>'],
  ['<form>', '</form>'], ['<object>', '</object>'], ['<span>', '</span>'], ['<nobr>', '</nobr>'],
  ['<p><b class=z>x</p>', ''], ['<button>', '</button>'], ['<dl><dd>', '</dd></dl>'],
  ['<template>', '</template>'], ['<select>', '</select>'], ['<ruby>', '</ruby>'], ['<h1>', '</h1>'],
];
const PROBES = [
  'y', 'y<a href="#">z</a>w', 'y<nobr>z</nobr>w', '<button>y</button>', '<table></table>y', '<b>y</b>',
  '<template>t</template>', '<noscript><img src=x></noscript>', '<p>q</p>',
];
const TEMPLATE_PLAIN = ['', '', '', ' id="t"', ' id="row"', ' class="x"'];
const TEMPLATE_ATTRS = [
  ' shadowrootmode="open"', ' shadowrootmode="closed"', ' SHADOWROOTMODE=open', ' shadowrootmode="x"', ' shadowrootmode',
  ' id=a shadowrootmode=open', ' for="m"', ' shadowrootdelegatesfocus', ' x=1 shadowrootmode=open shadowrootmode=closed',
  ' shadowrootclonable', ' data-rq-block="tb"', ' shadowrootmode="open" shadowrootserializable',
];
// The first start tag of a template decides its insertion mode ("in template").
// TEMPLATE_STARTS_BAD: names that block content or M-09 condition 1 rejects.
const TEMPLATE_STARTS_BAD = [
  '<col>', '<title>t</title>', '<select><option>o</option></select>', '<textarea>t</textarea>', '<frameset>',
  '<html>', '<body>', '<head>', '<colgroup></colgroup><col>', '<td><template><td>z</td></template></td>',
];
const TEMPLATE_STARTS = [
  '<tr><td>x</td></tr>', '<td>x</td>', '<th>h</th>', '<caption>c</caption>', '<colgroup><col></colgroup>', '<col>',
  '<tbody><tr><td>x</td></tr></tbody>', '<thead></thead>', '<tfoot><tr></tr></tfoot>', '<caption></caption><td>x</td>',
  '<tbody></tbody><tr><td>y</td></tr>', '<meta charset=x>', '<link rel=x>',
  '<style>a{}</style>', '<script>a</script>', '<base href=x>', '<template>i</template>',
  '<div>x</div>', '<p>x</p>', '<li>x</li>', '<table><tr><td>x</td></tr></table>', '<td><template><tr><td>z</td></tr></template></td>',
  '<form><input></form>', '<a href="#">a</a>', '<noscript>n</noscript>', '<noscript><img src=x></noscript>',
  '<svg><path d=M0></svg>', '<caption></caption><div>f</div>', '<tbody></tbody><span>f<b>g</b></span>', '<tr></tr>x<td>y</td>',
  '<colgroup></colgroup><p>q</p>', '<thead><tr><th>h</th></tr></thead><tbody><tr><td>d</td></tr></tbody>', '',
];
const PIECES = [
  'x', ' ', '&amp;', '<!--c-->', '<!--</template>-->', '<!--</noscript>-->', '<br>', '<img src=x>', '<hr>',
  '<span>s</span>', '<b>b</b>', '<i>i</i>', '<a href="#">a</a>', '<div>d</div>', '<p>p</p>', '<em>e</em>',
  '<ul><li>l</li></ul>', '<dl><dt>t</dt><dd>d</dd></dl>', '<table><tr><td>c</td></tr></table>',
  '<table><caption>c</caption></table>', '<button>b</button>', '<nobr>n</nobr>', '<ruby>r<rt>t</rt></ruby>',
  '<form><input></form>', '<h3>h</h3>', '<svg><path d=M0></svg>', '<math><mi>x</mi></math>', '<script>s()</script>',
  '<style>a{}</style>', '<iframe src=x></iframe>', '<object>o</object>', '<input type=hidden>', '<label>l</label>',
];
const MUTATIONS = [
  '<div>', '<p>', '<b>', '<span>', '<li>', '<dd>', '<td>', '<tr>', '<th>', '<caption>', '<colgroup>', '<col>',
  '<tbody>', '<table>', '<a href="#">', '<nobr>', '<button>', '<h2>', '<rt>', '<form>', '<object>', '<select>',
  '<option>', '<textarea>', '<title>', '<xmp>', '<plaintext>', '<frameset>', '<body>', '<html>', '<head>',
  '<!DOCTYPE html>', '</div>', '</p>', '</b>', '</span>', '</template>', '</noscript>', '</td>', '</tr>',
  '</table>', '</a>', '</br>', '</form>', '</li>', '<p data-rq-block="in">x</p>', '<span/>', '<image>',
  '<script type="application/rivqen-manifest+json">{}</script>', '<template>', '<noscript>',
];
// C pieces that M-11 and the content rules can accept (C_GOOD) and pieces that they
// reject or that test a boundary (C_HOSTILE).
const C_GOOD = [
  '', 'text', '&lt;', '<img src=x>', '<img src=x>', '<img src=x>', '<iframe src="x"></iframe>', '<iframe src="x"></iframe>',
  '<img src=x onerror=alert(1)>', '<a href="javascript:x">j</a>', '<div>x</div>', '<p>x</p>', '<hr>', '<h2>x</h2>',
  '<table><tr><td>x</td></tr></table>', '<svg><path d=M0></svg>', '<style>a{}</style>', '<script>a()</script>',
  '<form><input></form>', '<!--x-->', '<b>x</b>', '<span><b>x</b></span>', '<object>o</object>', '<image src=x>',
  '<link rel=stylesheet href=x>', '<meta http-equiv=refresh content=0>', '<input>', '<ul><li>x</li></ul>',
  '<img src=x><iframe src=y></iframe>', '<a href="#"><img src=x></a>', '<picture><img src=x></picture>',
];
const C_HOSTILE = [
  '<li>x</li>', '<dd>x</dd>', '<a href="#">x</a>', '<button>b</button>', '<nobr>n</nobr>', '<rt>r</rt>',
  '<td>x</td>', '<tr></tr>', '<col>', '<svg><g><path d=M0></g></svg>', '<template>t</template>',
  '<form></form>', '</div>', '</p>', '</span>', '<div data-rq-block="n0">x</div>',
  '<!--', '<!--</noscript>-->', '<textarea>t</textarea>', '<title>t</title>', '<select>',
  '<math><mi>x</mi></math>', '<noscript>', '<script type="application/rivqen-manifest+json">{}</script>',
  '<button>', '<table>', '<div>', '<p>x', '<span><div></span>', '<b>', '</b>', '<h1>x</h1>', '<p><div>x</div></p>',
  '<body>', '</noscript ', '<xmp>x</xmp>', '<plaintext>',
];

/** A noscript element with content C (sometimes unclosed). */
function noscript(rng, depth) {
  let c = '';
  const n = between(rng, 0, 3);
  for (let k = 0; k < n; k++) {
    if (chance(rng, 0.15) && depth < 2) c += content(rng, depth + 1, 'nos');
    else c += pick(rng, chance(rng, 0.75) ? C_GOOD : C_HOSTILE);
  }
  const close = chance(rng, 0.95) ? '</noscript>' : '';
  return `<noscript>${c}${close}`;
}

/** A template element with content (sometimes unclosed or closed wrong). */
function template(rng, depth, profile) {
  let s = `<template${chance(rng, 0.12) ? pick(rng, TEMPLATE_ATTRS) : pick(rng, TEMPLATE_PLAIN)}>`;
  if (chance(rng, 0.4)) s += chance(rng, 0.1) ? pick(rng, TEMPLATE_STARTS_BAD) : pick(rng, TEMPLATE_STARTS);
  if (chance(rng, 0.7) && depth < 3) s += content(rng, depth + 1, profile);
  if (chance(rng, 0.2)) s += pick(rng, TEMPLATE_STARTS);
  const r = rng();
  s += r < 0.9 ? '</template>' : r < 0.95 ? '' : pick(rng, ['</div>', '</td>', '</template></template>']);
  return s;
}

/** Block content: pieces, templates and noscripts. */
function content(rng, depth, profile) {
  let s = '';
  const n = between(rng, 0, depth === 0 ? 5 : 3);
  const pT = profile === 'nos' ? 0.05 : profile === 'tpl' ? 0.45 : 0.25;
  const pN = profile === 'tpl' ? 0.05 : profile === 'nos' ? 0.45 : 0.25;
  for (let k = 0; k < n; k++) {
    const r = rng();
    if (r < pT && depth < 4) s += template(rng, depth, profile);
    else if (r < pT + pN && depth < 4) s += noscript(rng, depth);
    else if (r < pT + pN + 0.03) s += pick(rng, MUTATIONS);
    else if (r < pT + pN + 0.12 && depth < 3) {
      // A wrapper around more content: a formatting, list, table cell or paragraph context.
      const [open, close] = pick(rng, [['<p>', '</p>'], ['<b>', '</b>'], ['<a href="#">', '</a>'], ['<ul><li>', '</li></ul>'],
        ['<table><tr><td>', '</td></tr></table>'], ['<div>', '</div>'], ['<span>', '</span>'], ['<button>', '</button>'],
        ['<form>', '</form>'], ['<object>', '</object>'], ['<ruby>', '</ruby>'], ['<h3>', '</h3>'], ['<table>', '</table>']]);
      s += open + content(rng, depth + 1, profile) + (chance(rng, 0.96) ? close : '');
    } else s += pick(rng, PIECES);
  }
  return s;
}

/**
 * Generate one document.
 * @param {number} seed document seed (docSeed of the cross-check generator)
 * @param {'tpl' | 'nos' | 'mix'} profile
 */
export function generateS5Document(seed, profile) {
  if (!PROFILES.includes(profile)) throw new RangeError(`unknown profile ${profile}`);
  const rng = makeRng(seed);
  let ids = 0;
  const parts = [];
  if (chance(rng, 0.5)) parts.push('<!DOCTYPE html><html><head><title>T</title></head><body>');
  if (chance(rng, 0.1)) parts.push(pick(rng, ['<noscript><img src=x></noscript>', '<template><tr><td>x</td></tr></template>', '<p>open']));
  const blocks = between(rng, 1, 3);
  for (let k = 0; k < blocks; k++) {
    const [open, close] = pick(rng, CONTEXTS);
    const tag = pick(rng, BLOCK_TAGS);
    // H-35 in block content (M-09 condition 1): the token stack reads raw text after
    // <template><col>, the tree builder ignores the start tag. Only the control tu
    // accepts it; it shows that the oracle sees the cut.
    const h35 = profile !== 'nos' && chance(rng, 0.03)
      ? `<table><tr><td><template><col><${pick(rng, ['style', 'script', 'xmp', 'iframe'])}></template></td></tr></table></${tag}><p data-rq-block="z${ids}">f</p>`
      : null;
    const raw = h35 === null ? '' : h35.match(/<col><([a-z]+)>/)[1];
    const body = h35 === null ? content(rng, 0, profile) : `${h35}</${raw}></template></td></tr></table>`;
    const end = chance(rng, 0.96) ? `</${tag}>` : '';
    parts.push(`${open}<${tag} data-rq-block="b${ids++}">${body}${end}${close}`);
  }
  if (chance(rng, 0.6)) {
    const tag = pick(rng, ['span', 'div', 'p']);
    parts.push(`<${tag} data-rq-block="b${ids++}">${pick(rng, PROBES)}</${tag}>`);
  }
  if (chance(rng, 0.04)) parts.push(pick(rng, ['<noscript>x', '<template>x', '<noscript><img src=x>']));
  return parts.join('');
}
