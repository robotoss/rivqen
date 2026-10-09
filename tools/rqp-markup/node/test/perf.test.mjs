// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// [perf] Linear time on adversarial inputs of the maximum size (5 MiB, M-01).
// A quadratic step on any of these inputs takes minutes, not seconds. The time
// limit is generous so that a slow machine does not fail the test.

import { describe, it } from 'node:test';
import assert from 'node:assert/strict';
import { LIMITS, analyze } from '../src/markup.mjs';

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
};

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
    });
  }
});
