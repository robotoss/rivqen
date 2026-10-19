// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// One group per rule of docs/engineering/protocol/markup.md section 2 (rqp/1).

import { describe, it } from 'node:test';
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { ErrorCode, LIMITS, analyze } from '../src/markup.mjs';
import { FORBIDDEN_CONTENT } from '../src/names.mjs';
import { blocksOf, doc, errorOf, run } from './helpers.mjs';

const {
  ENCODING, LIMIT, INVALID_ID, FORBIDDEN_ELEMENT, DUPLICATE, NESTED, STRUCTURE, BAD_JSON, RESERVED,
} = ErrorCode;

const P = (id, content = 'x') => `<p data-rq-block="${id}">${content}</p>`;
// The error of a document that is a noscript element with the given content C
// (the content is Buffer or string), else null.
const checkNoscript = (content) => errorOf(Buffer.concat([
  Buffer.from('<noscript>'), Buffer.from(content), Buffer.from('</noscript>'),
]));
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

describe('M-06 item 2 formatting end tags (H-26)', () => {
  const divs = (n) => '<div>'.repeat(n);

  it('rejects the review repro: </a> or </nobr> over 8 div elements, then a block with the same tag', () => {
    const a = `<a>${divs(8)}x</a><span data-rq-block="x">y<a>z</a>w</span>`;
    const nobr = `<nobr>${divs(8)}x</nobr><span data-rq-block="x">y<nobr>z</nobr>w</span>`;
    assert.equal(errorOf(doc(a)), STRUCTURE);
    assert.equal(errorOf(doc(nobr)), STRUCTURE);
  });

  it('rejects a formatting end tag over one special element', () => {
    assert.equal(errorOf(doc(`<a>${divs(1)}x</a>`)), STRUCTURE);
    assert.equal(errorOf(doc('<b><div>x</b>y</div><p data-rq-block="a">z</p>')), STRUCTURE);
    assert.equal(errorOf(doc('<em><p>x</em>')), STRUCTURE);
  });

  it('rejects a formatting end tag over an element pushed in a foreign region', () => {
    assert.equal(errorOf(doc('<svg><a href="#"><text>t</a></svg><p data-rq-block="a">z</p>')), STRUCTURE);
    assert.equal(errorOf(doc('<math><b><mrow></b></math>')), STRUCTURE);
  });

  it('accepts a formatting end tag over phrasing elements (the review control)', () => {
    const body = '<a href="#"><span>x</a><b><i>y</b><p><b>z</p>w'
      + '<span data-rq-block="x">y<a href="#">z</a>w</span>';
    assert.deepEqual(blocksOf(doc(body)).map((b) => b[0]), ['x']);
  });

  it('accepts a non-formatting end tag over special and foreign elements', () => {
    assert.equal(errorOf(doc('<span><div>x</span><svg><g><text>t</g></svg>')), null);
  });

  it('accepts formatting end tags that pop exactly, also in a foreign region', () => {
    assert.equal(errorOf(doc('<b><div>x</div></b><svg><a><text>t</text></a></svg>')), null);
  });

  it('accepts the formatting start tag shapes of the review', () => {
    const body = '<a href="#">x<div>y<a href="#">z</a></div></a>'
      + '<nobr>x<div>y<nobr>z</nobr></div></nobr><p data-rq-block="b">w</p>';
    assert.deepEqual(blocksOf(doc(body)).map((b) => b[0]), ['b']);
  });
});

describe('M-06 item 3 stray table end tags (H-30)', () => {
  it('rejects the review repros: </tbody> and </tr> in a cell without a match', () => {
    const tbody = '<table><td></tbody><div data-rq-block="x"><table></table>y</div></td></table>';
    const tr = '<table><td></tr><div data-rq-block="x">plain</div></td></table>';
    assert.equal(errorOf(doc(tbody)), STRUCTURE);
    assert.equal(errorOf(doc(tr)), STRUCTURE);
  });

  for (const name of ['tbody', 'thead', 'tfoot', 'tr', 'table']) {
    it(`rejects </${name}> that matches nothing in HTML context, also outside a table`, () => {
      assert.equal(errorOf(doc(`<table><td>x</${name}></td></table>`)), STRUCTURE);
      assert.equal(errorOf(`</${name.toUpperCase()}>`), STRUCTURE);
    });

    it(`accepts </${name}> that matches an element`, () => {
      const table = name === 'table' ? '<table></table>' : `<table><${name}></${name}></table>`;
      assert.equal(errorOf(doc(table)), null);
    });
  }

  it('accepts the other stray table end tags (the review control)', () => {
    const body = '<table><caption>c</caption></caption><col></colgroup><tr><td>a</td></td></th></col>'
      + '<td><span data-rq-block="c">x</span></td></tr></table>';
    assert.deepEqual(blocksOf(doc(body)).map((b) => b[0]), ['c']);
  });

  it('accepts implied row groups and rows', () => {
    assert.equal(errorOf(doc('<table><td>a</td></table><table><tr><td>b</td></tr></table>')), null);
  });

  it('rejects a stray table end tag also inside a template', () => {
    assert.equal(errorOf(doc('<template></tbody></template>')), STRUCTURE);
  });
});

describe('M-06 item 4 special-crossed mark (H-32)', () => {
  const divs = (n) => '<div>'.repeat(n);

  it('rejects the review repros: an earlier crossing over special elements, then the formatting end tag', () => {
    const span = `<a><span>${divs(8)}x</span></a><span data-rq-block="x">y<a>z</a>w</span>`;
    const nobr = `<nobr><span>${divs(8)}x</span></nobr><span data-rq-block="x">y<nobr>z</nobr>w</span>`;
    const list = '<a><li><ul><li><ul><li><ul><li><ul></li></li></li></li>x</a>'
      + '<span data-rq-block="x">y<a>z</a>w</span>';
    for (const body of [span, nobr, list]) assert.equal(errorOf(doc(body)), STRUCTURE);
  });

  it('rejects the examples of markup.md', () => {
    assert.equal(errorOf(doc('<a><span><div>a</span></a>')), STRUCTURE);
    assert.equal(errorOf(doc('<a><li><ul></li></a>')), STRUCTURE);
  });

  it('marks the new current node when a crossed element was pushed in a foreign region', () => {
    // </g> crosses the path: the svg gets the mark, </svg> passes it to the b.
    assert.equal(errorOf(doc('<b><svg><g><path></g></svg></b>')), STRUCTURE);
    assert.equal(errorOf(doc('<b><svg><g><path/></g></svg></b>')), null);
  });

  it('passes the mark down when an end tag pops exactly one marked element', () => {
    assert.equal(errorOf(doc('<b><q><span><div></span></q></b>')), STRUCTURE);
    assert.equal(errorOf(doc('<b><i><span><div></span></i></b>')), STRUCTURE);
  });

  it('rejects a formatting end tag that crosses a marked element', () => {
    // The q is marked; </b> crosses the q and the i (neither is special).
    assert.equal(errorOf(doc('<b><q><span><div></span><i>x</b>')), STRUCTURE);
  });

  it('does not mark for a crossing over elements that are not special (the review control)', () => {
    const body = '<a href="#"><b><span><i>x</span></b></a><span data-rq-block="x">y<a href="#">z</a>w</span>';
    assert.deepEqual(blocksOf(doc(body)).map((b) => b[0]), ['x']);
  });

  it('allows an end tag that is not in FORMATTING to pop a marked element', () => {
    assert.equal(errorOf(doc('<div><span><p></span>x</div>')), null);
  });

  it('pushes each element without the mark', () => {
    // The div is marked; the i above it is not.
    assert.equal(errorOf(doc('<div><span><p></span><i>x</i></div>')), null);
  });

  it('removes the mark with the popped element', () => {
    // The mark goes from the div to the section, never to the later b.
    assert.equal(errorOf(doc('<section><div><span><p></span></div><b>x</b></section>')), null);
  });

  it('does not change the mark for an end tag that matches nothing', () => {
    assert.equal(errorOf(doc('<b><span><i>x</span></x></b>')), null);
    assert.equal(errorOf(doc('<b><span><div>x</span></x></b>')), STRUCTURE);
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

  it('rejects a stray end tag in a region whose svg is the first element of the document', () => {
    assert.equal(errorOf('<svg></x></svg>'), STRUCTURE);
    assert.equal(errorOf('<math></x></math>'), STRUCTURE);
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
    assert.equal(errorOf(doc('<math><mi><![cdata[x]]></mi></math>')), STRUCTURE); // a bogus comment, not CDATA
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
  it('closes an open cell by a cell start tag (H-85a)', () => {
    assert.equal(errorOf(doc('<table><tr><td>a<td>b</td></tr></table>')), null);
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

describe('M-09 condition 1 col in a template (H-35)', () => {
  it('rejects the review repros: a raw text element after <template><col>', () => {
    const title = '<p data-rq-block="z">real</p><template><col><title></template>'
      + '<p data-rq-block="z">fake</p></title></template>';
    const textarea = '<template><col><textarea></template>'
      + '<script type="application/rivqen-manifest+json">{"evil":1}</script></textarea></template>'
      + '</head><body><p data-rq-block="z">q</p>';
    const style = '<template><col><style></template><p data-rq-block="f">fake</p></style></template>'
      + '<p data-rq-block="y">q</p>';
    assert.equal(errorOf(title), STRUCTURE);
    assert.equal(errorOf(textarea), STRUCTURE);
    assert.equal(errorOf(style), STRUCTURE);
  });

  it('rejects a col when a template is nearer than every table-family element', () => {
    assert.equal(errorOf(doc('<template><col></template>')), STRUCTURE);
    assert.equal(errorOf(doc('<template><div><col></div></template>')), STRUCTURE);
    assert.equal(errorOf(doc('<table><tr><td><template><col></template></td></tr></table>')), STRUCTURE);
    assert.equal(errorOf(doc('<template><table><template><col></template></table></template>')), STRUCTURE);
  });

  it('accepts a col in a table or a colgroup in a template', () => {
    assert.equal(errorOf(doc('<template><table><col></table></template>')), null);
    assert.equal(errorOf(doc('<template><colgroup><col></colgroup></template>')), null);
  });

  it('accepts other table parts in a template (the review controls)', () => {
    assert.equal(errorOf(doc('<template><tr><td>x</td></tr></template><p data-rq-block="a">x</p>')), null);
    const colgroup = '<template><colgroup><title></template><p data-rq-block="f">fake</p></title></colgroup>'
      + '</template><p data-rq-block="y">q</p>';
    assert.deepEqual(blocksOf(colgroup).map((b) => b[0]), ['y']);
  });

  it('accepts a col after the template is closed', () => {
    assert.equal(errorOf(doc('<template></template><col>')), null);
    assert.equal(errorOf(doc('<table><template></template><col></table>')), null);
  });
});

describe('M-09 condition 2 F is the current node (H-37)', () => {
  it('rejects the review repros: a foster-parented element above F', () => {
    const marquee = '<object><a href="1"><table><marquee><table></table></marquee></table>x</a></object>'
      + '<span data-rq-block="k">y<a href="2">z</a>w</span>';
    const unit = '<object><b><table><marquee><table></table></marquee></table></b></object>';
    assert.equal(errorOf(marquee), STRUCTURE);
    assert.equal(errorOf(`${unit.repeat(600)}<p data-rq-block="a">x</p>`), STRUCTURE);
    assert.equal(errorOf(doc('<table><tr><object><td>x</td></object></tr></table>')), STRUCTURE);
    assert.equal(errorOf(doc('<table><form><tr><td>x</td></tr></form></table>')), STRUCTURE);
  });

  // Each start tag that M-09 allows for F, and a table start tag.
  const allowed = {
    '<table>': ['caption', 'colgroup', 'tbody', 'thead', 'tfoot', 'col', 'tr', 'td', 'th', 'table'],
    '<table><tbody>': ['tr', 'td', 'th', 'table'],
    '<table><thead>': ['tr', 'td', 'th', 'table'],
    '<table><tfoot>': ['tr', 'td', 'th', 'table'],
    '<table><tr>': ['td', 'th', 'table'],
    '<table><colgroup>': ['col', 'table'],
  };
  for (const [prefix, names] of Object.entries(allowed)) {
    for (const name of names) {
      it(`rejects <${name}> with an element above F in ${prefix} and accepts it when F is the current node`, () => {
        assert.equal(errorOf(`${prefix}<div><${name}>`), STRUCTURE);
        assert.equal(errorOf(`${prefix}<div></div><${name}>`), null);
      });
    }
  }

  it('applies to every element above F, also a non-special one', () => {
    for (const above of ['b', 'span', 'form', 'object', 'marquee', 'applet', 'a']) {
      assert.equal(errorOf(`<table><tr><${above}><td>`), STRUCTURE, above);
    }
  });

  it('accepts a table in a cell or a caption, where F is not a table context', () => {
    assert.equal(errorOf(doc('<table><tr><td><div><table></table></div></td></tr></table>')), null);
    assert.equal(errorOf(doc('<table><caption><b><table></table></b></caption></table>')), null);
  });

  it('accepts the review control: an element closed before the table part', () => {
    const body = '<table><div></div><tr><td><span data-rq-block="a">x</span></td></tr></table>';
    assert.deepEqual(blocksOf(doc(body)), [['a', 'html', 'x']]);
  });

  it('does not apply when the search for F stops at a template', () => {
    assert.equal(errorOf(doc('<table><template><div><td>x</td></div></template></table>')), null);
  });

  it('does not apply to other start tags above F (M-24 k covers them in block content)', () => {
    assert.equal(errorOf(doc('<table><div><p>x</p></div></table>')), null);
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

describe('M-10 item 2 nested forms (H-36)', () => {
  const unit = '<form><object><form></form></object></form>';

  it('rejects the review repros: a form inside a form', () => {
    assert.equal(errorOf(doc(unit)), STRUCTURE);
    const anchor = `<a href="1">${unit.repeat(9)}<span data-rq-block="k">y<a href="2">z</a>w</span>`;
    assert.equal(errorOf(anchor), STRUCTURE);
    assert.equal(errorOf(`${unit.repeat(600)}<p data-rq-block="a">x</p>`), STRUCTURE);
  });

  it('rejects a form start tag at any depth under an open form', () => {
    assert.equal(errorOf(doc('<form><div><form>')), STRUCTURE);
    assert.equal(errorOf(doc('<form><table><tr><td><form>')), STRUCTURE);
    assert.equal(errorOf(doc('<form><template><form></form></template></form>')), STRUCTURE);
    assert.equal(errorOf(doc('<form><div data-rq-block="a"><form></form></div></form>')), STRUCTURE);
  });

  it('accepts forms one after another (the review control)', () => {
    const body = '<form><input></form><form><input></form><p data-rq-block="a">x</p>';
    assert.deepEqual(blocksOf(doc(body)).map((b) => b[0]), ['a']);
    assert.equal(errorOf(doc('<div data-rq-block="a"><form>x</form><form>y</form></div>')), null);
  });

  it('accepts a form after a crossing popped the earlier form (the stale pointer control)', () => {
    const body = '<div><form></div><form>x</form><p data-rq-block="a">x</p>';
    assert.deepEqual(blocksOf(doc(body)).map((b) => b[0]), ['a']);
  });

  it('does not count a form in a foreign region', () => {
    assert.equal(errorOf(doc('<form><svg><form></form></svg></form>')), null);
    assert.equal(errorOf(doc('<svg><form></form></svg><form></form>')), null);
  });
});

describe('M-10 item 3 template for (H-43)', () => {
  const block = '<div data-rq-block="a">x</div>';

  it('rejects a template start tag with a for attribute', () => {
    assert.equal(errorOf(`${block}<template for="p"><b>n</b></template>`), STRUCTURE);
  });

  it('matches the name ASCII case-insensitively, with any value or none', () => {
    for (const tag of ['<template FOR="">', '<template For=x>', '<template for>', '<template for=\'\'>']) {
      assert.equal(errorOf(`${block}${tag}</template>`), STRUCTURE, tag);
    }
  });

  it('rejects the attribute after other attributes and when it is duplicated', () => {
    assert.equal(errorOf(`${block}<template id="t" class=c for="p"></template>`), STRUCTURE);
    assert.equal(errorOf(`${block}<template for for="q"></template>`), STRUCTURE);
    assert.equal(errorOf(`${block}<template id="1" id="2" for></template>`), STRUCTURE);
  });

  it('rejects it in a document without a block and inside other elements', () => {
    assert.equal(errorOf('<template for="p"></template>'), STRUCTURE);
    assert.equal(errorOf(`${block}<table><template for="p"></template></table>`), STRUCTURE);
  });

  it('accepts a template without a for attribute (the control)', () => {
    assert.equal(errorOf(`${block}<template id="t"><b>n</b></template>`), null);
    assert.equal(errorOf(`${block}<template></template>`), null);
  });

  it('does not match other names or values', () => {
    for (const tag of ['<template data-for="p">', '<template id="for">', '<template fo="p">',
      '<template form="p">', '<template forx>', '<template xfor>']) {
      assert.equal(errorOf(`${block}${tag}</template>`), null, tag);
    }
  });

  it('does not apply to a template in a foreign region', () => {
    assert.equal(errorOf(`<svg><template for="p"></template></svg>${block}`), null);
    assert.equal(errorOf(`${block}<math><template for></template></math>`), null);
  });

  it('applies to other elements only as before: for on a label is valid', () => {
    assert.equal(errorOf(`${block}<label for="x">y</label>`), null);
  });

  it('keeps rejecting a template in block content and in noscript content', () => {
    assert.equal(errorOf('<div data-rq-block="a"><template for="p"></template></div>'), STRUCTURE);
    assert.equal(checkNoscript(Buffer.from('<template for="p"></template>')), STRUCTURE);
    assert.equal(checkNoscript(Buffer.from('<template></template>')), STRUCTURE);
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

  for (const name of ['frameset', 'math', 'noscript', 'plaintext', 'select', 'template']) {
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
    // The appended end tag is text in RCDATA: the end tag noscript is not reached.
    assert.equal(checkNoscript(Buffer.from('<title>')), STRUCTURE);
    assert.equal(errorOf(doc('<noscript><title></noscript>')), STRUCTURE);
  });
});

describe('M-11 item 5 balanced noscript content (H-27)', () => {
  it('rejects the review repros: a button or a table left open in the content', () => {
    const button = '<noscript><button></noscript><div data-rq-block="a">x<button>y</button>z</div>';
    const table = '<noscript><table></noscript><div data-rq-block="a">x</div>';
    assert.equal(errorOf(doc(button)), STRUCTURE);
    assert.equal(errorOf(doc(table)), STRUCTURE);
  });

  it('accepts balanced content (the review controls)', () => {
    const balanced = '<noscript><div><a href="#">x</a></div><table><tr><td>a</td></tr></table></noscript>'
      + '<p data-rq-block="a">x</p>';
    assert.deepEqual(blocksOf(doc(balanced)).map((b) => b[0]), ['a']);
    const head = '<head><noscript><div>q</div></noscript><title data-rq-block="t">T</title></head>'
      + '<body><div data-rq-block="a">x</div>';
    assert.deepEqual(blocksOf(head).map((b) => b[0]), ['t', 'a']);
  });

  it('rejects an element of the content that an end tag crosses', () => {
    assert.equal(errorOf(doc('<noscript><b><i>x</b></i></noscript>')), STRUCTURE);
    assert.equal(errorOf(doc('<noscript><div><span>x</div></noscript>')), STRUCTURE);
  });

  it('rejects a stray end tag in the content, also one that matches an element outside it', () => {
    assert.equal(errorOf(doc('<noscript></div><img src="p.gif"></noscript>')), STRUCTURE);
    assert.equal(errorOf(doc('<span><noscript></span></noscript></span>')), STRUCTURE);
    assert.equal(errorOf(doc('<noscript><b>x</b></b></noscript>')), STRUCTURE);
  });

  it('rejects an end tag of a void element in the content', () => {
    assert.equal(errorOf(doc('<noscript><br></br></noscript>')), STRUCTURE);
  });

  it('does not open an element for a void start tag, also with the self-closing flag', () => {
    assert.equal(errorOf(doc('<noscript><img src="p.gif"><br/><hr></noscript>')), null);
  });

  it('opens an element for a self-closing start tag that is not void', () => {
    assert.equal(errorOf(doc('<noscript><div/></noscript>')), STRUCTURE);
    assert.equal(errorOf(doc('<noscript><div/></div></noscript>')), null);
  });

  it('opens an element for raw text elements and closes it at their end tag', () => {
    const body = '<noscript><style>p{}</style><title>t</title><script>1</script><textarea>x</textarea></noscript>';
    assert.equal(errorOf(doc(body)), null);
  });

  it('applies to content that runs to the end of the input (E3)', () => {
    assert.equal(errorOf('<noscript><b>x</b>'), null);
    assert.equal(errorOf('<noscript><b>x'), STRUCTURE);
  });

  it('counts the elements of the content in the depth counter D (M-26 item 7, H-71)', () => {
    const deep = (n) => `${'<b>'.repeat(n)}x${'</b>'.repeat(n)}`;
    // The noscript element is element 255; its content adds up to 1 more.
    assert.equal(errorOf(`${'<div>'.repeat(254)}<noscript>${deep(1)}</noscript>${P('a')}`), null);
    assert.equal(errorOf(`${'<div>'.repeat(254)}<noscript>${deep(2)}</noscript>`), LIMIT);
    assert.equal(checkNoscript(Buffer.from(deep(255))), null);
    assert.equal(checkNoscript(Buffer.from(deep(256))), LIMIT);
    assert.equal(checkNoscript(Buffer.from(`${deep(255)}<b>`)), STRUCTURE);
  });

  it('gives D back after the end tag noscript', () => {
    const body = `${'<div>'.repeat(254)}<noscript>${'<b>x</b>'}</noscript>${'<i>x</i>'}`;
    assert.equal(errorOf(body), null);
  });
});

describe('M-11 item 6 table parts in noscript content (H-38)', () => {
  const block = '<div data-rq-block="x">a<table></table>b</div>';

  it('rejects the review repros: a table part in noscript content in a cell or a caption', () => {
    for (const [cell, part] of [['td', '<td></td>'], ['td', '<col>'], ['th', '<tr></tr>']]) {
      const body = `<table><tr><${cell}><noscript>${part}</noscript>${block}</${cell}></tr></table>`;
      assert.equal(errorOf(doc(body)), STRUCTURE, part);
    }
    const caption = `<table><caption><noscript><td></td></noscript>${block}</caption></table>`;
    assert.equal(errorOf(doc(caption)), STRUCTURE);
  });

  for (const name of ['caption', 'colgroup', 'tbody', 'td', 'tfoot', 'th', 'thead', 'tr', 'col']) {
    it(`rejects a balanced ${name} start tag without a table of the content`, () => {
      const element = name === 'col' ? '<col>' : `<${name}></${name}>`;
      assert.equal(checkNoscript(Buffer.from(element)), STRUCTURE);
      assert.equal(checkNoscript(Buffer.from(`<table>${element}</table>`)), null);
    });
  }

  it('accepts table parts in a table opened in the content (the review control)', () => {
    const body = `<table><tr><td><noscript><table><tr><td>x</td></tr></table></noscript>${block}</td></tr></table>`;
    assert.deepEqual(blocksOf(doc(body)), [['x', 'html', 'a<table></table>b']]);
  });

  it('needs the table of the content to be still open', () => {
    assert.equal(checkNoscript(Buffer.from('<table></table><tr></tr>')), STRUCTURE);
    assert.equal(checkNoscript(Buffer.from('<table><table></table></table><tr></tr>')), STRUCTURE);
    // Two tables: closing the inner one leaves the outer one open.
    assert.equal(checkNoscript(Buffer.from('<table><caption><table></table></caption><tbody></tbody></table>')), null);
  });

  it('accepts a table of the content under other elements', () => {
    assert.equal(checkNoscript(Buffer.from('<div><table><tr><td><b>x</b></td></tr></table></div>')), null);
  });

  it('does not count other elements of the content as a table', () => {
    const body = `<table><tr><td><noscript><div><td></td></div></noscript>${block}</td></tr></table>`;
    assert.equal(errorOf(doc(body)), STRUCTURE);
    assert.equal(checkNoscript(Buffer.from('<div><span><tr></tr></span></div>')), STRUCTURE);
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

describe('M-13 item 3 non-ASCII script types (H-28)', () => {
  it('rejects the review repros: dotless i and long s', () => {
    const dotless = '<script type="application/rıvqen-manifest+json">{}</script><p data-rq-block="a">x</p>';
    const longS = '<script type="application/rivqen-manifeſt+json">{}</script><p data-rq-block="a">x</p>';
    assert.equal(errorOf(doc(dotless)), RESERVED);
    assert.equal(errorOf(doc(longS)), RESERVED);
  });

  it('rejects any byte >= 0x80, also in a harmless type and in an unquoted value', () => {
    assert.equal(errorOf(doc('<script type="text/jävascript">1</script>')), RESERVED);
    assert.equal(errorOf(doc('<script type=\u{1F600}>1</script>')), RESERVED);
    assert.equal(errorOf(doc('<script type="text/javascript\u0080">1</script>')), RESERVED);
  });

  it('accepts an ASCII type and non-ASCII bytes outside the type value', () => {
    assert.equal(errorOf(doc('<script data-x="é" type="text/javascript">"é"</script>')), null);
    assert.equal(errorOf(doc('<p type="é">x</p><script type="\u007f">1</script>')), null);
  });

  it('applies before the json block check', () => {
    assert.equal(errorOf(doc('<script type="application/json " data-rq-block="j">[]</script>')), RESERVED);
  });

  it('applies in the noscript content', () => {
    const body = '<noscript><script type="application/rıvqen-manifest+json">{}</script></noscript>';
    assert.equal(errorOf(doc(body)), RESERVED);
    assert.equal(checkNoscript(Buffer.from('<script type="é">')), RESERVED);
  });

  it('uses only the first type attribute', () => {
    assert.equal(errorOf(doc('<script type="text/javascript" type="é">1</script>')), null);
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

  it('accepts a block after nested elements of the same name are closed', () => {
    const nested = `<template><template></template></template>${P('a')}`;
    assert.deepEqual(blocksOf(doc(nested)).map((b) => b[0]), ['a']);
  });

  it('finds F also when it is the first element of the document', () => {
    assert.equal(errorOf(`<table>${DIV('a')}</table>`), STRUCTURE);
    assert.equal(errorOf(`<table><td>${DIV('a')}</td></table>`), null);
  });

  it('finds F through elements that are not in the table family', () => {
    assert.equal(errorOf(doc(`<table><span><form>${DIV('a')}</form></span></table>`)), STRUCTURE);
    assert.equal(errorOf(doc(`<table><td><span>${DIV('a')}</span></td></table>`)), null);
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
    ['j: li in a div in an li', DIV('a', '<ul><li><div><li>a</li></div></li></ul>'), STRUCTURE],
    ['j: li directly in an li', DIV('a', '<ul><li><li>a</li></li></ul>'), STRUCTURE],
    ['j: dt in a dd', DIV('a', '<dl><dd><dt>a</dt></dd></dl>'), STRUCTURE],
    ['j: dd in a dt', DIV('a', '<dl><dt><span><dd>a</dd></span></dt></dl>'), STRUCTURE],
    ['j: li in an li after a closed inner list', DIV('a', '<ul><li><ol><li>x</li></ol><li>y</li></li></ul>'), STRUCTURE],
    ['j: dd in an li of a list outside the block', `<ul><li>${DIV('a', '<dl><dd><dt>a</dt></dd></dl>')}</li></ul>`, STRUCTURE],
    ['j: li in a list in an li', DIV('a', '<ul><li>a<ol><li>b</li></ol></li></ul>'), null],
    ['j: li in a list in a div in an li', DIV('a', '<ul><li><div><menu><li>b</li></menu></div></li></ul>'), null],
    ['j: items one after another', DIV('a', '<ul><li>a</li><li>b</li></ul><dl><dt>t</dt><dd>d</dd></dl>'), null],
    ['j: li in the content of an li outside the block', `<ul><li>${DIV('a', '<ul><li>x</li></ul>')}</li></ul>`, null],
    ['k: div in a table', DIV('a', '<table><div>x</div></table>'), STRUCTURE],
    ['k: table in a table', DIV('a', '<table><table></table></table>'), STRUCTURE],
    ['k: void element in a table', DIV('a', '<table><input type="hidden"></table>'), STRUCTURE],
    ['k: span in a row', DIV('a', '<table><tr><span>x</span></tr></table>'), STRUCTURE],
    ['k: b in a column group', DIV('a', '<table><colgroup><b>x</b></colgroup></table>'), STRUCTURE],
    ['k: table parts and col in a table', DIV('a', '<table><colgroup><col></colgroup><col><tbody><tr><td>x</td></tr></tbody></table>'), null],
    ['k: text and comments in a table', DIV('a', '<table>Total<!-- c --><tr>r<td>x</td></tr></table>'), null],
    ['k: elements in a cell and in a caption', DIV('a', '<table><caption><b>c</b></caption><tr><th><div>h</div></th><td><table><tr><td>x</td></tr></table></td></tr></table>'), null],
    ['k: a div after the table is closed', DIV('a', '<table><tr><td>x</td></tr></table><div>y</div>'), null],
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

describe('M-24 j and k: same block bytes as the WHATWG tree builder', () => {
  it('ends the block at the same end tag as a tree builder for nested lists and tables', () => {
    const content = '<ul><li>a<ul><li>b</li></ul></li><li>c</li></ul><table>t<tr><td><p>x</p></td></tr></table>';
    assert.deepEqual(blocksOf(doc(DIV('a', content))), [['a', 'html', content]]);
  });
});

describe('edge cases E1 to E8 (decisions H-21, H-22)', () => {
  it('E1: accepts < as a character in foreign text content', () => {
    const body = `<svg><desc>a < b</desc><style>a < b</style></svg><math><mi>1 <2</mi></math>${P('a')}`;
    assert.deepEqual(blocksOf(doc(body)).map((b) => b[0]), ['a']);
    assert.equal(errorOf(doc('<svg><desc>a <b>x</b></desc></svg>')), STRUCTURE);
  });

  it('E2: accepts the end of the input in a foreign region after all blocks', () => {
    assert.equal(errorOf(`${P('a')}<svg><title>Icon`), null);
    assert.equal(errorOf(`${P('a')}<math><mi>x`), null);
    assert.equal(errorOf(`${P('a')}<svg><g>`), null);
  });

  it('E3: checks noscript content without an end tag as the rest of the input', () => {
    assert.equal(errorOf(`${P('a')}<noscript><img src="p.gif" alt="">`), null);
    assert.equal(errorOf(`${P('a')}<noscript>${P('b')}`), STRUCTURE);
    assert.equal(errorOf(`${P('a')}<noscript><!-- x`), STRUCTURE);
  });

  it('E4: accepts <![CDATA[ without > before the end of the input, outside a block', () => {
    assert.equal(errorOf(`${P('a')}<![CDATA[x`), null);
    assert.equal(errorOf('<p data-rq-block="a">x<![CDATA[x'), STRUCTURE); // M-20
  });

  for (const [where, body] of [
    ['an svg style', '<svg><style><![CDATA[.a{fill:red}]]></style></svg>'],
    ['an svg script', '<svg><script><![CDATA[if (a<b) f();]]></script></svg>'],
    ['an svg desc', '<svg><desc><![CDATA[a<b]]></desc></svg>'],
    ['an svg title', '<svg><title>x<![CDATA[y]]>z</title></svg>'],
    ['an svg foreignObject', '<svg><foreignObject><![CDATA[a]]></foreignObject></svg>'],
    ['a math mi', '<math><mi><![CDATA[x]]></mi></math>'],
    ['a math annotation-xml', '<math><annotation-xml><![CDATA[x]]><![CDATA[y]]></annotation-xml></math>'],
  ]) {
    it(`E5: accepts a CDATA section that holds M-12 in ${where}`, () => {
      assert.deepEqual(blocksOf(doc(`${body}${P('a', 'r')}`)), [['a', 'html', 'r']]);
    });
  }

  it('E5: still applies M-12 to CDATA in foreign text content', () => {
    assert.equal(errorOf(doc(`<svg><style><![CDATA[a>b{}]]></style></svg>${P('a')}`)), STRUCTURE);
    assert.equal(errorOf(doc(`<svg><desc><![CDATA[a]>b]]></desc></svg>${P('a')}`)), STRUCTURE);
  });

  it('E5: keeps other tokens out of foreign text content', () => {
    assert.equal(errorOf(doc('<svg><style><![CDATA[a]]><g></g></style></svg>')), STRUCTURE);
    assert.equal(errorOf(doc('<svg><style><![CDATA[a]]><!-- c --></style></svg>')), STRUCTURE);
    assert.equal(errorOf(doc('<svg><desc><![CDATA[a]]><!x></desc></svg>')), STRUCTURE);
  });

  it('E5: reads a tag-like text in a CDATA section as text', () => {
    const body = `<svg><style><![CDATA[</style <b ]]></style></svg>${P('a', 'r')}`;
    assert.deepEqual(blocksOf(doc(body)), [['a', 'html', 'r']]);
  });

  it('E5 and E4: accepts a CDATA section in foreign text content that runs to the end', () => {
    assert.equal(errorOf(`${P('a')}<svg><style><![CDATA[x`), null);
  });

  it('E6: reads <? that reaches the end of the input as a comment', () => {
    assert.equal(errorOf(`${P('a')}<?php echo 1;`), null);
    assert.equal(errorOf(`${P('a')}<svg><desc><?x`), STRUCTURE);
  });

  it('E7: rejects frameset in a foreign region', () => {
    assert.equal(errorOf(doc(`<svg><frameset/></svg>${P('a')}`)), STRUCTURE);
    assert.equal(errorOf(doc(`<math><frameset></frameset></math>${P('a')}`)), STRUCTURE);
  });

  it('E8: rejects a manifest script in noscript content', () => {
    const manifest = '<script type="application/rivqen-manifest+json">{"v":1}</script>';
    assert.equal(errorOf(doc(`<noscript>${manifest}</noscript>${P('a')}`)), RESERVED);
    assert.equal(errorOf(doc(`<noscript><img alt="">${manifest}</noscript>`)), RESERVED);
    assert.equal(errorOf(`${P('a')}<noscript>${manifest}`), RESERVED); // E3: C runs to the end
    assert.equal(checkNoscript(Buffer.from(manifest)), RESERVED);
  });

  it('E8: applies all of M-13 in noscript content', () => {
    assert.equal(errorOf(doc('<noscript><SCRIPT Type="X-RIVQEN-MANIFEST">1</script></noscript>')), RESERVED);
    assert.equal(errorOf(doc('<noscript><script type="application/rivqen&#45;manifest+json">1</script></noscript>')), RESERVED);
    assert.equal(errorOf(doc('<noscript><script type=&a>1</script></noscript>')), RESERVED);
  });

  it('E8: accepts other scripts and manifest text that is raw text in the markup view', () => {
    assert.equal(errorOf(doc('<noscript><script type="text/javascript">1</script></noscript>')), null);
    assert.equal(errorOf(doc('<noscript><script>1</script></noscript>')), null);
    assert.equal(errorOf(doc('<noscript><script type="x" type="rivqen-manifest">1</script></noscript>')), null);
    assert.equal(errorOf(doc('<noscript><style><script type="rivqen-manifest"></style></noscript>')), null);
    assert.equal(errorOf(doc('<noscript><script>"<script type=&>"</script></noscript>')), null);
    assert.equal(errorOf(doc('<noscript><p type="rivqen-manifest">x</p></noscript>')), null);
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

describe('M-26 token stack depth (H-25)', () => {
  const MAX = 256; // markup.md M-26, independent of LIMITS
  const open = (name, n) => `<${name}>`.repeat(n);
  const close = (name, n) => `</${name}>`.repeat(n);
  const nest = (name, n, inner = '') => open(name, n) + inner + close(name, n);

  it('accepts a block element as element 256 and rejects it as element 257', () => {
    assert.deepEqual(blocksOf(nest('div', MAX - 1, DIV('a'))), [['a', 'html', 'x']]);
    assert.equal(errorOf(nest('div', MAX, DIV('a'))), LIMIT);
  });

  it('applies outside blocks, also when matching end tags close the elements later', () => {
    assert.deepEqual(blocksOf(nest('b', MAX) + DIV('a')), [['a', 'html', 'x']]);
    assert.equal(errorOf(nest('b', MAX + 1) + DIV('a')), LIMIT);
  });

  it('applies in block content', () => {
    assert.deepEqual(blocksOf(DIV('a', nest('b', MAX - 1, 'y'))), [['a', 'html', nest('b', MAX - 1, 'y')]]);
    assert.equal(errorOf(DIV('a', nest('b', MAX, 'y'))), LIMIT);
  });

  it('applies in a foreign region', () => {
    assert.equal(errorOf(`<svg>${nest('g', MAX - 1)}</svg>`), null);
    assert.equal(errorOf(`<svg>${nest('g', MAX)}</svg>`), LIMIT);
    assert.equal(errorOf(`<math>${nest('mrow', MAX)}</math>`), LIMIT);
  });

  it('counts HTML and foreign elements together', () => {
    assert.equal(errorOf(nest('div', 200, `<svg>${nest('g', MAX - 201)}</svg>`)), null);
    assert.equal(errorOf(nest('div', 200, `<svg>${nest('g', MAX - 200)}</svg>`)), LIMIT);
  });

  it('does not count elements that M-05 does not push', () => {
    const notPushed = '<br><img><input><hr><svg/><math/><svg><g/></svg>';
    // The svg element of the last item is pushed: 255 + 1 = 256 elements.
    assert.equal(errorOf(nest('div', MAX - 1, notPushed)), null);
    assert.equal(errorOf(nest('div', MAX, notPushed)), LIMIT);
    assert.equal(errorOf(`<svg>${nest('g', MAX - 1, '<path/><circle/>')}</svg>`), null);
  });

  it('frees the depth when an end tag pops exactly the current node', () => {
    const full = nest('b', MAX);
    assert.equal(errorOf(full + full + full), null);
    // Exact pops in a foreign region free their units too.
    assert.equal(errorOf(`<svg>${nest('g', MAX - 1)}</svg>`.repeat(3) + nest('b', MAX)), null);
  });

  it('frees the depth when a block ends', () => {
    const block = (id) => DIV(id, nest('b', MAX - 1, 'y'));
    assert.equal(run(block('a') + block('b')).blocks.length, 2);
  });

  it('counts the tags of the noscript content (M-26 item 7, H-71)', () => {
    assert.equal(errorOf(`<noscript>${nest('b', MAX - 1)}</noscript>`), null);
    assert.equal(errorOf(`<noscript>${nest('b', MAX)}</noscript>`), LIMIT);
  });

  it('counts the noscript element', () => {
    assert.equal(errorOf(nest('div', MAX - 1, '<noscript>x</noscript>')), null);
    assert.equal(errorOf(nest('div', MAX, '<noscript>x</noscript>')), LIMIT);
  });
});

describe('M-26 depth counter D (H-31)', () => {
  const MAX = 256; // markup.md M-26, independent of LIMITS
  const nest = (name, n, inner = '') => `<${name}>`.repeat(n) + inner + `</${name}>`.repeat(n);
  const deep = (n, inner) => nest('div', n, inner);

  it('keeps the units of the elements that a crossing pops', () => {
    // <div> and 255 <span>: D = 256. </div> crosses them and gives nothing back.
    const crossing = `<div>${'<span>'.repeat(MAX - 1)}</div>`;
    assert.equal(errorOf(`${crossing}text`), null); // no push after the crossing
    assert.equal(errorOf(`${crossing}<b>x</b>`), LIMIT);
    assert.equal(errorOf(`${crossing}<br>`), null); // a void element is not pushed
  });

  it('adds 2 for each <span><div></span> (the review repro)', () => {
    assert.equal(errorOf('<span><div></span>'.repeat(128)), null);
    assert.equal(errorOf(`${'<span><div></span>'.repeat(128)}<b>`), LIMIT);
    assert.equal(errorOf(`${'<span><div></span>'.repeat(300)}${DIV('x')}`), LIMIT);
  });

  it('rejects the depth shapes of the review that the tree builder keeps open', () => {
    // markup-edge-cases.md section 6.5: each unit keeps 2 or 3 of D.
    const shapes = [
      ['<h1><span><h2>x</h1>', 3], // a heading end tag stops at the other heading
      ['<li><ul></li>', 2], // </li> needs an li in list item scope
      ['<body><div></body>', 2], // </body> does not pop
      ['<form><div></form>', 2], // </form> removes only the form
      ['<p><b>x</p>y', 2], // the text reconstructs the b
    ];
    for (const [unit, units] of shapes) {
      const fit = Math.floor((MAX - 1) / units);
      assert.equal(errorOf(unit.repeat(fit) + P('a')), null, unit);
      assert.equal(errorOf(unit.repeat(fit + 1) + P('a')), LIMIT, unit);
    }
  });

  it('makes the new current node dirty after a crossing, so its exact pop gives nothing back', () => {
    const unit = '<span><span><div></span></span>'; // adds 3
    assert.equal(errorOf(unit.repeat(85) + DIV('a')), null); // 255 + 1
    assert.equal(errorOf(unit.repeat(86) + DIV('a')), LIMIT);
  });

  it('passes the dirty mark down when a dirty element is popped', () => {
    // Adds 4: </label> pops a dirty label, then q is dirty.
    const unit = '<q><label><span><div></span></label></q>';
    assert.equal(errorOf(unit.repeat(64)), null);
    assert.equal(errorOf(`${unit.repeat(64)}<p>`), LIMIT);
  });

  it('gives back the units of elements pushed above a dirty element', () => {
    // The dirty mark is on the entry: b elements pushed after it pop exactly.
    const dirty = `${'<div>'.repeat(MAX - 3)}<span><p></span>`; // D = 255, the last div is dirty
    assert.equal(errorOf(`${dirty}${'<b>x</b>'.repeat(300)}`), null);
    assert.equal(errorOf(`${dirty}<b><i>`), LIMIT);
  });

  it('never gives back the units of html and body', () => {
    assert.equal(errorOf('<body></body>'.repeat(MAX)), null);
    assert.equal(errorOf('<body></body>'.repeat(MAX + 1)), LIMIT);
    assert.equal(errorOf('<html></html>'.repeat(MAX + 1)), LIMIT);
    assert.equal(errorOf('<head></head>'.repeat(MAX + 1)), null);
  });

  it('makes the element below a popped body dirty', () => {
    // </body> makes the b dirty, so </b> gives nothing back: 2 units for each.
    assert.equal(errorOf('<b><body></body></b>'.repeat(128)), null);
    assert.equal(errorOf('<b><body></body></b>'.repeat(129)), LIMIT);
  });

  it('counts the implied row group and row of a cell in a table once for each table', () => {
    // table, row group, row and td: 4 units.
    assert.equal(errorOf(deep(MAX - 4, `<table>${'<td>x</td>'.repeat(300)}</table>`)), null);
    assert.equal(errorOf(deep(MAX - 3, '<table><td>x</td></table>')), LIMIT);
    assert.equal(errorOf(deep(MAX - 4, '<table><th>x</th></table>')), null);
    assert.equal(errorOf(deep(MAX - 3, '<table><th>x</th></table>')), LIMIT);
  });

  it('counts the implied row group of a row in a table', () => {
    assert.equal(errorOf(deep(MAX - 3, `<table>${'<tr></tr>'.repeat(300)}</table>`)), null);
    assert.equal(errorOf(deep(MAX - 2, '<table><tr></tr></table>')), LIMIT);
  });

  it('counts the implied row of a cell in a row group', () => {
    for (const group of ['tbody', 'thead', 'tfoot']) {
      for (const cell of ['td', 'th']) {
        const table = `<table><${group}>${`<${cell}>x</${cell}>`.repeat(3)}</${group}></table>`;
        assert.equal(errorOf(deep(MAX - 4, table)), null, `${group} ${cell}`);
        assert.equal(errorOf(deep(MAX - 3, table)), LIMIT, `${group} ${cell}`);
      }
    }
  });

  it('counts no implied part for a cell in a row and for a row in a row group', () => {
    assert.equal(errorOf(deep(MAX - 4, '<table><tbody><tr><td>x</td></tr></tbody></table>')), null);
    assert.equal(errorOf(deep(MAX - 3, '<table><tbody><tr></tr></tbody></table>')), null);
  });

  it('counts the implied column group of a col in a table, although col is not pushed', () => {
    assert.equal(errorOf(deep(MAX - 2, '<table><col><col></table>')), null);
    assert.equal(errorOf(deep(MAX - 1, '<table><col></table>')), LIMIT);
    // In an explicit colgroup a col implies nothing.
    assert.equal(errorOf(deep(MAX - 2, '<table><colgroup><col><col></colgroup></table>')), null);
  });

  it('gives the implied parts back when the table is popped exactly', () => {
    const table = '<table><td>x</td><col></table>'; // 5 units while open
    assert.equal(errorOf(deep(MAX - 5, table.repeat(100))), null);
  });

  it('keeps the units of a table that a crossing closes', () => {
    const unit = '<table><td>x</table>'; // </table> crosses the td: 4 units stay
    assert.equal(errorOf(unit.repeat(64)), null);
    assert.equal(errorOf(`${unit.repeat(64)}<b>`), LIMIT);
  });

  it('counts implied parts in block content', () => {
    const content = '<table><td>x</td></table>';
    assert.equal(errorOf(deep(MAX - 5, DIV('a', content))), null);
    assert.equal(errorOf(deep(MAX - 4, DIV('a', content))), LIMIT);
  });

  it('does not change D for an end tag that matches nothing', () => {
    assert.equal(errorOf(deep(MAX, '</span></b></x>')), null);
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
