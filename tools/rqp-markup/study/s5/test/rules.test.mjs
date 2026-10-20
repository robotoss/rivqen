// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// One test per option rule of the S5 study (src/markup-s5.mjs) and per patch rule
// variant (src/patch-s5.mjs). Synthetic documents only.

import { test } from 'node:test';
import assert from 'node:assert/strict';
import { Tokenizer } from '../../../crosscheck/node_modules/parse5/dist/index.js';
import { makeRng } from '../../../crosscheck/src/generator.mjs';
import { analyzeS5, hasShadowRootMode } from '../src/markup-s5.mjs';
import { COMBOS } from '../src/options.mjs';
import { patchRule } from '../src/patch-s5.mjs';

const run = (html, combo) => analyzeS5(Buffer.from(html, 'utf8'), { ...COMBOS[combo], trace: true });
const verdict = (html, combo) => {
  const r = run(html, combo);
  return r.valid ? 'valid' : r.why ?? r.error;
};
const block = (inner, tag = 'div') => `<!DOCTYPE html><body><${tag} data-rq-block="a">${inner}</${tag}><span data-rq-block="b">y</span>`;

test('base: template and noscript in block content stay invalid (M-22 item 2)', () => {
  assert.equal(verdict(block('<template>t</template>'), 'base'), 'M-22.2-template');
  assert.equal(verdict(block('<noscript><img src=x></noscript>'), 'base'), 'M-22.2-noscript');
  assert.equal(verdict('<template>t</template><noscript><img src=x></noscript>' + block('x'), 'base'), 'valid');
});

test('t1: a template in block content is valid; its content follows the content rules', () => {
  for (const c of ['t1', 't2', 'tu', 't1n1', 't2n2']) {
    assert.equal(verdict(block('x<template id="row"><div class="r"><b>n</b></div></template>z'), c), 'valid', c);
    assert.equal(verdict(block('<template><template>i</template><script>s()</script></template>'), c), 'valid', c);
  }
  assert.equal(verdict(block('<template><textarea>t</textarea></template>'), 't1'), 'M-22.2');
  assert.equal(verdict(block('<template><div>x</template></div>'), 't1'), 'M-23');
  assert.equal(verdict(block('<template><div>x</div></template>', 'p'), 't1'), 'M-24a'); // stricter than the tree builder
  assert.equal(verdict(block('<a href="#">x<template><a href="#">y</a></template></a>'), 't1'), 'M-24def');
  assert.equal(verdict(block('<template><span data-rq-block="c">x</span></template>'), 't1'), 'RQP_MARKUP_NESTED');
  assert.equal(verdict(block('<template for="p">x</template>'), 't1'), 'RQP_MARKUP_STRUCTURE'); // M-10 item 3
  assert.equal(verdict(block('<template><script type="application/rivqen-manifest+json">{}</script></template>'), 't1'), 'RQP_MARKUP_RESERVED');
  assert.equal(verdict(block('<template>x'), 't1'), 'M-23'); // the block end tag does not close the template
  assert.equal(verdict('<div data-rq-block="a"><template>x', 't1'), 'M-20');
});

test('t1, t2: an attribute shadowrootmode on a template in block content rejects, any value and spelling', () => {
  for (const attrs of [' shadowrootmode="open"', ' SHADOWROOTMODE=closed', ' shadowrootmode', ' shadowrootmode="x"', ' id=a shadowrootmode=open', ' x shadowrootmode=y shadowrootmode=z']) {
    const html = block(`<template${attrs}><slot></slot></template>`);
    for (const c of ['t1', 't2', 't1n1', 't2n2']) assert.equal(verdict(html, c), 'S5-shadowrootmode', `${c}${attrs}`);
    assert.equal(verdict(html, 'tu'), 'valid', `tu${attrs}`);
  }
  // Other shadowroot attributes have no effect without shadowrootmode.
  assert.equal(verdict(block('<template shadowrootdelegatesfocus shadowrootclonable>x</template>'), 't1'), 'valid');
  // Outside block content the rule does not apply (unchanged), and a foreign template is not a template.
  assert.equal(verdict(`<div><template shadowrootmode="open"><slot></slot></template></div>${block('x')}`, 't1'), 'valid');
  assert.equal(verdict(block('<svg><template shadowrootmode="open"></template></svg>'), 't1'), 'valid');
});

test('hasShadowRootMode reads the attribute names that the WHATWG tokenizer reads (property, 20 000 tags)', () => {
  const rng = makeRng(55);
  const parts = ['shadowrootmode', 'SHADOWROOTMODE', 'shadowrootmodex', 'xshadowrootmode', 'id', '=', '"', "'", ' ', '\t', '\n', '/', 'a', '=x', '="a>b"', "='a\"b'", 'shadowRootMode', 'é', '<', '`'];
  for (let i = 0; i < 20000; i++) {
    let tag = '<template';
    const n = 1 + Math.floor(rng() * 8);
    for (let k = 0; k < n; k++) tag += parts[Math.floor(rng() * parts.length)];
    tag += '>';
    let names = null;
    const handler = {
      onStartTag(t) {
        if (names === null) names = t.attrs.map((a) => a.name);
      },
      onEndTag() {}, onComment() {}, onDoctype() {}, onCharacter() {}, onNullCharacter() {}, onWhitespaceCharacter() {}, onEof() {},
    };
    new Tokenizer({ sourceCodeLocationInfo: true }, handler).write(tag, true);
    if (names === null) continue; // not one start tag
    const bytes = Buffer.from(tag, 'utf8');
    // parse5 drops a duplicate attribute; the study reads every attribute (fail closed). Both find the name.
    assert.equal(hasShadowRootMode(bytes, 0, bytes.length), names.includes('shadowrootmode'), JSON.stringify(tag));
  }
});

test('t2: a table part or col in a template needs a table opened in the template', () => {
  const inCell = (inner) => block(`<table><tr><td>${inner}</td></tr></table>`);
  const rows = inCell('<template><tr><td>x</td></tr></template>');
  assert.equal(verdict(rows, 't1'), 'valid');
  assert.equal(verdict(rows, 't2'), 'S5-template-table-part');
  assert.equal(verdict(inCell('<template><caption></caption><td>x</td></template>'), 't2'), 'S5-template-table-part');
  assert.equal(verdict(inCell('<template><table><tr><td>x</td></tr></table></template>'), 't2'), 'valid');
  assert.equal(verdict(block('<template><table><tbody><tr><td>x</td></tr></tbody></table></template>'), 't2'), 'valid');
  // Without a table in the content M-24 item g rejects already (label for the corpus).
  assert.equal(verdict(block('<template><tr><td>x</td></tr></template>'), 't1'), 'M-24g-template');
});

test('M-09 condition 1 applies in a template of block content; the control tu drops it', () => {
  const html = block('<table><tr><td><template><col></template></td></tr></table>');
  assert.equal(verdict(html, 't1'), 'M-09.1');
  assert.equal(verdict(html, 'tu'), 'valid');
});

test('n1: a noscript in block content is valid; M-11 and the content rules apply to C', () => {
  for (const c of ['n1', 'n2', 'nu', 't1n1', 't2n2']) {
    assert.equal(verdict(block('x<noscript><img src="/p.gif"></noscript>y'), c), 'valid', c);
    assert.equal(verdict(block('<noscript><iframe src="/f"></iframe></noscript>'), c), 'valid', c);
    assert.equal(verdict(block('<noscript>Enable JavaScript.</noscript>'), c), 'valid', c);
  }
  assert.equal(verdict(block('<noscript><div>x</div></noscript>', 'p'), 'n1'), 'C:M-24a');
  assert.equal(verdict(block('<noscript><div>x</div></noscript>', 'p'), 'nu'), 'valid');
  assert.equal(verdict(block('<a href="#"><noscript><a href="#">x</a></noscript></a>'), 'n1'), 'C:M-24def');
  assert.equal(verdict(block('<ul><li><noscript><li>x</li></noscript></li></ul>'), 'n1'), 'C:M-24j');
  assert.equal(verdict(block('<noscript><td>x</td></noscript>'), 'n1'), 'C:M-11-content'); // M-11 item 6
  assert.equal(verdict(block('<noscript><textarea>t</textarea></noscript>'), 'n1'), 'C:M-22.2');
  assert.equal(verdict(block('<noscript><template>t</template></noscript>'), 't1n1'), 'C:M-11-content'); // M-11 item 3
});

test('n1: the block end tag and a block start tag in C, a hidden end tag, C to the end of the input', () => {
  assert.equal(verdict(block('<noscript></div></noscript>'), 'n1'), 'C:M-11-balance');
  assert.equal(verdict(block('<noscript><p data-rq-block="c">x</p></noscript>'), 'n1'), 'C:M-11-content'); // M-11 item 2
  assert.equal(verdict(block('<noscript><!--</noscript>--></noscript>'), 'n1'), 'C:M-11-end');
  // E3: C runs to the end of the input; C itself is valid, the block has no end tag (M-20).
  assert.equal(verdict('<div data-rq-block="a"><noscript><img src=x>', 'n1'), 'M-20');
  assert.equal(verdict(block('<noscript><script type="application/rivqen-manifest+json">{}</script></noscript>'), 'n1'), 'RQP_MARKUP_RESERVED');
});

test('n2: in C, a start tag in PCLOSE must not close a p around the noscript element', () => {
  const html = block('<p>x<noscript><div>y</div></noscript>z</p>');
  assert.equal(verdict(html, 'n1'), 'valid');
  assert.equal(verdict(html, 'n2'), 'C:S5-noscript-pclose');
  // A p opened in C, or a p below a template, is not around the noscript.
  assert.equal(verdict(block('<noscript><p>x</p><div>y</div></noscript>'), 'n2'), 'valid');
  assert.equal(verdict(block('<p>x<template><noscript><div>y</div></noscript></template></p>'), 't2n2'), 'valid');
});

test('the trace records content start tags with parents, C tags and template ends', () => {
  const html = '<div data-rq-block="a">x<template><b>n</b></template><noscript><img src=x></noscript></div>';
  const r = run(html, 't1n1');
  assert.equal(r.valid, true);
  const names = r.trace.content.map((e) => e[3]);
  assert.deepEqual(names, ['template', 'b', 'noscript']);
  const [tpl, b] = r.trace.content;
  assert.equal(b[4], tpl[0]); // the parent of b is the template
  assert.equal(b[5], true); // in a template
  assert.equal(r.trace.templates[0].end, html.indexOf('</template>'));
  const ns = r.trace.noscripts[0];
  assert.equal(ns.inBlock, true);
  assert.equal(ns.end, html.indexOf('</noscript>'));
  assert.deepEqual(ns.tags.map((t) => t[2]), ['img']);
  assert.equal(ns.tags[0][0], html.indexOf('<img'));
});

test('patch rules: shipped reads no C, p1 rejects a noscript, p2 checks the start tags of C', () => {
  const check = (inner, rule) => {
    const html = block(inner);
    const r = run(html, 't1n1');
    assert.equal(r.valid, true, inner);
    return patchRule(rule, Buffer.from(html, 'utf8'), r.trace, r.blocks[0]).failure;
  };
  assert.equal(check('<noscript><img src=x onerror=alert(1)></noscript>', 'shipped'), null);
  assert.equal(check('<noscript><img src=x onerror=alert(1)></noscript>', 'p1'), 'noscript');
  assert.equal(check('<noscript><img src=x onerror=alert(1)></noscript>', 'p2'), 'attribute');
  assert.equal(check('<noscript><iframe src=x></iframe></noscript>', 'p2'), 'element');
  assert.equal(check('<noscript><img src="/p.gif"></noscript>', 'p2'), null);
  assert.equal(check('<template><script>s()</script></template>', 'shipped'), 'element'); // template contents are read
  assert.equal(check('<template><a href="javascript:x">j</a></template>', 'p1'), 'attribute');
  assert.equal(check('<template><b>t</b></template>', 'p1'), null);
});
