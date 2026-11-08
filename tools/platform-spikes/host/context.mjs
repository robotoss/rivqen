// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// Request context of the spike host: response helpers that also write the host.response event.

import { loggableValue } from "./util.mjs";

/** @typedef {[string, string]} Pair */

/**
 * @param {Pair[]} pairs
 * @returns {Record<string, string | string[]>} lower case names; log-safe values
 */
export function fieldsForLog(pairs) {
  /** @type {Record<string, string | string[]>} */
  const out = {};
  for (const [name, value] of pairs) {
    const n = name.toLowerCase();
    const v = loggableValue(n, value);
    const old = out[n];
    out[n] = old === undefined ? v : Array.isArray(old) ? [...old, v] : [old, v];
  }
  return out;
}

export class RequestContext {
  /**
   * @param {{ req: import("node:http").IncomingMessage, res: import("node:http").ServerResponse,
   *   q: URLSearchParams, path: string, run: string | null,
   *   site: { site: "A" | "B", host: string, port: string | null },
   *   ctx: { logs: import("./runlog.mjs").RunLogs, beacons: import("./runlog.mjs").BeaconGate, upstream: { host: string, port: number } },
   *   signal: AbortSignal }} init
   */
  constructor(init) {
    this.req = init.req;
    this.res = init.res;
    this.q = init.q;
    this.path = init.path;
    this.run = init.run;
    this.site = init.site;
    this.ctx = init.ctx;
    this.signal = init.signal;
  }

  /** @param {string} ev @param {Record<string, unknown>} [members] */
  log(ev, members) {
    this.ctx.logs.emit(this.run, ev, members);
  }

  /**
   * Write the head and log it. Without a Content-Length the body is chunked.
   * @param {number} status @param {Pair[]} pairs @param {{ noStore?: boolean }} [opts]
   */
  head(status, pairs, { noStore = true } = {}) {
    const all = [...pairs];
    if (noStore && !all.some(([n]) => n.toLowerCase() === "cache-control")) all.push(["Cache-Control", "no-store"]);
    this.log("host.response", { status, fields: fieldsForLog(all) });
    this.res.writeHead(status, all.flat());
  }

  /**
   * Send a whole answer.
   * @param {number} status @param {Pair[]} pairs @param {string | Uint8Array} body
   */
  send(status, pairs, body) {
    const len = typeof body === "string" ? Buffer.byteLength(body) : body.length;
    this.head(status, [...pairs, ["Content-Length", String(len)]]);
    this.res.end(body);
  }
}
