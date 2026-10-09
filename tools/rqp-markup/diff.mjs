#!/usr/bin/env node
// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// Differential runner for the RQP markup parser candidates.
// Interface: tools/rqp-markup/CONTRACT.md sections 2, 3 and 6.

import { spawn } from "node:child_process";
import { existsSync, readdirSync, readFileSync, statSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

export const HERE = path.dirname(fileURLToPath(import.meta.url));
export const REPO_ROOT = path.resolve(HERE, "..", "..");
export const DEFAULT_FIXTURES = path.join(REPO_ROOT, "fixtures", "rqp", "markup");
export const DEFAULT_CANDIDATES = path.join(HERE, "candidates.json");
const DEFAULT_TIMEOUT_S = 120;
const MAX_OUTPUT_BYTES = 256 * 1024 * 1024;
const MAX_FIELD_CHARS = 80;
const COMPARED_FIELDS = ["valid", "error", "template_revision", "page_revision", "blocks"];

export class RunnerError extends Error {
  constructor(message, options) {
    super(message, options);
    this.name = "RunnerError";
    this.code = "RUNNER_USAGE";
  }
}

/** Shorten a value for display. Never longer than MAX_FIELD_CHARS. */
export function show(value) {
  const text = JSON.stringify(value) ?? "undefined";
  return text.length <= MAX_FIELD_CHARS ? text : `${text.slice(0, MAX_FIELD_CHARS - 3)}...`;
}

function isPlainObject(v) {
  return typeof v === "object" && v !== null && !Array.isArray(v);
}

function collectDiffs(p, expected, actual, out) {
  if (Array.isArray(expected) && Array.isArray(actual)) {
    if (expected.length !== actual.length) {
      out.push(`${p}.length: expected ${expected.length}, got ${actual.length}`);
    }
    const n = Math.min(expected.length, actual.length);
    for (let i = 0; i < n; i++) collectDiffs(`${p}[${i}]`, expected[i], actual[i], out);
    return;
  }
  if (isPlainObject(expected) && isPlainObject(actual)) {
    const keys = new Set([...Object.keys(expected), ...Object.keys(actual)]);
    for (const k of [...keys].sort()) collectDiffs(`${p}.${k}`, expected[k], actual[k], out);
    return;
  }
  if (!Object.is(expected, actual)) {
    out.push(`${p}: expected ${show(expected)}, got ${show(actual)}`);
  }
}

/** Compare an actual result with expected.json. Returns detail lines; empty means equal. */
export function compareResult(expected, actual) {
  if (!isPlainObject(actual)) {
    return [`result: expected an object, got ${show(actual)}`];
  }
  const out = [];
  for (const field of COMPARED_FIELDS) {
    collectDiffs(field, expected[field], actual[field], out);
  }
  return out;
}

/** Read fixtures: every subdirectory with expected.json, in byte order of its name. */
export function loadFixtures(dir) {
  let names;
  try {
    names = readdirSync(dir);
  } catch (cause) {
    throw new RunnerError(`cannot read fixtures directory ${dir}`, { cause });
  }
  names.sort((a, b) => Buffer.compare(Buffer.from(a), Buffer.from(b)));
  const fixtures = [];
  for (const name of names) {
    const sub = path.join(dir, name);
    if (!statSync(sub).isDirectory()) continue;
    const expectedPath = path.join(sub, "expected.json");
    if (!existsSync(expectedPath)) {
      if (existsSync(path.join(sub, "input.html"))) {
        throw new RunnerError(`fixture ${name} has input.html but no expected.json`);
      }
      continue;
    }
    try {
      fixtures.push({ name, expected: JSON.parse(readFileSync(expectedPath, "utf8")) });
    } catch (cause) {
      throw new RunnerError(`cannot read expected.json of fixture ${name}`, { cause });
    }
  }
  return fixtures;
}

/** Parse and validate candidates.json content. */
export function parseCandidates(text) {
  let list;
  try {
    list = JSON.parse(text);
  } catch (cause) {
    throw new RunnerError("candidates.json is not valid JSON", { cause });
  }
  if (!Array.isArray(list)) throw new RunnerError("candidates.json must be an array");
  const isArgv = (a) => Array.isArray(a) && a.length > 0 && a.every((s) => typeof s === "string");
  for (const c of list) {
    if (
      !isPlainObject(c) ||
      typeof c.name !== "string" ||
      typeof c.cwd !== "string" ||
      !(c.build === null || isArgv(c.build)) ||
      !isArgv(c.run)
    ) {
      throw new RunnerError(`invalid candidate entry: ${show(c)}`);
    }
  }
  return list;
}

/**
 * Run argv in cwd. Never uses a shell. Resolves to
 * { kind: "exit", code, stdout, stderr } | { kind: "timeout" } | { kind: "spawn-error", error } | { kind: "overflow" }.
 */
export function runProcess(argv, cwd, timeoutMs, maxBytes = MAX_OUTPUT_BYTES) {
  return new Promise((resolve) => {
    let child;
    try {
      child = spawn(argv[0], argv.slice(1), {
        cwd,
        stdio: ["ignore", "pipe", "pipe"],
        detached: true,
      });
    } catch (error) {
      resolve({ kind: "spawn-error", error });
      return;
    }
    const out = [];
    const err = [];
    let size = 0;
    let settled = false;
    const killGroup = () => {
      try {
        process.kill(-child.pid, "SIGKILL");
      } catch {
        child.kill("SIGKILL");
      }
    };
    const finish = (value) => {
      if (settled) return;
      settled = true;
      clearTimeout(timer);
      resolve(value);
    };
    const timer = setTimeout(() => {
      killGroup();
      finish({ kind: "timeout" });
    }, timeoutMs);
    child.stdout.on("data", (chunk) => {
      size += chunk.length;
      if (size > maxBytes) {
        killGroup();
        finish({ kind: "overflow" });
        return;
      }
      out.push(chunk);
    });
    child.stderr.on("data", (chunk) => {
      if (err.length < 64) err.push(chunk);
    });
    child.on("error", (error) => finish({ kind: "spawn-error", error }));
    child.on("close", (code, signal) =>
      finish({
        kind: "exit",
        code: code ?? `signal ${signal}`,
        stdout: Buffer.concat(out).toString("utf8"),
        stderr: Buffer.concat(err).toString("utf8"),
      }),
    );
  });
}

/** First line of stderr, shortened. Candidates never print document content to stderr (CONTRACT section 2). */
export function stderrHint(stderr) {
  const line = stderr.split("\n").find((l) => l.trim() !== "");
  return line ? `: ${show(line.trim()).slice(1, -1)}` : "";
}

/** Parse batch stdout into Map(fixture -> result). Returns { results } or { crash }. */
export function parseBatchOutput(stdout) {
  const results = new Map();
  for (const line of stdout.split("\n")) {
    if (line.trim() === "") continue;
    let obj;
    try {
      obj = JSON.parse(line);
    } catch {
      return { crash: "output is not valid JSON" };
    }
    if (!isPlainObject(obj) || typeof obj.fixture !== "string") {
      return { crash: "output line has no string field fixture" };
    }
    if (!results.has(obj.fixture)) results.set(obj.fixture, obj);
  }
  return { results };
}

/**
 * Build and run one candidate.
 * Returns { state: "ok", results } | { state: "crash", reason } | { state: "notbuilt", reason }.
 */
export async function runCandidate(candidate, { repoRoot, fixturesDir, timeoutMs }) {
  const cwd = path.resolve(repoRoot, candidate.cwd);
  if (!existsSync(cwd) || !statSync(cwd).isDirectory()) {
    return { state: "notbuilt", reason: `directory ${candidate.cwd} does not exist` };
  }
  if (candidate.build !== null) {
    const b = await runProcess(candidate.build, cwd, timeoutMs);
    if (b.kind === "spawn-error") {
      return { state: "notbuilt", reason: `build command not runnable (${b.error.code ?? "error"})` };
    }
    if (b.kind === "timeout") return { state: "notbuilt", reason: "build timed out" };
    if (b.kind === "overflow") return { state: "notbuilt", reason: "build output too large" };
    if (b.code !== 0) {
      return { state: "notbuilt", reason: `build failed, exit ${b.code}${stderrHint(b.stderr)}` };
    }
  }
  const argv = candidate.run.map((a) => a.split("{dir}").join(fixturesDir));
  const r = await runProcess(argv, cwd, timeoutMs);
  if (r.kind === "spawn-error") {
    return { state: "notbuilt", reason: `run command not runnable (${r.error.code ?? "error"})` };
  }
  if (r.kind === "timeout") return { state: "crash", reason: `timeout after ${timeoutMs} ms` };
  if (r.kind === "overflow") return { state: "crash", reason: "output too large" };
  if (r.code !== 0) return { state: "crash", reason: `exit ${r.code}${stderrHint(r.stderr)}` };
  const parsed = parseBatchOutput(r.stdout);
  if (parsed.crash) return { state: "crash", reason: parsed.crash };
  return { state: "ok", results: parsed.results };
}

/** Build the report model: cells.get(fixture).get(candidate) and detail lines. */
export function evaluate(fixtures, runs) {
  const cells = new Map();
  const details = [];
  for (const f of fixtures) {
    const row = new Map();
    for (const [name, run] of runs) {
      let cell;
      if (run.state === "notbuilt") cell = "NOT BUILT";
      else if (run.state === "crash") cell = "CRASH";
      else if (!run.results.has(f.name)) cell = "MISSING";
      else {
        const diffs = compareResult(f.expected, run.results.get(f.name));
        cell = diffs.length === 0 ? "OK" : "DIFF";
        for (const d of diffs) details.push(`${f.name} [${name}] ${d}`);
      }
      row.set(name, cell);
    }
    cells.set(f.name, row);
  }
  return { cells, details };
}

/** Render the matrix as text. */
export function renderReport(fixtures, runs, { cells, details }) {
  const names = [...runs.keys()];
  const header = ["fixture", ...names];
  const rows = fixtures.map((f) => [f.name, ...names.map((n) => cells.get(f.name).get(n))]);
  const widths = header.map((h, i) => Math.max(h.length, ...rows.map((r) => r[i].length)));
  const line = (r) => r.map((c, i) => c.padEnd(widths[i])).join("  ").trimEnd();
  const lines = [line(header), ...rows.map(line)];
  const notes = [];
  for (const [name, run] of runs) {
    if (run.state !== "ok") {
      notes.push(`${name}: ${run.state === "notbuilt" ? "NOT BUILT" : "CRASH"}: ${run.reason}`);
    }
  }
  if (notes.length > 0) lines.push("", ...notes);
  if (details.length > 0) lines.push("", ...details);
  const total = rows.length * names.length;
  const ok = rows.flat().filter((c) => c === "OK").length;
  lines.push("", `${ok}/${total} OK`);
  return lines.join("\n");
}

/** True only when every cell is OK and there is at least one cell. */
export function allOk(cells) {
  let n = 0;
  for (const row of cells.values()) {
    for (const c of row.values()) {
      n++;
      if (c !== "OK") return false;
    }
  }
  return n > 0;
}

export function parseArgs(argv) {
  const opts = {
    only: null,
    fixtures: DEFAULT_FIXTURES,
    candidates: DEFAULT_CANDIDATES,
    timeoutS: DEFAULT_TIMEOUT_S,
  };
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i];
    const value = () => {
      const v = argv[++i];
      if (v === undefined) throw new RunnerError(`missing value for ${a}`);
      return v;
    };
    if (a === "--only") opts.only = value().split(",").filter((s) => s !== "");
    else if (a === "--fixtures") opts.fixtures = path.resolve(value());
    else if (a === "--candidates") opts.candidates = path.resolve(value());
    else if (a === "--timeout") {
      const t = Number(value());
      if (!Number.isFinite(t) || t <= 0) throw new RunnerError("--timeout must be a positive number of seconds");
      opts.timeoutS = t;
    } else throw new RunnerError(`unknown argument ${a}`);
  }
  return opts;
}

/**
 * Run the whole flow. Returns { exitCode, output }.
 * Exit codes: 0 all equal, 1 any difference or empty fixture set, 2 usage or I/O error.
 */
export async function main(argv, { repoRoot = REPO_ROOT } = {}) {
  try {
    const opts = parseArgs(argv);
    let list;
    try {
      list = parseCandidates(readFileSync(opts.candidates, "utf8"));
    } catch (cause) {
      if (cause instanceof RunnerError) throw cause;
      throw new RunnerError(`cannot read ${opts.candidates}`, { cause });
    }
    if (opts.only !== null) {
      const known = new Set(list.map((c) => c.name));
      const unknown = opts.only.filter((n) => !known.has(n));
      if (unknown.length > 0) throw new RunnerError(`unknown candidate: ${unknown.join(", ")}`);
      list = list.filter((c) => opts.only.includes(c.name));
    }
    if (list.length === 0) throw new RunnerError("no candidates selected");
    const fixtures = loadFixtures(opts.fixtures);
    if (fixtures.length === 0) {
      return { exitCode: 1, output: `no fixtures found in ${opts.fixtures}` };
    }
    const runs = new Map();
    for (const c of list) {
      runs.set(
        c.name,
        await runCandidate(c, { repoRoot, fixturesDir: opts.fixtures, timeoutMs: opts.timeoutS * 1000 }),
      );
    }
    const report = evaluate(fixtures, runs);
    return {
      exitCode: allOk(report.cells) ? 0 : 1,
      output: renderReport(fixtures, runs, report),
    };
  } catch (error) {
    if (error instanceof RunnerError) {
      const cause = error.cause instanceof Error ? `: ${error.cause.message}` : "";
      return { exitCode: 2, output: `error: ${error.message}${cause}` };
    }
    throw error;
  }
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const { exitCode, output } = await main(process.argv.slice(2));
  process.stdout.write(`${output}\n`);
  process.exitCode = exitCode;
}
