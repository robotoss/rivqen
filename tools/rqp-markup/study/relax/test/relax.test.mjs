// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// Behavior of the three relaxations of the study rule (report section 5).

import assert from 'node:assert/strict';
import { test } from 'node:test';
import { docSeed, generateDocument } from '../../../crosscheck/src/generator.mjs';
import { resultKey, subsetsOf } from '../src/combos.mjs';
import { generateRelaxDocument } from '../src/gen.mjs';
import { BASE, COMBOS, analyzeRelaxed, checkNoscript, flagsOf } from '../src/markup-relax.mjs';
import { Oracle } from '../src/oracle.mjs';

const run = (html, combo) => analyzeRelaxed(Buffer.from(html, 'utf8'), flagsOf(combo));
const valid = (html, combo) => run(html, combo).valid;
const why = (html, combo) => run(html, combo).why;

test('r1: a balanced svg or math subtree in html block content is valid', () => {
  for (const html of [
    '<div data-rq-block="a">x<svg viewBox="0 0 1 1"><path d="M0"/></svg>y</div>',
    '<span data-rq-block="a"><svg><g><path></path></g></svg></span>',
    '<p data-rq-block="a"><svg><title>t</title><desc>a &lt; b</desc></svg></p>',
    '<div data-rq-block="a"><math><mi>x</mi><mo>+</mo></math></div>',
    '<span data-rq-block="a"><svg/><math/></span>',
    '<div data-rq-block="a"><svg><image href="x"></image></svg></div>',
    '<div data-rq-block="a"><svg><a href="/x"><text>t</text></a></svg></div>',
    '<div data-rq-block="a"><svg><![CDATA[x]]></svg></div>',
  ]) {
    assert.equal(valid(html, 'base'), false, html);
    assert.equal(valid(html, 'r1'), true, html);
  }
});

test('r1: the blocks of a valid document have the offsets of the content', () => {
  const html = '<div data-rq-block="a">x<svg><path/></svg>y</div>';
  const r = run(html, 'r1');
  assert.deepEqual(r.blocks.map((b) => [b.id, b.start, b.end]), [['a', 23, html.indexOf('</div>')]]);
});

test('r1: the foreign region rules stay in force inside block content', () => {
  const cases = [
    ['<div data-rq-block="a"><svg><div>x</div></svg></div>', 'M-07.1'], // breakout
    ['<div data-rq-block="a"><svg><foreignObject><div>x</div></foreignObject></svg></div>', 'M-07.3'],
    ['<div data-rq-block="a"><svg><desc><!--c--></desc></svg></div>', 'M-07.5'],
    ['<div data-rq-block="a"><svg><style/></svg></div>', 'r1-forbidden:style'],
    ['<div data-rq-block="a"><svg><script>x</script></svg></div>', 'r1-forbidden:script'],
    ['<div data-rq-block="a"><svg><style>p{}</style></svg></div>', 'r1-forbidden:style'],
    ['<div data-rq-block="a"><svg><path></svg></div>', 'M-23-foreign'], // unclosed child without r3
    ['<div data-rq-block="a"><svg></p></svg></div>', 'M-23-foreign'], // stray end tag
    ['<div data-rq-block="a"><svg><path></div>', 'M-23-foreign'],
    ['<div data-rq-block="a"><svg><svg data-rq-block="b"></svg></svg></div>', 'M-17'],
    ['<div data-rq-block="a"><svg>', 'M-20'],
  ];
  for (const [html, rule] of cases) assert.equal(why(html, 'r1'), rule, html);
});

test('r1: svg in table context of the content is still foster-parented content (M-24 k)', () => {
  assert.equal(why('<div data-rq-block="a"><table><svg></svg></table></div>', 'r1'), 'M-24.k');
});

test('r1: the other names of FORBIDDEN_CONTENT stay forbidden in HTML context', () => {
  for (const name of ['script', 'style', 'iframe', 'template', 'noscript', 'select']) {
    assert.equal(why(`<div data-rq-block="a"><${name}></${name}></div>`, 'r1'), `M-22.2:${name}`);
  }
});

test('r3: the icon shape outside blocks is valid; the H-32 shapes with special crossings stay invalid', () => {
  const icon = '<a href="#"><svg><path d="M0"></svg></a><span data-rq-block="x">y<a>z</a>w</span>';
  assert.equal(why(icon, 'base'), 'M-06.4');
  assert.equal(valid(icon, 'r3'), true);
  const special = '<a href="#"><span><div></span><svg><path></svg></a><span data-rq-block="x">y<a>z</a>w</span>';
  assert.equal(why(special, 'r3'), 'M-06.4');
  // H-26 (M-06 item 2) is not relaxed: a formatting end tag crossing foreign elements.
  assert.equal(why('<svg><a><text>t</a></svg>', 'r3w'), 'M-06.2');
});

test('r3: scope root marks a crossing by an inner end tag; scope region does not', () => {
  const html = '<a href="#"><svg><g><path></g></svg></a>';
  assert.equal(why(html, 'r3'), 'M-06.4');
  assert.equal(valid(html, 'r3w'), true);
});

test('r3 with r1: the root end tag can cross foreign children inside block content', () => {
  const html = '<div data-rq-block="a"><a href="#"><svg><path></svg></a></div>';
  assert.equal(why(html, 'r1'), 'M-23-foreign');
  assert.equal(valid(html, 'r1+r3'), true);
  assert.equal(why('<div data-rq-block="a"><svg><g><path></g></svg></div>', 'r1+r3'), 'M-23-foreign');
  assert.equal(valid('<div data-rq-block="a"><svg><g><path></g></svg></div>', 'r1+r2+r3w'), true);
});

test('r3 keeps the depth counter: a crossing does not give back its units (M-26)', () => {
  const icons = '<span><svg><path></svg></span>'.repeat(130);
  assert.equal(run(icons, 'r3').error, 'RQP_MARKUP_LIMIT');
  assert.equal(valid('<span><svg><path/></svg></span>'.repeat(130), 'r3'), true);
});

test('r2: svg in noscript content is valid when the content is balanced', () => {
  const html = '<p>x</p><noscript><img src="x"><svg><path d="M0"/></svg></noscript><p data-rq-block="b">x</p>';
  assert.equal(why(html, 'base'), 'M-11.3:svg');
  assert.equal(valid(html, 'r2'), true);
  const crossing = '<p>x</p><noscript><svg><path></svg></noscript>';
  assert.equal(why(crossing, 'r2'), 'M-11.5-foreign');
  assert.equal(valid(crossing, 'r2+r3'), true);
});

test('r2: the foreign region rules apply in noscript content', () => {
  const cases = [
    ['<noscript><svg><div></div></svg></noscript>', 'M-11.r2-M-07.1'],
    ['<noscript><svg><desc><b>x</b></desc></svg></noscript>', 'M-11.r2-M-07.3'],
    ['<noscript><svg><desc><!--c--></desc></svg></noscript>', 'M-11.r2-M-07.5'],
    ['<noscript><svg><title/></svg></noscript>', 'M-11.r2-M-07.4'],
    ['<noscript><svg><![CDATA[a>b]]></svg></noscript>', 'M-11.r2-M-12'],
    ['<noscript><svg><g></svg></noscript>', 'M-11.5-foreign'],
    ['<noscript><svg><path>', 'M-11.5'],
    ['<noscript><math></math></noscript>', 'M-11.3:math'],
    ['<noscript><svg><select></select></svg></noscript>', 'M-11.3:select'],
    ['<noscript><svg><td></td></svg></noscript>', 'M-11.6'],
    ['<noscript><svg><p data-rq-block="a"></p></svg></noscript>', 'M-11.2'],
  ];
  for (const [html, rule] of cases) assert.equal(why(html, 'r2'), rule, html);
  assert.equal(run('<noscript><svg><script type="application/rivqen-manifest+json">{}</script></svg></noscript>', 'r2').error, 'RQP_MARKUP_RESERVED');
});

test('r2: in a foreign region of C, a start tag does not change the tokenizer state', () => {
  // In HTML context a style start tag makes RAWTEXT; in svg it is an SVG style element
  // whose content must be text (M-07 item 4), so the end tag closes it.
  assert.equal(valid('<p>x</p><noscript><svg><style>a{}</style></svg></noscript>', 'r2'), true);
  assert.equal(why('<p>x</p><noscript><svg><style><g></style></svg></noscript>', 'r2'), 'M-11.r2-M-07.3');
});

test('checkNoscript with r2 off is the shipped check', () => {
  for (const c of ['<img src=x>', '<svg></svg>', '<b></i>', '<table><tr></tr></table>', '<td></td>', '<p>']) {
    const r = checkNoscript(Buffer.from(c), BASE);
    const expect = c === '<img src=x>' || c === '<table><tr></tr></table>' ? null : 'RQP_MARKUP_STRUCTURE';
    assert.equal(r?.code ?? null, expect, c);
  }
});

test('property: a larger combination accepts every document of a smaller one, with the same result', () => {
  for (let i = 0; i < 3000; i++) {
    const text = i % 2 === 0 ? generateRelaxDocument(docSeed(91, i)) : generateDocument(docSeed(92, i), { profile: 'review2' });
    const bytes = Buffer.from(text, 'utf8');
    const results = new Map(COMBOS.map((c) => [c, analyzeRelaxed(bytes, flagsOf(c))]));
    for (const c of COMBOS) {
      for (const sub of subsetsOf(c)) {
        if (results.get(sub).valid) assert.equal(resultKey(results.get(c)), resultKey(results.get(sub)), `${c} over ${sub}: ${text}`);
      }
    }
  }
});

test('property: parse5 agrees in both views on documents that only a relaxation accepts', () => {
  let gained = 0;
  for (let i = 0; i < 4000; i++) {
    const text = generateRelaxDocument(docSeed(93, i));
    const bytes = Buffer.from(text, 'utf8');
    if (analyzeRelaxed(bytes, BASE).valid) continue;
    const r = analyzeRelaxed(bytes, flagsOf('r1+r2+r3w'), { trace: true });
    if (!r.valid) continue;
    gained++;
    // The head part of H-42 (noscript in head, scripting disabled) is study T-02.
    const findings = new Oracle(bytes).check(r).filter((f) => !/^noscript-.*-head$/.test(f.kind));
    assert.deepEqual(findings, [], text);
  }
  assert.ok(gained > 500, `too few gained documents: ${gained}`);
});

test('the trace has the start tags of block content with their foreign flag', () => {
  const html = '<div data-rq-block="a"><b>x</b><svg><path/></svg></div>';
  const r = analyzeRelaxed(Buffer.from(html), flagsOf('r1'), { trace: true });
  assert.deepEqual(r.trace.content.map(([at, f, , n]) => [at, f, n]), [[23, false, 'b'], [31, false, 'svg'], [36, true, 'path']]);
  assert.deepEqual(r.trace.blockNames, ['div']);
});

test('analyzeRelaxed: limits and encoding come first; a non-Uint8Array input throws', () => {
  assert.equal(analyzeRelaxed(Buffer.alloc(5 * 1024 * 1024 + 1, 0x20), flagsOf('r1')).why, 'M-01');
  assert.equal(analyzeRelaxed(Buffer.from([0xff]), flagsOf('r1')).why, 'M-02');
  assert.throws(() => analyzeRelaxed('x'), TypeError);
});
