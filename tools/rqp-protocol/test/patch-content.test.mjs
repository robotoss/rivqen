// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors
//
// The patch content rule of manifest-patch section 4, item 1 (H-73, H-78) and the edges of
// the attribute rule of section 4.2 (H-77, H-83, DL-048), one behavior per test. The name
// lists are written here from the spec text, so a changed list in src/markup.mjs fails.

import assert from 'node:assert/strict';
import { test } from 'node:test';
import { analyze, attributeRejected, patchContentFailure, srcsetUrls } from '../src/markup.mjs';

const b = (s) => Buffer.from(s, 'latin1');
const why = (s) => patchContentFailure(b(s));

/** Manifest-patch section 4, item 1.1 (H-78). */
const SPEC_PATCH_FORBIDDEN = ['base', 'basefont', 'bgsound', 'embed', 'iframe', 'link', 'meta', 'noscript', 'object', 'script', 'style'];

/** Manifest-patch section 4, FOREIGN_ALLOWED (H-73). */
const SPEC_FOREIGN_ALLOWED = (
  'svg g path circle ellipse line polyline polygon rect defs symbol use title desc lineargradient radialgradient stop ' +
  'clippath mask pattern marker text tspan filter feblend fecolormatrix fecomponenttransfer fecomposite fedisplacementmap ' +
  'fedropshadow feflood fefunca fefuncb fefuncg fefuncr fegaussianblur femerge femergenode femorphology feoffset ' +
  'feturbulence math mrow mi mo mn ms mtext mspace msub msup msubsup mfrac msqrt mroot mstyle munder mover munderover ' +
  'mmultiscripts mprescripts none mtable mtr mtd mpadded mphantom merror semantics annotation'
).split(' ');

test('item 1.1: each start tag of PATCH_FORBIDDEN in HTML context rejects, also self-closing and in upper case', () => {
  for (const name of SPEC_PATCH_FORBIDDEN) {
    for (const tag of [`<${name}>`, `<${name.toUpperCase()}/>`, `<b>x</b><${name} a="1">`]) assert.equal(why(tag), 'element', tag);
  }
  for (const name of ['img', 'div', 'a', 'textarea', 'video', 'source', 'template', 'svg', 'math']) assert.equal(why(`<${name}>`), null, name);
});

test('item 1.2: in a foreign region each FOREIGN_ALLOWED start tag passes, every other start tag rejects', () => {
  for (const root of ['svg', 'math']) {
    for (const name of SPEC_FOREIGN_ALLOWED) assert.equal(why(`<${root}><${name}/></${root}>`), null, `${root} ${name}`);
    for (const name of ['a', 'image', 'foreignobject', 'animate', 'set', 'script', 'style', 'textpath', 'feimage', 'maction', 'annotation-xml', 'div', 'img', 'x-future']) {
      assert.equal(why(`<${root}><${name}/></${root}>`), 'foreign', `${root} ${name}`);
    }
  }
  // Mixed case as the tokenizer lowers it.
  assert.equal(why('<svg><linearGradient><feGaussianBlur/></linearGradient></svg>'), null);
});

test('the first failure in document order is the reason', () => {
  assert.equal(why('<script>x</script><b onclick=x>'), 'element');
  assert.equal(why('<b onclick=x><script>x</script>'), 'attribute');
  assert.equal(why('<svg><a/></svg><script>x</script>'), 'foreign');
});

test('item 2 (H-77): a reference at the decision byte rejects only for a scheme character, ":" or 00 to 20', () => {
  const rejected = [
    '&#x5A;x', '&#x7a;x', '&#65;x', '&#48;x', '&#x39;x', '&#43;x', '&#45;x', '&#46;x', '&#58;x', '&#0;javascript:x', '&#32;javascript:x',
    'z&#58;x', 'Z&#58;x', 'a+&#58;x', 'a-&#58;x', 'a.&#58;x', 'a9&#58;x', 'a\tb&#58;x', 'a&plus;b:x', 'a&period;b:x', 'x&fjlig;', '\x01 &NewLine;x',
  ];
  const accepted = [
    '&#x40;x', '&#x5B;x', '&#x60;x', '&#x7B;x', '&#x2F;x', '&#x2f;x', '&#x2a;x', '&#x3B;x', '&#128;x', '&#x110000;x', '&#9999999999999;x',
    '&a106;x', '&#;javascript:x', '&#x;javascript:x', 'x colon;', 'x &#58;y', '&Plus;x', '&tab;x', '&colon', 'a&amp;b:x',
  ];
  for (const v of rejected) assert.equal(attributeRejected('href', b(v), 'a', true), true, JSON.stringify(v));
  for (const v of accepted) assert.equal(attributeRejected('href', b(v), 'a', true), false, JSON.stringify(v));
});

test('item 4.2 (H-83, DL-048 b): only the type part of a raster data: image is checked for "%" and "&"', () => {
  const img = (v) => attributeRejected('src', b(v), 'img', true);
  assert.equal(img('data:image/png;x=%41,y'), false);
  assert.equal(img('data:image/png,%26&amp;'), false);
  assert.equal(img('data:image/png'), false);
  assert.equal(img('data:image/p%6eg,x'), true);
  assert.equal(img('data:image/png&#59;x,y'), true);
  assert.equal(img('data:image/svg+xmlx,y'), true); // the prefix decides, as the text says
  // Without the element and the context the exception does not apply (fail closed).
  assert.equal(attributeRejected('src', b('data:image/png,x')), true);
  assert.equal(attributeRejected('src', b('data:image/png,x'), 'img', 1), true);
  assert.equal(attributeRejected('poster', b('data:image/png,x'), 'video', true), false);
  assert.equal(attributeRejected('src', b('data:image/png,x'), 'video', true), true);
});

/** Item 4.2 (H-83a, security review M1): the raster types, written here from the spec text. */
const SPEC_RASTER_TYPES = [
  'image/png', 'image/apng', 'image/jpeg', 'image/jpg', 'image/gif', 'image/webp', 'image/avif', 'image/bmp', 'image/x-icon',
  'image/vnd.microsoft.icon',
];

test('item 4.2 (M1): each raster type of the allow-list passes in img src, source src and srcset, and video poster', () => {
  for (const type of SPEC_RASTER_TYPES) {
    for (const [element, name] of [['img', 'src'], ['source', 'src'], ['img', 'srcset'], ['source', 'srcset'], ['video', 'poster']]) {
      assert.equal(attributeRejected(name, b(`data:${type};base64,x`), element, true), false, `${element} ${name} ${type}`);
      assert.equal(attributeRejected(name, b(`data:${type},x`), element, true), false, `${element} ${name} ${type}`);
    }
  }
});

test('item 4.2 (M1): the type part is compared after ASCII lower case, and parameters after ";" do not count', () => {
  const img = (v) => attributeRejected('src', b(v), 'img', true);
  for (const v of ['DATA:IMAGE/PNG,x', 'data:image/jpeg;charset=x;base64,x', 'data:image/gif,x', 'data:Image/WebP;base64,x', 'data:image/x-icon']) {
    assert.equal(img(v), false, JSON.stringify(v));
  }
});

test('item 4.2 (M1): a type part that is not exactly a raster type rejects (other SVG spellings, stray bytes, other images)', () => {
  const img = (v) => attributeRejected('src', b(v), 'img', true);
  const rejected = [
    // The probes of the security review.
    'data:image/ svg+xml,x', 'data:image/\x0bsvg+xml,x', 'data:image/\x0csvg+xml,x', 'data:image/svg\x01+xml,x', 'data:image/svg,x',
    'data:image/svg-xml,x', 'data:image/xml,x', 'data:image/png ,x', 'data:image/png\x0c;base64,x',
    // White space or a control byte inside or around a raster type; other image types; a type without a subtype.
    'data: image/png,x', 'data:image/png\x00,x', 'data:image/png\x7f,x', 'data:image/ png,x', 'data:image/pngx,x', 'data:image/png+xml,x',
    'data:image/tiff,x', 'data:image/heic,x', 'data:image/svg+xml,x', 'data:image/svg+xml;base64,x', 'data:image/,x', 'data:image,x',
    'data:,x', 'data:;base64,x', 'data:text/html,x', 'data:image/png\xa0,x', 'data:image/p\xe9g,x',
    // "%" (DL-048 b) and "&" (DL-051) in the type part never match.
    'data:image/p%6eg,x', 'data:image/png&#59;x,y', 'data:image/png&amp;,x',
  ];
  for (const v of rejected) assert.equal(img(v), true, JSON.stringify(v));
  // A tab, LF or CR is removed by item 3 (and by the URL parser), so the type is a raster type.
  assert.equal(img('data:image/p\tn\ng\r,x'), false);
});

test('item 4.2 (M1): a raster type in another attribute or element, or in a foreign region, still rejects', () => {
  assert.equal(attributeRejected('href', b('data:image/png,x'), 'a', true), true);
  assert.equal(attributeRejected('src', b('data:image/png,x'), 'video', true), true);
  assert.equal(attributeRejected('href', b('data:image/png,x'), 'image', false), true);
  assert.equal(attributeRejected('src', b('data:image/png,x'), 'img', false), true);
});

test('item 4.3 (H-83, DL-048 c): srcset splits on every ASCII white space and on commas', () => {
  for (const sep of ['\t', '\n', '\x0c', '\r', ' ']) {
    // The white space ends the URL "/a.png"; the comma after it starts the next candidate.
    assert.equal(attributeRejected('srcset', b(`/a.png${sep},javascript:x`), 'img', true), true, JSON.stringify(sep));
    // Without a comma the second word is a descriptor of the first candidate, not a URL.
    assert.equal(attributeRejected('srcset', b(`/a.png${sep}javascript:x`), 'img', true), false, JSON.stringify(sep));
  }
  assert.equal(attributeRejected('srcset', b('/a.png\x0b,javascript:x'), 'img', true), false); // vertical tab is not white space: one URL
});

test('item 4.3 (DL-048 c): each named reference that decodes to white space, ",", "(" or ")" rejects srcset', () => {
  for (const name of ['&Tab;', '&NewLine;', '&comma;', '&lpar;', '&rpar;']) {
    assert.equal(attributeRejected('srcset', b(`/a.png 1x${name}/b.png 2x`), 'img', true), true, name);
    assert.equal(attributeRejected('href', b(`/a.png${name}`), 'a', true), false, name); // only srcset is split
  }
  for (const ref of ['&#9;', '&#x0A;', '&#12;', '&#x0d', '&#32;', '&#x2C;', '&#40;', '&#x29;']) {
    assert.equal(attributeRejected('srcset', b(`/a.png${ref}`), 'img', true), true, ref);
  }
  // Other references stay: a query string with "&amp;" and a "&" that starts no reference.
  assert.equal(attributeRejected('srcset', b('/a.png?a=1&amp;b=2 1x, /b.png?c&d 2x, /c.png&#x2C9 3x'), 'img', true), false);
});

test('srcsetUrls: the candidate URLs of "parse a srcset attribute"', () => {
  const urls = (s) => srcsetUrls(b(s)).map((u) => u.toString('latin1'));
  assert.deepEqual(urls(' /a.png 1x,, /b.png (1, 2), /c.png,,/d.png'), ['/a.png', '/b.png', '/c.png,,/d.png']);
  assert.deepEqual(urls('/a.png,, /b.png 2x'), ['/a.png', '/b.png']);
  assert.deepEqual(urls('/a.png (x, javascript:y) 2x , /b.png'), ['/a.png', '/b.png']);
  assert.deepEqual(urls(', ,'), []);
  assert.deepEqual(urls(''), []);
});

test('H-96: noscript is rejected in a patch, in upper case and self-closing too', () => {
  for (const tag of ['<noscript>', '<NOSCRIPT/>', '<b>x</b><noscript a="1">']) assert.equal(why(tag), 'element', tag);
});

test('H-94: a template is allowed in a patch and the walk reads the start tags of its contents', () => {
  assert.equal(why('<template><span>x</span></template>'), null);
  assert.equal(why('<template><script>x</script></template>'), 'element');
  assert.equal(why('<template><noscript>x</noscript></template>'), 'element');
  assert.equal(why('<template><b onclick=x></template>'), 'attribute');
  assert.equal(why('<template><svg><a/></svg></template>'), 'foreign');
});

test('H-97: a template with shadowrootmode passes the patch content rule; the analysis of the patched document (step 8) rejects it', () => {
  const doc = (content) =>
    Buffer.from(`<!DOCTYPE html><html><head><title>T</title></head><body><p data-rq-block="price">${content}</p></body></html>`, 'latin1');
  assert.equal(analyze(doc('<template><span>x</span></template>')).valid, true);
  // DL-056: the old Chromium name `shadowroot` is rejected the same way.
  for (const attr of ['shadowrootmode="open"', 'SHADOWROOTMODE', 'shadowrootmode=open shadowrootmode=closed', 'shadowroot="open"', 'ShadowRoot', 'shadowroot=a shadowroot=b']) {
    const content = `<template ${attr}><span>x</span></template>`;
    assert.equal(why(content), null, attr);
    assert.equal(analyze(doc(content)).valid, false, attr);
  }
});
