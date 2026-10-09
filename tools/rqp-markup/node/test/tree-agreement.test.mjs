// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// markup.md section 2, principle: for a valid document, a token-only parser and a
// parser with a full WHATWG tree builder find the same blocks and the same content
// bytes. This test generates random documents and, for each document that the
// candidate accepts, compares the blocks with the tree of parse5 8.0.1
// (scripting enabled, source locations on).

import { describe, it } from 'node:test';
import assert from 'node:assert/strict';
import { parse } from 'parse5';
import { analyze } from '../src/markup.mjs';
import { checkResult, rng } from './helpers.mjs';

const ITERATIONS = Number(process.env.RQP_TREE_ITERATIONS ?? 3000);
const SEED = Number(process.env.RQP_TREE_SEED ?? 0x7ee);

const CONTAINERS = [
  'div', 'p', 'span', 'b', 'i', 'em', 'a', 'button', 'ul', 'ol', 'li', 'dl', 'dt', 'dd', 'section',
  'h1', 'h2', 'table', 'tbody', 'tr', 'td', 'th', 'caption', 'colgroup', 'select', 'option',
  'optgroup', 'ruby', 'rt', 'rp', 'rb', 'rtc', 'nobr', 'template', 'form', 'center', 'pre',
  'font', 'small', 'main', 'header', 'article', 'object', 'marquee', 'label', 'menu',
];
const VOIDS = ['br', 'img', 'hr', 'input', 'col', 'wbr', 'embed', 'meta', 'link'];
const RAW = [
  ['textarea', 'a <p data-rq-block="r1">x</p>'],
  ['style', 'p{} </p><div data-rq-block="r2">'],
  ['script', 'var s = "<p data-rq-block=\'r3\'>";'],
  ['script', '<!--<script></script><p data-rq-block="r4">--> '],
  ['title', 'T &amp; <b>'],
  ['iframe', '<p data-rq-block="r5">'],
  ['xmp', '<b>'],
  ['noscript', '<img src="a.png">'],
  ['noscript', '<p>x</p>'],
  ['noscript', '<script type="application/rivqen-manifest+json">{}</script>'], // E8
];
const BLOCK_NAMES = ['div', 'p', 'span', 'h1', 'section', 'title', 'nav', 'li', 'td'];
const IDS = ['a', 'b', 'c', 'd', 'e', 'f'];
const SVG_CHILDREN = ['g', 'path', 'title', 'desc', 'text', 'a', 'style', 'foreignObject', 'svg'];
const MATH_CHILDREN = ['mi', 'mo', 'mn', 'mrow', 'mtext'];
// Content of a foreign child: text, a tag (M-07), CDATA (E5, also with '>' for M-12).
const FOREIGN_TEXT = ['t', 't', 't', 'a < b', '<b>x</b>', '<![CDATA[a<b]]>', '<![CDATA[a>b]]>'];

function generate(r) {
  const out = [];
  const pick = r.pick;
  const element = (depth) => {
    const roll = r.next();
    if (roll < 0.12 || depth > 5) {
      out.push(pick(['x', ' ', 'text', '\u00e9', '\u{1F600}', '&amp;', '\r\n', '<!-- c -->', '<![CDATA[x]]>']));
      return;
    }
    if (roll < 0.2) {
      out.push(`<${pick(VOIDS)}${r.next() < 0.3 ? '/' : ''}>`);
      return;
    }
    if (roll < 0.27) {
      const [name, text] = pick(RAW);
      out.push(`<${name}>${text}</${name}>`);
      return;
    }
    if (roll < 0.31) {
      const root = r.next() < 0.7 ? 'svg' : 'math';
      const kids = root === 'svg' ? SVG_CHILDREN : MATH_CHILDREN;
      out.push(`<${root}>`);
      for (let i = r.int(3); i > 0; i--) {
        const k = pick(kids);
        out.push(r.next() < 0.3 ? `<${k}/>` : `<${k}>${pick(FOREIGN_TEXT)}</${k}>`);
      }
      out.push(r.next() < 0.95 ? `</${root}>` : '');
      return;
    }
    if (roll < 0.37) {
      const name = pick(BLOCK_NAMES);
      const id = pick(IDS);
      if (r.next() < 0.15) {
        out.push(`<script type="application/json" data-rq-block="${id}">${pick(['{"a":1}', '[]', '{', '"\\u00e9"'])}</script>`);
        return;
      }
      out.push(`<${name} data-rq-block="${id}">`);
      for (let i = r.int(4); i > 0; i--) element(depth + 1);
      if (r.next() < 0.95) out.push(`</${name}>`);
      return;
    }
    if (roll < 0.4) {
      out.push(`</${pick(CONTAINERS)}>`); // stray end tag
      return;
    }
    const name = pick(CONTAINERS);
    out.push(`<${name}${r.next() < 0.05 ? '/' : ''}>`);
    for (let i = r.int(4); i > 0; i--) element(depth + 1);
    if (r.next() < 0.9) out.push(`</${name}>`);
  };
  const head = r.next() < 0.5 ? '<head><title data-rq-block="t">T</title></head>' : '';
  out.push(`<!DOCTYPE html><html>${head}<body>`);
  for (let i = 1 + r.int(5); i > 0; i--) element(0);
  out.push('</body></html>');
  return out.join('');
}

/** Blocks in a parse5 tree: elements with a data-rq-block attribute, in document order. */
function treeBlocks(html) {
  const byteAt = [0];
  for (let i = 0; i < html.length; i++) {
    const unit = html.charCodeAt(i);
    byteAt.push(byteAt[i] + (unit < 0x80 ? 1 : unit < 0x800 || (unit >= 0xd800 && unit <= 0xdfff) ? 2 : 3));
  }
  const found = [];
  const walk = (node) => {
    const attr = node.attrs?.find((a) => a.name === 'data-rq-block');
    if (attr !== undefined) {
      const loc = node.sourceCodeLocation;
      found.push({
        id: attr.value,
        ns: node.namespaceURI,
        start: loc?.startTag ? byteAt[loc.startTag.endOffset] : null,
        end: loc?.endTag ? byteAt[loc.endTag.startOffset] : null,
      });
    }
    for (const child of node.childNodes ?? []) walk(child);
    if (node.content) walk(node.content);
  };
  walk(parse(html, { sourceCodeLocationInfo: true, scriptingEnabled: true }));
  return found.sort((x, y) => x.start - y.start);
}

describe('agreement with the parse5 8.0.1 tree builder', () => {
  it('finds the same blocks and content offsets for every valid generated document', (t) => {
    const r = rng(SEED);
    let valid = 0;
    let withBlocks = 0;
    for (let i = 0; i < ITERATIONS; i++) {
      const html = generate(r);
      const input = Buffer.from(html, 'utf8');
      const result = analyze(input);
      checkResult(input, result);
      if (!result.valid) continue;
      valid++;
      if (result.blocks.length > 0) withBlocks++;
      const ours = result.blocks.map((b) => ({ id: b.id, ns: 'http://www.w3.org/1999/xhtml', start: b.start, end: b.end }));
      assert.deepEqual(treeBlocks(html), ours, JSON.stringify(html));
    }
    t.diagnostic(`${ITERATIONS} documents, ${valid} valid, ${withBlocks} valid with blocks`);
    // The generator must reach valid documents with blocks, or the test proves nothing.
    assert.ok(withBlocks >= ITERATIONS / 20, `only ${withBlocks} of ${valid} valid documents have blocks`);
  });
});
