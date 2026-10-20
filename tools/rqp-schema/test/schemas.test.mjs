// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors
//
// The schema files themselves: Draft 2020-12, strict, stable ids, shared definitions in sync.

import assert from 'node:assert/strict';
import { test } from 'node:test';
import { KINDS, newAjv, readJson, schemaPath } from './helpers.mjs';

const DRAFT = 'https://json-schema.org/draft/2020-12/schema';
// Definitions that both schemas carry. They must be identical (one rule, two copies).
// Since H-103 and H-104 only the page revision is in both documents.
const SHARED_DEFS = ['page_revision'];

for (const kind of KINDS) {
  test(`${kind} schema is valid against the Draft 2020-12 meta-schema`, () => {
    const ajv = newAjv();
    const schema = readJson(schemaPath(kind));
    assert.equal(schema.$schema, DRAFT);
    assert.equal(ajv.validateSchema(schema), true, JSON.stringify(ajv.errors));
  });

  test(`${kind} schema compiles in strict mode`, () => {
    assert.doesNotThrow(() => newAjv().compile(readJson(schemaPath(kind))));
  });

  test(`${kind} schema has the URN id urn:rivqen:rqp:1:${kind}`, () => {
    assert.equal(readJson(schemaPath(kind)).$id, `urn:rivqen:rqp:1:${kind}`);
  });
}

test('manifest and patch schemas share an identical definition of the page revision', () => {
  const m = readJson(schemaPath('manifest')).$defs;
  const p = readJson(schemaPath('patch')).$defs;
  for (const name of SHARED_DEFS) assert.deepEqual(p[name], m[name], name);
});

test('the block id pattern is the markup rule M-15 pattern', () => {
  assert.equal(readJson(schemaPath('patch')).$defs.block_id.pattern, '^[a-z0-9][a-z0-9_-]{0,63}$');
});

test('H-103: the manifest has exactly protocol, origin, path, page_revision and the optional policy and limits', () => {
  const m = readJson(schemaPath('manifest'));
  assert.deepEqual(Object.keys(m.properties), ['protocol', 'origin', 'path', 'page_revision', 'policy', 'limits']);
  assert.deepEqual(m.required, ['protocol', 'origin', 'path', 'page_revision']);
  assert.equal(m.additionalProperties, false);
});

test('H-104: the envelope has exactly next_revision and operations; an operation exactly block_id and content', () => {
  const p = readJson(schemaPath('patch'));
  assert.deepEqual(Object.keys(p.properties), ['next_revision', 'operations']);
  assert.deepEqual(p.required, ['next_revision', 'operations']);
  assert.deepEqual(Object.keys(p.$defs.operation.properties), ['block_id', 'content']);
  assert.deepEqual(p.$defs.operation.required, ['block_id', 'content']);
  assert.equal(p.additionalProperties, false);
  assert.equal(p.$defs.operation.additionalProperties, false);
});
