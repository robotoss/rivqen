// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// The lab server (CONTRACT section 3): HTTP/1.1 on 127.0.0.1, raw socket writes so the header
// lines of a case are exact. It serves /cases.json and /c/<id>. It never parses or runs a case body.
//   node host/server.mjs [--port N]

import { readFile } from "node:fs/promises";
import { createServer } from "node:net";
import { join, resolve, sep } from "node:path";
import { pathToFileURL } from "node:url";
import { CASES_DIR, LabError, REPO_ROOT, isValidId, parseCase } from "./lab.mjs";

export const DEFAULT_PORT = 18090;
const MAX_REQUEST_HEAD = 16 * 1024;
const IDLE_TIMEOUT_MS = 10_000; // an idle connection is closed after this time
const CASE_PATH = /^\/c\/([A-Z0-9-]{1,64})$/;

/** @param {number} status @param {string} reason @param {[string, string][]} headers @param {Uint8Array} body */
function response(status, reason, headers, body) {
  const lower = new Set(headers.map(([n]) => n.toLowerCase()));
  const lines = headers.map(([n, v]) => `${n}: ${v}`);
  if (!lower.has("content-length")) lines.push(`Content-Length: ${body.length}`);
  if (!lower.has("connection")) lines.push("Connection: close");
  const head = Buffer.from(`HTTP/1.1 ${status} ${reason}\r\n${lines.join("\r\n")}\r\n\r\n`, "latin1");
  return Buffer.concat([head, body]);
}

const NOT_FOUND = response(404, "Not Found", [["Content-Type", "text/plain; charset=utf-8"]], Buffer.from("not found\n"));
const SERVER_ERROR = response(500, "Internal Server Error", [["Content-Type", "text/plain; charset=utf-8"]], Buffer.from("case error\n"));

/**
 * Build the bytes to write for a request target.
 * @param {string} target
 * @param {{ casesDir: string, repoRoot: string }} dirs
 * @returns {Promise<Buffer>}
 */
export async function answer(target, { casesDir, repoRoot }) {
  if (target === "/cases.json") {
    try {
      const body = await readFile(join(casesDir, "index.json"));
      return response(200, "OK", [["Content-Type", "application/json; charset=utf-8"]], body);
    } catch (err) {
      process.stderr.write(`lab server: cases.json: ${err instanceof Error ? err.message : String(err)}\n`);
      return SERVER_ERROR;
    }
  }
  const m = CASE_PATH.exec(target);
  if (m === null || !isValidId(m[1])) return NOT_FOUND;
  const id = m[1];
  let text;
  try {
    text = await readFile(join(casesDir, `${id}.json`), "utf8");
  } catch (err) {
    if (/** @type {NodeJS.ErrnoException} */ (err).code === "ENOENT") return NOT_FOUND;
    process.stderr.write(`lab server: case ${id}: ${err instanceof Error ? err.message : String(err)}\n`);
    return SERVER_ERROR;
  }
  try {
    const c = parseCase(JSON.parse(text), `${id}.json`);
    if (c.id !== id) throw new LabError("CASE_INVALID", `${id}.json: id does not match the file name`);
    const root = resolve(repoRoot);
    const file = resolve(root, c.source);
    if (!file.startsWith(root + sep)) throw new LabError("CASE_SOURCE", `${id}.json: source leaves the repository`);
    return response(200, "OK", c.headers, await readFile(file));
  } catch (err) {
    process.stderr.write(`lab server: case ${id}: ${err instanceof Error ? err.message : String(err)}\n`);
    return SERVER_ERROR;
  }
}

/**
 * Start the server on 127.0.0.1.
 * @param {{ port?: number, casesDir?: string, repoRoot?: string, idleTimeoutMs?: number }} [opts]
 * @returns {Promise<{ port: number, close: () => Promise<void> }>}
 */
export function startServer({
  port = DEFAULT_PORT,
  casesDir = CASES_DIR,
  repoRoot = REPO_ROOT,
  idleTimeoutMs = IDLE_TIMEOUT_MS,
} = {}) {
  const server = createServer((socket) => {
    let buf = Buffer.alloc(0);
    let done = false;
    // A socket error (a reset by the peer) ends this connection only.
    socket.on("error", () => socket.destroy());
    socket.setTimeout(idleTimeoutMs, () => socket.destroy());
    socket.on("data", (chunk) => {
      if (done) return;
      buf = Buffer.concat([buf, chunk]);
      const end = buf.indexOf("\r\n\r\n");
      if (end < 0) {
        if (buf.length > MAX_REQUEST_HEAD) socket.destroy();
        return;
      }
      done = true;
      const first = buf.subarray(0, buf.indexOf("\r\n")).toString("latin1");
      const parts = first.split(" ");
      const reply =
        parts.length === 3 && parts[0] === "GET"
          ? answer(parts[1], { casesDir, repoRoot })
          : Promise.resolve(NOT_FOUND);
      reply.then(
        (bytes) => socket.end(bytes),
        () => socket.end(SERVER_ERROR),
      );
    });
  });
  return new Promise((resolveStart, rejectStart) => {
    server.once("error", rejectStart);
    server.listen(port, "127.0.0.1", () => {
      const addr = server.address();
      if (addr === null || typeof addr === "string") {
        rejectStart(new LabError("LISTEN", "server has no TCP address"));
        return;
      }
      resolveStart({
        port: addr.port,
        close: () =>
          new Promise((res, rej) => {
            server.close((err) => (err ? rej(err) : res()));
            server.closeAllConnections?.();
          }),
      });
    });
  });
}

/** @param {string[]} argv @returns {number|null} */
function parsePort(argv) {
  const i = argv.indexOf("--port");
  if (i < 0) return DEFAULT_PORT;
  const n = Number(argv[i + 1]);
  return Number.isInteger(n) && n >= 0 && n <= 65535 ? n : null;
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  const port = parsePort(process.argv.slice(2));
  if (port === null) {
    process.stderr.write("server: --port needs an integer from 0 to 65535\n");
    process.exitCode = 2;
  } else {
    startServer({ port }).then(
      (s) => process.stdout.write(`lab server on 127.0.0.1:${s.port}\n`),
      (err) => {
        process.stderr.write(`server: ${err instanceof Error ? err.message : String(err)}\n`);
        process.exitCode = 1;
      },
    );
  }
}
