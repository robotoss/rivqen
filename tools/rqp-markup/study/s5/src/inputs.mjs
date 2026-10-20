// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// Input sets of the S5 study: repository files and synthetic documents only. Each
// function returns [name, bytes] pairs in a fixed order.

import { existsSync, readFileSync, readdirSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const HERE = path.dirname(fileURLToPath(import.meta.url));
export const REPO = path.resolve(HERE, '..', '..', '..', '..', '..');
export const MARKUP_FIXTURES = path.join(REPO, 'fixtures', 'rqp', 'markup');
export const CASES_DIR = path.join(REPO, 'tools', 'rqp-markup', 'crosscheck', 'cases');

/** The golden markup fixtures (fixtures/rqp/markup/<name>/input.html), optionally only some names. */
export function markupFixtures(names = null) {
  const list = names ?? readdirSync(MARKUP_FIXTURES).sort();
  return list
    .map((n) => [n, path.join(MARKUP_FIXTURES, n, 'input.html')])
    .filter(([, f]) => existsSync(f))
    .map(([n, f]) => [n, readFileSync(f)]);
}

/** The inputs of the cross-check case files (all, or the named files). */
export function crosscheckCases(files = null) {
  const list = files ?? readdirSync(CASES_DIR).filter((f) => f.endsWith('.json')).sort();
  return list.flatMap((file) =>
    JSON.parse(readFileSync(path.join(CASES_DIR, file), 'utf8')).map((c) => [`${file}:${c.id}`, Buffer.from(c.input, 'utf8')]),
  );
}
