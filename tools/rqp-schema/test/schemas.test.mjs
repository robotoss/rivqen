// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors
//
// The schema files themselves: Draft 2020-12, strict, stable ids, shared definitions in sync.

import assert from 'node:assert/strict';
import { test } from 'node:test';
import { KINDS, newAjv, readJson, schemaPath } from './helpers.mjs';

const DRAFT = 'https://json-schema.org/draft/2020-12/schema';
// Since H-107 the two schemas share no definition: the manifest has the page revision,
// the envelope the binding. Both use the canonical base64url tail of section 1.3, rule 2.
const CANONICAL_TAIL = '[A-Za-z0-9_-]{42}[AEIMQUYcgkosw048]$';

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

test('the page revision of the manifest and the binding of the patch have the same canonical base64url tail (H-107)', () => {
  const m = readJson(schemaPath('manifest')).$defs;
  const p = readJson(schemaPath('patch')).$defs;
  assert.equal(m.page_revision.pattern, `^r1\\.${CANONICAL_TAIL}`);
  assert.equal(p.binding.pattern, `^d1\\.${CANONICAL_TAIL}`);
  assert.equal(p.page_revision, undefined);
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

test('H-104, H-107: the envelope has exactly binding and operations; an operation exactly block_id and content', () => {
  const p = readJson(schemaPath('patch'));
  assert.deepEqual(Object.keys(p.properties), ['binding', 'operations']);
  assert.deepEqual(p.required, ['binding', 'operations']);
  assert.deepEqual(Object.keys(p.$defs.operation.properties), ['block_id', 'content']);
  assert.deepEqual(p.$defs.operation.required, ['block_id', 'content']);
  assert.equal(p.additionalProperties, false);
  assert.equal(p.$defs.operation.additionalProperties, false);
});
