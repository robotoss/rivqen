// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// Tokenizer tests: examples per state group, and a differential test against the
// parse5 8.0.1 tokenizer (WHATWG tokenizer with source locations), driven with the
// same M-04 state changes.

import { describe, it } from 'node:test';
import assert from 'node:assert/strict';
import { Tokenizer as Parse5Tokenizer, TokenizerMode } from 'parse5';
import { TextMode, TokenType, Tokenizer, tagName } from '../src/tokenizer.mjs';
import { TEXT_MODE } from '../src/names.mjs';
import { rng } from './helpers.mjs';

// Deeper runs: RQP_FUZZ_ITERATIONS=300000 RQP_FUZZ_SEED=7 npm test
const ITERATIONS = Number(process.env.RQP_FUZZ_ITERATIONS ?? 4000);
const SEED = Number(process.env.RQP_FUZZ_SEED ?? 0x5eed);

const KIND = {
  [TokenType.START_TAG]: 'start',
  [TokenType.END_TAG]: 'end',
  [TokenType.COMMENT]: 'comment',
  [TokenType.DOCTYPE]: 'doctype',
};

/** Tokens of our tokenizer with the M-04 state changes (HTML context only). */
function ours(input) {
  const bytes = Buffer.from(input, 'utf8');
  const tok = new Tokenizer(bytes);
  const out = [];
  const raw = (span) => (span === null ? null : bytes.toString('utf8', span.start, span.end));
  for (let guard = 0; guard <= bytes.length + 1; guard++) {
    const t = tok.next();
    if (t.type === TokenType.EOF) {
      assert.equal(t.start, bytes.length);
      return out;
    }
    const item = { kind: KIND[t.type], start: t.start, end: t.end };
    if (t.type === TokenType.START_TAG || t.type === TokenType.END_TAG) {
      item.name = t.name;
      item.selfClosing = t.selfClosing;
      item.block = raw(t.blockAttr);
      item.type = raw(t.typeAttr);
    }
    if (t.type === TokenType.START_TAG) {
      const mode = TEXT_MODE.get(t.name);
      if (mode !== undefined) tok.switchTo(mode, t.name);
    }
    out.push(item);
  }
  throw new Error('the tokenizer did not make progress');
}

const P5_MODE = {
  [TextMode.RCDATA]: TokenizerMode.RCDATA,
  [TextMode.RAWTEXT]: TokenizerMode.RAWTEXT,
  [TextMode.SCRIPT_DATA]: TokenizerMode.SCRIPT_DATA,
  [TextMode.PLAINTEXT]: TokenizerMode.PLAINTEXT,
};

/** Tokens of the parse5 tokenizer, with UTF-16 offsets converted to UTF-8 byte offsets. */
function reference(input) {
  const byteAt = [0];
  for (let i = 0; i < input.length; i++) {
    const unit = input.charCodeAt(i);
    byteAt.push(byteAt[i] + (unit < 0x80 ? 1 : unit < 0x800 || (unit >= 0xd800 && unit <= 0xdfff) ? 2 : 3));
  }
  const out = [];
  // For a comment or DOCTYPE that ends at EOF, parse5 8.0.1 gives the end offset
  // input.length + 1 (it adds 1 for a '>' that is not there). Clamp it.
  // A comment or DOCTYPE start is computed back from the current character; when
  // that character is outside the BMP (2 UTF-16 units), parse5 8.0.1 gives a start
  // one unit after the '<'. Move it back to the '<'.
  const startOf = (t) => {
    const s = t.location.startOffset;
    return input.charCodeAt(s) !== 0x3c && input.charCodeAt(s - 1) === 0x3c ? s - 1 : s;
  };
  const loc = (t) => ({
    start: byteAt[startOf(t)],
    end: byteAt[Math.min(t.location.endOffset, input.length)],
  });
  const attr = (t, name) => t.attrs.find((a) => a.name === name)?.value ?? null;
  const tag = (kind, t) => ({
    kind,
    ...loc(t),
    name: t.tagName,
    selfClosing: t.selfClosing,
    block: attr(t, 'data-rq-block'),
    type: attr(t, 'type'),
  });
  const handler = {
    onStartTag(t) {
      out.push(tag('start', t));
      const mode = TEXT_MODE.get(t.tagName);
      if (mode !== undefined) tokenizer.state = P5_MODE[mode];
    },
    onEndTag: (t) => out.push(tag('end', t)),
    onComment: (t) => out.push({ kind: 'comment', ...loc(t) }),
    onDoctype: (t) => out.push({ kind: 'doctype', ...loc(t) }),
    onEof() {},
    onCharacter() {},
    onNullCharacter() {},
    onWhitespaceCharacter() {},
  };
  const tokenizer = new Parse5Tokenizer({ sourceCodeLocationInfo: true }, handler);
  tokenizer.write(input, true);
  return out;
}

// Raw values and decoded values are equal when the raw value has no character
// reference, NUL or CR. For other raw values only the presence is compared.
function assertSameTokens(input) {
  const actual = ours(input);
  const expected = reference(input);
  const opaque = (v) => v !== null && /[&\0\r]/.test(v);
  const adjusted = actual.map((t, i) => {
    const e = expected[i];
    if (!('name' in t) || e === undefined || !('name' in e)) return t;
    return {
      ...t,
      block: opaque(t.block) && e.block !== null ? e.block : t.block,
      type: opaque(t.type) && e.type !== null ? e.type : t.type,
    };
  });
  assert.deepEqual(adjusted, expected, JSON.stringify(input));
}

function kinds(input) {
  return ours(input).map((t) => (t.name === undefined ? t.kind : `${t.kind}:${t.name}`));
}

describe('tokenizer', () => {
  describe('tags and attributes', () => {
    it('gives start and end tags with lower-case names and byte offsets', () => {
      assert.deepEqual(ours('é<DiV>x</dIv>'), [
        { kind: 'start', start: 2, end: 7, name: 'div', selfClosing: false, block: null, type: null },
        { kind: 'end', start: 8, end: 14, name: 'div', selfClosing: false, block: null, type: null },
      ]);
    });

    it('keeps the raw value spans of the first data-rq-block and type attributes', () => {
      const [t] = ours(`<p a=1 TYPE='x&amp;' data-rq-block="b" type=z Data-Rq-Block=c>`);
      assert.equal(t.type, 'x&amp;');
      assert.equal(t.block, 'b');
    });

    it('reads unquoted values up to white space or >', () => {
      const [t] = ours('<p data-rq-block=a/b"c type=t>');
      assert.equal(t.block, 'a/b"c');
      assert.equal(t.type, 't');
      assert.equal(t.selfClosing, false);
    });

    it('gives an attribute without a value an empty value', () => {
      assert.equal(ours('<p data-rq-block>')[0].block, '');
      assert.equal(ours('<p data-rq-block >')[0].block, '');
      assert.equal(ours('<p data-rq-block=>')[0].block, '');
      assert.equal(ours('<p data-rq-block = "v">')[0].block, 'v');
    });

    it('accepts attributes without white space after a quoted value', () => {
      const [t] = ours(`<p type="a"data-rq-block='b'>`);
      assert.equal(t.type, 'a');
      assert.equal(t.block, 'b');
    });

    it('treats = at the start of an attribute name as part of the name', () => {
      assert.equal(ours('<p =data-rq-block="x">')[0].block, null);
    });

    it('sets the self-closing flag only for /> at the end', () => {
      assert.equal(ours('<br/>')[0].selfClosing, true);
      assert.equal(ours('<br a="1"/>')[0].selfClosing, true);
      assert.equal(ours('<br / >')[0].selfClosing, false);
      assert.equal(ours('<br a=1/>')[0].selfClosing, false);
      assert.equal(ours('<br/ data-rq-block="x">')[0].block, 'x');
    });

    it('treats CR, LF, tab, form feed and space as white space in tags', () => {
      for (const ws of ['\r', '\n', '\t', '\f', ' ']) {
        assert.equal(ours(`<p${ws}data-rq-block${ws}=${ws}x${ws}>`)[0].block, 'x');
      }
    });

    it('drops a tag at the end of the input', () => {
      for (const input of ['<p', '<p ', '<p a', '<p a=', '<p a="x', "<p a='x", '<p a=x', '<p a="x"', '<p/', '</p']) {
        assert.deepEqual(ours(input), [], input);
      }
    });

    it('accepts every ASCII letter as the first character of a tag name', () => {
      assert.deepEqual(kinds('<a></Z><z></A>'), ['start:a', 'end:z', 'start:z', 'end:a']);
      assert.deepEqual(kinds('<@><[><`><{>'), []);
    });

    it('replaces U+0000 in a tag name with U+FFFD and lowers only ASCII letters', () => {
      assert.equal(tagName(Buffer.from('A\0É'), 0, 5), 'a�É');
      assert.equal(tagName(Buffer.from('ab'), 0, 2), 'ab');
    });
  });

  describe('text, comments and declarations', () => {
    it('treats < not followed by a letter, /, ! or ? as text', () => {
      assert.deepEqual(ours('a < b <1 <<p>'), [
        { kind: 'start', start: 10, end: 13, name: 'p', selfClosing: false, block: null, type: null },
      ]);
    });

    it('skips </> and treats </ at the end as text', () => {
      assert.deepEqual(kinds('</><p></'), ['start:p']);
    });

    it('ends comments by the WHATWG comment states', () => {
      const ends = (input) => ours(input).map((t) => t.end);
      assert.deepEqual(ends('<!---->'), [7]);
      assert.deepEqual(ends('<!-->x'), [5]);
      assert.deepEqual(ends('<!--->x'), [6]);
      assert.deepEqual(ends('<!-- a --!>x'), [11]);
      assert.deepEqual(ends('<!-- a -- b --->x'), [16]);
      assert.deepEqual(ends('<!--<!---->x'), [11]);
      assert.deepEqual(ends('<!--<!-x-->'), [11]);
      assert.deepEqual(ends('<!-- --!x -->'), [13]);
      assert.deepEqual(ends('<!-- a'), [6]);
    });

    it('ends bogus comments, processing instructions and DOCTYPEs at the first >', () => {
      assert.deepEqual(ours('<!x a>b').map((t) => [t.kind, t.end]), [['comment', 6]]);
      assert.deepEqual(ours('</ a>b').map((t) => [t.kind, t.end]), [['comment', 5]]);
      assert.deepEqual(ours('<?x "a>"b').map((t) => [t.kind, t.end]), [['comment', 7]]);
      assert.deepEqual(ours('<!DocType x "a>"').map((t) => [t.kind, t.end]), [['doctype', 15]]);
      assert.deepEqual(ours('<!doctype').map((t) => [t.kind, t.end]), [['doctype', 9]]);
    });

    it('marks <![CDATA[ comments and whether they end with >', () => {
      const tok = (input) => new Tokenizer(Buffer.from(input)).next();
      assert.deepEqual([tok('<![CDATA[a]]>').cdata, tok('<![CDATA[a]]>').closed], [true, true]);
      assert.deepEqual([tok('<![CDATA[a').cdata, tok('<![CDATA[a').closed], [true, false]);
      assert.equal(tok('<![cdata[a]]>').cdata, false);
      assert.equal(tok('<!-- a -->').cdata, false);
    });
  });

  describe('raw text', () => {
    it('ends RCDATA and RAWTEXT only at the appropriate end tag', () => {
      assert.deepEqual(kinds('<title><p></titles></title\t>'), ['start:title', 'end:title']);
      assert.deepEqual(kinds('<style></style'), ['start:style']);
      assert.deepEqual(kinds('<xmp></XMP/><p>'), ['start:xmp', 'end:xmp', 'start:p']);
    });

    it('reads attributes of the appropriate end tag', () => {
      assert.deepEqual(ours('<textarea></textarea a=">">').map((t) => t.end), [10, 27]);
    });

    it('never ends PLAINTEXT', () => {
      assert.deepEqual(kinds('<plaintext></plaintext><p>'), ['start:plaintext']);
    });

    it('follows the script data escape states', () => {
      const scripts = {
        '<script>a</script>': 2,
        '<script><!--</script>': 2,
        '<script><!--<script></script>--></script>': 2,
        '<script><!--<script>--></script>': 2,
        '<script><!--<script>-></script>': 1,
        '<script><!--<script></script >--></script>': 2,
        '<script><!--<scripts></script>': 2,
        '<script><!-- -- > <script></script>': 1,
        '<script><!---><script></script>': 2,
        '<script><!--<script/></script>': 1,
        '<script><!--<script</script>': 2,
        '<script><!--<script>-</script>-</script>': 2,
        '<script><!--<script><</script></script>': 2,
        '<script><!x</script>': 2,
      };
      for (const [input, count] of Object.entries(scripts)) {
        assert.equal(ours(input).length, count, input);
        assertSameTokens(input);
      }
    });
  });

  describe('differential test against the parse5 8.0.1 tokenizer', () => {
    const pieces = [
      '<', '</', '>', '/>', '/', '<!', '<!--', '-->', '--!>', '-', '--', '!', '<?', '?>', '<![CDATA[',
      ']]>', '<!DOCTYPE', '<!doctype ', '<p', '<div', '<P', '<script', '</script', '<SCRIPT', '<title',
      '</title', '<style', '</style', '<textarea', '</textarea', '<noscript', '</noscript', '<xmp',
      '<iframe', '<plaintext', '<svg', '</p', '</div', '<br', ' ', '\t', '\n', '\r\n', '\r', '\f', '=', '"',
      "'", 'data-rq-block', 'DATA-RQ-BLOCK', 'type', 'Type', 'a', 'x', 'script', 'title', '="v"', "='a>b'",
      '=u', '&amp;', '&#60;', 'é', '漢', '\u{1F600}', '\0', '`', 'b', '<a', '</a',
    ];

    function generate(r) {
      const n = r.int(r.next() < 0.9 ? 40 : 200);
      let s = '';
      for (let i = 0; i < n; i++) s += r.pick(pieces);
      return s;
    }

    it('gives the same tags, comments and DOCTYPEs with the same byte offsets', () => {
      const r = rng(SEED);
      for (let i = 0; i < ITERATIONS; i++) {
        const input = generate(r);
        assertSameTokens(input);
      }
    });

    it('agrees on the fixed examples of this file', () => {
      for (const input of [
        '<script><!--<script></script>--></script><p data-rq-block="a">x</p>',
        '<!--<!-- a --!><title>&lt;</title><p\r\ntype=a>',
        '<!DOCTYPE html PUBLIC "a>b"><![CDATA[x]]><?pi?></ x>',
      ]) {
        assertSameTokens(input);
      }
    });
  });
});
