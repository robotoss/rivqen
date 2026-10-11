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
  // Rules of the security review (markup.md section 2.12).
  '<div>', '<span>', '<i>', '</i>', '<nobr>', '</nobr>', '<q>', '</q>', '</li>', '<tbody>', '</tbody>',
  '<th>', '<col>', '<body>', '</body>', '</thead>', '<noscript><b>', '</b></noscript>',
  '<script type="rıvqen">', '<span><div></span>', '<div><div><div><div><div><div><div><div>',
];

/**
 * Elements of many kinds, all closed exactly. After it the token stack is
 * empty again, D is 0 (M-26), and no mark or name count is left.
 */
const CLOSED_PREFIX = '<b><i><select><option></option></select><template></template>'
  + '<table><td></td><col></table><ruby><rt></rt></ruby><button></button><a></a><nobr></nobr>'
  + '<svg><g></g></svg><noscript><p>x</p></noscript><ul><li></li></ul><h1></h1></i></b>';

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

  it('gives the same result after a prefix whose elements are all closed exactly', (t) => {
    const prefix = Buffer.from(CLOSED_PREFIX.repeat(20), 'utf8');
    const r = rng(SEED + 3);
    let valid = 0;
    for (let i = 0; i < ITERATIONS; i++) {
      const input = generatePieces(r);
      const alone = analyze(input);
      const after = analyze(Buffer.concat([prefix, input]));
      assert.equal(after.error, alone.error, JSON.stringify(input.toString('utf8')));
      if (!alone.valid) continue;
      valid++;
      const shift = (b) => [b.id, b.start + prefix.length, b.end + prefix.length, b.sha256];
      assert.deepEqual(after.blocks.map((b) => [b.id, b.start, b.end, b.sha256]), alone.blocks.map(shift));
    }
    t.diagnostic(`${valid} of ${ITERATIONS} inputs are valid`);
    assert.ok(valid > ITERATIONS / 10);
  });

  it('gives the same result for the same input', () => {
    const r = rng(SEED + 2);
    for (let i = 0; i < 200; i++) {
      const input = generatePieces(r);
      assert.deepEqual(analyze(input), analyze(Buffer.from(input)));
    }
  });
});
