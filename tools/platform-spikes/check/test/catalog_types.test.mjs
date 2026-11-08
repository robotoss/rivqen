// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

import assert from 'node:assert/strict';
import test from 'node:test';
import { catalogProblems } from '../src/catalog.mjs';

const okResult = (extra = {}) => ({ os: 'host-jvm', verdict: 'supported', condition: null, runs: ['a', 'b', 'c'], ...extra });
const okRow = (extra = {}) => ({
  id: 'CAP-X-01',
  platform: 'ffi',
  title: 'Title',
  prototype: 'PROTO',
  questions: ['Q-06'],
  sprint: 'S1',
  results: [okResult()],
  verdict: 'supported',
  ...extra,
});
const withResult = (extra) => [okRow({ results: [okResult(extra)] })];

test('catalog_types: a valid row has no problems', () => {
  assert.deepEqual(catalogProblems([okRow()]), []);
});

test('catalog_types: a catalog that is not an array is reported', () => {
  assert.deepEqual(catalogProblems({}), ['catalog: not a JSON array']);
  assert.deepEqual(catalogProblems(null), ['catalog: not a JSON array']);
});

for (const [name, value] of [['null', null], ['an array', []], ['a string', 'row'], ['a number', 7]]) {
  test(`catalog_types: a row that is ${name} is reported with its 1-based position`, () => {
    assert.deepEqual(catalogProblems([okRow(), value]), ['row 2: not an object']);
  });
}

for (const [name, id] of [['missing', undefined], ['empty', ''], ['a number', 5]]) {
  test(`catalog_types: a row whose id is ${name} is reported by position`, () => {
    assert.deepEqual(catalogProblems([okRow(), okRow({ id })]), ['row 2: id must be a non-empty string']);
  });
}

test('catalog_types: a duplicate id is reported once', () => {
  assert.deepEqual(catalogProblems([okRow(), okRow()]), ['CAP-X-01: duplicate id']);
});

for (const key of ['title', 'prototype', 'sprint']) {
  test(`catalog_types: an empty ${key} is reported`, () => {
    assert.deepEqual(catalogProblems([okRow({ [key]: '' })]), [`CAP-X-01: ${key} must be a non-empty string`]);
  });
}

test('catalog_types: a platform that is not allowed is reported', () => {
  assert.deepEqual(catalogProblems([okRow({ platform: 'web' })]), ['CAP-X-01: platform "web" is not allowed']);
});

for (const [name, questions] of [['missing', undefined], ['a string', 'Q-06'], ['an array with a number', [1]]]) {
  test(`catalog_types: questions that are ${name} are reported`, () => {
    assert.deepEqual(catalogProblems([okRow({ questions })]), ['CAP-X-01: questions must be an array of strings']);
  });
}

test('catalog_types: results that are not an array are reported and the verdict is not compared', () => {
  assert.deepEqual(catalogProblems([okRow({ results: 'none' })]), ['CAP-X-01: results must be an array']);
  assert.deepEqual(catalogProblems([okRow({ results: undefined })]), ['CAP-X-01: results must be an array']);
});

for (const [name, value] of [['null', null], ['an array', []], ['a number', 3]]) {
  test(`catalog_types: a result that is ${name} is reported with its 1-based position`, () => {
    assert.deepEqual(catalogProblems([okRow({ results: [okResult(), value] })]), ['CAP-X-01: result 2: not an object']);
  });
}

for (const [name, os] of [['missing', undefined], ['empty', ''], ['a number', 37]]) {
  test(`catalog_types: a result whose os is ${name} is reported`, () => {
    assert.deepEqual(catalogProblems(withResult({ os })), ['CAP-X-01: result 1: os must be a non-empty string']);
  });
}

test('catalog_types: results without an os are not reported as duplicates of each other', () => {
  const rows = [okRow({ results: [okResult({ os: '' }), okResult({ os: '' })] })];
  assert.deepEqual(catalogProblems(rows), [
    'CAP-X-01: result 1: os must be a non-empty string',
    'CAP-X-01: result 2: os must be a non-empty string',
  ]);
});

test('catalog_types: a result verdict that is not allowed is reported', () => {
  assert.deepEqual(catalogProblems(withResult({ verdict: 'pending' })), ['CAP-X-01: result 1: verdict "pending" is not allowed']);
});

for (const [name, runs] of [['missing', undefined], ['a string', 'run-1'], ['an array with a number', ['a', 1]]]) {
  test(`catalog_types: runs that are ${name} are reported`, () => {
    assert.deepEqual(catalogProblems(withResult({ runs })), ['CAP-X-01: result 1: runs must be an array of strings']);
  });
}

for (const [name, condition] of [['an empty string', ''], ['a number', 5]]) {
  test(`catalog_types: a condition that is ${name} is reported`, () => {
    assert.deepEqual(catalogProblems(withResult({ condition })), ['CAP-X-01: result 1: condition must be null or a non-empty string']);
  });
}

test('catalog_types: a condition that is absent or null is accepted', () => {
  assert.deepEqual(catalogProblems(withResult({ condition: undefined })), []);
  assert.deepEqual(catalogProblems(withResult({ condition: null })), []);
});

test('catalog_types: a conditional result with a condition is accepted', () => {
  const rows = [okRow({ verdict: 'conditional', results: [okResult({ verdict: 'conditional', condition: 'on 2 GB' })] })];
  assert.deepEqual(catalogProblems(rows), []);
});

for (const key of ['recording', 'metrics', 'webview']) {
  for (const [name, value] of [['an empty string', ''], ['a number', 5]]) {
    test(`catalog_types: ${key} that is ${name} is reported`, () => {
      assert.deepEqual(catalogProblems(withResult({ [key]: value })), [`CAP-X-01: result 1: ${key} must be a non-empty string`]);
    });
  }
  test(`catalog_types: ${key} that is absent, null or a string is accepted`, () => {
    for (const value of [undefined, null, 'some/path']) assert.deepEqual(catalogProblems(withResult({ [key]: value })), []);
  });
}

test('catalog_types: a source that is not a URL is reported', () => {
  for (const source of ['nope', '', 5]) {
    assert.deepEqual(catalogProblems(withResult({ source })), ['CAP-X-01: result 1: source must be a URL string']);
  }
});

test('catalog_types: a source that is absent, null or a URL is accepted', () => {
  for (const source of [undefined, null, 'https://developer.android.com/x']) {
    assert.deepEqual(catalogProblems(withResult({ source })), []);
  }
});

test('catalog_types: an impossible result with only runs is accepted', () => {
  const rows = [okRow({ verdict: 'impossible', results: [okResult({ verdict: 'impossible', runs: ['a'] })] })];
  assert.deepEqual(catalogProblems(rows), []);
});

test('catalog_types: an impossible result with only a source is accepted', () => {
  const rows = [okRow({ verdict: 'impossible', results: [okResult({ verdict: 'impossible', runs: [], source: 'https://x.test/p' })] })];
  assert.deepEqual(catalogProblems(rows), []);
});

test('catalog_types: an impossible result with a non-URL source and no runs needs a source or a run', () => {
  const rows = [okRow({ verdict: 'impossible', results: [okResult({ verdict: 'impossible', runs: [], source: 'nope' })] })];
  assert.deepEqual(catalogProblems(rows), [
    'CAP-X-01: result 1: source must be a URL string',
    'CAP-X-01: result 1: an impossible result needs a source or a run',
  ]);
});

test('catalog_types: a capability verdict that is not allowed is reported and not compared', () => {
  assert.deepEqual(catalogProblems([okRow({ verdict: 'maybe' })]), ['CAP-X-01: verdict "maybe" is not allowed']);
});

test('catalog_types: the capability verdict is not compared when a result is invalid', () => {
  const rows = [okRow({ verdict: 'pending', results: [okResult({ os: '' })] })];
  assert.deepEqual(catalogProblems(rows), ['CAP-X-01: result 1: os must be a non-empty string']);
});
