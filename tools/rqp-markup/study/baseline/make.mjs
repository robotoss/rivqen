// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// One-off: records baseline-s2.json with the pre-S3 Node.js candidate (see
// baseline.mjs). It was run once at commit a2836de, before the S3 candidate
// lanes. Do not run it again with a later candidate.
//   node tools/rqp-markup/study/baseline/make.mjs <fixture names file>

import { readFileSync, writeFileSync } from 'node:fs';
import { analyze } from '../../node/src/markup.mjs';
import { BASELINE_FILE, digestResults } from './baseline.mjs';
import { GROUPS } from './groups.mjs';

const names = readFileSync(process.argv[2], 'utf8').split('\n').filter(Boolean);
const fixtures = {};
for (const [name, bytes] of GROUPS.fixtures(names)) fixtures[name] = analyze(bytes);
if (Object.keys(fixtures).length !== names.length) throw new Error('a fixture input is missing');
const groups = {};
for (const [group, make] of Object.entries(GROUPS)) {
  if (group === 'fixtures') continue;
  const list = make();
  groups[group] = { count: list.length, digest: digestResults(list.map(([, b]) => analyze(b))) };
}
const out = { commit: 'a2836de', candidate: 'tools/rqp-markup/node (pre-S3 rules)', fixtures, groups };
writeFileSync(BASELINE_FILE, `${JSON.stringify(out, null, 1)}\n`);
console.log(`baseline: ${names.length} fixtures, ${Object.keys(groups).length} groups`);
