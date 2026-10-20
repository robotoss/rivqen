// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// The input groups of the pre-S5 baseline (baseline-s4.json, see make-s4.mjs). Each
// group is a function that returns [name, bytes] pairs in a fixed order. They are the
// inputs of the S5 equality test (../test/equality.test.mjs).

import { docSeed, generateDocument } from '../../../crosscheck/src/generator.mjs';
import { generateS5Document } from '../src/gen-s5.mjs';
import { crosscheckCases, markupFixtures } from '../src/inputs.mjs';

/** The case files at commit 270278c (before S5 added s5-cases.json). */
export const S4_CASE_FILES = Object.freeze([
  'conformance-cases.json', 'edge-cases.json', 'review-cases.json', 'review2-cases.json', 's3-cases.json',
  's4-cases.json', 's4b-cases.json',
]);

const gen = (profile) => () =>
  Array.from({ length: 400 }, (_, i) => [`${profile} ${i}`, Buffer.from(generateDocument(docSeed(4, i), { maxTokens: 24, profile }), 'utf8')]);

const s5 = (profile) => () =>
  Array.from({ length: 2000 }, (_, i) => [`${profile} ${i}`, Buffer.from(generateS5Document(docSeed(4, i), profile), 'utf8')]);

export const GROUPS_S4 = {
  /** Fixture names come from the baseline file (the set at 270278c); the bytes from the working tree. */
  fixtures: (names) => markupFixtures(names),
  cases: () => crosscheckCases(S4_CASE_FILES),
  classic: gen('classic'),
  review: gen('review'),
  review2: gen('review2'),
  review3: gen('review3'),
  tpl: s5('tpl'),
  nos: s5('nos'),
  mix: s5('mix'),
};
