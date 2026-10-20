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
  'font', 'small', 'main', 'header', 'article', 'object', 'marquee', 'label', 'menu', 'thead',
  'tfoot', 'applet',
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
  ['noscript', '<button>'], // M-11 item 5 (H-27)
  ['noscript', '<table>'],
  ['noscript', '<b><i>x</b></i>'],
  ['noscript', '</div><b>x</b>'],
  ['noscript', '<div><a href="#">x</a></div><table><tr><td>a</td></tr></table>'],
  ['noscript', '<td>x</td>'], // M-11 item 6 (H-38)
  ['noscript', '<col>'],
  ['noscript', '<table><caption>c</caption><tr><th>h</th></tr></table>'],
  ['script', ''],
];
// Shapes of the security review (markup.md section 2.12), with a repeat count.
const REVIEW = [
  (n) => `<a>${'<div>'.repeat(n)}x</a>`, // M-06 item 2 (H-26)
  (n) => `<nobr>${'<div>'.repeat(n)}x</nobr>`,
  (n) => `<b>${'<p>'.repeat(n)}x</b>`,
  (n) => `<a><span>${'<div>'.repeat(n)}x</span></a>`, // M-06 item 4 (H-32)
  (n) => `<nobr><span>${'<div>'.repeat(n)}x</span></nobr>`,
  (n) => `<a>${'<li><ul>'.repeat(n)}${'</li>'.repeat(n)}x</a>`,
  (n) => `<a href="#"><b><span>${'<i>'.repeat(n)}x</span></b></a>`,
  // M-06 item 3 (H-30). The ids are not in IDS: a second copy gives a duplicate.
  () => '<table><td></tbody><div data-rq-block="h">x<table></table>y</div></td></table>',
  () => '<table><tr><td></tr><p data-rq-block="i">x</p></td></tr></table>',
  () => '<table><tbody><tr><td></table><span data-rq-block="j">x</span>',
  () => '<table><tr><td>a</td></td></th></col></caption></colgroup></tr></table>',
  () => '<script type="application/rıvqen-manifest+json">{}</script>', // M-13 item 3 (H-28)
  () => '<script type="application/rivqen-manifeſt+json">{}</script>',
  (n) => '<span><div></span>'.repeat(n * 17), // M-26 (H-31)
  (n) => '<h1><span><h2>x</h1>'.repeat(n * 11),
  (n) => '<li><ul></li>'.repeat(n * 16),
  (n) => '<p><b>x</p>y'.repeat(n * 16),
  (n) => '<table><td>x</table>'.repeat(n * 8),
  // Security review round 2. M-09 condition 1 (H-35): raw text after <template><col>.
  () => '<template><col><title></template><p data-rq-block="g">fake</p></title></template>',
  () => '<template><col><textarea></template><script type="application/rivqen-manifest+json">{}</script></textarea></template>',
  () => '<template><colgroup><title></template><p data-rq-block="g">x</p></title></colgroup></template>',
  // M-10 item 2 (H-36): the form element pointer.
  (n) => '<form><object><form></form></object></form>'.repeat(n + 1),
  () => '<div><form></div><form>x</form>',
  // M-09 condition 2 (H-37): foster-parented elements above F.
  (n) => '<object><b><table><marquee><table></table></marquee></table></b></object>'.repeat(n + 1),
  () => '<object><a href="1"><table><marquee><table></table></marquee></table>x</a></object>',
  () => '<table><tr><object><td>x</td></object></tr></table>',
  () => '<table><form><tr><td>x</td></tr></form></table>',
  () => '<table><div></div><tr><td>x</td></tr></table>',
  // M-11 item 6 (H-38): table parts in noscript content in a cell or caption.
  () => '<table><tr><td><noscript><td></td></noscript><div data-rq-block="m">a<table></table>b</div></td></tr></table>',
  () => '<table><tr><td><noscript><col></noscript><div data-rq-block="m">a<table></table>b</div></td></tr></table>',
  () => '<table><caption><noscript><tr></tr></noscript><div data-rq-block="m">a<table></table>b</div></caption></table>',
  () => '<table><tr><td><noscript><table><tr><td>x</td></tr></table></noscript><div data-rq-block="n">a<table></table>b</div></td></tr></table>',
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
    if (roll < 0.44) {
      out.push(pick(REVIEW)(r.int(10)));
      // A later formatting start tag in a block runs the adoption agency again.
      const tag = pick(['a', 'nobr', 'b']);
      if (r.next() < 0.5) out.push(`<span data-rq-block="${pick(IDS)}">y<${tag}>z</${tag}>w</span>`);
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

const MANIFEST = 'RIVQEN-MANIFEST';

/**
 * The parse5 tree of a document: its blocks (elements with a data-rq-block
 * attribute, in document order), the largest DOM depth of an element (html = 1),
 * and the number of script elements whose decoded type looks like the manifest
 * type after Unicode upper-case mapping (markup.md M-13, H-28).
 */
function treeOf(html, scriptingEnabled) {
  const byteAt = [0];
  for (let i = 0; i < html.length; i++) {
    const unit = html.charCodeAt(i);
    byteAt.push(byteAt[i] + (unit < 0x80 ? 1 : unit < 0x800 || (unit >= 0xd800 && unit <= 0xdfff) ? 2 : 3));
  }
  const found = [];
  let maxDepth = 0;
  let manifests = 0;
  const walk = (node, depth) => {
    if (node.tagName !== undefined) maxDepth = Math.max(maxDepth, depth);
    const type = node.tagName === 'script' ? node.attrs.find((a) => a.name === 'type') : undefined;
    if (type !== undefined && type.value.toUpperCase().includes(MANIFEST)) manifests++;
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
    for (const child of node.childNodes ?? []) walk(child, depth + 1);
    if (node.content) walk(node.content, depth); // template contents count through the template
  };
  walk(parse(html, { sourceCodeLocationInfo: true, scriptingEnabled }), 0);
  return { blocks: found.sort((x, y) => x.start - y.start), maxDepth, manifests };
}

// markup.md M-26: with D <= 256, no element of the tree is deeper than 259.
const MAX_TREE_DEPTH = 259;

describe('agreement with the parse5 8.0.1 tree builder', () => {
  it('finds the same blocks and content offsets for every valid generated document, in both scripting views', (t) => {
    const r = rng(SEED);
    let valid = 0;
    let withBlocks = 0;
    let rejected = 0;
    for (let i = 0; i < ITERATIONS; i++) {
      const html = generate(r);
      const input = Buffer.from(html, 'utf8');
      const result = analyze(input);
      checkResult(input, result);
      if (!result.valid) {
        rejected++;
        continue;
      }
      valid++;
      if (result.blocks.length > 0) withBlocks++;
      const ours = result.blocks.map((b) => ({ id: b.id, ns: 'http://www.w3.org/1999/xhtml', start: b.start, end: b.end }));
      for (const scripting of [true, false]) {
        const tree = treeOf(html, scripting);
        const label = `scripting ${scripting}: ${JSON.stringify(html)}`;
        assert.deepEqual(tree.blocks, ours, label);
        assert.equal(tree.manifests, 0, label); // M-11 item 4, M-13
        assert.ok(tree.maxDepth <= MAX_TREE_DEPTH, `depth ${tree.maxDepth}, ${label}`); // M-26
      }
    }
    t.diagnostic(`${ITERATIONS} documents, ${valid} valid, ${withBlocks} valid with blocks, ${rejected} rejected`);
    // The generator must reach valid documents with blocks, or the test proves nothing.
    assert.ok(withBlocks >= ITERATIONS / 20, `only ${withBlocks} of ${valid} valid documents have blocks`);
  });
});
