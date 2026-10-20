#!/usr/bin/env node
// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// Fetch the pages of the markup corpus (WP-17 S1 T-15, H-39).
//
// Usage:
//   node fetch.mjs --list url-list.tsv --out <data-dir> [--concurrency 12] [--delay-ms 1000]
//                  [--timeout-ms 15000] [--max-bytes 5242880]
//
// Phase 1 fetches every listed URL. Phase 2 discovers one same-host inner page for each
// row with the kind "home+X" from the links of the fetched home page, then fetches it.
// The response bytes (after HTTP decompression, before any parsing) go to
// <data-dir>/raw/<id>.bin; one JSON line per URL goes to <data-dir>/fetch.jsonl.
//
// The fetched bytes are untrusted data. This script never executes them; it only reads
// href values with a bounded scan (discoverLink).
//
// Politeness: one request per host at a time, a delay between two requests to the same
// host, a timeout per request, and a size limit per page (the read stops after limit + 1
// bytes, so a page larger than the limit is kept as limit + 1 bytes and marked truncated).

import { mkdirSync, readFileSync, writeFileSync, appendFileSync, existsSync, readdirSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

export const MAX_BYTES = 5 * 1024 * 1024;
export const USER_AGENT =
  "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) " +
  "Chrome/129.0 Safari/537.36 RivqenMarkupCorpusTest/1.0 (one-off markup test client)";

export class CorpusError extends Error {
  /** @param {string} message @param {{ cause?: unknown, code?: string }} [options] */
  constructor(message, options = {}) {
    super(message, { cause: options.cause });
    this.name = "CorpusError";
    this.code = options.code ?? "CORPUS_USAGE";
  }
}

/**
 * Parse the URL list. Lines that start with "#" and empty lines are skipped.
 * @param {string} text
 * @returns {{ id: string, url: string, category: string, kind: string, discover: string | null, lang: string }[]}
 */
export function parseList(text) {
  const rows = [];
  const lines = text.split("\n");
  for (let n = 0; n < lines.length; n++) {
    const line = lines[n].replace(/\r$/, "");
    if (line.trim() === "" || line.startsWith("#")) continue;
    const cols = line.split("\t");
    if (cols.length !== 4) throw new CorpusError(`url list line ${n + 1}: expected 4 columns, got ${cols.length}`);
    const [url, category, kindCol, lang] = cols;
    let parsed;
    try {
      parsed = new URL(url);
    } catch (cause) {
      throw new CorpusError(`url list line ${n + 1}: not a URL`, { cause });
    }
    if (parsed.protocol !== "https:" && parsed.protocol !== "http:") {
      throw new CorpusError(`url list line ${n + 1}: only http and https`);
    }
    if (parsed.search !== "" || parsed.username !== "" || parsed.password !== "") {
      throw new CorpusError(`url list line ${n + 1}: no query string and no credentials allowed`);
    }
    const [kind, discover = null] = kindCol.split("+");
    if (!/^[a-z-]+$/.test(category) || !/^[a-z-]+$/.test(kind) || (discover !== null && !/^[a-z-]+$/.test(discover))) {
      throw new CorpusError(`url list line ${n + 1}: bad category or kind`);
    }
    rows.push({ id: `p${String(rows.length + 1).padStart(4, "0")}`, url, category, kind, discover, lang });
  }
  return rows;
}

const SKIP_PATH =
  /(^|\/)(login|log-in|signin|sign-in|signup|sign-up|register|account|accounts|my|profile|cart|basket|checkout|bag|wishlist|favorites|search|auth|oauth|logout|privacy|terms|cookies?|cookie-policy|legal|imprint|impressum|contact|contacts|subscribe|subscription|newsletter|rss|feed|help|support|faq|careers|jobs|about|tag|tags|author|authors|user|users|u)(\/|$)/i;
const SKIP_EXT = /\.(pdf|jpe?g|png|gif|webp|svg|ico|css|js|mjs|json|xml|rss|atom|zip|gz|mp4|mp3|webm|woff2?|ttf|apk|exe|dmg|txt)$/i;

const KIND_SCORE = {
  article: (p) => articleScore(p),
  post: (p) => articleScore(p),
  product: (p) => (/(\/products?\/|\/p\/|\/dp\/|\/item\/|\/goods\/|\/tovar|\/product-|\/sku\/|\/catalog\/[^/]+\/[^/]+)/i.test(p) ? 3 : 0),
  listing: (p) => (/\/(category|categories|catalog|catalogue|collections?|c|shop|section|rubric|rubrics|departments?|browse|men|women|sale|new)(\/|$)/i.test(p) ? 3 : 0),
  thread: (p) => (/\/(t|topic|topics|thread|threads|questions|q|s|comments|showthread|viewtopic)(\/|\.|$)/i.test(p) ? 3 : articleScore(p) > 0 ? 1 : 0),
  docs: (p) => (segments(p).length >= 1 ? 1 : 0),
};

function segments(p) {
  return p.split("/").filter((s) => s !== "");
}

function articleScore(p) {
  const segs = segments(p);
  if (segs.length === 0) return 0;
  const last = segs[segs.length - 1].replace(/\.(html?|shtml|php|aspx?)$/i, "");
  let score = 0;
  if ((last.match(/-/g) ?? []).length >= 3) score += 2;
  if (last.length >= 25) score += 1;
  if (/\/20\d\d\/|\/20\d\d-\d\d|\d{5,}/.test(p)) score += 1;
  if (segs.length >= 2) score += 1;
  return score >= 2 ? score : 0;
}

/**
 * Find one same-host inner link of the given kind in the home page bytes.
 * Bounded: reads at most the first 2 MiB and 5000 href values. Never decodes the page
 * beyond Latin-1 (URLs are ASCII or percent-encoded).
 * @param {Uint8Array} bytes
 * @param {string} baseUrl the final URL of the home page
 * @param {string} kind
 * @returns {string | null}
 */
export function discoverLink(bytes, baseUrl, kind) {
  const score = KIND_SCORE[kind];
  if (score === undefined) return null;
  const text = Buffer.from(bytes.subarray(0, 2 * 1024 * 1024)).toString("latin1");
  const base = new URL(baseUrl);
  const host = base.hostname.replace(/^www\./, "");
  const re = /\shref\s*=\s*(?:"([^"]{1,2000})"|'([^']{1,2000})'|([^\s"'>]{1,2000}))/gi;
  let best = null;
  let bestScore = 0;
  let seen = 0;
  for (const m of text.matchAll(re)) {
    if (++seen > 5000) break;
    const raw = (m[1] ?? m[2] ?? m[3] ?? "").replace(/&amp;/g, "&").trim();
    if (raw === "" || raw.startsWith("#") || /^(mailto|tel|javascript|data):/i.test(raw)) continue;
    let u;
    try {
      u = new URL(raw, base);
    } catch {
      continue; // Not a URL: skip this link only.
    }
    if (u.protocol !== "https:" && u.protocol !== "http:") continue;
    if (u.hostname.replace(/^www\./, "") !== host) continue;
    if (u.search !== "" || u.username !== "" || u.password !== "") continue;
    u.hash = "";
    const p = u.pathname;
    if (p === base.pathname || p === "/" || SKIP_PATH.test(p) || SKIP_EXT.test(p)) continue;
    const s = score(p);
    if (s > bestScore) {
      best = u.href;
      bestScore = s;
    }
  }
  return best;
}

/** The charset parameter of a Content-Type header value, lower case, or null. */
export function charsetOf(contentType) {
  const m = /;\s*charset\s*=\s*"?([A-Za-z0-9._:-]{1,40})"?/i.exec(contentType ?? "");
  return m ? m[1].toLowerCase() : null;
}

/**
 * Fetch one URL. Returns the metadata and the bytes (or null).
 * @param {string} url
 * @param {{ timeoutMs: number, maxBytes: number }} options
 */
export async function fetchOne(url, { timeoutMs, maxBytes }) {
  const started = Date.now();
  const meta = {
    status: null,
    final_host: null,
    final_url: null,
    redirected: false,
    content_type: null,
    content_encoding: null,
    charset: null,
    bytes: 0,
    truncated: false,
    error: null,
    ms: 0,
  };
  let bytes = null;
  try {
    const res = await fetch(url, {
      redirect: "follow",
      signal: AbortSignal.timeout(timeoutMs),
      headers: {
        "user-agent": USER_AGENT,
        accept: "text/html,application/xhtml+xml;q=0.9,*/*;q=0.5",
        "accept-language": "en,ru;q=0.8,*;q=0.5",
        "accept-encoding": "gzip, deflate, br",
      },
    });
    meta.status = res.status;
    meta.redirected = res.redirected;
    const finalUrl = new URL(res.url);
    meta.final_host = finalUrl.hostname;
    // Kept without query string and fragment (no session data).
    meta.final_url = `${finalUrl.origin}${finalUrl.pathname}`;
    meta.content_type = res.headers.get("content-type");
    meta.content_encoding = res.headers.get("content-encoding");
    meta.charset = charsetOf(meta.content_type);
    if (res.body === null) {
      bytes = new Uint8Array(0);
    } else {
      const reader = res.body.getReader();
      const chunks = [];
      let total = 0;
      for (;;) {
        const { done, value } = await reader.read();
        if (done) break;
        const room = maxBytes + 1 - total;
        if (value.length >= room) {
          chunks.push(value.subarray(0, room));
          total += room;
          meta.truncated = true;
          await reader.cancel();
          break;
        }
        chunks.push(value);
        total += value.length;
      }
      bytes = Buffer.concat(chunks, total);
    }
    meta.bytes = bytes.length;
  } catch (cause) {
    const name = cause instanceof Error ? cause.name : "Error";
    const code = cause instanceof Error && cause.cause instanceof Error ? (cause.cause.code ?? cause.cause.name) : null;
    meta.error = name === "TimeoutError" ? "timeout" : `${name}${code ? `:${code}` : ""}`;
    bytes = null;
  }
  meta.ms = Date.now() - started;
  return { meta, bytes };
}

/**
 * Run tasks with a global concurrency limit, one task per host at a time, and a delay
 * between two tasks of the same host.
 * @template T
 * @param {{ host: string, run: () => Promise<T> }[]} tasks
 * @param {{ concurrency: number, delayMs: number, now?: () => number, sleep?: (ms: number) => Promise<void> }} options
 * @returns {Promise<T[]>} results in task order
 */
export async function runPolite(tasks, { concurrency, delayMs, now = Date.now, sleep = defaultSleep, lastEnd = new Map() }) {
  if (tasks.length === 0) return [];
  const results = new Array(tasks.length);
  const pending = tasks.map((t, i) => ({ ...t, i }));
  const busy = new Set();
  let active = 0;
  let finished = 0;
  let timer = false;
  let resolveAll;
  let rejectAll;
  const done = new Promise((res, rej) => {
    resolveAll = res;
    rejectAll = rej;
  });

  const pump = () => {
    while (active < concurrency && pending.length > 0) {
      const t = now();
      let pick = -1;
      let wait = Infinity;
      for (let k = 0; k < pending.length; k++) {
        const host = pending[k].host;
        if (busy.has(host)) continue;
        const ready = (lastEnd.get(host) ?? -Infinity) + delayMs;
        if (ready <= t) {
          pick = k;
          break;
        }
        wait = Math.min(wait, ready - t);
      }
      if (pick < 0) {
        // Every free host is in its delay: wake up when the first one is ready.
        if (wait !== Infinity && !timer) {
          timer = true;
          sleep(wait).then(() => {
            timer = false;
            pump();
          }, rejectAll);
        }
        return;
      }
      const task = pending.splice(pick, 1)[0];
      busy.add(task.host);
      active++;
      task.run().then(
        (r) => {
          results[task.i] = r;
          busy.delete(task.host);
          lastEnd.set(task.host, now());
          active--;
          finished++;
          if (finished === tasks.length) resolveAll(results);
          else pump();
        },
        (err) => rejectAll(err),
      );
    }
  };
  pump();
  return done;
}

function defaultSleep(ms) {
  return new Promise((r) => setTimeout(r, ms));
}

function parseArgs(argv) {
  const opts = { list: null, out: null, concurrency: 12, delayMs: 1000, timeoutMs: 15000, maxBytes: MAX_BYTES };
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i];
    const v = argv[i + 1];
    if (v === undefined) throw new CorpusError(`option ${a} needs a value`);
    i++;
    if (a === "--list") opts.list = v;
    else if (a === "--out") opts.out = v;
    else if (a === "--concurrency") opts.concurrency = intArg(a, v, 1, 64);
    else if (a === "--delay-ms") opts.delayMs = intArg(a, v, 0, 60000);
    else if (a === "--timeout-ms") opts.timeoutMs = intArg(a, v, 1000, 120000);
    else if (a === "--max-bytes") opts.maxBytes = intArg(a, v, 1, MAX_BYTES);
    else throw new CorpusError(`unknown option ${a}`);
  }
  if (opts.list === null || opts.out === null) throw new CorpusError("--list and --out are required");
  return opts;
}

function intArg(name, v, min, max) {
  const n = Number(v);
  if (!Number.isInteger(n) || n < min || n > max) throw new CorpusError(`${name} must be an integer in ${min}..${max}`);
  return n;
}

const REPO_ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../../..");

/** Page bytes are copyrighted data: refuse an output directory inside the repository (final WP-17 security review I2). */
export function outsideRepo(out, repoRoot = REPO_ROOT) {
  const resolved = path.resolve(out);
  const rel = path.relative(path.resolve(repoRoot), resolved);
  const outside = rel === ".." || rel.startsWith(`..${path.sep}`) || path.isAbsolute(rel);
  if (!outside) {
    throw new CorpusError(`${resolved} is inside the repository: use a directory outside it`);
  }
  return resolved;
}

async function main(argv) {
  const opts = parseArgs(argv);
  const rows = parseList(readFileSync(opts.list, "utf8"));
  const out = outsideRepo(opts.out);
  const rawDir = path.join(out, "raw");
  if (existsSync(rawDir) && readdirSync(rawDir).length > 0) {
    throw new CorpusError(`${rawDir} is not empty: use a new directory`);
  }
  mkdirSync(rawDir, { recursive: true });
  const log = path.join(out, "fetch.jsonl");
  writeFileSync(log, "");

  const save = (entry, result) => {
    if (result.bytes !== null) writeFileSync(path.join(rawDir, `${entry.id}.bin`), result.bytes);
    const line = { ...entry, ...result.meta };
    appendFileSync(log, `${JSON.stringify(line)}\n`);
    return line;
  };
  const task = (entry) => ({
    host: new URL(entry.url).hostname,
    run: async () => save(entry, await fetchOne(entry.url, opts)),
  });

  const listed = rows.map((r) => ({ id: r.id, url: r.url, category: r.category, kind: r.kind, lang: r.lang, source: "listed", from: null }));
  process.stderr.write(`phase 1: ${listed.length} listed URLs\n`);
  const lastEnd = new Map();
  const first = await runPolite(listed.map(task), { ...opts, lastEnd });

  const second = [];
  const notFound = [];
  for (const r of rows) {
    if (r.discover === null) continue;
    const home = first.find((x) => x.id === r.id);
    if (!home || home.status !== 200 || !existsSync(path.join(rawDir, `${r.id}.bin`))) {
      notFound.push({ id: `${r.id}d`, from: r.id, reason: "home not fetched" });
      continue;
    }
    const link = discoverLink(readFileSync(path.join(rawDir, `${r.id}.bin`)), home.final_url ?? r.url, r.discover);
    if (link === null) {
      notFound.push({ id: `${r.id}d`, from: r.id, reason: `no ${r.discover} link` });
      continue;
    }
    second.push({ id: `${r.id}d`, url: link, category: r.category, kind: r.discover, lang: r.lang, source: "discovered", from: r.id });
  }
  process.stderr.write(`phase 2: ${second.length} discovered URLs, ${notFound.length} not found\n`);
  await runPolite(second.map(task), { ...opts, lastEnd });
  writeFileSync(path.join(out, "discover-missing.jsonl"), notFound.map((x) => JSON.stringify(x)).join("\n") + (notFound.length ? "\n" : ""));
  process.stderr.write("done\n");
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  main(process.argv.slice(2)).catch((err) => {
    process.stderr.write(`fetch: ${err instanceof Error ? err.message : String(err)}\n`);
    process.exit(2);
  });
}
