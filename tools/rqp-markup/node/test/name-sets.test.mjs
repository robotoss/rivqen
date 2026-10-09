// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// The name sets of markup.md section 2.1, checked by behavior. The lists are
// copied from markup.md here on purpose: they are an independent oracle for
// src/names.mjs.

import { describe, it } from 'node:test';
import assert from 'node:assert/strict';
import { ErrorCode } from '../src/markup.mjs';
import { blocksOf, doc, errorOf } from './helpers.mjs';

const { STRUCTURE } = ErrorCode;
const H = ['h1', 'h2', 'h3', 'h4', 'h5', 'h6'];

const VOID = ['area', 'base', 'basefont', 'bgsound', 'br', 'col', 'embed', 'frame', 'hr', 'image', 'img',
  'input', 'keygen', 'link', 'meta', 'param', 'source', 'track', 'wbr'];
const PCLOSE = ['address', 'article', 'aside', 'blockquote', 'center', 'dd', 'details', 'dialog', 'dir', 'div',
  'dl', 'dt', 'fieldset', 'figcaption', 'figure', 'footer', 'form', ...H, 'header', 'hgroup', 'hr', 'li',
  'listing', 'main', 'menu', 'nav', 'ol', 'p', 'plaintext', 'pre', 'search', 'section', 'summary', 'table',
  'ul', 'xmp'];
const GUARDED = ['a', 'applet', 'button', 'caption', 'colgroup', 'marquee', 'math', 'nobr', 'object', 'select',
  'svg', 'table', 'tbody', 'td', 'template', 'tfoot', 'th', 'thead', 'tr'];
const TABLE_PART = ['caption', 'colgroup', 'tbody', 'td', 'tfoot', 'th', 'thead', 'tr'];
const BREAKOUT = ['b', 'big', 'blockquote', 'body', 'br', 'center', 'code', 'dd', 'div', 'dl', 'dt', 'em',
  'embed', 'font', ...H, 'head', 'hr', 'i', 'img', 'li', 'listing', 'menu', 'meta', 'nobr', 'ol', 'p', 'pre',
  'ruby', 's', 'small', 'span', 'strike', 'strong', 'sub', 'sup', 'table', 'tt', 'u', 'ul', 'var'];
const INTEGRATION = ['annotation-xml', 'desc', 'foreignobject', 'mi', 'mn', 'mo', 'ms', 'mtext', 'title'];
const BLOCK_HTML = ['article', 'aside', 'div', 'footer', ...H, 'header', 'main', 'nav', 'p', 'section', 'span',
  'title'];
const TEXT = ['title', 'textarea', 'iframe', 'noembed', 'noframes', 'noscript', 'style', 'xmp', 'script',
  'plaintext'];

describe('name sets of markup.md section 2.1', () => {
  describe('VOID', () => {
    for (const name of VOID) {
      it(`does not push ${name}`, () => {
        // A pushed element would make </name> cross the guarded a (M-06).
        assert.equal(errorOf(doc(`<${name}><a>x</${name}></a>`)), null);
      });
    }

    it('pushes a name that is not void', () => {
      assert.equal(errorOf(doc('<span><a>x</span></a>')), STRUCTURE);
    });
  });

  describe('TEXT and the M-04 state changes', () => {
    for (const name of TEXT.filter((n) => n !== 'noscript')) {
      it(`treats markup after a ${name} start tag as text`, () => {
        const body = `<${name}><p data-rq-block="x">y</p></${name}><p data-rq-block="z">w</p>`;
        const expected = name === 'plaintext' ? [] : ['z'];
        assert.deepEqual(blocksOf(doc(body)).map((b) => b[0]), expected);
      });
    }

    it('treats markup after a noscript start tag as text, then checks it by M-11', () => {
      assert.equal(errorOf(doc('<noscript><p>x</p></noscript><p data-rq-block="z">w</p>')), null);
    });

    for (const name of TEXT) {
      it(`rejects a self-closing ${name} in a foreign region`, () => {
        assert.equal(errorOf(doc(`<svg><${name}/></svg>`)), STRUCTURE);
      });
    }

    it('accepts a self-closing element that is not in TEXT in a foreign region', () => {
      assert.equal(errorOf(doc('<svg><path/></svg>')), null);
    });
  });

  describe('PCLOSE', () => {
    // li, dd, dt need a list in the content (M-24 c), and every list is in PCLOSE;
    // plaintext and xmp are forbidden content (M-22). They are left out here.
    const content = (name) => (name === 'hr' ? '<hr>' : `<${name}>x</${name}>`);
    for (const name of PCLOSE.filter((n) => !['li', 'dd', 'dt', 'plaintext', 'xmp'].includes(n))) {
      it(`rejects ${name} in a span block and accepts it in a div block`, () => {
        assert.equal(errorOf(doc(`<span data-rq-block="a">${content(name)}</span>`)), STRUCTURE);
        const table = name === 'table' ? '<table></table>' : content(name);
        assert.equal(errorOf(doc(`<div data-rq-block="a">${table}</div>`)), null);
      });
    }

    it('accepts a name that is not in PCLOSE in a span block', () => {
      assert.equal(errorOf(doc('<span data-rq-block="a"><b>x</b></span>')), null);
    });
  });

  describe('headings (M-24 b)', () => {
    for (const name of H) {
      it(`rejects ${name} in a heading block and in an open heading of the content`, () => {
        assert.equal(errorOf(doc(`<h3 data-rq-block="a"><${name}>x</${name}></h3>`)), STRUCTURE);
        assert.equal(errorOf(doc(`<div data-rq-block="a"><${name}><h1>x</h1></${name}></div>`)), STRUCTURE);
      });
    }
  });

  describe('lists (M-24 c)', () => {
    for (const list of ['ul', 'ol', 'menu', 'dl']) {
      it(`accepts li, dd and dt in a ${list} opened in the content`, () => {
        const items = '<li>a</li><dd>b</dd><dt>c</dt>';
        assert.equal(errorOf(doc(`<div data-rq-block="a"><${list}>${items}</${list}></div>`)), null);
      });
    }
  });

  describe('GUARDED (M-06)', () => {
    // svg and math open a foreign region and select has its own rule (M-08):
    // these make the end tag invalid before crossing is checked.
    for (const name of GUARDED.filter((n) => !['svg', 'math', 'select'].includes(n))) {
      it(`rejects an end tag that crosses ${name}`, () => {
        assert.equal(errorOf(doc(`<div><${name}>x</div>`)), STRUCTURE);
      });
    }

    it('accepts an end tag that crosses an element that is not guarded', () => {
      assert.equal(errorOf(doc('<div><span><b><li>x</div>')), null);
    });

    for (const part of TABLE_PART) {
      it(`lets </table> cross ${part}`, () => {
        assert.equal(errorOf(doc(`<table><${part}>x</table>`)), null);
      });
    }

    for (const group of ['tbody', 'thead', 'tfoot']) {
      it(`lets </${group}> cross tr and td`, () => {
        assert.equal(errorOf(doc(`<table><${group}><tr><td>x</${group}></table>`)), null);
      });
    }

    it('lets </tr> cross td and th', () => {
      assert.equal(errorOf(doc('<table><tr><td>x</tr><tr><th>y</tr></table>')), null);
    });
  });

  describe('BREAKOUT (M-07 rule 1)', () => {
    for (const name of BREAKOUT) {
      it(`rejects ${name} in svg and in math`, () => {
        assert.equal(errorOf(doc(`<svg><${name}></svg>`)), STRUCTURE);
        assert.equal(errorOf(doc(`<math><${name}></math>`)), STRUCTURE);
      });
    }

    it('accepts a name that is not in BREAKOUT in svg', () => {
      assert.equal(errorOf(doc('<svg><g><rect></rect></g></svg>')), null);
    });
  });

  describe('INTEGRATION (M-07 rule 3)', () => {
    for (const name of INTEGRATION) {
      it(`accepts only text in ${name}`, () => {
        assert.equal(errorOf(doc(`<svg><${name}>text</${name}></svg>`)), null);
        assert.equal(errorOf(doc(`<svg><${name}><g></g></${name}></svg>`)), STRUCTURE);
      });
    }

    it('accepts elements in an element that is not an integration point', () => {
      assert.equal(errorOf(doc('<svg><g><g></g></g></svg>')), null);
    });
  });

  describe('BLOCK_HTML (M-16)', () => {
    for (const name of BLOCK_HTML) {
      it(`accepts ${name} as an html block`, () => {
        assert.deepEqual(blocksOf(doc(`<${name} data-rq-block="a">x</${name}>`)), [['a', 'html', 'x']]);
      });
    }
  });

  describe('select (M-08)', () => {
    for (const name of ['option', 'optgroup']) {
      it(`accepts ${name} start and end tags in select`, () => {
        assert.equal(errorOf(doc(`<select><${name}>x</${name}></select>`)), null);
      });
    }

    it('accepts hr in select', () => {
      assert.equal(errorOf(doc('<select><hr></select>')), null);
    });
  });

  describe('table parts (M-09)', () => {
    const allowed = {
      caption: ['table'],
      colgroup: ['table'],
      tbody: ['table'],
      thead: ['table'],
      tfoot: ['table'],
      col: ['table', 'colgroup'],
      tr: ['table', 'tbody', 'thead', 'tfoot'],
      td: ['table', 'tbody', 'thead', 'tfoot', 'tr'],
      th: ['table', 'tbody', 'thead', 'tfoot', 'tr'],
    };
    const open = (f) => (f === 'table' ? '<table>' : `<table><${f}>`);
    for (const [name, parents] of Object.entries(allowed)) {
      for (const f of ['table', 'tbody', 'thead', 'tfoot', 'tr', 'td', 'th', 'caption', 'colgroup']) {
        const ok = parents.includes(f);
        it(`${ok ? 'accepts' : 'rejects'} ${name} when F is ${f}`, () => {
          const tag = name === 'col' ? '<col>' : `<${name}></${name}>`;
          // F = td and th need a row; the row is closed explicitly before.
          const prefix = f === 'td' || f === 'th' ? `<table><tr><${f}>` : open(f);
          assert.equal(errorOf(doc(`${prefix}${tag}</table>`)), ok ? null : STRUCTURE);
        });
      }
    }
  });
});
