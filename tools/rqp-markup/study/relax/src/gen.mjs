// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// Seeded generator of small synthetic documents for the H-40 relaxation sweep
// (WP-17 S3 T-01). It reaches the shapes of the three relaxations:
//
//   block     an svg or math subtree in the content of an html block
//   noscript  an svg subtree in noscript content, in body and in head
//   icon      an svg subtree (often with unclosed children) inside formatting
//             and special elements outside blocks, then a probe block
//
// The subtrees have crossings, unclosed foreign children, integration points
// with text and with tags, breakout tags, foreignObject with html, nested svg
// and math, CDATA, comments, stray end tags and attribute shapes of the
// attribute rule. Document i of seed s is the same on every run. No real data.

import { makeRng } from '../../../crosscheck/src/generator.mjs';

/** @param {() => number} rng @template T @param {T[]} list @returns {T} */
const pick = (rng, list) => list[Math.floor(rng() * list.length)];
const chance = (rng, p) => rng() < p;
const between = (rng, lo, hi) => lo + Math.floor(rng() * (hi - lo + 1));

const SVG_ELEMENTS = [
  'g', 'g', 'path', 'path', 'path', 'circle', 'rect', 'a', 'text', 'tspan', 'use', 'image', 'defs', 'symbol',
  'linearGradient', 'stop', 'clipPath', 'mask', 'filter', 'feImage', 'feGaussianBlur', 'pattern', 'marker',
  'svg', 'switch', 'textPath', 'animate', 'set', 'animateTransform', 'animateMotion', 'mpath', 'x-el', 'math',
];
const MATH_ELEMENTS = [
  'mrow', 'mrow', 'mfrac', 'msup', 'msub', 'semantics', 'annotation', 'maction', 'mspace', 'mtable', 'mtr',
  'mtd', 'mglyph', 'malignmark', 'math', 'svg', 'menclose',
];
const MATH_INTEGRATION = ['mi', 'mo', 'mn', 'ms', 'mtext', 'annotation-xml'];
const SVG_INTEGRATION = ['title', 'desc', 'foreignObject'];
const FOREIGN_TEXT = ['style', 'script', 'textarea', 'iframe', 'noscript', 'xmp', 'noembed', 'noframes'];
const ATTRS = [
  ' d="M0 0"', ' href="#a"', ' xlink:href="#a"', ' fill="red"', ' class="i"', ' viewBox="0 0 1 1"',
  ' attributeName="href" values="#a;#b"', ' attributeName="fill" to="red"', ' onload="x()"', ' href="javascript:x"',
  ' encoding="text/html"', ' color="red"', ' style="fill:red"', ' href="data:image/svg+xml,x"', ' values="a;javascript:x"',
  ' xmlns="http://www.w3.org/2000/svg"', ' xmlns:xlink="http://www.w3.org/1999/xlink"', ' FILL=x', '/x=1',
];
const INTEGRATION_CONTENT = [
  'x', 'a < b', '1 <2', '<![CDATA[x]]>', '<![CDATA[a>b]]>', '', '&amp;', ' ', '<b>x</b>', '<p>', '<!--c-->',
  '</x>', '<div>x</div>', '<span>s</span>', '<svg></svg>', '<img src=x>',
];
const MUTATIONS = [
  '<div>', '<p>', '<b>', '<br>', '<span>', '<font color=red>', '<font>', '<li>', '<table>', '<img src=x>',
  '<h1>', '<pre>', '<ul>', '<em>', '<sub>', '<button>', '<select>', '<td>', '<tr>', '<input>', '<a href=#>',
  '<template>', '<option>', '<form>', '<object>', '<frameset>', '<noscript>', '<plaintext>', '<image>',
  '</svg>', '</math>', '</p>', '</br>', '</div>', '</a>', '</span>', '</noscript>', '</g>', '</path>', '</title>',
  '</table>', '</td>', '</image>', '</foreignObject>', '</mi>', '</b>', '<!DOCTYPE html>', '<?pi x?>',
  '<![CDATA[x]]>', '<![CDATA[a>b]]>', '<!--c-->', 'x', '<desc>', '<mi>', '</desc>',
];

/**
 * One foreign subtree. `ns` is "svg" or "math"; depth bounds the recursion.
 * @param {() => number} rng
 */
function foreignChildren(rng, ns, depth) {
  const n = depth >= 4 ? 0 : between(rng, 0, 3);
  let s = '';
  for (let k = 0; k < n; k++) {
    const r = rng();
    if (r < 0.08) s += pick(rng, MUTATIONS);
    else if (r < 0.15) s += pick(rng, ['x', ' ', '<!--c-->', '<![CDATA[t]]>', '&amp;']);
    else s += foreignElement(rng, ns, depth + 1);
  }
  return s;
}

function attrs(rng) {
  let s = '';
  const n = chance(rng, 0.5) ? 0 : between(rng, 1, 2);
  for (let k = 0; k < n; k++) s += pick(rng, ATTRS);
  return s;
}

function close(rng, name) {
  const r = rng();
  if (r < 0.18) return ''; // unclosed child (the icon shape of r3)
  if (r < 0.22) return `</${pick(rng, ['g', 'path', 'svg', 'a', 'title', 'p', 'span'])}>`;
  return `</${name}>`;
}

function foreignElement(rng, ns, depth) {
  const r = rng();
  if (r < 0.15) {
    const name = ns === 'svg' ? pick(rng, SVG_INTEGRATION) : pick(rng, MATH_INTEGRATION);
    const inner = pick(rng, INTEGRATION_CONTENT);
    return `<${name}${attrs(rng)}>${inner}${chance(rng, 0.1) ? '' : `</${name}>`}`;
  }
  if (r < 0.22) {
    const name = pick(rng, FOREIGN_TEXT);
    if (chance(rng, 0.15)) return `<${name}/>`;
    return `<${name}>${pick(rng, ['x', 'p{}', '<b>', '<![CDATA[x]]>', ''])}</${name}>`;
  }
  const name = ns === 'svg' ? pick(rng, SVG_ELEMENTS) : pick(rng, MATH_ELEMENTS);
  const a = attrs(rng);
  if (chance(rng, 0.25)) return `<${name}${a}/>`;
  const childNs = name === 'svg' ? 'svg' : name === 'math' ? 'math' : ns;
  return `<${name}${a}>${foreignChildren(rng, childNs, depth)}${close(rng, name)}`;
}

/** A root svg or math element with its subtree. */
export function foreignRoot(rng, { svgOnly = false } = {}) {
  const ns = svgOnly || chance(rng, 0.7) ? 'svg' : 'math';
  const a = attrs(rng);
  if (chance(rng, 0.05)) return `<${ns}${a}/>`;
  const end = chance(rng, 0.04) ? '' : chance(rng, 0.03) ? `</${ns === 'svg' ? 'math' : 'svg'}>` : `</${ns}>`;
  return `<${ns}${a}>${foreignChildren(rng, ns, 0)}${end}`;
}

const BLOCKS = ['div', 'span', 'p', 'section', 'h2', 'article', 'header', 'nav'];
const WRAPPERS = [
  ['<div>', '</div>'], ['<a href=#>', '</a>'], ['<b>', '</b>'], ['<p>', '</p>'], ['<ul><li>', '</li></ul>'],
  ['<table><tr><td>', '</td></tr></table>'], ['<button>', '</button>'], ['<span>', '</span>'],
  ['<nobr>', '</nobr>'], ['<form>', '</form>'], ['<object>', '</object>'], ['<table><caption>', '</caption></table>'],
  ['<li>', '</li>'], ['<h1>', '</h1>'], ['<label>', '</label>'],
];
const INNER_WRAPPERS = [
  ['', ''], ['', ''], ['<a href=#>', '</a>'], ['<button>', '</button>'], ['<span>', '</span>'], ['<i>', '</i>'],
  ['<div>', '</div>'], ['<label>', '</label>'], ['<a href=#><span>', '</span></a>'],
];
const FILLERS = ['', '', 'x', '<b>x</b>', '<a href=#>l</a>', '<span>s</span>', '<p>q</p>', '<br>', '<i>i</i>'];
const PROBES = [
  '<span data-rq-block="z">y<a href=#>z</a>w</span>',
  '<div data-rq-block="z"><a href=#>q</a></div>',
  '<p data-rq-block="z">y<nobr>n</nobr></p>',
  '<table><tr><td data-rq-block="z">c</td></tr></table>',
  '<span data-rq-block="z">y<b>b</b>w</span>',
  '',
];
const CROSSINGS = ['', '', '', '<span><div></span>', '<b><i></b>', '<a href=#><div></a>', '<span><svg><path></span>'];

function wrap(rng, list, n) {
  let open = '';
  let shut = '';
  for (let k = 0; k < n; k++) {
    const [o, c] = pick(rng, list);
    open += o;
    shut = c + shut;
  }
  return [open, shut];
}

function blockDoc(rng) {
  const [po, pc] = wrap(rng, WRAPPERS, between(rng, 0, 2));
  const [io, ic] = pick(rng, INNER_WRAPPERS);
  const b = pick(rng, BLOCKS);
  let content = pick(rng, FILLERS);
  const icons = chance(rng, 0.8) ? 1 : between(rng, 2, 4);
  for (let k = 0; k < icons; k++) content += `${io}${foreignRoot(rng)}${ic}${pick(rng, FILLERS)}`;
  return `${po}<${b} data-rq-block="b">${content}</${b}>${pc}${pick(rng, PROBES)}`;
}

function noscriptDoc(rng) {
  const inner = () => `${pick(rng, ['', '', '<img src=x>', 'x', '<a href=#>l</a>'])}${foreignRoot(rng, { svgOnly: chance(rng, 0.85) })}${pick(rng, ['', 'x', '<span>s</span>'])}`;
  if (chance(rng, 0.25)) {
    const head = pick(rng, ['', '<meta charset=utf-8>', '<link rel=x href=y>']);
    return `<!DOCTYPE html><html><head>${head}<noscript>${inner()}</noscript><title data-rq-block="t">T</title></head><body>${pick(rng, PROBES)}</body></html>`;
  }
  const [po, pc] = wrap(rng, WRAPPERS, between(rng, 0, 2));
  // A noscript at the start of the input goes into the implied head; text or a body
  // start tag first puts it into the body.
  const lead = po === '' && chance(rng, 0.7) ? pick(rng, ['x', '<p>x</p>', '<body>']) : '';
  return `${lead}${po}<noscript>${inner()}</noscript>${pick(rng, FILLERS)}${pc}${pick(rng, PROBES)}`;
}

// Icon shapes of real pages: foreign children without end tags, closed by </svg>.
const ICONS = [
  '<svg><path d="M0"></svg>', '<svg viewBox="0 0 1 1"><path d="M0"><path d="M1"></svg>', '<svg><g><path></g></svg>',
  '<svg><use href="#i"></svg>', '<svg><circle r=1><rect></svg>', '<svg><g><path></svg>', '<math><mrow><mi>x</mi></math>',
  '<svg><title>t</title><path></svg>', '<svg><a href=#><path></a></svg>', '<svg><path></path></svg>',
];

function iconDoc(rng) {
  const [po, pc] = wrap(rng, WRAPPERS, between(rng, 0, 1));
  const repeat = chance(rng, 0.7) ? 1 : between(rng, 2, 10);
  let s = po;
  for (let k = 0; k < repeat; k++) {
    const [io, ic] = pick(rng, [['<a href=#>', '</a>'], ['<b>', '</b>'], ['<nobr>', '</nobr>'], ['<span>', '</span>'], ['<button>', '</button>'], ['<a href=#><span>', '</span></a>'], ['<i><b>', '</b></i>']]);
    const icon = chance(rng, 0.5) ? pick(rng, ICONS) : foreignRoot(rng);
    s += `${io}${pick(rng, CROSSINGS)}${icon}${pick(rng, CROSSINGS)}${ic}`;
  }
  return `${s}${pc}${pick(rng, PROBES)}`;
}

/**
 * Document of the relaxation sweep for a document seed (see docSeed of the
 * cross-check generator). The size is bounded: the recursion depth is at most
 * 4 with at most 3 children per element.
 * @param {number} seed
 */
export function generateRelaxDocument(seed) {
  const rng = makeRng(seed);
  const r = rng();
  if (r < 0.45) return blockDoc(rng);
  if (r < 0.7) return noscriptDoc(rng);
  return iconDoc(rng);
}
