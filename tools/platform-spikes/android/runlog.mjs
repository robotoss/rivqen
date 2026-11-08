// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// Run-log helper of the Android prototype (CONTRACT.md sections 5, 8, 9). Node.js 22, standard
// library only. run.sh calls it; it never talks to the device or the host.
//
//   node runlog.mjs url <capability> <run> [params]              print the page URL of a run
//   node runlog.mjs join <work> <out> <capability> <api> <k> <run> <url> <elapsed-ms>
//        joins script + app + host parts into <out>/run-<k>.jsonl and writes run-<k>.metrics.json;
//        exit 1 when the run is not ok (the files are still written)
//   node runlog.mjs runjson <work> <out> <api>                    writes <out>/run.json
//   node runlog.mjs analyze <out> <capability>                    prints the verdict facts as JSON

import { createHash } from "node:crypto";
import { execFileSync } from "node:child_process";
import { readFileSync, readdirSync, writeFileSync, existsSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { buildChunk, nonceOf } from "../host/stream.mjs";
import { normalizeRunLog, parseRunLog } from "../check/src/runs.mjs";

const HERE = path.dirname(fileURLToPath(import.meta.url));
const ROOT = path.resolve(HERE, "../../..");
const ORIGIN = "https://localhost:18443";

/**
 * Page parameters of each capability (S1: site A). gate=beacon makes the host order causal. The
 * chunk size is 2048 bytes: after its first read the WebView passes bytes on only in full 2048-byte
 * read blocks (README "Read blocks"), so a smaller chunk waits for the next one and the gate times out.
 * The gate timeout (15 s) is longer than a cold WebView start (about 8 s seen on the first run).
 * BLOCK_SCENARIO is the evidence of that condition (1500-byte chunks; not a catalog run set).
 */
export const SCENARIOS = {
  "CAP-A-01": { chunks: 8, size: 2048, gate: "beacon", timeout: 15000 },
  "CAP-A-02": { chunks: 4, size: 2048, gate: "beacon", timeout: 15000, first: 1000 },
  "CAP-A-03": { chunks: 3, size: 2048, gate: "beacon", timeout: 15000, csp: 1 },
};
export const BLOCK_SCENARIO = "chunks=3&size=1500&gate=beacon&timeout=4000";

/**
 * The page URL of a run. `override` (exploration only, run.sh SPIKE_PARAMS) replaces the scenario
 * parameters, for example `chunks=3&size=1500&gate=beacon&timeout=4000`.
 */
export function pageUrl(capability, run, override) {
  const s = SCENARIOS[capability];
  if (s === undefined) throw new Error(`unknown capability ${capability}`);
  const q = new URLSearchParams({ run });
  const params = override ? Object.fromEntries(new URLSearchParams(override)) : s;
  for (const [k, v] of Object.entries(params)) {
    if (k === "run" || !/^[a-z]{1,16}$/.test(k) || !/^[A-Za-z0-9.-]{1,32}$/.test(String(v))) throw new Error(`bad parameter ${k}`);
    q.set(k, String(v));
  }
  return `${ORIGIN}/s/stream?${q}`;
}

/** The body the host sends for the URL (host/stream.mjs): byte count and SHA-256. */
export function expectedBody(url) {
  const q = new URL(url).searchParams;
  const run = q.get("run");
  const chunks = Number(q.get("chunks") ?? 4);
  const size = Number(q.get("size") ?? 1024);
  const nonce = q.get("csp") === "1" ? nonceOf(run) : null;
  const hash = createHash("sha256");
  let bytes = 0;
  for (let k = 0; k < chunks; k += 1) {
    const b = Buffer.from(buildChunk({ k, size, run, nonce }), "utf8");
    hash.update(b);
    bytes += b.length;
  }
  return { bytes, sha256: hash.digest("hex") };
}

/** JSON with recursively sorted object keys. */
export function sortedJson(value) {
  const sort = (v) => {
    if (Array.isArray(v)) return v.map(sort);
    if (v !== null && typeof v === "object") return Object.fromEntries(Object.keys(v).sort().map((k) => [k, sort(v[k])]));
    return v;
  };
  return JSON.stringify(sort(value));
}

const readJson = (p) => JSON.parse(readFileSync(p, "utf8"));
const readLines = (p) => (existsSync(p) ? readFileSync(p, "utf8").split("\n").filter((l) => l !== "").map((l) => JSON.parse(l)) : []);

/** The WebView provider from `dumpsys webviewupdate`. */
function webviewOf(work) {
  const p = path.join(work, "webviewupdate.txt");
  const dump = existsSync(p) ? readFileSync(p, "utf8") : "";
  const m = /Current WebView package \(name, version\): \(([^,]+), ([^)]+)\)/.exec(dump);
  return m ? { package: m[1], version: m[2] } : null;
}

/**
 * Facts that the byte acceptance needs, from one run. For a stream page (host status 200) the
 * fetched and the delivered bytes must equal the body that host/stream.mjs builds (count and
 * SHA-256) and the sum of host.chunk. For another answer (a host error page) the host logs no
 * chunks, so the delivered bytes must equal the fetched bytes.
 */
export function byteChecks(events, metrics, expectedStream) {
  const app = events.filter((e) => e.src === "app");
  const host = events.filter((e) => e.src === "host");
  const hostStatus = host.find((e) => e.ev === "host.response")?.status ?? null;
  const expected = hostStatus === 200 ? expectedStream : { bytes: metrics.fetch?.bytes ?? null, sha256: metrics.fetch?.sha256 ?? null };
  const hostBytes = host.filter((e) => e.ev === "host.chunk").reduce((n, e) => n + e.bytes, 0);
  const readBytes = app.filter((e) => e.ev === "app.stream.read").reduce((n, e) => n + e.bytes, 0);
  const fetchEnd = app.find((e) => e.ev === "app.fetch.end");
  const streamed = app.some((e) => e.ev === "app.intercept" && e.decision === "stream");
  const checks = {
    streamed,
    host_status: hostStatus,
    expected_from: hostStatus === 200 ? "host/stream.mjs" : "fetched bytes",
    host_chunk_bytes: hostBytes,
    expected_bytes: expected.bytes,
    fetch_bytes: fetchEnd?.bytes ?? null,
    fetch_complete: fetchEnd?.complete ?? null,
    stream_read_bytes: readBytes,
    stream_outcome: metrics.stream?.outcome ?? null,
    stream_once: metrics.stream !== null && metrics.stream !== undefined,
    fetch_sha256_is_expected: metrics.fetch?.sha256 === expected.sha256,
    stream_sha256_is_expected: metrics.stream?.sha256 === expected.sha256,
  };
  const problems = [];
  if (!streamed) problems.push("the main frame was not streamed");
  if (hostStatus === 200 && hostBytes !== expected.bytes) problems.push(`host sent ${hostBytes} bytes, expected ${expected.bytes}`);
  if (checks.fetch_bytes !== expected.bytes || checks.fetch_complete !== true) problems.push("the fetch did not read the whole body");
  if (readBytes !== expected.bytes || metrics.stream?.bytes !== expected.bytes) problems.push(`the WebView read ${readBytes} bytes`);
  if (checks.stream_outcome !== "end") problems.push(`stream outcome ${checks.stream_outcome}`);
  if (!checks.fetch_sha256_is_expected || !checks.stream_sha256_is_expected) problems.push("SHA-256 differs from the expected body");
  if ((metrics.dropped_events ?? 0) !== 0) problems.push("app events dropped");
  return { checks, problems, expected };
}

function join([work, out, capability, api, k, run, url, elapsed]) {
  const app = readLines(path.join(work, "app.jsonl"));
  const host = readLines(path.join(work, "host.jsonl"));
  const device = existsSync(path.join(work, "device.json")) ? readJson(path.join(work, "device.json")) : {};
  const status = existsSync(path.join(work, "status.json")) ? readJson(path.join(work, "status.json")) : { state: "failed", error: "no status.json" };
  const metrics = existsSync(path.join(work, "metrics.json")) ? readJson(path.join(work, "metrics.json")) : {};
  const wv = webviewOf(work);
  const start = {
    ev: "run.start", src: "script", seq: 0, t: 0, run, capability, platform: "android", os: api,
    device: device.model ?? null, webview: wv ? `${wv.package} ${wv.version}` : null, prototype: "A-STREAM", k: Number(k),
  };
  const all = [start, ...app, ...host];
  const { checks, problems, expected } = byteChecks(all, metrics, expectedBody(url));
  if (status.state !== "done") problems.unshift(`app: ${status.error ?? status.state}`);
  const end = { ev: "run.end", src: "script", seq: 1, t: Number(elapsed), ok: problems.length === 0, error: problems.length === 0 ? null : problems.join("; ") };
  const text = [...all, end].map(sortedJson).join("\n") + "\n";
  writeFileSync(path.join(out, `run-${k}.jsonl`), text);
  const parsed = parseRunLog(text);
  if (parsed.problems.length > 0) {
    console.error(`runlog: run-${k}.jsonl: ${parsed.problems.join("; ")}`);
    process.exitCode = 1;
  }
  const metricsFile = { run, url, metrics: metrics.metrics ?? {}, fetch: metrics.fetch ?? null, stream: metrics.stream ?? null, intercept: metrics.intercept ?? null, expected, checks };
  writeFileSync(path.join(out, `run-${k}.metrics.json`), JSON.stringify(metricsFile, null, 2) + "\n");
  console.log(`runlog: ${run}: ${end.ok ? "ok" : `NOT OK: ${end.error}`}`);
  if (!end.ok) process.exitCode = 1;
}

function tomlVersion(text, key) {
  const m = new RegExp(`^${key.replace(/[-.]/g, "\\$&")}\\s*=\\s*"([^"]+)"`, "m").exec(text);
  return m ? m[1] : null;
}

function runjson([work, out, api]) {
  const device = readJson(path.join(work, "device.json"));
  const toml = readFileSync(path.join(HERE, "gradle/libs.versions.toml"), "utf8");
  const wrapper = readFileSync(path.join(HERE, "gradle/wrapper/gradle-wrapper.properties"), "utf8");
  const dump = existsSync(path.join(work, "webviewupdate.txt")) ? readFileSync(path.join(work, "webviewupdate.txt"), "utf8") : "";
  const wv = webviewOf(work);
  const code = /versionName: [^,]+, versionCode: (\d+)/.exec(dump);
  const git = (args) => execFileSync("git", ["-C", ROOT, ...args], { encoding: "utf8" }).trim();
  const javaVersion = readFileSync(path.join(work, "java-version.txt"), "utf8").trim();
  const run = {
    date: new Date().toISOString(),
    platform: "android",
    api,
    prototype: "A-STREAM",
    tools: {
      agp: tomlVersion(toml, "agp"),
      kotlin: tomlVersion(toml, "kotlin"),
      gradle: /gradle-([0-9.]+)-bin\.zip/.exec(wrapper)?.[1] ?? null,
      okhttp: tomlVersion(toml, "okhttp"),
      androidx_webkit: tomlVersion(toml, "androidx-webkit"),
      compile_sdk: tomlVersion(toml, "android-compileSdk"),
      target_sdk: tomlVersion(toml, "android-targetSdk"),
      jdk: javaVersion,
      node: process.version,
    },
    device: { model: device.model, emulator_avd: `rivqen-p2-api${api}`, image: "google_apis arm64-v8a" },
    os: { release: device.release, sdk_int: device.sdk_int, build: device.fingerprint },
    webview: { package: wv?.package ?? device.webview_package, version: wv?.version ?? device.webview_version, version_code: code ? Number(code[1]) : null },
    user_agent: device.user_agent,
    features: device.features,
    app_commit: git(["rev-parse", "HEAD"]),
    app_tree_dirty: git(["status", "--porcelain", "--", "tools/platform-spikes/android"]) !== "",
  };
  writeFileSync(path.join(out, "run.json"), JSON.stringify(run, null, 2) + "\n");
}

/** Index of the first event that matches, or -1. */
const indexOf = (events, f) => events.findIndex(f);

/** The verdict facts of one run (no timing; timing is in the metrics). */
export function facts(events) {
  const app = events.filter((e) => e.src === "app");
  const host = events.filter((e) => e.src === "host");
  const chunks = host.filter((e) => e.ev === "host.chunk").map((e) => e.k);
  const probes = app.filter((e) => e.ev === "app.probe");
  const probe = (name) => probes.find((e) => e.name === name)?.value;
  const chunkProbes = probes.filter((e) => e.name === "chunk").map((e) => e.value);
  // Host order: chunk k+1 is written only after the beacon of chunk k (gate=beacon), unless a gate timeout.
  const gated = chunks.slice(0, -1).every((k) => {
    const beacon = indexOf(host, (e) => e.ev === "host.beacon" && e.k === k);
    const next = indexOf(host, (e) => e.ev === "host.chunk" && e.k === k + 1);
    return beacon >= 0 && next > beacon;
  });
  const streamRequests = host.filter((e) => e.ev === "host.request" && /^\/s\/stream\?/.test(e.target ?? ""));
  const firstNav = indexOf(app, (e) => e.ev === "app.nav");
  const fetchStart = indexOf(app, (e) => e.ev === "app.fetch.start");
  const intercept = app.find((e) => e.ev === "app.intercept");
  const fields = Object.fromEntries((intercept?.fields ?? []).map(([n, v]) => [n.toLowerCase(), v]));
  return {
    host_chunks: chunks.length,
    chunk_probes: chunkProbes,
    all_chunk_probes: chunks.length > 0 && chunks.every((k) => chunkProbes.includes(k)),
    gate_timeouts: host.filter((e) => e.ev === "host.gate_timeout").length,
    beacon_before_next_chunk: gated,
    stream_requests: streamRequests.length,
    fetch_start_first_app_event: fetchStart === 0,
    fetch_start_before_first_nav: fetchStart >= 0 && firstNav > fetchStart,
    intercept_decision: intercept?.decision ?? null,
    intercept_status: intercept?.status ?? null,
    intercept_csp: fields["content-security-policy"] ?? null,
    page_status: probe("status") ?? null,
    page_location: probe("location") ?? null,
    page_origin: probe("origin") ?? null,
    csp_inline: probe("csp.inline") ?? null,
    // Timing (app clock, ms from the load call): chunks whose content was ready to draw before the
    // network body ended (postVisualStateCallback against the end of the early fetch).
    visual_before_stream_end: chunks.filter((k) => {
      const visual = probe(`metric.visual_chunk_${k}`);
      const end = probe("metric.fetch_end");
      return typeof visual === "number" && typeof end === "number" && visual < end;
    }),
    nav: app.filter((e) => e.ev === "app.nav").map((e) => e.kind),
    errors: app.filter((e) => e.ev === "app.error").map((e) => e.error),
    run_ok: events.at(-1)?.ok ?? false,
  };
}

function analyze([out, capability]) {
  const files = readdirSync(out).filter((f) => /^run-\d+\.jsonl$/.test(f)).sort();
  const runs = files.map((f) => {
    const parsed = parseRunLog(readFileSync(path.join(out, f), "utf8"));
    const metrics = readJson(path.join(out, f.replace(/\.jsonl$/, ".metrics.json")));
    return { file: f, events: parsed.events, problems: parsed.problems, metrics: metrics.metrics };
  });
  const normalized = runs.map((r) => JSON.stringify(normalizeRunLog(r.events)));
  const result = {
    capability,
    runs: runs.length,
    stable: normalized.length >= 3 && normalized.every((n) => n === normalized[0]),
    shape_problems: runs.flatMap((r) => r.problems.map((p) => `${r.file}: ${p}`)),
    facts: runs.map((r) => ({ file: r.file, ...facts(r.events) })),
    metrics: runs.map((r) => ({ file: r.file, ...r.metrics })),
  };
  console.log(JSON.stringify(result, null, 2));
}

const [cmd, ...args] = process.argv.slice(2);
if (cmd === "url") console.log(pageUrl(args[0], args[1], args[2]));
else if (cmd === "join") join(args);
else if (cmd === "runjson") runjson(args);
else if (cmd === "analyze") analyze(args);
else if (cmd !== undefined) {
  console.error("usage: node runlog.mjs url|join|runjson|analyze …");
  process.exit(2);
}
