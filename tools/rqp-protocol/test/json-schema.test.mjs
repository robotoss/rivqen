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

test('strict JSON: white space, escapes, surrogate pairs and digits as in JSON.parse', () => {
  const texts = [
    '{\t"a"\r:\r\n[9, 19]}',
    '{ }',
    '[ ]',
    '{"a" :1}',
    '"\\\\\\/\\b\\f\\n\\r\\t\\""',
    '"\\ue000"',
    '"\\ud800\\udc00"',
    '"\\udbff\\udfff"',
  ];
  for (const t of texts) {
    const r = p(t);
    assert.equal(r.ok, true, t);
    assert.deepEqual(r.value, JSON.parse(t), t);
  }
  // Members are plain data properties, as in JSON.parse.
  assert.deepEqual(Object.getOwnPropertyDescriptor(p('{"a":1}').value, 'a'), Object.getOwnPropertyDescriptor(JSON.parse('{"a":1}'), 'a'));
});

test('strict JSON: the rule of each failure', () => {
  const cases = [
    ['"\\ud800\\n"', 'surrogate'],
    ['"\\ud800xudc00"', 'surrogate'],
    ['"\\ud800\\ndc00"', 'surrogate'],
    ['"\\ud800\\ue000"', 'surrogate'],
    ['"\\udfff"', 'surrogate'],
    ['"\\u12g4"', 'syntax'],
    ['"abc', 'syntax'],
    ['-', 'syntax'],
    ['[-,1]', 'syntax'],
    ['{"a"x1}', 'syntax'],
    ['[1x', 'syntax'],
    ['{x":1}', 'syntax'],
    ['1e+5', 'integer'],
  ];
  for (const [t, rule] of cases) assert.deepEqual(p(t), { ok: false, rule }, t);
  // Only EF BB BF at offset 0 is a byte order mark; other valid UTF-8 there is a syntax error.
  assert.deepEqual(parseStrict(Buffer.from([0xef, 0xbb, 0xbf])), { ok: false, rule: 'bom' });
  for (const b of [[0xe0, 0xbb, 0xbf], [0xef, 0x80, 0xbf], [0xef, 0xbb, 0x80]]) {
    assert.deepEqual(parseStrict(Buffer.from(b)), { ok: false, rule: 'syntax' }, JSON.stringify(b));
  }
  assert.deepEqual(parseStrict(Buffer.from([0x22, 0xc3, 0x28, 0x22])), { ok: false, rule: 'encoding' });
});

test('strict JSON: 16 nested objects are allowed', () => {
  assert.equal(p('{"a":'.repeat(15) + '{}' + '}'.repeat(15)).ok, true);
  assert.equal(p('{"a":'.repeat(16) + '{}' + '}'.repeat(16)).rule, 'depth');
});

test('schema: each type keyword', () => {
  const ok = [['object', {}], ['array', []], ['string', ''], ['integer', 1], ['number', 1.5], ['boolean', false], ['null', null]];
  for (const [type, v] of ok) assert.deepEqual(validate({ type }, v), [], type);
  const bad = [['object', null], ['object', 'x'], ['object', []], ['number', 'x'], ['null', 0], ['integer', 1.5]];
  for (const [type, v] of bad) assert.deepEqual(validate({ type }, v), [{ keyword: 'type', instancePath: '' }], `${type} ${JSON.stringify(v)}`);
  assert.throws(() => validate({ type: 'decimal' }, 1), SchemaError);
});

test('schema: code points at the surrogate bounds; a lone surrogate counts as one', () => {
  const max1 = { type: 'string', maxLength: 1 };
  for (const s of ['\u{10000}', '\u{10FFFF}', '\ud800', '\udc00']) assert.deepEqual(validate(max1, s), [], JSON.stringify(s));
  const min2 = { type: 'string', minLength: 2 };
  for (const s of ['a\udc00', '\ud800a', '\ud800\ue000', '\ue000\udc00']) assert.deepEqual(validate(min2, s), [], JSON.stringify(s));
  assert.deepEqual(validate(min2, 'a'), [{ keyword: 'minLength', instancePath: '' }]);
  // pattern uses the "u" flag: "." is one code point.
  assert.deepEqual(validate({ type: 'string', pattern: '^.$' }, '\u{1F600}'), []);
});

test('schema: string, number, array and object keywords apply only to their type', () => {
  const s = { minLength: 1, maxLength: 0, pattern: '^x$', minimum: 10, maximum: 0, minItems: 9, maxItems: 1, items: { type: 'string' }, required: ['a'], additionalProperties: false };
  assert.deepEqual(validate(s, 5).map((e) => e.keyword), ['minimum', 'maximum']);
  assert.deepEqual(validate(s, 'ab').map((e) => e.keyword), ['maxLength', 'pattern']);
  assert.deepEqual(validate(s, true), []);
  assert.deepEqual(validate(s, null), []);
  assert.deepEqual(validate({ type: 'array' }, [1]), []);
  assert.deepEqual(validate({ type: 'array', maxItems: 1 }, [1]), []);
  assert.deepEqual(validate({ type: 'array', maxItems: 1 }, [1, 2]), [{ keyword: 'maxItems', instancePath: '' }]);
});

test('schema: the instance path escapes "~" and "/" of member names (RFC 6901)', () => {
  const s = { type: 'object', properties: { 'a/b': { type: 'string' }, 'c~d': { type: 'string' } } };
  assert.deepEqual(validate(s, { 'a/b': 1, 'c~d': 2 }), [
    { keyword: 'type', instancePath: '/a~1b' },
    { keyword: 'type', instancePath: '/c~0d' },
  ]);
});

test('schema: only "#/$defs/<name>" of the root is a supported $ref', () => {
  const defs = { $defs: { a: { type: 'string' } } };
  assert.deepEqual(validate({ ...defs, $ref: '#/$defs/a' }, 'x'), []);
  for (const ref of ['x#/$defs/a', '#/$defs/a/b', '#/$defs/missing']) {
    assert.throws(() => validate({ ...defs, $ref: ref }, 'x'), SchemaError, ref);
  }
  assert.throws(() => validate({ $ref: '#/$defs/a' }, 'x'), SchemaError);
  assert.throws(
    () => validate({ type: 'string', format: 'uri' }, 'x'),
    (e) => e instanceof SchemaError && e.code === 'RQP_SCHEMA_UNSUPPORTED' && e.name === 'SchemaError',
  );
});
