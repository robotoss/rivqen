// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors
//
// The schema files themselves: Draft 2020-12, strict, stable ids, shared definitions in sync.

import assert from 'node:assert/strict';
import { test } from 'node:test';
import { KINDS, newAjv, readJson, schemaPath } from './helpers.mjs';

const DRAFT = 'https://json-schema.org/draft/2020-12/schema';
// Definitions that both schemas carry. They must be identical (one rule, two copies).
const SHARED_DEFS = ['block_id', 'format', 'sha256', 'template_revision', 'page_revision'];

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

test('manifest and patch schemas share identical definitions of ids, hashes and revisions', () => {
  const m = readJson(schemaPath('manifest')).$defs;
  const p = readJson(schemaPath('patch')).$defs;
  for (const name of SHARED_DEFS) assert.deepEqual(p[name], m[name], name);
});

test('the block id pattern is the markup rule M-15 pattern', () => {
  assert.equal(readJson(schemaPath('manifest')).$defs.block_id.pattern, '^[a-z0-9][a-z0-9_-]{0,63}$');
});
