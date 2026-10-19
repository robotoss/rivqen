// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// One-off: records baseline-s3.json with the pre-S4 Node.js candidate. It was run
// once with the candidate of commit 9bd8c32 (before the S4 decisions H-78, H-84 and
// H-86), extracted from git into a directory outside the tree. Do not run it again
// with a later candidate.
//   git show 9bd8c32:tools/rqp-markup/node/src/<file>.mjs > <dir>/<file>.mjs
//     (markup, names, tokenizer, json)
//   git ls-tree --name-only 9bd8c32 fixtures/rqp/markup/ > <names file>  (FX-* entries only)
//   node tools/rqp-markup/study/baseline/make-s3.mjs <dir>/markup.mjs <names file>

import { readFileSync, writeFileSync } from 'node:fs';
import path from 'node:path';
import { pathToFileURL } from 'node:url';
import { BASELINE_S3_FILE, digestResults } from './baseline.mjs';
import { GROUPS_S3 } from './groups-s3.mjs';

const { analyze } = await import(pathToFileURL(path.resolve(process.argv[2])).href);
const names = readFileSync(process.argv[3], 'utf8')
  .split('\n')
  .map((n) => path.basename(n))
  .filter((n) => n.startsWith('FX-RQ-MARKUP-'))
  .sort();
const fixtures = {};
for (const [name, bytes] of GROUPS_S3.fixtures(names)) fixtures[name] = analyze(bytes);
if (Object.keys(fixtures).length !== names.length) throw new Error('a fixture input is missing');
const groups = {};
for (const [group, make] of Object.entries(GROUPS_S3)) {
  if (group === 'fixtures') continue;
  const list = make();
  groups[group] = { count: list.length, digest: digestResults(list.map(([, b]) => analyze(b))) };
}
const out = { commit: '9bd8c32', candidate: 'tools/rqp-markup/node (pre-S4 rules)', fixtures, groups };
writeFileSync(BASELINE_S3_FILE, `${JSON.stringify(out, null, 1)}\n`);
console.log(`baseline: ${names.length} fixtures, ${Object.keys(groups).length} groups`);
