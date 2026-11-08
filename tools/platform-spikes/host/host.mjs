// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// The spike host (CONTRACT.md sections 1 to 5): two TLS listeners and a plain control listener,
// all on 127.0.0.1.

import { readFileSync } from "node:fs";
import http from "node:http";
import https from "node:https";
import { join } from "node:path";
import { RequestContext } from "./context.mjs";
import { control } from "./control.mjs";
import { page } from "./page.mjs";
import { BeaconGate, RunLogs } from "./runlog.mjs";
import { cookieSet, cut, echo, range, redirect } from "./scenarios.mjs";
import { beacon, stream } from "./stream.mjs";
import { ensureTls } from "./tls.mjs";
import { HostError, parseQuery, runOf, siteOf } from "./util.mjs";

export const LISTEN_ADDRESS = "127.0.0.1";
const MAX_HEADER_BYTES = 16 * 1024;

/** @type {Record<string, (rc: RequestContext) => unknown>} */
const ROUTES = {
  "/s/stream": stream,
  "/s/beacon": beacon,
  "/s/redirect": redirect,
  "/s/cookie/set": cookieSet,
  "/s/echo": echo,
  "/s/range": range,
  "/s/cut": cut,
  "/s/sw.js": notInS1,
  "/s/sw-page": notInS1,
};

function notInS1() {
  throw new HostError(501, "NOT_IMPLEMENTED", "Service Worker routes come in sprint S2");
}

/** @param {import("node:http").ServerResponse} res @param {HostError} err */
function plainError(res, err) {
  const text = `${err.code}: ${err.message}\n`;
  res.writeHead(err.status, { "Content-Type": "text/plain; charset=utf-8", "Content-Length": Buffer.byteLength(text), "Cache-Control": "no-store" });
  res.end(text);
}

/** Turn any thrown value into an answer, or close the connection when the head is out. */
function fail(res, err, logs, run) {
  const he = err instanceof HostError ? err : new HostError(500, "INTERNAL", "internal error", { cause: err });
  if (!(err instanceof HostError)) process.stderr.write(`spike host: ${err instanceof Error ? err.message : "non-error thrown"}\n`);
  if (res.headersSent) {
    res.destroy();
    return;
  }
  logs.emit(run, "host.response", { status: he.status, fields: { "content-type": "text/plain; charset=utf-8" } });
  plainError(res, he);
}

/**
 * @param {{ logs: RunLogs, beacons: BeaconGate, upstream: { host: string, port: number } }} ctx
 * @returns {(req: import("node:http").IncomingMessage, res: import("node:http").ServerResponse) => Promise<void>}
 */
function siteHandler(ctx) {
  return async (req, res) => {
    const abort = new AbortController();
    res.on("close", () => abort.abort());
    let run = null;
    try {
      const target = req.url ?? "";
      if (!target.startsWith("/")) throw new HostError(400, "BAD_TARGET", "origin-form target required");
      const qi = target.indexOf("?");
      const path = qi < 0 ? target : target.slice(0, qi);
      const q = parseQuery(qi < 0 ? "" : target.slice(qi + 1));
      run = runOf(req.headers["rq-lab-run"]?.toString(), q.get("run"));
      const site = siteOf(req.headers.host);
      const fields = [...new Set(Object.keys(req.headers))].sort();
      ctx.logs.emit(run, "host.request", {
        fields,
        method: req.method,
        range: req.headers.range,
        site: site?.site ?? null,
        target,
      });
      if (site === null) throw new HostError(400, "BAD_HOST", "unknown Host field");
      if (req.method !== "GET") throw new HostError(405, "METHOD", "only GET is allowed");
      const rc = new RequestContext({ req, res, q, path, run, site, ctx, signal: abort.signal });
      if (path === "/page" || path.startsWith("/page/")) return await page(rc);
      const route = Object.hasOwn(ROUTES, path) ? ROUTES[path] : undefined;
      if (route === undefined) throw new HostError(404, "NO_ROUTE", "no such route");
      await route(rc);
    } catch (err) {
      fail(res, err, ctx.logs, run);
    }
  };
}

/**
 * Start the host.
 * @param {{ tlsDir: string, ports?: { good?: number, bad?: number, control?: number }, sitePort: number,
 *   now?: () => number }} opts Port 0 picks a free port.
 * @returns {Promise<{ ports: { good: number, bad: number, control: number }, logs: RunLogs, close: () => Promise<void> }>}
 */
export async function startHost({ tlsDir, ports = {}, sitePort, now }) {
  ensureTls(tlsDir);
  const read = (f) => readFileSync(join(tlsDir, f));
  const ctx = {
    logs: new RunLogs({ now }),
    beacons: new BeaconGate(),
    upstream: { host: LISTEN_ADDRESS, port: sitePort },
  };
  const handler = siteHandler(ctx);
  const tlsServer = (key, cert) => {
    const s = https.createServer({ key: read(key), cert: read(cert), maxHeaderSize: MAX_HEADER_BYTES }, handler);
    s.on("tlsClientError", () => {}); // a client that rejects the certificate is an expected case
    return s;
  };
  const good = tlsServer("leaf.key", "leaf.pem");
  const bad = tlsServer("bad-leaf.key", "bad-leaf.pem");
  const ctl = http.createServer({ maxHeaderSize: MAX_HEADER_BYTES }, (req, res) => {
    control(req, res, ctx.logs).catch((err) => fail(res, err, ctx.logs, null));
  });
  const servers = [good, bad, ctl];
  const listen = (server, port) =>
    new Promise((resolve, reject) => {
      server.once("error", reject);
      server.listen(port, LISTEN_ADDRESS, () => {
        server.off("error", reject);
        resolve(/** @type {import("node:net").AddressInfo} */ (server.address()).port);
      });
    });
  const close = () => Promise.all(servers.map((s) => new Promise((resolve) => {
    s.close(() => resolve());
    s.closeAllConnections();
  }))).then(() => undefined);
  try {
    const bound = {
      good: await listen(good, ports.good ?? 18443),
      bad: await listen(bad, ports.bad ?? 18444),
      control: await listen(ctl, ports.control ?? 18445),
    };
    return { ports: bound, logs: ctx.logs, close };
  } catch (err) {
    await close();
    throw err;
  }
}
