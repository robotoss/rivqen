// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

import assert from "node:assert/strict";
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { after, before, describe, it } from "node:test";
import { fileURLToPath } from "node:url";

import { allOk, compareResult, main, parseArgs, show } from "../diff.mjs";

const HERE = path.dirname(fileURLToPath(import.meta.url));
const FAKE = path.join(HERE, "fake-candidate.mjs");

const VALID = {
  valid: true,
  error: null,
  blocks: [{ id: "price", format: "html", start: 10, end: 16, sha256: "a".repeat(43) }],
  template_revision: "t1.abc",
  page_revision: "r1.def",
};
const INVALID = {
  valid: false,
  error: "RQP_MARKUP_DUPLICATE",
  blocks: [],
  template_revision: null,
  page_revision: null,
};

let root;
let fixtures;

function writeFixture(name, expected) {
  const d = path.join(fixtures, name);
  mkdirSync(d, { recursive: true });
  writeFileSync(path.join(d, "input.html"), "<p></p>");
  writeFileSync(path.join(d, "expected.json"), JSON.stringify(expected));
}

function candidate(name, mode, extra = {}) {
  return { name, cwd: ".", build: null, run: ["node", FAKE, mode, "--batch", "{dir}"], ...extra };
}

async function run(list, args = []) {
  const file = path.join(root, "candidates.json");
  writeFileSync(file, JSON.stringify(list));
  return main(["--fixtures", fixtures, "--candidates", file, ...args], { repoRoot: root });
}

before(() => {
  root = mkdtempSync(path.join(tmpdir(), "rqp-diff-"));
  fixtures = path.join(root, "fixtures");
  writeFixture("FX-A", VALID);
  writeFixture("FX-B", INVALID);
});

after(() => {
  rmSync(root, { recursive: true, force: true });
});

describe("runner", () => {
  it("prints an OK matrix and exits 0 when every result equals expected.json", async () => {
    const r = await run([candidate("fake", "ok")]);
    assert.equal(r.exitCode, 0);
    assert.match(r.output, /^fixture\s+fake$/m);
    assert.match(r.output, /^FX-A\s+OK$/m);
    assert.match(r.output, /^FX-B\s+OK$/m);
    assert.match(r.output, /2\/2 OK/);
  });

  it("reports DIFF with detail lines and exits 1", async () => {
    const r = await run([candidate("fake", "diff")]);
    assert.equal(r.exitCode, 1);
    assert.match(r.output, /^FX-A\s+DIFF$/m);
    assert.match(r.output, /^FX-B\s+DIFF$/m);
    assert.match(r.output, /FX-A \[fake\] blocks\[0\]\.end: expected 16, got 17/);
  });

  it("prints at most 80 characters of a differing field", async () => {
    const r = await run([candidate("fake", "diff")]);
    const line = r.output.split("\n").find((l) => l.includes("page_revision"));
    assert.ok(line, "detail line exists");
    assert.ok(!line.includes("x".repeat(100)));
    const got = line.slice(line.indexOf("got ") + 4);
    assert.ok(got.length <= 80, `field text is ${got.length} characters`);
  });

  it("reports MISSING for a fixture without a result", async () => {
    const r = await run([candidate("fake", "missing")]);
    assert.equal(r.exitCode, 1);
    assert.match(r.output, /^FX-A\s+OK$/m);
    assert.match(r.output, /^FX-B\s+MISSING$/m);
  });

  it("reports CRASH when the exit code is not 0", async () => {
    const r = await run([candidate("fake", "exit")]);
    assert.equal(r.exitCode, 1);
    assert.match(r.output, /^FX-A\s+CRASH$/m);
    assert.match(r.output, /fake: CRASH: exit 3: fake failure/);
  });

  it("reports CRASH when the output is not valid JSON", async () => {
    const r = await run([candidate("fake", "badjson")]);
    assert.equal(r.exitCode, 1);
    assert.match(r.output, /^FX-A\s+CRASH$/m);
    assert.match(r.output, /not valid JSON/);
  });

  it("reports CRASH when an output line has no fixture field", async () => {
    const r = await run([candidate("fake", "nofixturefield")]);
    assert.equal(r.exitCode, 1);
    assert.match(r.output, /^FX-A\s+CRASH$/m);
  });

  it("reports CRASH on timeout", async () => {
    const r = await run([candidate("fake", "hang")], ["--timeout", "0.3"]);
    assert.equal(r.exitCode, 1);
    assert.match(r.output, /^FX-A\s+CRASH$/m);
    assert.match(r.output, /timeout after 300 ms/);
  });

  it("reports NOT BUILT when the candidate directory does not exist", async () => {
    const r = await run([candidate("gone", "ok", { cwd: "no-such-dir" })]);
    assert.equal(r.exitCode, 1);
    assert.match(r.output, /^FX-A\s+NOT BUILT$/m);
    assert.match(r.output, /gone: NOT BUILT: directory no-such-dir does not exist/);
  });

  it("reports NOT BUILT when the build command fails and does not run the candidate", async () => {
    const r = await run([candidate("b", "ok", { build: ["node", "-e", "process.exit(4)"] })]);
    assert.equal(r.exitCode, 1);
    assert.match(r.output, /^FX-A\s+NOT BUILT$/m);
    assert.match(r.output, /build failed, exit 4/);
  });

  it("reports NOT BUILT when the run executable does not exist", async () => {
    const r = await run([candidate("e", "ok", { run: ["./no-such-binary", "--batch", "{dir}"] })]);
    assert.equal(r.exitCode, 1);
    assert.match(r.output, /^FX-A\s+NOT BUILT$/m);
  });

  it("runs the build before the run", async () => {
    const marker = path.join(root, "built.txt");
    const build = ["node", "-e", `require("node:fs").writeFileSync(${JSON.stringify(marker)}, "x")`];
    const run1 = [
      "node",
      "-e",
      `require("node:fs").readFileSync(${JSON.stringify(marker)}); process.exit(0)`,
    ];
    const r = await run([candidate("ordered", "ok", { build, run: run1 })]);
    // Run prints nothing, so fixtures are MISSING. CRASH or NOT BUILT would mean the marker was absent.
    assert.match(r.output, /^FX-A\s+MISSING$/m);
  });

  it("compares several candidates in separate columns", async () => {
    const r = await run([candidate("good", "ok"), candidate("bad", "diff")]);
    assert.equal(r.exitCode, 1);
    assert.match(r.output, /^fixture\s+good\s+bad$/m);
    assert.match(r.output, /^FX-A\s+OK\s+DIFF$/m);
  });

  it("--only runs the named candidates and skips the others", async () => {
    const r = await run([candidate("good", "ok"), candidate("gone", "ok", { cwd: "no-such-dir" })], [
      "--only",
      "good",
    ]);
    assert.equal(r.exitCode, 0);
    assert.doesNotMatch(r.output, /gone/);
  });

  it("--only with an unknown name is a usage error with exit 2", async () => {
    const r = await run([candidate("good", "ok")], ["--only", "nope"]);
    assert.equal(r.exitCode, 2);
    assert.match(r.output, /unknown candidate: nope/);
  });

  it("an empty fixture set exits non-zero", async () => {
    const empty = path.join(root, "empty");
    mkdirSync(empty, { recursive: true });
    const file = path.join(root, "candidates.json");
    writeFileSync(file, JSON.stringify([candidate("fake", "ok")]));
    const r = await main(["--fixtures", empty, "--candidates", file], { repoRoot: root });
    assert.equal(r.exitCode, 1);
    assert.match(r.output, /no fixtures found/);
  });

  it("a missing fixtures directory is exit 2", async () => {
    const file = path.join(root, "candidates.json");
    writeFileSync(file, JSON.stringify([candidate("fake", "ok")]));
    const r = await main(["--fixtures", path.join(root, "absent"), "--candidates", file], { repoRoot: root });
    assert.equal(r.exitCode, 2);
  });

  it("an invalid candidates.json entry is exit 2", async () => {
    const r = await run([{ name: "x", cwd: ".", build: "make", run: ["a"] }]);
    assert.equal(r.exitCode, 2);
    assert.match(r.output, /invalid candidate entry/);
  });
});

describe("compareResult", () => {
  it("returns no lines for equal results and ignores the fixture field", () => {
    assert.deepEqual(compareResult(VALID, { fixture: "x", ...VALID }), []);
  });

  it("names the field that differs", () => {
    const lines = compareResult(VALID, { ...VALID, valid: false });
    assert.deepEqual(lines, ["valid: expected true, got false"]);
  });

  it("reports a block count difference", () => {
    const lines = compareResult(VALID, { ...VALID, blocks: [] });
    assert.deepEqual(lines, ["blocks.length: expected 1, got 0"]);
  });

  it("reports an extra field inside a block", () => {
    const blocks = [{ ...VALID.blocks[0], extra: 1 }];
    assert.deepEqual(compareResult(VALID, { ...VALID, blocks }), [
      "blocks[0].extra: expected undefined, got 1",
    ]);
  });

  it("distinguishes null from a missing field", () => {
    const { error, ...rest } = INVALID;
    assert.equal(error, "RQP_MARKUP_DUPLICATE");
    assert.equal(compareResult(INVALID, rest).length, 1);
  });
});

describe("helpers", () => {
  it("show keeps short values whole and cuts long values to 80 characters", () => {
    assert.equal(show("ab"), '"ab"');
    assert.equal(show("x".repeat(500)).length, 80);
    assert.equal(show("y".repeat(78)).length, 80);
  });

  it("allOk is false for no cells", () => {
    assert.equal(allOk(new Map()), false);
  });

  it("parseArgs rejects unknown arguments and a missing value", () => {
    assert.throws(() => parseArgs(["--nope"]), /unknown argument/);
    assert.throws(() => parseArgs(["--only"]), /missing value/);
    assert.throws(() => parseArgs(["--timeout", "0"]), /positive/);
    assert.equal(parseArgs(["--timeout", "5"]).timeoutS, 5);
    assert.deepEqual(parseArgs(["--only", "a,,b"]).only, ["a", "b"]);
  });
});
