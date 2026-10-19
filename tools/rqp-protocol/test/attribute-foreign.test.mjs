// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors
//
// Attribute rule of manifest-patch section 4.2 in a foreign region (svg, math; H-73):
// the rule reads every start tag that the markup analysis reads, with the token stack
// of M-05 and the state changes of M-04 only in HTML context. Also the limits of
// the walk, the attribute tokenizer, the normalization of item 3 and the byte order
// mark of the head rule (H-72).

import assert from 'node:assert/strict';
import { test } from 'node:test';
import { LIMITS } from '../src/common.mjs';
import { analyze, attributeRejected, documentHeadStart, passesAttributeRule, startTagOffsets } from '../src/markup.mjs';

const b = (s) => Buffer.from(s, 'latin1');
const rule = (s) => passesAttributeRule(b(s));

/** True when the markup analysis accepts the content as the content of an html block. */
const valid = (content) => analyze(b(`<p data-rq-block="x">${content}</p>`)).valid;

test('an attribute of an svg or math element that the rule denies rejects the content', () => {
  const rejected = [
    '<svg><title>x</title><path onload="x"/></svg>',
    '<svg onload=x></svg>',
    '<svg><use xlink:href="javascript:x"/></svg>',
    '<svg><use href=" \tJava\nScript:x"/></svg>',
    '<svg><desc>d</desc><use href="data:image/svg+xml,x"/></svg>',
    '<svg><text y="&#x6a;">t</text></svg>',
    '<svg><g><g><rect ONCLICK=x /></g></g></svg>',
    '<math><mi href="vbscript:x">x</mi></math>',
    '<math><mrow><mo onclick=x>+</mo></mrow></math>',
    '<svg><g title="a" title="javascript:x"/></svg>',
  ];
  for (const c of rejected) {
    assert.equal(valid(c), true, `${c}: must be valid markup`);
    assert.equal(rule(c), false, c);
  }
});

test('an allowed svg icon and math formula pass', () => {
  const accepted = [
    '<svg viewBox="0 0 8 8"><title>Icon</title><path d="M0 0h8v8z" fill="#333"/><use href="#i"/></svg>',
    '<svg><use xlink:href="/sprite.svg?a=1&amp;b=2#i"/></svg>',
    '<math><mi>x</mi><mo>+</mo><mn>1</mn></math>',
    '<svg/><b>y</b>',
  ];
  for (const c of accepted) {
    assert.equal(valid(c), true, `${c}: must be valid markup`);
    assert.equal(rule(c), true, c);
  }
});

test('in a foreign region a title start tag opens no RCDATA state: later start tags are read', () => {
  // The content is not valid markup (M-18); the walk must still not hide the path tag.
  assert.equal(rule('<svg><title/><path onload="x"/></svg>'), false);
  assert.deepEqual(startTagOffsets(b('<svg><title/><g/></svg>')), [0, 5, 13]);
});

test('in HTML context a title start tag is RCDATA and a style start tag is RAWTEXT', () => {
  assert.equal(rule('<title><a onclick=x></title><b>y</b>'), true);
  assert.equal(rule('<style><a onclick=x></style><b>y</b>'), true);
  assert.equal(rule('<textarea><a onclick=x></textarea>'), true);
});

test('the state changes apply again after the region is closed', () => {
  assert.equal(rule('<svg><g></g></svg><style><a onclick=x></style>'), true);
  assert.equal(rule('<svg></svg><style><a onclick=x></style><i onclick=x></i>'), false);
});

test('an end tag has no attributes for the rule', () => {
  assert.equal(rule('<b></b onclick=x>'), true);
});

test('a self-closing svg or math opens no region', () => {
  assert.equal(rule('<svg/><style><a onclick=x></style>'), true);
  assert.equal(rule('<math/><title><a onclick=x></title>'), true);
});

test('an unclosed region reads start tags to the end of the content', () => {
  assert.equal(rule('<svg><g>'), true);
  assert.equal(rule('<svg><g><title/><g onload=x>'), false);
});

test('a stack deeper than the limit fails closed, nothing throws', () => {
  assert.equal(rule('<svg>' + '<g>'.repeat(300)), false);
  assert.equal(startTagOffsets(b('<svg>' + '<g>'.repeat(300))), null);
  assert.equal(rule('<svg>' + '<g>'.repeat(200)), true);
  assert.equal(rule('<svg>' + '<g/>'.repeat(300) + '</svg>'), true); // self-closing elements are not open
});

test('the token stack holds exactly 256 entries', () => {
  assert.equal(rule('<svg>' + '<g>'.repeat(255)), true);
  assert.equal(rule('<svg>' + '<g>'.repeat(256)), false);
});

test('only svg and math open a foreign region; an svg region ends at the end tag of its root', () => {
  assert.equal(rule('<div><title><a onclick=x></title></div>'), true);
  assert.equal(rule('<math><mi><title><a onclick=x></title></mi></math>'), false);
  // An end tag of an element inside the region does not end the region.
  assert.equal(rule('<svg><g></g><title><a onclick=x></a></title></svg>'), false);
});

test('attribute tokenizer: "/" ends a tag name and an attribute name; "=" with no value is an attribute', () => {
  for (const c of ['<a/onclick=x>y</a>', '<a x/onclick=y>z</a>', '<a onclick=>y</a>']) assert.equal(rule(c), false, c);
});

test('attribute rule item 3: "&amp;" becomes "&", and only leading bytes 00 to 20 go', () => {
  assert.equal(attributeRejected('href', b('java&amp;script:x')), false);
  assert.equal(attributeRejected('href', b('java script:x')), false);
  assert.equal(attributeRejected('href', b(' \x00javascript:x')), true);
});

test('content over maxBlockBytes fails the rule (fail closed); content of exactly maxBlockBytes is read', () => {
  assert.equal(passesAttributeRule(Buffer.alloc(LIMITS.maxBlockBytes, 0x61)), true);
  assert.equal(passesAttributeRule(Buffer.alloc(LIMITS.maxBlockBytes + 1, 0x61)), false);
});

test('head rule: only EF BB BF at offset 0 counts as white space before the head start tag', () => {
  const head = (prefix) => documentHeadStart(Buffer.concat([Buffer.from(prefix), Buffer.from('<head>')]));
  assert.equal(head([0xef, 0xbb, 0xbf]), 9);
  for (const prefix of [[0xe0, 0xbb, 0xbf], [0xef, 0x80, 0xbf], [0xef, 0xbb, 0x80]]) assert.equal(head(prefix), -1, JSON.stringify(prefix));
});

// Property: for generated block content that the analysis accepts, the walk reads exactly
// the start tags that the tokenizer produces under M-04, M-05 and M-07.
function prng(seed) {
  let s = seed >>> 0;
  return (n) => {
    s = (Math.imul(s, 1664525) + 1013904223) >>> 0;
    return Math.floor((s / 2 ** 32) * n);
  };
}

/** Build content from a random tree. `visible` collects the offsets of the start tags a tokenizer reads. */
function generate(rnd) {
  let out = '';
  const visible = [];
  const tag = (text) => {
    visible.push(out.length);
    out += text;
  };
  const foreign = (depth) => {
    const n = rnd(4);
    for (let i = 0; i < n; i++) {
      switch (rnd(7)) {
        case 0:
          tag('<g>');
          if (depth < 4) foreign(depth + 1);
          out += '</g>';
          break;
        case 1:
          tag('<path d="M0 0"/>');
          break;
        case 2:
          tag('<title>');
          out += 'a text</title>';
          break;
        case 3:
          tag('<desc>');
          out += 'd</desc>';
          break;
        case 4:
          tag('<text>');
          out += 't';
          tag('<tspan>');
          out += 'u</tspan></text>';
          break;
        case 5:
          tag('<use href="#i"/>');
          break;
        default:
          tag('<rect x=1 />');
          break;
      }
    }
  };
  const html = (depth) => {
    const n = rnd(5);
    for (let i = 0; i < n; i++) {
      switch (rnd(8)) {
        case 0:
          tag('<svg>');
          foreign(0);
          out += '</svg>';
          break;
        case 1:
          tag('<svg/>');
          break;
        case 2:
          tag('<math>');
          tag('<mi>');
          out += 'x</mi></math>';
          break;
        case 3:
          tag('<title>');
          out += '<g> hidden </title>';
          break;
        case 4:
          tag('<style>');
          out += '<g> hidden </style>';
          break;
        case 5:
          out += '<!-- <g> -->';
          break;
        case 6:
          tag('<b>');
          if (depth < 3) html(depth + 1);
          out += '</b>';
          break;
        default:
          out += 'text &amp; more';
          break;
      }
    }
  };
  html(0);
  return { content: out, visible };
}

test('property: the walk reads the start tags of the tokenizer for random svg block content', () => {
  const rnd = prng(0x5eed17);
  let checked = 0;
  for (let i = 0; i < 400; i++) {
    const { content, visible } = generate(rnd);
    if (!valid(content)) continue;
    checked++;
    assert.deepEqual(startTagOffsets(b(content)), visible, content);
    assert.equal(rule(content), true, content);
    // Every visible start tag is checked: an event handler on any of them is found.
    for (const at of visible) {
      const end = content.indexOf('>', at);
      const cut = content[end - 1] === '/' ? end - 1 : end;
      const bad = `${content.slice(0, cut)} onload=x${content.slice(cut)}`;
      if (!valid(bad)) continue;
      assert.equal(rule(bad), false, bad);
    }
  }
  assert.ok(checked > 100, `only ${checked} valid samples`);
});
