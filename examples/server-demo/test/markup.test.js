// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors
//
// Markup glue of the demo: the insertion point of the manifest (H-72, H-79a) and the patch
// content rule (H-78, with the attribute rule of H-59, H-73, H-77, H-83). The markup
// analysis itself is the candidate in tools/rqp-markup/node (tested there).
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { analyzePage, documentHeadStart, passesPatchContentRule } from '../src/markup.js';

const b = (s) => Buffer.from(s, 'utf8');
const rule = (html) => passesPatchContentRule(b(html));

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

test('a document without a head start tag and without an implied head has no insertion point', () => {
  for (const doc of [
    '',
    '<html><body></body></html>', // body content first
    '<html><!-- <head> --></html>', // only in a comment
    'text<html><head></head></html>', // text before the head
    '<html>&amp;<head></head></html>', // a character reference is text
    '<html><p>x</p><head></head><body></body></html>', // another start tag before the head
    '<html><noscript></noscript><head></head></html>', // H-79a: noscript is not in the list
    '<html><template></template><head></head></html>', // H-79a: nor is template
    '<body><template><head></head></template></body>', // a head start tag in a template
    '<html></><head></head></html>', // "</>" gives no token: its bytes are not white space
    '<html>x<meta charset="utf-8">', // text before the head content
  ]) {
    assert.equal(at(doc), -1, doc);
  }
});

test('head content before any head start tag implies the head: the insertion point is its "<" (H-79a)', () => {
  for (const name of ['base', 'basefont', 'bgsound', 'link', 'meta', 'noframes', 'script', 'style', 'title', 'TITLE']) {
    const prefix = '<!DOCTYPE html>\n<!-- c --><html lang="en">\n';
    assert.equal(at(`${prefix}<${name} a="<head>">x</${name}><head>`), after(prefix), name);
  }
  assert.equal(at('<link/><head>'), 0);
  assert.equal(at('<metadata><head>'), -1);
  assert.equal(at('<html></title><head>'), after('<html></title><head>')); // an end tag is not head content
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

test('patch content rule: accepted content', () => {
  for (const html of [
    '<b>text</b>',
    '<a href="/p?a=1&amp;b=2" title="Tom &amp; Jerry">x</a>',
    '<a data-x="javascript" href="/javascript:x">x</a>',
    '<a href="/data:x" title="metadata:x">x</a>',
    '<img alt=plain src=/a.png>',
    // H-77: the decision byte is not "&", or its reference gives a character that ends a scheme.
    '<a href="?a=1&b=2">x</a>',
    '<a title="a &quot; b">x</a>',
    '<b title="&#169; 2026">x</b>',
    '<a href="&AMP;">x</a>',
    '<a href="&COLON;x">x</a>',
    '<a href="data&amp;:x">x</a>',
    '<a href="&#x3B;javascript:x">x</a>', // ";" ends the scheme before it starts
    '<a href="java&#X0073cript:x">x</a>', // the hex digits run on: U+073C, not "s"
    // H-83: raster data: images in img/source src and srcset and video poster.
    '<img src="data:image/png;base64,iVBO" alt="">',
    '<img src=" DATA:Image/GIF,x">',
    '<source src="data:image/webp,x">',
    '<video poster="data:image/jpeg;base64,x"></video>',
    '<img srcset="/a.png 1x, data:image/png;base64,iVBO 2x">',
    '<img srcset="data:image/png;base64,a,b 1x">', // a comma inside a data: URL stays in the URL
    '<img srcset="/a.png (1x, javascript:x 2x">', // inside parentheses: one descriptor, no second candidate
  ]) {
    assert.equal(rule(html), true, html);
  }
});

test('patch content rule: event handlers and javascript:, vbscript: and data: URLs are rejected', () => {
  for (const html of [
    '<b onclick="x()">',
    '<b ONLOAD=x>',
    '<b class=a onmouseover>',
    '<a href="javascript:x">',
    '<a href=" JavaScript:x">',
    '<a href="vbscript:x">',
    '<a href="\tVBScript:x">',
    '<img src="data:text/html;base64,AAAA">',
    '<a href="  DATA:text/html,x">',
    '<a href="&#x09;javascript:x">',
    '<a href="java\tscript:x">',
    '<p><i class=a CLASS="b" onclick=x></i></p>',
    // H-77: a reference at the decision byte that continues or completes a scheme.
    '<a href="&#106;avascript:x">',
    '<a href="java&#x73;cript:x">',
    '<a href="java&#X0073;cript:x">',
    '<a href="java&#x73\ncript:x">', // the trap: the reference read on the raw bytes is "s"
    '<a href="java&colon;x">',
    '<a href="&Tab;javascript:x">',
    '<a href="javascript&#58;x">',
    '<a href="vb&#115cript:x">',
    '<a href="&#0;javascript:x">',
    '<a href="&#43;x:y">', // "+", "-" and "." continue a scheme
    '<a href="a&#x2D;b:y">',
    '<a href="&#046;x:y">',
    // H-83, H-83a, DL-048: not a raster image, or not an image place.
    '<img src="data:image/svg+xml,x">',
    '<img src="data:IMAGE/SVG+XML;base64,x">',
    '<img src="data:image/svg%2Bxml,x">',
    '<img src="data:image/svg&#43;xml,x">',
    '<img src="data: image/png,x">',
    '<a href="data:image/png,x">',
    '<video src="data:image/png,x">',
    '<img poster="data:image/png,x">',
    '<svg><image href="data:image/png,x"/></svg>',
    '<img srcset="/a.png 1x, data:image/svg+xml,x 2x">',
    '<source srcset="/a.png 1x, javascript:x 2x">',
    '<img srcset="/a.png 1x,&#32;data:text/html,x 2x">',
    '<img srcset="/a.png&comma;javascript:x">',
    '<img srcset="/a.png, javascript:x">', // a trailing comma ends the candidate: no descriptors
    '<a srcset="data:image/png,x">',
  ]) {
    assert.equal(rule(html), false, html);
  }
});

test('patch content rule: the elements of PATCH_FORBIDDEN in HTML context are rejected (H-78)', () => {
  for (const name of ['script', 'style', 'link', 'meta', 'iframe', 'object', 'embed', 'base', 'basefont', 'bgsound']) {
    assert.equal(rule(`<${name}>`), false, name);
    assert.equal(rule(`<${name.toUpperCase()}/>`), false, name);
  }
  assert.equal(rule('<b>script</b><!-- <script> -->'), true);
});

test('patch content rule: in a foreign region only FOREIGN_ALLOWED start tags are allowed (H-73, H-78)', () => {
  for (const html of ['<svg><a href="#x"><text>t</text></a></svg>', '<svg><foreignObject>t</foreignObject></svg>', '<svg><script>x</script></svg>',
    '<svg><animate/></svg>', '<math><maction>x</maction></math>', '<svg><image href="/i.png"/></svg>', '<svg><g><style>x</style></g></svg>']) {
    assert.equal(rule(html), false, html);
  }
  for (const html of ['<svg><linearGradient id="g"><stop/></linearGradient><feGaussianBlur/></svg>', '<math><mfrac><mi>a</mi><mn>2</mn></mfrac></math>']) {
    assert.equal(rule(html), true, html);
  }
});

test('patch content rule: an attribute in a text-mode element is not an attribute of the content', () => {
  assert.equal(rule('<xmp><a onclick=x></xmp><b>y</b>'), true);
  assert.equal(rule('<!-- <a onclick=x> --><b>y</b>'), true);
});

test('a block above maxBlockBytes fails the rule; a block of exactly that size passes', () => {
  const LIMIT = 1024 * 1024;
  const filler = (n) => b('<p>' + 'a'.repeat(n - 7) + '</p>');
  assert.equal(passesPatchContentRule(filler(LIMIT)), true);
  assert.equal(passesPatchContentRule(filler(LIMIT + 1)), false);
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
    assert.equal(rule(html), false, html);
  }
  assert.equal(rule('<svg viewBox="0 0 8 8"><title>Icon</title><path d="M0 0h8v8z"/><use href="#i"/></svg>'), true);
  assert.equal(rule('<math><mi>x</mi><mo>+</mo></math>'), true);
});

test('attribute rule: in a region a title start tag opens no RCDATA state, in HTML context it does', () => {
  // Not valid markup (M-18), but the walk must not hide the path start tag behind it.
  assert.equal(rule('<svg><title/><path onload="x"/></svg>'), false);
  assert.equal(rule('<title><a onclick=x></title><b>y</b>'), true);
});

test('attribute rule: text-mode state changes apply again after the region closes; a self-closing svg opens none', () => {
  assert.equal(rule('<svg><g></g></svg><xmp><a onclick=x></xmp>'), true);
  assert.equal(rule('<svg/><xmp><a onclick=x></xmp>'), true);
  assert.equal(rule('<math/><title><a onclick=x></title>'), true);
  assert.equal(rule('<math><mtext><b onclick=x></b></mtext></math>'), false); // math starts a region
  assert.equal(rule('<svg></svg><xmp><a onclick=x></xmp><i onclick=x></i>'), false);
  assert.equal(rule('<svg><g><title/><g onload=x>'), false);
});

test('attribute rule: a stack deeper than the limit fails closed', () => {
  assert.equal(rule('<svg>' + '<g>'.repeat(300)), false);
  assert.equal(rule('<svg>' + '<g>'.repeat(200)), true);
  assert.equal(rule('<svg>' + '<g/>'.repeat(300) + '</svg>'), true); // self-closing elements are not open
  assert.equal(rule('<i onclick=x></i onclick=x>'), false);
  assert.equal(rule('<b></b onclick=x>'), true); // an end tag has no attributes for the rule
});
