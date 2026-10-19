// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// Property checks of the S5 study on small seeded sweeps (the full sweeps are in
// ../results): the proposed combinations t2, n1, n2, t2n1 and t2n2 have no finding; every
// combination accepts a superset of the smaller ones (monotonic); the controls tu and
// nu have findings, so the oracle reaches the shapes.

import { test } from 'node:test';
import assert from 'node:assert/strict';
import { sweep } from '../src/sweep.mjs';

const PROPOSED = ['t2', 'n1', 'n2', 't2n1', 't2n2'];
const noFindings = (row) => Object.keys(row.findings).length === 0 && Object.keys(row.gainedFindings).length === 0;

for (const source of ['tpl', 'nos', 'mix', 'review2']) {
  test(`sweep ${source}: proposed combinations have no finding, results are monotonic`, () => {
    const out = sweep({ source, seed: 9, count: 4000, examples: 1 });
    assert.equal(out.count, 4000);
    assert.equal(out.monotonic, 0);
    for (const c of ['base', ...PROPOSED]) assert.ok(noFindings(out.combos[c]), `${c}: ${JSON.stringify(out.combos[c].gainedFindings)}`);
    // No patch apply gap for the patch rules p1 and p2.
    for (const c of PROPOSED) {
      for (const k of Object.keys(out.combos[c].patch.findings)) assert.ok(!/^p[12]:patch-unseen/.test(k), `${c} ${k}`);
    }
    if (source !== 'review2') assert.ok(out.combos.t1n1.gained > 100, `${source} gained ${out.combos.t1n1.gained}`);
  });
}

test('the controls have findings: the oracle reaches the shapes that the rules remove', () => {
  const t = sweep({ source: 'tpl', seed: 9, count: 4000, examples: 0 });
  assert.ok((t.combos.tu.gainedFindings['parse5-enabled-blocks'] ?? 0) > 0);
  const n = sweep({ source: 'nos', seed: 9, count: 4000, examples: 0 });
  assert.ok((n.combos.nu.gainedFindings['parse5-disabled-blocks'] ?? 0) > 0);
  // The shipped patch rule with noscript content: the scripting-disabled parse makes elements that it did not read.
  assert.ok((n.combos.n1.patch.findings['shipped:patch-unseen-disabled'] ?? 0) > 0);
});
