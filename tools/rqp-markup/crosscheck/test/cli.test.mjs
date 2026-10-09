// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

import assert from "node:assert/strict";
import { existsSync, mkdirSync, mkdtempSync, readdirSync, rmSync, writeFileSync } from "node:fs";
import os from "node:os";
import path from "node:path";
import { after, test } from "node:test";
import { fileURLToPath } from "node:url";
import { evaluateDocs, main, parseArgs, viewCell } from "../src/cli.mjs";
import { CrosscheckError } from "../src/run.mjs";

const SUPPORT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..", "test-support");
const TMP = mkdtempSync(path.join(os.tmpdir(), "rqp-crosscheck-test-"));
after(() => rmSync(TMP, { recursive: true, force: true }));

// A marker in every test document: the output must never contain it.
const SECRET = "SECRET-DOCUMENT-TEXT";

function fixturesDir(name, docs) {
  const dir = path.join(TMP, name);
  for (const [n, text] of Object.entries(docs)) {
    mkdirSync(path.join(dir, n), { recursive: true });
    writeFileSync(path.join(dir, n, "input.html"), text);
  }
  return dir;
}

function candidatesFile(name, modes) {
  const file = path.join(TMP, `${name}.json`);
  const list = modes.map((m) => ({ name: `fake-${m}`, cwd: SUPPORT, build: null, run: ["node", "fake-candidate.mjs", m, "--batch", "{dir}"] }));
  writeFileSync(file, JSON.stringify(list));
  return file;
}

async function run(argv) {
  const lines = [];
  const code = await main(argv, { write: (l) => lines.push(l) });
  const out = lines.join("\n");
  assert.ok(!out.includes(SECRET), "the output must not contain document text");
  return { code, out };
}

const DOCS = {
  a: `<!DOCTYPE html><div data-rq-block="a">${SECRET}</div>`,
  b: `<!DOCTYPE html><ul><li><div data-rq-block="b"><p>${SECRET}</p></div></li></ul>`,
};

test("parseArgs: commands, defaults and limits", () => {
  assert.throws(() => parseArgs([]), CrosscheckError);
  assert.throws(() => parseArgs(["nope"]), CrosscheckError);
  assert.throws(() => parseArgs(["sweep", "--count", "0"]), CrosscheckError);
  assert.throws(() => parseArgs(["sweep", "--count", "1.5"]), CrosscheckError);
  assert.throws(() => parseArgs(["sweep", "--max-tokens", "201"]), CrosscheckError);
  assert.throws(() => parseArgs(["check", "--scripting", "maybe"]), CrosscheckError);
  assert.throws(() => parseArgs(["check", "--only"]), /needs a value/);
  assert.throws(() => parseArgs(["check", "--bogus"]), /unknown option/);
  const o = parseArgs(["sweep", "--seed", "9", "--count", "5", "--scripting", "both", "--only", "rust,php", "--timeout", "3"]);
  assert.deepEqual([o.seed, o.count, o.views, o.only, o.timeoutMs], [9, 5, ["enabled", "disabled"], ["rust", "php"], 3000]);
  assert.deepEqual(parseArgs(["check"]).views, ["enabled"]);
});

test("check: a candidate that agrees with parse5 passes", async () => {
  const dir = fixturesDir("agree", DOCS);
  const r = await run(["check", "--fixtures", dir, "--candidates", candidatesFile("p5", ["p5"])]);
  assert.equal(r.code, 0);
  assert.match(r.out, /fake-p5: 2 document\(s\), 2 valid/);
  assert.match(r.out, /findings: 0/);
});

test("check: a candidate that misses a block fails and names the field, not the document", async () => {
  const dir = fixturesDir("miss", DOCS);
  const r = await run(["check", "--fixtures", dir, "--candidates", candidatesFile("v0", ["valid0"])]);
  assert.equal(r.code, 1);
  assert.match(r.out, /parse5-enabled-blocks a \[fake-valid0\] block count: candidate 0, parse5 1/);
  assert.match(r.out, /findings: 2/);
});

test("check: --results reads candidate output from a file", async () => {
  const dir = fixturesDir("results", { a: DOCS.a });
  const file = path.join(TMP, "results.jsonl");
  const start = Buffer.byteLength('<!DOCTYPE html><div data-rq-block="a">');
  const block = { id: "a", format: "html", start, end: start + SECRET.length, sha256: "x" };
  writeFileSync(file, `${JSON.stringify({ fixture: "a", valid: true, error: null, blocks: [block] })}\n`);
  const ok = await run(["check", "--fixtures", dir, "--results", file]);
  assert.equal(ok.code, 0);
  writeFileSync(file, `${JSON.stringify({ fixture: "a", valid: true, error: null, blocks: [{ ...block, end: start + 1 }] })}\n`);
  const bad = await run(["check", "--fixtures", dir, "--results", file]);
  assert.equal(bad.code, 1);
  assert.match(bad.out, /end: candidate \d+, parse5 \d+/);
});

test("check --expected: the expected.json of each fixture is checked against parse5", async () => {
  const dir = fixturesDir("expected", { a: DOCS.a, b: DOCS.b });
  const start = Buffer.byteLength('<!DOCTYPE html><div data-rq-block="a">');
  const good = { valid: true, error: null, blocks: [{ id: "a", format: "html", start, end: start + SECRET.length, sha256: "x" }] };
  writeFileSync(path.join(dir, "a", "expected.json"), JSON.stringify(good));
  writeFileSync(path.join(dir, "b", "expected.json"), JSON.stringify({ valid: false, error: "RQP_MARKUP_STRUCTURE", blocks: [] }));
  const ok = await run(["check", "--fixtures", dir, "--expected"]);
  assert.equal(ok.code, 0);
  assert.match(ok.out, /expected: 2 document\(s\), 1 valid/);
  writeFileSync(path.join(dir, "a", "expected.json"), JSON.stringify({ ...good, blocks: [{ ...good.blocks[0], start: start - 1 }] }));
  const bad = await run(["check", "--fixtures", dir, "--expected"]);
  assert.equal(bad.code, 1);
  writeFileSync(path.join(dir, "a", "expected.json"), "{");
  assert.equal((await run(["check", "--fixtures", dir, "--expected"])).code, 2);
});

test("check: a missing result line is a finding", async () => {
  const dir = fixturesDir("missing", { a: DOCS.a });
  const file = path.join(TMP, "empty.jsonl");
  writeFileSync(file, "");
  const r = await run(["check", "--fixtures", dir, "--results", file]);
  assert.equal(r.code, 1);
  assert.match(r.out, /missing a \[results\] no result line/);
});

test("check --input: one document, with the parse5 views and the candidate cells", async () => {
  const file = path.join(TMP, "one.html");
  writeFileSync(file, DOCS.a);
  const r = await run(["check", "--input", file, "--scripting", "both", "--candidates", candidatesFile("p5b", ["p5", "invalid"])]);
  assert.match(r.out, /parse5: enabled 1 block\(s\); disabled 1 block\(s\)/);
  assert.match(r.out, /fake-p5: valid 1/);
  assert.match(r.out, /fake-invalid: STRUCTURE/);
  // The two candidates do not agree with each other: that is a finding too.
  assert.match(r.out, /candidates input \[\*\] fake-p5: valid a\/html\/\d+\/\d+/);
  assert.equal(r.code, 1);
  const alone = await run(["check", "--input", file, "--only", "fake-p5", "--candidates", candidatesFile("p5c", ["p5", "invalid"])]);
  assert.equal(alone.code, 0);
});

test("check: a result line with a wrong shape is reported once", async () => {
  const dir = fixturesDir("shape", { a: DOCS.a });
  const file = path.join(TMP, "shape.jsonl");
  writeFileSync(file, `${JSON.stringify({ fixture: "a", valid: "yes", blocks: [] })}\n`);
  const r = await run(["check", "--fixtures", dir, "--results", file, "--scripting", "both"]);
  assert.equal(r.code, 1);
  assert.equal(r.out.split("\n").filter((l) => l.includes("bad-result")).length, 1);
});

test("check: a candidate that crashes makes the run fail", async () => {
  const dir = fixturesDir("crash", { a: DOCS.a });
  const r = await run(["check", "--fixtures", dir, "--candidates", candidatesFile("exit", ["exit"])]);
  assert.equal(r.code, 1);
  assert.match(r.out, /candidate fake-exit: crash: exit 3/);
});

test("usage and input errors exit with 2", async () => {
  assert.equal((await run(["check", "--fixtures", path.join(TMP, "does-not-exist"), "--results", path.join(TMP, "x")])).code, 2);
  assert.equal((await run(["check", "--only", "nope", "--candidates", candidatesFile("u", ["p5"])])).code, 2);
  const bad = path.join(TMP, "bad-cases.json");
  writeFileSync(bad, JSON.stringify([{ id: "../x", input: "a" }]));
  assert.equal((await run(["cases", "--cases", bad])).code, 2);
  writeFileSync(bad, JSON.stringify([{ id: "a", input: "a" }, { id: "a", input: "b" }]));
  assert.equal((await run(["cases", "--cases", bad])).code, 2);
});

test("cases: one row per case with both parse5 views and each candidate", async () => {
  const file = path.join(TMP, "cases.json");
  writeFileSync(
    file,
    JSON.stringify([
      { id: "c1", input: DOCS.a },
      { id: "c2", input: `<noscript><script type="application/rivqen-manifest+json">${SECRET}</script></noscript>` },
    ]),
  );
  const r = await run(["cases", "--cases", file, "--candidates", candidatesFile("cs", ["p5", "valid0"])]);
  assert.equal(r.code, 0);
  const rows = r.out.split("\n");
  assert.equal(rows[0], "case | parse5 (scripting on) | parse5 (scripting off) | fake-p5 | fake-valid0");
  assert.equal(rows[1], "c1 | 1 block(s) | 1 block(s) | valid 1 | valid 0 DISAGREE(enabled,disabled)");
  assert.equal(rows[2], "c2 | 0 block(s) | 0 block(s) 1 manifest | valid 0 DISAGREE(disabled) | valid 0 DISAGREE(disabled)");
});

test("sweep: an agreeing candidate gives no finding and the same summary on every run", async () => {
  const argv = ["sweep", "--count", "60", "--chunk", "25", "--seed", "3", "--candidates", candidatesFile("sw", ["p5"])];
  const first = await run(argv);
  const second = await run(argv);
  assert.equal(first.code, 0);
  assert.match(first.out, /fake-p5 \| 60 \| \d+ \| \d+ \| 0/);
  assert.match(first.out, /findings: none/);
  assert.equal(first.out, second.out);
});

test("sweep: findings are saved as files, at most --keep of each kind", async () => {
  const out = path.join(TMP, "sweep-out");
  const r = await run(["sweep", "--count", "200", "--chunk", "70", "--keep", "2", "--out", out, "--candidates", candidatesFile("swb", ["valid0", "invalid"])]);
  assert.equal(r.code, 1);
  assert.match(r.out, /findings: .*parse5-enabled-blocks=\d+/);
  assert.match(r.out, /candidates=\d+/);
  const saved = readdirSync(out);
  assert.equal(saved.filter((n) => n.startsWith("parse5-enabled-blocks-fake-valid0-")).length, 2);
  for (const n of saved) {
    assert.ok(existsSync(path.join(out, n, "input.html")));
    assert.ok(existsSync(path.join(out, n, "finding.json")));
  }
});

test("evaluateDocs and viewCell describe the parse5 view", () => {
  const docs = [{ name: "d", bytes: Buffer.from('<p data-rq-block="a">x<p>y<svg><title data-rq-block="s"></title></svg>') }];
  const [e] = evaluateDocs(docs, new Map(), ["enabled"]);
  assert.equal(viewCell(e.p5.get("enabled")), "2 block(s) 1 no-end 1 foreign");
  assert.equal(viewCell({ ok: false, reason: "x", blocks: [], manifests: 0 }), "no tree");
});
