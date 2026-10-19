// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// The attribute rule of manifest-patch section 4.2 in the study, and the proposed items
// for foreign content (report section 5.4).
//
// Changed (S4 T-08): the study rule models the pre-S4 attribute rule (H-48, H-59, H-73).
// The tests compare it with the frozen pre-S4 reference (../../baseline/reference-s3.mjs),
// not with the shipped reference, which follows H-77, H-78 and H-83 now
// (passesAttributeRule is passesPatchContentRule there).

import assert from 'node:assert/strict';
import { test } from 'node:test';
import { attributeRejected, passesAttributeRule } from '../../baseline/reference-s3.mjs';
import { makeRng } from '../../../crosscheck/src/generator.mjs';
import { attributeReason, attributesOf, blockAttributeStats, foreignReasons, walkStartTags } from '../src/attrs.mjs';
import { analyzeRelaxed, flagsOf } from '../src/markup-relax.mjs';

const attrs = (tag) => attributesOf(Buffer.from(tag), 0, tag.length);

test('attributeReason gives the item of section 4.2', () => {
  const cases = [
    ['onclick', 'x()', 'on'],
    ['onbegin', 'x()', 'on'],
    ['href', 'java&#x09;script:x', 'amp'],
    ['href', '/p?a=1&amp;b=2', null],
    ['href', ' JavaScript:x', 'scheme'],
    ['xlink:href', '\tda\nta:x', 'scheme'],
    ['to', 'vbscript:x', 'scheme'],
    ['href', '/javascript:x', null],
    ['values', 'a;javascript:x', null], // item 4 reads only the start of the value
  ];
  for (const [name, value, want] of cases) assert.equal(attributeReason(name, Buffer.from(value, 'latin1')), want, `${name}=${value}`);
});

test('property: attributeReason rejects exactly what the shipped rule rejects', () => {
  const rng = makeRng(17);
  const parts = ['on', 'href', 'x', 'javascript:', 'JAVA', 'script:', 'data:', 'vbscript:', '&amp;', '&', '\t', '\n', ' ', '\x01', ';', 'a'];
  for (let i = 0; i < 20000; i++) {
    let name = '';
    let value = '';
    for (let k = Math.floor(rng() * 3); k > 0; k--) name += parts[Math.floor(rng() * 4)];
    for (let k = Math.floor(rng() * 5); k > 0; k--) value += parts[Math.floor(rng() * parts.length)];
    const v = Buffer.from(value, 'latin1');
    assert.equal(attributeReason(name || 'x', v) !== null, attributeRejected(name || 'x', v), `${name}=${JSON.stringify(value)}`);
  }
});

test('foreignReasons: SMIL elements must not target by href or animate href or event attributes', () => {
  assert.deepEqual(foreignReasons('animate', attrs('<animate href="#x" attributeName="fill">')), ['smil-target']);
  assert.deepEqual(foreignReasons('set', attrs('<set xlink:href="#x">')), ['smil-target']);
  assert.deepEqual(foreignReasons('discard', attrs('<discard href="#x">')), ['smil-target']);
  assert.deepEqual(foreignReasons('animate', attrs('<animate attributeName=" xlink:HREF ">')), ['smil-attributename']);
  assert.deepEqual(foreignReasons('set', attrs('<set attributeName="onclick" to="x">')), ['smil-attributename']);
  assert.deepEqual(foreignReasons('animatetransform', attrs('<animateTransform attributeName="transform" values="0;90">')), []);
  assert.deepEqual(foreignReasons('animate', attrs('<animate attributeName="fill" values="red; javascript:x">')), ['values-scheme']);
  assert.deepEqual(foreignReasons('path', attrs('<path href="#x" values="a;b">')), []);
});

test('walkStartTags reads the start tags of the reference walk, also in a foreign region', () => {
  const content = Buffer.from('<b>x</b><svg><title>t</title><a href="#"><path/></a></svg><i onclick=x>');
  assert.deepEqual(walkStartTags(content).map((t) => t.name), ['b', 'svg', 'title', 'a', 'path', 'i']);
  assert.equal(passesAttributeRule(content), false); // the onclick on <i>
  assert.equal(passesAttributeRule(Buffer.from('<svg><a href="javascript:x"></a></svg>')), false);
  assert.equal(passesAttributeRule(Buffer.from('<svg><a href="/x"><path d="M0"/></a></svg>')), true);
});

test('blockAttributeStats counts names and items, not values', () => {
  const html = '<div data-rq-block="a"><i onclick="x()">x</i><svg><use href="#i"/><animate attributeName="href" values="#a"></animate></svg></div>';
  const r = analyzeRelaxed(Buffer.from(html), flagsOf('r1'), { trace: true });
  assert.equal(r.valid, true);
  const s = blockAttributeStats(Buffer.from(html), r);
  assert.equal(s.htmlTags, 1);
  assert.equal(s.foreignTags, 3);
  assert.equal(s.current.html.on, 1);
  assert.equal(s.proposed['smil-attributename'], 1);
  assert.deepEqual([...s.foreignElements.keys()].sort(), ['animate', 'svg', 'use']);
  assert.equal(s.foreignAttributes.get('href'), 1);
  assert.ok(s.rejectedCurrent && s.rejectedProposed);
  assert.ok(!JSON.stringify([...s.foreignAttributes]).includes('#i'));
});
