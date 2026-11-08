// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// Joins the parts of one iOS run into the run log of CONTRACT.md section 5:
// run.start (script), the app part, the host part, run.end (script). Keys sorted, LF.
// Also writes run-<k>.metrics.json and, with --run-json, run.json (section 9).
//
// node join.mjs --app <app.jsonl> --host <host.jsonl> --metrics <metrics.json> --meta <meta.json> --out <dir>
//   meta.json: {"run","capability","os","k","device","prototype","ok","error","t_end"}
// node join.mjs --run-json <app-run.json> --facts <facts.json> --out <dir>

import { readFileSync, writeFileSync, mkdirSync } from "node:fs";
import path from "node:path";

/** JSON with sorted keys at every level (the host uses the same rule). */
export function stableStringify(v) {
  if (Array.isArray(v)) return `[${v.map((x) => stableStringify(x === undefined ? null : x)).join(",")}]`;
  if (v !== null && typeof v === "object") {
    const parts = [];
    for (const k of Object.keys(v).sort()) {
      if (v[k] !== undefined) parts.push(`${JSON.stringify(k)}:${stableStringify(v[k])}`);
    }
    return `{${parts.join(",")}}`;
  }
  return JSON.stringify(v);
}

/** Lines of a JSON Lines text; each must be an object from the named source. */
export function parseLines(text, src) {
  const out = [];
  for (const [i, line] of text.split("\n").entries()) {
    if (line === "") continue;
    const obj = JSON.parse(line);
    if (obj === null || typeof obj !== "object" || Array.isArray(obj)) throw new Error(`${src} line ${i + 1}: not an object`);
    if (obj.src !== src) throw new Error(`${src} line ${i + 1}: src is ${JSON.stringify(obj.src)}`);
    out.push(obj);
  }
  return out;
}

/**
 * The joined run log text.
 * @param {{ run: string, capability: string, os: string, k: number, device: string, prototype: string,
 *   ok: boolean, error: string | null, t_end: number }} meta
 * @param {string} appText @param {string} hostText
 */
export function joinRun(meta, appText, hostText) {
  const start = {
    ev: "run.start", src: "script", seq: 0, t: 0, run: meta.run, capability: meta.capability, platform: "ios",
    os: meta.os, device: meta.device, webview: null, prototype: meta.prototype, k: meta.k,
  };
  const end = { ev: "run.end", src: "script", seq: 1, t: meta.t_end, ok: meta.ok, error: meta.error };
  const events = [start, ...parseLines(appText, "app"), ...parseLines(hostText, "host"), end];
  return events.map((e) => `${stableStringify(e)}\n`).join("");
}

function args(argv) {
  const out = {};
  for (let i = 0; i < argv.length; i += 2) {
    if (!argv[i].startsWith("--") || i + 1 >= argv.length) throw new Error(`bad argument ${argv[i]}`);
    out[argv[i].slice(2)] = argv[i + 1];
  }
  return out;
}

function main() {
  const a = args(process.argv.slice(2));
  mkdirSync(a.out, { recursive: true });
  if (a["run-json"] !== undefined) {
    const app = JSON.parse(readFileSync(a["run-json"], "utf8"));
    const facts = JSON.parse(readFileSync(a.facts, "utf8"));
    const run = {
      date: facts.date,
      tools: facts.tools,
      device_model: facts.device_model,
      os: app.os_version,
      os_build: app.simulator_runtime_build,
      simulator_model: app.simulator_model,
      webview: null,
      webkit_version: app.webkit_version,
      webkit_build: app.webkit_build,
      user_agent: app.user_agent,
      app_commit: facts.app_commit,
    };
    writeFileSync(path.join(a.out, "run.json"), `${JSON.stringify(JSON.parse(stableStringify(run)), null, 2)}\n`);
    return;
  }
  const meta = JSON.parse(readFileSync(a.meta, "utf8"));
  const text = joinRun(meta, readFileSync(a.app, "utf8"), readFileSync(a.host, "utf8"));
  writeFileSync(path.join(a.out, `run-${meta.k}.jsonl`), text);
  const metrics = JSON.parse(readFileSync(a.metrics, "utf8"));
  writeFileSync(path.join(a.out, `run-${meta.k}.metrics.json`), `${stableStringify(metrics)}\n`);
}

if (import.meta.url === `file://${process.argv[1]}`) main();
