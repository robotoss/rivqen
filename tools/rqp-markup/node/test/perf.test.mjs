// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// [perf] Linear time on adversarial inputs of the maximum size (5 MiB, M-01).
// A quadratic step on any of these inputs takes minutes, not seconds. The time
// limit is generous so that a slow machine does not fail the test.
//
// The depth counter D of M-26 (D <= 256, H-31) bounds the token stack, and so
// the cost of one pop or one end-tag search. A step that is linear in the
// stack depth is then at most 256 times slower, not quadratic in the input
// length. A crossing keeps its units in D, so a document can cross at most
// 256 elements in total; the crossing shapes below cross once, at the limit.
// The noscript content C has its own stack (M-11 item 5), bounded by the size
// of C only.

import { describe, it } from 'node:test';
import assert from 'node:assert/strict';
import { ErrorCode, LIMITS, analyze } from '../src/markup.mjs';

const MAX = LIMITS.maxDocumentBytes;
const LIMIT_MS = 10_000;

/** Repeat `unit` between `head` and `tail` up to the maximum input size. */
function fill(head, unit, tail = '') {
  const n = Math.floor((MAX - head.length - tail.length) / unit.length);
  return Buffer.from(head + unit.repeat(n) + tail, 'utf8');
}

function manyAttributes() {
  const parts = ['<div'];
  let size = 4;
  for (let i = 0; size < MAX - 16; i++) {
    const attr = ` a${i.toString(36)}`;
    parts.push(attr);
    size += attr.length;
  }
  parts.push('>');
  return Buffer.from(parts.join(''), 'latin1');
}

/** Four blocks with about 1 MiB of content each (M-26 allows 1 MiB). */
function largeBlocks(open, unit, close, endTag) {
  const content = open + unit.repeat(Math.floor((LIMITS.maxBlockBytes - 256) / unit.length)) + close;
  const parts = [];
  for (const id of ['a', 'b', 'c', 'd']) parts.push(endTag(id).replace('CONTENT', content));
  return Buffer.from(parts.join(''), 'utf8');
}

/** `open` n times, then `close` n times, up to the maximum input size. */
function nested(open, close, head = '', tail = '') {
  const n = Math.floor((MAX - head.length - tail.length) / (open.length + close.length));
  return Buffer.from(head + open.repeat(n) + close.repeat(n) + tail, 'utf8');
}

/** `open` d times, then `close` d times. */
const nest = (open, close, d) => open.repeat(d) + close.repeat(d);

/** Five html blocks. The content of each is `unit` repeated, just under 1 MiB (M-26). */
function blocks(unit) {
  const content = unit.repeat(Math.floor((LIMITS.maxBlockBytes - 4096) / unit.length));
  const parts = [];
  for (let k = 1; k <= 5; k++) parts.push(`<div data-rq-block="b${k}">${content}</div>`);
  return Buffer.from(parts.join(''), 'utf8');
}

/** Five html blocks. The content of each is `open` n times, then `close` n times, just under 1 MiB. */
function nestedBlocks(open, close) {
  return blocks(nest(open, close, Math.floor((LIMITS.maxBlockBytes - 4096) / (open.length + close.length))));
}

// M-26 (H-25): at most 256 elements on the token stack. The block element of
// blocks() is at depth 1, so its content can add 255 elements.
const DEPTH = LIMITS.maxOpenElements;
const LIST = `<ul>${nest('<li><ul>', '</ul></li>', (DEPTH - 2) / 2)}</ul>`; // 255 elements

const inputs = {
  'one tag with distinct attributes': manyAttributes,
  'one tag with repeated data-rq-block attributes': () => fill('<div', ' data-rq-block=a', '>'),
  'deep nesting': () => fill('', '<div>'),
  'deep nesting of crossable elements, then one end tag': () => fill('<div>', '<span>', '</div>'),
  'unmatched end tags': () => fill('', '</x>'),
  'a comment with many dashes and bangs': () => fill('<!--', '-<!-!-'),
  'script data with escape openers': () => fill('<script>', '<!--<script>-->'),
  'RCDATA with near end tags': () => fill('<title>', '</titl'),
  'many noscript elements': () => fill('', '<noscript><img></noscript>'),
  'JSON blocks of 1 MiB': () => largeBlocks(
    '['.repeat(63), '{"k":-1.5e3,"s":"\\u00e9"},', '0' + ']'.repeat(63),
    (id) => `<script type="application/json" data-rq-block="${id}">CONTENT</script>`,
  ),
  'html blocks of 1 MiB': () => largeBlocks(
    '', '<b>x</b><br>&amp;', '',
    (id) => `<div data-rq-block="${id}">CONTENT</div>`,
  ),
  'svg with many elements': () => fill('<svg>', '<g></g>', '</svg>'),
  'table rows with cells': () => fill('<table>', '<tr><td>x</td></tr>', '</table>'),
  'many CDATA sections': () => fill('', '<![CDATA[]]>'),
  // Beyond the depth limit: the push of element 257 ends the pass (M-26).
  'deep nesting closed by matching end tags, beyond the depth limit': () => nested('<b>', '</b>'),
  'mixed names closed by matching end tags, beyond the depth limit': () => nested('<b><i>', '</i></b>'),
  'nested lists in block content, beyond the depth limit': () => nestedBlocks('<ul><li>', '</li></ul>'),
  'nested elements in block content, beyond the depth limit': () => nestedBlocks('<b>', '</b>'),
  'deep nesting in svg, beyond the depth limit': () => nested('<g>', '</g>', '<svg>', '</svg>'),
  'unmatched end tags on a stack beyond the depth limit': () => nested('<div>', '</x>'),
  // At the depth limit (256 elements), repeated up to 5 MiB: the most stack
  // work that a valid document can have. Each end tag pops one element, except
  // the two inputs whose names say otherwise.
  'nesting to the depth limit closed by matching end tags': () => fill('', nest('<b>', '</b>', DEPTH)),
  'mixed names to the depth limit closed by matching end tags': () =>
    fill('', nest('<b><i>', '</i></b>', DEPTH / 2)),
  'nested lists to the depth limit in block content': () => blocks(LIST),
  'nested elements to the depth limit in block content': () => blocks(nest('<b>', '</b>', DEPTH - 1)),
  'nesting to the depth limit in svg closed by matching end tags': () =>
    fill('<svg>', nest('<g>', '</g>', DEPTH - 1), '</svg>'),
  'one crossing of the depth limit, then unmatched end tags': () =>
    fill(`<div>${'<span>'.repeat(DEPTH - 1)}</div>`, '</span></div>'),
  'unmatched end tags on a stack at the depth limit': () => fill('<div>'.repeat(DEPTH), '</x>'),
  // M-26 item 3: implied parts count once for each table.
  'table cells without rows, at the depth limit': () =>
    fill('<div>'.repeat(DEPTH - 4) + '<table>', '<td>x</td><th>y</th>', '</table>'),
  'tables with implied parts, popped exactly': () => fill('', '<table><td>x</td><col></table>'),
  // M-11 item 5: one noscript content, nested to 5 MiB and closed again.
  'noscript content nested to 5 MiB and balanced': () =>
    nested('<em>', '</em>', '<noscript>', '</noscript>'),
  'noscript content nested to 5 MiB, unbalanced at its end': () =>
    fill('<noscript>', '<em>', '</noscript>'),
  // Rules of security review round 2 (H-35 to H-38).
  'forms one after another': () => fill('', '<form><div></div></form>'),
  'table parts after foster-parented elements that are closed': () =>
    fill('<table>', '<div></div><tr><td>x</td></tr>', '</table>'),
  'cols in a table in a template': () => fill('<template><table>', '<colgroup><col></colgroup><col>', '</table></template>'),
  'noscript content with tables nested to 5 MiB and balanced': () =>
    nested('<table><tr><td>', '</td></tr></table>', '<noscript>', '</noscript>'),
  'noscript content with table parts after its tables are closed': () =>
    fill('<noscript>', '<table></table>', '<tr></tr></noscript>'),
};

// The expected result of each input. A valid input must give its block count:
// if it became invalid, the pass would end early and the test would not
// measure the stack work.
const LIMIT = { error: ErrorCode.LIMIT };
const VALID = { error: null, blocks: 0 };
const STRUCTURE = { error: ErrorCode.STRUCTURE };
const expected = new Map([
  ['one tag with distinct attributes', VALID],
  ['one tag with repeated data-rq-block attributes', STRUCTURE], // no block end tag (M-20)
  ['unmatched end tags', VALID],
  ['a comment with many dashes and bangs', VALID],
  ['script data with escape openers', VALID],
  ['RCDATA with near end tags', VALID],
  ['many noscript elements', VALID],
  ['JSON blocks of 1 MiB', { error: null, blocks: 4 }],
  ['html blocks of 1 MiB', { error: null, blocks: 4 }],
  ['svg with many elements', VALID],
  ['table rows with cells', VALID],
  ['many CDATA sections', VALID],
  ['deep nesting', LIMIT],
  ['deep nesting of crossable elements, then one end tag', LIMIT],
  ['deep nesting closed by matching end tags, beyond the depth limit', LIMIT],
  ['mixed names closed by matching end tags, beyond the depth limit', LIMIT],
  ['nested lists in block content, beyond the depth limit', LIMIT],
  ['nested elements in block content, beyond the depth limit', LIMIT],
  ['deep nesting in svg, beyond the depth limit', LIMIT],
  ['unmatched end tags on a stack beyond the depth limit', LIMIT],
  ['nesting to the depth limit closed by matching end tags', { error: null, blocks: 0 }],
  ['mixed names to the depth limit closed by matching end tags', { error: null, blocks: 0 }],
  ['nested lists to the depth limit in block content', { error: null, blocks: 5 }],
  ['nested elements to the depth limit in block content', { error: null, blocks: 5 }],
  ['nesting to the depth limit in svg closed by matching end tags', { error: null, blocks: 0 }],
  ['one crossing of the depth limit, then unmatched end tags', VALID],
  ['unmatched end tags on a stack at the depth limit', VALID],
  ['table cells without rows, at the depth limit', VALID],
  ['tables with implied parts, popped exactly', VALID],
  ['noscript content nested to 5 MiB and balanced', VALID],
  ['noscript content nested to 5 MiB, unbalanced at its end', STRUCTURE],
  ['forms one after another', VALID],
  ['table parts after foster-parented elements that are closed', VALID],
  ['cols in a table in a template', VALID],
  ['noscript content with tables nested to 5 MiB and balanced', VALID],
  ['noscript content with table parts after its tables are closed', STRUCTURE], // M-11 item 6
]);

describe('[perf] linear time at 5 MiB', () => {
  for (const [name, make] of Object.entries(inputs)) {
    it(`[perf] ${name}`, (t) => {
      const input = make();
      assert.ok(input.length <= MAX && input.length > 4_000_000, `${input.length} bytes`);
      const t0 = performance.now();
      const result = analyze(input);
      const ms = performance.now() - t0;
      t.diagnostic(`${Math.round(ms)} ms, ${result.valid ? 'valid' : result.error}`);
      assert.ok(ms < LIMIT_MS, `${Math.round(ms)} ms`);
      const want = expected.get(name);
      assert.ok(want !== undefined, 'every input has an expected result');
      assert.equal(result.error, want.error);
      if (want.blocks !== undefined) assert.equal(result.blocks.length, want.blocks);
    });
  }
});
