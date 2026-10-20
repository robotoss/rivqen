#!/usr/bin/env node
// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// Write documents of the relaxation generator as a batch directory
// (<dir>/<name>/input.html), for the cross-check tool and the four shipped candidates:
//
//   node src/write-relax.mjs --out <new empty dir> [--seed n] [--count n]
//   cd ../../crosscheck && node src/cli.mjs check --fixtures <dir> --only rust,node,java,php --scripting both
//
// The documents are synthetic (src/gen.mjs).

import { existsSync, mkdirSync, readdirSync, writeFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { docSeed } from '../../../crosscheck/src/generator.mjs';
import { generateRelaxDocument } from './gen.mjs';

export function writeRelax(out, seed, count) {
  if (existsSync(out) && readdirSync(out).length > 0) throw new Error(`${out} is not empty`);
  for (let i = 0; i < count; i++) {
    const dir = path.join(out, `relax-${seed}-${String(i).padStart(7, '0')}`);
    mkdirSync(dir, { recursive: true });
    writeFileSync(path.join(dir, 'input.html'), generateRelaxDocument(docSeed(seed, i)));
  }
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const o = { out: null, seed: 40, count: 20000 };
  const a = process.argv.slice(2);
  for (let i = 0; i < a.length; i += 2) {
    if (a[i] === '--out') o.out = path.resolve(a[i + 1]);
    else if (a[i] === '--seed') o.seed = Number(a[i + 1]);
    else if (a[i] === '--count') o.count = Number(a[i + 1]);
    else {
      process.stderr.write(`write-relax: unknown option ${a[i]}\n`);
      process.exit(2);
    }
  }
  if (o.out === null || !Number.isSafeInteger(o.seed) || !Number.isSafeInteger(o.count) || o.count < 1 || o.count > 1_000_000) {
    process.stderr.write('usage: write-relax.mjs --out <dir> [--seed n] [--count 1..1000000]\n');
    process.exit(2);
  }
  try {
    writeRelax(o.out, o.seed, o.count);
  } catch (err) {
    process.stderr.write(`write-relax: ${err instanceof Error ? err.message : String(err)}\n`);
    process.exit(2);
  }
}
