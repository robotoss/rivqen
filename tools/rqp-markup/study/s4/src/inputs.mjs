// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// Input sets of the S4 T-03 studies. Each function returns [name, bytes] pairs in a
// fixed order. Only repository files and synthetic documents.

import { existsSync, readFileSync, readdirSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { PAGE_IDS, pageBytes } from '../../../../rqp-protocol/src/pages.mjs';

const HERE = path.dirname(fileURLToPath(import.meta.url));
export const REPO = path.resolve(HERE, '..', '..', '..', '..', '..');
const MARKUP_FIXTURES = path.join(REPO, 'fixtures', 'rqp', 'markup');
const PROTOCOL_FIXTURES = path.join(REPO, 'fixtures', 'rqp', 'protocol');
const EXAMPLES = path.join(REPO, 'spec', 'rqp', 'examples');
const CASES = path.join(REPO, 'tools', 'rqp-markup', 'crosscheck', 'cases');

/** The golden markup fixtures (fixtures/rqp/markup/<name>/input.html). */
export function markupFixtures() {
  return readdirSync(MARKUP_FIXTURES)
    .sort()
    .map((n) => [n, path.join(MARKUP_FIXTURES, n, 'input.html')])
    .filter(([, f]) => existsSync(f))
    .map(([n, f]) => [n, readFileSync(f)]);
}

/** The inputs of every cross-check case file. */
export function crosscheckCases() {
  return readdirSync(CASES)
    .filter((f) => f.endsWith('.json'))
    .sort()
    .flatMap((file) => JSON.parse(readFileSync(path.join(CASES, file), 'utf8')).map((c) => [`${file}:${c.id}`, Buffer.from(c.input, 'utf8')]));
}

/** Every document of the protocol fixtures: pages (files and recipes), response bodies, spec examples. */
export function protocolDocuments() {
  const out = PAGE_IDS.map((id) => [`page:${id}`, pageBytes(id)]);
  for (const n of readdirSync(PROTOCOL_FIXTURES).sort()) {
    const f = path.join(PROTOCOL_FIXTURES, n, 'response.body');
    if (existsSync(f)) out.push([`body:${n}`, readFileSync(f)]);
  }
  const walk = (dir) => {
    for (const e of readdirSync(dir, { withFileTypes: true }).sort((a, b) => (a.name < b.name ? -1 : 1))) {
      const p = path.join(dir, e.name);
      if (e.isDirectory()) walk(p);
      else if (e.name.endsWith('.html')) out.push([`spec:${path.relative(EXAMPLES, p)}`, readFileSync(p)]);
    }
  };
  walk(EXAMPLES);
  return out;
}
