// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// The input groups of the pre-S4 baseline (baseline-s3.json, see make-s3.mjs).
// Each group is a function that returns [name, bytes] pairs in a fixed order.
// They are the inputs of the S4 T-03 equality test (study/s4/test/equality.test.mjs).

import { existsSync, readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { docSeed, generateDocument } from '../../crosscheck/src/generator.mjs';
import { generateTableDocument } from '../s4/src/table-gen.mjs';

const HERE = path.dirname(fileURLToPath(import.meta.url));
const ROOT = path.resolve(HERE, '..', '..', '..', '..');
const FIXTURES = path.join(ROOT, 'fixtures', 'rqp', 'markup');
const CASES = path.join(ROOT, 'tools', 'rqp-markup', 'crosscheck', 'cases');

/** The case files of S3 (before S4 added new ones). */
export const S3_CASE_FILES = ['conformance-cases.json', 'edge-cases.json', 'review-cases.json', 'review2-cases.json', 's3-cases.json'];

const gen = (profile) => () =>
  Array.from({ length: 400 }, (_, i) => [`${profile} ${i}`, Buffer.from(generateDocument(docSeed(4, i), { maxTokens: 24, profile }), 'utf8')]);

export const GROUPS_S3 = {
  /** Fixture names come from the baseline file (the S3 set); the bytes from the working tree. */
  fixtures: (names) =>
    names.filter((n) => existsSync(path.join(FIXTURES, n, 'input.html'))).map((n) => [n, readFileSync(path.join(FIXTURES, n, 'input.html'))]),
  cases: () =>
    S3_CASE_FILES.flatMap((file) => JSON.parse(readFileSync(path.join(CASES, file), 'utf8')).map((c) => [`${file}:${c.id}`, Buffer.from(c.input, 'utf8')])),
  classic: gen('classic'),
  review: gen('review'),
  review2: gen('review2'),
  review3: gen('review3'),
  tables: () => Array.from({ length: 2000 }, (_, i) => [`tables ${i}`, Buffer.from(generateTableDocument(4, i), 'utf8')]),
};
