// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// Fake parser candidate for the runner tests. It echoes expected.json and
// breaks the output in the way that --mode names.
// Usage: node fake-candidate.mjs <mode> --batch <fixtures-dir>

import { readdirSync, readFileSync } from "node:fs";
import path from "node:path";

const [mode, flag, dir] = process.argv.slice(2);
if (flag !== "--batch" || !dir) process.exit(2);

if (mode === "exit") {
  process.stderr.write("fake failure\n");
  process.exit(3);
}
if (mode === "hang") {
  setInterval(() => {}, 1000);
} else if (mode === "badjson") {
  process.stdout.write("this is not json\n");
} else if (mode === "nofixturefield") {
  process.stdout.write('{"valid":true}\n');
} else {
  const names = readdirSync(dir).sort();
  const shown = mode === "missing" ? names.slice(0, -1) : names;
  for (const name of shown) {
    const result = JSON.parse(readFileSync(path.join(dir, name, "expected.json"), "utf8"));
    if (mode === "diff") {
      result.page_revision = `r1.${"x".repeat(200)}`;
      if (result.blocks.length > 0) result.blocks[0].end += 1;
    }
    process.stdout.write(`${JSON.stringify({ fixture: name, ...result })}\n`);
  }
}
