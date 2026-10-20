// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// Determinism, the baseline against the 2026-10-19 evidence, and invariants of
// the options.

import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';
import { pageRevision } from '../baseline/common.mjs';
import { manifestRaw } from '../src/breakdown.mjs';
import { headerBlocks } from '../src/hpack.mjs';
import { DELTA_OPTIONS, MANIFEST_OPTIONS, withoutDefaultMembers } from '../src/options.mjs';
import { DEMO_CONFIG, DEMO_TARGET, demoPage } from '../src/pages.mjs';
import { exchange, MANIFEST_END, MANIFEST_START } from '../src/rqp.mjs';
import { sizes } from '../src/sizes.mjs';
import { compute, scenarios } from '../src/study.mjs';
import { rawOnly } from '../run.mjs';
import { tables } from '../src/tables.mjs';

test('two runs give the same results object', async () => {
  const a = await compute({ withHeaders: false });
  const b = await compute({ withHeaders: false });
  assert.deepEqual(a, b);
});

test('the raw numbers of the committed results are the numbers of a new run', async () => {
  const committed = JSON.parse(readFileSync(new URL('../results/bytes.json', import.meta.url), 'utf8'));
  const lab = JSON.parse(readFileSync(new URL('../results/legacy-lab.json', import.meta.url), 'utf8')).observations;
  const fresh = await compute({ lab, withHeaders: false });
  const { headers: _h1, ...c } = rawOnly(committed);
  const { headers: _h2, ...f } = rawOnly(fresh);
  assert.deepEqual(f, c);
});

test('HPACK header block sizes are the same on two connections', async () => {
  const ex = [{ request: { ':path': '/catalog', ':authority': 'shop.example.com', 'rq-version': '1' }, response: { ':status': 304, etag: '"r1.x"' } }];
  assert.deepEqual(await headerBlocks(ex), await headerBlocks(ex));
});

test('the baseline gives the demo bytes of evidence/benchmarks/rqp-bytes-2026-10-19.json', () => {
  const ex = exchange(demoPage(0), demoPage(1), DEMO_TARGET, DEMO_CONFIG);
  const evidence = JSON.parse(readFileSync(new URL('../../../../../evidence/benchmarks/rqp-bytes-2026-10-19.json', import.meta.url), 'utf8'));
  const cold = evidence.results.find((r) => r.visit === '1. cold').rqp;
  const data = evidence.results.find((r) => r.visit === '3. data changed').rqp;
  // The raw bytes must be equal. The compressed sizes depend on the zlib and Brotli
  // builds: Node.js 26 on macOS arm64 (the evidence) and Node.js 24 on Linux x64 (CI)
  // give a gzip size that differs by 1 byte for the same raw bytes. So a compressed
  // size may differ by at most 1 % (minimum 2 bytes).
  const near = (actual, expected, label) => {
    assert.equal(actual.raw, expected.raw, `${label} raw`);
    for (const enc of ['gzip', 'br']) {
      const limit = Math.max(2, Math.ceil(expected[enc] * 0.01));
      assert.ok(Math.abs(actual[enc] - expected[enc]) <= limit, `${label} ${enc}: ${actual[enc]} vs ${expected[enc]}`);
    }
  };
  near(sizes(ex.cold.body), cold, 'cold');
  assert.equal(ex.changed.kind, data.kind);
  near(sizes(ex.changed.body), data, 'data change');
});

test('the raw manifest rows add up to F minus the page in every scenario', () => {
  for (const s of scenarios()) {
    const ex = exchange(s.pageA, s.pageB, s.target, s.config);
    assert.equal(manifestRaw(ex.manifestA).total, ex.cold.body.length - s.pageA.length, s.key);
  }
});

test('every manifest option changes only the manifest element of F', () => {
  for (const s of scenarios().filter((x) => x.kind === 'demo' || x.key === 'grid-16-large-one')) {
    const ex = exchange(s.pageA, s.pageB, s.target, s.config);
    for (const [id, fn] of Object.entries(MANIFEST_OPTIONS)) {
      const f = fn(ex);
      const head = s.pageA.subarray(0, ex.insertAtA);
      const tail = s.pageA.subarray(ex.insertAtA);
      assert.ok(f.subarray(0, head.length).equals(head), id);
      assert.ok(f.subarray(f.length - tail.length).equals(tail), id);
      const element = f.subarray(head.length, f.length - tail.length).toString('utf8');
      assert.ok(element.startsWith('<script type="application/') && element.endsWith(MANIFEST_END), id);
      if (id !== 'M10_short_manifest_type') JSON.parse(element.slice(MANIFEST_START.length, -MANIFEST_END.length));
    }
  }
});

test('the minimal manifest options keep the page revision and drop the block list', () => {
  const ex = exchange(demoPage(0), demoPage(1), DEMO_TARGET, DEMO_CONFIG);
  const text = (f) => {
    const s = f.toString('utf8');
    const at = s.indexOf(MANIFEST_START) + MANIFEST_START.length;
    return JSON.parse(s.slice(at, s.indexOf(MANIFEST_END, at)));
  };
  const m3 = text(MANIFEST_OPTIONS.M3_minimal_manifest(ex));
  assert.deepEqual(Object.keys(m3), ['protocol', 'origin', 'path', 'page_revision', 'policy', 'limits']);
  assert.equal(m3.page_revision, ex.manifestA.page_revision);
  const m1 = text(MANIFEST_OPTIONS.M1_omit_default_members(ex));
  assert.equal(m1.policy, undefined);
  assert.equal(m1.limits, undefined);
  assert.equal(m1.page_revision, pageRevision(ex.manifestA.template_revision, ex.manifestA.blocks, {}));
});

test('only default policy and limits values are omitted', () => {
  assert.deepEqual(withoutDefaultMembers(DEMO_CONFIG), {});
  assert.deepEqual(withoutDefaultMembers({ policy: { mode: 'public', ttl_seconds: 0, offline: true }, limits: { max_patch_bytes: 1024 } }), {
    policy: { mode: 'public', offline: true },
    limits: { max_patch_bytes: 1024 },
  });
  assert.deepEqual(withoutDefaultMembers({ policy: { ttl_seconds: 60 } }), { policy: { ttl_seconds: 60 } });
  assert.deepEqual(withoutDefaultMembers({}), {});
});

test('delta options keep the content and the next revision of the baseline delta', () => {
  const ex = exchange(demoPage(0), demoPage(1), DEMO_TARGET, DEMO_CONFIG);
  const contents = ex.delta.operations.map((o) => o.content);
  for (const id of ['DC1_minimal_object', 'DC2_minimal_short_names', 'DC3_minimal_array']) {
    const text = DELTA_OPTIONS[id](ex).toString('utf8');
    for (const c of contents) assert.ok(text.includes(JSON.stringify(c)), id);
    assert.ok(text.includes(ex.delta.next_revision), id);
    assert.ok(!text.includes(ex.delta.base_revision) && !text.includes(ex.delta.operations[0].sha256), id);
  }
  const framed = DELTA_OPTIONS.DC4_content_outside_json(ex);
  const nl = framed.indexOf(0x0a);
  const [next, ops] = JSON.parse(framed.subarray(0, nl).toString('utf8'));
  assert.equal(next, ex.delta.next_revision);
  let at = nl + 1;
  ops.forEach(([id, len], i) => {
    assert.equal(id, ex.delta.operations[i].block_id);
    assert.equal(framed.subarray(at, at + len).toString('utf8'), contents[i]);
    at += len;
  });
  assert.equal(at, framed.length);
});

test('the report tables of the committed results have every table and only real numbers', () => {
  const committed = JSON.parse(readFileSync(new URL('../results/bytes.json', import.meta.url), 'utf8'));
  const t = tables(committed);
  for (const name of ['demo_visits', 'demo_manifest', 'demo_delta', 'markup', 'grid_cold', 'grid_change', 'options_manifest', 'options_delta_all', 'options_delta_one', 'headers', 'proposals', 'visit_totals']) {
    assert.equal(typeof t[name], 'string', name);
    assert.doesNotMatch(t[name], /undefined|NaN|\[object Object\]/, name);
  }
  // Header row, alignment row, the demo and the 16 grid scenarios.
  assert.equal(t.grid_change.split('\n').length, 2 + 17);
});
