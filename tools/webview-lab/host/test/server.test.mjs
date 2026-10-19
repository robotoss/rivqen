// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

import assert from "node:assert/strict";
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { connect } from "node:net";
import { networkInterfaces, tmpdir } from "node:os";
import { join } from "node:path";
import { after, before, test } from "node:test";
import { CASES_DIR, REPO_ROOT } from "../lab.mjs";
import { startServer } from "../server.mjs";

let root;
let server;

/** Send raw request bytes and return the raw response bytes. */
function raw(port, request, host = "127.0.0.1") {
  return new Promise((resolve, reject) => {
    const chunks = [];
    const s = connect(port, host, () => s.write(request));
    s.on("data", (c) => chunks.push(c));
    s.on("end", () => resolve(Buffer.concat(chunks)));
    s.on("error", reject);
  });
}

/** Split a response into the head lines and the body bytes. */
function split(buf) {
  const at = buf.indexOf("\r\n\r\n");
  assert.ok(at > 0, "no end of head");
  return { lines: buf.subarray(0, at).toString("latin1").split("\r\n"), body: buf.subarray(at + 4) };
}

const get = (path) => raw(server.port, `GET ${path} HTTP/1.1\r\nHost: x\r\n\r\n`);

function writeCase(c, body) {
  writeFileSync(join(root, "cases", `${c.id}.json`), JSON.stringify(c));
  writeFileSync(join(root, c.source), body);
}

before(async () => {
  root = mkdtempSync(join(tmpdir(), "lab-server-"));
  mkdirSync(join(root, "cases"));
  mkdirSync(join(root, "pages"));
  const base = { question: "Q-20", views: ["on"], probe: null };
  writeCase({ ...base, id: "TWO", source: "pages/two.html", headers: [["Content-Type", "text/html; charset=utf-8"], ["Content-Type", "text/html; charset=iso-8859-1"]] }, "<p>two");
  writeCase({ ...base, id: "NONE", source: "pages/none.html", headers: [] }, "<p>none");
  writeCase({ ...base, id: "BOM", source: "pages/bom.html", headers: [["Content-Type", "text/html"]] }, Buffer.from([0xef, 0xbb, 0xbf, 0x3c, 0x70, 0x3e, 0xff]));
  writeCase({ ...base, id: "OWN", source: "pages/own.html", headers: [["content-length", "3"], ["connection", "keep-alive"]] }, "abc");
  writeCase({ ...base, id: "LATIN", source: "pages/latin.html", headers: [["X-Name", "caféÿ"]] }, "l");
  writeFileSync(join(root, "cases", "ESCAPE.json"), JSON.stringify({ ...base, id: "ESCAPE", source: "../outside.html", headers: [] }));
  writeFileSync(join(root, "cases", "BADJSON.json"), "{");
  writeFileSync(join(root, "cases", "index.json"), '["TWO","NONE","BOM"]');
  server = await startServer({ port: 0, casesDir: join(root, "cases"), repoRoot: root });
});

after(async () => {
  await server.close();
  rmSync(root, { recursive: true, force: true });
});

test("two Content-Type lines of a case stay two lines, in order", async () => {
  const { lines, body } = split(await get("/c/TWO"));
  assert.equal(lines[0], "HTTP/1.1 200 OK");
  assert.deepEqual(lines.slice(1), [
    "Content-Type: text/html; charset=utf-8",
    "Content-Type: text/html; charset=iso-8859-1",
    "Content-Length: 6",
    "Connection: close",
  ]);
  assert.equal(body.toString(), "<p>two");
});

test("a Latin-1 header value is written byte for byte", async () => {
  const buf = await get("/c/LATIN");
  const at = buf.indexOf("X-Name: ");
  assert.deepEqual([...buf.subarray(at + 8, at + 8 + 5)], [0x63, 0x61, 0x66, 0xe9, 0xff]);
  assert.equal(buf[at + 13], 0x0d);
});

test("a case without a Content-Type line gets none", async () => {
  const { lines } = split(await get("/c/NONE"));
  assert.deepEqual(lines.slice(1), ["Content-Length: 7", "Connection: close"]);
});

test("body bytes are sent unchanged, a byte order mark and an invalid byte included", async () => {
  const { body } = split(await get("/c/BOM"));
  assert.deepEqual([...body], [0xef, 0xbb, 0xbf, 0x3c, 0x70, 0x3e, 0xff]);
});

test("the server adds Content-Length and Connection only when the case has no such line", async () => {
  const { lines } = split(await get("/c/OWN"));
  assert.deepEqual(lines.slice(1), ["content-length: 3", "connection: keep-alive"]);
});

test("/cases.json gives the ids in order", async () => {
  const { lines, body } = split(await get("/cases.json"));
  assert.equal(lines[0], "HTTP/1.1 200 OK");
  assert.deepEqual(JSON.parse(body.toString()), ["TWO", "NONE", "BOM"]);
});

for (const path of ["/", "/c/", "/c/MISSING", "/c/two", "/c/TWO/", "/c/TWO?x=1", "/c/../cases.json", "/cases.json/", "/index.json", "/cases/TWO.json"]) {
  test(`404 for ${path}`, async () => {
    assert.equal(split(await get(path)).lines[0], "HTTP/1.1 404 Not Found");
  });
}

test("404 for a method other than GET", async () => {
  const res = await raw(server.port, "POST /c/TWO HTTP/1.1\r\nHost: x\r\n\r\n");
  assert.equal(split(res).lines[0], "HTTP/1.1 404 Not Found");
});

test("a broken case file gives 500, not a crash", async () => {
  assert.equal(split(await get("/c/BADJSON")).lines[0], "HTTP/1.1 500 Internal Server Error");
});

test("a case source outside the repository gives 500", async () => {
  assert.equal(split(await get("/c/ESCAPE")).lines[0], "HTTP/1.1 500 Internal Server Error");
});

test("the server listens on 127.0.0.1 only", async (t) => {
  const external = Object.values(networkInterfaces())
    .flat()
    .find((i) => i && i.family === "IPv4" && !i.internal);
  if (!external) return t.skip("no non-loopback IPv4 address on this host");
  await assert.rejects(raw(server.port, "GET / HTTP/1.1\r\n\r\n", external.address), { code: "ECONNREFUSED" });
});

test("the real BOM page is served with its byte order mark", async () => {
  const real = await startServer({ port: 0, casesDir: CASES_DIR, repoRoot: REPO_ROOT });
  try {
    const { lines, body } = split(await raw(real.port, "GET /c/BOM-NONE HTTP/1.1\r\n\r\n"));
    assert.ok(!lines.some((l) => /^content-type:/i.test(l)));
    assert.deepEqual([...body.subarray(0, 3)], [0xef, 0xbb, 0xbf]);
  } finally {
    await real.close();
  }
});
