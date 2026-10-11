#!/usr/bin/env node
// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// Run the cross-check depth analysis (../crosscheck/src/depth.mjs, parse5 tree builder,
// scripting enabled) on one file and print one JSON object: max D, WHATWG stack, DOM depth,
// the number of invariant violations and the rule classes. Never prints document text.
//
// Usage: node depth-cli.mjs <file>

import { readFileSync } from "node:fs";
import { analyzeDepth } from "../crosscheck/src/depth.mjs";

const file = process.argv[2];
if (file === undefined) {
  process.stderr.write("usage: depth-cli.mjs <file>\n");
  process.exit(2);
}
try {
  const r = analyzeDepth(readFileSync(file));
  process.stdout.write(
    `${JSON.stringify({ ok: r.ok, maxD: r.maxD, maxStack: r.maxStack, maxDepth: r.maxDepth, violations: r.violations.length, classes: r.classes })}\n`,
  );
} catch (err) {
  process.stderr.write(`depth-cli: ${err instanceof Error ? err.name : "error"}\n`);
  process.exit(2);
}
