// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// GET /page/<path>: pass-through to `rust-reference --site` (CONTRACT.md section 3).
// Status, fields and body go through unchanged, except the hop-by-hop fields.

import http from "node:http";
import { pipeline } from "node:stream/promises";
import { HostError } from "./util.mjs";

const HOP_BY_HOP = new Set([
  "connection", "keep-alive", "proxy-authenticate", "proxy-authorization", "proxy-connection",
  "te", "trailer", "transfer-encoding", "upgrade",
]);
// The path rule of the reference server (server/rust-reference/README.md, site mode), so a path
// that could leave the site directory never leaves this host.
const SITE_PATH = /^(\/[A-Za-z0-9_-][A-Za-z0-9._-]*)*\/?$/;
const MAX_PATH = 512;
const UPSTREAM_TIMEOUT_MS = 10_000;

/**
 * Drop the hop-by-hop fields, and the fields that the Connection field names.
 * @param {string[]} raw flat name/value list
 * @returns {[string, string][]}
 */
export function endToEndFields(raw) {
  const named = new Set();
  for (let i = 0; i < raw.length; i += 2) {
    if (raw[i].toLowerCase() === "connection") {
      for (const t of raw[i + 1].split(",")) named.add(t.trim().toLowerCase());
    }
  }
  /** @type {[string, string][]} */
  const out = [];
  for (let i = 0; i < raw.length; i += 2) {
    const n = raw[i].toLowerCase();
    if (!HOP_BY_HOP.has(n) && !named.has(n)) out.push([raw[i], raw[i + 1]]);
  }
  return out;
}

/** @param {import("./context.mjs").RequestContext} rc */
export async function page(rc) {
  const sitePath = rc.path.slice("/page".length) || "/";
  if (sitePath.length > MAX_PATH || !SITE_PATH.test(sitePath)) throw new HostError(404, "BAD_PAGE_PATH", "not a site path");
  const rawQuery = rc.req.url?.includes("?") ? rc.req.url.slice(rc.req.url.indexOf("?")) : "";
  const { host, port } = rc.ctx.upstream;
  /** @type {Record<string, string | string[]>} */
  const headers = {};
  for (const [n, v] of endToEndFields(rc.req.rawHeaders)) {
    const old = headers[n];
    headers[n] = old === undefined ? v : Array.isArray(old) ? [...old, v] : [old, v];
  }
  const up = await new Promise((resolve, reject) => {
    const r = http.request({ host, port, method: "GET", path: `${sitePath}${rawQuery}`, headers }, resolve);
    r.setTimeout(UPSTREAM_TIMEOUT_MS, () => r.destroy(new Error("upstream timeout")));
    r.on("error", reject);
    rc.signal.addEventListener("abort", () => r.destroy(), { once: true });
    r.end();
  }).catch((cause) => {
    throw new HostError(502, "UPSTREAM_UNAVAILABLE", "reference server did not answer", { cause });
  });
  rc.head(up.statusCode, endToEndFields(up.rawHeaders), { noStore: false });
  try {
    await pipeline(up, rc.res);
  } catch {
    rc.res.destroy(); // the client or the upstream went away; nothing more to say
  }
}
