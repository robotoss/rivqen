// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// One group per rule of docs/engineering/protocol/markup.md section 2 (rqp/1).

import { describe, it } from 'node:test';
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { ErrorCode, LIMITS, analyze, checkNoscript } from '../src/markup.mjs';
import { FORBIDDEN_CONTENT } from '../src/names.mjs';
import { blocksOf, doc, errorOf, run } from './helpers.mjs';

const {
  ENCODING, LIMIT, INVALID_ID, FORBIDDEN_ELEMENT, DUPLICATE, NESTED, STRUCTURE, BAD_JSON, RESERVED,
} = ErrorCode;

const P = (id, content = 'x') => `<p data-rq-block="${id}">${content}</p>`;
const DIV = (id, content = 'x') => `<div data-rq-block="${id}">${content}</div>`;

describe('M-01 input size', () => {
  it('accepts an input of exactly 5 MiB', () => {
    const input = Buffer.alloc(LIMITS.maxDocumentBytes, 0x61);
    assert.equal(run(input).valid, true);
  });

  it('rejects an input of 5 MiB + 1 byte with RQP_MARKUP_LIMIT', () => {
    assert.equal(errorOf(Buffer.alloc(LIMITS.maxDocumentBytes + 1, 0x61)), LIMIT);
  });

  it('checks the size before the encoding', () => {
    assert.equal(errorOf(Buffer.alloc(LIMITS.maxDocumentBytes + 1, 0xff)), LIMIT);
  });
});

describe('M-02 encoding', () => {
  const bad = {
    'invalid continuation': [0xc3, 0x28],
    'overlong 2-byte form': [0xc0, 0xaf],
    'overlong 3-byte form': [0xe0, 0x80, 0xaf],
    'overlong 4-byte form': [0xf0, 0x80, 0x80, 0xaf],
    'surrogate code point': [0xed, 0xa0, 0x80],
    'code point above U+10FFFF': [0xf4, 0x90, 0x80, 0x80],
    'byte F5': [0xf5, 0x80, 0x80, 0x80],
    'lone continuation byte': [0x80],
    'truncated sequence at the end': [0xe2, 0x82],
  };
  for (const [name, seq] of Object.entries(bad)) {
    it(`rejects ${name} with RQP_MARKUP_ENCODING`, () => {
      const input = Buffer.concat([Buffer.from(doc(P('a'))), Buffer.from(seq)]);
      assert.equal(errorOf(input), ENCODING);
    });
  }

  it('accepts the largest code point U+10FFFF', () => {
    assert.deepEqual(blocksOf(doc(P('a', '\u{10FFFF}'))), [['a', 'html', '\u{10FFFF}']]);
  });

  it('counts a BOM at offset 0 in the offsets and keeps it in the template', () => {
    const body = P('a', 'x');
    const plain = run(body);
    const withBom = run(Buffer.concat([Buffer.from([0xef, 0xbb, 0xbf]), Buffer.from(body)]));
    assert.equal(withBom.blocks[0].start, plain.blocks[0].start + 3);
    assert.notEqual(withBom.template_revision, plain.template_revision);
  });

  it('treats U+FEFF after offset 0 as a normal character', () => {
    assert.deepEqual(blocksOf(`﻿﻿${P('a', '﻿')}`), [['a', 'html', '﻿']]);
  });

  it('accepts U+0000 and other control characters', () => {
    assert.deepEqual(blocksOf(doc(P('a', '\0\x01\x7f'))), [['a', 'html', '\0\x01\x7f']]);
  });
});

describe('M-03 tokenizer', () => {
  it('does not end a start tag at > inside a quoted attribute value', () => {
    assert.deepEqual(blocksOf(`<p title="a>b" data-rq-block='c'>x</p>`), [['c', 'html', 'x']]);
  });

  it('uses the first of two attributes with the same name', () => {
    assert.deepEqual(blocksOf(`<p data-rq-block="a" DATA-RQ-BLOCK="B!">x</p>`), [['a', 'html', 'x']]);
  });

  it('ignores markup in comments, also after <!--> and --!>', () => {
    const input = `<!-- ${P('f')} --><!-->${P('a')}<!-- x --!>${P('b')}<!--->${P('c')}`;
    assert.deepEqual(blocksOf(input).map((b) => b[0]), ['a', 'b', 'c']);
  });

  it('ends a bogus comment and a processing instruction at the first >', () => {
    const input = `<!x ${P('f1')}</ ${P('f2')}<?pi ${P('f3')}${P('real')}`;
    // Each comment ends at the '>' of the fake start tag, so only "real" is a block.
    assert.deepEqual(blocksOf(input).map((b) => b[0]), ['real']);
  });

  it('ends a DOCTYPE at the first >, also inside quotes', () => {
    const input = `<!DOCTYPE html PUBLIC "a>${P('real')}`;
    assert.deepEqual(blocksOf(input).map((b) => b[0]), ['real']);
  });

  it('counts CR LF as two bytes', () => {
    const input = `<div\r\ndata-rq-block="a"\r\n>\r\nx\r\n</div\r\n>`;
    assert.deepEqual(blocksOf(input), [['a', 'html', '\r\nx\r\n']]);
  });

  it('compares tag and attribute names in lower case', () => {
    assert.deepEqual(blocksOf(`<DIV Data-Rq-Block="a">x</DiV>`), [['a', 'html', 'x']]);
  });

  it('gives end tag attributes and the self-closing flag on an end tag no effect', () => {
    assert.deepEqual(blocksOf(`<div data-rq-block="a">x</div foo=">" />`), [['a', 'html', 'x']]);
  });
});

describe('M-04 tokenizer state changes', () => {
  for (const name of ['title', 'textarea', 'style', 'iframe', 'noembed', 'noframes', 'xmp']) {
    it(`treats markup in ${name} as text`, () => {
      assert.deepEqual(blocksOf(`<${name}>${P('fake')}</${name}>${P('real')}`).map((b) => b[0]), ['real']);
    });
  }

  it('treats markup in script data as text, also in the double-escaped state', () => {
    const input = `<script><!--<script>a</script>${P('fake')}--></script>${P('real')}`;
    assert.deepEqual(blocksOf(input).map((b) => b[0]), ['real']);
  });

  it('treats the rest of the input after plaintext as text', () => {
    assert.equal(run(`<plaintext>${P('fake')}`).blocks.length, 0);
  });

  it('changes the state also with the self-closing flag', () => {
    assert.deepEqual(blocksOf(`<title/>${P('fake')}</title>${P('real')}`).map((b) => b[0]), ['real']);
  });

  it('ends raw text only at the appropriate end tag', () => {
    const input = `<style></styles></STYLE ${P('fake')}`;
    assert.equal(run(input).blocks.length, 0);
    assert.deepEqual(blocksOf(`<style></styles></STYLE>${P('real')}`).map((b) => b[0]), ['real']);
  });

  it('does not change the state in a foreign region', () => {
    // In svg, <style> content is markup: the <b> start tag is seen and breaks M-07.
    assert.equal(errorOf(doc('<svg><style><b>x</b></style></svg>')), STRUCTURE);
  });
});

describe('M-05 token stack', () => {
  it('does not push void elements', () => {
    assert.deepEqual(blocksOf(`<div data-rq-block="a"><br><img src="x"><hr></div>`), [
      ['a', 'html', '<br><img src="x"><hr>'],
    ]);
  });

  it('does not open a foreign region for a self-closing svg or math', () => {
    assert.deepEqual(blocksOf(`<svg/><math/>${P('a')}`).map((b) => b[0]), ['a']);
  });

  it('ignores an end tag without a matching element', () => {
    assert.deepEqual(blocksOf(`</span></div>${P('a')}`).map((b) => b[0]), ['a']);
  });

  it('ignores an unmatched end tag also on a deep stack', () => {
    // If the end tag popped anything, it would cross the guarded button (M-06).
    assert.equal(errorOf(doc('<div><span><button></x></button></span></div>')), null);
  });

  it('pops crossed elements', () => {
    // After </div> the span is closed, so the block is not inside an open span.
    assert.deepEqual(blocksOf(`<div><span>a</div>${P('b')}`).map((b) => b[0]), ['b']);
  });

  it('pushes a self-closing non-void element', () => {
    // The span is open, so </span> crosses the guarded a (M-06).
    assert.equal(errorOf(doc('<span/><a>x</span></a>')), STRUCTURE);
  });

  it('pops the block element at the block end tag', () => {
    // If the span stayed open, </span> would cross the guarded a (M-06).
    assert.equal(errorOf(doc('<span data-rq-block="a">x</span><a></span></a>')), null);
  });

  it('ignores a self-closing flag on a non-void name outside blocks', () => {
    // <div/> pushes a div; the following </div> pops it and nothing else.
    assert.equal(errorOf(`<button><div/></div>${P('a')}</button>`), null);
  });
});

describe('M-06 crossing', () => {
  it('rejects an end tag that crosses a button', () => {
    assert.equal(errorOf(doc('<div><button>a</div>')), STRUCTURE);
  });

  it('rejects an end tag that crosses an a', () => {
    assert.equal(errorOf(doc('<table><tr><td><a href="#">a</td></tr></table>')), STRUCTURE);
  });

  it('allows crossing elements that are not guarded', () => {
    assert.equal(errorOf(doc('<div><span>a</div><ul><li>a</ul>')), null);
  });

  it('allows a table-family end tag to cross table parts', () => {
    assert.equal(errorOf(doc('<table><tr><td>a</tr></table><table><tr><td>b</table>')), null);
  });

  it('does not allow a non-table end tag to cross table parts', () => {
    assert.equal(errorOf(doc('<div><table><tr><td>a</div>')), STRUCTURE);
  });

  it('does not let a table-family end tag cross other guarded elements', () => {
    assert.equal(errorOf(doc('<table><tr><td><button>a</td></tr></table>')), STRUCTURE);
  });

  it('does not guard elements pushed in a foreign region', () => {
    assert.equal(errorOf(doc('<svg><a><text>t</svg>')), null);
  });
});

describe('M-07 foreign regions', () => {
  it('rejects a BREAKOUT start tag in svg and in math', () => {
    assert.equal(errorOf(doc('<svg><p>x</p></svg>')), STRUCTURE);
    assert.equal(errorOf(doc('<math><font>x</font></math>')), STRUCTURE);
  });

  it('accepts text in MathML integration points', () => {
    assert.equal(errorOf(doc('<math><mi>x</mi><mo>=</mo><mn>2</mn></math>')), null);
  });

  it('rejects a stray end tag in a foreign region', () => {
    assert.equal(errorOf(doc('<svg><g></x></g></svg>')), STRUCTURE);
  });

  it('rejects an end tag in a foreign region after its element was closed', () => {
    assert.equal(errorOf(doc('<svg><g></g></g></svg>')), STRUCTURE);
  });

  it('rejects an end tag in a foreign region that matches an element outside it', () => {
    assert.equal(errorOf(doc('<div><svg></div></svg>')), STRUCTURE);
  });

  it('ends a region only with the end tag of its own svg', () => {
    // The inner svg is an element of the region; the first </svg> pops only it.
    assert.equal(errorOf(doc('<svg><svg></svg><p>x</p></svg>')), STRUCTURE);
    assert.equal(errorOf(doc(`<svg><svg></svg></svg>${P('a')}`)), null);
  });

  it('rejects tags and comments in an integration point', () => {
    assert.equal(errorOf(doc('<svg><foreignObject><div>a</div></foreignObject></svg>')), STRUCTURE);
    assert.equal(errorOf(doc('<svg><desc><!-- c --></desc></svg>')), STRUCTURE);
    assert.equal(errorOf(doc('<math><mi><![CDATA[x]]></mi></math>')), STRUCTURE);
    assert.equal(errorOf(doc('<svg><title><!DOCTYPE x></title></svg>')), STRUCTURE);
  });

  it('accepts a self-closing integration point', () => {
    assert.equal(errorOf(doc('<svg><desc/><g></g></svg>')), null);
  });

  it('requires text-only content and no self-closing flag for TEXT names', () => {
    assert.equal(errorOf(doc('<svg><style>.a{}</style></svg>')), null);
    assert.equal(errorOf(doc('<svg><script/></svg>')), STRUCTURE);
    assert.equal(errorOf(doc('<svg><script><g/></script></svg>')), STRUCTURE);
    assert.equal(errorOf(doc('<svg><plaintext></plaintext></svg>')), STRUCTURE);
  });

  it('allows a non-matching end tag only after text-only content ends', () => {
    assert.equal(errorOf(doc('<svg><title>a</g></title></svg>')), STRUCTURE);
  });
});

describe('M-08 select', () => {
  it('accepts option, optgroup, hr, comments and text', () => {
    const select = '<select><optgroup label="g"><option>a</option><hr><!--c--></optgroup></select>';
    assert.equal(errorOf(doc(select)), null);
  });

  it('rejects other start tags inside select', () => {
    assert.equal(errorOf(doc('<select><b>a</b></select>')), STRUCTURE);
  });

  it('rejects other end tags inside select', () => {
    assert.equal(errorOf(doc('<div><select></div></select>')), STRUCTURE);
  });

  it('rejects a DOCTYPE inside select', () => {
    assert.equal(errorOf(doc('<select><!DOCTYPE html></select>')), STRUCTURE);
  });

  it('requires an explicit </select>', () => {
    assert.equal(errorOf('<select><option>a'), STRUCTURE);
  });
});

describe('M-09 table parts', () => {
  it('rejects a cell while a cell is open', () => {
    assert.equal(errorOf(doc('<table><tr><td>a<td>b</td></tr></table>')), STRUCTURE);
  });

  it('accepts explicitly closed cells and rows', () => {
    assert.equal(errorOf(doc('<table><tbody><tr><td>a</td><th>b</th></tr></tbody></table>')), null);
  });

  it('rejects a col inside a cell and accepts it in a colgroup', () => {
    assert.equal(errorOf(doc('<table><tr><td><col></td></tr></table>')), STRUCTURE);
    assert.equal(errorOf(doc('<table><colgroup><col></colgroup><col></table>')), null);
  });

  it('rejects a row group or caption inside a row group', () => {
    assert.equal(errorOf(doc('<table><tbody><caption>c</caption></tbody></table>')), STRUCTURE);
    assert.equal(errorOf(doc('<table><thead><tbody></tbody></thead></table>')), STRUCTURE);
  });

  it('rejects a row inside a row', () => {
    assert.equal(errorOf(doc('<table><tr><tr></tr></tr></table>')), STRUCTURE);
  });

  it('allows table parts without a table', () => {
    assert.equal(errorOf(doc('<td>a</td><tr></tr>')), null);
  });

  it('stops the search for F at a template', () => {
    assert.equal(errorOf(doc('<table><tr><td><template><td>a</td></template></td></tr></table>')), null);
  });
});

describe('M-10 frameset', () => {
  it('rejects a frameset start tag', () => {
    assert.equal(errorOf('<html><frameset></frameset></html>'), STRUCTURE);
  });

  it('accepts frameset text in raw text', () => {
    assert.equal(errorOf(doc('<script>"<frameset>"</script>')), null);
  });
});

describe('M-11 noscript', () => {
  it('accepts typical tracking fallbacks', () => {
    const body = '<noscript><iframe src="https://www.example.com/"></iframe></noscript>'
      + '<noscript><img src="https://www.example.com/p.gif" alt=""></noscript>';
    assert.equal(errorOf(doc(body)), null);
  });

  it('rejects block markup in noscript content', () => {
    assert.equal(errorOf(doc(`<noscript>${P('ns')}</noscript>`)), STRUCTURE);
  });

  it('rejects content that ends inside a comment in the markup view', () => {
    assert.equal(errorOf(doc(`<noscript><!-- </noscript>${P('a')}<!-- --></noscript>`)), STRUCTURE);
  });

  it('rejects content that ends inside a tag or raw text in the markup view', () => {
    assert.equal(errorOf(doc('<noscript><img alt="</noscript>">')), STRUCTURE);
    assert.equal(errorOf(doc('<noscript><style></noscript>')), STRUCTURE);
  });

  for (const name of ['frameset', 'math', 'noscript', 'plaintext', 'select', 'svg', 'template']) {
    it(`rejects a ${name} start tag in noscript content`, () => {
      assert.equal(errorOf(doc(`<noscript><${name}></noscript>`)), STRUCTURE);
    });
  }

  it('checks noscript content that runs to the end of the input', () => {
    assert.equal(errorOf(`<noscript><img>`), null);
    assert.equal(errorOf(`<noscript>${P('a')}`), STRUCTURE);
  });

  it('requires the first noscript end tag exactly at the end of the content', () => {
    assert.equal(checkNoscript(Buffer.from('<p>a</p>')), null);
    assert.equal(checkNoscript(Buffer.from('<title></noscript>')), STRUCTURE);
  });
});

describe('M-12 CDATA', () => {
  it('rejects a CDATA section with > before ]]>', () => {
    assert.equal(errorOf(doc('<p><![CDATA[ a > b ]]></p>')), STRUCTURE);
  });

  it('accepts a CDATA section without > inside', () => {
    assert.equal(errorOf(doc('<svg><![CDATA[ a < b ]]></svg><![CDATA[]]>')), null);
  });

  it('rejects ]]> that overlaps the opening <![CDATA[', () => {
    assert.equal(errorOf(doc('<![CDATA[]>')), STRUCTURE);
  });

  it('rejects a CDATA section whose first > follows only one ]', () => {
    assert.equal(errorOf(doc('<![CDATA[x x]>')), STRUCTURE);
    assert.equal(errorOf(doc('<![CDATA[x]x>')), STRUCTURE);
  });

  it('accepts a CDATA section that runs to the end of the input', () => {
    assert.equal(errorOf('<![CDATA[ a < b'), null);
  });
});

describe('M-13 reserved manifest', () => {
  it('rejects a manifest script', () => {
    assert.equal(errorOf(doc('', '<script type="application/rivqen-manifest+json">{}</script>')), RESERVED);
  });

  it('matches the type ASCII case-insensitively inside the raw value', () => {
    assert.equal(errorOf(doc('', '<script TYPE=" Application/Rivqen-Manifest+JSON ">{}</script>')), RESERVED);
  });

  it('rejects a type with a character reference', () => {
    assert.equal(errorOf(doc('<script type="text/javascript&#x20;">1</script>')), RESERVED);
  });

  it('applies also in a foreign region', () => {
    assert.equal(errorOf(doc('<svg><script type="rivqen-manifest">x</script></svg>')), RESERVED);
  });

  it('folds only ASCII letters when it matches the type', () => {
    // CR (0x0D) | 0x20 is '-': a wrong case fold would see "rivqen-manifest".
    assert.equal(errorOf(doc('<script type="rivqen\rmanifest">1</script>')), null);
    assert.equal(errorOf(doc('<script type="x RIVQEN-MANIFEST">1</script>')), RESERVED);
  });

  it('uses only the first type attribute', () => {
    const tag = '<script type="text/javascript" type="application/rivqen-manifest+json">1</script>';
    assert.equal(errorOf(doc(tag)), null);
  });

  it('does not check other elements or raw text', () => {
    assert.equal(errorOf(doc('<p type="rivqen-manifest">x</p><style><script type="&"></style>')), null);
  });
});

describe('M-14 block start tag', () => {
  it('does not treat an end tag with the attribute as a block', () => {
    assert.equal(run(doc('<p>a</p data-rq-block="x">')).blocks.length, 0);
  });

  it('does not treat similar attribute names as markers', () => {
    assert.equal(run(doc('<p data-rq-block-x="y">b</p><p data-rq-blocks="z">c</p>')).blocks.length, 0);
  });

  it('does not treat attribute values or character references as markup', () => {
    const body = `<p title='<p data-rq-block="v">'>&lt;p data-rq-block="w"&gt;</p>`;
    assert.equal(run(doc(body)).blocks.length, 0);
  });
});

describe('M-15 block id', () => {
  const valid = ['a', '0', 'a_b-c', 'x'.repeat(64)];
  for (const id of valid) {
    it(`accepts the id ${id.length > 8 ? `of ${id.length} characters` : id}`, () => {
      assert.deepEqual(blocksOf(P(id)).map((b) => b[0]), [id]);
    });
  }

  const invalid = {
    empty: '<p data-rq-block="">x</p>',
    'without a value': '<p data-rq-block>x</p>',
    'with 65 characters': P('x'.repeat(65)),
    'with a leading hyphen': P('-a'),
    'with a leading underscore': P('_a'),
    'with an upper-case letter': P('aB'),
    'with a dot': P('a.b'),
    'with white space': P('a b'),
    'with a non-ASCII letter': P('café'),
    'with a character reference': P('&#112;rice'),
  };
  for (const [name, input] of Object.entries(invalid)) {
    it(`rejects an id ${name} with RQP_MARKUP_INVALID_ID`, () => {
      assert.equal(errorOf(doc(input)), INVALID_ID);
    });
  }
});

describe('M-16 block element', () => {
  for (const name of ['article', 'aside', 'div', 'footer', 'h1', 'h6', 'header', 'main', 'nav', 'p', 'section', 'span']) {
    it(`accepts ${name} as an html block`, () => {
      assert.deepEqual(blocksOf(`<${name} data-rq-block="a">x</${name}>`), [['a', 'html', 'x']]);
    });
  }

  it('accepts title as an html block with raw RCDATA content', () => {
    const input = `<head><title data-rq-block="t">a &amp; <b></title></head>`;
    assert.deepEqual(blocksOf(input), [['t', 'html', 'a &amp; <b>']]);
  });

  it('accepts a script with type application/json as a json block', () => {
    const input = `<script type="application/json" data-rq-block="d">{"a":"</p>"}</script>`;
    assert.deepEqual(blocksOf(input), [['d', 'json', '{"a":"</p>"}']]);
  });

  const forbidden = {
    'a void element': '<img data-rq-block="a">',
    'li': '<ul><li data-rq-block="a">x</li></ul>',
    'a script without type': '<script data-rq-block="a">1</script>',
    'a script with another JSON type': '<script type="application/ld+json" data-rq-block="a">{}</script>',
    'a script with a type in other case': '<script type="Application/JSON" data-rq-block="a">{}</script>',
    'a script with a type with white space': '<script type="application/json " data-rq-block="a">{}</script>',
    'a style element': '<style data-rq-block="a">p{}</style>',
    'a textarea': '<textarea data-rq-block="a">x</textarea>',
    'a template': '<template data-rq-block="a">x</template>',
    'an svg title': '<svg><title data-rq-block="a">x</title></svg>',
  };
  for (const [name, input] of Object.entries(forbidden)) {
    it(`rejects ${name} with RQP_MARKUP_FORBIDDEN_ELEMENT`, () => {
      assert.equal(errorOf(doc(input)), FORBIDDEN_ELEMENT);
    });
  }
});

describe('M-17 nesting', () => {
  it('rejects a block start tag in the content of a block', () => {
    assert.equal(errorOf(doc(DIV('a', P('b')))), NESTED);
  });

  it('checks nesting before the id and the element', () => {
    assert.equal(errorOf(doc(DIV('a', '<img data-rq-block="B!">'))), NESTED);
  });
});

describe('M-18 start tag form', () => {
  it('rejects a self-closing block start tag', () => {
    assert.equal(errorOf(doc('<div data-rq-block="a"/>x</div>')), STRUCTURE);
  });
});

describe('M-19 block context', () => {
  const invalid = {
    'inside a template': `<template>${DIV('a')}</template>`,
    'directly in a table': `<table>${DIV('a')}</table>`,
    'directly in a row group': `<table><tbody>${DIV('a')}</tbody></table>`,
    'directly in a row': `<table><tr>${DIV('a')}</tr></table>`,
    'directly in a column group': `<table><colgroup>${DIV('a')}</colgroup></table>`,
    'inside a template in a cell': `<table><tr><td><template>${DIV('a')}</template></td></tr></table>`,
  };
  for (const [name, input] of Object.entries(invalid)) {
    it(`rejects a block ${name}`, () => {
      assert.equal(errorOf(doc(input)), STRUCTURE);
    });
  }

  it('rejects a block inside select', () => {
    assert.equal(errorOf(doc(`<select>${P('a')}</select>`)), STRUCTURE);
  });

  it('accepts blocks in a caption, a td and a th, and outside tables', () => {
    const table = `<table><caption>${P('c')}</caption><tr><td>${DIV('d')}</td><th>${P('h')}</th></tr></table>`;
    assert.deepEqual(blocksOf(doc(`${table}${P('o')}`)).map((b) => b[0]), ['c', 'd', 'h', 'o']);
  });

  it('accepts a block after the template is closed', () => {
    assert.deepEqual(blocksOf(doc(`<template><p>x</p></template>${P('a')}`)).map((b) => b[0]), ['a']);
  });
});

describe('M-20 explicit end tag', () => {
  it('rejects an html block that runs to the end of the input', () => {
    assert.equal(errorOf('<div data-rq-block="a">x'), STRUCTURE);
  });

  it('rejects a title block and a json block that run to the end of the input', () => {
    assert.equal(errorOf('<title data-rq-block="a">x'), STRUCTURE);
    assert.equal(errorOf('<script type="application/json" data-rq-block="a">{}'), STRUCTURE);
  });

  it('rejects a p block closed by an implied end tag', () => {
    assert.equal(errorOf(doc('<p data-rq-block="a">one<p>two</p>')), STRUCTURE);
  });

  it('accepts an empty block', () => {
    assert.deepEqual(blocksOf(doc('<p data-rq-block="a"></p>')), [['a', 'html', '']]);
  });
});

describe('M-21 unique id', () => {
  it('rejects two blocks with the same id', () => {
    assert.equal(errorOf(doc(P('a') + DIV('a'))), DUPLICATE);
  });
});

describe('M-22 allowed tokens in html content', () => {
  it('rejects a DOCTYPE token in the content', () => {
    assert.equal(errorOf(doc(DIV('a', '<!DOCTYPE html>'))), STRUCTURE);
  });

  for (const name of FORBIDDEN_CONTENT) {
    it(`rejects a ${name} start tag in the content`, () => {
      assert.equal(errorOf(doc(DIV('a', `<${name}>`))), STRUCTURE);
    });
  }

  it('rejects a self-closing non-void start tag and accepts a self-closing void one', () => {
    assert.equal(errorOf(doc(DIV('a', '<span/>'))), STRUCTURE);
    assert.equal(errorOf(doc(DIV('a', '<br/><img src="a"/>'))), null);
  });

  it('rejects an end tag of a void element', () => {
    assert.equal(errorOf(doc(DIV('a', '</br>'))), STRUCTURE);
  });

  it('accepts comments, character references and event handler attributes', () => {
    assert.equal(errorOf(doc(DIV('a', '<!-- c --><b onclick="x()">&amp;</b>'))), null);
  });
});

describe('M-23 balance', () => {
  it('rejects a block end tag while an element of the content is open', () => {
    assert.equal(errorOf(doc(DIV('a', '<b>x'))), STRUCTURE);
  });

  it('rejects a stray end tag in the content', () => {
    assert.equal(errorOf(doc(DIV('a', 'x</b>'))), STRUCTURE);
  });

  it('rejects an end tag that closes an element opened before the block', () => {
    assert.equal(errorOf(doc(`<section>${DIV('a', 'x</section>')}`)), STRUCTURE);
  });

  it('requires explicit end tags for optional-end elements in the content', () => {
    assert.equal(errorOf(doc(DIV('a', '<ul><li>a<li>b</ul>'))), STRUCTURE);
    assert.equal(errorOf(doc(DIV('a', '<ul><li>a</li><li>b</li></ul>'))), null);
  });

  it('ends the block at the end tag that pops the block element', () => {
    assert.deepEqual(blocksOf(doc(DIV('a', '<div>in</div>'))), [['a', 'html', '<div>in</div>']]);
  });
});

describe('M-24 no implied end tags', () => {
  const cases = [
    ['a: p block with a PCLOSE start tag', P('a', '<ul></ul>'), STRUCTURE],
    ['a: span block with a PCLOSE start tag', '<span data-rq-block="a"><div></div></span>', STRUCTURE],
    ['a: div block with a PCLOSE start tag', DIV('a', '<p>x</p><ul></ul>'), null],
    ['b: heading block with a heading', '<h1 data-rq-block="a"><h2>x</h2></h1>', STRUCTURE],
    ['b: heading opened in the content', DIV('a', '<h1><h2>x</h2></h1>'), STRUCTURE],
    ['b: headings one after another', DIV('a', '<h1>x</h1><h2>y</h2>'), null],
    ['c: li without a list in the content', DIV('a', '<li>x</li>'), STRUCTURE],
    ['c: dt without dl in the content', DIV('a', '<dt>x</dt>'), STRUCTURE],
    ['c: dd in a dl in the content', DIV('a', '<dl><dt>x</dt><dd>y</dd></dl>'), null],
    ['c: li in a menu in the content', DIV('a', '<menu><li>x</li></menu>'), null],
    ['d: button while a button is open', DIV('a', '<button><button>x</button></button>'), STRUCTURE],
    ['d: button inside a button outside the block', `<button>${DIV('a', '<button>x</button>')}</button>`, STRUCTURE],
    ['e: a while an a is open', DIV('a', '<a><a>x</a></a>'), STRUCTURE],
    ['e: a after an a is closed', DIV('a', '<a>x</a><a>y</a>'), null],
    ['f: nobr while a nobr is open', DIV('a', '<nobr><nobr>x</nobr></nobr>'), STRUCTURE],
    ['g: td without a table in the content', DIV('a', '<td>x</td>'), STRUCTURE],
    ['g: col without a table in the content', DIV('a', '<col>'), STRUCTURE],
    ['g: table parts in a table in the content', DIV('a', '<table><caption>c</caption><tr><td>x</td></tr></table>'), null],
    ['h: rb without ruby in the content', DIV('a', '<rb>x</rb>'), STRUCTURE],
    ['h: rtc while an rtc is open', DIV('a', '<ruby><rtc><rtc></rtc></rtc></ruby>'), STRUCTURE],
    ['h: rb while an rt is open', DIV('a', '<ruby>a<rt>b<rb>c</rb></rt></ruby>'), STRUCTURE],
    ['i: rt without ruby in the content', `<ruby>a${P('a', '<rt>b</rt>')}</ruby>`, STRUCTURE],
    ['i: rp while an rb is open', DIV('a', '<ruby><rb>a<rp>(</rp></rb></ruby>'), STRUCTURE],
    ['i: rt inside an rtc', DIV('a', '<ruby>a<rtc><rt>b</rt></rtc></ruby>'), null],
    ['h, i: ruby parts after each other', DIV('a', '<ruby><rb>a</rb><rt>b</rt><rp>(</rp><rtc>c</rtc></ruby>'), null],
  ];
  for (const [name, body, code] of cases) {
    it(`${code === null ? 'accepts' : 'rejects'} ${name}`, () => {
      assert.equal(errorOf(doc(body)), code);
    });
  }

  it('accepts a div block after an open p (the p closes before the div)', () => {
    assert.deepEqual(blocksOf(doc(`<p>text${DIV('a')}`)).map((b) => b[0]), ['a']);
  });

  it('accepts a span block inside a paragraph', () => {
    assert.deepEqual(blocksOf(doc(`<p>a <span data-rq-block="s">b</span> c</p>`)).map((b) => b[0]), ['s']);
  });
});

describe('M-25 JSON', () => {
  const json = (content) => doc(`<script type="application/json" data-rq-block="d">${content}</script>`);

  it('accepts a JSON text', () => {
    assert.equal(errorOf(json('{"a":[1,2.5e3,true,null,"\\u00e9"]}')), null);
  });

  it('rejects content that is not a JSON text with RQP_MARKUP_BAD_JSON', () => {
    for (const bad of ['', '{"a":1,}', ' {}', '{"a":"\\ud800"}', '[1] [2]']) {
      assert.equal(errorOf(json(bad)), BAD_JSON, JSON.stringify(bad));
    }
  });

  it('checks JSON after the block size', () => {
    const big = `"${'a'.repeat(LIMITS.maxBlockBytes)}`; // too large and not JSON
    assert.equal(errorOf(json(big)), LIMIT);
  });
});

describe('M-26 block limits', () => {
  const many = (n) => doc(Array.from({ length: n }, (_, i) => P(`b${i}`)).join(''));

  it('accepts 256 blocks', () => {
    assert.equal(run(many(256)).blocks.length, 256);
  });

  it('rejects 257 blocks with RQP_MARKUP_LIMIT', () => {
    assert.equal(errorOf(many(257)), LIMIT);
  });

  it('accepts a block of 1 MiB and rejects a block of 1 MiB + 1 byte', () => {
    assert.equal(errorOf(doc(P('a', 'x'.repeat(LIMITS.maxBlockBytes)))), null);
    assert.equal(errorOf(doc(P('a', 'x'.repeat(LIMITS.maxBlockBytes + 1)))), LIMIT);
  });

  it('counts the block size in bytes, not in characters', () => {
    const content = 'é'.repeat(LIMITS.maxBlockBytes / 2) + 'x';
    assert.equal(errorOf(doc(P('a', content))), LIMIT);
  });
});

describe('M-27 result', () => {
  const sha = (data) => createHash('sha256').update(data).digest('base64url');

  it('gives blocks in document order with byte offsets and hashes', () => {
    const input = Buffer.from(`<p>\u{1F600}</p>${P('b', 'é')}<span data-rq-block="a">x</span>`);
    const result = analyze(input);
    assert.deepEqual(result.blocks.map((b) => b.id), ['b', 'a']);
    const [b] = result.blocks;
    assert.equal(b.start, Buffer.byteLength(`<p>\u{1F600}</p><p data-rq-block="b">`));
    assert.equal(b.end, b.start + 2);
    assert.equal(b.sha256, sha(Buffer.from('é')));
    assert.equal(b.sha256.length, 43);
  });

  it('computes the template and page revisions of CONTRACT.md section 4', () => {
    const input = Buffer.from(`<p data-rq-block="a">x</p><script type="application/json" data-rq-block="j">[]</script>`);
    const result = analyze(input);
    const template = '<p data-rq-block="a"></p><script type="application/json" data-rq-block="j"></script>';
    const t = `t1.${sha(`rqp-t1\n${template}`)}`;
    const preimage = `rqp-r1\n${t}\na\thtml\t${sha('x')}\nj\tjson\t${sha('[]')}\n`;
    assert.deepEqual(result, {
      valid: true,
      error: null,
      blocks: [
        { id: 'a', format: 'html', start: 21, end: 22, sha256: sha('x') },
        { id: 'j', format: 'json', start: 76, end: 78, sha256: sha('[]') },
      ],
      template_revision: t,
      page_revision: `r1.${sha(preimage)}`,
    });
  });

  it('gives a document without blocks an empty list and revisions', () => {
    const result = run('<p>x</p>');
    assert.deepEqual(result.blocks, []);
    assert.equal(result.template_revision, `t1.${sha('rqp-t1\n<p>x</p>')}`);
    assert.equal(result.page_revision, `r1.${sha(`rqp-r1\n${result.template_revision}\n`)}`);
  });

  it('gives an invalid document no blocks and no revisions', () => {
    assert.deepEqual(run(doc(P('a') + P('a'))), {
      valid: false, error: DUPLICATE, blocks: [], template_revision: null, page_revision: null,
    });
  });

  it('accepts an empty input', () => {
    assert.equal(run('').valid, true);
  });

  it('accepts any Uint8Array view, also with an offset', () => {
    const backing = Buffer.from(`zz${P('a', 'y')}`);
    const view = new Uint8Array(backing.buffer, backing.byteOffset + 2, backing.length - 2);
    assert.deepEqual(analyze(view).blocks.map((b) => [b.start, b.end]), [[21, 22]]);
  });

  it('rejects an argument that is not a Uint8Array', () => {
    assert.throws(() => analyze('text'), TypeError);
  });
});
