// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

import { mkdtempSync, readFileSync, rmSync } from "node:fs";
import http from "node:http";
import https from "node:https";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { startHost } from "../host.mjs";
import { ensureTls } from "../tls.mjs";

/** Make a temporary TLS directory (never the real $LAB_TLS). */
export function tempTls() {
  const dir = mkdtempSync(join(tmpdir(), "rq-spike-tls-"));
  ensureTls(dir);
  return { dir, ca: readFileSync(join(dir, "ca.pem")), cleanup: () => rmSync(dir, { recursive: true, force: true }) };
}

/** Start a host on ephemeral ports with a fake upstream port (default: nothing listens). */
export async function startTestHost(tls, { sitePort = 1 } = {}) {
  return startHost({ tlsDir: tls.dir, ports: { good: 0, bad: 0, control: 0 }, sitePort });
}

/**
 * Open a request on the good port and give the pieces as they come.
 * @returns {Promise<{ status: number, headers: http.IncomingHttpHeaders, body: Buffer, aborted: boolean }>}
 */
export function get(port, path, { ca, host = `localhost:${port}`, headers = {} }) {
  return new Promise((resolve, reject) => {
    const req = https.request({ host: "127.0.0.1", port, path, method: "GET", ca, servername: "localhost", agent: false,
      headers: { Host: host, ...headers } }, (res) => {
      const parts = [];
      let aborted = false;
      res.on("data", (d) => parts.push(d));
      res.on("aborted", () => { aborted = true; });
      res.on("error", () => { aborted = true; });
      res.on("close", () => resolve({ status: res.statusCode ?? 0, headers: res.headers, body: Buffer.concat(parts), aborted }));
    });
    req.on("error", reject);
    req.end();
  });
}

/** Like get, but gives the response early and a list of data events with their time. */
export function open(port, path, { ca, host = `localhost:${port}`, headers = {} }) {
  const t0 = Date.now();
  const events = [];
  let text = "";
  const listeners = new Set();
  const req = https.request({ host: "127.0.0.1", port, path, method: "GET", ca, servername: "localhost", agent: false,
    headers: { Host: host, ...headers } });
  const done = new Promise((resolve, reject) => {
    req.on("error", reject);
    req.on("response", (res) => {
      res.setEncoding("utf8");
      res.on("data", (d) => {
        text += d;
        events.push({ at: Date.now() - t0, data: d });
        for (const l of listeners) l();
      });
      res.on("end", () => resolve(res));
      res.on("error", reject);
    });
  });
  req.end();
  return {
    done,
    events,
    get text() { return text; },
    /** Resolve when the received text matches. */
    until(re, ms = 5000) {
      return new Promise((resolve, reject) => {
        const check = () => {
          if (re.test(text)) { listeners.delete(check); clearTimeout(timer); resolve(Date.now() - t0); }
        };
        const timer = setTimeout(() => { listeners.delete(check); reject(new Error(`timeout waiting for ${re}`)); }, ms);
        listeners.add(check);
        check();
      });
    },
    abort: () => req.destroy(),
  };
}

/** Plain HTTP request to the control port. */
export function control(port, method, path, body, { host = `127.0.0.1:${port}`, type = "application/json" } = {}) {
  return new Promise((resolve, reject) => {
    const data = body === undefined ? undefined : typeof body === "string" ? body : JSON.stringify(body);
    const req = http.request({ host: "127.0.0.1", port, path, method, agent: false,
      headers: { Host: host, ...(data === undefined ? {} : { "Content-Type": type, "Content-Length": Buffer.byteLength(data) }) } }, (res) => {
      const parts = [];
      res.on("data", (d) => parts.push(d));
      res.on("end", () => resolve({ status: res.statusCode, headers: res.headers, text: Buffer.concat(parts).toString("utf8") }));
    });
    req.on("error", reject);
    req.end(data);
  });
}

export const wait = (ms) => new Promise((r) => setTimeout(r, ms));

export async function startRun(host, run) {
  const r = await control(host.ports.control, "POST", "/run/start", { run, capability: "CAP-T-01", platform: "test", os: "1", k: 1 });
  if (r.status !== 200) throw new Error(`run/start ${r.status}: ${r.text}`);
}

/** Events of a run so far, parsed. */
export async function events(host, run) {
  const r = await control(host.ports.control, "GET", `/run/log?run=${run}`);
  return r.text.split("\n").filter(Boolean).map((l) => JSON.parse(l));
}
