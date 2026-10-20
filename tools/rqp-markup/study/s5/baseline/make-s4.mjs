// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// One-off: records baseline-s4.json with the pre-S5 Node.js candidate (commit
// 270278c: the S4 rules H-78, H-84, H-86, H-85a; template and noscript are in
// FORBIDDEN_CONTENT). Run it only with that candidate, for example extracted from git
// into a directory outside the tree:
//   git show 270278c:tools/rqp-markup/node/src/<file>.mjs > <dir>/<file>.mjs
//     (markup, names, tokenizer, json)
//   git ls-tree --name-only 270278c fixtures/rqp/markup/ > <names file>
//   node tools/rqp-markup/study/s5/baseline/make-s4.mjs <dir>/markup.mjs <names file>
// The groups use the S5 generator (../src/gen-s5.mjs): when the generator changes,
// record the file again with the same candidate.

import { createHash } from 'node:crypto';
import { readFileSync, writeFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { GROUPS_S4 } from './groups-s4.mjs';

const HERE = path.dirname(fileURLToPath(import.meta.url));
export const BASELINE_S4_FILE = path.join(HERE, 'baseline-s4.json');

/** One digest over a list of result objects, in order (JSON text, one line each). */
export function digestResults(results) {
  const h = createHash('sha256');
  for (const r of results) h.update(`${JSON.stringify(r)}\n`);
  return h.digest('base64url');
}

/** The recorded baseline: { commit, fixtures: { name: result }, groups: { name: { count, digest } } }. */
export function loadBaselineS4() {
  return JSON.parse(readFileSync(BASELINE_S4_FILE, 'utf8'));
}

async function main(argv) {
  const { analyze } = await import(pathToFileURL(path.resolve(argv[0])).href);
  const names = readFileSync(argv[1], 'utf8')
    .split('\n')
    .map((n) => path.basename(n))
    .filter((n) => n.startsWith('FX-RQ-MARKUP-'))
    .sort();
  const fixtures = {};
  for (const [name, bytes] of GROUPS_S4.fixtures(names)) fixtures[name] = analyze(bytes);
  if (Object.keys(fixtures).length !== names.length) throw new Error('a fixture input is missing');
  const groups = {};
  for (const [group, make] of Object.entries(GROUPS_S4)) {
    if (group === 'fixtures') continue;
    const list = make();
    groups[group] = { count: list.length, digest: digestResults(list.map(([, b]) => analyze(b))) };
  }
  const out = { commit: '270278c', candidate: 'tools/rqp-markup/node (pre-S5 rules)', fixtures, groups };
  writeFileSync(BASELINE_S4_FILE, `${JSON.stringify(out, null, 1)}\n`);
  process.stdout.write(`baseline: ${names.length} fixtures, ${Object.keys(groups).length} groups\n`);
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  main(process.argv.slice(2)).catch((err) => {
    process.stderr.write(`make-s4: ${err instanceof Error ? err.message : String(err)}\n`);
    process.exit(2);
  });
}
