// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors
//
// Markup glue of the demo: the insertion point of the manifest and the attribute rule.
// The markup analysis itself is the candidate in tools/rqp-markup/node (tested there).
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { analyzePage, documentHeadEnd, passesAttributeRule } from '../src/markup.js';

const b = (s) => Buffer.from(s, 'utf8');

const at = (doc) => documentHeadEnd(b(doc));

test('the manifest goes before the head end tag; a head end tag in a comment or raw text is not it', () => {
  const doc = '<!DOCTYPE html><html><head><!-- </head> --><title>x </head></title><style>/* </head> */</style></head><body></body></html>';
  assert.equal(at(doc), doc.lastIndexOf('</head>'));
});

test('a document without a head end tag has no insertion point', () => {
  assert.equal(at('<html><body><p>x</p></body></html>'), -1);
  assert.equal(at('<html><head><!-- </head> --></html>'), -1);
});

test('a stray </head> in the body is not the head end tag (DL-031)', () => {
  assert.equal(at('<html><body><p>x</p></head></body></html>'), -1);
  assert.equal(at('<html><head><title>t</title><body></head></body></html>'), -1);
});

test('a </head> inside a template element in the head is not the head end tag', () => {
  const doc = '<html><head><template><b></b></template></head><body></body></html>';
  assert.equal(at(doc), doc.indexOf('</head>'));
  // The </head> inside the template pops the document head from the token stack (item 3).
  assert.equal(at('<html><head><template></head></template></head><body></body></html>'), -1);
});

test('content before <head> means no head end tag', () => {
  assert.equal(at('<html><p>x</p><head></head><body></body></html>'), -1);
  assert.equal(at('text<html><head></head></html>'), -1);
});

test('white space, comments, DOCTYPE and a byte order mark may come before <head>', () => {
  const doc = '﻿ <!DOCTYPE html>\n<!-- c --><html>\n<head>\n<meta charset="utf-8">\n</head><body></body></html>';
  assert.equal(at(doc), Buffer.from(doc).indexOf('</head>'));
});

test('an svg element in the head ends the head: no head end tag follows', () => {
  assert.equal(at('<html><head><svg></svg></head><body></body></html>'), -1);
  assert.equal(at('<html><head><svg></head></svg></head><body></body></html>'), -1);
});

test('an end tag other than head, body, html and br is ignored before the head and in the head (DL-034)', () => {
  const doc = '<html></p><head><meta charset=utf-8></meta></div><title>x</title></head><body></body></html>';
  assert.equal(at(doc), doc.indexOf('</head>'));
  assert.equal(at('<html><head></br></head><body></body></html>'), -1);
  assert.equal(at('<html><head></body></head></html>'), -1);
  assert.equal(at('<html></html><head></head>'), -1);
});

test('the first head end tag in head position is the insertion point', () => {
  const doc = '<html><head></head><body></head></body></html>';
  assert.equal(at(doc), doc.indexOf('</head>'));
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
    '<a href="/data:x" title="metadata:x">x</a>',
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
    '<a href="vbscript:x">',
    '<a href="\tVBScript:x">',
    '<img src="data:text/html;base64,AAAA">',
    '<a href="  DATA:text/html,x">',    '<a href="&#x09;javascript:x">',
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
