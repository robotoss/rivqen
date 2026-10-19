// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// Rules of WP-17 S5 (docs/engineering/protocol/markup.md): H-94 (template and
// noscript in the content, M-22 item 6, M-24 item g), H-95 (M-11 item 10) and
// H-97 (every attribute named shadowrootmode counts). Each test checks the code.

import { describe, it } from 'node:test';
import assert from 'node:assert/strict';
import { ErrorCode } from '../src/markup.mjs';
import { doc, errorOf } from './helpers.mjs';

const { STRUCTURE, RESERVED } = ErrorCode;

const inBlock = (content, tag = 'div') => `<${tag} data-rq-block="a">${content}</${tag}>`;
const err = (content, tag) => errorOf(doc(inBlock(content, tag)));

describe('template in the content (H-94, M-22 items 2 and 6)', () => {
  it('accepts a template with its own end tag', () => {
    assert.equal(err('<template><b>x</b></template>'), null);
  });

  it('rejects a template without its end tag (M-23)', () => {
    assert.equal(err('<template>x'), STRUCTURE);
  });

  it('rejects an end tag of the block element inside a template (M-23)', () => {
    assert.equal(errorOf(doc('<div data-rq-block="a"><template></div></template></div>')), STRUCTURE);
  });

  it('rejects shadowrootmode with any value, case and position', () => {
    for (const attrs of [
      'shadowrootmode="open"', 'shadowrootmode', 'ShadowRootMode=closed', 'SHADOWROOTMODE=""',
      'id=x shadowrootmode=open', 'a=1 b=2 shadowrootmode',
    ]) {
      assert.equal(err(`<template ${attrs}></template>`), STRUCTURE, attrs);
    }
  });

  it('counts a duplicate shadowrootmode that the tokenizer drops (H-97)', () => {
    assert.equal(err('<template shadowrootmode=open shadowrootmode=closed></template>'), STRUCTURE);
    assert.equal(err('<template id=x id=y shadowrootmode></template>'), STRUCTURE);
  });

  it('rejects shadowrootmode on a nested template', () => {
    assert.equal(err('<template><template shadowrootmode=open></template></template>'), STRUCTURE);
  });

  it('allows the other shadowroot attributes and similar names', () => {
    for (const attrs of [
      'shadowrootclonable', 'shadowrootdelegatesfocus', 'shadowrootserializable',
      'data-shadowrootmode=open', 'shadowrootmodes', 'shadowroot',
    ]) {
      assert.equal(err(`<template ${attrs}></template>`), null, attrs);
    }
  });

  it('allows shadowrootmode outside block content and in a foreign region', () => {
    assert.equal(errorOf(doc('<template shadowrootmode=open></template>' + inBlock('x'))), null);
    assert.equal(err('<svg><template shadowrootmode=open></template></svg>'), null);
  });

  it('allows shadowrootmode on an element that is not a template', () => {
    assert.equal(err('<div shadowrootmode=open></div>'), null);
    assert.equal(err('<span shadowrootmode></span>'), null);
  });

  it('keeps M-10 item 3 (for) in a template of the content', () => {
    assert.equal(err('<template for=x></template>'), STRUCTURE);
  });

  it('keeps M-09 condition 1 for col in a template', () => {
    assert.equal(err('<template><col></template>'), STRUCTURE);
  });
});

describe('M-24 item g with F is none (H-94)', () => {
  it('accepts a table inside a template', () => {
    assert.equal(err('<template><table><tr><td>x</td></tr></table></template>'), null);
  });

  it('rejects a table part that is a child of the template', () => {
    assert.equal(err('<template><tr><td>x</td></tr></template>'), STRUCTURE);
    assert.equal(err('<template><td>x</td></template>'), STRUCTURE);
  });

  it('rejects a table part in a template inside a cell of a table of the content', () => {
    assert.equal(err('<table><tr><td><template><tr><td>x</td></tr></template></td></tr></table>'), STRUCTURE);
  });

  it('accepts a table part after the template is closed', () => {
    assert.equal(err('<table><tr><td><template></template></td><td>y</td></tr></table>'), null);
  });
});

describe('noscript in the content (H-94, H-95, M-11 item 10)', () => {
  it('accepts a noscript with phrasing content', () => {
    assert.equal(err('x<noscript><img src="/p.gif"></noscript>y'), null);
    assert.equal(err('a<noscript>Enable <b>JS</b></noscript>b', 'span'), null);
  });

  it('accepts a list and a table in C', () => {
    assert.equal(err('<noscript><ul><li>x</li></ul></noscript>'), null);
    assert.equal(err('<noscript><table><tr><td>x</td></tr></table></noscript>'), null);
  });

  it('accepts a noscript inside a template of the content', () => {
    assert.equal(err('<template><noscript><img src="/p.gif"></noscript></template>'), null);
  });

  it('rejects M-24 item a in C', () => {
    assert.equal(err('<noscript><div>x</div></noscript>', 'p'), STRUCTURE);
  });

  it('rejects M-24 item b in C', () => {
    assert.equal(err('<noscript><h3>x</h3></noscript>', 'h2'), STRUCTURE);
  });

  it('rejects M-24 item c in C', () => {
    assert.equal(err('<noscript><li>x</li></noscript>'), STRUCTURE);
  });

  it('rejects M-24 item e in C', () => {
    assert.equal(err('<a href="#"><noscript><a href="#">x</a></noscript></a>'), STRUCTURE);
  });

  it('rejects M-24 item k in C', () => {
    assert.equal(err('<noscript><table><b>x</b></table></noscript>'), STRUCTURE);
  });

  it('rejects M-22 item 2 in C', () => {
    assert.equal(err('<noscript><textarea>t</textarea></noscript>'), STRUCTURE);
  });

  it('rejects a template with shadowrootmode in C (M-11 item 3)', () => {
    assert.equal(err('<noscript><template shadowrootmode=open></template></noscript>'), STRUCTURE);
  });

  it('rejects a DOCTYPE token in C (DL-055)', () => {
    assert.equal(err('<noscript><!DOCTYPE html></noscript>'), STRUCTURE);
  });

  it('rejects the end tag of the block element in C (M-23)', () => {
    assert.equal(err('<noscript></div></noscript>'), STRUCTURE);
  });

  it('rejects the end tag of a void element in C (M-22 item 4)', () => {
    assert.equal(err('<noscript><img></img></noscript>'), STRUCTURE);
  });

  it('rejects a manifest script in C with RESERVED', () => {
    assert.equal(err('<noscript><script type="application/rivqen-manifest+json"></script></noscript>'), RESERVED);
  });

  it('does not apply the content rules to a noscript outside a block', () => {
    assert.equal(errorOf(doc('<p><noscript><div>x</div></noscript></p>')), null);
  });
});
