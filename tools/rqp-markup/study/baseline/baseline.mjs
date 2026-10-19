// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// The pre-S3 baseline of the two S3 studies (T-01 relax, T-02 noscript).
//
// The study copies model the markup rules as they were before the S3 decisions
// H-71, H-73, H-74 and H-75, behind flags. After Phase B the shipped Node.js
// candidate follows the new rules, so the study tests cannot compare "flags
// off" with the shipped candidate any more. They compare with this baseline
// instead: the results of the pre-S3 Node.js candidate (commit a2836de),
// recorded once in baseline-s2.json by make.mjs. Do not regenerate the file
// with a later candidate.
//
// The S4 T-03 study (study/s4) has its own baseline in the same form: the
// results of the pre-S4 Node.js candidate (commit 9bd8c32, before H-78, H-84
// and H-86), recorded once in baseline-s3.json by make-s3.mjs.

import { createHash } from 'node:crypto';
import { readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const HERE = path.dirname(fileURLToPath(import.meta.url));
export const BASELINE_FILE = path.join(HERE, 'baseline-s2.json');
export const BASELINE_S3_FILE = path.join(HERE, 'baseline-s3.json');

/** One digest over a list of result objects, in order (JSON text, one line each). */
export function digestResults(results) {
  const h = createHash('sha256');
  for (const r of results) h.update(`${JSON.stringify(r)}\n`);
  return h.digest('base64url');
}

/** The recorded baseline: { commit, fixtures: { name: result }, groups: { name: { count, digest } } }. */
export function loadBaseline(file = BASELINE_FILE) {
  return JSON.parse(readFileSync(file, 'utf8'));
}
