// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// Run-log helper of the Android prototype (CONTRACT.md sections 5, 8, 9). Node.js 22, standard
// library only. run.sh calls it; it never talks to the device or the host.
//
//   node runlog.mjs url <capability> <run> [params] [set] [api]  print the page URL of a run
//   node runlog.mjs scenario <capability> [set]                  print the app scenario of a set
//   node runlog.mjs sets                                         print the named sets as JSON
//   node runlog.mjs join <work> <out> <capability> <api> <k> <run> <url> <elapsed-ms>
//        joins script + app + host parts into <out>/run-<k>.jsonl and writes run-<k>.metrics.json;
//        exit 1 when the run is not ok (the files are still written)
//   node runlog.mjs runjson <work> <out> <api>                    writes <out>/run.json
//   node runlog.mjs analyze <out> <capability>                    prints the verdict facts as JSON
//   node runlog.mjs same <stored-dir> <run-file>                  exit 0 when the run file has the
//        normalized log of the stored runs (at least 3, all stable); the recording mode uses it

import { createHash } from "node:crypto";
import { execFileSync } from "node:child_process";
import { readFileSync, readdirSync, writeFileSync, existsSync, mkdirSync } from "node:fs";
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

/** Site B on the bad-certificate port (CONTRACT section 1): no `adb reverse` is needed for it. */
const BAD_ORIGIN = "https://10.0.2.2:18444";

/** CAP-A-06 page: one synthetic cookie that scripts can read (no HttpOnly). */
const COOKIE_PAGE = { name: "a06-set", value: "v1", attrs: "Secure,SameSite=Lax" };

/** The final page of the redirect runs: the HTML echo, with the run id so that the host logs it. */
const echoTarget = (run) => `/s/echo?html=1&run=${run}`;

/**
 * The run sets of each capability (S2). The set "" is the catalog set (`<capability>/<api>/`); a named
 * set is a supporting set in `<capability>/<api>/<set>/` (run.sh SPIKE_SET=<set>). Members:
 * - `path`, `origin` (default site A), `params` (an object, or a function of the run id);
 * - `scenario`: the app scenario (RunParams Scenario; default `stream`);
 * - `byApi`: members that replace the others on one API level (the read block differs);
 * - `kind`: what an ok run needs (`byteChecks`): `stream` (the S1 byte checks), `pass` (the rule passed
 *   the main frame and the WebView read no byte of the early fetch), `probe` (the WebView read no byte
 *   of the early fetch; the decision is the observation), `cut` (the delivered bytes equal the bytes
 *   before the cut, the stream ended with an error, and the fallback load ran), `partial` (a `cut`
 *   run whose beacon of chunk 0 came before the cut: the delivered part reached the renderer).
 */
/**
 * WebView 91 (API 29 image) passes bytes on in 64 KiB blocks: each native read asks for the free
 * space of a 64 KiB Mojo data pipe and the stream read fills it (README "Read blocks"). So the API 29
 * stream sets use 65,536-byte chunks, as the API 37 sets use 2048-byte chunks.
 */
const BLOCK_29 = 65536;

/** `/s/cut` hold of the partial set (ms): longer than a cold WebView start (7.4 s seen) and the commit. */
const PARTIAL_HOLD_MS = 15000;

export const SETS = {
  "CAP-A-01": {
    "": {
      path: "/s/stream", params: SCENARIOS["CAP-A-01"], kind: "stream",
      byApi: { 29: { params: { chunks: 4, size: BLOCK_29, gate: "beacon", timeout: 15000 } } },
    },
    "block-1500": { path: "/s/stream", params: { chunks: 3, size: 1500, gate: "beacon", timeout: 4000 }, kind: "stream" },
    "paint-delay": {
      path: "/s/stream", params: { chunks: 3, size: 2048, gate: "delay", delay: 1500 }, kind: "stream",
      byApi: { 29: { params: { chunks: 3, size: BLOCK_29, gate: "delay", delay: 1500 } } },
    },
    // The S1 scenario on API 29: small chunks wait for the 64 KiB block or the end of the body.
    "small-2048": { path: "/s/stream", params: SCENARIOS["CAP-A-01"], kind: "stream" },
  },
  "CAP-A-02": {
    "": {
      path: "/s/stream", params: SCENARIOS["CAP-A-02"], kind: "stream",
      byApi: { 29: { params: { chunks: 2, size: BLOCK_29, gate: "beacon", timeout: 15000, first: 1000 } } },
    },
  },
  "CAP-A-03": {
    "": {
      path: "/s/stream", params: SCENARIOS["CAP-A-03"], kind: "stream",
      byApi: { 29: { params: { chunks: 2, size: BLOCK_29, gate: "beacon", timeout: 15000, csp: 1 } } },
    },
    "status-400": { path: "/s/stream", params: { chunks: 0 }, kind: "stream" },
  },
  // Redirects (CL-44, H-67): the early fetch does not follow; the rule passes a 3xx head.
  "CAP-A-04": {
    "": { path: "/s/redirect", params: (run) => ({ n: 1, code: 302, to: echoTarget(run) }), kind: "pass" },
    "cross-site": {
      path: "/s/redirect",
      params: (run) => ({ n: 1, code: 302, cross: 1, alt: "10.0.2.2", to: echoTarget(run) }),
      kind: "pass",
    },
    "chain-307": { path: "/s/redirect", params: (run) => ({ n: 2, code: 307, to: echoTarget(run) }), kind: "pass" },
    "wrr-302": { path: "/s/redirect", params: (run) => ({ n: 1, code: 302, to: echoTarget(run) }), scenario: "redirect-3xx", kind: "probe" },
  },
  // TLS error (A-03): an allowed origin with a self-signed certificate.
  "CAP-A-05": { "": { origin: BAD_ORIGIN, path: "/s/echo", params: { html: 1 }, kind: "pass" } },
  // Partial response: a cut main-frame stream (CL-33 fallback), then byte ranges of a subresource.
  "CAP-A-08": {
    "": { path: "/s/cut", params: { after: 3000, size: 8192 }, scenario: "cut-range", kind: "cut" },
    // S3 partial page: the host holds the connection for PARTIAL_HOLD_MS after the delivered bytes,
    // whose start is the chunk-0 markup (beacon k=0, chunk probe 0), then it closes. The hold starts
    // at the early fetch, before the WebView exists, so it is longer than a cold WebView start. On
    // API 29 the delivered part is larger than one 64 KiB read block, so the first block (with the
    // chunk-0 markup) reaches the renderer before the close.
    "partial": {
      path: "/s/cut", params: { after: 3000, size: 8192, hold: PARTIAL_HOLD_MS }, scenario: "cut-partial", kind: "partial",
      byApi: { 29: { params: { after: BLOCK_29 + 4464, size: 2 * BLOCK_29 + 8192, hold: PARTIAL_HOLD_MS } } },
    },
  },
  // Cookie of an intercepted response (COOKIE_INTERCEPT): one Set-Cookie, visible to scripts.
  "CAP-A-06": {
    "": { path: "/s/cookie/set", params: COOKIE_PAGE, scenario: "cookie-on", kind: "fetched" },
    "on-map": { path: "/s/cookie/set", params: COOKIE_PAGE, scenario: "cookie-on-map", kind: "fetched" },
    "off": { path: "/s/cookie/set", params: COOKIE_PAGE, scenario: "cookie-off", kind: "fetched" },
    "manual": { path: "/s/cookie/set", params: COOKIE_PAGE, scenario: "cookie-manual", kind: "fetched" },
  },
  // Cookie sync between CookieManager and the native fetch (steps without the early fetch).
  "CAP-A-07": {
    "": { path: "/s/echo", params: { html: 1 }, scenario: "cookie-sync", kind: "steps" },
    "persist": { path: "/s/echo", params: { html: 1 }, scenario: "cookie-persist", seed: "cookie-seed", kind: "steps" },
  },
};

/** The run set, or an error for an unknown capability or set. */
export function setOf(capability, set = "", api = "37") {
  const sets = SETS[capability];
  if (sets === undefined) throw new Error(`unknown capability ${capability}`);
  const s = sets[set];
  if (s === undefined) throw new Error(`unknown set ${JSON.stringify(set)} of ${capability}`);
  const { byApi, ...base } = s;
  return { origin: ORIGIN, scenario: "stream", ...base, ...(byApi?.[api] ?? {}) };
}

/**
 * The page URL of a run. `override` (exploration only, run.sh SPIKE_PARAMS) replaces the set
 * parameters, for example `chunks=3&size=1500&gate=beacon&timeout=4000`.
 */
export function pageUrl(capability, run, override, set = "", api = "37") {
  const s = setOf(capability, set, api);
  const q = new URLSearchParams({ run });
  const params = override ? Object.fromEntries(new URLSearchParams(override)) : typeof s.params === "function" ? s.params(run) : s.params;
  for (const [k, v] of Object.entries(params)) {
    const value = String(v);
    // An override is a plain value; a set value may be a path with a query (the redirect target).
    const valueOk = override ? /^[A-Za-z0-9.-]{1,32}$/.test(value) : /^[\x21-\x7e]{1,200}$/.test(value);
    if (k === "run" || !/^[a-z]{1,16}$/.test(k) || !valueOk) throw new Error(`bad parameter ${k}`);
    q.set(k, value);
  }
  return `${s.origin}${s.path}?${q}`;
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
export function byteChecks(events, metrics, expectedStream, kind = "stream") {
  if (kind === "pass" || kind === "probe") return passChecks(events, metrics, kind);
  if (kind === "cut") return cutChecks(events, metrics);
  if (kind === "partial") return partialChecks(events, metrics);
  if (kind === "fetched") return fetchedChecks(events, metrics);
  if (kind === "steps") return stepChecks(events, metrics);
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

const EMPTY_SHA256 = createHash("sha256").digest("hex");

/**
 * Checks of a streamed page that is not built by host/stream.mjs (CAP-A-06 cookie page): the first
 * main-frame request streamed; the fetch read the whole body; the WebView read the same bytes (count
 * and SHA-256) and the end.
 */
export function fetchedChecks(events, metrics) {
  const app = events.filter((e) => e.src === "app");
  const intercepts = app.filter((e) => e.ev === "app.intercept");
  const fetchEnd = app.find((e) => e.ev === "app.fetch.end");
  const readBytes = app.filter((e) => e.ev === "app.stream.read").reduce((n, e) => n + e.bytes, 0);
  const checks = {
    decisions: intercepts.map((e) => e.decision),
    fetch_bytes: fetchEnd?.bytes ?? null,
    fetch_complete: fetchEnd?.complete ?? null,
    stream_read_bytes: readBytes,
    stream_outcome: metrics.stream?.outcome ?? null,
    stream_sha256_is_fetch: metrics.stream?.sha256 !== undefined && metrics.stream?.sha256 === metrics.fetch?.sha256,
  };
  const problems = [];
  if (intercepts[0]?.decision !== "stream") problems.push("the main frame was not streamed");
  if (checks.fetch_complete !== true || !(checks.fetch_bytes > 0)) problems.push("the fetch did not read the whole body");
  if (readBytes !== checks.fetch_bytes || metrics.stream?.bytes !== checks.fetch_bytes || !checks.stream_sha256_is_fetch) {
    problems.push(`the WebView read ${readBytes} bytes, the fetch ${checks.fetch_bytes}`);
  }
  if (checks.stream_outcome !== "end") problems.push(`stream outcome ${checks.stream_outcome}`);
  if ((metrics.dropped_events ?? 0) !== 0) problems.push("app events dropped");
  return { checks, problems, expected: { bytes: checks.fetch_bytes, sha256: metrics.fetch?.sha256 ?? null } };
}

/** Checks of a step run (CAP-A-07): the last step is `done`, no app error, nothing dropped. */
export function stepChecks(events, metrics) {
  const app = events.filter((e) => e.src === "app");
  const steps = app.filter((e) => e.ev === "app.probe" && e.name === "native.step").map((e) => e.value);
  const errors = app.filter((e) => e.ev === "app.error").map((e) => e.error);
  const checks = { steps, errors };
  const problems = [];
  if (steps.at(-1) !== "done") problems.push(`steps did not end: ${steps.join(",")}`);
  if (errors.length > 0) problems.push(`app errors: ${errors.join("; ")}`);
  if ((metrics.dropped_events ?? 0) !== 0) problems.push("app events dropped");
  return { checks, problems, expected: { bytes: null, sha256: null } };
}

/**
 * Checks of a run whose main frame the WebView loads itself (redirect, TLS error). `pass`: every
 * main-frame decision is `pass`. Both kinds: the WebView read no byte of the early fetch (the reader
 * side was closed unread, outcome `cancel`), and the fetch delivered nothing.
 */
export function passChecks(events, metrics, kind) {
  const app = events.filter((e) => e.src === "app");
  const decisions = app.filter((e) => e.ev === "app.intercept").map((e) => e.decision);
  const readBytes = app.filter((e) => e.ev === "app.stream.read").reduce((n, e) => n + e.bytes, 0);
  const checks = {
    decisions,
    stream_read_bytes: readBytes,
    stream_outcome: metrics.stream?.outcome ?? null,
    stream_bytes: metrics.stream?.bytes ?? null,
    stream_sha256_is_empty: metrics.stream?.sha256 === EMPTY_SHA256,
  };
  const problems = [];
  if (decisions.length === 0) problems.push("no main-frame decision");
  if (kind === "pass" && decisions.some((d) => d !== "pass")) problems.push(`decisions ${decisions.join(",")}`);
  if (readBytes !== 0 || checks.stream_bytes !== 0 || !checks.stream_sha256_is_empty) problems.push(`the WebView read ${readBytes} bytes of the early fetch`);
  if (checks.stream_outcome !== "cancel") problems.push(`stream outcome ${checks.stream_outcome}`);
  if ((metrics.dropped_events ?? 0) !== 0) problems.push("app events dropped");
  return { checks, problems, expected: { bytes: 0, sha256: EMPTY_SHA256 } };
}

/**
 * Checks of a cut run (CAP-A-08): the first main-frame request streamed; the fetch read exactly the
 * bytes before the cut (`host.cut.after`) and did not complete; the WebView read the same bytes
 * (count and SHA-256) and the stream ended with an error; the app made the fallback load, whose
 * main-frame request passed (`claimed`).
 */
export function cutChecks(events, metrics) {
  const app = events.filter((e) => e.src === "app");
  const host = events.filter((e) => e.src === "host");
  const intercepts = app.filter((e) => e.ev === "app.intercept");
  const cuts = host.filter((e) => e.ev === "host.cut").map((e) => e.after);
  const fetchEnd = app.find((e) => e.ev === "app.fetch.end");
  const readBytes = app.filter((e) => e.ev === "app.stream.read").reduce((n, e) => n + e.bytes, 0);
  const steps = app.filter((e) => e.ev === "app.probe" && e.name === "native.step").map((e) => e.value);
  const checks = {
    decisions: intercepts.map((e) => `${e.decision}${e.reason ? `:${e.reason}` : ""}`),
    host_cuts: cuts,
    fetch_bytes: fetchEnd?.bytes ?? null,
    fetch_complete: fetchEnd?.complete ?? null,
    stream_read_bytes: readBytes,
    stream_outcome: metrics.stream?.outcome ?? null,
    stream_sha256_is_fetch: metrics.stream?.sha256 !== undefined && metrics.stream?.sha256 === metrics.fetch?.sha256,
    steps,
  };
  const problems = [];
  if (intercepts[0]?.decision !== "stream") problems.push("the first main-frame request was not streamed");
  if (!(intercepts.length >= 2 && intercepts[1].decision === "pass" && intercepts[1].reason === "claimed")) {
    problems.push("no fallback request that passed as claimed");
  }
  if (cuts.length === 0 || checks.fetch_bytes !== cuts[0] || checks.fetch_complete !== false) problems.push("the fetch did not end at the cut");
  if (readBytes !== checks.fetch_bytes || metrics.stream?.bytes !== checks.fetch_bytes || !checks.stream_sha256_is_fetch) {
    problems.push(`the WebView read ${readBytes} bytes, the fetch ${checks.fetch_bytes}`);
  }
  if (checks.stream_outcome !== "error") problems.push(`stream outcome ${checks.stream_outcome}`);
  if (!steps.includes("fallback-load")) problems.push("no fallback load");
  if ((metrics.dropped_events ?? 0) !== 0) problems.push("app events dropped");
  return { checks, problems, expected: { bytes: checks.fetch_bytes, sha256: metrics.fetch?.sha256 ?? null } };
}

/**
 * Checks of a partial-page run (CAP-A-08 `partial`): the checks of a cut run, and the delivered part
 * reached the renderer before the host closed the connection: the beacon of chunk 0 (in the first
 * bytes of the body) came before the first `host.cut` in host order.
 */
export function partialChecks(events, metrics) {
  const result = cutChecks(events, metrics);
  const host = events.filter((e) => e.src === "host");
  result.checks.beacon_before_cut = beaconBeforeCut(host);
  if (!result.checks.beacon_before_cut) result.problems.push("no beacon of chunk 0 before the cut: the delivered part did not reach the renderer");
  return result;
}

/** True when `host.beacon` k=0 comes before the first `host.cut` (host events in host order). */
function beaconBeforeCut(host) {
  const beacon = host.findIndex((e) => e.ev === "host.beacon" && e.k === 0);
  const cut = host.findIndex((e) => e.ev === "host.cut");
  return beacon >= 0 && cut > beacon;
}

function join([work, out, capability, api, k, run, url, elapsed, set = ""]) {
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
  const { kind } = setOf(capability, set, api);
  const { checks, problems, expected } = byteChecks(all, metrics, kind === "stream" ? expectedBody(url) : null, kind);
  if (status.state !== "done") problems.unshift(`app: ${status.error ?? status.state}`);
  const end = { ev: "run.end", src: "script", seq: 1, t: Number(elapsed), ok: problems.length === 0, error: problems.length === 0 ? null : problems.join("; ") };
  const text = [...all, end].map(sortedJson).join("\n") + "\n";
  // A run that is not ok never goes into the evidence folder (it would replace a good run of the same k).
  if (!end.ok) {
    out = path.join(process.env.LAB_OUT ?? work, "android-failed", capability, api, ...(set ? [set] : []));
    mkdirSync(out, { recursive: true });
  }
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

function runjson([work, out, api, set = "", scenario = "stream"]) {
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
    set: set === "" ? null : set,
    scenario,
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
    // The trigger is the chunk probe, or the chunk beacon when the page has no document-start channel.
    visual_before_stream_end: chunks.filter((k) => {
      const visual = probe(`metric.visual_chunk_${k}`) ?? probe(`metric.visual_beacon_${k}`);
      const end = probe("metric.fetch_end");
      return typeof visual === "number" && typeof end === "number" && visual < end;
    }),
    nav: app.filter((e) => e.ev === "app.nav").map((e) => e.kind),
    errors: app.filter((e) => e.ev === "app.error").map((e) => e.error),
    // S2 facts: redirect, TLS error, partial response.
    fetch_head_status: app.find((e) => e.ev === "app.fetch.head")?.status ?? null,
    intercepts: app.filter((e) => e.ev === "app.intercept").map((e) => ({
      decision: e.decision, reason: e.reason ?? null, redirect: e.redirect ?? null, status: e.status ?? null, url: e.url,
    })),
    navs: app.filter((e) => e.ev === "app.nav").map((e) => ({ kind: e.kind, url: e.url, error: e.error ?? null })),
    ssl_errors: app.filter((e) => e.ev === "app.ssl_error").map((e) => ({ url: e.url, primary_error: e.primary_error, decision: e.decision })),
    probes: Object.fromEntries(probes.filter((e) => !e.name.startsWith("metric.") && e.name !== "chunk").map((e) => [e.name, e.value])),
    native: probes.filter((e) => e.name.startsWith("native.")).map((e) => [e.name, e.value]),
    host_requests: host.filter((e) => e.ev === "host.request").map((e) => `${e.site} ${e.target}${e.range ? ` [${e.range}]` : ""}`),
    host_statuses: host.filter((e) => e.ev === "host.response").map((e) => e.status),
    host_cuts: host.filter((e) => e.ev === "host.cut").map((e) => e.after),
    // S3 partial page: host order of the beacons and the cuts, and whether chunk 0 reached the renderer first.
    host_beacons_and_cuts: host.filter((e) => e.ev === "host.beacon" || e.ev === "host.cut").map((e) => (e.ev === "host.cut" ? `cut ${e.after}` : `beacon ${e.k}`)),
    beacon_before_cut: beaconBeforeCut(host),
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

/**
 * Whether one run log (text) has the normalized log of the stored runs (texts). The stored runs
 * must be at least 3 and stable among themselves (CONTRACT §7, §8). Runs stored with the S1 app
 * have no `reason` and `redirect` members on `app.intercept` (S2 lane members, CONTRACT §5); then
 * those two members of the run are not compared, and `note` says so.
 * @param {string[]} stored @param {string} run
 * @returns {{ same: boolean, reason: string | null, note: string | null }}
 */
export function sameAsStored(stored, run) {
  if (stored.length < 3) return { same: false, reason: `${stored.length} stored runs, at least 3 needed`, note: null };
  const parse = (text) => {
    const parsed = parseRunLog(text);
    if (parsed.problems.length > 0) throw new Error(parsed.problems.join("; "));
    return parsed.events;
  };
  const storedEvents = stored.map(parse);
  const s1 = !storedEvents.some((events) => events.some((e) => e.ev === "app.intercept" && ("reason" in e || "redirect" in e)));
  const runEvents = parse(run).map((e) => {
    if (!s1 || e.ev !== "app.intercept") return e;
    const { reason: _reason, redirect: _redirect, ...rest } = e;
    return rest;
  });
  const base = storedEvents.map((events) => JSON.stringify(normalizeRunLog(events)));
  const note = s1 ? "stored runs of the S1 app: app.intercept reason and redirect not compared" : null;
  if (!base.every((n) => n === base[0])) return { same: false, reason: "the stored runs are not stable", note };
  if (JSON.stringify(normalizeRunLog(runEvents)) !== base[0]) return { same: false, reason: "the run differs from the stored runs", note };
  return { same: true, reason: null, note };
}

function same([dir, file]) {
  const stored = readdirSync(dir).filter((f) => /^run-\d+\.jsonl$/.test(f)).sort().map((f) => readFileSync(path.join(dir, f), "utf8"));
  const r = sameAsStored(stored, readFileSync(file, "utf8"));
  console.log(r.same ? `runlog: ${path.basename(file)} has the log of the ${stored.length} stored runs` : `runlog: NOT SAME: ${r.reason}`);
  if (r.note !== null) console.log(`runlog: note: ${r.note}`);
  if (!r.same) process.exitCode = 1;
}

const [cmd, ...args] = process.argv.slice(2);
if (cmd === "url") console.log(pageUrl(args[0], args[1], args[2] || undefined, args[3] ?? "", args[4] ?? "37"));
else if (cmd === "scenario") console.log(setOf(args[0], args[1] ?? "").scenario);
else if (cmd === "seed") console.log(setOf(args[0], args[1] ?? "").seed ?? "");
else if (cmd === "sets") console.log(JSON.stringify(Object.fromEntries(Object.entries(SETS).map(([c, s]) => [c, Object.keys(s)]))));
else if (cmd === "join") join(args);
else if (cmd === "runjson") runjson(args);
else if (cmd === "analyze") analyze(args);
else if (cmd === "same") same(args);
else if (cmd !== undefined) {
  console.error("usage: node runlog.mjs url|scenario|sets|join|runjson|analyze|same …");
  process.exit(2);
}
