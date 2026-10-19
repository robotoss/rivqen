// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// Compares results/<platform>/ with parse5 (CONTRACT section 5) and writes results/<platform>/summary.md.
//   node host/compare.mjs --platform ios|android [--results DIR]
// Exit code: 0 (also when the platform differs from parse5), 1 for a malformed result file, 2 for a usage error.

import { existsSync } from "node:fs";
import { readFile, writeFile } from "node:fs/promises";
import { join, resolve, sep } from "node:path";
import { pathToFileURL } from "node:url";
import { CASES_DIR, LAB_DIR, LabError, MARKER, Q20_CASES, REPO_ROOT, parseCase } from "./lab.mjs";
import { loadParse5 } from "./parse5.mjs";
import { serializeParse5 } from "./tree.mjs";

/**
 * @typedef {object} Result
 * @property {"ok"|"unavailable"} walker
 * @property {string|null} tree
 * @property {string|null} characterSet
 * @property {string|null} probe
 * @property {string|null} error
 *
 * @typedef {{ status: "equal"|"different"|"unavailable"|"missing"|"info", detail: string }} Verdict
 * @typedef {{ id: string, view: string, question: string, verdict: Verdict }} Row
 */

/**
 * Check the shape of one result file. The text comes from an app, so it enters as unknown.
 * @param {string} text
 * @param {string} id
 * @param {string} view
 * @param {string} where  file name, for the error message
 * @returns {Result}
 */
export function parseResult(text, id, view, where) {
  const bad = (/** @type {string} */ why) => new LabError("RESULT_MALFORMED", `${where}: ${why}`);
  let raw;
  try {
    raw = JSON.parse(text);
  } catch (err) {
    throw new LabError("RESULT_MALFORMED", `${where}: not JSON`, err);
  }
  if (raw === null || typeof raw !== "object" || Array.isArray(raw)) throw bad("not an object");
  const o = /** @type {Record<string, unknown>} */ (raw);
  if (o.id !== id) throw bad(`id is ${JSON.stringify(o.id)}, expected ${JSON.stringify(id)}`);
  if (o.view !== view) throw bad(`view is ${JSON.stringify(o.view)}, expected ${JSON.stringify(view)}`);
  if (o.walker !== "ok" && o.walker !== "unavailable") throw bad("walker must be ok or unavailable");
  for (const k of ["tree", "characterSet", "probe", "error"]) {
    if (o[k] !== null && o[k] !== undefined && typeof o[k] !== "string") throw bad(`${k} must be a string or null`);
  }
  if (o.walker === "ok" && typeof o.tree !== "string") throw bad("walker ok needs a tree string");
  const str = (/** @type {unknown} */ v) => (typeof v === "string" ? v : null);
  return { walker: o.walker, tree: str(o.tree), characterSet: str(o.characterSet), probe: str(o.probe), error: str(o.error) };
}

/** @param {string} s */
function clip(s) {
  const t = s.length > 160 ? `${s.slice(0, 160)}...` : s;
  return JSON.stringify(t);
}

/**
 * The first different line of two trees.
 * @param {string} expected parse5 tree
 * @param {string} actual platform tree
 * @returns {string|null} null when equal
 */
export function firstDifference(expected, actual) {
  if (expected === actual) return null;
  const a = expected.split("\n");
  const b = actual.split("\n");
  const n = Math.max(a.length, b.length);
  for (let i = 0; i < n; i++) {
    if (a[i] !== b[i]) {
      const left = i < a.length ? clip(a[i]) : "(end of tree)";
      const right = i < b.length ? clip(b[i]) : "(end of tree)";
      return `line ${i + 1}: parse5 ${left}, platform ${right}`;
    }
  }
  return null;
}

/**
 * Q-19: the tree of the platform against the tree of parse5 in the same scripting view.
 * @param {{ parse: (html: string, options?: object) => any }} parse5
 * @param {Uint8Array} body
 * @param {"on"|"off"} view
 * @param {Result} result
 * @returns {Verdict}
 */
export function compareTree(parse5, body, view, result) {
  if (result.walker === "unavailable") return { status: "unavailable", detail: result.error ?? "no reason given" };
  const html = new TextDecoder("utf-8").decode(body); // strips a leading byte order mark, as a WebView does
  const expected = serializeParse5(parse5.parse(html, { scriptingEnabled: view === "on" }));
  const diff = firstDifference(expected, /** @type {string} */ (result.tree));
  return diff === null ? { status: "equal", detail: "" } : { status: "different", detail: diff };
}

/**
 * Q-20: the character set and the marker text.
 * @param {"utf8"|"latin1"|"info"} expect
 * @param {Result} result
 * @returns {Verdict}
 */
export function compareEncoding(expect, result) {
  if (result.walker === "unavailable") return { status: "unavailable", detail: result.error ?? "no reason given" };
  const seen = `characterSet ${clip(result.characterSet ?? "null")}, probe ${clip(result.probe ?? "null")}`;
  if (expect === "info") return { status: "info", detail: `no rule decides; ${seen}` };
  const utf8Bytes = new TextEncoder().encode(MARKER);
  const wantSets = expect === "utf8" ? ["utf-8"] : ["windows-1252", "iso-8859-1"];
  const wantProbe = expect === "utf8" ? MARKER : new TextDecoder("windows-1252").decode(utf8Bytes);
  const setOk = result.characterSet !== null && wantSets.includes(result.characterSet.toLowerCase());
  const probeOk = result.probe === wantProbe;
  if (setOk && probeOk) return { status: "equal", detail: "" };
  return { status: "different", detail: `expected ${wantSets[0]} and probe ${clip(wantProbe)}; ${seen}` };
}

/** @param {string} s */
function cell(s) {
  return s.replace(/\\/g, "\\\\").replace(/\|/g, "\\|").replace(/[\r\n]+/g, " ");
}

/**
 * @param {string} platform
 * @param {Row[]} rows
 * @returns {string}
 */
export function buildSummary(platform, rows) {
  const count = (/** @type {string} */ s) => rows.filter((r) => r.verdict.status === s).length;
  const out = [
    `# WebView lab summary: ${platform}`,
    "",
    "<!-- Generated by tools/webview-lab/host/compare.mjs. Do not edit. -->",
    "",
    `Rows: ${rows.length}. Equal: ${count("equal")}. Different: ${count("different")}. Info: ${count("info")}. Unavailable: ${count("unavailable")}. Missing: ${count("missing")}.`,
    "",
    "| Case | Question | View | Result | Detail |",
    "|---|---|---|---|---|",
  ];
  for (const r of rows) out.push(`| ${r.id} | ${r.question} | ${r.view} | ${r.verdict.status} | ${cell(r.verdict.detail)} |`);
  return `${out.join("\n")}\n`;
}

/**
 * Compare every case of the case list with the results of one platform.
 * @param {{ platform: string, resultsDir: string, casesDir?: string, repoRoot?: string, parse5?: { parse: (html: string, options?: object) => any } }} opts
 * @returns {Promise<{ rows: Row[], summary: string }>}
 */
export async function compareAll({ platform, resultsDir, casesDir = CASES_DIR, repoRoot = REPO_ROOT, parse5 }) {
  const p5 = parse5 ?? (await loadParse5());
  const index = JSON.parse(await readFile(join(casesDir, "index.json"), "utf8"));
  if (!Array.isArray(index) || index.some((x) => typeof x !== "string")) {
    throw new LabError("INDEX_INVALID", "cases/index.json is not a list of ids");
  }
  /** @type {Row[]} */
  const rows = [];
  const root = resolve(repoRoot);
  for (const id of /** @type {string[]} */ (index)) {
    const c = parseCase(JSON.parse(await readFile(join(casesDir, `${id}.json`), "utf8")), `${id}.json`);
    const file = resolve(root, c.source);
    if (!file.startsWith(root + sep)) throw new LabError("CASE_SOURCE", `${id}.json: source leaves the repository`);
    const body = await readFile(file);
    const q20 = Q20_CASES.find((q) => q.id === c.id);
    for (const view of c.views) {
      const name = `${c.id}.${view}.json`;
      const path = join(resultsDir, name);
      /** @type {Verdict} */
      let verdict;
      if (!existsSync(path)) {
        verdict = { status: "missing", detail: "no result file" };
      } else {
        const result = parseResult(await readFile(path, "utf8"), c.id, view, name);
        verdict = q20 ? compareEncoding(q20.expect, result) : compareTree(p5, body, view, result);
      }
      rows.push({ id: c.id, view, question: c.question, verdict });
    }
  }
  return { rows, summary: buildSummary(platform, rows) };
}

/** @param {string[]} argv @returns {Promise<number>} */
export async function main(argv) {
  const opt = (/** @type {string} */ name) => {
    const i = argv.indexOf(name);
    return i >= 0 ? argv[i + 1] : undefined;
  };
  const platform = opt("--platform");
  if (platform !== "ios" && platform !== "android") {
    process.stderr.write("compare: --platform ios|android is required\n");
    return 2;
  }
  const resultsDir = opt("--results") ?? join(LAB_DIR, "results", platform);
  if (!existsSync(resultsDir)) {
    process.stderr.write(`compare: ${resultsDir} does not exist\n`);
    return 2;
  }
  try {
    const { rows, summary } = await compareAll({ platform, resultsDir });
    await writeFile(join(resultsDir, "summary.md"), summary);
    process.stdout.write(`compare: ${rows.length} rows, summary in ${join(resultsDir, "summary.md")}\n`);
    return 0;
  } catch (err) {
    process.stderr.write(`compare: ${err instanceof Error ? err.message : String(err)}\n`);
    return err instanceof LabError && err.code === "RESULT_MALFORMED" ? 1 : 2;
  }
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  main(process.argv.slice(2)).then((code) => {
    process.exitCode = code;
  });
}
