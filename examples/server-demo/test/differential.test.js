// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors
//
// Differential property tests: the demo's own head rule and patch content rule (DL-036,
// DL-046: a second implementation on purpose) against the reference of
// tools/rqp-protocol. The reference is imported by tests only. Seeded, deterministic.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { documentHeadStart, passesPatchContentRule } from '../src/markup.js';
import {
  documentHeadStart as referenceHeadStart,
  passesPatchContentRule as referenceRule,
} from '../../../tools/rqp-protocol/src/markup.mjs';

function prng(seed) {
  let s = seed >>> 0;
  return (n) => {
    s = (Math.imul(s, 1103515245) + 12345) >>> 0;
    return Math.floor((s / 2 ** 32) * n);
  };
}
const one = (rnd, list) => list[rnd(list.length)];

const VALUE_PARTS = [
  'javascript', 'JaVaScRiPt', 'vbscript', 'data', 'DATA', 'https', 'java', 'x', ':', ':', '&colon;', '&#58;', '&#x3a', '&COLON;',
  '&#106;', '&#x6A', '&#0000115;', '&#x73\n', '&#169;', '&quot;', '&amp;', '&Tab;', '&NewLine;', '&fjlig;', '&plus;', '&period;',
  '&#32;', '&#x09;', '&#44;', '&comma;', '&lpar;', '&#x28;', '\t', '\n', '\r', ' ', '\x01', '\x0c', ',', '(', ')', ' 1x', ' 2x',
  'image/png;base64,iVBO', 'image/svg+xml,x', 'image/svg%2Bxml,x', 'image/svg&#43;xml,x', 'image/gif', 'text/html,x', '/a.png',
  '?a=1&b=2', '&', '&#', '&#x', ';', '&#43;', '&#x2D;', '&#046;', '&#x2e', ', ',
];
const ELEMENTS = ['a', 'img', 'img', 'img', 'source', 'source', 'video', 'video', 'b', 'svg', 'math', 'use', 'image', 'p', 'p', 'a', 'textarea', 'title', 'xmp',
  'g', 'path', 'text', 'mi'];
const RARE_ELEMENTS = ['script', 'style', 'iframe', 'link', 'meta', 'object', 'embed', 'base', 'foreignObject', 'animate'];
const NAMES = ['href', 'src', 'src', 'srcset', 'srcset', 'poster', 'title', 'alt', 'onclick', 'xlink:href', 'data-x', 'SRCSET'];

/** Random block content: start tags with random attributes, text, end tags. */
function content(rnd) {
  let out = '';
  const n = 1 + rnd(3);
  for (let i = 0; i < n; i++) {
    const element = rnd(20) === 0 ? one(rnd, RARE_ELEMENTS) : one(rnd, ELEMENTS);
    let attrs = '';
    for (let k = rnd(3); k > 0; k--) {
      let value = '';
      for (let j = 1 + rnd(5); j > 0; j--) value += one(rnd, VALUE_PARTS);
      const q = one(rnd, ['"', "'", '']);
      attrs += ` ${one(rnd, NAMES)}=${q}${q === '' ? value.replace(/[\s>"'`=<]/g, '') : value.replaceAll(q, '')}${q}`;
    }
    out += `<${element}${attrs}${rnd(4) === 0 ? '/' : ''}>${one(rnd, ['x', '', '<b>y</b>', '</g>'])}`;
    if (rnd(2) === 0) out += `</${element}>`;
  }
  return out;
}

test('property: the demo patch content rule gives the reference verdict for 20 000 random blocks', (t) => {
  const rnd = prng(0xd1ff);
  let rejected = 0;
  for (let i = 0; i < 20000; i++) {
    const c = Buffer.from(content(rnd), 'utf8');
    const want = referenceRule(c);
    assert.equal(passesPatchContentRule(c), want, c.toString('utf8'));
    if (!want) rejected++;
  }
  // Both verdicts occur often, so the comparison is not trivial.
  t.diagnostic(`${rejected} of 20000 rejected`);
  assert.ok(rejected > 4000 && rejected < 18000, `${rejected} rejected`);
});

const PREFIX_PARTS = [
  ' ', '\n', '\t', '\f', '\r', '\v', '﻿', '<!DOCTYPE html>', '<!-- c -->', '<!-- <head> -->', '<html>', '<html lang="en">', '</p>',
  '</br>', '</head>', '</title>', '</>', 'x', '&amp;', '<head>', '<HEAD lang=en>', '<head/>', '<meta charset="utf-8">', '<title>t</title>',
  '<link rel=icon>', '<script>s</script>', '<style>p{}</style>', '<base href=/>', '<noframes>n</noframes>', '<noscript>n</noscript>',
  '<template>t</template>', '<body>', '<p>', '<svg>', '<metadata>',
];

test('property: the demo insertion point is the reference insertion point for 20 000 random documents (H-72, H-79a)', (t) => {
  const rnd = prng(0x79a);
  let implied = 0;
  for (let i = 0; i < 20000; i++) {
    let doc = '';
    for (let j = 1 + rnd(6); j > 0; j--) doc += one(rnd, PREFIX_PARTS);
    const bytes = Buffer.from(doc, 'utf8');
    const want = referenceHeadStart(bytes);
    assert.equal(documentHeadStart(bytes), want, JSON.stringify(doc));
    if (want >= 0 && bytes[want] === 0x3c) implied++;
  }
  t.diagnostic(`${implied} implied heads`);
  assert.ok(implied > 1000, `${implied} implied heads`);
});
