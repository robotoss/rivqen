// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// Golden fixtures FX-RQ-MARKUP-* (fixtures/rqp/markup), in process. The
// differential runner (tools/rqp-markup/diff.mjs) checks the same through the CLI.

import { describe, it } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync, readdirSync, existsSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import path from 'node:path';
import { analyze } from '../src/markup.mjs';

// The nearest fixtures/rqp/markup above this file (also from a mutation sandbox).
function findFixtures() {
  let dir = path.dirname(fileURLToPath(import.meta.url));
  for (;;) {
    const candidate = path.join(dir, 'fixtures', 'rqp', 'markup');
    if (existsSync(candidate)) return candidate;
    const parent = path.dirname(dir);
    if (parent === dir) throw new Error('fixtures/rqp/markup not found');
    dir = parent;
  }
}

const FIXTURES = findFixtures();
const names = readdirSync(FIXTURES).filter((n) => existsSync(path.join(FIXTURES, n, 'expected.json'))).sort();

describe('golden fixtures', () => {
  it('has the fixture set', () => {
    assert.ok(names.length >= 117, `found ${names.length} fixtures`);
  });

  for (const name of names) {
    it(name, () => {
      const dir = path.join(FIXTURES, name);
      const expected = JSON.parse(readFileSync(path.join(dir, 'expected.json'), 'utf8'));
      const actual = analyze(readFileSync(path.join(dir, 'input.html')));
      for (const field of ['valid', 'error', 'blocks', 'template_revision', 'page_revision']) {
        assert.deepEqual(actual[field], expected[field], field);
      }
    });
  }
});
