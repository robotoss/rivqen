#!/usr/bin/env node
// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// Run the markup corpus measurement (WP-17 S1 T-15, H-39).
//
// Usage: node run.mjs --data <data-dir> --evidence <evidence-dir> [--jobs 8] [--timeout-s 120]
//
// Input: <data-dir>/pages.jsonl and <data-dir>/pages/<id>/input.html (layout.mjs).
// Steps:
//   1. The four candidates in batch mode (CONTRACT.md section 2) on pages/.
//   2. The rule probe (first rule ID of the Rust candidate library) on pages/.
//   3. Per page, in a child process with a timeout: classify.mjs (all rule classes, H-35 …
//      H-38, block picks) and depth-cli.mjs (parse5 depth analysis of the cross-check tool).
//   4. Simulated blocks (DESIGN estimate) for each page that all candidates accept:
//      variant "small" (h1 and id/class matches) and variant "main" (the first main);
//      candidates, probe and classify --blocks on each variant.
//   5. The parse5 cross-check (../crosscheck, --only rust --scripting both) on pages/ and on
//      the variants.
// Output: <evidence-dir>/urls.tsv and <evidence-dir>/results.jsonl. No page content is
// written to the evidence directory: only URLs, numbers, rule ids and tag names.

import { mkdirSync, readFileSync, writeFileSync, existsSync, readdirSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { parseCandidates, runCandidate, runProcess, parseBatchOutput } from "../diff.mjs";
import { choosePicks, insertBlocks } from "./classify.mjs";

const HERE = path.dirname(fileURLToPath(import.meta.url));
const REPO_ROOT = path.resolve(HERE, "..", "..", "..");
const PROBE = path.join(HERE, "rule-probe", "target", "release", "rqp-markup-rule-probe");
const CROSSCHECK = path.resolve(HERE, "..", "crosscheck");

function parseArgs(argv) {
  const o = { data: null, evidence: null, jobs: 8, timeoutS: 120 };
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i];
    const v = argv[++i];
    if (v === undefined) throw new Error(`option ${a} needs a value`);
    if (a === "--data") o.data = path.resolve(v);
    else if (a === "--evidence") o.evidence = path.resolve(v);
    else if (a === "--jobs") o.jobs = Number(v);
    else if (a === "--timeout-s") o.timeoutS = Number(v);
    else throw new Error(`unknown option ${a}`);
  }
  if (o.data === null || o.evidence === null) throw new Error("--data and --evidence are required");
  if (!Number.isInteger(o.jobs) || o.jobs < 1 || o.jobs > 64) throw new Error("--jobs must be 1..64");
  if (!Number.isInteger(o.timeoutS) || o.timeoutS < 1) throw new Error("--timeout-s must be a positive integer");
  return o;
}

/** Run fn on each item with at most `jobs` at a time. */
async function pool(items, jobs, fn) {
  const out = new Array(items.length);
  let next = 0;
  const worker = async () => {
    while (next < items.length) {
      const i = next++;
      out[i] = await fn(items[i], i);
    }
  };
  await Promise.all(Array.from({ length: Math.min(jobs, items.length) }, worker));
  return out;
}

const log = (s) => process.stderr.write(`${s}\n`);

async function runCandidates(dir, timeoutMs) {
  const list = parseCandidates(readFileSync(path.join(HERE, "..", "candidates.json"), "utf8"));
  const out = {};
  for (const c of list) {
    const t0 = Date.now();
    const r = await runCandidate({ ...c, build: null }, { repoRoot: REPO_ROOT, fixturesDir: dir, timeoutMs });
    log(`  ${c.name}: ${r.state} in ${((Date.now() - t0) / 1000).toFixed(1)} s${r.state === "ok" ? "" : ` (${r.reason})`}`);
    out[c.name] = r.state === "ok" ? r.results : null;
  }
  return out;
}

async function runProbe(dir, timeoutMs) {
  const r = await runProcess([PROBE, dir], HERE, timeoutMs);
  if (r.kind !== "exit" || r.code !== 0) throw new Error(`rule probe failed: ${r.kind} ${r.code ?? ""}`);
  const parsed = parseBatchOutput(r.stdout);
  if (parsed.crash) throw new Error(`rule probe: ${parsed.crash}`);
  return parsed.results;
}

async function runJson(argv, timeoutMs) {
  const r = await runProcess(argv, HERE, timeoutMs, 16 * 1024 * 1024);
  if (r.kind !== "exit") return { error: r.kind };
  if (r.code !== 0) return { error: `exit ${r.code}` };
  try {
    return JSON.parse(r.stdout);
  } catch {
    return { error: "bad output" };
  }
}

/** The parse5 cross-check on a directory: finding kinds per page (no detail text). */
async function crosscheck(dir, timeoutMs) {
  const r = await runProcess(
    [process.execPath, "src/cli.mjs", "check", "--fixtures", dir, "--only", "rust", "--scripting", "both", "--timeout", String(Math.ceil(timeoutMs / 1000))],
    CROSSCHECK,
    timeoutMs * 4,
  );
  if (r.kind !== "exit") return { error: r.kind, findings: {} };
  const findings = {};
  let summary = null;
  for (const line of r.stdout.split("\n")) {
    const m = /^ {2}(\S+) (\S+) \[(\S+)\] /.exec(line);
    if (m) {
      const [, kind, name] = m;
      (findings[name] ??= new Set()).add(kind);
    }
    if (line.startsWith("findings:")) summary = line;
  }
  return { exit: r.code, summary, findings: Object.fromEntries(Object.entries(findings).map(([k, v]) => [k, [...v].sort()])) };
}

const pick = (r) => (r ? { valid: r.valid === true, error: r.error ?? null, blocks: Array.isArray(r.blocks) ? r.blocks.length : 0 } : null);

async function variant(name, pages, results, opts) {
  const dir = path.join(opts.data, `blocks-${name}`);
  if (existsSync(dir) && readdirSync(dir).length > 0) throw new Error(`${dir} is not empty`);
  mkdirSync(dir, { recursive: true });
  const made = [];
  for (const p of pages) {
    const res = results.get(p.id);
    if (!res.allValid || res.classify.picks === undefined) continue;
    const picks = choosePicks(res.classify.picks)[name];
    if (picks.length === 0) {
      res[`blocks_${name}`] = { picks: [] };
      continue;
    }
    const bytes = readFileSync(path.join(opts.data, "pages", p.id, "input.html"));
    const out = insertBlocks(bytes, picks, name === "small" ? "s" : "m");
    if (out === null) {
      res[`blocks_${name}`] = { picks: picks.map((x) => `${x.kind}:${x.name}`), error: "insert refused" };
      continue;
    }
    mkdirSync(path.join(dir, p.id));
    writeFileSync(path.join(dir, p.id, "input.html"), out);
    res[`blocks_${name}`] = { picks: picks.map((x) => `${x.kind}:${x.name}`) };
    made.push(p);
  }
  log(`variant ${name}: ${made.length} documents`);
  const cands = await runCandidates(dir, opts.timeoutS * 1000 * 10);
  const probe = await runProbe(dir, opts.timeoutS * 1000 * 10);
  await pool(made, opts.jobs, async (p) => {
    const res = results.get(p.id);
    const v = res[`blocks_${name}`];
    v.candidates = Object.fromEntries(Object.entries(cands).map(([c, m]) => [c, pick(m?.get(p.id))]));
    v.probe_rule = probe.get(p.id)?.rule ?? null;
    const cl = await runJson([process.execPath, "classify.mjs", path.join(dir, p.id, "input.html"), "--blocks"], opts.timeoutS * 1000);
    v.block_rules = cl.blockRules?.classes ?? null;
    v.block_details = cl.blockRules?.details ?? null;
    v.doc_classes = cl.classes ?? null;
  });
  const cc = await crosscheck(dir, opts.timeoutS * 1000 * 10);
  log(`crosscheck ${name}: ${cc.summary ?? cc.error}`);
  for (const p of made) results.get(p.id)[`blocks_${name}`].crosscheck = cc.findings[p.id] ?? [];
  return { documents: made.length, crosscheck: cc.summary ?? cc.error };
}

async function main(argv) {
  const opts = parseArgs(argv);
  if (!existsSync(PROBE)) throw new Error("build the rule probe first: cargo build --release in rule-probe/");
  const all = readFileSync(path.join(opts.data, "pages.jsonl"), "utf8")
    .split("\n")
    .filter((l) => l !== "")
    .map((l) => JSON.parse(l));
  const pages = all.filter((p) => p.corpus === "yes");
  const pagesDir = path.join(opts.data, "pages");
  log(`pages: ${pages.length}`);

  log("candidates on pages/");
  const cands = await runCandidates(pagesDir, opts.timeoutS * 1000 * 10);
  const probe = await runProbe(pagesDir, opts.timeoutS * 1000 * 10);

  log("classify and depth");
  const results = new Map();
  await pool(pages, opts.jobs, async (p) => {
    const file = path.join(pagesDir, p.id, "input.html");
    const argv = [process.execPath, "classify.mjs", file];
    if (p.charset) argv.push("--charset", p.charset);
    const classify = await runJson(argv, opts.timeoutS * 1000);
    const depth = await runJson([process.execPath, "depth-cli.mjs", file], opts.timeoutS * 1000);
    const c = Object.fromEntries(Object.entries(cands).map(([n, m]) => [n, pick(m?.get(p.id))]));
    const vals = Object.values(c);
    const allValid = vals.every((x) => x !== null && x.valid);
    results.set(p.id, { candidates: c, allValid, probe: probe.get(p.id) ?? null, classify, depth });
  });

  log("crosscheck on pages/");
  const cc = await crosscheck(pagesDir, opts.timeoutS * 1000 * 10);
  log(`crosscheck pages: ${cc.summary ?? cc.error}`);

  const small = await variant("small", pages, results, opts);
  const mainV = await variant("main", pages, results, opts);

  mkdirSync(opts.evidence, { recursive: true });
  const tsv = ["id\turl\tcategory\tkind\tlang\tsource\thttp_status\tbytes\ttruncated\tcorpus\tplatform"];
  for (const p of all) {
    tsv.push(
      [p.id, p.url, p.category, p.kind, p.lang, p.source, p.status ?? p.error ?? "", p.bytes, p.truncated ? "yes" : "no", p.corpus, p.platform ?? ""]
        .map((x) => String(x).replace(/[\t\n]/g, " "))
        .join("\t"),
    );
  }
  writeFileSync(path.join(opts.evidence, "urls.tsv"), `${tsv.join("\n")}\n`);

  const lines = [];
  for (const p of pages) {
    const r = results.get(p.id);
    const cl = r.classify;
    lines.push(
      JSON.stringify({
        id: p.id,
        url: p.url,
        category: p.category,
        kind: p.kind,
        lang: p.lang,
        source: p.source,
        platform: p.platform,
        bytes: p.bytes,
        truncated: p.truncated,
        charset: p.charset,
        candidates: r.candidates,
        probe_rule: r.probe?.rule ?? null,
        classify_error: cl.error ?? null,
        utf8: cl.utf8 ?? null,
        classes: cl.classes ?? null,
        details: cl.details ?? null,
        research: cl.research ?? null,
        research_details: cl.researchDetails ?? null,
        max_d: cl.maxD ?? null,
        max_units: cl.maxUnits ?? null,
        existing_blocks: cl.existingBlocks ?? null,
        parse5_depth: r.depth.error ? { error: r.depth.error } : r.depth,
        crosscheck: cc.findings[p.id] ?? [],
        blocks_small: r.blocks_small ?? null,
        blocks_main: r.blocks_main ?? null,
      }),
    );
  }
  writeFileSync(path.join(opts.evidence, "results.jsonl"), `${lines.join("\n")}\n`);
  writeFileSync(
    path.join(opts.evidence, "run.json"),
    `${JSON.stringify({ date: new Date().toISOString().slice(0, 10), pages: pages.length, crosscheck_pages: cc.summary ?? cc.error, variants: { small, main: mainV } }, null, 2)}\n`,
  );
  log("done");
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  main(process.argv.slice(2)).catch((err) => {
    process.stderr.write(`run: ${err instanceof Error ? err.message : String(err)}\n`);
    process.exit(2);
  });
}
