// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// Writes cases/*.json from the golden markup fixtures and the Q-20 list (CONTRACT section 2.3).
//   node host/build-cases.mjs           write cases/
//   node host/build-cases.mjs --check   exit 1 when cases/ is not current

import { existsSync } from "node:fs";
import { mkdir, readdir, readFile, rm, writeFile } from "node:fs/promises";
import { join, relative } from "node:path";
import { pathToFileURL } from "node:url";
import { CASES_DIR, FIXTURES_DIR, FIXTURE_PREFIX, HOST_DIR, LabError, Q20_CASES, REPO_ROOT, UTF8_CT, isValidId } from "./lab.mjs";

const Q19_RE = /NOSCRIPT|^FX-RQ-MARKUP-EDGE-E3-|^FX-RQ-MARKUP-EDGE-E8-/;

/** @param {string} path @returns {string} repository path with forward slashes */
function repoPath(path) {
  return relative(REPO_ROOT, path).split("\\").join("/");
}

/**
 * The files that cases/ must hold: file name -> text.
 * @param {{ fixturesDir?: string, hostDir?: string, repoRoot?: string }} [opts]
 * @returns {Promise<Map<string, string>>}
 */
export async function expectedFiles({ fixturesDir = FIXTURES_DIR, hostDir = HOST_DIR, repoRoot = REPO_ROOT } = {}) {
  const rel = (/** @type {string} */ p) => relative(repoRoot, p).split("\\").join("/");
  const names = (await readdir(fixturesDir, { withFileTypes: true }))
    .filter((e) => e.isDirectory() && e.name.startsWith(FIXTURE_PREFIX) && Q19_RE.test(e.name))
    .map((e) => e.name)
    .sort();
  /** @type {object[]} */
  const cases = [];
  for (const name of names) {
    const input = join(fixturesDir, name, "input.html");
    if (!existsSync(input)) throw new LabError("FIXTURE_MISSING", `${rel(input)} does not exist`);
    cases.push({
      id: name.slice(FIXTURE_PREFIX.length),
      question: "Q-19",
      source: rel(input),
      headers: [["Content-Type", UTF8_CT]],
      views: ["on", "off"],
      probe: null,
    });
  }
  for (const q of Q20_CASES) {
    const page = join(hostDir, "pages", q.source);
    if (!existsSync(page)) throw new LabError("PAGE_MISSING", `${rel(page)} does not exist`);
    cases.push({ id: q.id, question: "Q-20", source: rel(page), headers: q.headers, views: ["on"], probe: "#m" });
  }
  const files = new Map();
  for (const c of cases) {
    const id = /** @type {{ id: string }} */ (c).id;
    if (!isValidId(id)) throw new LabError("CASE_ID", `case id ${JSON.stringify(id)} is not valid`);
    if (files.has(`${id}.json`)) throw new LabError("CASE_ID", `case id ${id} is not unique`);
    files.set(`${id}.json`, `${JSON.stringify(c, null, 2)}\n`);
  }
  files.set("index.json", `${JSON.stringify(cases.map((c) => /** @type {{ id: string }} */ (c).id), null, 2)}\n`);
  return files;
}

/**
 * Compare cases/ with the expected files.
 * @param {string} casesDir
 * @param {Map<string, string>} files
 * @returns {Promise<string[]>} the file names that differ
 */
export async function diffCases(casesDir, files) {
  /** @type {string[]} */
  const diffs = [];
  const present = existsSync(casesDir) ? (await readdir(casesDir)).filter((n) => n.endsWith(".json")) : [];
  for (const [name, text] of files) {
    if (!present.includes(name)) diffs.push(`${name}: missing`);
    else if ((await readFile(join(casesDir, name), "utf8")) !== text) diffs.push(`${name}: changed`);
  }
  for (const name of present) if (!files.has(name)) diffs.push(`${name}: stale`);
  return diffs.sort();
}

/**
 * @param {string} casesDir
 * @param {Map<string, string>} files
 */
export async function writeCases(casesDir, files) {
  await mkdir(casesDir, { recursive: true });
  for (const name of await readdir(casesDir)) {
    if (name.endsWith(".json") && !files.has(name)) await rm(join(casesDir, name));
  }
  for (const [name, text] of files) await writeFile(join(casesDir, name), text);
}

/** @param {string[]} argv @returns {Promise<number>} exit code */
export async function main(argv) {
  const check = argv.includes("--check");
  const unknown = argv.filter((a) => a !== "--check");
  if (unknown.length > 0) {
    process.stderr.write(`build-cases: unknown argument ${unknown[0]}\n`);
    return 2;
  }
  const files = await expectedFiles();
  if (check) {
    const diffs = await diffCases(CASES_DIR, files);
    if (diffs.length > 0) {
      process.stderr.write(`build-cases: ${repoPath(CASES_DIR)} is not current:\n${diffs.map((d) => `  ${d}\n`).join("")}`);
      return 1;
    }
    process.stdout.write(`build-cases: ${files.size - 1} cases current\n`);
    return 0;
  }
  await writeCases(CASES_DIR, files);
  process.stdout.write(`build-cases: wrote ${files.size - 1} cases\n`);
  return 0;
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  main(process.argv.slice(2)).then(
    (code) => {
      process.exitCode = code;
    },
    (err) => {
      process.stderr.write(`build-cases: ${err instanceof Error ? err.message : String(err)}\n`);
      process.exitCode = 1;
    },
  );
}
