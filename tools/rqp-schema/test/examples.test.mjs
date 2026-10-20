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

// Boundary values are generated here to keep the example files small.

const shop = () => readJson(exampleFiles('valid', 'manifest').find((f) => f.name === 'shop.json').path);
const price = () => readJson(exampleFiles('valid', 'patch').find((f) => f.name === 'shop-price.json').path);

test('a patch with 256 operations passes and one with 257 fails with maxItems', () => {
  const op = (i) => ({ block_id: `b${i}`, content: '' });
  assert.equal(errorsOf('patch', { ...price(), operations: Array.from({ length: 256 }, (_, i) => op(i)) }), null);
  const errors = errorsOf('patch', { ...price(), operations: Array.from({ length: 257 }, (_, i) => op(i)) });
  assert.deepEqual(errors.map((e) => [e.keyword, e.instancePath]), [['maxItems', '/operations']]);
});

test('content of 1 048 576 code points passes and 1 048 577 fails with maxLength', () => {
  const at = (n) => {
    const p = price();
    p.operations[0].content = 'x'.repeat(n);
    return p;
  };
  assert.equal(errorsOf('patch', at(1048576)), null);
  const errors = errorsOf('patch', at(1048577));
  assert.ok(errors.some((e) => e.keyword === 'maxLength' && e.instancePath === '/operations/0/content'));
});

test('an empty block content has the hash 47DEQpj8HBSa-_TImW-5JCeuQeRkm5NMpJWZG3hSuFU', () => {
  assert.equal(contentHash(''), '47DEQpj8HBSa-_TImW-5JCeuQeRkm5NMpJWZG3hSuFU');
});

test('an operation has no op member (H-104): replace_block, remove_block and insert_block fail at /operations/0', () => {
  const p = price();
  assert.deepEqual(readJson(schemaPath('patch')).properties.operations.items, { $ref: '#/$defs/operation' });
  for (const op of ['replace_block', 'remove_block', 'insert_block']) {
    const env = { ...p, operations: [{ op, ...p.operations[0] }] };
    assert.deepEqual(errorsOf('patch', env).map((e) => [e.keyword, e.instancePath]), [['additionalProperties', '/operations/0']], op);
  }
});

test('an operation with empty content passes the schema (the result of the withdrawn remove_block)', () => {
  const p = price();
  p.operations[0] = { ...p.operations[0], content: '' };
  assert.equal(errorsOf('patch', p), null);
});

test('H-103, H-104: every member of the documents before H-103 and H-104 is an unknown member, also with a valid value', () => {
  const m = shop();
  const t = 't1.86FtOeBe__KDLc87s9j68x63RC6DK9AYpEkvnbgQ_LM';
  for (const extra of [{ template_revision: t }, { blocks: [] }]) {
    assert.deepEqual(errorsOf('manifest', { ...m, ...extra }).map((e) => [e.keyword, e.instancePath]), [['additionalProperties', '']], Object.keys(extra)[0]);
  }
  const p = price();
  // H-107: also next_revision, the member of H-104 that the binding replaces.
  for (const extra of [{ type: 'rivqen.patch' }, { protocol: 'rqp/1' }, { base_revision: m.page_revision }, { template_revision: t }, { next_revision: m.page_revision }]) {
    assert.deepEqual(errorsOf('patch', { ...p, ...extra }).map((e) => [e.keyword, e.instancePath]), [['additionalProperties', '']], Object.keys(extra)[0]);
  }
  for (const extra of [{ op: 'replace_block' }, { format: 'html' }, { sha256: contentHash('130.00') }]) {
    const env = { ...p, operations: [{ ...p.operations[0], ...extra }] };
    assert.deepEqual(errorsOf('patch', env).map((e) => [e.keyword, e.instancePath]), [['additionalProperties', '/operations/0']], Object.keys(extra)[0]);
  }
});

test('each of the 16 canonical last characters of a revision and of a binding passes and the 48 others fail', () => {
  const alphabet = 'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_';
  for (let i = 0; i < 64; i += 1) {
    const m = { ...shop(), page_revision: 'r1.' + 'A'.repeat(42) + alphabet[i] };
    const p = { ...price(), binding: 'd1.' + 'A'.repeat(42) + alphabet[i] };
    assert.equal(errorsOf('manifest', m) === null, i % 4 === 0, `manifest, last character ${alphabet[i]}`);
    assert.equal(errorsOf('patch', p) === null, i % 4 === 0, `patch, last character ${alphabet[i]}`);
  }
});

test('H-107: the binding has the prefix d1. and 43 characters; a page revision, another prefix or length fails', () => {
  const p = price();
  const tail = p.binding.slice(3);
  assert.equal(errorsOf('patch', p), null);
  for (const value of [`r1.${tail}`, `t1.${tail}`, `d2.${tail}`, `D1.${tail}`, `d1${tail}`, `d1.${tail}=`, `d1.${tail.slice(1)}`, `d1.${tail}A`, `d1.${tail.replace(/^./, '+')}`]) {
    assert.deepEqual(errorsOf('patch', { ...p, binding: value }).map((e) => [e.keyword, e.instancePath]), [['pattern', '/binding']], value);
  }
  // The envelope of H-104 (next_revision in place of the binding) fails twice: required binding and an unknown member.
  const { binding, ...rest } = p;
  const h104 = errorsOf('patch', { next_revision: shop().page_revision, ...rest }).map((e) => [e.keyword, e.instancePath]);
  assert.deepEqual(h104.sort(), [['additionalProperties', ''], ['required', '']]);
  assert.equal(typeof binding, 'string');
});

test('the JSON Schema does not see duplicate member names: the client parser must reject them', () => {
  // JSON.parse keeps the last value; the schema then validates a different object than other parsers see.
  const p = price();
  const other = 'd1.' + 'A'.repeat(43);
  const text = JSON.stringify(p).replace('"binding"', `"binding":"${other}","binding"`);
  assert.equal(errorsOf('patch', JSON.parse(text)), null);
  assert.equal(JSON.parse(text).binding, p.binding);
});
