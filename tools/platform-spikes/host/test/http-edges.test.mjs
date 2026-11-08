// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// Edge behavior of the spike host over HTTP: error answers, routing, query rules, parameter bounds,
// the control port body rules and the page pass-through.

import assert from "node:assert/strict";
import http from "node:http";
import https from "node:https";
import { after, before, test } from "node:test";
import { parseRange } from "../scenarios.mjs";
import { RunLogs } from "../runlog.mjs";
import { control, events, get, open, startRun, startTestHost, tempTls, wait } from "./helpers.mjs";

let tls;
let host;
let upstream;
let port;
let ctl;
const upstreamSeen = [];
let upstreamClosed;
before(async () => {
  tls = tempTls();
  upstream = http.createServer((req, res) => {
    upstreamSeen.push({ method: req.method, url: req.url, raw: req.rawHeaders });
    if (req.url === "/hang") {
      res.on("close", () => upstreamClosed?.());
      return;
    }
    if (req.url === "/dies") {
      res.writeHead(200, { "Content-Length": "100" });
      return res.write("12345", () => setImmediate(() => req.socket.destroy()));
    }
    res.writeHead(200, { "Content-Length": "2" });
    res.end("ok");
  });
  await new Promise((r) => upstream.listen(0, "127.0.0.1", r));
  host = await startTestHost(tls, { sitePort: upstream.address().port });
  port = host.ports.good;
  ctl = host.ports.control;
});
after(async () => {
  await host.close();
  await new Promise((r) => {
    upstream.close(r);
    upstream.closeAllConnections();
  });
  tls.cleanup();
});
const g = (path, opts = {}) => get(port, path, { ca: tls.ca, ...opts });
const o = (path, opts = {}) => open(port, path, { ca: tls.ca, ...opts });
const bodyOf = async (path, opts) => (await g(path, opts)).body.toString();

/** A request with any method on the good port; gives the status and the text. */
function send(method, path) {
  return new Promise((resolve, reject) => {
    const req = https.request({ host: "127.0.0.1", port, path, method, ca: tls.ca, servername: "localhost", agent: false,
      headers: { Host: `localhost:${port}` } }, (res) => {
      const parts = [];
      res.on("data", (d) => parts.push(d));
      res.on("end", () => resolve({ status: res.statusCode, text: Buffer.concat(parts).toString() }));
    });
    req.on("error", reject);
    req.end();
  });
}

/** Capture what the host writes to stderr while `fn` runs. */
async function captureStderr(fn) {
  const real = process.stderr.write;
  const written = [];
  process.stderr.write = (chunk) => {
    written.push(String(chunk));
    return true;
  };
  try {
    await fn();
  } finally {
    process.stderr.write = real;
  }
  return written.join("");
}

/** Make the next `RunLogs.emit` of event `ev` throw `thrown`, once. */
function throwOnce(t, ev, thrown) {
  const real = RunLogs.prototype.emit;
  let armed = true;
  RunLogs.prototype.emit = function emit(run, name, members) {
    if (armed && name === ev) {
      armed = false;
      throw thrown;
    }
    return real.call(this, run, name, members);
  };
  t.after(() => {
    RunLogs.prototype.emit = real;
  });
}

// error answers

test("an unexpected error gives a plain 500 that holds no request data, and the run log shows it", async (t) => {
  await startRun(host, "boom-1");
  throwOnce(t, "host.request", new TypeError("secret detail"));
  let r;
  const stderr = await captureStderr(async () => {
    r = await g("/s/echo?run=boom-1");
  });
  assert.equal(r.status, 500);
  assert.equal(r.body.toString(), "INTERNAL: internal error\n");
  assert.equal(r.headers["content-type"], "text/plain; charset=utf-8");
  assert.equal(r.headers["cache-control"], "no-store");
  assert.equal(r.headers["content-length"], String(r.body.length));
  assert.match(stderr, /spike host: secret detail/);
  const resp = (await events(host, "boom-1")).filter((e) => e.ev === "host.response");
  assert.deepEqual(resp.map((e) => [e.status, e.fields]), [[500, { "content-type": "text/plain; charset=utf-8" }]]);
});

test("a thrown value that is not an Error still gives a 500", async (t) => {
  throwOnce(t, "host.request", "just text");
  let r;
  const stderr = await captureStderr(async () => {
    r = await g("/s/echo?run=boom-2");
  });
  assert.equal(r.status, 500);
  assert.match(stderr, /spike host: non-error thrown/);
});

test("a known error is answered without a line on stderr", async () => {
  let r;
  const stderr = await captureStderr(async () => {
    r = await g("/s/nothing");
  });
  assert.equal(r.status, 404);
  assert.equal(stderr, "");
});

test("an unexpected error after the head went out closes the connection and adds no second answer", async (t) => {
  await startRun(host, "boom-3");
  throwOnce(t, "host.chunk", new Error("late failure"));
  let outcome;
  await captureStderr(async () => {
    outcome = await g("/s/stream?run=boom-3&chunks=3&size=300&delay=0").then((r) => r, (e) => ({ error: e }));
  });
  assert.ok(outcome.error !== undefined || outcome.aborted === true, "the client sees a broken response");
  const statuses = (await events(host, "boom-3")).filter((e) => e.ev === "host.response").map((e) => e.status);
  assert.deepEqual(statuses, [200]);
});

// routing and query

test("a request target that is not in origin form is refused", async () => {
  const r = await g(`http://localhost:${port}/s/echo`);
  assert.equal(r.status, 400);
  assert.match(r.body.toString(), /^BAD_TARGET: /);
});

test("an unknown route is 404, a method other than GET is 405, an unknown Host is 400", async () => {
  assert.match((await g("/s/nothing")).body.toString(), /^NO_ROUTE: /);
  const post = await send("POST", "/s/echo");
  assert.equal(post.status, 405);
  assert.match(post.text, /^METHOD: /);
  const bad = await g("/s/echo", { host: "evil.example" });
  assert.equal(bad.status, 400);
  assert.match(bad.body.toString(), /^BAD_HOST: /);
});

test("the Service Worker routes answer 501 with their own code", async () => {
  const r = await g("/s/sw.js");
  assert.equal(r.status, 501);
  assert.match(r.body.toString(), /^NOT_IMPLEMENTED: /);
});

test("percent-encoded non-ASCII in the path is a missing route, not a bad query", async () => {
  assert.equal((await g("/s/%C3%A9")).status, 404);
});

test("an empty query is accepted and a second question mark belongs to the value", async () => {
  assert.equal((await g("/s/echo?")).status, 200);
  const r = JSON.parse(await bodyOf("/s/echo?a=b?c"));
  assert.equal(r.target, "/s/echo?a=b?c");
});

test("a non-ASCII query is refused with its own code", async () => {
  const r = await g("/s/echo?x=%C3%A9");
  assert.equal(r.status, 400);
  assert.match(r.body.toString(), /^BAD_QUERY: /);
});

test("the request event lists the field names once each, sorted", async () => {
  await startRun(host, "fields-1");
  await g("/s/echo?run=fields-1", { headers: { Zzz: "1", Aaa: "2" } });
  const [req] = await events(host, "fields-1");
  assert.deepEqual(req.fields, [...req.fields].sort());
  assert.ok(req.fields.indexOf("aaa") < req.fields.indexOf("host") && req.fields.indexOf("host") < req.fields.indexOf("zzz"));
  assert.equal(new Set(req.fields).size, req.fields.length);
});

test("a client that goes away stops the work of its request", async () => {
  await startRun(host, "gone-1");
  const s = o("/s/stream?run=gone-1&chunks=2&size=300&gate=beacon&timeout=150");
  await s.until(/data-k="0"/);
  s.abort();
  await s.done.catch(() => {});
  await wait(350); // longer than the gate timeout
  const ev = await events(host, "gone-1");
  assert.equal(ev.some((e) => e.ev === "host.gate_timeout"), false);
  assert.deepEqual(ev.filter((e) => e.ev === "host.chunk").map((e) => e.k), [0]);
});

// stream parameters

test("stream accepts the explicit default values of gate and csp", async () => {
  const r = await g("/s/stream?chunks=1&size=300&gate=delay&csp=0");
  assert.equal(r.status, 200);
  assert.equal(r.headers["content-security-policy"], undefined);
});

test("stream without a run uses the nonce norun", async () => {
  const r = await g("/s/stream?chunks=1&size=300&csp=1");
  assert.equal(r.headers["content-security-policy"], "script-src 'nonce-norun'");
  assert.ok(r.body.toString().includes('<script nonce="norun">'));
});

test("stream accepts chunks times size at the total limit and refuses one byte more", async () => {
  const s = o("/s/stream?chunks=1024&size=65536&delay=0");
  await s.until(/data-k="0"/);
  s.abort();
  await s.done.catch(() => {});
  assert.equal((await g("/s/stream?chunks=1024&size=65537")).status, 400);
});

test("stream ends at once after the last chunk, whatever the delay", { timeout: 15_000 }, async () => {
  const r = await g("/s/stream?chunks=1&size=200&delay=60000");
  assert.equal(r.status, 200);
  assert.ok(r.body.length >= 200);
});

test("stream sends exactly the requested number of chunks", async () => {
  await startRun(host, "count-1");
  await g("/s/stream?run=count-1&chunks=3&size=300");
  const ev = await events(host, "count-1");
  assert.deepEqual(ev.filter((e) => e.ev === "host.chunk").map((e) => e.k), [0, 1, 2]);
});

test("stream refuses delay, timeout and size above their limits and accepts timeout 0", async () => {
  for (const q of ["delay=60001", "timeout=60001", "first=-1", "size=x"]) assert.equal((await g(`/s/stream?${q}`)).status, 400, q);
  await startRun(host, "to-0");
  assert.equal((await g("/s/stream?run=to-0&chunks=2&size=300&gate=beacon&timeout=0")).status, 200);
  const ev = await events(host, "to-0");
  assert.deepEqual(ev.filter((e) => e.ev === "host.gate_timeout").map((e) => e.k), [0]);
});

test("beacon takes k from 0 to 1023 and needs it", async () => {
  assert.equal((await g("/s/beacon?k=0")).status, 204);
  assert.equal((await g("/s/beacon?k=1023")).status, 204);
  for (const q of ["k=1024", "k=1025", "k=-1", "k=x", ""]) assert.equal((await g(`/s/beacon?${q}`)).status, 400, q);
});

// scenario routes

test("redirect accepts cross and alt values 0 and the default, and answers with a text body", async () => {
  const r = await g("/s/redirect?cross=0&alt=127.0.0.1&n=1");
  assert.equal(r.status, 302);
  assert.equal(r.headers.location, "/s/echo");
  assert.equal(r.headers["content-type"], "text/plain; charset=utf-8");
  assert.equal(r.body.toString(), "redirect\n");
});

test("redirect across sites keeps a Host field without a port as it is", async () => {
  const r = await g("/s/redirect?n=1&cross=1", { host: "localhost" });
  assert.equal(r.headers.location, "https://127.0.0.1/s/echo");
});

test("cookie set answers with a page that probes location, origin and cookie", async () => {
  const r = await g("/s/cookie/set?name=a&value=b");
  const body = r.body.toString();
  assert.ok(body.startsWith('<!doctype html><meta charset="utf-8"><title>cookie</title><p>cookie set</p><script>'));
  assert.ok(body.includes('rqLab.probe("location",document.location.href);rqLab.probe("origin",window.origin);rqLab.probe("cookie",document.cookie)'));
  assert.equal(r.headers["content-type"], "text/html; charset=utf-8");
});

test("cookie set refuses an attribute with text before or after it", async () => {
  for (const attrs of ["xSecure", "Secure1", "xHttpOnly", "HttpOnlyx", "Max-Age=5x", "xMax-Age=5", "SameSite=Laxx", "ASameSite=Lax"]) {
    assert.equal((await g(`/s/cookie/set?name=a&value=b&attrs=${encodeURIComponent(attrs)}`)).status, 400, attrs);
  }
});

test("echo in HTML form escapes ampersand, less-than and greater-than", async () => {
  const text = await bodyOf("/s/echo?html=1", { headers: { Cookie: "a=<b>&c" } });
  assert.ok(text.includes("a=&lt;b&gt;&amp;c"), text);
  assert.ok(!text.includes("<b>"));
});

test("range answers carry a binary content type, and a refused range has an empty body", async () => {
  const full = await g("/s/range?size=10");
  assert.equal(full.headers["content-type"], "application/octet-stream");
  const bad = await g("/s/range?size=10", { headers: { Range: "bytes=20-" } });
  assert.equal(bad.status, 416);
  assert.equal(bad.headers["content-length"], "0");
  assert.equal(bad.body.length, 0);
  assert.equal(bad.headers["content-type"], "application/octet-stream");
});

test("range header text must match the whole pattern", () => {
  for (const h of ["xbytes=1-2", "bytes=1-2x", "xbytes=-5", "bytes=-5x", "bytes=1-2-3", "bytes= 1-2"]) assert.equal(parseRange(h, 100), "bad", h);
  assert.deepEqual(parseRange("bytes=5-5", 100), { from: 5, to: 5 });
  assert.equal(parseRange("bytes=5-4", 100), "bad");
});

test("cut sends the document start before it cuts, and size must be above the start", async () => {
  const CUT_HEAD = '<!doctype html><meta charset="utf-8"><title>cut</title><p>';
  const r = await g("/s/cut?after=100&size=1000");
  assert.ok(r.body.toString().startsWith(CUT_HEAD));
  assert.equal((await g(`/s/cut?after=0&size=${CUT_HEAD.length}`)).status, 400);
  const edge = await g(`/s/cut?after=0&size=${CUT_HEAD.length + 1}`);
  assert.equal(edge.status, 200);
  assert.equal(edge.headers["content-length"], String(CUT_HEAD.length + 1));
});

// control port

const START = { run: "cb-1", capability: "CAP-T-01", platform: "test", os: "1", k: 1 };
const padded = (length) => {
  const text = JSON.stringify(START);
  return text + " ".repeat(length - text.length);
};

test("control takes a body of 4096 bytes and refuses 4097", async () => {
  const ok = await control(ctl, "POST", "/run/start", padded(4096));
  assert.equal(ok.status, 200);
  await control(ctl, "POST", "/run/end", { run: "cb-1" });
  const big = await control(ctl, "POST", "/run/start", padded(4097));
  assert.equal(big.status, 413);
  assert.match(big.text, /^BODY_TOO_LARGE: /);
});

test("control answers a body that is not JSON with BAD_JSON on every route that reads a body", async () => {
  for (const path of ["/run/start", "/run/end"]) {
    const r = await control(ctl, "POST", path, "{not json");
    assert.equal(r.status, 400, path);
    assert.match(r.text, /^BAD_JSON: /);
  }
});

test("control answers a wrong content type with BAD_CONTENT_TYPE", async () => {
  const r = await control(ctl, "POST", "/run/start", START, { type: "text/plain" });
  assert.equal(r.status, 415);
  assert.match(r.text, /^BAD_CONTENT_TYPE: /);
});

test("run end gives 404 when the body is null, an array, a string or has no run", async () => {
  for (const body of ["null", "[]", '"cb-9"', "{}", "5"]) {
    const r = await control(ctl, "POST", "/run/end", body);
    assert.equal(r.status, 404, body);
    assert.match(r.text, /^NO_RUN: /);
  }
});

test("control answers carry their content type and no-store", async () => {
  const start = await control(ctl, "POST", "/run/start", { ...START, run: "cb-ct" });
  assert.equal(start.headers["content-type"], "application/json");
  assert.equal(start.headers["cache-control"], "no-store");
  const log = await control(ctl, "GET", "/run/log?run=cb-ct");
  assert.equal(log.headers["content-type"], "application/x-ndjson");
  assert.equal(log.headers["cache-control"], "no-store");
  await control(ctl, "POST", "/run/end", { run: "cb-ct" });
  const missing = await control(ctl, "GET", "/run/log?run=cb-ct");
  assert.equal(missing.headers["content-type"], "text/plain; charset=utf-8");
  assert.match(missing.text, /^NO_RUN: /);
});

test("control routes: a path under /run/ with another method is 405, another path is 404, both with NO_ROUTE", async () => {
  const a = await control(ctl, "GET", "/run/start");
  assert.equal(a.status, 405);
  assert.match(a.text, /^NO_ROUTE: /);
  const b = await control(ctl, "GET", "/other");
  assert.equal(b.status, 404);
  assert.match(b.text, /^NO_ROUTE: /);
});

test("control refuses a request head above 16 KiB", async () => {
  const status = await new Promise((resolve) => {
    const req = http.request({ host: "127.0.0.1", port: ctl, method: "GET", path: "/run/log", agent: false,
      headers: { Host: `127.0.0.1:${ctl}`, "X-Big": "a".repeat(17_000) } }, (res) => {
      res.resume();
      resolve(res.statusCode);
    });
    req.on("error", () => resolve("closed"));
    req.end();
  });
  assert.ok(status === 431 || status === "closed", String(status));
});

// page pass-through

test("page accepts a site path of 512 characters and refuses 513", async () => {
  assert.equal((await g(`/page/${"a".repeat(511)}`)).status, 200);
  assert.equal((await g(`/page/${"a".repeat(512)}`)).status, 404);
  assert.match((await g(`/page/${"a".repeat(512)}`)).body.toString(), /^BAD_PAGE_PATH: /);
});

test("page sends a field that the client repeats as separate lines", async () => {
  await g("/page/multi", { headers: { "X-Multi": ["a", "b", "c"] } });
  const raw = upstreamSeen.findLast((s) => s.url === "/multi").raw;
  const values = raw.filter((v, i) => i % 2 === 1 && raw[i - 1].toLowerCase() === "x-multi");
  assert.deepEqual(values, ["a", "b", "c"]);
});

test("page asks the reference server with GET", async () => {
  await g("/page/method-check");
  assert.equal(upstreamSeen.findLast((s) => s.url === "/method-check").method, "GET");
});

test("page stops the request to the reference server when the client goes away", { timeout: 15_000 }, async () => {
  const closed = new Promise((resolve) => {
    upstreamClosed = resolve;
  });
  const s = o("/page/hang");
  while (!upstreamSeen.some((x) => x.url === "/hang")) await wait(10);
  s.abort();
  await s.done.catch(() => {});
  await closed;
});

test("page breaks the connection when the reference server dies in the middle of a body", async () => {
  const outcome = await g("/page/dies").then((r) => r, (e) => ({ error: e }));
  assert.ok(outcome.error !== undefined || outcome.aborted === true, "the client sees a broken response");
});
