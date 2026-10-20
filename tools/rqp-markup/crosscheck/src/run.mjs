// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// Run parser candidates in batch mode (CONTRACT.md section 2). Process handling is shared
// with the differential runner (tools/rqp-markup/diff.mjs): argv arrays, no shell, timeout,
// bounded output, process group kill.

import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import path from "node:path";
import { parseBatchOutput, parseCandidates, runCandidate, runProcess, stderrHint } from "../../diff.mjs";

export class CrosscheckError extends Error {
  /** @param {string} message @param {{ cause?: unknown, code?: string }} [options] */
  constructor(message, options = {}) {
    super(message, { cause: options.cause });
    this.name = "CrosscheckError";
    this.code = options.code ?? "CROSSCHECK_USAGE";
  }
}

/**
 * Read candidates.json and keep the named candidates.
 *
 * @param {string} file
 * @param {string[] | null} only
 */
export function loadCandidates(file, only) {
  let text;
  try {
    text = readFileSync(file, "utf8");
  } catch (cause) {
    throw new CrosscheckError(`cannot read the candidate list ${file}`, { cause });
  }
  let list;
  try {
    list = parseCandidates(text);
  } catch (cause) {
    throw new CrosscheckError("the candidate list is not valid", { cause });
  }
  if (only === null) return list;
  for (const name of only) {
    if (!list.some((c) => c.name === name)) throw new CrosscheckError(`unknown candidate: ${name}`);
  }
  return list.filter((c) => only.includes(c.name));
}

/**
 * Run the build command of a candidate once.
 * @returns {Promise<string | null>} a reason when the build failed
 */
export async function buildCandidate(candidate, { repoRoot, timeoutMs }) {
  if (candidate.build === null) return null;
  const r = await runProcess(candidate.build, path.resolve(repoRoot, candidate.cwd), timeoutMs);
  if (r.kind !== "exit") return `build ${r.kind}`;
  if (r.code !== 0) return `build failed, exit ${r.code}${stderrHint(r.stderr)}`;
  return null;
}

/**
 * Run one candidate in batch mode on a directory, without its build step.
 * @returns {Promise<{ ok: true, results: Map<string, unknown> } | { ok: false, reason: string }>}
 */
export async function runBatch(candidate, dir, { repoRoot, timeoutMs }) {
  const r = await runCandidate({ ...candidate, build: null }, { repoRoot, fixturesDir: dir, timeoutMs });
  if (r.state === "ok") return { ok: true, results: r.results };
  return { ok: false, reason: `${r.state}: ${r.reason}` };
}

/**
 * Read pre-computed results (one JSON line per fixture, with the field "fixture").
 * @param {string} file
 * @returns {Map<string, unknown>}
 */
export function readResultsFile(file) {
  let text;
  try {
    text = readFileSync(file, "utf8");
  } catch (cause) {
    throw new CrosscheckError(`cannot read the results file ${file}`, { cause });
  }
  const parsed = parseBatchOutput(text);
  if (parsed.crash) throw new CrosscheckError(`results file: ${parsed.crash}`);
  return parsed.results;
}

/**
 * Write documents as a batch directory: <dir>/<name>/input.html.
 * @param {string} dir
 * @param {{ name: string, bytes: Uint8Array }[]} docs
 */
export function writeBatchDir(dir, docs) {
  for (const d of docs) {
    mkdirSync(path.join(dir, d.name), { recursive: true });
    writeFileSync(path.join(dir, d.name, "input.html"), d.bytes);
  }
}
