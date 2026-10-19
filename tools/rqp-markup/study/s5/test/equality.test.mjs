// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// With both flags off the study code gives the pre-S5 results: the study copy of the
// Node.js candidate equals the pre-S5 candidate (commit 270278c, recorded in
// ../baseline/baseline-s4.json) on the 311 golden fixtures of that commit, the cross-check
// cases of that commit and generated documents (the cross-check profiles and the S5
// generator). The trace does not change a result.

import { test } from 'node:test';
import assert from 'node:assert/strict';
import { digestResults, loadBaselineS4 } from '../baseline/make-s4.mjs';
import { GROUPS_S4 } from '../baseline/groups-s4.mjs';
import { analyzeS5, contractFields } from '../src/markup-s5.mjs';
import { COMBOS, COMBO_NAMES } from '../src/options.mjs';

const off = (bytes) => contractFields(analyzeS5(bytes));

test('flags off equal the pre-S5 Node.js candidate on the 311 golden fixtures of commit 270278c', () => {
  const baseline = loadBaselineS4();
  assert.equal(baseline.commit, '270278c');
  const names = Object.keys(baseline.fixtures);
  assert.equal(names.length, 311);
  const list = GROUPS_S4.fixtures(names);
  assert.equal(list.length, 311);
  for (const [name, bytes] of list) assert.deepEqual(off(bytes), baseline.fixtures[name], name);
});

test('flags off equal the pre-S5 candidate on the cross-check cases and generated documents', () => {
  const baseline = loadBaselineS4();
  let total = 0;
  for (const [group, make] of Object.entries(GROUPS_S4)) {
    if (group === 'fixtures') continue;
    const list = make();
    total += list.length;
    assert.equal(list.length, baseline.groups[group].count, group);
    assert.equal(digestResults(list.map(([, b]) => off(b))), baseline.groups[group].digest, group);
  }
  assert.ok(total >= 8000);
});

test('the trace does not change the result of any combination', () => {
  for (const group of ['tpl', 'nos', 'mix']) {
    for (const [name, bytes] of GROUPS_S4[group]().slice(0, 300)) {
      for (const c of COMBO_NAMES) {
        const a = contractFields(analyzeS5(bytes, COMBOS[c]));
        const b = contractFields(analyzeS5(bytes, { ...COMBOS[c], trace: true }));
        assert.deepEqual(b, a, `${c} ${name}`);
      }
    }
  }
});
