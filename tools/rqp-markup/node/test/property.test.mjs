// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// Property tests with a fixed seed. Deeper runs:
//   RQP_PROP_ITERATIONS=200000 RQP_PROP_SEED=9 npm test

import { describe, it } from 'node:test';
import assert from 'node:assert/strict';
import { analyze } from '../src/markup.mjs';
import { checkResult, rng } from './helpers.mjs';

const ITERATIONS = Number(process.env.RQP_PROP_ITERATIONS ?? 3000);
const SEED = Number(process.env.RQP_PROP_SEED ?? 0xc0ffee);

const pieces = [
  '<p data-rq-block="a">', '<div data-rq-block="b">', '<span data-rq-block="c">', '</p>', '</div>',
  '</span>', '<title data-rq-block="t">', '</title>', '<script type="application/json" data-rq-block="j">',
  '</script>', '<script>', '{"k":[1,2]}', '<!--', '-->', '<svg>', '</svg>', '<math>', '<mi>', '</mi>',
  '<table>', '<tr>', '<td>', '</td>', '</tr>', '</table>', '<select>', '<option>', '</select>',
  '<noscript>', '</noscript>', '<template>', '</template>', '<br>', '<img/>', '<b>', '</b>', '<a>',
  '</a>', '<li>', '<ul>', '</ul>', '<![CDATA[', ']]>', '<!DOCTYPE html>', '<?x?>', '<', '>', '"', "'",
  '=', ' ', '\r\n', 'text', 'é', '\u{1F600}', '\0', '&amp;', '﻿', '<h1>', '</h1>',
  '<ruby>', '<rt>', '</rt>', '</ruby>', '<button>', '</button>', '<style>', '</style>', '<frameset>',
];

function generatePieces(r) {
  const n = r.int(60);
  let s = '';
  for (let i = 0; i < n; i++) s += r.pick(pieces);
  return Buffer.from(s, 'utf8');
}

function generateBytes(r) {
  const n = r.int(200);
  const b = Buffer.alloc(n);
  for (let i = 0; i < n; i++) b[i] = r.next() < 0.5 ? r.pick([0x3c, 0x3e, 0x2f, 0x21, 0x2d, 0x22, 0x3d, 0x20]) : r.int(256);
  return b;
}

describe('properties', () => {
  it('never throws and keeps the result invariants on generated markup', (t) => {
    const r = rng(SEED);
    let withBlocks = 0;
    for (let i = 0; i < ITERATIONS; i++) {
      const input = generatePieces(r);
      const result = analyze(input);
      checkResult(input, result);
      if (result.blocks.length > 0) withBlocks++;
    }
    t.diagnostic(`${withBlocks} of ${ITERATIONS} inputs are valid with blocks`);
    assert.ok(withBlocks > 0);
  });

  it('never throws and keeps the result invariants on random bytes', () => {
    const r = rng(SEED + 1);
    for (let i = 0; i < ITERATIONS; i++) {
      const input = generateBytes(r);
      checkResult(input, analyze(input));
    }
  });

  it('gives the same result for the same input', () => {
    const r = rng(SEED + 2);
    for (let i = 0; i < 200; i++) {
      const input = generatePieces(r);
      assert.deepEqual(analyze(input), analyze(Buffer.from(input)));
    }
  });
});
