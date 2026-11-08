// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

import assert from 'node:assert/strict';
import test from 'node:test';
import { catalogProblems } from '../src/catalog.mjs';
import { expectProblem } from './helpers.mjs';

const row = (overrides = {}) => ({
  id: 'CAP-X-01',
  platform: 'ffi',
  title: 'Title',
  prototype: 'PROTO',
  questions: ['Q-06'],
  sprint: 'S1',
  results: [],
  verdict: 'pending',
  ...overrides,
});

test('catalog_fields: a complete row has no problems', () => {
  assert.deepEqual(catalogProblems([row()]), []);
});

test('catalog_fields: a missing title is reported', () => {
  const { title, ...rest } = row();
  expectProblem(catalogProblems([rest]), /CAP-X-01: title must be/);
});

test('catalog_fields: a platform outside android, ios, ffi and url is reported', () => {
  expectProblem(catalogProblems([row({ platform: 'windows' })]), /platform "windows" is not allowed/);
});

test('catalog_fields: questions must be an array of strings', () => {
  expectProblem(catalogProblems([row({ questions: 'Q-06' })]), /questions must be an array of strings/);
});

test('catalog_fields: a result verdict outside the allowed values is reported', () => {
  const results = [{ os: 'host-jvm', verdict: 'maybe', runs: [] }];
  expectProblem(catalogProblems([row({ results })]), /verdict "maybe" is not allowed/);
});

test('catalog_fields: a capability verdict outside the allowed values is reported', () => {
  expectProblem(catalogProblems([row({ verdict: 'unknown' })]), /verdict "unknown" is not allowed/);
});

test('catalog_fields: a source that is not a URL is reported', () => {
  const results = [{ os: 'host-jvm', verdict: 'impossible', source: 'not a url', runs: [] }];
  expectProblem(catalogProblems([row({ results })]), /source must be a URL string/);
});

test('catalog_fields: a duplicate capability id is reported', () => {
  expectProblem(catalogProblems([row(), row()]), /CAP-X-01: duplicate id/);
});
