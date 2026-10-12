// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors
//
// The JSON rules of manifest-patch.md section 3.2 and the small schema
// validator against the examples of spec/rqp/examples (the expected keyword
// and instance path of each invalid example are those that Ajv 8.20.0 reports).

import assert from 'node:assert/strict';
import { readdirSync } from 'node:fs';
import { join } from 'node:path';
import { test } from 'node:test';
import { parseStrict } from '../src/json-strict.mjs';
import { SchemaError, loadSchema, validate } from '../src/schema.mjs';
import { EXAMPLES_DIR, readJson } from './helpers.mjs';

const p = (text) => parseStrict(Buffer.from(text, 'utf8'));

test('strict JSON: valid texts give the same value as JSON.parse', () => {
  for (const t of ['{}', '[]', ' {"a":[1,true,false,null,"x\\u00e9\\n"]} ', '"\\ud83d\\ude00"', '0', '{"__proto__":1}']) {
    const r = p(t);
    assert.equal(r.ok, true, t);
    assert.deepEqual(r.value, JSON.parse(t));
  }
});

test('strict JSON: each rule of section 3.2 rejects', () => {
  const cases = [
    ['{"a":1,"a":1}', 'duplicate'],
    ['{"a":{"b":1,"b":2}}', 'duplicate'],
    ['"\\ud800"', 'surrogate'],
    ['"\\udc00"', 'surrogate'],
    ['"\\ud800\\u0041"', 'surrogate'],
    ['60.0', 'integer'],
    ['6e1', 'integer'],
    ['-1', 'integer'],
    ['{"a":', 'syntax'],
    ['{"a":1,}', 'syntax'],
    ['"a\u0001"', 'syntax'],
    ['01', 'syntax'],
    ['"\\x"', 'syntax'],
    ['tru', 'syntax'],
    ['{} {}', 'syntax'],
  ];
  for (const [t, rule] of cases) assert.equal(p(t).rule, rule, t);
  assert.equal(parseStrict(Buffer.from([0xef, 0xbb, 0xbf, 0x7b, 0x7d])).rule, 'bom');
  assert.equal(parseStrict(Buffer.from([0x22, 0xc3, 0x28, 0x22])).rule, 'encoding');
});

test('strict JSON: the nesting depth limit is 16', () => {
  assert.equal(p('['.repeat(16) + ']'.repeat(16)).ok, true);
  assert.equal(p('['.repeat(17) + ']'.repeat(17)).rule, 'depth');
  assert.equal(p('{"a":'.repeat(17) + '1' + '}'.repeat(17)).rule, 'depth');
  assert.equal(p('['.repeat(100000)).rule, 'depth');
});

for (const kind of ['manifest', 'patch']) {
  const schema = loadSchema(kind);
  const dir = (validity) => join(EXAMPLES_DIR, validity, kind);
  test(`schema ${kind}: every valid example passes`, () => {
    for (const f of readdirSync(dir('valid'))) assert.deepEqual(validate(schema, readJson(join(dir('valid'), f))), [], f);
  });
  test(`schema ${kind}: every invalid example fails with the keyword and path of the example`, () => {
    for (const f of readdirSync(dir('invalid'))) {
      const { expect, instance } = readJson(join(dir('invalid'), f));
      const errors = validate(schema, instance);
      assert.ok(errors.some((e) => e.keyword === expect.keyword && e.instancePath === expect.instancePath), `${f}: ${JSON.stringify(errors)}`);
    }
  });
}

test('schema: code points, not UTF-16 units, count for maxLength', () => {
  const s = { type: 'string', maxLength: 1 };
  assert.deepEqual(validate(s, '\u{1F600}'), []);
  assert.deepEqual(validate(s, 'ab'), [{ keyword: 'maxLength', instancePath: '' }]);
});

test('schema: an unknown keyword is an error, as in Ajv strict mode', () => {
  assert.throws(() => validate({ type: 'string', format: 'uri' }, 'x'), SchemaError);
  assert.throws(() => validate({ additionalProperties: true }, {}), SchemaError);
  assert.throws(() => validate({ $ref: '#/other' }, {}), SchemaError);
});
