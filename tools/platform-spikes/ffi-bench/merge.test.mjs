// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// Tests of merge.mjs: node --test tools/platform-spikes/ffi-bench/merge.test.mjs

import { test } from 'node:test';
import assert from 'node:assert/strict';
import { buildResult, MergeError } from './merge.mjs';

const OPT = { target: 'host-jvm', date: '2026-11-08', machine: 'M', os: 'O', toolchain: { b: '2', a: '1' } };

function kase(extra = {}) {
  return {
    size: 1048576, path: 'jni-direct', chunk: 16384, docs: 1000, median_ns: 5, p95_ns: 6,
    chunk_median_ns: 1, copied_bytes: 1048576, rss_delta_kib: 3, sha256_ok: true, ...extra,
  };
}

test('contract member order, indicative, sorted toolchain', () => {
  const r = buildResult(OPT, [['a.json', kase()]]);
  assert.deepEqual(Object.keys(r), ['target', 'date', 'machine', 'os', 'toolchain', 'indicative', 'cases']);
  assert.equal(r.indicative, true);
  assert.deepEqual(Object.keys(r.toolchain), ['a', 'b']);
  assert.deepEqual(Object.keys(r.cases[0]), [
    'path', 'size', 'chunk', 'docs', 'median_ns', 'p95_ns', 'chunk_median_ns',
    'copied_bytes', 'rss_delta_kib', 'sha256_ok',
  ]);
});

test('copy split is kept after copied_bytes and must add up', () => {
  const c = kase({ copied_bytes: 3145728, rust_copied_bytes: 1048576, glue_copied_bytes: 2097152 });
  const r = buildResult(OPT, [['a.json', c]]);
  const keys = Object.keys(r.cases[0]);
  assert.deepEqual(keys.slice(7, 10), ['copied_bytes', 'rust_copied_bytes', 'glue_copied_bytes']);
  assert.equal(r.cases[0].glue_copied_bytes, 2097152);
  assert.throws(
    () => buildResult(OPT, [['a.json', { ...c, glue_copied_bytes: 1 }]]),
    (e) => e instanceof MergeError && /!= copied_bytes/.test(e.message),
  );
  assert.throws(() => buildResult(OPT, [['a.json', kase({ rust_copied_bytes: 1048576 })]]), /without its pair/);
  assert.throws(
    () => buildResult(OPT, [['a.json', kase({ rust_copied_bytes: -1, glue_copied_bytes: 1048577 })]]),
    /not a count/,
  );
});

test('refuses a bad case set', () => {
  assert.throws(() => buildResult(OPT, [['a.json', kase({ sha256_ok: false })]]), /sha256_ok/);
  const missing = kase();
  delete missing.p95_ns;
  assert.throws(() => buildResult(OPT, [['a.json', missing]]), /no p95_ns/);
  assert.throws(() => buildResult(OPT, [['a.json', kase()], ['b.json', kase({ docs: 10 })]]), /b\.json: docs 10/);
  assert.throws(() => buildResult(OPT, []), /no case files/);
  assert.throws(() => buildResult({ ...OPT, os: '' }, [['a.json', kase()]]), /--os/);
  assert.throws(() => buildResult(OPT, [['a.json', [1]]]), /not an object/);
});
