// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// GET /s/stream and GET /s/beacon (CONTRACT.md section 3).

import { HostError, MAX_CHUNKS, MAX_MS, MAX_SIZE, MAX_TOTAL, enumParam, intParam, sleep } from "./util.mjs";

const DOC_HEAD = '<!doctype html><html><head><meta charset="utf-8"><title>stream</title></head><body>';

/** CSP nonce source for a run id: the run id without characters that a nonce cannot hold. */
export const nonceOf = (run) => (run === null ? "norun" : run.replaceAll(".", "_"));

/**
 * One chunk of HTML of about `size` bytes (never shorter than its fixed markup).
 * @param {{ k: number, size: number, run: string | null, nonce: string | null }} p
 */
export function buildChunk({ k, size, run, nonce }) {
  const runPart = run === null ? "" : `run=${run}&`;
  const prefix = `${k === 0 ? DOC_HEAD : ""}<p data-k="${k}">`;
  const nonceAttr = nonce === null ? "" : ` nonce="${nonce}"`;
  const suffix = `</p><img src="/s/beacon?${runPart}k=${k}" hidden>` +
    `<script${nonceAttr}>window.rqLab&&window.rqLab.probe("chunk",${k})</script>`;
  const fill = Math.max(0, size - prefix.length - suffix.length);
  return `${prefix}${"x".repeat(fill)}${suffix}`;
}

/**
 * Wait for the beacon of a chunk.
 * @param {{ arrived: Promise<void>, cancel: () => void }} expectation
 * @param {number} timeoutMs @param {AbortSignal} signal
 * @returns {Promise<"beacon" | "timeout" | "aborted">}
 */
async function waitBeacon(expectation, timeoutMs, signal) {
  const local = new AbortController();
  const timer = sleep(timeoutMs, AbortSignal.any([signal, local.signal])).then((passed) => (passed ? "timeout" : "aborted"));
  const got = expectation.arrived.then(() => "beacon");
  const result = await Promise.race([got, timer]);
  local.abort();
  expectation.cancel();
  return signal.aborted ? "aborted" : /** @type {"beacon" | "timeout" | "aborted"} */ (result);
}

/** @param {import("node:http").ServerResponse} res @param {string} data @returns {Promise<boolean>} */
function write(res, data) {
  return new Promise((resolve) => {
    if (res.destroyed) return resolve(false);
    res.write(data, (err) => resolve(err === null || err === undefined));
  });
}

/** @param {import("./context.mjs").RequestContext} rc */
export async function stream(rc) {
  const { q, run, signal, res } = rc;
  const chunks = intParam(q, "chunks", { def: 4, min: 1, max: MAX_CHUNKS });
  const size = intParam(q, "size", { def: 1024, min: 1, max: MAX_SIZE });
  if (chunks * size > MAX_TOTAL) throw new HostError(400, "BAD_PARAM", "chunks * size is too large");
  const first = intParam(q, "first", { def: 0, min: 0, max: MAX_MS });
  const delay = intParam(q, "delay", { def: 0, min: 0, max: MAX_MS });
  const timeout = intParam(q, "timeout", { def: 5000, min: 0, max: MAX_MS });
  const gate = enumParam(q, "gate", ["beacon", "delay"], "delay");
  const csp = enumParam(q, "csp", ["0", "1"], "0") === "1";
  const nonce = csp ? nonceOf(run) : null;

  if (!(await sleep(first, signal))) return;
  /** @type {[string, string][]} */
  const fields = [["Content-Type", "text/html; charset=utf-8"]];
  if (csp) fields.push(["Content-Security-Policy", `script-src 'nonce-${nonce}'`]);
  rc.head(200, fields);
  res.flushHeaders();

  for (let k = 0; k < chunks; k += 1) {
    const last = k === chunks - 1;
    const expectation = gate === "beacon" && !last ? rc.ctx.beacons.expect(run, k) : null;
    const html = buildChunk({ k, size, run, nonce });
    if (!(await write(res, html))) {
      expectation?.cancel();
      return;
    }
    rc.log("host.chunk", { k, bytes: Buffer.byteLength(html) });
    if (last) break;
    if (expectation === null) {
      if (!(await sleep(delay, signal))) return;
    } else {
      const outcome = await waitBeacon(expectation, timeout, signal);
      if (outcome === "aborted") return;
      if (outcome === "timeout") rc.log("host.gate_timeout", { k });
    }
  }
  res.end();
}

/** @param {import("./context.mjs").RequestContext} rc */
export function beacon(rc) {
  const k = intParam(rc.q, "k", { min: 0, max: MAX_CHUNKS - 1 });
  rc.log("host.beacon", { k });
  rc.ctx.beacons.arrive(rc.run, k);
  rc.head(204, []);
  rc.res.end();
}
