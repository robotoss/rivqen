// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// The input groups of the pre-S3 baseline (baseline.mjs). Each group is a
// function that returns [name, bytes] pairs in a fixed order.

import { existsSync, readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { docSeed, generateDocument } from '../../crosscheck/src/generator.mjs';
import { generateRelaxDocument } from '../relax/src/gen.mjs';
import { generateNoscriptDocument } from '../noscript/src/generator.mjs';

const HERE = path.dirname(fileURLToPath(import.meta.url));
const ROOT = path.resolve(HERE, '..', '..', '..', '..');
const FIXTURES = path.join(ROOT, 'fixtures', 'rqp', 'markup');
const CASES = path.join(ROOT, 'tools', 'rqp-markup', 'crosscheck', 'cases');

/** The 197 golden fixtures and the 4 case files of S2 (before S3 added new ones). */
export const S2_CASE_FILES = ['conformance-cases.json', 'edge-cases.json', 'review-cases.json', 'review2-cases.json'];
export const N = 1500;

const gen = (profile) => () =>
  Array.from({ length: N }, (_, i) => [`${profile} ${i}`, Buffer.from(generateDocument(docSeed(7, i), { maxTokens: 20, profile }), 'utf8')]);

export const GROUPS = {
  /** Fixture names come from the baseline file (the S2 set); the bytes from the working tree. */
  fixtures: (names) =>
    names.filter((n) => existsSync(path.join(FIXTURES, n, 'input.html'))).map((n) => [n, readFileSync(path.join(FIXTURES, n, 'input.html'))]),
  cases: () =>
    S2_CASE_FILES.flatMap((file) => JSON.parse(readFileSync(path.join(CASES, file), 'utf8')).map((c) => [`${file}:${c.id}`, Buffer.from(c.input, 'utf8')])),
  classic: gen('classic'),
  review: gen('review'),
  review2: gen('review2'),
  review3: gen('review3'),
  relax: () => Array.from({ length: N }, (_, i) => [`relax ${i}`, Buffer.from(generateRelaxDocument(docSeed(8, i)), 'utf8')]),
  noscriptBody: () => Array.from({ length: N }, (_, i) => [`body ${i}`, Buffer.from(generateNoscriptDocument(7, i, 'body').input, 'utf8')]),
  noscriptHead: () => Array.from({ length: N }, (_, i) => [`head ${i}`, Buffer.from(generateNoscriptDocument(7, i, 'head').input, 'utf8')]),
};
