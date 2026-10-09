// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { existsSync, mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { after, before, describe, it } from "node:test";
import { fileURLToPath } from "node:url";

import {
  DEFAULT_CANDIDATES,
  DEFAULT_FIXTURES,
  REPO_ROOT,
  compareResult,
  evaluate,
  loadFixtures,
  main,
  parseBatchOutput,
  parseCandidates,
  renderReport,
  runProcess,
  show,
  stderrHint,
} from "../diff.mjs";

const HERE = path.dirname(fileURLToPath(import.meta.url));
const DIFF = path.join(HERE, "..", "diff.mjs");
const FAKE = path.join(HERE, "fake-candidate.mjs");

let root;

before(() => {
  root = mkdtempSync(path.join(tmpdir(), "rqp-units-"));
});

after(() => {
  rmSync(root, { recursive: true, force: true });
});

function fixtureDir(name, files) {
  const d = path.join(root, name);
  mkdirSync(d, { recursive: true });
  for (const [rel, content] of Object.entries(files)) {
    mkdirSync(path.dirname(path.join(d, rel)), { recursive: true });
    writeFileSync(path.join(d, rel), content);
  }
  return d;
}

describe("defaults", () => {
  it("point at the repository layout", () => {
    assert.ok(existsSync(path.join(REPO_ROOT, "AGENTS.md")));
    assert.ok(existsSync(DEFAULT_CANDIDATES));
    assert.equal(DEFAULT_FIXTURES, path.join(REPO_ROOT, "fixtures", "rqp", "markup"));
  });

  it("candidates.json lists the four planned candidates with valid entries", async () => {
    const { readFileSync } = await import("node:fs");
    const list = parseCandidates(readFileSync(DEFAULT_CANDIDATES, "utf8"));
    assert.deepEqual(list.map((c) => c.name), ["rust", "node", "java", "php"]);
    for (const c of list) assert.ok(c.run.includes("{dir}"));
  });
});

describe("show", () => {
  it("keeps a value of exactly 80 characters whole", () => {
    const v = "y".repeat(78);
    assert.equal(show(v), JSON.stringify(v));
  });

  it("cuts a longer value to 80 characters ending in dots", () => {
    const s = show("x".repeat(500));
    assert.equal(s.length, 80);
    assert.ok(s.endsWith("..."));
  });

  it("shows undefined", () => {
    assert.equal(show(undefined), "undefined");
  });
});

describe("compareResult", () => {
  const base = { valid: true, error: null, blocks: [], template_revision: "t", page_revision: "p" };

  it("rejects a result that is not an object", () => {
    assert.deepEqual(compareResult(base, "oops"), ['result: expected an object, got "oops"']);
    assert.equal(compareResult(base, [1]).length, 1);
    assert.equal(compareResult(base, null).length, 1);
  });

  it("compares every contract field", () => {
    for (const f of ["valid", "error", "template_revision", "page_revision"]) {
      const lines = compareResult(base, { ...base, [f]: "other" });
      assert.equal(lines.length, 1, f);
      assert.ok(lines[0].startsWith(`${f}:`), f);
    }
  });

  it("lists block field differences in key order", () => {
    const e = { ...base, blocks: [{ b: 2, a: 1 }] };
    const a = { ...base, blocks: [{ b: 9, a: 8 }] };
    assert.deepEqual(compareResult(e, a), [
      "blocks[0].a: expected 1, got 8",
      "blocks[0].b: expected 2, got 9",
    ]);
  });

  it("reports an array against a non-array", () => {
    assert.equal(compareResult(base, { ...base, blocks: { length: 0 } }).length, 1);
  });

  it("reports an object against an array in a block", () => {
    const e = { ...base, blocks: [{ a: 1 }] };
    assert.equal(compareResult(e, { ...base, blocks: [[1]] }).length, 1);
  });

  it("does not report fields outside the contract", () => {
    assert.deepEqual(compareResult(base, { ...base, note: "x" }), []);
  });
});

describe("loadFixtures", () => {
  it("returns fixtures in byte order and skips plain files and empty directories", () => {
    const d = fixtureDir("lf1", {
      "b/expected.json": '{"valid":true}',
      "B/expected.json": '{"valid":false}',
      "a/expected.json": '{"valid":true}',
      "note.txt": "x",
    });
    mkdirSync(path.join(d, "empty"));
    const names = loadFixtures(d).map((f) => f.name);
    assert.deepEqual(names, ["B", "a", "b"]);
  });

  it("parses expected.json", () => {
    const d = fixtureDir("lf2", { "a/expected.json": '{"valid":false,"error":"E"}' });
    assert.deepEqual(loadFixtures(d), [{ name: "a", expected: { valid: false, error: "E" } }]);
  });

  it("fails when input.html has no expected.json", () => {
    const d = fixtureDir("lf3", { "a/input.html": "<p></p>" });
    assert.throws(() => loadFixtures(d), /fixture a has input\.html but no expected\.json/);
  });

  it("fails with the cause when expected.json is not JSON", () => {
    const d = fixtureDir("lf4", { "a/expected.json": "{" });
    assert.throws(
      () => loadFixtures(d),
      (e) => e.name === "RunnerError" && e.code === "RUNNER_USAGE" && /fixture a/.test(e.message) && e.cause instanceof Error,
    );
  });

  it("fails with the cause when the directory is missing", () => {
    assert.throws(
      () => loadFixtures(path.join(root, "absent")),
      (e) => /cannot read fixtures directory/.test(e.message) && e.cause instanceof Error,
    );
  });
});

describe("parseCandidates", () => {
  const good = { name: "n", cwd: ".", build: null, run: ["x"] };

  it("accepts a valid list, with and without build", () => {
    assert.equal(parseCandidates(JSON.stringify([good, { ...good, build: ["b"] }])).length, 2);
  });

  it("rejects text that is not JSON, with the cause", () => {
    assert.throws(
      () => parseCandidates("{"),
      (e) => /not valid JSON/.test(e.message) && e.cause instanceof Error,
    );
  });

  it("rejects JSON that is not an array", () => {
    assert.throws(() => parseCandidates("{}"), /must be an array/);
  });

  it("rejects each malformed entry", () => {
    const bad = [
      null,
      [],
      { ...good, name: 1 },
      { ...good, name: undefined },
      { ...good, cwd: 1 },
      { ...good, cwd: undefined },
      { ...good, build: [] },
      { ...good, build: [1] },
      { ...good, build: "make" },
      { ...good, build: undefined },
      { ...good, run: [] },
      { ...good, run: [1] },
      { ...good, run: ["a", 1] },
      { ...good, run: "x" },
    ];
    for (const c of bad) {
      assert.throws(() => parseCandidates(JSON.stringify([c])), /invalid candidate entry/, JSON.stringify(c));
    }
  });
});

describe("parseBatchOutput", () => {
  it("collects lines by fixture, ignores blank lines, keeps the first of a duplicate", () => {
    const out = parseBatchOutput('{"fixture":"a","n":1}\n\n  \n{"fixture":"b","n":2}\n{"fixture":"a","n":3}\n');
    assert.deepEqual([...out.results.keys()], ["a", "b"]);
    assert.equal(out.results.get("a").n, 1);
  });

  it("returns a crash reason for bad JSON, a non-object, an array and a missing fixture field", () => {
    assert.match(parseBatchOutput("{").crash, /not valid JSON/);
    assert.match(parseBatchOutput("5").crash, /no string field fixture/);
    assert.match(parseBatchOutput("[]").crash, /no string field fixture/);
    assert.match(parseBatchOutput('{"fixture":1}').crash, /no string field fixture/);
    assert.match(parseBatchOutput("null").crash, /no string field fixture/);
  });

  it("accepts empty output as no results", () => {
    assert.equal(parseBatchOutput("").results.size, 0);
  });
});

describe("stderrHint", () => {
  it("returns the first non-blank line, trimmed and shortened", () => {
    assert.equal(stderrHint("\n  \n  boom  \nsecond"), ": boom");
    assert.equal(stderrHint(""), "");
    assert.equal(stderrHint(`${"z".repeat(300)}`).length, 2 + 78);
  });
});

describe("runProcess", () => {
  it("returns exit code, stdout and stderr", async () => {
    const r = await runProcess(["node", "-e", 'process.stdout.write("o");process.stderr.write("e");process.exit(5)'], root, 10_000);
    assert.deepEqual(r, { kind: "exit", code: 5, stdout: "o", stderr: "e" });
  });

  it("runs in the given directory", async () => {
    const r = await runProcess(["node", "-e", "process.stdout.write(process.cwd())"], root, 10_000);
    assert.equal(r.stdout, path.resolve(root));
  });

  it("does not pass arguments through a shell", async () => {
    const r = await runProcess(["node", "-e", "process.stdout.write(process.argv[1])", "a b;$(x)"], root, 10_000);
    assert.equal(r.stdout, "a b;$(x)");
  });

  it("reports a missing executable as spawn-error with its code", async () => {
    const r = await runProcess(["./no-such-binary"], root, 10_000);
    assert.equal(r.kind, "spawn-error");
    assert.equal(r.error.code, "ENOENT");
  });

  it("reports an argument that cannot be spawned as spawn-error", async () => {
    const r = await runProcess(["bad\0name"], root, 10_000);
    assert.equal(r.kind, "spawn-error");
  });

  it("reports a signal exit", async () => {
    const r = await runProcess(["node", "-e", 'process.kill(process.pid, "SIGKILL")'], root, 10_000);
    assert.equal(r.kind, "exit");
    assert.equal(r.code, "signal SIGKILL");
  });

  it("kills a process that runs too long, with its children", async () => {
    const marker = path.join(root, "child-alive.txt");
    const child = `setTimeout(()=>require("node:fs").writeFileSync(${JSON.stringify(marker)},"x"),700)`;
    const parent = `require("node:child_process").spawn(process.execPath,["-e",${JSON.stringify(child)}],{stdio:"ignore"});setInterval(()=>{},1000)`;
    const r = await runProcess(["node", "-e", parent], root, 300);
    assert.equal(r.kind, "timeout");
    await new Promise((resolve) => setTimeout(resolve, 1200));
    assert.equal(existsSync(marker), false, "child process was killed with the group");
  });

  it("stops a process whose output is over the limit", async () => {
    const r = await runProcess(["node", "-e", 'process.stdout.write("x".repeat(1000));setInterval(()=>{},1000)'], root, 10_000, 100);
    assert.equal(r.kind, "overflow");
  });

  it("accepts output of exactly the limit", async () => {
    const r = await runProcess(["node", "-e", 'process.stdout.write("x".repeat(100))'], root, 10_000, 100);
    assert.equal(r.kind, "exit");
    assert.equal(r.stdout.length, 100);
  });

  it("counts output of several chunks together", async () => {
    const script = 'process.stdout.write("x".repeat(60));setTimeout(()=>{process.stdout.write("x".repeat(60));setInterval(()=>{},1000)},100)';
    const r = await runProcess(["node", "-e", script], root, 10_000, 100);
    assert.equal(r.kind, "overflow");
  });

  it("keeps the start of a long stderr", async () => {
    const script = 'for(let i=0;i<200;i++)process.stderr.write("L"+i+"\\n")';
    const r = await runProcess(["node", "-e", script], root, 10_000);
    assert.ok(r.stderr.startsWith("L0\n"));
  });
});

describe("report", () => {
  const fixtures = [
    { name: "FX-A", expected: { valid: true, blocks: [] } },
    { name: "FX-LONGER", expected: { valid: true, blocks: [] } },
  ];
  const runs = new Map([
    ["one", { state: "ok", results: new Map([["FX-A", { valid: true, blocks: [] }]]) }],
    ["two", { state: "crash", reason: "exit 3: boom" }],
    ["three", { state: "notbuilt", reason: "directory x does not exist" }],
    ["four", { state: "ok", results: new Map([["FX-A", { valid: false, blocks: [] }], ["FX-LONGER", { valid: true, blocks: [] }]]) }],
  ]);

  it("evaluates each cell state", () => {
    const { cells, details } = evaluate(fixtures, runs);
    assert.deepEqual([...cells.get("FX-A").values()], ["OK", "CRASH", "NOT BUILT", "DIFF"]);
    assert.deepEqual([...cells.get("FX-LONGER").values()], ["MISSING", "CRASH", "NOT BUILT", "OK"]);
    assert.deepEqual(details, ["FX-A [four] valid: expected true, got false"]);
  });

  it("renders the matrix, candidate notes, details and the summary", () => {
    const text = renderReport(fixtures, runs, evaluate(fixtures, runs));
    assert.equal(
      text,
      [
        "fixture    one      two    three      four",
        "FX-A       OK       CRASH  NOT BUILT  DIFF",
        "FX-LONGER  MISSING  CRASH  NOT BUILT  OK",
        "",
        "two: CRASH: exit 3: boom",
        "three: NOT BUILT: directory x does not exist",
        "",
        "FX-A [four] valid: expected true, got false",
        "",
        "2/8 OK",
      ].join("\n"),
    );
  });

  it("omits the note and detail sections when there are none", () => {
    const ok = new Map([["c", { state: "ok", results: new Map([["FX-A", { valid: true, blocks: [] }]]) }]]);
    const one = [fixtures[0]];
    assert.equal(renderReport(one, ok, evaluate(one, ok)), "fixture  c\nFX-A     OK\n\n1/1 OK");
  });
});

describe("main errors", () => {
  function candidatesFile(content) {
    const file = path.join(root, `cand-${Math.random().toString(16).slice(2)}.json`);
    writeFileSync(file, content);
    return file;
  }
  const fx = () => fixtureDir("mainfx", { "a/expected.json": '{"valid":true,"blocks":[]}' });

  it("exit 2 with the cause when candidates.json is missing", async () => {
    const r = await main(["--fixtures", fx(), "--candidates", path.join(root, "absent.json")], { repoRoot: root });
    assert.equal(r.exitCode, 2);
    assert.match(r.output, /^error: cannot read .*absent\.json: ENOENT/);
  });

  it("exit 2 for candidates.json that is not JSON", async () => {
    const r = await main(["--fixtures", fx(), "--candidates", candidatesFile("{")], { repoRoot: root });
    assert.equal(r.exitCode, 2);
    assert.match(r.output, /candidates\.json is not valid JSON: /);
  });

  it("exit 2 for candidates.json that is not an array", async () => {
    const r = await main(["--fixtures", fx(), "--candidates", candidatesFile("{}")], { repoRoot: root });
    assert.equal(r.exitCode, 2);
    assert.match(r.output, /must be an array/);
  });

  it("exit 2 when no candidate is selected", async () => {
    const r = await main(["--fixtures", fx(), "--candidates", candidatesFile("[]")], { repoRoot: root });
    assert.equal(r.exitCode, 2);
    assert.match(r.output, /no candidates selected/);
  });

  it("lists every unknown candidate name", async () => {
    const file = candidatesFile(JSON.stringify([{ name: "a", cwd: ".", build: null, run: ["x"] }]));
    const r = await main(["--fixtures", fx(), "--candidates", file, "--only", "p,q"], { repoRoot: root });
    assert.equal(r.output, "error: unknown candidate: p, q");
  });

  it("exit 2 for a bad argument", async () => {
    const r = await main(["--bogus"], { repoRoot: root });
    assert.equal(r.exitCode, 2);
    assert.equal(r.output, "error: unknown argument --bogus");
  });

  it("a build that cannot start is NOT BUILT with the error code", async () => {
    const list = [{ name: "a", cwd: ".", build: ["./no-such-binary"], run: ["node", FAKE, "ok", "--batch", "{dir}"] }];
    const r = await main(["--fixtures", fx(), "--candidates", candidatesFile(JSON.stringify(list))], { repoRoot: root });
    assert.match(r.output, /a: NOT BUILT: build command not runnable \(ENOENT\)/);
  });

  it("a build that runs too long is NOT BUILT", async () => {
    const list = [{ name: "a", cwd: ".", build: ["node", "-e", "setInterval(()=>{},1000)"], run: ["node", FAKE, "ok", "--batch", "{dir}"] }];
    const r = await main(["--fixtures", fx(), "--candidates", candidatesFile(JSON.stringify(list)), "--timeout", "0.3"], { repoRoot: root });
    assert.match(r.output, /a: NOT BUILT: build timed out/);
  });

  it("a run command that cannot start is NOT BUILT with the error code", async () => {
    const list = [{ name: "a", cwd: ".", build: null, run: ["./no-such-binary", "{dir}"] }];
    const r = await main(["--fixtures", fx(), "--candidates", candidatesFile(JSON.stringify(list))], { repoRoot: root });
    assert.match(r.output, /a: NOT BUILT: run command not runnable \(ENOENT\)/);
  });

  it("replaces {dir} inside a longer argument", async () => {
    const dir = fx();
    const script = `process.stdout.write(JSON.stringify({fixture:"a",blocks:[],valid:process.argv[1]==="d=${dir}"})+"\\n")`;
    const list = [{ name: "a", cwd: ".", build: null, run: ["node", "-e", script, "d={dir}"] }];
    const r = await main(["--fixtures", dir, "--candidates", candidatesFile(JSON.stringify(list))], { repoRoot: root });
    assert.equal(r.exitCode, 0, r.output);
  });
});

describe("command line", () => {
  it("prints the matrix to stdout and sets the exit code", () => {
    const dir = fixtureDir("cli1", { "a/expected.json": '{"valid":true,"blocks":[]}' });
    const cand = path.join(root, "cli-cand.json");
    writeFileSync(cand, JSON.stringify([{ name: "f", cwd: root, build: null, run: ["node", FAKE, "ok", "--batch", "{dir}"] }]));
    const r = spawnSync("node", [DIFF, "--fixtures", dir, "--candidates", cand], { encoding: "utf8" });
    assert.equal(r.status, 0);
    assert.equal(r.stdout, "fixture  f\na        OK\n\n1/1 OK\n");
  });

  it("exits 2 and prints the usage error", () => {
    const r = spawnSync("node", [DIFF, "--bogus"], { encoding: "utf8" });
    assert.equal(r.status, 2);
    assert.equal(r.stdout, "error: unknown argument --bogus\n");
  });
});
