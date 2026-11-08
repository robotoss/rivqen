// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import http from "node:http";
import net from "node:net";
import os from "node:os";
import { dirname, join } from "node:path";
import { after, before, test } from "node:test";
import { fileURLToPath } from "node:url";
import { parseConfig } from "../main.mjs";
import { control, events, get, startRun, startTestHost, tempTls } from "./helpers.mjs";

let tls;
let host;
let upstream;
let seen;
let port;
before(async () => {
  tls = tempTls();
  seen = [];
  upstream = http.createServer((req, res) => {
    seen.push({ url: req.url, headers: req.headers, raw: req.rawHeaders });
    if (req.url.startsWith("/leak")) {
      res.writeHead(200, ["Set-Cookie", "sid=Real_Secret; Path=/", "Content-Length", "0"]);
      return res.end();
    }
    if (req.url === "/down") return req.socket.destroy();
    res.writeHead(203, [
      "Content-Type", "text/html", "X-Custom", "yes", "Set-Cookie", "a=b; Path=/", "Set-Cookie", "c=d",
      "Connection", "x-private", "X-Private", "secret", "Keep-Alive", "timeout=99", "Content-Length", "5",
    ]);
    res.end("hello");
  });
  await new Promise((r) => upstream.listen(0, "127.0.0.1", r));
  host = await startTestHost(tls, { sitePort: upstream.address().port });
  port = host.ports.good;
});
after(async () => {
  await host.close();
  await new Promise((r) => upstream.close(r));
  tls.cleanup();
});
const g = (path, opts = {}) => get(port, path, { ca: tls.ca, ...opts });

test("page_passthrough", async () => {
  await startRun(host, "page-1");
  const r = await g("/page/shop/index.html?x=1", { headers: { "X-Req": "1", Connection: "x-drop", "X-Drop": "no", "Rq-Lab-Run": "page-1" } });
  assert.equal(r.status, 203);
  assert.equal(r.body.toString(), "hello");
  assert.equal(r.headers["x-custom"], "yes");
  assert.deepEqual(r.headers["set-cookie"], ["a=b; Path=/", "c=d"]);
  assert.equal(r.headers["x-private"], undefined, "field named by Connection is dropped");
  assert.notEqual(r.headers["keep-alive"], "timeout=99");
  assert.equal(r.headers["cache-control"], undefined, "the page keeps its own caching");
  const s = seen.at(-1);
  assert.equal(s.url, "/shop/index.html?x=1");
  assert.equal(s.headers["x-req"], "1");
  assert.equal(s.headers["x-drop"], undefined);
  assert.equal(s.headers.host, `localhost:${port}`);
  const ev = await events(host, "page-1");
  const resp = ev.find((e) => e.ev === "host.response");
  assert.equal(resp.status, 203);
  assert.equal(resp.fields["x-custom"], "yes");
  assert.deepEqual(resp.fields["set-cookie"], ["a=b; Path=/", "c=d"]);
  await control(host.ports.control, "POST", "/run/end", { run: "page-1" });
});

test("page_root_and_trailing_slash", async () => {
  assert.equal((await g("/page")).status, 203);
  assert.equal(seen.at(-1).url, "/");
  assert.equal((await g("/page/")).status, 203);
  assert.equal(seen.at(-1).url, "/");
  assert.equal((await g("/page/a/")).status, 203);
  assert.equal(seen.at(-1).url, "/a/");
});

test("page_refuses_traversal_and_odd_paths", async () => {
  const before = seen.length;
  for (const p of ["/page/../etc/passwd", "/page/a/../b", "/page/%2e%2e/x", "/page/.hidden", "/page/a%2Fb", "/page//x",
    "/page/a%20b", "/page/a%5Cb", `/page/${"a".repeat(600)}`, "/pagex"]) {
    const r = await g(p);
    assert.ok(r.status === 404 || r.status === 400, `${p} -> ${r.status}`);
  }
  assert.equal(seen.length, before, "nothing reached the reference server");
});

test("page_upstream_down_gives_502", async () => {
  const r = await g("/page/down");
  assert.equal(r.status, 502);
  assert.match(r.body.toString(), /^UPSTREAM_UNAVAILABLE: /);
});

test("control_run_log", async () => {
  const ctl = host.ports.control;
  const start = await control(ctl, "POST", "/run/start", { run: "CAP-T-01-test-1-1", capability: "CAP-T-01", platform: "test", os: "1", k: 1 });
  assert.equal(start.status, 200);
  assert.deepEqual(JSON.parse(start.text), { ok: true, run: "CAP-T-01-test-1-1" });
  await g("/s/echo?run=CAP-T-01-test-1-1", { headers: { Range: "bytes=1-2", Cookie: "sid=real-secret value" } });
  await g("/s/echo", { headers: { "Rq-Lab-Run": "CAP-T-01-test-1-1" } });
  await g("/s/echo?run=other-run");
  const mid = await control(ctl, "GET", "/run/log?run=CAP-T-01-test-1-1");
  assert.equal(mid.status, 200);
  const end = await control(ctl, "POST", "/run/end", { run: "CAP-T-01-test-1-1" });
  assert.equal(end.status, 200);
  assert.equal(end.headers["content-type"], "application/x-ndjson");
  assert.ok(end.text.startsWith(mid.text) && mid.text.length > 0);
  assert.ok(end.text.endsWith("\n"));
  const lines = end.text.split("\n").filter(Boolean);
  const ev = lines.map((l) => JSON.parse(l));
  assert.deepEqual(ev.map((e) => e.ev), ["host.request", "host.response", "host.request", "host.response"]);
  assert.deepEqual(ev.map((e) => e.seq), [0, 1, 2, 3]);
  assert.ok(ev.every((e) => e.src === "host" && Number.isInteger(e.t) && e.t >= 0));
  for (const l of lines) {
    const keys = Object.keys(JSON.parse(l));
    assert.deepEqual(keys, [...keys].sort(), "keys are sorted");
  }
  assert.equal(ev[0].site, "A");
  assert.equal(ev[0].method, "GET");
  assert.equal(ev[0].range, "bytes=1-2");
  assert.equal(ev[0].target, "/s/echo?run=CAP-T-01-test-1-1");
  assert.ok(ev[0].fields.includes("cookie") && !end.text.includes("real-secret"), "cookie value is never logged");
  assert.equal(ev[1].status, 200);
  assert.equal(ev[1].fields["content-type"], "application/json");
  assert.equal(ev[1].fields["cache-control"], "no-store");
  // the run is closed
  assert.equal((await control(ctl, "POST", "/run/end", { run: "CAP-T-01-test-1-1" })).status, 404);
  assert.equal((await control(ctl, "GET", "/run/log?run=CAP-T-01-test-1-1")).status, 404);
});

test("control_rejects_bad_input", async () => {
  const ctl = host.ports.control;
  const ok = { run: "ctl-1", capability: "CAP-T-01", platform: "test", os: "1", k: 1 };
  const bad = async (body, status, opts) => assert.equal((await control(ctl, "POST", "/run/start", body, opts)).status, status, JSON.stringify(body));
  await bad({ ...ok, run: "has space" }, 400);
  await bad({ ...ok, run: 5 }, 400);
  await bad({ ...ok, capability: "a/b" }, 400);
  await bad({ ...ok, platform: undefined }, 400);
  await bad({ ...ok, os: "x".repeat(65) }, 400);
  await bad({ ...ok, k: -1 }, 400);
  await bad({ ...ok, k: 1.5 }, 400);
  await bad({ ...ok, k: null }, 400);
  await bad([], 400);
  await bad("null", 400);
  await bad("{not json", 400);
  await bad("x".repeat(5000), 413);
  await bad(ok, 415, { type: "text/plain" });
  await bad(ok, 400, { host: "evil.example" });
  // DNS rebinding guard: the whole Host value must be the loopback name and an optional port.
  await bad(ok, 400, { host: "evil127.0.0.1" });
  await bad(ok, 400, { host: "localhost.evil.example" });
  await bad(ok, 400, { host: "127.0.0.1:123456" });
  // The content type must be JSON, with parameters only after a semicolon.
  await bad(ok, 415, { type: "text/application/json" });
  await bad(ok, 415, { type: "application/jsonx" });
  await bad(ok, 415, { type: "application/json+x; charset=utf-8" });
  assert.equal((await control(ctl, "POST", "/run/start", { ...ok, run: "ctl-ct" }, { type: "Application/JSON ; charset=utf-8" })).status, 200);
  await control(ctl, "POST", "/run/end", { run: "ctl-ct" });
  assert.equal((await control(ctl, "GET", "/run/start")).status, 405);
  assert.equal((await control(ctl, "GET", "/other")).status, 404);
  assert.equal((await control(ctl, "GET", "/run/log")).status, 404);
  assert.equal((await control(ctl, "POST", "/run/end", { run: 1 })).status, 404);
  assert.equal((await control(ctl, "POST", "/run/start", ok)).status, 200);
  assert.equal((await control(ctl, "POST", "/run/start", ok)).status, 409);
  assert.equal((await control(ctl, "POST", "/run/start", { ...ok, run: "ctl-2", k: "k1" })).status, 200);
  await control(ctl, "POST", "/run/end", { run: "ctl-1" });
  await control(ctl, "POST", "/run/end", { run: "ctl-2" });
});

test("control_limits_open_runs", async () => {
  const ctl = host.ports.control;
  const ids = [];
  let last = 0;
  for (let i = 0; i < 20 && last !== 429; i += 1) {
    last = (await control(ctl, "POST", "/run/start", { run: `many-${i}`, capability: "C", platform: "p", os: "1", k: i })).status;
    if (last === 200) ids.push(`many-${i}`);
  }
  assert.equal(last, 429);
  assert.equal(ids.length, 16);
  for (const run of ids) await control(ctl, "POST", "/run/end", { run });
});

test("binds_loopback", async () => {
  for (const p of Object.values(host.ports)) {
    // A connection through the loopback address works; the servers report that address.
    assert.ok(p > 0);
  }
  const here = dirname(fileURLToPath(import.meta.url));
  const child = spawn(process.execPath, [join(here, "..", "main.mjs"), "--port", "0", "--bad-port", "0", "--control-port", "0", "--site-port", "1", "--tls-dir", tls.dir], { stdio: ["ignore", "pipe", "pipe"] });
  try {
    const line = await new Promise((resolve, reject) => {
      let out = "";
      child.stdout.on("data", (d) => { out += d; if (out.includes("\n")) resolve(out.trim()); });
      child.on("exit", (c) => reject(new Error(`exited ${c}`)));
    });
    const m = /^listening (\d+) (\d+) (\d+)$/.exec(line);
    assert.ok(m, line);
    const ports = m.slice(1).map(Number);
    const reach = (address, port) => new Promise((resolve) => {
      const sock = net.connect({ host: address, port });
      sock.on("connect", () => { sock.destroy(); resolve(true); });
      sock.on("error", () => resolve(false));
    });
    for (const p of ports) assert.equal(await reach("127.0.0.1", p), true);
    const external = Object.values(os.networkInterfaces()).flat().find((i) => i && i.family === "IPv4" && !i.internal);
    if (external) for (const p of ports) assert.equal(await reach(external.address, p), false, `${external.address}:${p}`);
  } finally {
    child.kill("SIGTERM");
  }
});

test("log_never_holds_a_cookie_value_outside_the_synthetic_set", async () => {
  await startRun(host, "leak-1");
  const r = await g("/page/leak?run=leak-1");
  assert.equal(r.headers["set-cookie"][0], "sid=Real_Secret; Path=/", "the app still gets the real field");
  const text = (await control(host.ports.control, "POST", "/run/end", { run: "leak-1" })).text;
  assert.ok(!text.includes("Real_Secret"));
  assert.ok(text.includes("[redacted]"));
});

test("listening_line_uses_the_contract_ports", () => {
  const c = parseConfig([], { LAB_TLS: "/x" });
  assert.deepEqual(c, { tlsDir: "/x", ports: { good: 18443, bad: 18444, control: 18445 }, sitePort: 18480 });
  const o = parseConfig(["--port", "1", "--tls-dir", "/y"], { LAB_TLS: "/x", LAB_HOST_BAD_PORT: "2", LAB_SITE_PORT: "3" });
  assert.deepEqual(o, { tlsDir: "/y", ports: { good: 1, bad: 2, control: 18445 }, sitePort: 3 });
  assert.throws(() => parseConfig([], {}), /LAB_TLS/);
  assert.throws(() => parseConfig(["--port", "70000"], { LAB_TLS: "/x" }), /port/);
  assert.throws(() => parseConfig(["--port", "x"], { LAB_TLS: "/x" }), /port/);
  assert.throws(() => parseConfig(["--bogus"], { LAB_TLS: "/x" }));
});
