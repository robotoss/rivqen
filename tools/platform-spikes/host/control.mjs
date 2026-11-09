// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// Control endpoint (CONTRACT.md section 4): plain HTTP on 127.0.0.1, for the run script only.

import { HostError, isRunId } from "./util.mjs";

const MAX_BODY = 4096;
const LOOPBACK_HOST = /^(127\.0\.0\.1|localhost)(:\d{1,5})?$/;

/** @param {import("node:http").IncomingMessage} req @returns {Promise<unknown>} */
async function readJson(req) {
  if (!/^application\/json(\s*;.*)?$/i.test(req.headers["content-type"] ?? "")) {
    throw new HostError(415, "BAD_CONTENT_TYPE", "content-type must be application/json");
  }
  const parts = [];
  let total = 0;
  for await (const part of req) {
    total += part.length;
    if (total > MAX_BODY) throw new HostError(413, "BODY_TOO_LARGE", "body is too large");
    parts.push(part);
  }
  try {
    return JSON.parse(Buffer.concat(parts).toString("utf8"));
  } catch (cause) {
    throw new HostError(400, "BAD_JSON", "body is not JSON", { cause });
  }
}

/**
 * @param {import("node:http").IncomingMessage} req
 * @param {import("node:http").ServerResponse} res
 * @param {import("./runlog.mjs").RunLogs} logs
 */
export async function control(req, res, logs) {
  if (!LOOPBACK_HOST.test(req.headers.host ?? "")) throw new HostError(400, "BAD_HOST", "unexpected Host field");
  const url = new URL(req.url ?? "/", "http://127.0.0.1");
  const json = (status, text, type = "application/json") => {
    res.writeHead(status, { "Content-Type": type, "Content-Length": Buffer.byteLength(text), "Cache-Control": "no-store" });
    res.end(text);
  };
  const route = `${req.method} ${url.pathname}`;
  if (route === "POST /run/start") {
    const run = logs.start(await readJson(req));
    return json(200, JSON.stringify({ ok: true, run }));
  }
  if (route === "POST /run/end") {
    const body = await readJson(req);
    const run = body !== null && typeof body === "object" ? /** @type {Record<string, unknown>} */ (body).run : undefined;
    const text = isRunId(run) ? logs.end(run) : null;
    if (text === null) throw new HostError(404, "NO_RUN", "unknown run");
    return json(200, text, "application/x-ndjson");
  }
  if (route === "GET /run/log") {
    const run = url.searchParams.get("run");
    const text = isRunId(run) ? logs.text(run) : null;
    if (text === null) throw new HostError(404, "NO_RUN", "unknown run");
    return json(200, text, "application/x-ndjson");
  }
  throw new HostError(url.pathname.startsWith("/run/") ? 405 : 404, "NO_ROUTE", "no such control route");
}
