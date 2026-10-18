// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors
//
// Markup glue of the demo: the insertion point of the manifest (H-72) and the attribute rule.
// The markup analysis itself is the candidate in tools/rqp-markup/node (tested there).
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { analyzePage, documentHeadStart, passesAttributeRule } from '../src/markup.js';

const b = (s) => Buffer.from(s, 'utf8');

const at = (doc) => documentHeadStart(b(doc));
const after = (prefix) => b(prefix).length;

test('the manifest goes immediately after the head start tag; "<head>" in a comment is not it (H-72)', () => {
  const prefix = '<!DOCTYPE html><!-- <head> --><html><head>';
  assert.equal(at(`${prefix}<title>x</title></head><body></body></html>`), after(prefix));
});

test('white space, comments, DOCTYPE tokens, html start tags and a byte order mark may come before <head>', () => {
  const prefix = '﻿ <!DOCTYPE html>\n<!-- c --><html>\n<html lang="en"><HEAD class="a>b">';
  assert.equal(at(`${prefix}<meta charset="utf-8"></head><body></body></html>`), after(prefix));
});

test('white space before the head start tag is tab, line feed, form feed, carriage return or space; vertical tab is not', () => {
  for (const ws of ['\t', '\n', '\f', '\r', ' ']) {
    assert.equal(at(`<html>${ws}<head></head></html>`), after(`<html>${ws}<head>`), JSON.stringify(ws));
  }
  assert.equal(at('<html>\v<head></head></html>'), -1);
});

test('what comes after the head start tag does not matter: the head needs no end tag', () => {
  for (const rest of ['</head>', '<body>', '<div>x</div>', 'text', '<svg></svg>', '<noscript><img src=x></noscript>', '</br>', '<head>', '']) {
    assert.equal(at(`<html><head>${rest}<body></body></html>`), after('<html><head>'), rest);
  }
});

test('a document without a head start tag of the document has no insertion point', () => {
  for (const doc of [
    '',
    '<html><title>x</title></head><body></body></html>', // no <head> start tag
    '<html><!-- <head> --></html>', // only in a comment
    'text<html><head></head></html>', // text before the head
    '<html>&amp;<head></head></html>', // a character reference is text
    '<html><p>x</p><head></head><body></body></html>', // another start tag before the head
    '<html><meta charset="utf-8"><head></head></html>', // head content before the head (the head is implied)
    '<body><template><head></head></template></body>', // a head start tag in a template
    '<html></><head></head></html>', // "</>" gives no token: its bytes are not white space
  ]) {
    assert.equal(at(doc), -1, doc);
  }
});

test('an end tag other than head, body, html and br is ignored before the head (DL-034)', () => {
  const prefix = '</p><html></div></META><head>';
  assert.equal(at(`${prefix}</head><body></body></html>`), after(prefix));
  for (const end of ['</br>', '</BR>', '</body>', '</html>', '</head>']) {
    assert.equal(at(`<html>${end}<head></head><body></body></html>`), -1, end);
  }
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

test('a block above maxBlockBytes fails the attribute rule; a block of exactly that size passes', () => {
  const LIMIT = 1024 * 1024;
  const filler = (n) => b('<p>' + 'a'.repeat(n - 7) + '</p>');
  assert.equal(passesAttributeRule(filler(LIMIT)), true);
  assert.equal(passesAttributeRule(filler(LIMIT + 1)), false);
});

test('attribute rule: every start tag of an svg or math region is checked (H-73)', () => {
  for (const html of [
    '<svg><title>x</title><path onload="x"/></svg>',
    '<svg><use xlink:href="javascript:x"/></svg>',
    '<svg><desc>d</desc><use href=" \tData:image/svg+xml,x"/></svg>',
    '<svg><text y="&#x6a;">t</text></svg>',
    '<svg><g><g><rect ONCLICK=x /></g></g></svg>',
    '<math><mrow><mo onclick=x>+</mo></mrow></math>',
  ]) {
    assert.equal(passesAttributeRule(b(html)), false, html);
  }
  assert.equal(passesAttributeRule(b('<svg viewBox="0 0 8 8"><title>Icon</title><path d="M0 0h8v8z"/><use href="#i"/></svg>')), true);
  assert.equal(passesAttributeRule(b('<math><mi>x</mi><mo>+</mo></math>')), true);
});

test('attribute rule: in a region a title start tag opens no RCDATA state, in HTML context it does', () => {
  // Not valid markup (M-18), but the walk must not hide the path start tag behind it.
  assert.equal(passesAttributeRule(b('<svg><title/><path onload="x"/></svg>')), false);
  assert.equal(passesAttributeRule(b('<title><a onclick=x></title><b>y</b>')), true);
});

test('attribute rule: text-mode state changes apply again after the region closes; a self-closing svg opens none', () => {
  assert.equal(passesAttributeRule(b('<svg><g></g></svg><style><a onclick=x></style>')), true);
  assert.equal(passesAttributeRule(b('<svg/><style><a onclick=x></style>')), true);
  assert.equal(passesAttributeRule(b('<svg></svg><style><a onclick=x></style><i onclick=x></i>')), false);
  assert.equal(passesAttributeRule(b('<svg><g><title/><g onload=x>')), false);
});

test('attribute rule: a stack deeper than the limit fails closed', () => {
  assert.equal(passesAttributeRule(b('<svg>' + '<g>'.repeat(300))), false);
  assert.equal(passesAttributeRule(b('<svg>' + '<g>'.repeat(200))), true);
});

