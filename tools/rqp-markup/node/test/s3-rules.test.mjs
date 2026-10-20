// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// Rules of WP-17 S3 (docs/engineering/protocol/markup.md section 2.13):
// H-71 (M-11 items 7 and 8, M-26 item 7), H-73 (the region of the content; item 5 left markup in H-78),
// H-74 (M-11 item 9) and H-75 (M-06 item 4, M-11 item 5, M-23). Each test names
// an input that only the named rule rejects, so the reason is the rule.

import { describe, it } from 'node:test';
import assert from 'node:assert/strict';
import { ErrorCode } from '../src/markup.mjs';
import {
  FORBIDDEN_CONTENT, NOSCRIPT_FORBIDDEN,
} from '../src/names.mjs';
import { doc, errorOf } from './helpers.mjs';

const { LIMIT, STRUCTURE, RESERVED, FORBIDDEN_ELEMENT, NESTED } = ErrorCode;

const inBlock = (content, tag = 'div') => `<${tag} data-rq-block="a">${content}</${tag}>`;
const ns = (content) => errorOf(doc(`<noscript>${content}</noscript>`));

describe('name sets (H-73, H-74)', () => {
  it('does not forbid svg and math as HTML names of block content (they open a region)', () => {
    assert.equal(FORBIDDEN_CONTENT.has('svg'), false);
    assert.equal(FORBIDDEN_CONTENT.has('math'), false);
  });

  it('keeps math, and not svg, in the forbidden start tags of the noscript content', () => {
    assert.equal(NOSCRIPT_FORBIDDEN.has('math'), true);
    assert.equal(NOSCRIPT_FORBIDDEN.has('svg'), false);
  });
});

describe('the region of the content (H-73, H-78)', () => {
  for (const name of ['a', 'animate', 'set', 'foreignobject', 'image', 'script', 'style', 'maction', 'foo']) {
    it(`accepts <${name}> in a foreign region of the content (H-78)`, () => {
      assert.equal(errorOf(inBlock(`<svg><${name}></${name}></svg>`)), null);
    });
  }

  it('allows the same names as HTML elements outside a region', () => {
    assert.equal(errorOf(inBlock('<a href="/x">t</a><image>')), null);
  });

  it('opens a region for svg and math without the self-closing flag', () => {
    assert.equal(errorOf(inBlock('<svg><path d="M0"/></svg>x')), null);
    assert.equal(errorOf(inBlock('<math><mi>x</mi></math>')), null);
    // In the region an HTML element is a breakout (M-07 item 1).
    assert.equal(errorOf(inBlock('<svg><div></div></svg>')), STRUCTURE);
  });

  it('opens no region for a self-closing svg or math (M-22 item 3, M-05 item 1)', () => {
    // Without a region, <a> is an HTML element and is allowed.
    assert.equal(errorOf(inBlock('<svg/><a href="/x">t</a><math/>')), null);
    assert.equal(errorOf(inBlock('<span/>')), STRUCTURE);
  });

  it('applies M-22 rule 2 again after the region ends', () => {
    assert.equal(errorOf(inBlock('<svg></svg><textarea>1</textarea>')), STRUCTURE);
    assert.equal(errorOf(inBlock('<svg></svg><b>x</b>')), null);
  });

  it('applies M-07 in the region of the content: integration points are text only', () => {
    assert.equal(errorOf(inBlock('<svg><desc>a &lt; b</desc></svg>')), null);
    assert.equal(errorOf(inBlock('<svg><desc><g></g></desc></svg>')), STRUCTURE); // M-07 item 3
    assert.equal(errorOf(inBlock('<svg><title><!--x--></title></svg>')), STRUCTURE);
  });

  it('puts M-17 before the region rules for a block start tag', () => {
    assert.equal(errorOf(inBlock('<svg><g data-rq-block="b"></g></svg>')), NESTED);
    assert.equal(errorOf('<svg><g data-rq-block="b"></g></svg>'), FORBIDDEN_ELEMENT); // outside a block: M-16
  });

  it('applies item k of M-24 to the svg start tag in table context', () => {
    assert.equal(errorOf(inBlock('<table><svg></svg></table>')), STRUCTURE);
    assert.equal(errorOf(inBlock('<table><tr><td>x<svg><g></g></svg></td></tr></table>')), null);
  });

  it('counts the pushes in a region of the content in D (M-26)', () => {
    const icons = (n) => inBlock('<svg><path d="M0"></svg>'.repeat(n), 'div');
    // Each icon keeps 2 units (svg and path: the crossing gives nothing back).
    assert.equal(errorOf(icons(127)), null); // 1 + 127 * 2 = 255
    assert.equal(errorOf(icons(128)), LIMIT); // 257
  });
});

describe('M-23 balance in a region of the content (H-73, H-75)', () => {
  it('lets the end tag of the region root close the elements above it', () => {
    assert.equal(errorOf(inBlock('<svg><path d="M0"></svg>')), null);
    assert.equal(errorOf(inBlock('<svg><g><path d="M0"></svg>')), null);
    assert.equal(errorOf(inBlock('<math><mrow><mi>x</mi></math>')), null);
  });

  it('requires every other end tag to match the current node', () => {
    assert.equal(errorOf(inBlock('<svg><g><path d="M0"></g></svg>')), STRUCTURE);
    assert.equal(errorOf(inBlock('<svg><g></x></g></svg>')), STRUCTURE);
  });

  it('does not let the end tag of an inner svg cross', () => {
    assert.equal(errorOf(inBlock('<svg><svg></svg></svg>')), null);
    assert.equal(errorOf(inBlock('<svg><svg><path d="M0"></svg></svg>')), STRUCTURE);
  });

  it('does not close the block while the region is open', () => {
    assert.equal(errorOf('<div data-rq-block="a"><svg><path></div>'), STRUCTURE);
  });

  it('rejects the end of the input inside a region of the content (M-07 item 6, M-20)', () => {
    assert.equal(errorOf('<div data-rq-block="a"><svg><path>'), STRUCTURE);
    assert.equal(errorOf('<p data-rq-block="a">x</p><svg><path>'), null); // E2 outside a block
  });

  it('keeps M-22 item 4 in HTML context', () => {
    assert.equal(errorOf(inBlock('<br></br>')), STRUCTURE);
  });
});

describe('M-06 item 4: the end tag of the region root sets no mark (H-75)', () => {
  it('accepts the icon in a link', () => {
    assert.equal(errorOf(doc('<a href="#"><svg><path d="M0"></svg></a>')), null);
    assert.equal(errorOf(doc('<a href="#"><math><mi>x</mi></math></a>')), null);
  });

  it('marks for a crossing by an end tag that does not match the region root', () => {
    assert.equal(errorOf(doc('<a href="#"><svg><g><path d="M0"></g></svg></a>')), STRUCTURE);
    assert.equal(errorOf(doc('<a href="#"><svg><svg><path d="M0"></svg></svg></a>')), STRUCTURE);
  });

  it('passes a popped mark on by item (b) when the end tag matches the root', () => {
    // </g> marks the svg; </svg> pops it and passes the mark to the b.
    assert.equal(errorOf(doc('<b><svg><g><path></g></svg>x</b>')), STRUCTURE);
    assert.equal(errorOf(doc('<b><svg><g><path/></g></svg>x</b>')), null);
  });

  it('keeps the crossing in D: the root end tag makes the new current node dirty', () => {
    const icon = '<span><svg><path d="M0"></svg></span>';
    assert.equal(errorOf(icon.repeat(85)), null); // 255
    assert.equal(errorOf(icon.repeat(86)), LIMIT); // 258
  });
});

describe('M-11 items 5 and 9: the foreign region of the noscript content (H-74, H-75)', () => {
  it('accepts an svg element and an svg in a link', () => {
    assert.equal(ns('<img src="p.gif"><svg width="1"><path d="M0"></path></svg>'), null);
    assert.equal(ns('<a href="/"><svg><path d="M0"></svg>Home</a>'), null);
  });

  it('opens no region for a self-closing svg', () => {
    // After <svg/> the tokenizer is in HTML context: a style is raw text.
    assert.equal(ns('<svg/><style><g></g></style>'), null);
  });

  it('does not switch the tokenizer in the region (M-04)', () => {
    assert.equal(ns('<style><g></g></style>'), null);
    // In the region <style> is a foreign text element: <g> is a tag, M-07 item 4.
    assert.equal(ns('<svg><style><g></g></style></svg>'), STRUCTURE);
  });

  it('lets only the end tag of the region root cross', () => {
    assert.equal(ns('<svg><g><path d="M0"></svg>'), null);
    assert.equal(ns('<svg><g><path d="M0"></g></svg>'), STRUCTURE);
    assert.equal(ns('<svg><svg><path d="M0"></svg></svg>'), STRUCTURE);
  });

  it('rejects an svg end tag without an open svg', () => {
    assert.equal(ns('</svg>'), STRUCTURE);
    assert.equal(ns('<b></svg></b>'), STRUCTURE);
  });

  it('rejects a region that is open at the end of the content (item 5)', () => {
    assert.equal(ns('<svg><path d="M0">'), STRUCTURE);
    assert.equal(ns('<svg><title>'), STRUCTURE);
  });

  it('applies item 3 in the region: math, noscript, template, plaintext and select', () => {
    for (const name of ['math', 'noscript', 'template', 'plaintext', 'select']) {
      assert.equal(ns(`<svg><${name}/></svg>`), STRUCTURE, name);
    }
  });

  it('applies item 2 in the region', () => {
    assert.equal(ns('<svg><g data-rq-block="b"></g></svg>'), STRUCTURE);
  });

  it('applies item 4 (M-13) in the region', () => {
    assert.equal(ns('<svg><script type="application/rivqen-manifest+json"></script></svg>'), RESERVED);
  });

  it('applies item 6 in the region', () => {
    assert.equal(ns('<svg><tr></tr></svg>'), STRUCTURE);
    assert.equal(ns('<table><tr><td>x</td></tr></table>'), null);
  });

  it('applies M-07 items 1 and 3 through item 8', () => {
    assert.equal(ns('<svg><p>x</p></svg>'), STRUCTURE);
    assert.equal(ns('<svg><title><g></g></title></svg>'), STRUCTURE);
  });

  it('applies M-12 through item 8', () => {
    assert.equal(ns('<![CDATA[a>b]]>'), STRUCTURE);
    assert.equal(ns('<svg><style><![CDATA[a]]></style></svg>'), null);
  });
});

describe('M-11 item 7: html, body and head in the noscript content (H-71)', () => {
  for (const name of ['html', 'body', 'head']) {
    it(`rejects a ${name} start tag, also in a region`, () => {
      assert.equal(ns(`<${name}></${name}>`), STRUCTURE);
      assert.equal(ns(`<svg><${name}></${name}></svg>`), STRUCTURE);
    });
    it(`rejects a ${name} end tag`, () => {
      assert.equal(ns(`</${name}>`), STRUCTURE);
    });
  }
});

describe('M-11 item 8: the main pass on the noscript content (H-71)', () => {
  it('rejects a second form while a form is open around the noscript (M-10 item 2)', () => {
    assert.equal(errorOf(doc('<form><noscript><form>x</form></noscript></form>')), STRUCTURE);
    assert.equal(errorOf(doc('<form><noscript><p>x</p></noscript></form>')), null);
    assert.equal(errorOf(doc('<form></form><noscript><form>x</form></noscript>')), null);
  });

  it('rejects M-09 condition 2 with the noscript element as the current node', () => {
    assert.equal(errorOf(doc('<table><noscript><table></table></noscript></table>')), STRUCTURE);
  });

  it('rejects a table part with a table of the outer document (item 6)', () => {
    assert.equal(errorOf(doc('<table><noscript><tr></tr></noscript></table>')), STRUCTURE);
  });

  it('applies M-13 and M-26 to the tokens of the content', () => {
    assert.equal(ns('<script type="Rivqen-Manifest"></script>'), RESERVED);
    const deep = (n) => `<noscript>${'<b>'.repeat(n)}${'</b>'.repeat(n)}</noscript>`;
    assert.equal(errorOf(deep(255)), null); // with the noscript element: 256
    assert.equal(errorOf(deep(256)), LIMIT);
  });

  it('starts from the D of the main pass and gives it back at the end tag', () => {
    const many = '<noscript><svg><path d="M0"></svg></noscript>'.repeat(300);
    assert.equal(errorOf(doc(many)), null);
    const outer = (n) => '<div>'.repeat(n) + '<noscript><i>x</i></noscript>' + '</div>'.repeat(n);
    assert.equal(errorOf(outer(254)), null);
    assert.equal(errorOf(outer(255)), LIMIT);
  });

  it('does not keep the dirty mark that the root end tag sets on the noscript element', () => {
    // The root end tag in C makes the noscript element dirty in the pass of
    // item 8. Without a restore, the end tag noscript would not give D back.
    const one = '<span><noscript><svg><path d="M0"></svg></noscript></span>';
    assert.equal(errorOf(doc(one.repeat(300))), null);
  });

  it('handles a noscript content that runs to the end of the input (E3)', () => {
    assert.equal(errorOf('<form><noscript><form>'), STRUCTURE);
    assert.equal(errorOf('<noscript><svg><path>'), STRUCTURE);
    assert.equal(errorOf('<noscript><svg><path></svg>'), null);
  });
});

describe('names of markup.md section 2.1 and the noscript scan at the root of the stack', () => {
  it('has exactly the forbidden start tags of the noscript content', () => {
    assert.deepEqual(
      [...NOSCRIPT_FORBIDDEN].sort(),
      ['frameset', 'math', 'noscript', 'plaintext', 'select', 'template'],
    );
  });

  it('accepts a doctype token in the content of a noscript element', () => {
    assert.equal(errorOf('<noscript><!DOCTYPE html></noscript>'), null);
  });

  it('requires the end tag of the current node in a noscript element at the root', () => {
    assert.equal(errorOf('<noscript><div><span></span></div></noscript>'), null);
    assert.equal(errorOf('<noscript><div><span></div></noscript>'), STRUCTURE);
  });
});
