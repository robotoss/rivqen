// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// M-25: JSON text (RFC 8259 section 2), depth limit 64, checked on the byte
// range [start, end) of a larger buffer.

import { describe, it } from 'node:test';
import assert from 'node:assert/strict';
import { MAX_DEPTH, isJsonText } from '../src/json.mjs';

// Bytes around the range that would make a wrong check pass or fail: a parser
// that reads outside [start, end) gives another answer for some of them.
const SURROUNDINGS = [['', ''], ['[', ']'], ['"', '0e+5]}"'], ['{', '}'], ['1', '1'], ['', 'ue']];

function check(text) {
  const results = new Set();
  for (const [before, after] of SURROUNDINGS) {
    const b = Buffer.from(before + text + after, 'utf8');
    const start = Buffer.byteLength(before);
    results.add(isJsonText(b, start, start + Buffer.byteLength(text)));
  }
  assert.equal(results.size, 1, `the answer depends on bytes outside the range: ${JSON.stringify(text)}`);
  return [...results][0];
}

const VALID = [
  '0', '-0', '1', '-12', '10', '1.5', '-0.25', '1e5', '1E5', '1e+5', '1e-5', '1.5E-10', '1e400', '9'.repeat(400),
  'true', 'false', 'null', '""', '"a"', '"\\"\\\\\\/\\b\\f\\n\\r\\t"', '"\\u0041\\u00e9\\uFFFF"',
  '"\\ud83d\\ude00"', '"\\uD83D\\uDE00"', '"é\u{1F600}"', '"\u007f"', '[]', '{}', '[1,"a",true,null,{}]',
  '{"a":1,"a":2}', '{"a":{"b":[[]]}}', ' \t\r\n[ 1 , 2 ] \t\r\n', '\r\n{\r\n"k" : "v"\r\n}\r\n',
  '[-1,0.5,1e1,"x"]', '{"":0}',
];

const INVALID = [
  '', ' ', ' {}', '﻿{}', '+1', '01', '-', '-a', '1.', '.5', '1.e5', '1e', '1e+', '1E-', '0x1', 'NaN',
  'Infinity', '- 1', 'tru', 'true1', 'nul', 'fals', 'TRUE', 'True', 'nulL', '"', '"a', "'a'", '"\u0001"',
  '"\t"', '"\\x"', '"\\u12"', '"\\u12G4"', '"\\ud800"', '"\\ud800a"', '"\\ud800\\u0041"', '"\\ud800\\ud800"',
  '"\\udc00"', '"\\ud800\\', '"\\ud800\\u"', '"\\', '[', ']', '[1,]', '[,1]', '[1 2]', '[1;2]', '{', '}',
  '{"a"}', '{"a":}', '{"a" 1}', '{,}', '{"a":1,}', '{a:1}', '{"a":1 "b":2}', '{1:2}', '[1]]', '{}}', '[] []',
  '1 2', '[1}', '{"a":1]', '"a"b', 'nullnull', '[-]', '[1e]', '[0.]',
];

describe('M-25 JSON text', () => {
  for (const text of VALID) {
    it(`accepts ${JSON.stringify(text).slice(0, 40)}`, () => {
      assert.equal(check(text), true);
    });
  }

  for (const text of INVALID) {
    it(`rejects ${JSON.stringify(text).slice(0, 40)}`, () => {
      assert.equal(check(text), false);
    });
  }

  it(`accepts the nesting depth ${MAX_DEPTH} and rejects ${MAX_DEPTH + 1}`, () => {
    assert.equal(MAX_DEPTH, 64);
    assert.equal(check(`${'['.repeat(64)}${']'.repeat(64)}`), true);
    assert.equal(check(`${'[{"a":'.repeat(32)}1${'}]'.repeat(32)}`), true);
    assert.equal(check(`${'['.repeat(65)}${']'.repeat(65)}`), false);
    assert.equal(check(`${'{"a":'.repeat(65)}1${'}'.repeat(65)}`), false);
  });

  it('accepts depth 64 again after the containers close', () => {
    const deep = `${'['.repeat(64)}${']'.repeat(64)}`;
    assert.equal(check(`[${'['.repeat(63)}${']'.repeat(63)},${'['.repeat(63)}${']'.repeat(63)}]`), true);
    assert.equal(check(`${deep}`), true);
  });
});
