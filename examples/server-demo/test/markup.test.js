// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors
//
// Markup glue of the demo: the insertion point of the manifest and the attribute rule.
// The markup analysis itself is the candidate in tools/rqp-markup/node (tested there).
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { analyzePage, firstHeadEnd, passesAttributeRule } from '../src/markup.js';

const b = (s) => Buffer.from(s, 'utf8');

test('the manifest goes before the first </head> in HTML context, not in a comment, raw text or svg', () => {
  const doc = '<html><head><!-- </head> --><title>x </head></title><style>/* </head> */</style><svg></head></svg></head><body></body></html>';
  assert.equal(firstHeadEnd(b(doc)), doc.lastIndexOf('</head>'));
});

test('a document without a </head> in HTML context has no insertion point', () => {
  assert.equal(firstHeadEnd(b('<html><body><p>x</p></body></html>')), -1);
  assert.equal(firstHeadEnd(b('<html><head><!-- </head> --></html>')), -1);
});

test('the first of two </head> end tags is the insertion point', () => {
  const doc = '<html><head></head><body></head></body></html>';
  assert.equal(firstHeadEnd(b(doc)), doc.indexOf('</head>'));
});

test('the candidate analysis is used: a duplicate block id is invalid, blocks have byte offsets', () => {
  const bad = analyzePage(b('<html><head></head><body><p data-rq-block="a">1</p><p data-rq-block="a">2</p></body></html>'));
  assert.equal(bad.valid, false);
  const ok = analyzePage(b('<html><head></head><body><p data-rq-block="a">é1</p></body></html>'));
  assert.deepEqual(ok.blocks.map((x) => [x.id, x.format, x.end - x.start]), [['a', 'html', 3]]);
});

test('attribute rule: accepted values', () => {
  for (const html of [
    '<b>text</b>',
    '<a href="/p?a=1&amp;b=2" title="Tom &amp; Jerry">x</a>',
    '<a data-x="javascript" href="/javascript:x">x</a>',
    '<img alt=plain src=/a.png>',
    '<script type="x">var onclick = "&";</script>',
  ]) {
    assert.equal(passesAttributeRule(b(html)), true, html);
  }
});

test('attribute rule: event handlers, javascript: URLs and other character references are rejected', () => {
  for (const html of [
    '<b onclick="x()">',
    '<b ONLOAD=x>',
    '<b class=a onmouseover>',
    '<a href="javascript:x">',
    '<a href=" JavaScript:x">',
    '<a href="&#x09;javascript:x">',
    '<a href="java\tscript:x">',
    '<a href="java&amp;#x09;script:x">x</a><i href="&#106;avascript:x">',
    '<a title="a &quot; b">',
    '<a href="?a=1&b=2">',
    '<a href="&AMP;">',
    '<p><i class=a CLASS="b" onclick=x></i></p>',
  ]) {
    assert.equal(passesAttributeRule(b(html)), false, html);
  }
});

test('attribute rule: an attribute in a text-mode element is not an attribute of the content', () => {
  assert.equal(passesAttributeRule(b('<style><a onclick=x></style><b>y</b>')), true);
  assert.equal(passesAttributeRule(b('<!-- <a onclick=x> --><b>y</b>')), true);
});
