// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// Fake parser candidate for the crosscheck tests (CONTRACT.md section 2, batch mode only).
// Usage: node fake-candidate.mjs <mode> --batch <dir>
//   p5       valid with the parse5 blocks when every block has an end tag, else invalid (agrees with parse5)
//   invalid  every document is invalid
//   valid0   every document is valid with no blocks (disagrees when parse5 sees a block)
//   exit     exit code 3

import { readdirSync, readFileSync } from "node:fs";
import path from "node:path";
import { parse5View } from "../src/view.mjs";

const [mode, flag, dir] = process.argv.slice(2);
if (flag !== "--batch" || !dir) process.exit(2);
if (mode === "exit") process.exit(3);

const names = readdirSync(dir).sort((a, b) => Buffer.compare(Buffer.from(a), Buffer.from(b)));
for (const name of names) {
  let bytes;
  try {
    bytes = readFileSync(path.join(dir, name, "input.html"));
  } catch {
    continue;
  }
  let result = { valid: false, error: "RQP_MARKUP_STRUCTURE", blocks: [], template_revision: null, page_revision: null };
  if (mode === "valid0") {
    result = { valid: true, error: null, blocks: [], template_revision: "t1.x", page_revision: "r1.x" };
  } else if (mode === "p5") {
    const view = parse5View(bytes);
    if (view.ok && view.manifests === 0 && view.blocks.every((b) => b.html && b.end !== null && b.start !== null)) {
      const blocks = view.blocks.map((b) => ({
        id: b.id,
        format: b.tag === "script" ? "json" : "html",
        start: b.start,
        end: b.end,
        sha256: "x",
      }));
      result = { valid: true, error: null, blocks, template_revision: "t1.x", page_revision: "r1.x" };
    }
  }
  process.stdout.write(`${JSON.stringify({ fixture: name, ...result })}\n`);
}
