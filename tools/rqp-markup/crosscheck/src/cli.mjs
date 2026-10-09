#!/usr/bin/env node
// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// Differential cross-check of RQP markup parser candidates against parse5 (WHATWG tree builder).
// Commands: check, cases, sweep. See ../README.md. The tool never prints a document.

import { existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, statSync, writeFileSync } from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { compareCandidates, compareWithView, resultShapeError, show } from "./compare.mjs";
import { docSeed, generateDocument } from "./generator.mjs";
import { CrosscheckError, buildCandidate, loadCandidates, readResultsFile, runBatch, writeBatchDir } from "./run.mjs";
import { parse5View } from "./view.mjs";

const HERE = path.dirname(fileURLToPath(import.meta.url));
export const TOOL_DIR = path.resolve(HERE, "..");
export const REPO_ROOT = path.resolve(TOOL_DIR, "..", "..", "..");
const DEFAULTS = {
  candidates: path.join(TOOL_DIR, "..", "candidates.json"),
  fixtures: path.join(REPO_ROOT, "fixtures", "rqp", "markup"),
  cases: path.join(TOOL_DIR, "cases", "edge-cases.json"),
};
const MAX_COUNT = 10_000_000;
const MAX_DETAIL_LINES = 6;
const USAGE = `usage:
  node src/cli.mjs check [--fixtures dir | --input file] [--results file.jsonl] [common options]
  node src/cli.mjs cases [--cases file.json] [common options]
  node src/cli.mjs sweep [--seed n] [--count n] [--max-tokens n] [--chunk n] [--out dir] [--keep n] [common options]
common options: --only name,... --candidates file --timeout seconds --build --scripting enabled|disabled|both`;

/** @param {string} name @param {string | undefined} v @param {number} min @param {number} max */
function intArg(name, v, min, max) {
  const n = Number(v);
  if (v === undefined || !Number.isSafeInteger(n) || n < min || n > max) {
    throw new CrosscheckError(`${name} needs an integer from ${min} to ${max}`);
  }
  return n;
}

/** Parse the command line. Exported for tests. */
export function parseArgs(argv) {
  const [command, ...rest] = argv;
  if (!["check", "cases", "sweep"].includes(command ?? "")) throw new CrosscheckError(USAGE);
  const o = {
    command,
    only: null,
    candidates: DEFAULTS.candidates,
    timeoutMs: 300_000,
    build: false,
    views: ["enabled"],
    fixtures: DEFAULTS.fixtures,
    input: null,
    results: null,
    cases: DEFAULTS.cases,
    seed: 1,
    count: 1000,
    maxTokens: 16,
    chunk: 2000,
    out: null,
    keep: 20,
  };
  for (let i = 0; i < rest.length; i++) {
    const flag = rest[i];
    const value = () => {
      const v = rest[++i];
      if (v === undefined) throw new CrosscheckError(`${flag} needs a value`);
      return v;
    };
    switch (flag) {
      case "--only":
        o.only = value().split(",").filter((s) => s !== "");
        break;
      case "--candidates":
        o.candidates = path.resolve(value());
        break;
      case "--timeout":
        o.timeoutMs = intArg(flag, value(), 1, 86_400) * 1000;
        break;
      case "--build":
        o.build = true;
        break;
      case "--scripting": {
        const v = value();
        if (!["enabled", "disabled", "both"].includes(v)) throw new CrosscheckError("--scripting is enabled, disabled or both");
        o.views = v === "both" ? ["enabled", "disabled"] : [v];
        break;
      }
      case "--fixtures":
        o.fixtures = path.resolve(value());
        break;
      case "--input":
        o.input = path.resolve(value());
        break;
      case "--results":
        o.results = path.resolve(value());
        break;
      case "--cases":
        o.cases = path.resolve(value());
        break;
      case "--seed":
        o.seed = intArg(flag, value(), 0, 0xffffffff);
        break;
      case "--count":
        o.count = intArg(flag, value(), 1, MAX_COUNT);
        break;
      case "--max-tokens":
        o.maxTokens = intArg(flag, value(), 1, 200);
        break;
      case "--chunk":
        o.chunk = intArg(flag, value(), 1, 100_000);
        break;
      case "--out":
        o.out = path.resolve(value());
        break;
      case "--keep":
        o.keep = intArg(flag, value(), 0, 100_000);
        break;
      default:
        throw new CrosscheckError(`unknown option ${show(flag)}\n${USAGE}`);
    }
  }
  return o;
}

/**
 * Evaluate one set of documents: parse5 views, candidate results, comparisons.
 *
 * @param {{ name: string, bytes: Uint8Array }[]} docs
 * @param {Map<string, Map<string, unknown>>} resultsByCandidate candidate → (doc name → result)
 * @param {string[]} views
 * @returns {{ name: string, findings: { kind: string, candidate: string, details: string[] }[], cells: Map<string, string>, p5: Map<string, import("./view.mjs").View> }[]}
 */
export function evaluateDocs(docs, resultsByCandidate, views) {
  return docs.map((doc) => {
    const p5 = new Map(views.map((v) => [v, parse5View(doc.bytes, { scripting: v === "enabled" })]));
    const findings = [];
    const cells = new Map();
    const perDoc = new Map();
    for (const [cand, results] of resultsByCandidate) {
      const r = results.get(doc.name);
      if (r === undefined) {
        findings.push({ kind: "missing", candidate: cand, details: ["no result line"] });
        cells.set(cand, "MISSING");
        continue;
      }
      perDoc.set(cand, r);
      const shape = resultShapeError(r);
      if (shape !== null) {
        findings.push({ kind: "bad-result", candidate: cand, details: [shape] });
        cells.set(cand, "BAD");
        continue;
      }
      const disagreeing = [];
      for (const [v, view] of p5) {
        const c = compareWithView(r, view);
        if (c.status === "disagree") {
          // kind: parse5-<view>-<categories>, for example parse5-enabled-blocks or parse5-disabled-manifest
          findings.push({ kind: `parse5-${v}-${c.categories.join("+")}`, candidate: cand, details: c.details });
          disagreeing.push(v);
        }
      }
      const o = /** @type {{ valid: boolean, error?: unknown, blocks: unknown[] }} */ (r);
      const base = o.valid ? `valid ${o.blocks.length}` : String(o.error).replace(/^RQP_MARKUP_/, "");
      cells.set(cand, disagreeing.length === 0 ? base : `${base} DISAGREE(${disagreeing.join(",")})`);
    }
    const mismatch = compareCandidates(perDoc);
    if (mismatch.length > 0) findings.push({ kind: "candidates", candidate: "*", details: mismatch });
    return { name: doc.name, findings, cells, p5 };
  });
}

/** Short text for a parse5 view: block count, implied ends, manifests. */
export function viewCell(view) {
  if (!view.ok) return "no tree";
  const noEnd = view.blocks.filter((b) => b.end === null).length;
  const foreign = view.blocks.filter((b) => !b.html).length;
  let s = `${view.blocks.length} block(s)`;
  if (noEnd > 0) s += ` ${noEnd} no-end`;
  if (foreign > 0) s += ` ${foreign} foreign`;
  if (view.manifests > 0) s += ` ${view.manifests} manifest`;
  return s;
}

async function candidateResults(o, dir, write) {
  /** @type {Map<string, Map<string, unknown>>} */
  const out = new Map();
  if (o.results !== null) {
    out.set("results", readResultsFile(o.results));
    return { out, failed: 0 };
  }
  let failed = 0;
  for (const c of loadCandidates(o.candidates, o.only)) {
    const r = await runBatch(c, dir, { repoRoot: REPO_ROOT, timeoutMs: o.timeoutMs });
    if (r.ok) out.set(c.name, r.results);
    else {
      failed++;
      write(`candidate ${c.name}: ${r.reason}`);
    }
  }
  return { out, failed };
}

async function buildAll(o, write) {
  if (!o.build || o.results !== null) return 0;
  let failed = 0;
  for (const c of loadCandidates(o.candidates, o.only)) {
    const reason = await buildCandidate(c, { repoRoot: REPO_ROOT, timeoutMs: o.timeoutMs });
    if (reason !== null) {
      failed++;
      write(`candidate ${c.name}: ${reason}`);
    }
  }
  return failed;
}

function printFinding(write, label, f) {
  const lines = f.details.slice(0, MAX_DETAIL_LINES);
  for (const d of lines) write(`  ${f.kind} ${label} [${f.candidate}] ${d}`);
  if (f.details.length > lines.length) write(`  ${f.kind} ${label} [${f.candidate}] ... ${f.details.length - lines.length} more`);
}

function readDocsFromDir(dir) {
  if (!existsSync(dir) || !statSync(dir).isDirectory()) throw new CrosscheckError(`not a directory: ${dir}`);
  const names = readdirSync(dir)
    .filter((n) => existsSync(path.join(dir, n, "input.html")))
    .sort((a, b) => Buffer.compare(Buffer.from(a), Buffer.from(b)));
  return names.map((name) => ({ name, bytes: readFileSync(path.join(dir, name, "input.html")) }));
}

async function cmdCheck(o, write) {
  let dir = o.fixtures;
  let tmp = null;
  if (o.input !== null) {
    let bytes;
    try {
      bytes = readFileSync(o.input);
    } catch (cause) {
      throw new CrosscheckError(`cannot read ${o.input}`, { cause });
    }
    tmp = mkdtempSync(path.join(os.tmpdir(), "rqp-crosscheck-"));
    writeBatchDir(tmp, [{ name: "input", bytes }]);
    dir = tmp;
  }
  try {
    const docs = readDocsFromDir(dir);
    let failed = await buildAll(o, write);
    const run = await candidateResults(o, dir, write);
    failed += run.failed;
    const evaluated = evaluateDocs(docs, run.out, o.views);
    let problems = 0;
    for (const e of evaluated) {
      for (const f of e.findings) {
        problems++;
        printFinding(write, e.name, f);
      }
    }
    for (const [cand, results] of run.out) {
      const valid = [...results.values()].filter((r) => r && typeof r === "object" && r.valid === true).length;
      write(`${cand}: ${docs.length} document(s), ${valid} valid`);
    }
    if (o.input !== null) {
      for (const e of evaluated) {
        write(`parse5: ${[...e.p5].map(([v, view]) => `${v} ${viewCell(view)}`).join("; ")}`);
        for (const [cand, cell] of e.cells) write(`${cand}: ${cell}`);
      }
    }
    write(`findings: ${problems}`);
    return problems === 0 && failed === 0 ? 0 : 1;
  } finally {
    if (tmp !== null) rmSync(tmp, { recursive: true, force: true });
  }
}

function loadCases(file) {
  let list;
  try {
    list = JSON.parse(readFileSync(file, "utf8"));
  } catch (cause) {
    throw new CrosscheckError(`cannot read the cases file ${file}`, { cause });
  }
  const ok =
    Array.isArray(list) &&
    list.every((c) => c && typeof c.id === "string" && /^[A-Za-z0-9_-]{1,64}$/.test(c.id) && typeof c.input === "string");
  if (!ok) throw new CrosscheckError("cases file: a list of { id, input } with ids of [A-Za-z0-9_-]");
  if (new Set(list.map((c) => c.id)).size !== list.length) throw new CrosscheckError("cases file: duplicate id");
  return list;
}

async function cmdCases(o, write) {
  const cases = loadCases(o.cases);
  const tmp = mkdtempSync(path.join(os.tmpdir(), "rqp-crosscheck-"));
  try {
    const docs = cases.map((c) => ({ name: c.id, bytes: Buffer.from(c.input, "utf8") }));
    writeBatchDir(tmp, docs);
    let failed = await buildAll(o, write);
    const run = await candidateResults(o, tmp, write);
    failed += run.failed;
    const evaluated = evaluateDocs(docs, run.out, ["enabled", "disabled"]);
    const cands = [...run.out.keys()];
    write(["case", "parse5 (scripting on)", "parse5 (scripting off)", ...cands].join(" | "));
    for (const e of evaluated) {
      const row = [e.name, viewCell(e.p5.get("enabled")), viewCell(e.p5.get("disabled"))];
      for (const c of cands) row.push(e.cells.get(c) ?? "-");
      write(row.join(" | "));
    }
    return failed === 0 ? 0 : 1;
  } finally {
    rmSync(tmp, { recursive: true, force: true });
  }
}

async function cmdSweep(o, write) {
  const outDir = o.out ?? path.join(os.tmpdir(), `rqp-crosscheck-sweep-${o.seed}`);
  const tmp = mkdtempSync(path.join(os.tmpdir(), "rqp-crosscheck-"));
  const stats = new Map();
  const kept = new Map();
  const totals = new Map();
  let failed = await buildAll(o, write);
  write(`sweep seed=${o.seed} count=${o.count} max-tokens=${o.maxTokens} chunk=${o.chunk} scripting=${o.views.join(",")}`);
  // One batch directory with fixed slot names, reused by every chunk: creating and deleting
  // directories is the slowest part on some file systems.
  const dir = path.join(tmp, "batch");
  let slots = 0;
  try {
    for (let first = 0; first < o.count; first += o.chunk) {
      const n = Math.min(o.chunk, o.count - first);
      const docs = [];
      for (let k = 0; k < n; k++) {
        const i = first + k;
        const text = generateDocument(docSeed(o.seed, i), { maxTokens: o.maxTokens });
        docs.push({ name: `s${String(k).padStart(6, "0")}`, bytes: Buffer.from(text, "utf8"), index: i });
      }
      for (let k = n; k < slots; k++) rmSync(path.join(dir, `s${String(k).padStart(6, "0")}`), { recursive: true, force: true });
      slots = n;
      writeBatchDir(dir, docs);
      const run = await candidateResults(o, dir, write);
      failed += run.failed;
      const evaluated = evaluateDocs(docs, run.out, o.views);
      evaluated.forEach((e, k) => {
        const doc = docs[k];
        for (const [cand, results] of run.out) {
          const s = stats.get(cand) ?? { docs: 0, valid: 0, withBlocks: 0, findings: 0 };
          const r = results.get(doc.name);
          s.docs++;
          if (r && typeof r === "object" && r.valid === true) {
            s.valid++;
            if (Array.isArray(r.blocks) && r.blocks.length > 0) s.withBlocks++;
          }
          s.findings += e.findings.filter((f) => f.candidate === cand).length;
          stats.set(cand, s);
        }
        for (const f of e.findings) {
          totals.set(f.kind, (totals.get(f.kind) ?? 0) + 1);
          const count = kept.get(f.kind) ?? 0;
          if (count >= o.keep) continue;
          kept.set(f.kind, count + 1);
          const label = `seed=${o.seed} index=${doc.index}`;
          printFinding(write, label, f);
          const save = path.join(outDir, `${f.kind}-${f.candidate === "*" ? "all" : f.candidate}-${o.seed}-${doc.index}`);
          mkdirSync(save, { recursive: true });
          writeFileSync(path.join(save, "input.html"), doc.bytes);
          writeFileSync(path.join(save, "finding.json"), `${JSON.stringify({ seed: o.seed, index: doc.index, maxTokens: o.maxTokens, ...f }, null, 2)}\n`);
        }
      });
    }
  } finally {
    rmSync(tmp, { recursive: true, force: true });
  }
  write("candidate | documents | valid | valid with blocks | findings");
  for (const [cand, s] of stats) write(`${cand} | ${s.docs} | ${s.valid} | ${s.withBlocks} | ${s.findings}`);
  const kinds = [...totals.entries()].map(([k, v]) => `${k}=${v}`).join(" ");
  write(`findings: ${kinds === "" ? "none" : kinds}`);
  if (totals.size > 0) write(`saved inputs: ${outDir}`);
  return totals.size === 0 && failed === 0 ? 0 : 1;
}

/**
 * @param {string[]} argv
 * @param {{ write?: (line: string) => void }} [io]
 * @returns {Promise<number>} exit code: 0 no finding, 1 findings or a candidate failed, 2 usage or input error
 */
export async function main(argv, { write = (line) => process.stdout.write(`${line}\n`) } = {}) {
  try {
    const o = parseArgs(argv);
    if (o.command === "check") return await cmdCheck(o, write);
    if (o.command === "cases") return await cmdCases(o, write);
    return await cmdSweep(o, write);
  } catch (error) {
    if (error instanceof CrosscheckError) {
      const cause = error.cause instanceof Error ? ` (${error.cause.message.split("\n")[0].slice(0, 80)})` : "";
      write(`error: ${error.message}${cause}`);
      return 2;
    }
    throw error;
  }
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  process.exitCode = await main(process.argv.slice(2));
}
