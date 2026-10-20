// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// Rules of WP-17 S4 (docs/engineering/protocol/markup.md): H-78 (patch-safety
// elements and raw text in the content, M-04, M-22, M-23), H-84 (M-13 item 2,
// DL-048 a) and H-86 (M-16 JSON types). Each test names an input that only the
// named rule decides, so the reason is the rule.

import { describe, it } from 'node:test';
import assert from 'node:assert/strict';
import { ErrorCode } from '../src/markup.mjs';
import { blocksOf, doc, errorOf, run } from './helpers.mjs';

const { STRUCTURE, RESERVED, BAD_JSON, FORBIDDEN_ELEMENT, DUPLICATE } = ErrorCode;

const inBlock = (content, tag = 'div') => `<${tag} data-rq-block="a">${content}</${tag}>`;
const contentOf = (content) => blocksOf(doc(inBlock(content)))[0][2];

describe('patch-safety elements in the content (H-78, M-22 item 2)', () => {
  const names = ['base', 'basefont', 'bgsound', 'embed', 'iframe', 'link', 'meta', 'object', 'script', 'style'];
  for (const name of names) {
    it(`accepts a <${name}> start tag in the content`, () => {
      const closed = ['iframe', 'object', 'script', 'style'].includes(name) ? `</${name}>` : '';
      assert.equal(errorOf(doc(inBlock(`<${name}>${closed}`))), null);
    });
  }

  it('still rejects the other names of FORBIDDEN_CONTENT', () => {
    for (const name of ['noembed', 'noframes', 'plaintext', 'textarea', 'title', 'xmp']) {
      assert.equal(errorOf(doc(inBlock(`<${name}>`))), STRUCTURE, name);
    }
  });

  it('keeps M-22 item 3 for a self-closing script, style, iframe and object', () => {
    for (const name of ['script', 'style', 'iframe', 'object']) {
      assert.equal(errorOf(doc(inBlock(`<${name}/>`))), STRUCTURE, name);
    }
  });

  it('keeps M-22 item 4 for the end tag of a void element', () => {
    for (const name of ['embed', 'link', 'meta', 'base', 'basefont', 'bgsound']) {
      assert.equal(errorOf(doc(inBlock(`<${name}></${name}>`))), STRUCTURE, name);
    }
  });

  it('keeps M-13 for a script in the content', () => {
    assert.equal(errorOf(doc(inBlock('<script type="rivqen-manifest">1</script>'))), RESERVED);
  });

  it('accepts the foreign elements that the old item 5 rejected', () => {
    assert.equal(errorOf(doc(inBlock('<svg><script>1</script><style>p{}</style><a></a></svg>'))), null);
  });
});

describe('raw text in the content (H-78, M-04, M-23)', () => {
  it('reads a block end tag in a script as text', () => {
    const c = 'x<script>document.write("</div>")</script>y';
    assert.equal(contentOf(c), c);
  });

  it('reads a block start tag in a script as text (M-17 does not apply)', () => {
    const c = '<script>"<p data-rq-block=\'b\'>"</script>';
    assert.equal(contentOf(c), c);
  });

  it('ends a script at </SCRIPT > and at </script foo>', () => {
    assert.equal(contentOf('<script>1</SCRIPT >'), '<script>1</SCRIPT >');
    assert.equal(contentOf('<script>1</script foo>'), '<script>1</script foo>');
  });

  it('does not end a script at </scripty>', () => {
    assert.equal(errorOf(doc(inBlock('<script>1</scripty>'))), STRUCTURE);
    assert.equal(contentOf('<script>1</scripty></script>'), '<script>1</scripty></script>');
  });

  it('uses the escaped and double-escaped states of script data', () => {
    const c = '<script>a<!--<script>b</script>c--></script>';
    assert.equal(contentOf(c), c);
  });

  it('rejects a script that stays in the double-escaped state (M-20)', () => {
    assert.equal(errorOf(doc(inBlock('<script><!--<script></script>'))), STRUCTURE);
  });

  it('reads style and iframe as RAWTEXT', () => {
    for (const name of ['style', 'iframe']) {
      const c = `<${name}><p data-rq-block="b"></div><!--</${name}>`;
      assert.equal(contentOf(c), c, name);
    }
  });

  it('rejects a raw text element that has no end tag before the end of the input', () => {
    assert.equal(errorOf(`<div data-rq-block="a"><style>x</div>`), STRUCTURE);
  });

  it('has the block end tag after the raw text end tag only', () => {
    assert.equal(errorOf(doc('<div data-rq-block="a"><script>1</div>')), STRUCTURE);
  });
});

describe('M-13 item 2: character references in the type (H-84, DL-048 a)', () => {
  const script = (type) => doc(`<script type="${type}">1</script>`);
  const rejected = [
    'text/x&amp;y', 'text/a&b', 'application&#x2F;x', 'text/&#0000065pp', 'text/plain&#X9',
    'application/rivqen&#45;manifest+json', 'application/r&#305;vqen-manifest+json',
    'a&#32;b', 'a&#0;b', 'a&#43;b', 'a&#46;b', 'a&#48;b', 'a&#57;b', 'a&#128;b',
    'a&Zb', 'a&zb', 'a&#90;b', 'a&#122;b', 'a&#xA;b', 'a&#xa;b', 'a&#xF;b', 'a&#xf;b',
    'a&#99999999999999999999;b', 'a&#xFFFFFFFFFFFFFFFFFFFF;b', 'a&#65', 'a&#x41',
  ];
  for (const type of rejected) {
    it(`rejects ${type}`, () => assert.equal(errorOf(script(type)), RESERVED));
  }
  const accepted = [
    'text/x-tmpl & b', 'text/x&#58;y', 'text/x&#x7E;', 'text/x&#;', 'text/x&#x;', 'text/x&', 'a&#xg',
    'a&#a;b', 'a&#33a;b', 'a&#x_;b', 'a&#x`;b', 'a&#xG;b', 'a&#xg;b',
    'a&#33;b', 'a&#42;b', 'a&#x7f;b', 'a&&1',
  ];
  for (const type of accepted) {
    it(`accepts ${type}`, () => assert.equal(errorOf(script(type)), null));
  }
  it('checks the & of a reference that follows another &', () => {
    assert.equal(errorOf(script('a&&#65;')), RESERVED);
  });
  it('applies the same rule in the noscript content', () => {
    assert.equal(errorOf(doc('<noscript><script type="a&#65;">1</script></noscript>')), RESERVED);
    assert.equal(errorOf(doc('<noscript><script type="a&#58;">1</script></noscript>')), null);
  });
});

describe('M-16 JSON types (H-86)', () => {
  const json = (type, content = '{}', id = 'a') =>
    doc(`<script type="${type}" data-rq-block="${id}">${content}</script>`);
  it('accepts application/json and application/ld+json as json blocks', () => {
    assert.deepEqual(blocksOf(json('application/json')), [['a', 'json', '{}']]);
    assert.deepEqual(blocksOf(json('application/ld+json')), [['a', 'json', '{}']]);
  });
  it('rejects other types, other case and white space', () => {
    for (const t of ['Application/LD+JSON', 'application/ld+json ', 'application/ld+JSON', 'application/x+json', 'text/json']) {
      assert.equal(errorOf(json(t)), FORBIDDEN_ELEMENT, t);
    }
  });
  it('applies M-25 to application/ld+json', () => {
    assert.equal(errorOf(json('application/ld+json', '{"a":}')), BAD_JSON);
  });
  it('counts both types in M-21 duplicates', () => {
    const two = doc(
      '<script type="application/json" data-rq-block="a">{}</script>'
      + '<script type="application/ld+json" data-rq-block="a">{}</script>',
    );
    assert.equal(errorOf(two), DUPLICATE);
  });
  it('gives a valid result for a json-ld block', () => {
    assert.equal(run(json('application/ld+json')).valid, true);
  });
});
