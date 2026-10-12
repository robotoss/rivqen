// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors
//
// Example instances in spec/rqp/examples/{valid,invalid}/{manifest,patch}/ and boundary values.

import assert from 'node:assert/strict';
import { test } from 'node:test';
import { KINDS, contentHash, exampleFiles, readJson, schemaPath, validators } from './helpers.mjs';

const v = validators();

function errorsOf(kind, instance) {
  const ok = v[kind](instance);
  return ok ? null : v[kind].errors;
}

for (const kind of KINDS) {
  for (const { name, path } of exampleFiles('valid', kind)) {
    test(`valid ${kind} example ${name} passes the schema`, () => {
      assert.equal(errorsOf(kind, readJson(path)), null);
    });
  }

  for (const { name, path } of exampleFiles('invalid', kind)) {
    test(`invalid ${kind} example ${name} fails with ${readJson(path).expect.keyword} at "${readJson(path).expect.instancePath}"`, () => {
      const { description, expect, instance } = readJson(path);
      assert.equal(typeof description, 'string');
      const errors = errorsOf(kind, instance);
      assert.notEqual(errors, null, 'the instance must be invalid');
      const hit = errors.some((e) => e.keyword === expect.keyword && e.instancePath === expect.instancePath);
      assert.ok(hit, `expected ${expect.keyword} at "${expect.instancePath}", got ${JSON.stringify(errors.map((e) => [e.keyword, e.instancePath]))}`);
    });
  }
}

test('every invalid example has exactly one defect: no error outside the expected path', () => {
  // An error at a parent path of the defect (for example `required` in the operation of a
  // wrong `op`) or below it is allowed. Any other error means a second defect.
  for (const kind of KINDS) {
    for (const { name, path } of exampleFiles('invalid', kind)) {
      const { expect, instance } = readJson(path);
      const target = expect.instancePath;
      const errors = errorsOf(kind, instance);
      for (const e of errors) {
        const related = target.startsWith(e.instancePath) || e.instancePath.startsWith(target);
        assert.ok(related, `${kind}/${name}: unrelated error ${e.keyword} at ${e.instancePath}`);
      }
    }
  }
});

test('valid replace_block examples carry the SHA-256 of the UTF-8 bytes of the decoded content string', () => {
  for (const { name, path } of exampleFiles('valid', 'patch')) {
    for (const op of readJson(path).operations) {
      if (op.op === 'replace_block') assert.equal(op.sha256, contentHash(op.content), `${name} ${op.block_id}`);
    }
  }
});

// Boundary values are generated here to keep the example files small.

const shop = () => readJson(exampleFiles('valid', 'manifest').find((f) => f.name === 'shop.json').path);
const price = () => readJson(exampleFiles('valid', 'patch').find((f) => f.name === 'shop-price.json').path);
const block = (i) => ({ id: `b${i}`, format: 'html', sha256: contentHash(String(i)) });

test('a manifest with 256 blocks passes', () => {
  const m = { ...shop(), blocks: Array.from({ length: 256 }, (_, i) => block(i)) };
  assert.equal(errorsOf('manifest', m), null);
});

test('a manifest with 257 blocks fails with maxItems', () => {
  const m = { ...shop(), blocks: Array.from({ length: 257 }, (_, i) => block(i)) };
  assert.deepEqual(errorsOf('manifest', m).map((e) => [e.keyword, e.instancePath]), [['maxItems', '/blocks']]);
});

test('a patch with 256 operations passes and one with 257 fails with maxItems', () => {
  const op = (i) => ({ op: 'replace_block', block_id: `b${i}`, format: 'html', content: '', sha256: contentHash('') });
  assert.equal(errorsOf('patch', { ...price(), operations: Array.from({ length: 256 }, (_, i) => op(i)) }), null);
  const errors = errorsOf('patch', { ...price(), operations: Array.from({ length: 257 }, (_, i) => op(i)) });
  assert.deepEqual(errors.map((e) => [e.keyword, e.instancePath]), [['maxItems', '/operations']]);
});

test('content of 1 048 576 code points passes and 1 048 577 fails with maxLength', () => {
  const at = (n) => {
    const p = price();
    p.operations[0].content = 'x'.repeat(n);
    p.operations[0].sha256 = contentHash(p.operations[0].content);
    return p;
  };
  assert.equal(errorsOf('patch', at(1048576)), null);
  const errors = errorsOf('patch', at(1048577));
  assert.ok(errors.some((e) => e.keyword === 'maxLength' && e.instancePath === '/operations/0/content'));
});

test('an empty block content has the hash 47DEQpj8HBSa-_TImW-5JCeuQeRkm5NMpJWZG3hSuFU', () => {
  assert.equal(contentHash(''), '47DEQpj8HBSa-_TImW-5JCeuQeRkm5NMpJWZG3hSuFU');
});

test('replace_block is the only operation (H-47): remove_block and insert_block fail at /operations/0/op', () => {
  const p = price();
  assert.deepEqual(readJson(schemaPath('patch')).properties.operations.items, { $ref: '#/$defs/replace_block' });
  for (const op of ['remove_block', 'insert_block']) {
    const env = { ...p, operations: [{ ...p.operations[0], op }] };
    assert.deepEqual(errorsOf('patch', env).map((e) => [e.keyword, e.instancePath]), [['const', '/operations/0/op']], op);
  }
});

test('replace_block with empty content passes the schema (the result of the withdrawn remove_block)', () => {
  const p = price();
  p.operations[0] = { ...p.operations[0], content: '', sha256: contentHash('') };
  assert.equal(errorsOf('patch', p), null);
});

test('each of the 16 canonical last characters passes and the 48 others fail', () => {
  const alphabet = 'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_';
  for (let i = 0; i < 64; i += 1) {
    const m = shop();
    m.blocks[0].sha256 = 'A'.repeat(42) + alphabet[i];
    const ok = errorsOf('manifest', m) === null;
    assert.equal(ok, i % 4 === 0, `last character ${alphabet[i]}`);
  }
});

test('the JSON Schema does not see duplicate member names: the client parser must reject them', () => {
  // JSON.parse keeps the last value; the schema then validates a different object than other parsers see.
  const text = JSON.stringify(price()).replace('"type":"rivqen.patch"', '"type":"x","type":"rivqen.patch"');
  assert.equal(errorsOf('patch', JSON.parse(text)), null);
});
