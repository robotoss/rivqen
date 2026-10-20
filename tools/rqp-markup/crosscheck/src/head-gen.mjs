// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// Seeded generator of document prefixes for the head sweep (derived from the generator of the
// S4 study `../study/s4/src/head-gen.mjs`, H-79, so the same seed gives the same documents): tokens
// before a head start tag (white space, comments, DOCTYPE tokens, html start tags,
// ignored end tags, and some tokens that end item 1 of the head rule), a stop token
// (head content, a head start tag, body content, text), and a suffix after the stop
// point with head content, noscript, template, text, comments, html attributes, a
// late head start tag, a body and blocks. Synthetic documents only. Document i of
// seed s is the same on every run.

import { docSeed, makeRng } from './generator.mjs';

const pick = (rng, list) => list[Math.floor(rng() * list.length)];

/** Tokens and gaps that item 1 of the head rule accepts before the head. */
const PRE = [
  ' ', '\n', '\t', '\r\n', '\f', '<!--c-->', '<!---->', '<!-->', '<!--a--!>', '<!DOCTYPE html>', '<!doctype HTML>',
  '<!DOCTYPE html PUBLIC "-//W3C//DTD HTML 4.01//EN" "http://www.w3.org/TR/html4/strict.dtd">', '<html>',
  '<html lang="en">', '<HTML class="a" data-x=1>', '<html/>', '</p>', '</div>', '</x>', '</script>', '<?pi x?>',
  '<!x>', '</ x>', '<![CDATA[x]]>',
];
/** Tokens and gaps that end item 1 without an insertion point (also with H-79). */
const PRE_BAD = ['x', '&amp;', ' ', '</br>', '</head>', '</body>', '</html>', '</>', '<!DOCTYPE html>x'];
/** Stop tokens of H-79 (the list of the decision). */
const STOP_H79 = [
  '<meta charset="utf-8">', '<META CHARSET=utf-8>', '<meta name="viewport" content="width=device-width">',
  '<meta http-equiv="Content-Security-Policy" content="default-src \'self\'">', '<link rel="stylesheet" href="a.css">',
  '<link rel=preload href=x as=script/>', '<base href="/">', '<basefont>', '<bgsound src=x>', '<script>var a = 1 < 2;</script>',
  '<script src="a.js"></script>', '<script type="application/json">{"a":1}</script>', '<script>document.write("<p>")</script>',
  '<script><!--<script></script>--></script>', '<style>a > b {}</style>', '<style><!--</style>', '<title>T <b></title>',
  '<title/>T</title>', '<noframes><p>x</noframes>', '<script/>', '<meta>',
];
/** Other stop tokens: noscript and template (option l1), body content (l2), head start tags. */
const STOP_OTHER = [
  '<noscript><img src="x"></noscript>', '<noscript><link rel="a" href="b"></noscript>', '<noscript>t</noscript>',
  '<template><p>x</p></template>', '<template><meta></template>', '<head>', '<head lang="en">', '<HEAD/>', '<div>x</div>',
  '<body class="b">', '<p>x', '<img src="x">', '<svg><title>t</title></svg>', '<table><td>x</table>', '<frameset>',
  '<a href="#">a</a>', '<input type="hidden">', '<span data-rq-block="s">x</span>',
];
/** Pieces after the stop point. */
const SUFFIX = [
  ' ', '\n', '<!--c-->', 'text', ' t ', '<meta charset="utf-8">', '<link rel="icon" href="i">', '<title>T</title>',
  '<title data-rq-block="t">T</title>', '<script>x()</script>', '<style>p{}</style>', '<base href="/b/">',
  '<noscript><img src="x"></noscript>', '<noscript><link rel="a" href="b"></noscript>', '<noscript><div>d</div></noscript>',
  '<noscript>t</noscript>', '<noscript><meta http-equiv="refresh" content="0"></noscript>', '<template><p>x</p></template>',
  '<template><td>c</td></template>', '<html data-x="1">', '<html lang="fr" class="c">', '<head>', '<head lang="x">', '</head>',
  '<body class="b">', '<body>', '</body>', '</html>', '<!DOCTYPE html>', '</br>', '<p>p', '<div data-rq-block="d">x</div>',
  '<span>s</span>', '<frameset>', '<svg><title>t</title></svg>', '<table><td>x</table>', '</p>', '<br>', ' ',
];

/**
 * One synthetic document.
 * @param {number} seed sweep seed
 * @param {number} index document index
 * @returns {string}
 */
export function generateHeadDocument(seed, index) {
  const rng = makeRng(docSeed(seed, index));
  const parts = [];
  if (rng() < 0.05) parts.push('﻿');
  const nPre = Math.floor(rng() * 6);
  for (let i = 0; i < nPre; i++) parts.push(rng() < 0.08 ? pick(rng, PRE_BAD) : pick(rng, PRE));
  parts.push(rng() < 0.6 ? pick(rng, STOP_H79) : pick(rng, STOP_OTHER));
  const nSuf = Math.floor(rng() * 9);
  for (let i = 0; i < nSuf; i++) parts.push(pick(rng, SUFFIX));
  return parts.join('');
}
